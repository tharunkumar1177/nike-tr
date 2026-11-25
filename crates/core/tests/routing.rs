//! Routing tracks and vias and editing the design rules through the session, with undo,
//! redo, save, and the ratsnest that routed copper shrinks, on the golden divider project.

use std::fs;
use std::path::Path;

use tron_core::{
    CoreError, DesignRules, Nm, Point, Project, Ratsnest, RouteRequest, RouteSegment, Session,
};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/wiring/divider.kicad_sch");

/// A two-layer board with the divider's two 0603 resistors 10 mm apart: `R1` from `VIN` to
/// `OUT` and `R2` from `OUT` to `GND`, so only `OUT` needs a connection.
const DIVIDER_BOARD: &str = r#"(kicad_pcb (version 20260206) (generator "pcbnew") (generator_version "10.0")
    (general (thickness 1.6))
    (layers (0 "F.Cu" signal) (2 "B.Cu" signal) (25 "Edge.Cuts" user))
    (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu")
        (uuid "a0000000-0000-4000-8000-000000000001") (at 10 10)
        (property "Reference" "R1" (at 0 -1.43 0) (layer "F.SilkS"))
        (pad "1" smd roundrect (at -0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste")
            (roundrect_rratio 0.25) (net "VIN"))
        (pad "2" smd roundrect (at 0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste")
            (roundrect_rratio 0.25) (net "OUT")))
    (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu")
        (uuid "a0000000-0000-4000-8000-000000000002") (at 20 10)
        (property "Reference" "R2" (at 0 -1.43 0) (layer "F.SilkS"))
        (pad "1" smd roundrect (at -0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste")
            (roundrect_rratio 0.25) (net "OUT"))
        (pad "2" smd roundrect (at 0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste")
            (roundrect_rratio 0.25) (net "GND")))
)"#;

/// Hands out predictable UUIDs.
fn counter() -> impl FnMut() -> Uuid {
    let mut next = 0xd000_0000_0000_4000_8000_0000_0000_0000u128;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

/// Opens the golden divider with [`DIVIDER_BOARD`] in `parent/bench`.
fn session(parent: &Path) -> Session {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("divider.kicad_sch"), GOLDEN).unwrap();
    fs::write(dir.join("divider.kicad_pcb"), DIVIDER_BOARD).unwrap();
    Session::new(Project::load(&dir).unwrap())
}

/// A point at thousandths of a millimeter.
fn um(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 1_000), Nm(y * 1_000))
}

/// A route segment from `start` to `end` on `layer`.
fn piece(start: Point, end: Point, layer: &str) -> RouteSegment {
    RouteSegment {
        start,
        end,
        layer: layer.to_owned(),
    }
}

/// A route of `OUT` with the default sizes.
fn route(segments: Vec<RouteSegment>, vias: Vec<Point>) -> RouteRequest {
    let rules = DesignRules::default();
    RouteRequest {
        net: Some("OUT".to_owned()),
        width: rules.track_width,
        via_diameter: rules.via_diameter,
        via_drill: rules.via_drill,
        segments,
        vias,
    }
}

/// The unrouted connections of the session's board.
fn ratsnest_lines(session: &Session) -> usize {
    Ratsnest::new(session.project().board()).lines().len()
}

/// The message of an `InvalidEdit` error.
fn invalid_edit<T: std::fmt::Debug>(result: Result<T, CoreError>) -> String {
    match result {
        Err(CoreError::InvalidEdit(message)) => message,
        other => panic!("expected an invalid edit, got {other:?}"),
    }
}

/// The `OUT` pads of `R1` and `R2`.
const R1_OUT: (i64, i64) = (10_825, 10_000);
const R2_OUT: (i64, i64) = (19_175, 10_000);

#[test]
fn a_straight_route_joins_the_pads_and_undoes() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path());
    assert_eq!(ratsnest_lines(&session), 1);
    let (a, b) = (um(R1_OUT.0, R1_OUT.1), um(R2_OUT.0, R2_OUT.1));
    let request = route(vec![piece(a, b, "F.Cu")], Vec::new());
    let changes = session.route_track(&request, &mut counter()).unwrap();
    assert!(changes.routes);
    assert!(!changes.footprints);
    assert_eq!(session.undo_label(), Some("Route Track"));
    let tracks = session.project().board().tracks();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].net.as_deref(), Some("OUT"));
    assert_eq!(tracks[0].width, Nm(200_000));
    assert_eq!(ratsnest_lines(&session), 0);

    session.undo().unwrap();
    assert!(session.project().board().tracks().is_empty());
    assert_eq!(ratsnest_lines(&session), 1);
    session.redo().unwrap();
    assert_eq!(ratsnest_lines(&session), 0);

    session.save().unwrap();
    let reopened = Project::load(&temp.path().join("bench")).unwrap();
    assert_eq!(reopened.board().tracks(), tracks);
    let path = temp.path().join("bench/divider.kicad_pcb");
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("(net \"OUT\")"), "{text}");
}

