use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Every way a project operation in `core` can fail.
#[derive(Debug)]
pub enum CoreError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        line: usize,
        column: usize,
        message: String,
    },
    InvalidFile {
        path: PathBuf,
        message: String,
    },
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
    },
    InvalidName(String),
    AlreadyExists(PathBuf),
    NotAProject(PathBuf),
    InvalidEdit(String),
    NothingToUndo,
    NothingToRedo,
}

impl CoreError {
    pub(crate) fn io(path: &Path, source: io::Error) -> CoreError {
        CoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            CoreError::Parse {
                path,
                line,
                column,
                message,
            } => write!(
                f,
                "{}: line {line}, column {column}: {message}",
                path.display()
            ),
            CoreError::InvalidFile { path, message } => write!(f, "{}: {message}", path.display()),
            CoreError::UnsupportedVersion { path, found } => write!(
                f,
                "{}: format version {found} is newer than this version of Tron supports",
                path.display()
            ),
            CoreError::InvalidName(message) | CoreError::InvalidEdit(message) => {
                f.write_str(message)
            }
            CoreError::AlreadyExists(path) => {
                write!(f, "{} already exists and is not empty", path.display())
            }
            CoreError::NotAProject(path) => {
                write!(
                    f,
                    "{} does not contain a Tron or KiCad project",
                    path.display()
                )
            }
            CoreError::NothingToUndo => f.write_str("there is nothing to undo"),
            CoreError::NothingToRedo => f.write_str("there is nothing to redo"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
