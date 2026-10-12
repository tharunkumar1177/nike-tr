use std::fs;
use std::path::{Path, PathBuf};

use tron_library::preview::{
    Fill, FootprintGraphic, Layer, Pad, PadKind, PadShape, Pin, Shape, Xy,
};
use tron_library::{
    ATTRIBUTION, Index, ItemKind, LibraryError, META_ATTRIBUTION, META_KICAD_TAG, Mount,
    SCHEMA_VERSION, SearchHit, build_index,
};

const OPAMP_LIBRARY: &str = "symbols/Amplifier_Operational.kicad_symdir";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kicad")
}

fn build_fixtures(output: &Path) -> tron_library::BuildReport {
    let fixtures = fixtures();
    let symbols = fixtures.join("symbols");
    let footprints = fixtures.join("footprints");
    build_index(&symbols, &footprints, "10.0.7", output).unwrap()
}

fn fixture_index(dir: &Path) -> Index {
    let output = dir.join("library.db");
    build_fixtures(&output);
    Index::open(&output).unwrap()
}

fn names(hits: &[SearchHit]) -> Vec<(ItemKind, &str, &str)> {
    hits.iter()
        .map(|hit| (hit.kind, hit.lib.as_str(), hit.name.as_str()))
        .collect()
}

fn find<'a>(hits: &'a [SearchHit], name: &str) -> &'a SearchHit {
    let Some(hit) = hits.iter().find(|hit| hit.name == name) else {
        panic!("{name} missing from {:?}", names(hits));
    };
    hit
}

#[test]
fn builds_every_fixture_library_and_records_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("library.db");
    let report = build_fixtures(&output);
    assert_eq!(report.symbol_libraries, 2);
    assert_eq!(report.symbols, 4);
    assert_eq!(report.footprint_libraries, 2);
    assert_eq!(report.footprints, 2);
    assert!(!dir.path().join("library.db.partial").exists());

    let index = Index::open(&output).unwrap();
    assert_eq!(index.item_count(ItemKind::Symbol).unwrap(), 4);
    assert_eq!(index.item_count(ItemKind::Footprint).unwrap(), 2);
    assert_eq!(
        index.meta(META_KICAD_TAG).unwrap().as_deref(),
        Some("10.0.7")
    );
    assert_eq!(
        index.meta(META_ATTRIBUTION).unwrap().as_deref(),
        Some(ATTRIBUTION)
    );
    assert_eq!(index.meta("missing").unwrap(), None);
}

#[test]
fn rebuilding_replaces_an_existing_index() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("library.db");
    build_fixtures(&output);
    build_fixtures(&output);
    let index = Index::open(&output).unwrap();
    assert_eq!(index.item_count(ItemKind::Symbol).unwrap(), 4);
}

#[test]
fn exact_names_rank_first() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let hits = index.search("tl072", 10).unwrap();
    assert_eq!(
        names(&hits),
        [(ItemKind::Symbol, "Amplifier_Operational", "TL072")]
    );
    assert_eq!(
        hits[0].description,
        "Dual Low-Noise JFET-Input Operational Amplifiers, DIP-8/SOIC-8"
    );
}

#[test]
fn prefixes_match_symbols_and_footprints() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let hits = index.search("resist", 10).unwrap();
    assert_eq!(find(&hits, "R").kind, ItemKind::Symbol);
    let footprint = find(&hits, "R_0603_1608Metric");
    assert_eq!(footprint.kind, ItemKind::Footprint);
    assert_eq!(footprint.lib, "Resistor_SMD");

    let hits = index.search("opamp", 10).unwrap();
    assert_eq!(hits.len(), 2);
    find(&hits, "LM2904");
    find(&hits, "TL072");
}

#[test]
fn every_term_must_match() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let hits = index.search("Device:LED", 10).unwrap();
    assert_eq!(names(&hits), [(ItemKind::Symbol, "Device", "LED")]);
    let hits = index.search("0603", 10).unwrap();
    assert_eq!(
        names(&hits),
        [(ItemKind::Footprint, "Resistor_SMD", "R_0603_1608Metric")]
    );
    assert!(index.search("resistor led", 10).unwrap().is_empty());
}

