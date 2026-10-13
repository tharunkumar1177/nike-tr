//! An open project with its command history.
//!
//! Every edit is an invertible [`Command`]. Undo applies the inverse of the
//! latest command and redo applies it again, so both restore exact states.

use std::path::Path;

use uuid::Uuid;

use crate::error::CoreError;
use crate::kicad::{Mirror, PlacedSymbol, TitleBlock};
use crate::kicad_value::text_arg;
use crate::manifest::{MAX_GRID, MIN_GRID};
use crate::project::Project;
use crate::sexpr::SExpr;
use crate::units::{Nm, Point};

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
    /// Symbols were placed; each definition was embedded on first use. The indices are the
    /// child positions the nodes are inserted at, in ascending order.
    PlaceSymbols {
        symbols: Vec<(usize, SExpr)>,
        /// `(index, node)` within `(lib_symbols)`.
        definitions: Vec<(usize, SExpr)>,
        /// The placement created the `(lib_symbols)` node itself; undo removes it again.
        created_lib_symbols: bool,
    },
    /// Symbols were removed, with definitions that became unused, all at their original
    /// child positions so undo restores them exactly.
    DeleteSymbols {
        symbols: Vec<(usize, SExpr)>,
        /// `(index, node)` within `(lib_symbols)`.
        definitions: Vec<(usize, SExpr)>,
        /// The removed `(lib_symbols)` node had been created by the placement being undone.
        created_lib_symbols: bool,
    },
    /// A set of symbols moved by the same offset.
    MoveSymbols {
        uuids: Vec<Uuid>,
        dx: Nm,
        dy: Nm,
    },
    /// A set of symbols rotated or mirrored about their own origins.
    TransformSymbols {
        /// `(uuid, before, after)` per transformed symbol.
        symbols: Vec<(Uuid, PlacedSymbol, PlacedSymbol)>,
    },
    /// Commands applied in order as one undo step.
    Group {
        label: String,
        commands: Vec<Command>,
    },
}

/// The transform applied by [`Session::transform_symbols`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolTransform {
    /// Quarter turns counterclockwise on screen (the KiCad angle convention).
    Rotate(i32),
    /// Mirror about the symbol origin: `Mirror::X` flips horizontally, `Mirror::Y`
    /// vertically.
    Mirror(Mirror),
}

