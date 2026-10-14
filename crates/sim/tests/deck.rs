mod common;

use common::{Circuit, Label, divider, low_pass, low_pass_transient, part};
use tron_sim::{Analysis, Deck, PreflightError, Transient, deck};

fn operating_point(circuit: &Circuit) -> Result<Deck, PreflightError> {
    deck(&circuit.schematic(), Some(&Analysis::OperatingPoint))
}

fn error(circuit: &Circuit) -> PreflightError {
    operating_point(circuit).unwrap_err()
}

#[test]
fn the_divider_deck_names_nodes_after_its_nets() {
    let deck = operating_point(&divider()).unwrap();
    assert_eq!(
        deck.text(),
        "* Tron simulation\n\
         V1 VIN 0 DC 5e0\n\
         R1 VIN /OUT 1e4\n\
         R2 /OUT 0 4.7e3\n\
         .op\n\
         .end\n"
    );
    let nodes: Vec<(&str, &str)> = deck
        .nodes
        .iter()
        .map(|(node, net)| (node.as_str(), net.as_str()))
        .collect();
    assert_eq!(nodes, [("/OUT", "/OUT"), ("VIN", "VIN")]);
    assert_eq!(deck.devices["R2"], "R2");
}

#[test]
fn the_low_pass_deck_runs_a_transient_from_initial_conditions() {
    let analysis = low_pass_transient();
    let deck = deck(&low_pass().schematic(), Some(&analysis)).unwrap();
    assert_eq!(
        deck.lines,
        [
            "* Tron simulation",
            "V1 VIN 0 DC 1e0",
            "R1 VIN /OUT 1e3",
            "C1 /OUT 0 1e-6",
            ".tran 1e-5 5e-3 0e0 1e-5 uic",
            ".end",
        ]
    );
}

#[test]
fn unsafe_net_names_become_distinct_nodes() {
    let mut circuit = divider();
    circuit.labels = vec![
        Label::Global("V.IN", (50.8, 53.34)),
        Label::Global("V_IN", (76.2, 64.77)),
    ];
    let deck = operating_point(&circuit).unwrap();
    assert_eq!(deck.lines[2], "R1 V_IN V_IN_2 1e4");
    assert_eq!(deck.nodes["V_IN"], "V.IN");
    assert_eq!(deck.nodes["V_IN_2"], "V_IN");
}

#[test]
fn excluded_symbols_are_left_out() {
    let mut circuit = divider();
    let excluded = part("Device:R", "U1", "LM358", (101.6, 57.15)).excluded();
    let mut disabled = part("Device:R", "TP1", "probe", (101.6, 72.39));
    disabled.fields.push(("Sim.Enable", "0"));
    circuit.parts.extend([excluded, disabled]);
    let deck = operating_point(&circuit).unwrap();
    assert_eq!(deck.devices.len(), 3);
}

#[test]
fn a_missing_or_invalid_analysis_is_reported_first() {
    let schematic = divider().schematic();
    assert_eq!(deck(&schematic, None), Err(PreflightError::NoAnalysis));
    let backwards = Analysis::Transient(Transient {
        step: 1e-3,
        stop: 1e-6,
        start: 0.0,
        max_step: None,
        initial_conditions: false,
    });
    let result = deck(&schematic, Some(&backwards));
    let Err(PreflightError::InvalidAnalysis(message)) = result else {
        panic!("a step longer than the run is rejected");
    };
    assert!(message.contains("step"), "{message}");
}

#[test]
fn a_circuit_without_ground_is_rejected() {
    let mut circuit = divider();
    circuit.parts.retain(|part| part.lib_id != "power:GND");
    assert_eq!(error(&circuit), PreflightError::NoGround);
}

#[test]
fn a_symbol_without_a_model_is_named() {
    let mut circuit = divider();
    let unmapped = part("Device:R", "U1", "LM358", (101.6, 57.15));
    circuit.parts.push(unmapped);
    let PreflightError::NoModel { reference, reason } = error(&circuit) else {
        panic!("U1 has no model");
    };
    assert_eq!(reference, "U1");
    assert!(reason.contains("no simulation model"), "{reason}");
    let shown = PreflightError::NoModel { reference, reason }.to_string();
    assert!(shown.starts_with("U1: "), "{shown}");
}

#[test]
fn references_must_be_annotated_and_unique() {
    let mut unannotated = divider();
    unannotated.parts[1].reference = "R?";
    assert_eq!(
        error(&unannotated),
        PreflightError::Unannotated("R?".to_owned())
    );

    let mut duplicated = divider();
    duplicated.parts[2].reference = "R1";
    assert_eq!(
        error(&duplicated),
        PreflightError::DuplicateReference("R1".to_owned())
    );
}

#[test]
fn a_schematic_with_only_power_symbols_has_nothing_to_simulate() {
    let mut circuit = divider();
    circuit.parts.retain(|part| part.lib_id == "power:GND");
    assert_eq!(error(&circuit), PreflightError::Empty);
}
