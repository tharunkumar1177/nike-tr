use tron_core::kicad::Schematic;
use tron_sim::{Analysis, Deck, PreflightError, Transient, deck};

const FONT: &str = "(effects (font (size 1.27 1.27)))";

fn pin(electrical: &str, y: f64, angle: u32, name: &str, number: &str, hidden: bool) -> String {
    let hide = if hidden { " (hide yes)" } else { "" };
    format!(
        "(pin {electrical} line (at 0 {y} {angle}) (length 1.27){hide} \
         (name \"{name}\" {FONT}) (number \"{number}\" {FONT}))"
    )
}

/// A two-pin definition with pin 1 on top, as in KiCad's `Device` and `Simulation_SPICE`
/// libraries. `half` is the distance from the symbol origin to each pin end.
fn two_pin(lib_id: &str, prefix: &str, half: f64) -> String {
    let name = lib_id.split_once(':').unwrap().1;
    format!(
        "(symbol \"{lib_id}\" (exclude_from_sim no) (in_bom yes) (on_board yes) \
         (property \"Reference\" \"{prefix}\" (at 2.032 0 90) {FONT}) \
         (property \"Value\" \"{name}\" (at 0 0 90) {FONT}) \
         (symbol \"{name}_1_1\" {} {}))",
        pin("passive", half, 270, "", "1", false),
        pin("passive", -half, 90, "", "2", false)
    )
}

fn definitions() -> String {
    let ground = format!(
        "(symbol \"power:GND\" (power) (exclude_from_sim no) (in_bom yes) (on_board yes) \
         (property \"Reference\" \"#PWR\" (at 0 -6.35 0) (hide yes) {FONT}) \
         (property \"Value\" \"GND\" (at 0 -3.81 0) {FONT}) \
         (symbol \"GND_1_1\" {}))",
        pin("power_in", 0.0, 270, "GND", "1", true)
    );
    [
        two_pin("Device:R", "R", 3.81),
        two_pin("Device:C", "C", 3.81),
        two_pin("Simulation_SPICE:VDC", "V", 5.08),
        ground,
    ]
    .join(" ")
}

fn property(key: &str, value: &str, (x, y): (f64, f64)) -> String {
    format!("(property \"{key}\" \"{value}\" (at {x} {y} 0) {FONT})")
}

struct Part {
    lib_id: &'static str,
    reference: &'static str,
    value: &'static str,
    at: (f64, f64),
    fields: Vec<(&'static str, &'static str)>,
    excluded: bool,
}

fn part(
    lib_id: &'static str,
    reference: &'static str,
    value: &'static str,
    at: (f64, f64),
) -> Part {
    Part {
        lib_id,
        reference,
        value,
        at,
        fields: Vec::new(),
        excluded: false,
    }
}

impl Part {
    fn field(mut self, key: &'static str, value: &'static str) -> Part {
        self.fields.push((key, value));
        self
    }

    fn excluded(mut self) -> Part {
        self.excluded = true;
        self
    }
}

