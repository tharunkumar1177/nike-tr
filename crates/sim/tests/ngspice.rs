//! Runs real ngspice. CI downloads the pinned shared library and sets `TRON_NGSPICE` to it.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use common::{divider, low_pass, low_pass_ac, low_pass_sweep, low_pass_transient};
use tron_sim::{
    AcQuantity, Analysis, Converged, Convergence, Deck, DiagnosisKind, Engine, EngineError,
    EngineGuard, Plot, Transient, deck,
};

fn library() -> PathBuf {
    std::env::var_os("TRON_NGSPICE")
        .map(PathBuf::from)
        .expect("TRON_NGSPICE must name the ngspice shared library")
}

/// A copy of the archive's code models (in `lib/ngspice`, beside `dll-vs`) in a folder
/// whose name has a space, as under `Program Files`.
fn code_models() -> &'static Path {
    static MODELS: OnceLock<PathBuf> = OnceLock::new();
    MODELS.get_or_init(|| {
        let library = library();
        let archive = library
            .parent()
            .and_then(Path::parent)
            .expect("the library is inside the archive layout")
            .join("lib")
            .join("ngspice");
        let folder = format!("tron code models {}", std::process::id());
        let models = std::env::temp_dir().join(folder);
        std::fs::create_dir_all(&models).unwrap();
        for entry in std::fs::read_dir(&archive).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "cm") {
                let copy = models.join(path.file_name().unwrap());
                std::fs::copy(&path, copy).unwrap();
            }
        }
        models
    })
}

fn engine() -> EngineGuard {
    match Engine::lock(&library(), Some(code_models())) {
        Ok(engine) => engine,
        Err(error) => panic!("{error}"),
    }
}

fn run(engine: &mut Engine, deck: &Deck) -> Plot {
    match engine.run(deck) {
        Ok(plot) => plot,
        Err(error) => panic!("{error}"),
    }
}

/// Runs `deck` with the shunt retry, and panics if it fails.
fn run_convergent(engine: &mut Engine, deck: &Deck) -> Converged {
    match engine.run_convergent(deck, &AtomicBool::new(false), |_| {}) {
        Ok(converged) => converged,
        Err(error) => panic!("{error}"),
    }
}

/// SPICE names mapped to schematic names.
fn names(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|&(spice, name)| (spice.to_owned(), name.to_owned()))
        .collect()
}

/// A deck written by hand, with its node and device names.
fn hand_deck(lines: &[&str], nodes: &[(&str, &str)], devices: &[(&str, &str)]) -> Deck {
    Deck {
        lines: lines.iter().map(|line| (*line).to_owned()).collect(),
        nodes: names(nodes),
        devices: names(devices),
    }
}

fn divider_deck() -> Deck {
    let analysis = Analysis::OperatingPoint;
    deck(&divider().schematic(), Some(&analysis)).unwrap()
}

fn relative_error(actual: f64, expected: f64) -> f64 {
    ((actual - expected) / expected).abs()
}

#[test]
fn the_divider_operating_point_matches_the_analytic_value() {
    let plot = run(&mut engine(), &divider_deck());
    assert!(plot.name.starts_with("op"), "{}", plot.name);
    assert_eq!(plot.scale, None);

    let out = plot.vector("/OUT").expect("the plot has /out")[0];
    let expected = 5.0 * 4.7e3 / (10e3 + 4.7e3);
    assert!(relative_error(out, expected) < 5e-7, "{out} != {expected}");
    let vin = plot.vector("VIN").expect("the plot has vin")[0];
    assert!(relative_error(vin, 5.0) < 5e-7, "{vin}");
    let current = plot.vector("v1#branch").expect("the plot has v1#branch")[0];
    let expected_current = -5.0 / (10e3 + 4.7e3);
    assert!(
        relative_error(current, expected_current) < 5e-7,
        "{current} != {expected_current}"
    );
}

#[test]
fn the_low_pass_step_response_matches_the_analytic_curve() {
    let analysis = low_pass_transient();
    let deck = deck(&low_pass().schematic(), Some(&analysis)).unwrap();
    let plot = run(&mut engine(), &deck);
    assert!(plot.name.starts_with("tran"), "{}", plot.name);
    assert_eq!(plot.scale.as_deref(), Some("time"));

    let time = plot.vector("time").expect("the plot has time");
    let out = plot.vector("/OUT").expect("the plot has /out");
    assert_eq!(time.len(), out.len());
    assert!(time.len() >= 500, "{} points", time.len());
    let end = *time.last().unwrap();
    assert!(relative_error(end, 5e-3) < 1e-9, "the run ends at {end}");

    let tau = 1e3 * 1e-6;
    let mut worst = 0.0f64;
    for (&t, &v) in time.iter().zip(out) {
        let expected = 1.0 - (-t / tau).exp();
        worst = worst.max((v - expected).abs());
    }
    assert!(worst < 0.01, "largest error {worst} V");

    let samples = plot.waveform("/OUT", 0.0, 5e-3, 100).unwrap();
    assert!(samples.len() <= 100, "{} samples", samples.len());
    let last = samples.last().unwrap();
    let expected = 1.0 - (-5.0f64).exp();
    assert!((last.y - expected).abs() < 0.01, "{last:?}");
}

