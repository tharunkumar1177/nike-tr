//! Simulation commands. One ngspice run at a time goes to a blocking worker, its results
//! stay here, and the UI fetches each waveform reduced to what its plot can show.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};
use tron_core::Xy;
use tron_sim::{Analysis, Engine, OpAnchors, OpKind, OpValue, Plot, deck, op_anchors};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{AppError, AppErrorKind};
use crate::ipc::AppState;

pub const SIMULATION_PROGRESS_EVENT: &str = "simulation-progress";
/// The most samples one waveform request returns.
pub const MAX_WAVEFORM_POINTS: u32 = 20_000;
/// Names the ngspice library to load instead of the bundled one.
const NGSPICE_ENV: &str = "TRON_NGSPICE";
const NGSPICE_RESOURCE: &str = "ngspice/ngspice.dll";
/// Where the installer puts the code models, beside the library.
const CODE_MODELS_DIR: &str = "codemodels";

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SimulationFinished {
    /// Names these results in `fetch_waveform`.
    pub run_id: Uuid,
    /// The vector the others are plotted against, such as `time`, or `null` for an
    /// operating point, whose vectors hold one value each.
    pub scale: Option<String>,
    /// Every result vector in lower case, such as `/out` or `v1#branch`.
    pub vectors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SimulationProgress {
    pub percentage: f64,
}

/// One waveform sample, in SI units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WaveformPoint {
    pub x: f64,
    pub y: f64,
}

/// What an operating-point annotation measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OpAnnotationKind {
    Voltage,
    Current,
}

/// One operating-point value and where the schematic shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpAnnotation {
    /// The result vector, as `fetch_waveform` names it.
    pub vector: String,
    pub kind: OpAnnotationKind,
    /// The net name of a voltage, or the symbol reference of a current.
    pub label: String,
    /// In volts or amperes.
    pub value: f64,
    pub position: Xy,
}

impl From<OpValue> for OpAnnotation {
    fn from(value: OpValue) -> OpAnnotation {
        let kind = match value.anchor.kind {
            OpKind::Voltage => OpAnnotationKind::Voltage,
            OpKind::Current => OpAnnotationKind::Current,
        };
        OpAnnotation {
            vector: value.vector,
            kind,
            label: value.anchor.label,
            value: value.value,
            position: Xy::from(value.anchor.position),
        }
    }
}

#[derive(Default)]
pub struct SimState {
    inner: Mutex<SimInner>,
}

/// The results of the last finished run.
struct Results {
    run_id: Uuid,
    plot: Plot,
    /// Where each operating-point value is shown, found from the schematic that ran.
    anchors: OpAnchors,
}

#[derive(Default)]
struct SimInner {
    /// The cancel flag of the run in progress.
    running: Option<Arc<AtomicBool>>,
    results: Option<Results>,
    /// Increases on every document change, so a run that started before a change can't
    /// store results for the old document.
    generation: u64,
}

impl SimState {
    fn lock(&self) -> MutexGuard<'_, SimInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Discards the stored results, and those of a run in progress, because the document
    /// they came from changed.
    pub fn invalidate(&self) {
        let mut inner = self.lock();
        inner.results = None;
        inner.generation = inner.generation.wrapping_add(1);
    }
}

impl SimInner {
    fn results(&self, run_id: Uuid) -> Result<&Results, AppError> {
        match &self.results {
            Some(results) if results.run_id == run_id => Ok(results),
            _ => Err(AppError::new(
                AppErrorKind::SimNotFound,
                "These simulation results are no longer available. Run the simulation again.",
                None,
            )),
        }
    }

    /// The operating-point values of run `run_id`. A transient has none.
    fn op_annotations(&self, run_id: Uuid) -> Result<Vec<OpAnnotation>, AppError> {
        let results = self.results(run_id)?;
        if results.plot.scale.is_some() {
            return Ok(Vec::new());
        }
        let values = results.anchors.values(&results.plot);
        Ok(values.into_iter().map(OpAnnotation::from).collect())
    }
}

