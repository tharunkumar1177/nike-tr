use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Every way building or reading the component index can fail.
#[derive(Debug)]
pub enum LibraryError {
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
    /// Every library that could not be read during an index build.
    Sources(Vec<LibraryError>),
    Database(rusqlite::Error),
    Preview(serde_json::Error),
    SchemaMismatch {
        found: i64,
        expected: i64,
    },
    /// A stored symbol definition that cannot be read or merged with its ancestors.
    Definition {
        lib: String,
        name: String,
        message: String,
    },
}

impl LibraryError {
    pub(crate) fn io(path: &Path, source: io::Error) -> LibraryError {
        LibraryError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn invalid(path: &Path, message: impl Into<String>) -> LibraryError {
        LibraryError::InvalidFile {
            path: path.to_path_buf(),
            message: message.into(),
        }
    }
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            LibraryError::Parse {
                path,
                line,
                column,
                message,
            } => write!(
                f,
                "{}: line {line}, column {column}: {message}",
                path.display()
            ),
            LibraryError::InvalidFile { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            LibraryError::Sources(errors) => {
                f.write_str("some libraries could not be read:")?;
                for error in errors {
                    write!(f, "\n  {error}")?;
                }
                Ok(())
            }
            LibraryError::Database(err) => write!(f, "index database: {err}"),
            LibraryError::Preview(err) => write!(f, "preview geometry: {err}"),
            LibraryError::SchemaMismatch { found, expected } => write!(
                f,
                "index schema version {found} does not match the supported version {expected}"
            ),
            LibraryError::Definition { lib, name, message } => {
                write!(f, "symbol {lib}:{name}: {message}")
            }
        }
    }
}

impl std::error::Error for LibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LibraryError::Io { source, .. } => Some(source),
            LibraryError::Database(err) => Some(err),
            LibraryError::Preview(err) => Some(err),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(err: rusqlite::Error) -> LibraryError {
        LibraryError::Database(err)
    }
}

impl From<serde_json::Error> for LibraryError {
    fn from(err: serde_json::Error) -> LibraryError {
        LibraryError::Preview(err)
    }
}