#[test]
fn search_respects_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    assert_eq!(index.search("r", 1).unwrap().len(), 1);
}

#[test]
fn query_syntax_in_user_input_is_searched_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    for query in [
        "", "   ", "\"", "*", "NEAR(", "a OR", "AND", "R\"x", "((((", "-", "^r",
    ] {
        assert!(index.search(query, 10).is_ok(), "query {query:?} failed");
    }
    assert!(index.search("\"", 10).unwrap().is_empty());
}

#[test]
fn symbols_store_properties_and_y_down_preview_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let hits = index.search("Device:R", 10).unwrap();
    let r = index.symbol(find(&hits, "R").id).unwrap().unwrap();
    assert_eq!(r.lib, "Device");
    assert_eq!(r.description, "Resistor");
    assert_eq!(r.keywords, "R res resistor");
    assert_eq!(r.reference, "R");
    assert!(r.datasheet.is_empty());
    assert_eq!(r.footprint_filters, "R_*");
    assert_eq!(r.extends, None);
    assert_eq!((r.unit_count, r.pin_count, r.power), (1, 2, false));
    assert!(!r.preview.pin_numbers_visible);
    assert!(r.preview.pin_names_visible);
    assert_eq!(
        r.preview.shapes,
        [Shape::Rect {
            start: Xy(-1_016_000, 2_540_000),
            end: Xy(1_016_000, -2_540_000),
            width: 254_000,
            fill: Fill::None,
        }]
    );
    assert_eq!(
        r.preview.pins,
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
}

#[test]
fn derived_symbols_inherit_graphics_and_keep_their_own_properties() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let id = index.search("TL072", 1).unwrap()[0].id;
    let tl072 = index.symbol(id).unwrap().unwrap();
    assert_eq!(tl072.extends.as_deref(), Some("LM2904"));
    assert_eq!(
        tl072.datasheet,
        "http://www.ti.com/lit/ds/symlink/tl071.pdf"
    );
    assert_eq!(tl072.reference, "U");
    assert_eq!((tl072.unit_count, tl072.pin_count), (3, 8));
    assert_eq!(
        tl072.preview.shapes,
        [Shape::Polyline {
            points: vec![
                Xy(-5_080_000, -5_080_000),
                Xy(5_080_000, 0),
                Xy(-5_080_000, 5_080_000),
                Xy(-5_080_000, -5_080_000),
            ],
            width: 254_000,
            fill: Fill::Background,
        }]
    );

    let pins = &tl072.preview.pins;
    let numbers: Vec<&str> = pins.iter().map(|pin| pin.number.as_str()).collect();
    assert_eq!(numbers, ["1", "2", "3"]);
    let pin_names: Vec<&str> = pins.iter().map(|pin| pin.name.as_str()).collect();
    assert_eq!(pin_names, ["", "-", "+"]);
    let connections: Vec<Xy> = pins.iter().map(|pin| pin.connection).collect();
    assert_eq!(
        connections,
        [
            Xy(7_620_000, 0),
            Xy(-7_620_000, 2_540_000),
            Xy(-7_620_000, -2_540_000),
        ]
    );
    let bodies: Vec<Xy> = pins.iter().map(|pin| pin.body).collect();
    assert_eq!(
        bodies,
        [
            Xy(5_080_000, 0),
            Xy(-5_080_000, 2_540_000),
            Xy(-5_080_000, -2_540_000),
        ]
    );
}

#[test]
fn hidden_pin_names_are_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let hits = index.search("Device:LED", 1).unwrap();
    let led = index.symbol(hits[0].id).unwrap().unwrap();
    assert!(!led.preview.pin_names_visible);
    assert_eq!(led.preview.shapes.len(), 5);
    assert_eq!(led.preview.pins.len(), 2);
}