/// The ngspice library: the one `TRON_NGSPICE` names, or else the bundled one.
fn ngspice_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let mut checked = Vec::new();
    if let Some(path) = std::env::var_os(NGSPICE_ENV).map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        checked.push(path);
    }
    let paths = app.path();
    if let Ok(path) = paths.resolve(NGSPICE_RESOURCE, BaseDirectory::Resource) {
        if path.is_file() {
            return Ok(path);
        }
        checked.push(path);
    }
    let checked: Vec<String> = checked
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    Err(AppError::new(
        AppErrorKind::SimEngineLoad,
        "The ngspice simulator was not found.",
        (!checked.is_empty()).then(|| format!("Looked for {}.", checked.join(" and "))),
    ))
}

/// The XSPICE code models for the library at `library`: the installer's `codemodels`
/// folder beside it, or `lib/ngspice` in the layout of ngspice's own archive.
fn code_model_dir(library: &Path) -> Result<PathBuf, AppError> {
    let beside = library.parent().map(|dir| dir.join(CODE_MODELS_DIR));
    let archive = library
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("lib").join("ngspice"));
    let candidates: Vec<PathBuf> = beside.into_iter().chain(archive).collect();
    if let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) {
        return Ok(dir.clone());
    }
    let checked: Vec<String> = candidates
        .iter()
        .map(|dir| dir.display().to_string())
        .collect();
    Err(AppError::new(
        AppErrorKind::SimEngineLoad,
        "The ngspice code models were not found.",
        Some(format!("Looked in {}.", checked.join(" and "))),
    ))
}

fn unexpected(message: &str, error: impl ToString) -> AppError {
    AppError::new(
        AppErrorKind::SimAnalysisFailed,
        message,
        Some(error.to_string()),
    )
}

/// Checks the schematic, runs the project's analysis, and keeps the results until the
/// document changes or the next run finishes. Emits `simulation-progress` as it runs.
#[tauri::command]
pub async fn start_simulation(app: AppHandle) -> Result<SimulationFinished, AppError> {
    let path = ngspice_path(&app)?;
    let code_models = code_model_dir(&path)?;
    let state = app.state::<AppState>();
    let (deck, anchors, cancel, generation) = state.with_session(|session| {
        let project = session.project();
        let analysis = project.analysis().map(Analysis::from);
        let deck = deck(project.schematic(), analysis.as_ref())?;
        let anchors = op_anchors(project.schematic(), &deck)?;
        let sim = app.state::<SimState>();
        let mut inner = sim.lock();
        if inner.running.is_some() {
            return Err(AppError::new(
                AppErrorKind::SimRunning,
                "A simulation is already running.",
                None,
            ));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        inner.running = Some(Arc::clone(&cancel));
        Ok((deck, anchors, cancel, inner.generation))
    })??;

    let emitter = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let mut engine = Engine::lock(&path, Some(&code_models))?;
        engine.run_cancellable(&deck, &cancel, move |percentage| {
            let progress = SimulationProgress { percentage };
            if let Err(error) = emitter.emit(SIMULATION_PROGRESS_EVENT, progress) {
                eprintln!("could not report simulation progress: {error}");
            }
        })
    })
    .await;

    let sim = app.state::<SimState>();
    let mut inner = sim.lock();
    inner.running = None;
    let plot = match outcome {
        Ok(result) => result?,
        Err(error) => return Err(unexpected("The simulation stopped unexpectedly.", error)),
    };
    if inner.generation != generation {
        return Err(AppError::new(
            AppErrorKind::SimCancelled,
            "The schematic changed while the simulation ran, so its results were discarded.",
            None,
        ));
    }
    let run_id = Uuid::new_v4();
    let finished = SimulationFinished {
        run_id,
        scale: plot.scale.clone(),
        vectors: plot
            .vectors
            .iter()
            .map(|vector| vector.name.clone())
            .collect(),
    };
    inner.results = Some(Results {
        run_id,
        plot,
        anchors,
    });
    Ok(finished)
}

/// Stops the simulation in progress, which then fails with `simCancelled`. Does nothing
/// when none is running.
#[tauri::command]
pub async fn cancel_simulation(app: AppHandle) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let sim = app.state::<SimState>();
        // Held while halting, so the halt can't reach a run started after this one.
        let inner = sim.lock();
        if let Some(cancel) = &inner.running {
            cancel.store(true, Ordering::SeqCst);
            Engine::halt();
        }
    })
    .await
    .map_err(|error| unexpected("The simulation could not be cancelled.", error))
}

