//! Editing the board outline and the copper layer stackup through the session, with undo,
//! redo, and save, on the golden divider project.

use std::fs;
use std::path::Path;

use tron_core::{CoreError, LayerKind, Nm, OutlineShape, Point, Project, Session};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/wiring/divider.kicad_sch");

/// A four-layer board with a custom inner plane name, a named bottom layer, a user layer,
/// an ENIG finish, a non-default setup value, a track on `In2.Cu`, and an outline drawn as
/// four lines next to a silkscreen line.
const FOUR_LAYERS: &str = r#"(kicad_pcb (version 20260206) (generator "pcbnew") (generator_version "10.0")
    (general (thickness 1.6) (legacy_teardrops no))
    (paper "A4")
    (layers (0 "F.Cu" signal) (4 "In1.Cu" signal) (6 "In2.Cu" power "GND plane")
        (2 "B.Cu" signal "Bottom") (5 "F.SilkS" user "F.Silkscreen") (25 "Edge.Cuts" user)
        (39 "User.1" user))
    (setup
        (stackup (layer "F.Cu" (type "copper") (thickness 0.035)) (copper_finish "ENIG"))
        (pad_to_mask_clearance 0.05))
    (gr_line (start 0 0) (end 40 0) (stroke (width 0.1) (type default)) (layer "Edge.Cuts"))
    (gr_line (start 40 0) (end 40 30) (stroke (width 0.1) (type default)) (layer "Edge.Cuts"))
    (gr_line (start 0 -2) (end 40 -2) (stroke (width 0.15) (type default)) (layer "F.SilkS"))
    (gr_line (start 40 30) (end 0 30) (stroke (width 0.1) (type default)) (layer "Edge.Cuts"))
    (gr_line (start 0 30) (end 0 0) (stroke (width 0.1) (type default)) (layer "Edge.Cuts"))
    (segment (start 5 5) (end 20 5) (width 0.25) (layer "In2.Cu") (net "GND")
        (uuid "5a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d"))
)"#;

/// Hands out predictable UUIDs.
fn counter() -> impl FnMut() -> Uuid {
    let mut next = 0xc000_0000_0000_4000_8000_0000_0000_0000u128;
    move || {
        next += 1;
        Uuid::from_u128(next)
    }
}

/// Opens the golden divider in `parent/bench`, with `board` as its board file when given.
fn session(parent: &Path, board: Option<&str>) -> Session {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("divider.kicad_sch"), GOLDEN).unwrap();
    if let Some(board) = board {
        fs::write(dir.join("divider.kicad_pcb"), board).unwrap();
    }
    Session::new(Project::load(&dir).unwrap())
}

/// A point at whole millimeters.
fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000))
}

/// A `width` by `height` mm rectangle with its corner at the origin.
fn rectangle(width: i64, height: i64) -> Vec<Point> {
    vec![mm(0, 0), mm(width, 0), mm(width, height), mm(0, height)]
}

/// The session's board as file text.
fn board_text(session: &Session) -> String {
    session.project().board().to_file_string()
}

/// The corners of the session's outline when it is exactly one polygon.
fn outline_corners(session: &Session) -> Option<Vec<Point>> {
    match session.project().board().edge_cuts().as_slice() {
        [graphic] => match &graphic.shape {
            OutlineShape::Polygon { points } => Some(points.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// The message of an `InvalidEdit` error.
fn invalid_edit<T: std::fmt::Debug>(result: Result<T, CoreError>) -> String {
    match result {
        Err(CoreError::InvalidEdit(message)) => message,
        other => panic!("expected an invalid edit, got {other:?}"),
    }
}

#[test]
fn an_outline_is_drawn_replaced_and_undone() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), None);
    assert!(session.project().board().edge_cuts().is_empty());

    let changes = session
        .edit_board_outline(&rectangle(50, 40), &mut counter())
        .unwrap();
    assert!(changes.board_outline);
    assert!(!changes.footprints && !changes.stackup);
    assert_eq!(outline_corners(&session), Some(rectangle(50, 40)));
    let graphic = &session.project().board().edge_cuts()[0];
    assert_eq!(graphic.width, Nm(50_000));
    assert!(graphic.uuid.is_some());
    assert_eq!(session.undo_label(), Some("Edit Board Outline"));

    let mut closed_loop = rectangle(80, 60);
    closed_loop.push(mm(0, 0));
    session
        .edit_board_outline(&closed_loop, &mut counter())
        .unwrap();
    assert_eq!(outline_corners(&session), Some(rectangle(80, 60)));

    let again = session
        .edit_board_outline(&rectangle(80, 60), &mut counter())
        .unwrap();
    assert!(!again.any());

    assert!(session.undo().unwrap().board_outline);
    assert_eq!(outline_corners(&session), Some(rectangle(50, 40)));
    session.undo().unwrap();
    assert!(session.project().board().edge_cuts().is_empty());
    session.redo().unwrap();
    assert_eq!(outline_corners(&session), Some(rectangle(50, 40)));

    session.save().unwrap();
    let reopened = Project::load(&temp.path().join("bench")).unwrap();
    assert_eq!(reopened.board(), session.project().board());
}

#[test]
fn an_outline_replaces_every_edge_cuts_graphic_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), Some(FOUR_LAYERS));
    let before = board_text(&session);
    assert_eq!(session.project().board().edge_cuts().len(), 4);

    session
        .edit_board_outline(&rectangle(40, 30), &mut counter())
        .unwrap();
    assert_eq!(outline_corners(&session), Some(rectangle(40, 30)));
    let text = board_text(&session);
    let polygon = text.find("(gr_poly").unwrap();
    let silkscreen = text.find("(layer \"F.SilkS\")").unwrap();
    assert!(polygon < silkscreen, "{text}");
    assert!(text.find("(segment").unwrap() > silkscreen, "{text}");

    session.undo().unwrap();
    assert_eq!(board_text(&session), before);
}

