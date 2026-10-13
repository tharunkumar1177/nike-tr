use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use tron_core::kicad::Schematic;
use tron_core::kicad_value::{child_text, children};
use tron_core::{LabelKind, LabelShape, Nm, Placement, Point, Project, Session, netlist, sexpr};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/wiring/divider.kicad_sch");

/// Net names mapped to their `(reference, pin)` nodes.
type NetMap = BTreeMap<String, BTreeSet<(String, String)>>;

fn net_map(schematic: &Schematic) -> NetMap {
    let mut map = NetMap::new();
    for net in netlist(schematic).unwrap() {
        let nodes = net
            .nodes
            .into_iter()
            .map(|node| (node.reference, node.pin))
            .collect();
        map.insert(net.name, nodes);
    }
    map
}

fn expected_nets() -> NetMap {
    let net = |name: &str, nodes: &[(&str, &str)]| {
        let nodes = nodes
            .iter()
            .map(|(reference, pin)| (reference.to_string(), pin.to_string()))
            .collect();
        (name.to_owned(), nodes)
    };
    NetMap::from([
        net("+5V", &[("R1", "1")]),
        net("/OUT", &[("R1", "2"), ("R2", "1")]),
        net("GND", &[("R2", "2")]),
        net("VIN", &[("R3", "1")]),
        net("unconnected-(R3-Pad2)", &[("R3", "2")]),
    ])
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm::from_mm(x).unwrap(), Nm::from_mm(y).unwrap())
}

fn counter() -> impl FnMut() -> Uuid {
    let mut next = 0xc000_0000_0000_4000_8000_0000_0000_0000u128;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

fn golden_project(parent: &Path) -> Project {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("divider.kicad_sch"), GOLDEN).unwrap();
    Project::load(&dir).unwrap()
}

#[test]
fn the_golden_divider_reads_its_wiring_and_round_trips() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    assert_eq!(schematic.wires().len(), 6);
    assert_eq!(schematic.junctions().len(), 1);
    let labels = schematic.labels();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].kind, LabelKind::Local);
    assert_eq!(labels[1].kind, LabelKind::Global(LabelShape::Input));
    assert_eq!(labels[1].position, mm(127.0, 50.8));

    let reparsed = Schematic::parse(&schematic.to_file_string()).unwrap();
    assert_eq!(reparsed, schematic);
}

#[test]
fn pins_connect_where_the_definition_places_them() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    let points = schematic.pin_points();
    for point in [mm(100.33, 57.15), mm(100.33, 64.77), mm(100.33, 50.8)] {
        assert!(points.contains(&point), "{point:?} in {points:?}");
    }
}

#[test]
fn the_golden_divider_has_the_expected_nets() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    assert_eq!(net_map(&schematic), expected_nets());
    let nets = netlist(&schematic).unwrap();
    let out = nets.iter().find(|net| net.name == "/OUT").unwrap();
    assert_eq!(out.nodes[0].pin_type, "passive");
}

#[test]
fn drawing_the_wiring_with_commands_reproduces_the_nets() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let original = session.project().clone();
    let mut fresh = counter();

    let schematic = session.project().schematic();
    let mut wiring: Vec<Uuid> = schematic.wires().iter().map(|wire| wire.uuid).collect();
    wiring.extend(schematic.labels().iter().map(|label| label.uuid));
    session.delete_wiring(&wiring, &mut fresh).unwrap();
    let bare = session.project().schematic();
    assert!(bare.wires().is_empty() && bare.labels().is_empty());
    assert!(bare.junctions().is_empty());

    let wires = [
        [mm(100.33, 50.8), mm(100.33, 57.15)],
        [mm(100.33, 64.77), mm(100.33, 72.39)],
        [mm(100.33, 68.58), mm(110.49, 68.58)],
        [mm(100.33, 80.01), mm(100.33, 86.36)],
        [mm(127.0, 50.8), mm(127.0, 57.15)],
    ];
    for points in wires {
        session.draw_wire(&points, &mut fresh).unwrap();
    }
    let out = mm(110.49, 68.58);
    session
        .place_label(LabelKind::Local, "OUT", out, 0, &mut fresh)
        .unwrap();
    let vin = LabelKind::Global(LabelShape::Input);
    session
        .place_label(vin, "VIN", mm(127.0, 50.8), 1, &mut fresh)
        .unwrap();

    let rebuilt = session.project().schematic();
    assert_eq!(rebuilt.wires().len(), 6);
    let junctions = rebuilt.junctions();
    assert_eq!(junctions.len(), 1);
    assert_eq!(junctions[0].position, mm(100.33, 68.58));
    assert_eq!(net_map(rebuilt), expected_nets());

    for _ in 0..8 {
        session.undo().unwrap();
    }
    assert_eq!(session.project(), &original);
}

#[test]
fn moving_and_rotating_wiring_undoes_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let original = session.project().clone();
    let mut fresh = counter();
    let label = session.project().schematic().labels()[0].uuid;

    let step = Nm(1_270_000);
    session
        .move_wiring(&[label], step, Nm::ZERO, &mut fresh)
        .unwrap();
    assert_eq!(session.undo_label(), Some("Move Label"));
    let moved = session.project().schematic().labels()[0].clone();
    assert_eq!(moved.position, mm(111.76, 68.58));
    let nets = net_map(session.project().schematic());
    assert!(!nets.contains_key("/OUT"));

    session.rotate_labels(&[label], 2, &mut fresh).unwrap();
    let text = session.project().schematic().to_file_string();
    assert!(text.contains("(at 111.76 68.58 180)"), "{text}");
    assert!(text.contains("(justify right bottom)"), "{text}");

    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(session.project(), &original);
}

