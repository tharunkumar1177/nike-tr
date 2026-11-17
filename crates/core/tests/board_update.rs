//! Update Board from Schematic on the golden divider, with the real KiCad 10 resistor
//! footprint from the component library fixtures, and the ratsnest it produces.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use tron_core::sexpr::{self, SExpr};
use tron_core::{
    Angle, Board, BoardUpdateOptions, BoardUpdatePlan, Changes, FootprintField, FootprintTransform,
    Nm, Point, Project, Ratsnest, Session, Side, plan_board_update, required_footprints,
};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/wiring/divider.kicad_sch");
const R_0603: &str = include_str!(
    "../../library/tests/fixtures/kicad/footprints/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod"
);
const LIB_ID: &str = "Resistor_SMD:R_0603_1608Metric";

/// Hands out predictable UUIDs.
fn counter() -> impl FnMut() -> Uuid {
    let mut next = 0xb000_0000_0000_4000_8000_0000_0000_0000u128;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

/// Opens the golden divider in `parent/bench`.
fn golden_session(parent: &Path) -> Session {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("divider.kicad_sch"), GOLDEN).unwrap();
    Session::new(Project::load(&dir).unwrap())
}

/// The resistor footprint as the component library hands it to a board: named
/// `<lib>:<name>`, without the library file's version and generator.
fn definitions() -> HashMap<String, SExpr> {
    let root = sexpr::parse(R_0603).unwrap();
    let mut items = vec![SExpr::atom("footprint"), SExpr::string(LIB_ID)];
    for item in root.args().iter().skip(1) {
        let file_only = matches!(
            item.head(),
            Some("version" | "generator" | "generator_version")
        );
        if !file_only {
            items.push(item.clone());
        }
    }
    HashMap::from([(LIB_ID.to_owned(), SExpr::List(items))])
}

/// Plans the update of the session's board with the resistor footprint available.
fn plan(session: &Session, options: BoardUpdateOptions) -> BoardUpdatePlan {
    let project = session.project();
    let definitions = definitions();
    let plan = plan_board_update(project.schematic(), project.board(), &definitions, options);
    plan.unwrap()
}

/// Applies the update to the session's board with the resistor footprint available.
fn apply(session: &mut Session, options: BoardUpdateOptions) -> Changes {
    let result = session.apply_board_update(&definitions(), options, &mut counter());
    result.unwrap()
}

/// The UUID of the symbol with `reference`.
fn symbol(session: &Session, reference: &str) -> Uuid {
    let symbols = session.project().schematic().symbols();
    let found = symbols.iter().find(|item| item.reference() == reference);
    found.unwrap().uuid
}

/// Gives R1, R2, and R3 the 0603 footprint.
fn assign_footprints(session: &mut Session) {
    for reference in ["R1", "R2", "R3"] {
        let uuid = symbol(session, reference);
        session.set_symbol_field(uuid, "Footprint", LIB_ID).unwrap();
    }
}

/// Each footprint's reference mapped to its pads' `(number, net)`.
fn pad_nets(board: &Board) -> BTreeMap<String, Vec<(String, Option<String>)>> {
    board
        .footprints()
        .into_iter()
        .map(|footprint| {
            let pads = footprint
                .pads
                .iter()
                .map(|pad| (pad.number.clone(), pad.net.clone()))
                .collect();
            (footprint.reference, pads)
        })
        .collect()
}

/// `(number, net)` for a two-pad footprint.
fn pads(first: &str, second: &str) -> Vec<(String, Option<String>)> {
    vec![
        ("1".to_owned(), Some(first.to_owned())),
        ("2".to_owned(), Some(second.to_owned())),
    ]
}

/// A point given in millimeters.
fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm::from_mm(x).unwrap(), Nm::from_mm(y).unwrap())
}

#[test]
fn the_update_places_every_footprint_with_its_nets() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    let schematic = session.project().schematic();
    let required: Vec<_> = required_footprints(schematic).into_iter().collect();
    assert_eq!(required, [LIB_ID]);

    let options = BoardUpdateOptions::default();
    let first = plan(&session, options);
    let added: Vec<_> = first
        .additions
        .iter()
        .map(|part| part.reference.as_str())
        .collect();
    assert_eq!(added, ["R1", "R2", "R3"]);
    assert!(first.issues.is_empty(), "{:?}", first.issues);

    let changes = apply(&mut session, options);
    assert!(changes.footprints);
    assert_eq!(session.undo_label(), Some("Update Board from Schematic"));
    let board = session.project().board();
    assert_eq!(
        pad_nets(board),
        BTreeMap::from([
            ("R1".to_owned(), pads("+5V", "/OUT")),
            ("R2".to_owned(), pads("/OUT", "GND")),
            ("R3".to_owned(), pads("VIN", "unconnected-(R3-Pad2)")),
        ])
    );
    let footprints = board.footprints();
    let r1 = &footprints[0];
    assert_eq!(r1.lib_id, LIB_ID);
    assert_eq!(r1.value, "10k");
    assert_eq!(r1.symbol_uuid(), Some(symbol(&session, "R1")));
    assert_eq!(r1.position, mm(1.48, 10.73));
    assert_eq!(footprints[1].position, mm(6.98, 10.73));

    let ratsnest = Ratsnest::new(board);
    let lines = ratsnest.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].net, "/OUT");
    assert_eq!(lines[0].from, mm(2.305, 10.73));
    assert_eq!(lines[0].to, mm(6.155, 10.73));

    let again = plan(&session, options);
    assert!(!again.has_changes(), "{again:?}");
    assert!(!apply(&mut session, options).any());

    session.undo().unwrap();
    assert!(session.project().board().footprints().is_empty());
    session.redo().unwrap();
    assert_eq!(session.project().board().footprints().len(), 3);
}

