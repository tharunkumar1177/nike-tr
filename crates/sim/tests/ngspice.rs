//! Runs real ngspice. CI downloads the pinned shared library and sets `TRON_NGSPICE` to it.

mod common;

use std::path::PathBuf;

use common::{divider, low_pass, low_pass_transient};
use tron_sim::{Analysis, Deck, Engine, EngineError, EngineGuard, Plot, deck};

fn engine() -> EngineGuard {
    let path = std::env::var_os("TRON_NGSPICE")
        .map(PathBuf::from)
        .expect("TRON_NGSPICE must name the ngspice shared library");
    match Engine::lock(&path) {
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
