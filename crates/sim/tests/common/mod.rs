//! KiCad-format schematics for the simulation tests, built from parts, wires, and labels.

#![allow(dead_code)]

use tron_core::AcSweep;
use tron_core::kicad::Schematic;
use tron_sim::{Ac, Analysis, Transient};

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

pub struct Part {
    pub lib_id: &'static str,
    pub reference: &'static str,
    pub value: &'static str,
    pub at: (f64, f64),
    pub fields: Vec<(&'static str, &'static str)>,
    pub excluded: bool,
}

pub fn part(
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
    pub fn field(mut self, key: &'static str, value: &'static str) -> Part {
        self.fields.push((key, value));
        self
    }

    pub fn excluded(mut self) -> Part {
        self.excluded = true;
        self
    }
}

pub enum Label {
    Local(&'static str, (f64, f64)),
    Global(&'static str, (f64, f64)),
}

pub struct Circuit {
    pub parts: Vec<Part>,
    pub wires: Vec<[(f64, f64); 2]>,
    pub labels: Vec<Label>,
}

impl Circuit {
    pub fn schematic(&self) -> Schematic {
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
pub fn source(reference: &'static str, at: (f64, f64), params: &'static str) -> Part {
    part("Simulation_SPICE:VDC", reference, "VDC", at)
        .field("Sim.Device", "V")
        .field("Sim.Type", "DC")
        .field("Sim.Pins", "1=+ 2=-")
        .field("Sim.Params", params)
}

/// A 5 V source across R1 (10k) over R2 (4k7). The source's top joins R1 at the global
/// label VIN, the resistors meet at the local label OUT, and both bottoms are grounded.
pub fn divider() -> Circuit {
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
pub fn low_pass() -> Circuit {
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

/// The low-pass driven by a 1 V AC source with no DC part: a cutoff of 1/(2π RC).
pub fn low_pass_ac() -> Circuit {
    let mut circuit = low_pass();
    circuit.parts[0] = source("V1", (50.8, 63.5), "dc=0 ac=1");
    circuit
}

/// 1 Hz to 1 MHz, a hundred points per decade.
pub fn low_pass_sweep() -> Analysis {
    Analysis::Ac(Ac {
        sweep: AcSweep::Decade,
        points: 100,
        start: 1.0,
        stop: 1e6,
    })
}

/// Five time constants of the low-pass, from zero initial conditions, in 10 µs steps.
pub fn low_pass_transient() -> Analysis {
    Analysis::Transient(Transient {
        step: 1e-5,
        stop: 5e-3,
        start: 0.0,
        max_step: Some(1e-5),
        initial_conditions: true,
    })
}