/// The vector `vector` of run `run_id` between `min_x` and `max_x`, reduced to at most
/// `budget` samples (capped at `MAX_WAVEFORM_POINTS`) so peaks stay visible.
#[tauri::command]
pub fn fetch_waveform(
    state: State<'_, SimState>,
    run_id: Uuid,
    vector: String,
    min_x: f64,
    max_x: f64,
    budget: u32,
) -> Result<Vec<WaveformPoint>, AppError> {
    let inner = state.lock();
    let plot = &inner.results(run_id)?.plot;
    let budget = budget.min(MAX_WAVEFORM_POINTS) as usize;
    let samples = plot
        .waveform(&vector, min_x, max_x, budget)
        .ok_or_else(|| {
            AppError::new(
                AppErrorKind::SimNotFound,
                format!("The simulation has no signal named {vector}."),
                None,
            )
        })?;
    Ok(samples
        .into_iter()
        .map(|sample| WaveformPoint {
            x: sample.x,
            y: sample.y,
        })
        .collect())
}

/// The operating-point values of run `run_id`, each with where the schematic shows it.
/// Empty for a transient. Ground and nodes inside device models are left out.
#[tauri::command]
pub fn fetch_op_annotations(
    state: State<'_, SimState>,
    run_id: Uuid,
) -> Result<Vec<OpAnnotation>, AppError> {
    state.lock().op_annotations(run_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tron_sim::Vector;

    #[test]
    fn code_models_are_found_beside_the_library_or_in_the_archive_layout() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("ngspice");
        std::fs::create_dir_all(installed.join(CODE_MODELS_DIR)).unwrap();
        let found = code_model_dir(&installed.join("ngspice.dll")).unwrap();
        assert_eq!(found, installed.join(CODE_MODELS_DIR));

        let archive = root.path().join("Spice64_dll");
        let models = archive.join("lib").join("ngspice");
        std::fs::create_dir_all(&models).unwrap();
        let library = archive.join("dll-vs").join("ngspice.dll");
        assert_eq!(code_model_dir(&library).unwrap(), models);

        let missing = root.path().join("empty").join("ngspice.dll");
        let error = code_model_dir(&missing).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::SimEngineLoad);
    }

    #[test]
    fn a_document_change_discards_the_results() {
        let state = SimState::default();
        let plot = Plot {
            name: "op1".to_owned(),
            scale: None,
            vectors: vec![Vector {
                name: "/out".to_owned(),
                values: vec![1.5],
            }],
        };
        state.lock().results = Some(Results {
            run_id: Uuid::nil(),
            plot,
            anchors: OpAnchors::default(),
        });
        state.invalidate();
        let inner = state.lock();
        assert!(inner.results.is_none());
        assert_eq!(inner.generation, 1);
    }

    #[test]
    fn op_annotations_need_the_current_run_and_a_transient_has_none() {
        let mut inner = SimInner::default();
        let error = inner.op_annotations(Uuid::nil()).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::SimNotFound);

        let transient = Plot {
            name: "tran1".to_owned(),
            scale: Some("time".to_owned()),
            vectors: Vec::new(),
        };
        inner.results = Some(Results {
            run_id: Uuid::nil(),
            plot: transient,
            anchors: OpAnchors::default(),
        });
        assert!(inner.op_annotations(Uuid::nil()).unwrap().is_empty());
        let stale = inner.op_annotations(Uuid::from_u128(1)).unwrap_err();
        assert_eq!(stale.kind, AppErrorKind::SimNotFound);
    }

    #[test]
    fn an_op_annotation_serializes_for_the_ui() {
        let annotation = OpAnnotation {
            vector: "/out".to_owned(),
            kind: OpAnnotationKind::Voltage,
            label: "/OUT".to_owned(),
            value: 1.5,
            position: Xy(1_270_000, 2_540_000),
        };
        let json = serde_json::to_value(&annotation).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "vector": "/out",
                "kind": "voltage",
                "label": "/OUT",
                "value": 1.5,
                "position": [1_270_000, 2_540_000]
            })
        );
    }
}
