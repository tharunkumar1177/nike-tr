use std::path::Path;

use serde::Serialize;
use tron_core::CoreError;
use tron_sim::{EngineError, PreflightError};
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
    /// A download failed; retrying resumes it.
    Network,
    /// A download did not match its pinned checksum and was discarded.
    Checksum,
    /// The component library has not been downloaded.
    LibraryNotReady,
    /// The cached component library could not be read.
    Library,
    /// No simulation analysis is set.
    SimNoAnalysis,
    /// The analysis parameters are out of range; the message names the parameter.
    SimInvalidAnalysis,
    /// The circuit has no ground net.
    SimNoGround,
    /// A symbol has no usable simulation model; the detail names the symbol.
    SimNoModel,
    /// A simulated symbol is not annotated; the detail names its reference.
    SimUnannotated,
    /// Two simulated symbols share a reference; the detail names it.
    SimDuplicateReference,
    /// The schematic's connectivity could not be derived.
    SimNetlist,
    /// No symbol takes part in the simulation.
    SimEmpty,
    /// The ngspice simulator could not be found or loaded.
    SimEngineLoad,
    /// ngspice rejected the circuit; the detail is what it reported.
    SimCircuitRejected,
    /// The analysis failed, or ngspice stopped; the detail is what it reported.
    SimAnalysisFailed,
    /// A simulation is already running.
    SimRunning,
    /// The simulation was cancelled, or the schematic changed while it ran.
    SimCancelled,
    /// The requested results or signal no longer exist.
    SimNotFound,
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

impl From<PreflightError> for AppError {
    fn from(error: PreflightError) -> AppError {
        use AppErrorKind as Kind;
        let message = error.to_string();
        let (kind, detail) = match error {
            PreflightError::NoAnalysis => (Kind::SimNoAnalysis, None),
            PreflightError::InvalidAnalysis(_) => (Kind::SimInvalidAnalysis, None),
            PreflightError::NoGround => (Kind::SimNoGround, None),
            PreflightError::NoModel { reference, .. } => (Kind::SimNoModel, Some(reference)),
            PreflightError::Unannotated(reference) => (Kind::SimUnannotated, Some(reference)),
            PreflightError::DuplicateReference(reference) => {
                (Kind::SimDuplicateReference, Some(reference))
            }
            PreflightError::Netlist(_) => (Kind::SimNetlist, None),
            PreflightError::Empty => (Kind::SimEmpty, None),
        };
        AppError::new(kind, message, detail)
    }
}

impl From<EngineError> for AppError {
    fn from(error: EngineError) -> AppError {
        use AppErrorKind as Kind;
        match error {
            EngineError::Load { .. } | EngineError::AlreadyLoaded { .. } => AppError::new(
                Kind::SimEngineLoad,
                "The ngspice simulator could not be loaded.",
                Some(error.to_string()),
            ),
            EngineError::Circuit(lines) => AppError::new(
                Kind::SimCircuitRejected,
                "ngspice rejected the circuit.",
                Some(lines.join("\n")),
            ),
            EngineError::Analysis(lines) => AppError::new(
                Kind::SimAnalysisFailed,
                "The simulation failed.",
                Some(lines.join("\n")),
            ),
            EngineError::Exited => AppError::new(Kind::SimAnalysisFailed, error.to_string(), None),
            EngineError::Cancelled => {
                AppError::new(Kind::SimCancelled, "The simulation was cancelled.", None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn preflight_errors_keep_their_kind_and_name_the_symbol() {
        let error = AppError::from(PreflightError::NoModel {
            reference: "D1".to_owned(),
            reason: "no SPICE model".to_owned(),
        });
        assert_eq!(error.kind, AppErrorKind::SimNoModel);
        assert_eq!(error.message, "D1: no SPICE model");
        assert_eq!(error.detail.as_deref(), Some("D1"));
        let no_ground = AppError::from(PreflightError::NoGround);
        let json = serde_json::to_value(no_ground).unwrap();
        assert_eq!(json["kind"], "simNoGround");
        let cancelled = AppError::from(EngineError::Cancelled);
        assert_eq!(cancelled.kind, AppErrorKind::SimCancelled);
    }

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
