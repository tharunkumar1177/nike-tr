use std::path::Path;

use serde::Serialize;
use tron_core::CoreError;
use ts_rs::TS;

/// A stable, machine-readable error category the UI maps to its own copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AppErrorKind {
    NoProject,
    Io,
    Parse,
    InvalidFile,
    UnsupportedVersion,
    InvalidName,
    AlreadyExists,
    NotAProject,
    InvalidEdit,
    NothingToUndo,
    NothingToRedo,
    Settings,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppError {
    pub kind: AppErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(kind: AppErrorKind, message: impl Into<String>, detail: Option<String>) -> AppError {
        AppError {
            kind,
            message: message.into(),
            detail,
        }
    }

    pub fn no_project() -> AppError {
        AppError::new(AppErrorKind::NoProject, "No project is open.", None)
    }

    pub fn window(error: tauri::Error) -> AppError {
        AppError::new(
            AppErrorKind::Window,
            "The application window could not be updated.",
            Some(error.to_string()),
        )
    }

    pub fn settings(message: impl Into<String>, detail: impl ToString) -> AppError {
        AppError::new(AppErrorKind::Settings, message, Some(detail.to_string()))
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl From<CoreError> for AppError {
    fn from(error: CoreError) -> AppError {
        use AppErrorKind as Kind;
        match error {
            CoreError::Io { path, source } => AppError::new(
                Kind::Io,
                format!("Could not access {}.", path.display()),
                Some(source.to_string()),
            ),
            CoreError::Parse {
                path,
                line,
                column,
                message,
            } => AppError::new(
                Kind::Parse,
                format!(
                    "Parse error in {} at line {line}, column {column}.",
                    file_name(&path)
                ),
                Some(message),
            ),
            CoreError::InvalidFile { path, message } => AppError::new(
                Kind::InvalidFile,
                format!("{} is not a valid project file.", file_name(&path)),
                Some(message),
            ),
            CoreError::UnsupportedVersion { path, found } => AppError::new(
                Kind::UnsupportedVersion,
                format!("{} was saved by a newer version of Tron.", file_name(&path)),
                Some(format!(
                    "Format version {found} is not supported by this version."
                )),
            ),
            CoreError::InvalidName(message) => AppError::new(Kind::InvalidName, message, None),
            CoreError::AlreadyExists(path) => AppError::new(
                Kind::AlreadyExists,
                format!(
                    "The folder {} already exists and is not empty.",
                    path.display()
                ),
                None,
            ),
            CoreError::NotAProject(path) => AppError::new(
                Kind::NotAProject,
                format!("No Tron or KiCad project was found in {}.", path.display()),
                None,
            ),
            CoreError::InvalidEdit(message) => AppError::new(Kind::InvalidEdit, message, None),
            CoreError::NothingToUndo => {
                AppError::new(Kind::NothingToUndo, "There is nothing to undo.", None)
            }
            CoreError::NothingToRedo => {
                AppError::new(Kind::NothingToRedo, "There is nothing to redo.", None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_errors_name_the_file_line_and_column() {
        let error = AppError::from(CoreError::Parse {
            path: PathBuf::from("projects").join("amp").join("amp.kicad_sch"),
            line: 12,
            column: 4,
            message: "this '(' is never closed".to_owned(),
        });
        assert_eq!(error.kind, AppErrorKind::Parse);
        assert_eq!(
            error.message,
            "Parse error in amp.kicad_sch at line 12, column 4."
        );
        assert_eq!(error.detail.as_deref(), Some("this '(' is never closed"));
    }

    #[test]
    fn serializes_with_camel_case_kinds() {
        let json = serde_json::to_value(AppError::no_project()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "noProject",
                "message": "No project is open.",
                "detail": null
            })
        );
    }
}
