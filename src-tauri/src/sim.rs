//! Simulation commands. One ngspice run at a time goes to a blocking worker, its results
//! stay here, and the UI fetches each waveform reduced to what its plot can show.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};
use tron_core::{Nm, Point, Xy};
use tron_sim::{
    Analysis, Engine, Measurements, OpAnchors, OpKind, OpValue, Plot, Probe, ProbeMap, deck,
    op_anchors, probe_map,
};
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

/// The statistics of a trace over a span of the run, in the units of its vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceMeasurements {
    /// The span measured, clipped to the run.
    pub from: f64,
    pub to: f64,
    pub min: f64,
    pub max: f64,
    /// The mean over the span, so uneven time steps are weighted by their width.
    pub average: f64,
    pub rms: f64,
    pub peak_to_peak: f64,
}

impl From<Measurements> for TraceMeasurements {
    fn from(measured: Measurements) -> TraceMeasurements {
        TraceMeasurements {
            from: measured.from,
            to: measured.to,
            min: measured.min,
            max: measured.max,
            average: measured.average,
            rms: measured.rms,
            peak_to_peak: measured.peak_to_peak,
        }
    }
}

/// Exact readings of one trace, from its full-resolution samples.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceReadout {
    /// `null` when the span requested misses the run.
    pub span: Option<TraceMeasurements>,
    /// The value at each cursor requested, in order, or `null` for one outside the run.
    pub cursors: Vec<Option<f64>>,
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

/// A schematic item the probe tool clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum ProbeTarget {
    /// A wire, label, junction, or symbol body.
    Item { uuid: Uuid },
    /// The pin of the symbol `symbol` that connects at `position`.
    Pin { symbol: Uuid, position: Xy },
}

/// What a probed item measures in a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum ProbeResult {
    /// A result vector to plot.
    Vector {
        /// As `fetch_waveform` names it, such as `/out` or `v1#branch`.
        vector: String,
        /// The net name of a voltage, or the symbol reference of a current.
        label: String,
        quantity: OpAnnotationKind,
    },
    /// The ground net, which is always 0 V.
    Ground,
    /// A device whose current ngspice doesn't report, which is any but a voltage source or
    /// an inductor.
    NoCurrent { reference: String },
    /// An item that isn't part of the simulated circuit.
    NotSimulated,
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
    /// What each item of the schematic that ran measures.
    probes: ProbeMap,
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

    /// What `target` measures in run `run_id`.
    fn resolve_probe(&self, run_id: Uuid, target: ProbeTarget) -> Result<ProbeResult, AppError> {
        let results = self.results(run_id)?;
        let probe = match target {
            ProbeTarget::Item { uuid } => results.probes.item(uuid),
            ProbeTarget::Pin { symbol, position } => {
                let at = Point {
                    x: Nm(position.0),
                    y: Nm(position.1),
                };
                results.probes.pin(symbol, at)
            }
        };
        let find = |name: &str| {
            let vectors = &results.plot.vectors;
            let found = vectors.iter().find(|v| v.name.eq_ignore_ascii_case(name));
            found.map(|vector| vector.name.clone())
        };
        Ok(match probe {
            None => ProbeResult::NotSimulated,
            Some(Probe::Ground) => ProbeResult::Ground,
            Some(Probe::Node { node, net }) => match find(node) {
                Some(vector) => ProbeResult::Vector {
                    vector,
                    label: net.clone(),
                    quantity: OpAnnotationKind::Voltage,
                },
                None => ProbeResult::NotSimulated,
            },
            Some(Probe::Device { device, reference }) => match find(&format!("{device}#branch")) {
                Some(vector) => ProbeResult::Vector {
                    vector,
                    label: reference.clone(),
                    quantity: OpAnnotationKind::Current,
                },
                None => ProbeResult::NoCurrent {
                    reference: reference.clone(),
                },
            },
        })
    }

    /// The vector `vector` of run `run_id` between `min_x` and `max_x`, where a missing
    /// bound means the end of the run, reduced to at most `budget` samples.
    fn waveform(
        &self,
        run_id: Uuid,
        vector: &str,
        min_x: Option<f64>,
        max_x: Option<f64>,
        budget: u32,
    ) -> Result<Vec<WaveformPoint>, AppError> {
        let plot = &self.results(run_id)?.plot;
        let budget = budget.min(MAX_WAVEFORM_POINTS) as usize;
        let min_x = min_x.unwrap_or(f64::NEG_INFINITY);
        let max_x = max_x.unwrap_or(f64::INFINITY);
        let samples = plot
            .waveform(vector, min_x, max_x, budget)
            .ok_or_else(|| missing_vector(vector))?;
        Ok(samples
            .into_iter()
            .map(|sample| WaveformPoint {
                x: sample.x,
                y: sample.y,
            })
            .collect())
    }

    /// The statistics of `vector` in run `run_id` between `min_x` and `max_x`, where a
    /// missing bound means the end of the run, and its value at each of `cursors`.
    fn measure_trace(
        &self,
        run_id: Uuid,
        vector: &str,
        min_x: Option<f64>,
        max_x: Option<f64>,
        cursors: &[f64],
    ) -> Result<TraceReadout, AppError> {
        let plot = &self.results(run_id)?.plot;
        let Some(series) = plot.series(vector) else {
            return Err(missing_vector(vector));
        };
        let min_x = min_x.unwrap_or(f64::NEG_INFINITY);
        let max_x = max_x.unwrap_or(f64::INFINITY);
        Ok(TraceReadout {
            span: series.measure(min_x, max_x).map(TraceMeasurements::from),
            cursors: cursors.iter().map(|&at| series.value_at(at)).collect(),
        })
    }
}

