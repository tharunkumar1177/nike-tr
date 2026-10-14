use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use tron_core::kicad::Schematic;
use tron_core::{LabelKind, Nm, Placement, Point, Project, Session, Severity, Violation, erc};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/wiring/divider.kicad_sch");

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm::from_mm(x).unwrap(), Nm::from_mm(y).unwrap())
}

fn counter() -> impl FnMut() -> Uuid {
    let mut next = 0xe000_0000_0000_4000_8000_0000_0000_0000u128;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

fn golden_session(parent: &Path) -> Session {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("divider.kicad_sch"), GOLDEN).unwrap();
    Session::new(Project::load(&dir).unwrap())
}

fn rule_counts(violations: &[Violation]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for violation in violations {
        *counts.entry(violation.rule).or_default() += 1;
    }
    counts
}

#[test]
fn the_golden_divider_reports_kicads_violations() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    let violations = erc::check(&schematic).unwrap();
    let found: BTreeSet<(&str, Severity, Vec<String>)> = violations
        .iter()
        .map(|v| (v.rule, v.severity, v.items.clone()))
        .collect();
    let power = |reference: &str| {
        let pin = format!("{reference} pin 1 [Power input]");
        ("power_pin_not_driven", Severity::Error, vec![pin])
    };
    let unconnected = vec!["R3 pin 2 [Passive]".to_owned()];
    let label = vec!["Global label VIN".to_owned()];
    let expected = BTreeSet::from([
        power("#PWR01"),
        power("#PWR02"),
        ("pin_not_connected", Severity::Error, unconnected),
        ("isolated_pin_label", Severity::Warning, label),
    ]);
    assert_eq!(found, expected, "{violations:#?}");

    let free = violations
        .iter()
        .find(|v| v.rule == "pin_not_connected")
        .unwrap();
    assert_eq!(free.position, mm(127.0, 64.77));
    assert_eq!(free.message, "Pin not connected");
    assert!(free.id.starts_with("pin_not_connected:") && free.id.ends_with(":2"));
    let severities: Vec<Severity> = violations.iter().map(|v| v.severity).collect();
    assert_eq!(severities.last(), Some(&Severity::Warning));
    assert_eq!(erc::check(&schematic).unwrap(), violations);
}

#[test]
fn edits_add_and_clear_violations() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    let mut fresh = counter();

    session
        .place_no_connect(mm(127.0, 64.77), &mut fresh)
        .unwrap();
    let flagged = session.erc().unwrap();
    assert!(!rule_counts(&flagged).contains_key("pin_not_connected"));

    session
        .place_no_connect(mm(10.16, 10.16), &mut fresh)
        .unwrap();
    let loose = [mm(20.32, 20.32), mm(30.48, 20.32)];
    session.draw_wire(&loose, &mut fresh).unwrap();
    session
        .place_label(LabelKind::Local, "LOOSE", mm(40.64, 40.64), 0, &mut fresh)
        .unwrap();
    let schematic = session.project().schematic();
    let definition = schematic.lib_symbol("Device:R").unwrap().clone();
    let resistor = fresh();
    let pins = [fresh(), fresh()];
    let placement = Placement {
        lib_id: "Device:R",
        position: mm(152.4, 40.64),
        rotation_steps: 0,
        mirror: None,
        uuid: resistor,
        pin_uuids: &pins,
    };
    session.place_symbol(&definition, placement).unwrap();

    let counts = rule_counts(&session.erc().unwrap());
    assert_eq!(counts.get("no_connect_dangling"), Some(&1));
    assert_eq!(counts.get("unconnected_wire_endpoint"), Some(&2));
    assert_eq!(counts.get("label_dangling"), Some(&1));
    assert_eq!(counts.get("unannotated"), Some(&1));
    assert_eq!(counts.get("pin_not_connected"), Some(&2));

    session
        .set_symbol_field(resistor, "Reference", "R1")
        .unwrap();
    let violations = session.erc().unwrap();
    let counts = rule_counts(&violations);
    assert!(!counts.contains_key("unannotated"));
    let duplicate = violations
        .iter()
        .find(|v| v.rule == "duplicate_reference")
        .unwrap();
    assert_eq!(duplicate.uuids[0], resistor);
    assert_eq!(duplicate.position, mm(152.4, 40.64));
    let ids: BTreeSet<&str> = violations.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids.len(), violations.len());
}