enum Label {
    Local(&'static str, (f64, f64)),
    Global(&'static str, (f64, f64)),
}

struct Circuit {
    parts: Vec<Part>,
    wires: Vec<[(f64, f64); 2]>,
    labels: Vec<Label>,
}

impl Circuit {
    fn schematic(&self) -> Schematic {
        let mut serial = 0u32;
        let mut uuid = || {
            serial += 1;
            format!("00000000-0000-4000-8000-{serial:012x}")
        };
        let mut items = Vec::new();
        for [(x1, y1), (x2, y2)] in &self.wires {
            items.push(format!(
                "(wire (pts (xy {x1} {y1}) (xy {x2} {y2})) (stroke (width 0) (type default)) \
                 (uuid \"{}\"))",
                uuid()
            ));
        }
        for label in &self.labels {
            items.push(match label {
                Label::Local(text, (x, y)) => format!(
                    "(label \"{text}\" (at {x} {y} 0) {FONT} (uuid \"{}\"))",
                    uuid()
                ),
                Label::Global(text, (x, y)) => format!(
                    "(global_label \"{text}\" (shape input) (at {x} {y} 0) {FONT} \
                     (uuid \"{}\"))",
                    uuid()
                ),
            });
        }
        for part in &self.parts {
            let (x, y) = part.at;
            let exclude = if part.excluded { "yes" } else { "no" };
            let mut properties = vec![
                property("Reference", part.reference, part.at),
                property("Value", part.value, part.at),
            ];
            for (key, value) in &part.fields {
                properties.push(property(key, value, part.at));
            }
            items.push(format!(
                "(symbol (lib_id \"{}\") (at {x} {y} 0) (unit 1) (exclude_from_sim {exclude}) \
                 (in_bom yes) (on_board yes) (dnp no) (uuid \"{}\") {} \
                 (pin \"1\" (uuid \"{}\")) (pin \"2\" (uuid \"{}\")))",
                part.lib_id,
                uuid(),
                properties.join(" "),
                uuid(),
                uuid()
            ));
        }
        let source = format!(
            "(kicad_sch (version 20231120) (generator \"eeschema\") (generator_version \"10.0\") \
             (uuid \"6d0e1a52-3c8f-4b47-9a1e-2f8a6d3c7e21\") (paper \"A4\") \
             (lib_symbols {}) {} (sheet_instances (path \"/\" (page \"1\"))))",
            definitions(),
            items.join(" ")
        );
        Schematic::parse(&source).unwrap()
    }
}

/// A DC voltage source with the fields KiCad's `Simulation_SPICE:VDC` carries.
fn source(reference: &'static str, at: (f64, f64), params: &'static str) -> Part {
    part("Simulation_SPICE:VDC", reference, "VDC", at)
        .field("Sim.Device", "V")
        .field("Sim.Type", "DC")
        .field("Sim.Pins", "1=+ 2=-")
        .field("Sim.Params", params)
}

/// A 5 V source across R1 (10k) over R2 (4k7). The source's top joins R1 at the global
/// label VIN, the resistors meet at the local label OUT, and both bottoms are grounded.
fn divider() -> Circuit {
    Circuit {
        parts: vec![
            source("V1", (50.8, 63.5), "dc=5"),
            part("Device:R", "R1", "10k", (76.2, 57.15)),
            part("Device:R", "R2", "4k7", (76.2, 72.39)),
            part("power:GND", "#PWR01", "GND", (50.8, 81.28)),
            part("power:GND", "#PWR02", "GND", (76.2, 81.28)),
        ],
        wires: vec![
            [(50.8, 58.42), (50.8, 53.34)],
            [(50.8, 53.34), (76.2, 53.34)],
            [(76.2, 60.96), (76.2, 64.77)],
            [(76.2, 64.77), (76.2, 68.58)],
            [(50.8, 68.58), (50.8, 81.28)],
            [(76.2, 76.2), (76.2, 81.28)],
        ],
        labels: vec![
            Label::Global("VIN", (50.8, 53.34)),
            Label::Local("OUT", (76.2, 64.77)),
        ],
    }
}

/// A 1 V step into R1 (1k) and C1 (1u) to ground: a 1 ms time constant.
fn low_pass() -> Circuit {
    Circuit {
        parts: vec![
            source("V1", (50.8, 63.5), "dc=1"),
            part("Device:R", "R1", "1k", (76.2, 57.15)),
            part("Device:C", "C1", "1u", (76.2, 72.39)),
            part("power:GND", "#PWR01", "GND", (50.8, 81.28)),
            part("power:GND", "#PWR02", "GND", (76.2, 81.28)),
        ],
        ..divider()
    }
}

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
    let analysis = Analysis::Transient(Transient {
        step: 1e-5,
        stop: 5e-3,
        start: 0.0,
        max_step: Some(1e-5),
        initial_conditions: true,
    });
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
        Label::Global("V IN", (50.8, 53.34)),
        Label::Global("V_IN", (76.2, 64.77)),
    ];
    let deck = operating_point(&circuit).unwrap();
    assert_eq!(deck.lines[2], "R1 V_IN V_IN_2 1e4");
    assert_eq!(deck.nodes["V_IN"], "V IN");
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
