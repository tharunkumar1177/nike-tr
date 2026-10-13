use std::fs;
use std::path::Path;

use tron_core::kicad::Schematic;
use tron_core::symbol::{Fill, Pin, Shape, Xy};
use tron_core::{Angle, Mirror, Nm, Placement, Point, Project, Session};
use uuid::Uuid;

const GOLDEN: &str = include_str!("golden/symbols/amp.kicad_sch");

const SHEET_UUID: &str = "0c6f3a52-9d0e-4f43-8c55-6a3b1b0e2d71";
const SYMBOL_4: Uuid = Uuid::from_u128(0x11111111_2222_4333_8444_888888888888);
const PIN_A: Uuid = Uuid::from_u128(0x11111111_2222_4333_8444_888888888801);
const PIN_B: Uuid = Uuid::from_u128(0x11111111_2222_4333_8444_888888888802);

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn golden_project(parent: &Path) -> Project {
    let dir = parent.join("bench");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("amp.kicad_sch"), GOLDEN).unwrap();
    Project::load(&dir).unwrap()
}

#[test]
fn a_schematic_with_placed_symbols_round_trips_byte_identically() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    assert_eq!(schematic.to_file_string(), GOLDEN);

    let temp = tempfile::tempdir().unwrap();
    let project = golden_project(temp.path());
    project.save().unwrap();
    assert_eq!(read(&project.schematic_path()), GOLDEN);
    assert_eq!(Project::load(project.dir()).unwrap(), project);
}

#[test]
fn placed_symbols_read_as_typed_instances() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    assert_eq!(
        schematic.sheet_uuid(),
        Some(Uuid::parse_str(SHEET_UUID).unwrap())
    );
    assert_eq!(schematic.lib_symbol_ids(), ["Device:R"]);

    let symbols = schematic.symbols();
    assert_eq!(symbols.len(), 3);

    let first = &symbols[0];
    assert_eq!(first.lib_id, "Device:R");
    assert_eq!(first.position, Point::new(Nm(114_300_000), Nm(85_090_000)));
    assert_eq!(first.rotation, Angle::ZERO);
    assert_eq!(first.mirror, None);
    assert_eq!((first.unit, first.reference()), (1, "R1"));
    assert_eq!(first.pins.len(), 2);
    assert_eq!(first.properties.len(), 4);
    let reference = &first.properties[0];
    assert_eq!(
        reference.position,
        Point::new(Nm(116_332_000), Nm(85_090_000))
    );
    assert_eq!(reference.rotation, Angle::from_decidegrees(900));
    assert!(!reference.hidden);
    assert!(
        first.properties[2].hidden,
        "the Footprint property is hidden"
    );

    let rotated = &symbols[1];
    assert_eq!(rotated.rotation, Angle::from_decidegrees(900));
    assert_eq!(
        rotated.properties[0].rotation,
        Angle::from_decidegrees(1800)
    );
    assert_eq!(rotated.reference(), "R2");

    let mirrored = &symbols[2];
    assert_eq!(mirrored.mirror, Some(Mirror::X));
    assert_eq!(mirrored.reference(), "R3");
    assert_eq!(
        mirrored.properties[0].position,
        Point::new(Nm(87_503_000), Nm(60_960_000))
    );
}

#[test]
fn the_embedded_definition_produces_a_preview() {
    let schematic = Schematic::parse(GOLDEN).unwrap();
    let preview = schematic.symbol_preview("Device:R").unwrap().unwrap();
    assert_eq!(
        preview.shapes,
        [Shape::Rect {
            start: Xy(-1_016_000, 2_540_000),
            end: Xy(1_016_000, -2_540_000),
            width: 254_000,
            fill: Fill::None,
        }]
    );
    assert_eq!(
        preview.pins,
        [
            Pin {
                connection: Xy(0, -3_810_000),
                body: Xy(0, -2_540_000),
                number: "1".to_owned(),
                name: String::new(),
                electrical: "passive".to_owned(),
            },
            Pin {
                connection: Xy(0, 3_810_000),
                body: Xy(0, 2_540_000),
                number: "2".to_owned(),
                name: String::new(),
                electrical: "passive".to_owned(),
            },
        ]
    );
    assert!(!preview.pin_numbers_visible);
    assert!(preview.pin_names_visible);
    assert!(schematic.symbol_preview("Device:Missing").is_none());
}

