//! An open project with its command history.
//!
//! Every edit is an invertible [`Command`]. Undo applies the inverse of the
//! latest command and redo applies it again, so both restore exact states.

use std::path::Path;

use crate::error::CoreError;
use crate::kicad::TitleBlock;
use crate::manifest::{MAX_GRID, MIN_GRID};
use crate::project::Project;
use crate::units::Nm;

pub const MAX_FIELD_CHARS: usize = 256;
pub const PROPERTIES_LABEL: &str = "Project Properties";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetTitleBlock {
        before: Box<TitleBlock>,
        after: Box<TitleBlock>,
    },
    SetGridSize {
        before: Nm,
        after: Nm,
    },
    /// Commands applied in order as one undo step.
    Group {
        label: String,
        commands: Vec<Command>,
    },
}

impl Command {
    pub fn label(&self) -> &str {
        match self {
            Command::SetTitleBlock { .. } => "Title Block",
            Command::SetGridSize { .. } => "Grid Size",
            Command::Group { label, .. } => label,
        }
    }

    pub fn inverse(&self) -> Command {
        match self {
            Command::SetTitleBlock { before, after } => Command::SetTitleBlock {
                before: after.clone(),
                after: before.clone(),
            },
            Command::SetGridSize { before, after } => Command::SetGridSize {
                before: *after,
                after: *before,
            },
            Command::Group { label, commands } => Command::Group {
                label: label.clone(),
                commands: commands.iter().rev().map(Command::inverse).collect(),
            },
        }
    }

    fn apply(&self, project: &mut Project, changes: &mut Changes) {
        match self {
            Command::SetTitleBlock { after, .. } => {
                project.set_title_block(after);
                changes.title_block = true;
            }
            Command::SetGridSize { after, .. } => {
                project.set_grid_size(*after);
                changes.grid_size = true;
            }
            Command::Group { commands, .. } => {
                for command in commands {
                    command.apply(project, changes);
                }
            }
        }
    }
}

/// Which parts of the document a command touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    pub title_block: bool,
    pub grid_size: bool,
}

impl Changes {
    pub fn any(self) -> bool {
        self.title_block || self.grid_size
    }
}

#[derive(Debug, Clone)]
struct Entry {
    id: u64,
    command: Command,
}

#[derive(Debug, Clone)]
pub struct Session {
    project: Project,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    next_id: u64,
    /// The id on top of the undo stack when the project was last saved.
    saved_top: Option<u64>,
}

impl Session {
    pub fn new(project: Project) -> Session {
        Session {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 1,
            saved_top: None,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|entry| entry.id) != self.saved_top
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|entry| entry.command.label())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.command.label())
    }

    /// Sets the title block and grid as one undo step. Unchanged values are
    /// left out, and an edit that changes nothing records nothing.
    pub fn edit_properties(
        &mut self,
        title_block: TitleBlock,
        grid_size: Nm,
    ) -> Result<Changes, CoreError> {
        validate_title_block(&title_block)?;
        if !(MIN_GRID..=MAX_GRID).contains(&grid_size) {
            return Err(CoreError::InvalidEdit(format!(
                "The grid size must be between {} mm and {} mm.",
                MIN_GRID.to_mm(),
                MAX_GRID.to_mm()
            )));
        }
        let mut commands = Vec::new();
        let current_block = self.project.title_block();
        if current_block != title_block {
            commands.push(Command::SetTitleBlock {
                before: Box::new(current_block),
                after: Box::new(title_block),
            });
        }
        let current_grid = self.project.grid_size();
        if current_grid != grid_size {
            commands.push(Command::SetGridSize {
                before: current_grid,
                after: grid_size,
            });
        }
        if commands.is_empty() {
            return Ok(Changes::default());
        }
        Ok(self.execute(Command::Group {
            label: PROPERTIES_LABEL.to_owned(),
            commands,
        }))
    }

    pub fn undo(&mut self) -> Result<Changes, CoreError> {
        let entry = self.undo.pop().ok_or(CoreError::NothingToUndo)?;
        let mut changes = Changes::default();
        entry
            .command
            .inverse()
            .apply(&mut self.project, &mut changes);
        self.redo.push(entry);
        Ok(changes)
    }

    pub fn redo(&mut self) -> Result<Changes, CoreError> {
        let entry = self.redo.pop().ok_or(CoreError::NothingToRedo)?;
        let mut changes = Changes::default();
        entry.command.apply(&mut self.project, &mut changes);
        self.undo.push(entry);
        Ok(changes)
    }

    pub fn save(&mut self) -> Result<(), CoreError> {
        self.project.save()?;
        self.mark_saved();
        Ok(())
    }

    /// Saves into the new folder `parent/name`, which becomes the project's
    /// location. On failure the session still points at the old location.
    pub fn save_as(&mut self, parent: &Path, name: &str) -> Result<(), CoreError> {
        self.project = self.project.save_as(parent, name)?;
        self.mark_saved();
        Ok(())
    }

    fn execute(&mut self, command: Command) -> Changes {
        let mut changes = Changes::default();
        command.apply(&mut self.project, &mut changes);
        self.undo.push(Entry {
            id: self.next_id,
            command,
        });
        self.next_id += 1;
        self.redo.clear();
        changes
    }

    fn mark_saved(&mut self) {
        self.saved_top = self.undo.last().map(|entry| entry.id);
    }
}