#[test]
fn the_low_pass_cutoff_matches_the_analytic_value() {
    let analysis = low_pass_sweep();
    let deck = deck(&low_pass_ac().schematic(), Some(&analysis)).unwrap();
    let plot = run(&mut engine(), &deck);
    assert!(plot.name.starts_with("ac"), "{}", plot.name);
    assert_eq!(plot.scale.as_deref(), Some("frequency"));
    let frequency = plot.vector("frequency").expect("the plot has frequency");
    assert_eq!(frequency.len(), 601);
    assert!(plot.find("frequency").unwrap().imaginary.is_none());

    let magnitude = plot.ac_series("/OUT", AcQuantity::MagnitudeDb).unwrap();
    let half_power = 20.0 * std::f64::consts::FRAC_1_SQRT_2.log10();
    let samples = magnitude.decimate(0.0, f64::INFINITY, 20_000);
    assert_eq!(samples.len(), 601);
    let crossing = samples
        .windows(2)
        .find(|pair| pair[0].y >= half_power && pair[1].y < half_power)
        .expect("the magnitude falls through -3 dB");
    // Between the samples either side, the magnitude is a straight line in log frequency.
    let (a, b) = (crossing[0], crossing[1]);
    let (x0, x1) = (a.x.log10(), b.x.log10());
    let cutoff = 10f64.powf(x0 + (x1 - x0) * (half_power - a.y) / (b.y - a.y));
    let expected = 1.0 / (std::f64::consts::TAU * 1e3 * 1e-6);
    assert!(
        relative_error(cutoff, expected) < 0.01,
        "{cutoff} != {expected}"
    );

    let low = magnitude.value_at(1.0).unwrap();
    assert!(low.abs() < 1e-3, "{low} dB at 1 Hz");
    let phase = plot.ac_series("/OUT", AcQuantity::PhaseDeg).unwrap();
    let at_cutoff = phase.value_at(expected).unwrap();
    assert!((at_cutoff + 45.0).abs() < 0.5, "{at_cutoff}");
    let last = *frequency.last().unwrap();
    assert!(relative_error(last, 1e6) < 1e-6, "the sweep ends at {last}");
    let high = phase.value_at(last).unwrap();
    assert!((high + 90.0).abs() < 0.1, "{high}° at 1 MHz");
}

#[test]
fn an_xspice_code_model_runs_from_a_path_with_a_space() {
    let mut engine = engine();
    let lines = [
        "* Tron simulation",
        "V1 in 0 DC 2",
        "A1 in out amp",
        ".model amp gain(gain=2.5)",
        "R1 out 0 1e3",
        ".op",
        ".end",
    ];
    let gain = Deck {
        lines: Vec::from(lines.map(str::to_owned)),
        nodes: Default::default(),
        devices: Default::default(),
    };
    let plot = run(&mut engine, &gain);
    let out = plot.vector("out").expect("the plot has out")[0];
    assert!(relative_error(out, 5.0) < 1e-9, "{out}");
}

#[test]
fn a_long_transient_can_be_cancelled_while_it_runs() {
    let analysis = Analysis::Transient(Transient {
        step: 1e-5,
        stop: 2.0,
        start: 0.0,
        max_step: Some(1e-7),
        initial_conditions: true,
    });
    let deck = deck(&low_pass().schematic(), Some(&analysis)).unwrap();
    let mut engine = engine();
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, progress) = mpsc::channel();
    let canceller = {
        let cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            let _ = progress.recv_timeout(Duration::from_secs(3));
            cancel.store(true, Ordering::SeqCst);
            Engine::halt();
        })
    };
    let started = Instant::now();
    let result = engine.run_cancellable(&deck, &cancel, move |percent| {
        let _ = sender.send(percent);
    });
    let elapsed = started.elapsed();
    // Joined while this test still holds the engine, so the halt can't reach another run.
    canceller.join().unwrap();
    assert_eq!(result.unwrap_err(), EngineError::Cancelled);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");

    let plot = run(&mut engine, &divider_deck());
    assert!(plot.vector("/OUT").is_some());
}

