//! Runs real ngspice. CI downloads the pinned shared library and sets `TRON_NGSPICE` to it.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use common::{divider, low_pass, low_pass_transient};
use tron_sim::{Analysis, Deck, Engine, EngineError, EngineGuard, Plot, Transient, deck};

fn library() -> PathBuf {
    std::env::var_os("TRON_NGSPICE")
        .map(PathBuf::from)
        .expect("TRON_NGSPICE must name the ngspice shared library")
}

fn engine() -> EngineGuard {
    match Engine::lock(&library()) {
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
fn code_models_load_from_an_absolute_path_without_spinit() {
    // The archive keeps the code models in `lib/ngspice`, beside `dll-vs`. The copy goes
    // to a folder with a space, as under `Program Files`.
    let library = library();
    let analog = library
        .parent()
        .and_then(|dir| dir.parent())
        .expect("the library is inside the archive layout")
        .join("lib")
        .join("ngspice")
        .join("analog.cm");
    let folder = format!("tron code models {}", std::process::id());
    let models = std::env::temp_dir().join(folder);
    std::fs::create_dir_all(&models).unwrap();
    std::fs::copy(&analog, models.join("analog.cm")).unwrap();
    let mut engine = engine();
    for _ in 0..2 {
        if let Err(error) = engine.load_code_models(&models) {
            panic!("{error}");
        }
    }
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