fn validate_title_block(block: &TitleBlock) -> Result<(), CoreError> {
    let fields = [&block.title, &block.date, &block.revision, &block.company]
        .into_iter()
        .chain(block.comments.iter());
    for field in fields {
        if field.chars().count() > MAX_FIELD_CHARS || field.contains(['\n', '\r']) {
            return Err(CoreError::InvalidEdit(format!(
                "Title block fields must be a single line of at most {MAX_FIELD_CHARS} characters."
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn session() -> Session {
        Session::new(Project::new(PathBuf::from("unsaved"), "demo", Uuid::nil()).unwrap())
    }

    fn block(title: &str) -> TitleBlock {
        TitleBlock {
            title: title.to_owned(),
            ..TitleBlock::default()
        }
    }

    #[test]
    fn undo_restores_the_exact_previous_project() {
        let mut session = session();
        let original = session.project().clone();
        let changes = session
            .edit_properties(block("Amp"), Nm(2_540_000))
            .unwrap();
        assert_eq!(
            changes,
            Changes {
                title_block: true,
                grid_size: true,
            }
        );
        let edited = session.project().clone();
        assert_ne!(edited, original);

        session.undo().unwrap();
        assert_eq!(session.project(), &original);
        session.redo().unwrap();
        assert_eq!(session.project(), &edited);
    }

    #[test]
    fn undo_and_redo_walk_through_several_edits() {
        let mut session = session();
        let grid = session.project().grid_size();
        let states: Vec<Project> = ["One", "Two", "Three"]
            .iter()
            .map(|title| {
                session.edit_properties(block(title), grid).unwrap();
                session.project().clone()
            })
            .collect();

        session.undo().unwrap();
        assert_eq!(session.project(), &states[1]);
        session.undo().unwrap();
        assert_eq!(session.project(), &states[0]);
        session.redo().unwrap();
        session.redo().unwrap();
        assert_eq!(session.project(), &states[2]);
        assert!(matches!(session.redo(), Err(CoreError::NothingToRedo)));
    }

    #[test]
    fn an_edit_that_changes_nothing_records_nothing() {
        let mut session = session();
        let grid = session.project().grid_size();
        let changes = session
            .edit_properties(TitleBlock::default(), grid)
            .unwrap();
        assert!(!changes.any());
        assert!(!session.can_undo());
        assert!(!session.is_dirty());
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut session = session();
        let grid = session.project().grid_size();
        session.edit_properties(block("One"), grid).unwrap();
        session.undo().unwrap();
        assert!(session.can_redo());
        session.edit_properties(block("Two"), grid).unwrap();
        assert!(!session.can_redo());
        assert_eq!(session.undo_label(), Some(PROPERTIES_LABEL));
    }

    #[test]
    fn dirty_state_follows_the_saved_point_through_undo_and_redo() {
        let mut session = session();
        let grid = session.project().grid_size();
        assert!(!session.is_dirty());
        session.edit_properties(block("One"), grid).unwrap();
        assert!(session.is_dirty());
        session.undo().unwrap();
        assert!(!session.is_dirty());
        session.redo().unwrap();
        assert!(session.is_dirty());

        session.mark_saved();
        assert!(!session.is_dirty());
        session.undo().unwrap();
        assert!(session.is_dirty());
        session.redo().unwrap();
        assert!(!session.is_dirty());

        session.undo().unwrap();
        session.edit_properties(block("Two"), grid).unwrap();
        session.undo().unwrap();
        assert!(session.is_dirty(), "the saved state is no longer reachable");
    }

    #[test]
    fn invalid_edits_are_rejected_and_not_recorded() {
        let mut session = session();
        let grid = session.project().grid_size();
        assert!(matches!(
            session.edit_properties(block("two\nlines"), grid),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.edit_properties(block(&"x".repeat(MAX_FIELD_CHARS + 1)), grid),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.edit_properties(TitleBlock::default(), Nm(0)),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(!session.can_undo());
        assert!(matches!(session.undo(), Err(CoreError::NothingToUndo)));
    }
}