#[test]
fn placing_moving_and_deleting_through_a_session_survives_a_reload() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new(golden_project(temp.path()));
    let original = session.project().clone();

    let definition = session
        .project()
        .schematic()
        .lib_symbol("Device:R")
        .unwrap()
        .clone();
    let changes = session
        .place_symbol(
            &definition,
            Placement {
                lib_id: "Device:R",
                position: Point::new(Nm(150_000_000), Nm(40_000_000)),
                rotation_steps: 1,
                mirror: Some(Mirror::Y),
                uuid: SYMBOL_4,
                pin_uuids: &[PIN_A, PIN_B],
            },
        )
        .unwrap();
    assert!(changes.symbols && !changes.definitions);
    assert_eq!(session.project().schematic().lib_symbol_ids(), ["Device:R"]);

    let placed = session.project().schematic().symbols()[3].clone();
    assert_eq!(placed.reference(), "R?");
    assert_eq!(placed.rotation, Angle::from_decidegrees(900));
    assert_eq!(placed.mirror, Some(Mirror::Y));
    assert_eq!(
        placed.properties[0].position,
        Point::new(Nm(150_000_000), Nm(37_968_000))
    );
    assert_eq!(placed.properties[0].rotation, Angle::ZERO);
    let text = session.project().schematic().to_file_string();
    assert!(text.contains("(mirror y)"), "{text}");
    assert!(text.contains("(reference \"R?\")"), "{text}");

    session.undo().unwrap();
    assert_eq!(session.project(), &original);
    session.redo().unwrap();
    assert_eq!(session.project().schematic().symbols().len(), 4);

    session.delete_symbols(&[SYMBOL_4]).unwrap();
    assert_eq!(session.project(), &original);

    session.save().unwrap();
    assert!(!session.is_dirty());
    assert_eq!(
        Project::load(session.project().dir()).unwrap(),
        *session.project()
    );
}

#[test]
fn placing_into_a_file_without_lib_symbols_undoes_exactly() {
    const BARE: &str = "(kicad_sch\n\t(version 20231120)\n\t(generator \"eeschema\")\n\t(uuid \"0c6f3a52-9d0e-4f43-8c55-6a3b1b0e2d71\")\n\t(paper \"A4\")\n\t(sheet_instances\n\t\t(path \"/\"\n\t\t\t(page \"1\")\n\t\t)\n\t)\n)\n";
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("bare");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("amp.kicad_sch"), BARE).unwrap();
    let mut session = Session::new(Project::load(&dir).unwrap());

    let definition = Schematic::parse(GOLDEN)
        .unwrap()
        .lib_symbol("Device:R")
        .unwrap()
        .clone();
    let changes = session
        .place_symbol(
            &definition,
            Placement {
                lib_id: "Device:R",
                position: Point::new(Nm(100_000_000), Nm(50_000_000)),
                rotation_steps: 0,
                mirror: None,
                uuid: SYMBOL_4,
                pin_uuids: &[PIN_A, PIN_B],
            },
        )
        .unwrap();
    assert!(changes.symbols && changes.definitions);

    // The new (lib_symbols) lands after the sheet header, before the placed symbol.
    let text = session.project().schematic().to_file_string();
    let lib_symbols = text.find("(lib_symbols").unwrap();
    let placed = text.find("(lib_id").unwrap();
    assert!(lib_symbols < placed, "{text}");

    session.undo().unwrap();
    assert_eq!(session.project().schematic().to_file_string(), BARE);
    session.redo().unwrap();
    assert!(text.contains("(lib_id \"Device:R\")"));
}
