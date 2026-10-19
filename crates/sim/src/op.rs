//! Where each operating-point result is shown on the schematic.

use std::collections::{BTreeMap, HashMap, HashSet};

use tron_core::kicad::Schematic;
use tron_core::{LabelKind, Nm, Point, Wire, netlist};

use crate::deck::Deck;
use crate::error::PreflightError;
use crate::ngspice::Plot;

/// What an operating-point value measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// A node voltage to ground, in volts.
    Voltage,
    /// The current through a device, in amperes.
    Current,
}

/// Where one deck node or device is annotated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpAnchor {
    pub kind: OpKind,
    /// The net name for a voltage, the symbol reference for a current.
    pub label: String,
    pub position: Point,
}

/// The anchors of a deck, keyed by lower-case SPICE name, as ngspice names its results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpAnchors {
    nodes: BTreeMap<String, OpAnchor>,
    devices: BTreeMap<String, OpAnchor>,
}

/// One operating-point value and where it is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct OpValue {
    /// The ngspice vector, such as `/out` or `v1#branch`.
    pub vector: String,
    pub anchor: OpAnchor,
    pub value: f64,
}

/// Finds where each node voltage and device current of `deck` is shown. A voltage sits at
/// its net's first label, or else at the middle of the longest wire on the net, or else at
/// the net's first pin. A current sits at its device's first pin. Ground has no anchor.
pub fn op_anchors(schematic: &Schematic, deck: &Deck) -> Result<OpAnchors, PreflightError> {
    let nets = netlist(schematic).map_err(PreflightError::Netlist)?;
    let pins = pin_positions(schematic);
    let labels = schematic.labels();
    let wires = schematic.wires();
    let mut anchors = OpAnchors::default();
    for (node, net_name) in &deck.nodes {
        let Some(net) = nets.iter().find(|net| &net.name == net_name) else {
            continue;
        };
        let net_pins: Vec<Point> = net
            .nodes
            .iter()
            .filter_map(|pin| pin_position(&pins, &pin.reference, &pin.pin))
            .collect();
        let label = labels.iter().find(|label| match label.kind {
            LabelKind::Local => net_name.strip_prefix('/') == Some(label.text.as_str()),
            LabelKind::Global(_) => *net_name == label.text,
        });
        let position = match label {
            Some(label) => Some(label.position),
            None => {
                let middle = longest_wire_middle(&wires, &net_pins);
                middle.or_else(|| net_pins.first().copied())
            }
        };
        if let Some(position) = position {
            let anchor = OpAnchor {
                kind: OpKind::Voltage,
                label: net_name.clone(),
                position,
            };
            anchors.nodes.insert(node.to_ascii_lowercase(), anchor);
        }
    }
    for (device, reference) in &deck.devices {
        let first = pins.get(reference.as_str()).and_then(|list| list.first());
        if let Some((_, position)) = first {
            let anchor = OpAnchor {
                kind: OpKind::Current,
                label: reference.clone(),
                position: *position,
            };
            anchors.devices.insert(device.to_ascii_lowercase(), anchor);
        }
    }
    Ok(anchors)
}

impl OpAnchors {
    /// Pairs each operating-point vector in `plot` with its anchor, in vector order.
    /// Vectors with no anchor, such as internal nodes, are left out.
    pub fn values(&self, plot: &Plot) -> Vec<OpValue> {
        let mut values = Vec::new();
        for vector in &plot.vectors {
            let value = vector.values.first();
            let anchor = self.anchor(&vector.name);
            if let (Some(&value), Some(anchor)) = (value, anchor) {
                values.push(OpValue {
                    vector: vector.name.clone(),
                    anchor: anchor.clone(),
                    value,
                });
            }
        }
        values
    }

    fn anchor(&self, vector: &str) -> Option<&OpAnchor> {
        let vector = vector.to_ascii_lowercase();
        match vector.strip_suffix("#branch") {
            Some(device) => self.devices.get(device),
            None => self.nodes.get(&vector),
        }
    }
}

/// Every symbol's pins by reference, in definition order, across all its units.
fn pin_positions(schematic: &Schematic) -> HashMap<String, Vec<(String, Point)>> {
    let mut pins: HashMap<String, Vec<(String, Point)>> = HashMap::new();
    for symbol in schematic.symbols() {
        let Some(definition) = schematic.lib_symbol(&symbol.lib_id) else {
            continue;
        };
        let Ok(connections) = symbol.pin_connections(definition) else {
            continue;
        };
        let entry = pins.entry(symbol.reference().to_owned()).or_default();
        for pin in connections {
            entry.push((pin.number, pin.position));
        }
    }
    pins
}

fn pin_position(
    pins: &HashMap<String, Vec<(String, Point)>>,
    reference: &str,
    number: &str,
) -> Option<Point> {
    let pins = pins.get(reference)?;
    let found = pins.iter().find(|(pin, _)| pin == number);
    found.map(|(_, position)| *position)
}

/// The middle of the longest wire reachable from `start` through shared wire ends. Ties go
/// to the first wire in the file.
fn longest_wire_middle(wires: &[Wire], start: &[Point]) -> Option<Point> {
    let mut points: HashSet<Point> = start.iter().copied().collect();
    let mut reached = vec![false; wires.len()];
    let mut grew = true;
    while grew {
        grew = false;
        for (index, wire) in wires.iter().enumerate() {
            let touches = points.contains(&wire.start) || points.contains(&wire.end);
            if reached[index] || !touches {
                continue;
            }
            reached[index] = true;
            points.insert(wire.start);
            points.insert(wire.end);
            grew = true;
        }
    }
    let mut longest: Option<(i128, &Wire)> = None;
    for (index, wire) in wires.iter().enumerate() {
        if !reached[index] {
            continue;
        }
        let length = squared_length(wire);
        if longest.is_none_or(|(best, _)| length > best) {
            longest = Some((length, wire));
        }
    }
    longest.map(|(_, wire)| Point {
        x: Nm((wire.start.x.0 + wire.end.x.0) / 2),
        y: Nm((wire.start.y.0 + wire.end.y.0) / 2),
    })
}

fn squared_length(wire: &Wire) -> i128 {
    let dx = i128::from(wire.end.x.0 - wire.start.x.0);
    let dy = i128::from(wire.end.y.0 - wire.start.y.0);
    dx * dx + dy * dy
}