#[test]
fn invalid_wiring_edits_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let mut fresh = counter();
    let diagonal = [mm(0.0, 0.0), mm(2.54, 2.54)];
    assert!(session.draw_wire(&diagonal, &mut fresh).is_err());
    let anywhere = mm(10.16, 10.16);
    let local = LabelKind::Local;
    let spaced = session.place_label(local, "A B", anywhere, 0, &mut fresh);
    assert!(spaced.is_err());
    let unknown = [Uuid::from_u128(7)];
    assert!(session.delete_wiring(&unknown, &mut fresh).is_err());
    let wire = session.project().schematic().wires()[0].uuid;
    assert!(session.rotate_labels(&[wire], 1, &mut fresh).is_err());
    assert!(!session.can_undo());
}

#[test]
fn no_connect_flags_place_move_and_delete_as_undo_steps() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let original = session.project().clone();
    let mut fresh = counter();
    let pin = mm(127.0, 64.77);

    session.place_no_connect(pin, &mut fresh).unwrap();
    assert_eq!(session.undo_label(), Some("Place No-Connect Flag"));
    let schematic = session.project().schematic();
    let flags = schematic.no_connects();
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].position, pin);
    assert!(schematic.junctions().len() == 1 && schematic.wires().len() == 6);
    assert_eq!(net_map(schematic), expected_nets());
    let text = schematic.to_file_string();
    assert!(text.contains("(no_connect"), "{text}");
    let reparsed = Schematic::parse(&text).unwrap();
    assert_eq!(&reparsed, schematic);
    assert!(session.place_no_connect(pin, &mut fresh).is_err());

    let flag = flags[0].uuid;
    let step = Nm(2_540_000);
    session
        .move_wiring(&[flag], step, Nm::ZERO, &mut fresh)
        .unwrap();
    assert_eq!(session.undo_label(), Some("Move No-Connect Flag"));
    let moved = session.project().schematic().no_connects()[0];
    assert_eq!(moved.position, mm(129.54, 64.77));
    assert!(session.rotate_labels(&[flag], 1, &mut fresh).is_err());

    session.delete_wiring(&[flag], &mut fresh).unwrap();
    assert_eq!(session.undo_label(), Some("Delete No-Connect Flag"));
    assert!(session.project().schematic().no_connects().is_empty());

    for _ in 0..3 {
        session.undo().unwrap();
    }
    assert_eq!(session.project(), &original);
}

#[test]
fn annotation_numbers_new_symbols_top_to_bottom_in_one_step() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let mut fresh = counter();
    let schematic = session.project().schematic();
    let definition = schematic.lib_symbol("Device:R").unwrap().clone();
    let r2 = schematic
        .symbols()
        .into_iter()
        .find(|symbol| symbol.reference() == "R2")
        .unwrap();
    session.delete_symbols(&[r2.uuid]).unwrap();

    let spots = [mm(152.4, 40.64), mm(139.7, 40.64), mm(139.7, 30.48)];
    let mut placed = Vec::new();
    for position in spots {
        let uuid = fresh();
        let pins = [fresh(), fresh()];
        let placement = Placement {
            lib_id: "Device:R",
            position,
            rotation_steps: 0,
            mirror: None,
            uuid,
            pin_uuids: &pins,
        };
        session.place_symbol(&definition, placement).unwrap();
        placed.push(uuid);
    }
    let before = session.project().clone();

    let (changes, count) = session.annotate();
    assert!(changes.symbols);
    assert_eq!(count, 3);
    assert_eq!(session.undo_label(), Some("Annotate Schematic"));
    let symbols = session.project().schematic().symbols();
    let reference = |uuid: Uuid| {
        let symbol = symbols.iter().find(|symbol| symbol.uuid == uuid).unwrap();
        symbol.reference().to_owned()
    };
    assert_eq!(reference(placed[2]), "R2");
    assert_eq!(reference(placed[1]), "R4");
    assert_eq!(reference(placed[0]), "R5");
    let text = session.project().schematic().to_file_string();
    assert!(text.contains("(reference \"R5\")"), "{text}");
    assert!(!text.contains("R?"), "{text}");
    assert!(text.contains("\"#PWR01\""), "{text}");

    let (again, none) = session.annotate();
    assert!(!again.any() && none == 0);
    assert_eq!(session.undo_label(), Some("Annotate Schematic"));
    session.undo().unwrap();
    assert_eq!(session.project(), &before);
}

/// The nets of a netlist file exported by KiCad, which lists nets that have at least one
/// symbol pin and leaves power symbols out.
fn kicad_nets(source: &str) -> NetMap {
    let export = sexpr::parse(source).unwrap();
    let nets = export.child("nets").expect("the export has (nets)");
    let mut map = NetMap::new();
    for net in children(nets, "net") {
        let name = child_text(net, "name").unwrap().to_owned();
        let nodes = children(net, "node")
            .map(|node| {
                let reference = child_text(node, "ref").unwrap().to_owned();
                (reference, child_text(node, "pin").unwrap().to_owned())
            })
            .collect();
        map.insert(name, nodes);
    }
    map
}

/// The golden schematic's netlist matches the one KiCad exports for it. The fixture is
/// produced by opening `divider.kicad_sch` in KiCad as project `divider` and exporting the
/// KiCad netlist to `tests/golden/wiring/divider.net`.
#[test]
#[ignore = "needs tests/golden/wiring/divider.net exported from KiCad"]
fn the_netlist_matches_the_kicad_export() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = dir.join("tests/golden/wiring/divider.net");
    let exported = fs::read_to_string(path).unwrap();
    let schematic = Schematic::parse(GOLDEN).unwrap();
    assert_eq!(net_map(&schematic), kicad_nets(&exported));
}