#[test]
fn footprints_store_pads_and_front_graphics() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let id = index.search("LED_D5.0mm", 1).unwrap()[0].id;
    let led = index.footprint(id).unwrap().unwrap();
    assert_eq!(led.lib, "LED_THT");
    assert_eq!(led.tags, "LED");
    assert!(led.description.starts_with("LED, diameter 5.0mm, 2 pins"));
    assert_eq!((led.pad_count, led.mount), (2, Mount::ThroughHole));

    let layers: Vec<Layer> = led
        .preview
        .graphics
        .iter()
        .map(|graphic| graphic.layer)
        .collect();
    assert_eq!(
        layers,
        [
            Layer::Silkscreen,
            Layer::Silkscreen,
            Layer::Silkscreen,
            Layer::Silkscreen,
            Layer::Courtyard,
            Layer::Fabrication,
            Layer::Fabrication,
            Layer::Fabrication,
        ]
    );
    assert_eq!(
        led.preview.graphics[3],
        FootprintGraphic {
            layer: Layer::Silkscreen,
            shape: Shape::Circle {
                center: Xy(1_270_000, 0),
                radius: 2_500_000,
                width: 120_000,
                fill: Fill::None,
            },
        }
    );
    assert_eq!(
        led.preview.pads[1],
        Pad {
            number: "2".to_owned(),
            kind: PadKind::ThroughHole,
            shape: PadShape::Circle,
            center: Xy(2_540_000, 0),
            size: Xy(1_800_000, 1_800_000),
            rotation: 0,
            corner_radius: 0,
            drill: Some(Xy(900_000, 900_000)),
        }
    );
}

#[test]
fn rounded_smd_pads_store_their_corner_radius() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    let id = index.search("R_0603_1608Metric", 1).unwrap()[0].id;
    let r = index.footprint(id).unwrap().unwrap();
    assert_eq!((r.pad_count, r.mount), (2, Mount::Smd));
    assert_eq!(r.preview.graphics.len(), 4);
    let pad = &r.preview.pads[0];
    assert_eq!((pad.kind, pad.shape), (PadKind::Smd, PadShape::RoundRect));
    assert_eq!(pad.center, Xy(-825_000, 0));
    assert_eq!(pad.corner_radius, 200_000);
    assert_eq!(pad.drill, None);
}

#[test]
fn unknown_ids_return_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let index = fixture_index(dir.path());
    assert_eq!(index.symbol(9_999).unwrap(), None);
    assert_eq!(index.footprint(9_999).unwrap(), None);
}

#[test]
fn a_missing_parent_symbol_fails_the_build_without_writing_output() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join(OPAMP_LIBRARY);
    fs::create_dir_all(&library).unwrap();
    let source = fixtures().join(OPAMP_LIBRARY).join("TL072.kicad_sym");
    fs::copy(source, library.join("TL072.kicad_sym")).unwrap();

    let symbols = dir.path().join("symbols");
    let footprints = fixtures().join("footprints");
    let output = dir.path().join("library.db");
    let result = build_index(&symbols, &footprints, "10.0.7", &output);
    match result {
        Err(LibraryError::Sources(failures)) => {
            assert_eq!(failures.len(), 1);
            let message = failures[0].to_string();
            assert!(message.contains("LM2904"), "{message}");
        }
        other => panic!("expected a source failure, got {other:?}"),
    }
    assert!(!output.exists());
}

#[test]
fn opening_rejects_files_that_are_not_an_index() {
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("garbage.db");
    fs::write(&garbage, vec![b'x'; 4096]).unwrap();
    match Index::open(&garbage) {
        Err(LibraryError::Database(_)) => {}
        Err(err) => panic!("expected a database error, got {err}"),
        Ok(_) => panic!("a garbage file opened as an index"),
    }

    let empty = dir.path().join("empty.db");
    fs::write(&empty, b"").unwrap();
    match Index::open(&empty) {
        Err(LibraryError::SchemaMismatch { found, expected }) => {
            assert_eq!((found, expected), (0, SCHEMA_VERSION));
        }
        Err(err) => panic!("expected a schema mismatch, got {err}"),
        Ok(_) => panic!("an empty file opened as an index"),
    }

    assert!(Index::open(&dir.path().join("absent.db")).is_err());
}