#[test]
fn a_circuit_that_converges_directly_says_so() {
    let converged = run_convergent(&mut engine(), &divider_deck());
    assert_eq!(converged.convergence, Convergence::Direct);
    assert_eq!(converged.diagnosis, None);
    assert!(converged.plot.vector("/OUT").is_some());
}

#[test]
fn a_floating_capacitor_node_converges_with_stepping() {
    let lines = [
        "* Tron simulation",
        "V1 a 0 DC 5",
        "R1 a 0 1e3",
        "C1 a b 1e-6",
        ".op",
        ".end",
    ];
    let nodes = [("a", "IN"), ("b", "FLOAT")];
    let devices = [("V1", "V1"), ("R1", "R1"), ("C1", "C1")];
    let floating = hand_deck(&lines, &nodes, &devices);
    let mut engine = engine();
    let converged = run_convergent(&mut engine, &floating);
    let output = engine.output().join("\n");
    assert_eq!(converged.convergence, Convergence::Stepped, "{output}");
    assert_eq!(converged.diagnosis, None);
    let input = converged.plot.vector("a").expect("the plot has a")[0];
    assert!(relative_error(input, 5.0) < 1e-6, "{input}");
}

#[test]
fn a_floating_resistor_without_stepping_is_rescued_by_a_shunt() {
    let lines = [
        "* Tron simulation",
        "V1 a 0 DC 5",
        "R1 a 0 1e3",
        "R2 b c 1e3",
        ".options gminsteps=0 srcsteps=0",
        ".op",
        ".end",
    ];
    let nodes = [("a", "IN"), ("b", "/B"), ("c", "/C")];
    let devices = [("V1", "V1"), ("R1", "R1"), ("R2", "R2")];
    let island = hand_deck(&lines, &nodes, &devices);
    let mut engine = engine();
    let converged = run_convergent(&mut engine, &island);
    let output = engine.output().join("\n");
    let convergence = converged.convergence;
    assert_eq!(convergence, Convergence::RshuntRescue, "{output}");
    let diagnosis = converged.diagnosis.expect("the failure is kept");
    assert_eq!(diagnosis.reference(), Some("R2"), "{diagnosis:?}");
    let input = converged.plot.vector("a").expect("the plot has a")[0];
    assert!(relative_error(input, 5.0) < 1e-6, "{input}");

    let plot = run(&mut engine, &divider_deck());
    assert!(plot.vector("/OUT").is_some());
}

#[test]
fn a_loop_of_voltage_sources_stays_a_convergence_error() {
    let lines = [
        "* Tron simulation",
        "V1 a 0 DC 5",
        "V2 a 0 DC 3",
        "R1 a 0 1e3",
        ".op",
        ".end",
    ];
    let devices = [("V1", "V1"), ("V2", "V2"), ("R1", "R1")];
    let shorted = hand_deck(&lines, &[("a", "IN")], &devices);
    let mut engine = engine();
    let cancel = AtomicBool::new(false);
    let result = engine.run_convergent(&shorted, &cancel, |_| {});
    let error = result.unwrap_err();
    let output = engine.output().join("\n");
    let EngineError::Convergence(diagnosis) = &error else {
        panic!("unexpected error: {error}\n{output}");
    };
    assert_eq!(
        diagnosis.kind,
        DiagnosisKind::TimestepTooSmall,
        "{diagnosis:?}"
    );
    assert_eq!(diagnosis.reference(), Some("V1"), "{diagnosis:?}");
    assert_eq!(
        error.to_string(),
        "Transient step too small at the initial timepoint in v1 (V1)."
    );

    let plot = run(&mut engine, &divider_deck());
    assert!(plot.vector("/OUT").is_some());
}

#[test]
fn a_rejected_circuit_reports_ngspice_errors_and_the_engine_recovers() {
    let lines = [
        "* Tron simulation",
        "R1 a 0 1e3",
        "X1 a 0 nosuch_subcircuit",
        ".op",
        ".end",
    ];
    let broken = Deck {
        lines: Vec::from(lines.map(str::to_owned)),
        nodes: Default::default(),
        devices: Default::default(),
    };
    let mut engine = engine();
    let error = engine.run(&broken).unwrap_err();
    let (EngineError::Circuit(lines) | EngineError::Analysis(lines)) = &error else {
        panic!("unexpected error: {error}");
    };
    assert!(!lines.is_empty(), "ngspice explained the failure");
    assert!(error.to_string().contains("nosuch_subcircuit"), "{error}");

    let plot = run(&mut engine, &divider_deck());
    assert!(plot.vector("/OUT").is_some());
}