#[test]
fn outlines_that_are_not_one_simple_polygon_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), None);
    let bowtie = [mm(0, 0), mm(10, 10), mm(10, 0), mm(0, 10)];
    let message = invalid_edit(session.edit_board_outline(&bowtie, &mut counter()));
    assert_eq!(message, "The board outline must not cross itself.");
    let line = [mm(0, 0), mm(10, 0)];
    let message = invalid_edit(session.edit_board_outline(&line, &mut counter()));
    assert_eq!(message, "The board outline needs at least three corners.");
    assert!(!session.can_undo());
    assert!(session.project().board().edge_cuts().is_empty());
}

#[test]
fn the_copper_layer_count_changes_as_one_step() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), None);
    let before = board_text(&session);

    let changes = session.edit_stackup(4).unwrap();
    assert!(changes.stackup);
    assert!(!changes.board_outline && !changes.footprints);
    assert_eq!(session.undo_label(), Some("Edit Stackup"));
    let board = session.project().board();
    assert_eq!(
        board.copper_layer_names(),
        ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]
    );
    let stackup = board.stackup().unwrap();
    assert_eq!(stackup.thickness(), Nm(1_600_000));
    let dielectrics = stackup.layers.iter().filter(|l| l.is_dielectric());
    assert_eq!(dielectrics.count(), 3);

    assert!(!session.edit_stackup(4).unwrap().any());
    session.edit_stackup(2).unwrap();
    assert_eq!(session.project().board().copper_layer_count(), 2);
    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(board_text(&session), before);
}

#[test]
fn only_even_counts_from_two_to_eight_are_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), None);
    let expected = "The copper layer count must be an even number between 2 and 8.";
    for copper in [0, 3, 10] {
        assert_eq!(invalid_edit(session.edit_stackup(copper)), expected);
    }
    assert!(!session.can_undo());
}

#[test]
fn growing_the_stackup_keeps_other_layers_and_settings() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), Some(FOUR_LAYERS));
    let before = board_text(&session);

    session.edit_stackup(6).unwrap();
    let board = session.project().board();
    let names: Vec<String> = board.layers().into_iter().map(|entry| entry.name).collect();
    assert_eq!(
        names,
        [
            "F.Cu",
            "In1.Cu",
            "In2.Cu",
            "In3.Cu",
            "In4.Cu",
            "B.Cu",
            "F.SilkS",
            "Edge.Cuts",
            "User.1"
        ]
    );
    let layers = board.layers();
    assert_eq!(layers[2].kind, LayerKind::Power);
    assert_eq!(layers[2].user_name.as_deref(), Some("GND plane"));
    assert_eq!(layers[3].ordinal, 8);
    assert_eq!(layers[3].kind, LayerKind::Signal);
    assert_eq!(layers[5].user_name.as_deref(), Some("Bottom"));
    let stackup = board.stackup().unwrap();
    assert_eq!(stackup.copper_finish.as_deref(), Some("ENIG"));
    let copper = stackup.layers.iter().filter(|l| l.kind == "copper");
    assert_eq!(copper.count(), 6);
    let text = board_text(&session);
    assert!(text.contains("(pad_to_mask_clearance 0.05)"), "{text}");
    assert!(text.contains("(legacy_teardrops no)"), "{text}");

    session.undo().unwrap();
    assert_eq!(board_text(&session), before);
}

#[test]
fn removing_a_layer_that_is_still_used_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = session(temp.path(), Some(FOUR_LAYERS));
    let message = invalid_edit(session.edit_stackup(2));
    assert_eq!(
        message,
        "The In2.Cu layer is still used by a footprint, track, via, zone, or graphic. \
         Remove those items first."
    );
    assert!(!session.can_undo());
    assert_eq!(session.project().board().copper_layer_count(), 4);
}

#[test]
fn a_board_without_a_layer_table_or_setup_gets_them() {
    let temp = tempfile::tempdir().unwrap();
    let bare = r#"(kicad_pcb (version 20260206) (generator "pcbnew") (paper "A4"))"#;
    let mut session = session(temp.path(), Some(bare));
    let before = board_text(&session);

    session.edit_stackup(4).unwrap();
    let text = board_text(&session);
    let at = |needle: &str| text.find(needle).unwrap();
    assert!(at("(generator_version") < at("(general"), "{text}");
    assert!(at("(general") < at("(paper"), "{text}");
    assert!(at("(paper") < at("(layers"), "{text}");
    assert!(at("(layers") < at("(setup"), "{text}");
    assert!(text.contains("(thickness 1.6)"), "{text}");
    assert_eq!(session.project().board().copper_layer_count(), 4);
    assert!(session.project().board().stackup().is_some());

    session.save().unwrap();
    let reopened = Project::load(&temp.path().join("bench")).unwrap();
    assert_eq!(reopened.board(), session.project().board());
    session.undo().unwrap();
    assert_eq!(board_text(&session), before);
}
