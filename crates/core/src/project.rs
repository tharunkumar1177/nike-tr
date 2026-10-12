//! A project on disk: a directory holding `tron.json`, `<name>.kicad_sch`,
//! and `<name>.kicad_pcb`.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::CoreError;
use crate::kicad::{Board, FileError, PageSize, Schematic, TitleBlock};
use crate::manifest::{Manifest, ManifestError};
use crate::units::Nm;

pub const MANIFEST_FILE: &str = "tron.json";
pub const MAX_NAME_CHARS: usize = 64;

const FORBIDDEN_NAME_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks that `name` can be used as both a folder name and a file stem on
/// every supported platform.
pub fn validate_name(name: &str) -> Result<(), CoreError> {
    let invalid = |message: String| Err(CoreError::InvalidName(message));
    if name.is_empty() {
        return invalid("Enter a project name.".to_owned());
    }
    if name.trim() != name {
        return invalid("Project names can't start or end with a space.".to_owned());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return invalid(format!(
            "Project names can be at most {MAX_NAME_CHARS} characters."
        ));
    }
    if name
        .chars()
        .any(|c| FORBIDDEN_NAME_CHARS.contains(&c) || c.is_control())
    {
        return invalid(
            "Project names can't contain any of these characters: < > : \" / \\ | ? *".to_owned(),
        );
    }
    if name.ends_with('.') {
        return invalid("Project names can't end with a period.".to_owned());
    }
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        return invalid(format!(
            "\"{name}\" is reserved by Windows. Choose another name."
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    dir: PathBuf,
    manifest: Manifest,
    schematic: Schematic,
    board: Board,
}

impl Project {
    /// A blank project in memory. Nothing is written until [`Project::save`].
    pub fn new(dir: PathBuf, name: &str, uuid: Uuid) -> Result<Project, CoreError> {
        validate_name(name)?;
        Ok(Project {
            dir,
            manifest: Manifest::new(name),
            schematic: Schematic::new_empty(uuid),
            board: Board::new_empty(),
        })
    }

    /// Creates the folder `parent/name` and writes a blank project into it.
    /// The folder may already exist only if it is empty.
    pub fn create(parent: &Path, name: &str, uuid: Uuid) -> Result<Project, CoreError> {
        let dir = prepare_dir(parent, name)?;
        let project = Project::new(dir, name, uuid)?;
        project.save()?;
        Ok(project)
    }

    /// Opens the project in `path`, which is either the project folder or a
    /// file inside it. A KiCad project without `tron.json` opens too; the
    /// manifest is written on the first save.
    pub fn load(path: &Path) -> Result<Project, CoreError> {
        let (dir, picked_stem) = if path.is_dir() {
            (path.to_path_buf(), None)
        } else {
            let dir = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or_else(|| CoreError::NotAProject(path.to_path_buf()))?;
            let stem = match path.extension().and_then(|ext| ext.to_str()) {
                Some("kicad_sch" | "kicad_pcb" | "kicad_pro") => path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned),
                _ => None,
            };
            (dir.to_path_buf(), stem)
        };
        if !dir.is_dir() {
            return Err(CoreError::io(
                &dir,
                io::Error::new(io::ErrorKind::NotFound, "the folder does not exist"),
            ));
        }

        let manifest_path = dir.join(MANIFEST_FILE);
        let has_manifest = manifest_path.is_file();
        let manifest = if has_manifest {
            let source = read_text(&manifest_path)?;
            Manifest::parse(&source).map_err(|error| manifest_error(&manifest_path, error))?
        } else {
            let name = match picked_stem {
                Some(name) => name,
                None => root_schematic_stem(&dir)?,
            };
            Manifest::new(&name)
        };
        if manifest.name.is_empty() || manifest.name.contains(['/', '\\']) {
            return Err(CoreError::InvalidFile {
                path: manifest_path,
                message: "the project name must be a plain file name".to_owned(),
            });
        }

        let schematic_path = dir.join(format!("{}.kicad_sch", manifest.name));
        if !schematic_path.is_file() {
            return Err(if has_manifest {
                CoreError::io(
                    &schematic_path,
                    io::Error::new(io::ErrorKind::NotFound, "the schematic file is missing"),
                )
            } else {
                CoreError::NotAProject(dir)
            });
        }
        let schematic = Schematic::parse(&read_text(&schematic_path)?)
            .map_err(|error| file_error(&schematic_path, error))?;

        let board_path = dir.join(format!("{}.kicad_pcb", manifest.name));
        let board = if board_path.is_file() {
            Board::parse(&read_text(&board_path)?)
                .map_err(|error| file_error(&board_path, error))?
        } else {
            Board::new_empty()
        };

        Ok(Project {
            dir,
            manifest,
            schematic,
            board,
        })
    }

    /// Writes all three files. Each write is atomic, so a failure leaves the
    /// previous version of that file intact.
    pub fn save(&self) -> Result<(), CoreError> {
        write_atomic(&self.manifest_path(), &self.manifest.to_file_string())?;
        write_atomic(&self.schematic_path(), &self.schematic.to_file_string())?;
        write_atomic(&self.board_path(), &self.board.to_file_string())
    }

    /// Writes a copy of this project into the new folder `parent/name` and
    /// returns the copy. `self` is left unchanged.
    pub fn save_as(&self, parent: &Path, name: &str) -> Result<Project, CoreError> {
        let dir = prepare_dir(parent, name)?;
        let mut copy = self.clone();
        copy.dir = dir;
        copy.manifest.name = name.to_owned();
        copy.save()?;
        Ok(copy)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn title_block(&self) -> TitleBlock {
        self.schematic.title_block()
    }

    pub fn grid_size(&self) -> Nm {
        Nm(self.manifest.schematic.grid_size_nm)
    }

    pub fn page_size(&self) -> PageSize {
        self.schematic.page_size()
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.dir.join(MANIFEST_FILE)
    }

    pub fn schematic_path(&self) -> PathBuf {
        self.dir.join(format!("{}.kicad_sch", self.manifest.name))
    }

    pub fn board_path(&self) -> PathBuf {
        self.dir.join(format!("{}.kicad_pcb", self.manifest.name))
    }

    pub(crate) fn set_title_block(&mut self, block: &TitleBlock) {
        self.schematic.set_title_block(block);
    }

    pub(crate) fn set_grid_size(&mut self, grid: Nm) {
        self.manifest.schematic.grid_size_nm = grid.0;
    }
}

/// Writes `contents` to a temporary file next to `path`, flushes it to disk,
/// then renames it over `path`.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), CoreError> {
    let (Some(dir), Some(file_name)) = (path.parent(), path.file_name()) else {
        return Err(CoreError::io(
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "not a file path"),
        ));
    };
    let temp = dir.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    write_then_rename(&temp, path, contents).map_err(|error| {
        // The temporary file is only debris at this point; the target is intact.
        let _ = fs::remove_file(&temp);
        CoreError::io(path, error)
    })
}