#[test]
fn the_update_saves_and_reopens() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    apply(&mut session, BoardUpdateOptions::default());
    session.save().unwrap();
    let reopened = Project::load(&temp.path().join("bench")).unwrap();
    assert_eq!(reopened.board(), session.project().board());
    assert_eq!(reopened.board().footprints().len(), 3);
}

#[test]
fn later_updates_rename_and_remove_footprints() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    let options = BoardUpdateOptions::default();
    apply(&mut session, options);

    let r2 = symbol(&session, "R2");
    session.set_symbol_field(r2, "Value", "22k").unwrap();
    let r3 = symbol(&session, "R3");
    session.delete_symbols(&[r3]).unwrap();
    let update = plan(&session, options);
    assert_eq!(update.field_changes.len(), 1);
    let change = &update.field_changes[0];
    assert_eq!(change.field, FootprintField::Value);
    assert_eq!(change.reference, "R2");
    assert_eq!(change.to, "22k");
    let removed: Vec<_> = update
        .removals
        .iter()
        .map(|part| part.reference.as_str())
        .collect();
    assert_eq!(removed, ["R3"]);
    assert!(update.additions.is_empty());

    let kept = BoardUpdateOptions {
        delete_extra: false,
        ..options
    };
    assert!(plan(&session, kept).removals.is_empty());

    apply(&mut session, options);
    let footprints = session.project().board().footprints();
    let values: Vec<_> = footprints
        .iter()
        .map(|footprint| (footprint.reference.as_str(), footprint.value.as_str()))
        .collect();
    assert_eq!(values, [("R1", "10k"), ("R2", "22k")]);
}

#[test]
fn footprints_move_rotate_and_flip_as_single_steps() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    apply(&mut session, BoardUpdateOptions::default());
    let r1 = session.project().board().footprints()[0].uuid;
    let original = session.project().board().clone();
    let r1_pad = |session: &Session| {
        let footprint = session.project().board().footprint(r1).unwrap();
        footprint.pad_position(&footprint.pads[1])
    };

    let moved = session
        .move_footprints(&[r1], Nm(1_000_000), Nm::ZERO)
        .unwrap();
    assert!(moved.footprints);
    assert_eq!(session.undo_label(), Some("Move Footprint"));
    let footprint = session.project().board().footprint(r1).unwrap();
    assert_eq!(footprint.position, mm(2.48, 10.73));
    assert_eq!(r1_pad(&session), mm(3.305, 10.73));
    let lines = Ratsnest::new(session.project().board()).lines();
    assert_eq!(lines[0].from, mm(3.305, 10.73));

    let rotated = session
        .transform_footprints(&[r1], FootprintTransform::Rotate(1))
        .unwrap();
    assert!(rotated.footprints);
    let footprint = session.project().board().footprint(r1).unwrap();
    assert_eq!(footprint.rotation, Angle::from_decidegrees(900));
    assert_eq!(r1_pad(&session), mm(2.48, 9.905));

    session
        .transform_footprints(&[r1], FootprintTransform::Flip)
        .unwrap();
    assert_eq!(session.undo_label(), Some("Flip Footprint"));
    let footprint = session.project().board().footprint(r1).unwrap();
    assert_eq!(footprint.side, Side::Back);
    assert_eq!(footprint.rotation, Angle::from_decidegrees(2700));
    assert_eq!(r1_pad(&session), mm(2.48, 11.555));

    let unchanged = session
        .transform_footprints(&[r1], FootprintTransform::Rotate(4))
        .unwrap();
    assert!(!unchanged.any());
    let missing = session.move_footprints(&[Uuid::nil()], Nm(1), Nm::ZERO);
    assert!(missing.is_err());

    for _ in 0..3 {
        session.undo().unwrap();
    }
    assert_eq!(session.project().board(), &original);
}

#[test]
fn footprints_missing_from_the_library_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    let r3 = symbol(&session, "R3");
    let missing = "Resistor_SMD:R_0805_2012Metric";
    session.set_symbol_field(r3, "Footprint", missing).unwrap();
    let options = BoardUpdateOptions::default();
    let update = plan(&session, options);
    assert_eq!(update.additions.len(), 2);
    assert_eq!(update.issues.len(), 1);
    assert_eq!(update.issues[0].reference, "R3");
    assert!(update.issues[0].message.contains(missing));

    apply(&mut session, options);
    assert_eq!(session.project().board().footprints().len(), 2);
}

#[test]
#[ignore = "needs tests/golden/board/divider.kicad_pcb, written by KiCad 10's Update PCB from \
    Schematic after giving R1, R2, and R3 the footprint Resistor_SMD:R_0603_1608Metric"]
fn the_update_matches_kicad() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = dir.join("tests/golden/board/divider.kicad_pcb");
    let expected = Board::parse(&fs::read_to_string(path).unwrap()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut session = golden_session(temp.path());
    assign_footprints(&mut session);
    apply(&mut session, BoardUpdateOptions::default());
    let board = session.project().board();
    assert_eq!(pad_nets(board), pad_nets(&expected));
    let links = |board: &Board| {
        let mut links: Vec<_> = board
            .footprints()
            .into_iter()
            .map(|footprint| (footprint.reference.clone(), footprint.symbol_uuid()))
            .collect();
        links.sort();
        links
    };
    assert_eq!(links(board), links(&expected));
}