#[test]
fn a_two_layer_route_changes_layer_through_vias() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path());
    let start = um(R1_OUT.0, R1_OUT.1);
    let end = um(R2_OUT.0, R2_OUT.1);
    let corners = [
        start,
        um(12_000, 10_000),
        um(12_000, 12_000),
        um(14_000, 14_000),
        um(18_000, 14_000),
        um(19_175, 12_825),
        end,
    ];
    let mut segments = vec![piece(corners[0], corners[1], "F.Cu")];
    for pair in corners[1..].windows(2) {
        segments.push(piece(pair[0], pair[1], "B.Cu"));
    }
    let request = route(segments, vec![corners[1], end]);
    session.route_track(&request, &mut counter()).unwrap();
    let board = session.project().board();
    assert_eq!(board.tracks().len(), 6);
    let vias = board.vias();
    assert_eq!(vias.len(), 2);
    assert_eq!(vias[0].position, corners[1]);
    assert_eq!(vias[0].size, Nm(600_000));
    assert_eq!(vias[1].layers, ("F.Cu".to_owned(), "B.Cu".to_owned()));
    assert_eq!(ratsnest_lines(&session), 0);

    let uuids: Vec<Uuid> = vias.iter().map(|via| via.uuid).collect();
    session.delete_routes(&uuids[1..]).unwrap();
    assert_eq!(session.undo_label(), Some("Delete Track"));
    assert_eq!(session.project().board().vias().len(), 1);
    assert_eq!(ratsnest_lines(&session), 1);
    session.undo().unwrap();
    assert_eq!(session.project().board().vias(), vias);
    let message = invalid_edit(session.delete_routes(&[Uuid::from_u128(7)]));
    assert!(message.contains("no longer on the board"), "{message}");
}

#[test]
fn routes_that_break_the_rules_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path());
    let start = um(R1_OUT.0, R1_OUT.1);
    let corner = um(12_000, 10_000);
    let cases = [
        (route(Vec::new(), Vec::new()), "at least one segment"),
        (
            route(vec![piece(start, um(12_000, 11_000), "F.Cu")], Vec::new()),
            "diagonal",
        ),
        (
            route(vec![piece(start, start, "F.Cu")], Vec::new()),
            "no length",
        ),
        (
            route(vec![piece(start, corner, "In1.Cu")], Vec::new()),
            "no In1.Cu",
        ),
        (
            route(
                vec![
                    piece(start, corner, "F.Cu"),
                    piece(corner, um(12_000, 12_000), "B.Cu"),
                ],
                Vec::new(),
            ),
            "without a via",
        ),
        (
            route(
                vec![
                    piece(start, corner, "F.Cu"),
                    piece(um(12_000, 11_000), um(12_000, 12_000), "F.Cu"),
                ],
                Vec::new(),
            ),
            "end to start",
        ),
        (
            route(vec![piece(start, corner, "F.Cu")], vec![um(13_000, 10_000)]),
            "corner or end",
        ),
    ];
    for (request, expected) in cases {
        let message = invalid_edit(session.route_track(&request, &mut counter()));
        assert!(message.contains(expected), "{message}");
    }
    let mut stranger = route(vec![piece(start, corner, "F.Cu")], Vec::new());
    stranger.net = Some("NOPE".to_owned());
    let message = invalid_edit(session.route_track(&stranger, &mut counter()));
    assert!(message.contains("no net named NOPE"), "{message}");
    assert!(!session.can_undo());
}

#[test]
fn design_rules_are_saved_in_the_project_file_and_undo() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path());
    assert_eq!(session.design_rules(), DesignRules::default());
    let rules = DesignRules {
        clearance: Nm(150_000),
        track_width: Nm(300_000),
        ..DesignRules::default()
    };
    let changes = session.edit_design_rules(rules).unwrap();
    assert!(changes.design_rules);
    assert!(!changes.erc_settings);
    assert_eq!(session.undo_label(), Some("Edit Design Rules"));
    assert_eq!(session.design_rules(), rules);
    assert!(!session.edit_design_rules(rules).unwrap().any());

    let broken = DesignRules {
        via_drill: Nm(700_000),
        ..rules
    };
    let message = invalid_edit(session.edit_design_rules(broken));
    assert!(message.contains("via drill"), "{message}");

    session.save().unwrap();
    let path = temp.path().join("bench/divider.kicad_pro");
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("\"min_copper_edge_clearance\": 0.5"), "{text}");
    let reopened = Session::new(Project::load(&temp.path().join("bench")).unwrap());
    assert_eq!(reopened.design_rules(), rules);

    session.undo().unwrap();
    assert_eq!(session.design_rules(), DesignRules::default());
}
