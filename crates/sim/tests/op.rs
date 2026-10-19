//! Where operating-point results are shown, for schematics built in `common`.

mod common;

use common::{Circuit, divider};
use tron_core::{Nm, Point};
use tron_sim::{Analysis, OpKind, OpValue, Plot, Vector, deck, op_anchors};

fn at(x_mm: f64, y_mm: f64) -> Point {
    Point {
        x: Nm((x_mm * 1e6).round() as i64),
        y: Nm((y_mm * 1e6).round() as i64),
    }
}

fn op_plot(vectors: &[(&str, f64)]) -> Plot {
    Plot {
        name: "op1".to_owned(),
        scale: None,
        vectors: vectors
            .iter()
            .map(|(name, value)| Vector {
                name: (*name).to_owned(),
                values: vec![*value],
            })
            .collect(),
    }
}

/// The vector, kind, label, position, and value of one annotation.
type Row = (String, OpKind, String, Point, f64);

fn row(vector: &str, kind: OpKind, label: &str, position: Point, value: f64) -> Row {
    (vector.to_owned(), kind, label.to_owned(), position, value)
}

fn values_for(circuit: &Circuit, plot: &Plot) -> Vec<Row> {
    let schematic = circuit.schematic();
    let deck = deck(&schematic, Some(&Analysis::OperatingPoint)).unwrap();
    let anchors = op_anchors(&schematic, &deck).unwrap();
    let values = anchors.values(plot);
    values
        .into_iter()
        .map(|OpValue { vector, anchor, value }| {
            (vector, anchor.kind, anchor.label, anchor.position, value)
        })
        .collect()
}

#[test]
fn voltages_sit_at_net_labels_and_currents_at_the_first_pin() {
    let plot = op_plot(&[("vin", 5.0), ("/out", 1.6), ("v1#branch", -3e-4)]);
    let values = values_for(&divider(), &plot);
    let vin = row("vin", OpKind::Voltage, "VIN", at(50.8, 53.34), 5.0);
    let out = row("/out", OpKind::Voltage, "/OUT", at(76.2, 64.77), 1.6);
    let current = row("v1#branch", OpKind::Current, "V1", at(50.8, 58.42), -3e-4);
    assert_eq!(values, vec![vin, out, current]);
}

#[test]
fn an_unlabelled_net_uses_the_middle_of_its_longest_wire() {
    let circuit = Circuit {
        labels: Vec::new(),
        ..divider()
    };
    let schematic = circuit.schematic();
    let deck = deck(&schematic, Some(&Analysis::OperatingPoint)).unwrap();
    let names: Vec<&String> = deck.nodes.keys().collect();
    let plot = op_plot(&[(names[0].as_str(), 5.0), (names[1].as_str(), 1.6)]);
    let mut positions: Vec<Point> = values_for(&circuit, &plot)
        .into_iter()
        .map(|(_, _, _, position, _)| position)
        .collect();
    positions.sort_by_key(|point| (point.x, point.y));
    // The source's top net runs 25.4 mm across the top; the divider's middle net has two
    // 3.81 mm wires, and the first in the file wins.
    assert_eq!(positions, vec![at(63.5, 53.34), at(76.2, 62.865)]);
}

#[test]
fn ground_and_unknown_vectors_have_no_annotation() {
    let plot = op_plot(&[("0", 0.0), ("internal#node", 1.0), ("@r1[i]", 2.0)]);
    assert!(values_for(&divider(), &plot).is_empty());
}