impl Command {
    pub fn label(&self) -> &str {
        match self {
            Command::SetTitleBlock { .. } => "Title Block",
            Command::SetGridSize { .. } => "Grid Size",
            Command::PlaceSymbols { .. } => "Place Symbol",
            Command::DeleteSymbols { .. } => "Delete Symbol",
            Command::MoveSymbols { .. } => "Move Symbol",
            Command::TransformSymbols { .. } => "Transform Symbol",
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
            Command::PlaceSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => Command::DeleteSymbols {
                symbols: symbols.clone(),
                definitions: definitions.clone(),
                created_lib_symbols: *created_lib_symbols,
            },
            Command::DeleteSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => Command::PlaceSymbols {
                symbols: symbols.clone(),
                definitions: definitions.clone(),
                created_lib_symbols: *created_lib_symbols,
            },
            Command::MoveSymbols { uuids, dx, dy } => Command::MoveSymbols {
                uuids: uuids.clone(),
                dx: -*dx,
                dy: -*dy,
            },
            Command::TransformSymbols { symbols } => Command::TransformSymbols {
                symbols: symbols
                    .iter()
                    .map(|(uuid, before, after)| (*uuid, after.clone(), before.clone()))
                    .collect(),
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
            Command::PlaceSymbols {
                symbols,
                definitions,
                ..
            } => {
                // Symbols first: their indices were recorded before a missing
                // `(lib_symbols)` node was created, which shifts the root children.
                for (index, node) in symbols {
                    project
                        .schematic_mut()
                        .insert_placed_at(*index, node.clone());
                }
                for (index, node) in definitions {
                    project
                        .schematic_mut()
                        .insert_definition_at(*index, node.clone());
                }
                changes.symbols = !symbols.is_empty();
                changes.definitions = !definitions.is_empty();
            }
            Command::DeleteSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => {
                for (_, node) in symbols {
                    if let Some(symbol) = PlacedSymbol::read(node) {
                        project.schematic_mut().remove_placed(symbol.uuid);
                    }
                }
                for (_, node) in definitions {
                    if let Some(name) = text_arg(node, 0) {
                        project.schematic_mut().remove_definition(name);
                    }
                }
                if *created_lib_symbols {
                    project.schematic_mut().remove_empty_lib_symbols();
                }
                changes.symbols = !symbols.is_empty();
                changes.definitions = !definitions.is_empty();
            }
            Command::MoveSymbols { uuids, dx, dy } => {
                for uuid in uuids {
                    if let Some(symbol) = project.schematic().symbol(*uuid) {
                        let moved = symbol.moved_by(*dx, *dy);
                        project.schematic_mut().replace_placed(&moved);
                    }
                }
                changes.symbols = !uuids.is_empty();
            }
            Command::TransformSymbols { symbols } => {
                for (_, _, after) in symbols {
                    project.schematic_mut().replace_placed(after);
                }
                changes.symbols = !symbols.is_empty();
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
    /// Placed symbols were added, moved, transformed, or removed.
    pub symbols: bool,
    /// The embedded `(lib_symbols)` definitions changed.
    pub definitions: bool,
}

impl Changes {
    pub fn any(self) -> bool {
        self.title_block || self.grid_size || self.symbols || self.definitions
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

    /// Places a library symbol on the schematic as one undo step. `definition` is the
    /// flattened `(symbol ...)` the library index returns for `lib_id`; it is embedded in
    /// `(lib_symbols)` on first use and removed again when the last instance using it is
    /// deleted. `rotation_steps` counts 90° steps counterclockwise on screen. The symbol
    /// gets `uuid` and one pin UUID per definition pin, and its reference is left
    /// unannotated (`R?`).
    pub fn place_symbol(
        &mut self,
        definition: &SExpr,
        lib_id: &str,
        position: Point,
        rotation_steps: i32,
        mirror: Option<Mirror>,
        uuid: Uuid,
        pin_uuids: &[Uuid],
    ) -> Result<Changes, CoreError> {
        if lib_id.is_empty() {
            return Err(CoreError::InvalidEdit("The library ID is empty.".to_owned()));
        }
        if self.project.schematic().symbol(uuid).is_some() {
            return Err(CoreError::InvalidEdit(
                "A symbol with this UUID is already placed.".to_owned(),
            ));
        }
        let project_name = self.project.name().to_owned();
        let (_, node) = self
            .project
            .schematic()
            .build_placement(
                definition,
                lib_id,
                position,
                rotation_steps,
                mirror,
                uuid,
                pin_uuids,
                &project_name,
            )
            .map_err(CoreError::InvalidEdit)?;
        let index = self.project.schematic().placed_insert_index();
        let created_lib_symbols = !self.project.schematic().has_lib_symbols();
        let definitions = if self.project.schematic().lib_symbol(lib_id).is_some() {
            Vec::new()
        } else {
            let mut renamed = definition.clone();
            if let Some(items) = renamed.items_mut() {
                items[1] = SExpr::string(lib_id);
            }
            let index = self.project.schematic().definition_insert_index(lib_id);
            vec![(index, renamed)]
        };
        Ok(self.execute(Command::PlaceSymbols {
            symbols: vec![(index, node)],
            definitions,
            created_lib_symbols: created_lib_symbols && !definitions.is_empty(),
        }))
    }

    /// Moves every symbol in `uuids` by `(dx, dy)` as one undo step.
    pub fn move_symbols(
        &mut self,
        uuids: &[Uuid],
        dx: Nm,
        dy: Nm,
    ) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() || (dx == Nm::ZERO && dy == Nm::ZERO) {
            return Ok(Changes::default());
        }
        self.check_symbols(&uuids)?;
        Ok(self.execute(Command::MoveSymbols {
            uuids,
            dx,
            dy,
        }))
    }

    /// Rotates or mirrors every symbol in `uuids` about its own origin as one undo step.
    pub fn transform_symbols(
        &mut self,
        uuids: &[Uuid],
        transform: SymbolTransform,
    ) -> Result<Changes, CoreError> {
        let no_change = matches!(
            transform,
            SymbolTransform::Rotate(steps) if steps.rem_euclid(4) == 0
        );
        let uuids = dedupe(uuids);
        if uuids.is_empty() || no_change {
            return Ok(Changes::default());
        }
        let mut symbols = Vec::with_capacity(uuids.len());
        for uuid in &uuids {
            let before = self
                .project
                .schematic()
                .symbol(*uuid)
                .ok_or_else(missing_symbol)?;
            let after = match transform {
                SymbolTransform::Rotate(steps) => before.rotated(steps),
                SymbolTransform::Mirror(axis) => before.mirrored(axis),
            };
            symbols.push((*uuid, before, after));
        }
        Ok(self.execute(Command::TransformSymbols { symbols }))
    }

    /// Deletes every symbol in `uuids` as one undo step. Definitions no longer used by any
    /// remaining symbol leave `(lib_symbols)` within the same step.
    pub fn delete_symbols(&mut self, uuids: &[Uuid]) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() {
            return Ok(Changes::default());
        }
        let schematic = self.project.schematic();
        let mut symbols = Vec::with_capacity(uuids.len());
        let mut deleted_lib_ids: Vec<String> = Vec::new();
        for uuid in &uuids {
            let Some((index, node)) = schematic.placed_entry(*uuid) else {
                return Err(missing_symbol());
            };
            if let Some(symbol) = PlacedSymbol::read(&node)
                && !deleted_lib_ids.contains(&symbol.lib_id)
            {
                deleted_lib_ids.push(symbol.lib_id);
            }
            symbols.push((index, node));
        }
        symbols.sort_by_key(|(index, _)| *index);

        let remaining: Vec<String> = schematic
            .symbols()
            .into_iter()
            .filter(|symbol| !uuids.contains(&symbol.uuid))
            .map(|symbol| symbol.lib_id)
            .collect();
        let mut definitions = Vec::new();
        for lib_id in &deleted_lib_ids {
            if !remaining.contains(lib_id)
                && let Some(entry) = schematic.definition_entry(lib_id)
            {
                definitions.push(entry);
            }
        }
        definitions.sort_by_key(|(index, _)| *index);
        Ok(self.execute(Command::DeleteSymbols {
            symbols,
            definitions,
            created_lib_symbols: false,
        }))
    }

    /// Errors when any of `uuids` is not a symbol placed on the schematic.
    fn check_symbols(&self, uuids: &[Uuid]) -> Result<(), CoreError> {
        for uuid in uuids {
            if self.project.schematic().symbol(*uuid).is_none() {
                return Err(missing_symbol());
            }
        }
        Ok(())
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

/// The selection with duplicates removed, order kept.
fn dedupe(uuids: &[Uuid]) -> Vec<Uuid> {
    let mut unique = Vec::with_capacity(uuids.len());
    for uuid in uuids {
        if !unique.contains(uuid) {
            unique.push(*uuid);
        }
    }
    unique
}

fn missing_symbol() -> CoreError {
    CoreError::InvalidEdit(
        "A symbol in the selection is not placed on the schematic.".to_owned(),
    )
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
    use crate::units::Angle;
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
                symbols: false,
                definitions: false,
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

    const SYMBOL_1: Uuid = Uuid::from_u128(1);
    const SYMBOL_2: Uuid = Uuid::from_u128(2);
    const PIN_1: Uuid = Uuid::from_u128(3);
    const PIN_2: Uuid = Uuid::from_u128(4);
    const PIN_3: Uuid = Uuid::from_u128(5);
    const PINS: [Uuid; 2] = [PIN_1, PIN_2];

    fn r_definition() -> SExpr {
        crate::sexpr::parse(
            "(symbol \"Device:R\" \
            (property \"Reference\" \"R\" (at 2.032 0 90)) \
            (property \"Value\" \"R\" (at 0 0 90)) \
            (property \"Footprint\" \"\" (at -1.778 0 90) (hide yes)) \
            (property \"Datasheet\" \"\" (at 0 0 0) (hide yes)) \
            (symbol \"R_0_1\" (rectangle (start -1.016 -2.54) (end 1.016 2.54))) \
            (symbol \"R_1_1\" \
            (pin passive line (at 0 3.81 270) (length 1.27) (name \"\") (number \"1\")) \
            (pin passive line (at 0 -3.81 90) (length 1.27) (name \"\") (number \"2\"))))",
        )
        .unwrap()
    }

    fn led_definition() -> SExpr {
        crate::sexpr::parse(
            "(symbol \"Device:LED\" \
            (property \"Reference\" \"D\" (at 0 2.54 0)) \
            (property \"Value\" \"LED\" (at 0 -2.54 0)) \
            (property \"Footprint\" \"\" (at 0 0 0) (hide yes)) \
            (property \"Datasheet\" \"\" (at 0 0 0) (hide yes)) \
            (symbol \"LED_1_1\" \
            (pin passive line (at -3.81 0 0) (length 2.54) (name \"K\") (number \"1\"))))",
        )
        .unwrap()
    }

    fn place(session: &mut Session, uuid: Uuid) -> Changes {
        session
            .place_symbol(
                &r_definition(),
                "Device:R",
                Point::new(Nm(100_000_000), Nm(50_000_000)),
                0,
                None,
                uuid,
                &PINS,
            )
            .unwrap()
    }

    #[test]
    fn placing_adds_the_symbol_and_embeds_the_definition() {
        let mut session = session();
        let changes = place(&mut session, SYMBOL_1);
        assert_eq!(
            changes,
            Changes {
                title_block: false,
                grid_size: false,
                symbols: true,
                definitions: true,
            }
        );
        let schematic = session.project().schematic();
        assert_eq!(schematic.lib_symbol_ids(), ["Device:R"]);
        let symbols = schematic.symbols();
        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.reference(), "R?");
        assert_eq!((symbol.unit, symbol.pins.len()), (1, 2));
        assert_eq!(symbol.uuid, SYMBOL_1);
        assert_eq!(symbol.pins[0].uuid, PIN_1);

        let text = schematic.to_file_string();
        assert!(text.contains("(lib_id \"Device:R\")"), "{text}");
        assert!(text.contains("(symbol \"Device:R\""), "{text}");
        assert!(text.contains("(reference \"R?\")"), "{text}");
        assert!(text.contains("(project \"demo\""), "{text}");
        assert!(text.find("(lib_id").unwrap() < text.find("(sheet_instances").unwrap());
    }

    #[test]
    fn place_undoes_and_redoes_exactly() {
        let mut session = session();
        let original = session.project().clone();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();
        session.undo().unwrap();
        assert_eq!(session.project(), &original);
        session.redo().unwrap();
        assert_eq!(session.project(), &placed);
    }

    #[test]
    fn definitions_are_shared_and_removed_with_the_last_instance() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let changes = place(&mut session, SYMBOL_2);
        assert!(!changes.definitions, "the second placement reuses it");
        assert_eq!(session.project().schematic().lib_symbol_ids().len(), 1);
        assert_eq!(session.project().schematic().symbols().len(), 2);

        let changes = session.delete_symbols(&[SYMBOL_2]).unwrap();
        assert!(changes.symbols && !changes.definitions);
        assert_eq!(session.project().schematic().lib_symbol_ids(), ["Device:R"]);

        let changes = session.delete_symbols(&[SYMBOL_1]).unwrap();
        assert!(changes.symbols && changes.definitions);
        assert!(session.project().schematic().lib_symbol_ids().is_empty());
        assert!(session.project().schematic().symbols().is_empty());
    }

    #[test]
    fn delete_undoes_and_redoes_exactly() {
        let mut session = session();
        let original = session.project().clone();
        place(&mut session, SYMBOL_1);
        place(&mut session, SYMBOL_2);
        let placed = session.project().clone();

        session.delete_symbols(&[SYMBOL_1, SYMBOL_2]).unwrap();
        let deleted = session.project().clone();
        assert_eq!(&deleted, &original);

        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
        session.redo().unwrap();
        assert_eq!(session.project(), &deleted);
    }

    #[test]
    fn moving_shifts_symbols_and_their_properties() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();
        let changes = session
            .move_symbols(&[SYMBOL_1], Nm(10_000_000), Nm(-5_000_000))
            .unwrap();
        assert!(changes.symbols && !changes.definitions);
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(
            symbol.position,
            Point::new(Nm(110_000_000), Nm(45_000_000))
        );
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(112_032_000), Nm(45_000_000))
        );
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
        session.redo().unwrap();
        assert_eq!(session.project().schematic().symbols()[0], symbol);
    }