fn write_then_rename(temp: &Path, path: &Path, contents: &str) -> io::Result<()> {
    let mut file = File::create(temp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)
}

fn prepare_dir(parent: &Path, name: &str) -> Result<PathBuf, CoreError> {
    validate_name(name)?;
    if !parent.is_dir() {
        return Err(CoreError::io(
            parent,
            io::Error::new(io::ErrorKind::NotFound, "the folder does not exist"),
        ));
    }
    let dir = parent.join(name);
    match fs::read_dir(&dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(CoreError::AlreadyExists(dir));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&dir).map_err(|error| CoreError::io(&dir, error))?;
        }
        Err(error) => return Err(CoreError::io(&dir, error)),
    }
    Ok(dir)
}

/// Picks the root sheet of a KiCad project folder: the schematic named after
/// the `.kicad_pro` file, or the only schematic in the folder.
fn root_schematic_stem(dir: &Path) -> Result<String, CoreError> {
    let mut schematics = Vec::new();
    let mut projects = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| CoreError::io(dir, error))?;
    for entry in entries {
        let path = entry.map_err(|error| CoreError::io(dir, error))?.path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("kicad_sch") => schematics.push(stem.to_owned()),
            Some("kicad_pro") => projects.push(stem.to_owned()),
            _ => {}
        }
    }
    if let Some(stem) = projects.iter().find(|stem| schematics.contains(stem)) {
        return Ok(stem.clone());
    }
    match schematics.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(CoreError::NotAProject(dir.to_path_buf())),
    }
}

fn read_text(path: &Path) -> Result<String, CoreError> {
    fs::read_to_string(path).map_err(|error| CoreError::io(path, error))
}

fn file_error(path: &Path, error: FileError) -> CoreError {
    match error {
        FileError::Syntax(error) => CoreError::Parse {
            path: path.to_path_buf(),
            line: error.line,
            column: error.column,
            message: error.message,
        },
        FileError::WrongKind { expected } => CoreError::InvalidFile {
            path: path.to_path_buf(),
            message: format!("expected a {expected} file"),
        },
    }
}

fn manifest_error(path: &Path, error: ManifestError) -> CoreError {
    match error {
        ManifestError::Syntax {
            line,
            column,
            message,
        } => CoreError::Parse {
            path: path.to_path_buf(),
            line,
            column,
            message,
        },
        ManifestError::Invalid(message) => CoreError::InvalidFile {
            path: path.to_path_buf(),
            message,
        },
        ManifestError::UnsupportedVersion(found) => CoreError::UnsupportedVersion {
            path: path.to_path_buf(),
            found,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_error(name: &str) -> String {
        match validate_name(name) {
            Err(CoreError::InvalidName(message)) => message,
            other => panic!("expected {name:?} to be rejected, got {other:?}"),
        }
    }

    #[test]
    fn accepts_ordinary_project_names() {
        for name in ["amp", "Buck converter (rev 2)", "sensor_v1.2", "Ünïcode"] {
            assert!(validate_name(name).is_ok(), "{name:?} should be accepted");
        }
    }

    #[test]
    fn rejects_names_that_cannot_be_file_names() {
        assert_eq!(name_error(""), "Enter a project name.");
        assert_eq!(
            name_error(" amp"),
            "Project names can't start or end with a space."
        );
        assert!(name_error("a/b").contains("can't contain"));
        assert!(name_error("a:b").contains("can't contain"));
        assert!(name_error("tab\there").contains("can't contain"));
        assert_eq!(name_error("amp."), "Project names can't end with a period.");
        assert!(name_error("con").contains("reserved by Windows"));
        assert!(name_error("LPT1.backup").contains("reserved by Windows"));
        assert!(name_error(&"x".repeat(MAX_NAME_CHARS + 1)).contains("at most 64"));
    }
}