fn missing_vector(vector: &str) -> AppError {
    AppError::new(
        AppErrorKind::SimNotFound,
        format!("The simulation has no signal named {vector}."),
        None,
    )
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
    let (deck, anchors, probes, cancel, generation) = state.with_session(|session| {
        let project = session.project();
        let analysis = project.analysis().map(Analysis::from);
        let deck = deck(project.schematic(), analysis.as_ref())?;
        let anchors = op_anchors(project.schematic(), &deck)?;
        let probes = probe_map(project.schematic(), &deck)?;
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
        Ok((deck, anchors, probes, cancel, inner.generation))
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
        probes,
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

/// The vector `vector` of run `run_id` between `min_x` and `max_x`, where a missing bound
/// means the end of the run, reduced to at most `budget` samples (capped at
/// `MAX_WAVEFORM_POINTS`) so peaks stay visible.
#[tauri::command]
pub fn fetch_waveform(
    state: State<'_, SimState>,
    run_id: Uuid,
    vector: String,
    min_x: Option<f64>,
    max_x: Option<f64>,
    budget: u32,
) -> Result<Vec<WaveformPoint>, AppError> {
    state.lock().waveform(run_id, &vector, min_x, max_x, budget)
}

/// The exact minimum, maximum, average, RMS, and peak-to-peak value of `vector` in run
/// `run_id` between `min_x` and `max_x`, where a missing bound means the end of the run,
/// and its value at each of `cursors`, all from the full-resolution samples.
#[tauri::command]
pub fn measure_trace(
    state: State<'_, SimState>,
    run_id: Uuid,
    vector: String,
    min_x: Option<f64>,
    max_x: Option<f64>,
    cursors: Vec<f64>,
) -> Result<TraceReadout, AppError> {
    state
        .lock()
        .measure_trace(run_id, &vector, min_x, max_x, &cursors)
}

/// What the probed item `target` measures in run `run_id`.
#[tauri::command]
pub fn resolve_probe(
    state: State<'_, SimState>,
    run_id: Uuid,
    target: ProbeTarget,
) -> Result<ProbeResult, AppError> {
    state.lock().resolve_probe(run_id, target)
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
            probes: ProbeMap::default(),
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
            probes: ProbeMap::default(),
        });
        assert!(inner.op_annotations(Uuid::nil()).unwrap().is_empty());
        let stale = inner.op_annotations(Uuid::from_u128(1)).unwrap_err();
        assert_eq!(stale.kind, AppErrorKind::SimNotFound);
    }

    fn transient(vectors: &[(&str, &[f64])]) -> SimInner {
        let plot = Plot {
            name: "tran1".to_owned(),
            scale: Some("time".to_owned()),
            vectors: vectors
                .iter()
                .map(|(name, values)| Vector {
                    name: (*name).to_owned(),
                    values: values.to_vec(),
                })
                .collect(),
        };
        SimInner {
            results: Some(Results {
                run_id: Uuid::nil(),
                plot,
                anchors: OpAnchors::default(),
                probes: ProbeMap::default(),
            }),
            ..SimInner::default()
        }
    }

    #[test]
    fn a_waveform_without_bounds_covers_the_whole_run() {
        let time: &[f64] = &[0.0, 1e-3, 2e-3];
        let inner = transient(&[("time", time), ("/out", &[0.0, 1.0, 4.0])]);
        let points = inner
            .waveform(Uuid::nil(), "/out", None, None, 100)
            .unwrap();
        let xs: Vec<f64> = points.iter().map(|point| point.x).collect();
        assert_eq!(xs, vec![0.0, 1e-3, 2e-3]);
        let missing = inner.waveform(Uuid::nil(), "/in", None, None, 100);
        assert_eq!(missing.unwrap_err().kind, AppErrorKind::SimNotFound);
    }

    #[test]
    fn a_trace_is_measured_over_its_span_with_a_value_at_each_cursor() {
        let time: &[f64] = &[0.0, 1e-3, 2e-3];
        let inner = transient(&[("time", time), ("/out", &[0.0, 2.0, 2.0])]);
        let nil = Uuid::nil();
        let readout = inner
            .measure_trace(nil, "/out", None, None, &[1e-3, 5e-3])
            .unwrap();
        let span = readout.span.unwrap();
        assert_eq!((span.from, span.to), (0.0, 2e-3));
        assert_eq!((span.min, span.max, span.peak_to_peak), (0.0, 2.0, 2.0));
        assert!((span.average - 1.5).abs() < 1e-12, "{}", span.average);
        assert_eq!(readout.cursors, vec![Some(2.0), None]);
        let json = serde_json::to_value(&readout).unwrap();
        assert_eq!(json["span"]["peakToPeak"], 2.0);
        assert_eq!(json["cursors"], serde_json::json!([2.0, null]));

        let outside = inner.measure_trace(nil, "/out", Some(1.0), None, &[]);
        assert_eq!(outside.unwrap().span, None);
        let missing = inner.measure_trace(nil, "/in", None, None, &[]);
        assert_eq!(missing.unwrap_err().kind, AppErrorKind::SimNotFound);
        let other = Uuid::from_u128(1);
        let stale = inner.measure_trace(other, "/out", None, None, &[]);
        assert_eq!(stale.unwrap_err().kind, AppErrorKind::SimNotFound);
    }

    #[test]
    fn a_probe_needs_the_current_run_and_a_simulated_item() {
        let inner = transient(&[("time", &[0.0])]);
        let target = ProbeTarget::Item {
            uuid: Uuid::from_u128(7),
        };
        let stale = inner.resolve_probe(Uuid::from_u128(1), target);
        assert_eq!(stale.unwrap_err().kind, AppErrorKind::SimNotFound);
        let outside = inner.resolve_probe(Uuid::nil(), target).unwrap();
        assert_eq!(outside, ProbeResult::NotSimulated);
    }

    #[test]
    fn probe_targets_and_results_match_the_ui_shapes() {
        let pin: ProbeTarget = serde_json::from_value(serde_json::json!({
            "kind": "pin",
            "symbol": "00000000-0000-0000-0000-000000000007",
            "position": [1_270_000, 2_540_000]
        }))
        .unwrap();
        assert_eq!(
            pin,
            ProbeTarget::Pin {
                symbol: Uuid::from_u128(7),
                position: Xy(1_270_000, 2_540_000),
            }
        );
        let result = ProbeResult::NoCurrent {
            reference: "R1".to_owned(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "noCurrent", "reference": "R1" })
        );
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