    #[test]
    fn rotating_and_mirroring_transform_about_the_symbol_origin() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();

        session
            .transform_symbols(&[SYMBOL_1], SymbolTransform::Rotate(1))
            .unwrap();
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(symbol.rotation, Angle::from_decidegrees(900));
        assert_eq!(symbol.position, Point::new(Nm(100_000_000), Nm(50_000_000)));
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(100_000_000), Nm(47_968_000))
        );
        assert_eq!(symbol.properties[0].rotation, Angle::from_decidegrees(1800));
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);

        session
            .transform_symbols(&[SYMBOL_1], SymbolTransform::Mirror(Mirror::X))
            .unwrap();
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(symbol.mirror, Some(Mirror::X));
        assert_eq!(symbol.rotation, Angle::ZERO);
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(97_968_000), Nm(50_000_000))
        );
        assert_eq!(symbol.properties[0].rotation, Angle::from_decidegrees(900));
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
    }

    #[test]
    fn definitions_stay_sorted_by_library_id() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        session
            .place_symbol(
                &led_definition(),
                "Device:LED",
                Point::ORIGIN,
                0,
                None,
                SYMBOL_2,
                &[PIN_3],
            )
            .unwrap();
        assert_eq!(
            session.project().schematic().lib_symbol_ids(),
            ["Device:LED", "Device:R"]
        );
    }

    #[test]
    fn no_op_symbol_commands_record_nothing() {
        let mut session = session();
        assert!(
            !session
                .move_symbols(&[SYMBOL_1], Nm::ZERO, Nm::ZERO)
                .unwrap()
                .any()
        );
        assert!(
            !session
                .transform_symbols(&[SYMBOL_1], SymbolTransform::Rotate(4))
                .unwrap()
                .any()
        );
        assert!(!session.delete_symbols(&[]).unwrap().any());
        assert!(!session.can_undo());
    }

    #[test]
    fn invalid_symbol_commands_are_rejected() {
        let mut session = session();
        let unknown = Uuid::from_u128(99);
        assert!(matches!(
            session.move_symbols(&[unknown], Nm(1_000_000), Nm::ZERO),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.transform_symbols(&[unknown], SymbolTransform::Rotate(1)),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.delete_symbols(&[unknown]),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(!session.can_undo());

        place(&mut session, SYMBOL_1);
        assert!(matches!(
            session.place_symbol(
                &r_definition(),
                "Device:R",
                Point::ORIGIN,
                0,
                None,
                SYMBOL_1,
                &PINS,
            ),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.place_symbol(
                &crate::sexpr::parse("(wire (pts))").unwrap(),
                "Device:R",
                Point::ORIGIN,
                0,
                None,
                SYMBOL_2,
                &[],
            ),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.place_symbol(
                &r_definition(),
                "Device:R",
                Point::ORIGIN,
                0,
                None,
                SYMBOL_2,
                &PINS[..1],
            ),
            Err(CoreError::InvalidEdit(_))
        ));
        assert_eq!(session.project().schematic().symbols().len(), 1);
    }
}
