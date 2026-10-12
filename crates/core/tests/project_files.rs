use std::fs;
use std::path::Path;

use tron_core::{CoreError, Nm, Project, Session, TitleBlock};
use uuid::Uuid;

const GOLDEN_UUID: &str = "5b1c2f0e-8a4d-4c47-9b1e-2f8a6d3c7e10";

fn golden_uuid() -> Uuid {
    Uuid::parse_str(GOLDEN_UUID).unwrap()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn edited_session(parent: &Path) -> Session {
    let project = Project::create(parent, "amp", golden_uuid()).unwrap();
    let mut session = Session::new(project);
    let mut block = TitleBlock {
        title: "Class A \"amplifier\"".to_owned(),
        date: "2026-10-12".to_owned(),
        revision: "B".to_owned(),
        company: "Bench Labs".to_owned(),
        ..TitleBlock::default()
    };
    block.comments[3] = "Fourth comment".to_owned();
    session.edit_properties(block, Nm(2_540_000)).unwrap();
    session
}

#[test]
fn a_new_project_writes_the_golden_files() {
    let temp = tempfile::tempdir().unwrap();
    let project = Project::create(temp.path(), "blank", golden_uuid()).unwrap();

    let schematic = include_str!("golden/blank/blank.kicad_sch");
    let board = include_str!("golden/blank/blank.kicad_pcb");
    let manifest = include_str!("golden/blank/tron.json");

    assert_eq!(project.dir(), temp.path().join("blank"));
    assert_eq!(read(&project.schematic_path()), schematic);
    assert_eq!(read(&project.board_path()), board);
    assert_eq!(read(&project.manifest_path()), manifest);
}

#[test]
fn a_saved_project_reopens_with_identical_content() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = edited_session(temp.path());
    session.save().unwrap();
    assert!(!session.is_dirty());

    let reopened = Project::load(session.project().dir()).unwrap();
    assert_eq!(&reopened, session.project());

    let files = |project: &Project| {
        [
            read(&project.manifest_path()),
            read(&project.schematic_path()),
            read(&project.board_path()),
        ]
    };
    let first = files(&reopened);
    reopened.save().unwrap();
    assert_eq!(files(&reopened), first);
}

#[test]
fn a_project_opens_from_a_file_inside_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let project = Project::create(temp.path(), "blank", golden_uuid()).unwrap();
    assert_eq!(Project::load(&project.manifest_path()).unwrap(), project);
    assert_eq!(Project::load(&project.schematic_path()).unwrap(), project);
}

#[test]
fn save_as_writes_a_copy_and_moves_the_session() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = edited_session(temp.path());
    let original_dir = session.project().dir().to_path_buf();

    session.save_as(temp.path(), "amp copy").unwrap();
    assert!(!session.is_dirty());
    assert_eq!(session.project().dir(), temp.path().join("amp copy"));
    assert_eq!(session.project().name(), "amp copy");
    assert!(
        session
            .project()
            .schematic_path()
            .ends_with("amp copy.kicad_sch")
    );

    let copy = Project::load(session.project().dir()).unwrap();
    assert_eq!(&copy, session.project());
    let untouched = Project::load(&original_dir).unwrap();
    assert_eq!(untouched.title_block(), TitleBlock::default());
}

#[test]
fn a_kicad_project_without_a_manifest_opens_and_keeps_unknown_nodes() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("bench");
    fs::create_dir(&dir).unwrap();
    let source = include_str!("golden/kicad-only/amp.kicad_sch");
    fs::write(dir.join("amp.kicad_sch"), source).unwrap();

    let project = Project::load(&dir).unwrap();
    assert_eq!(project.name(), "amp");
    assert_eq!(project.title_block().title, "Class A amplifier");
    assert_eq!(project.title_block().comments[1], "Bench prototype");
    assert_eq!(project.page_size().width, Nm(297_000_000));

    project.save().unwrap();
    assert!(project.manifest_path().is_file());
    assert!(project.board_path().is_file());
    let written = read(&project.schematic_path());
    assert!(written.contains("(wire"));
    assert!(written.contains("(label \"VIN\""));
    assert_eq!(Project::load(&dir).unwrap(), project);
}

#[test]
fn parse_errors_name_the_file_line_and_column() {
    let temp = tempfile::tempdir().unwrap();
    let project = Project::create(temp.path(), "broken", golden_uuid()).unwrap();
    let truncated = "(kicad_sch\n  (version 20231120)\n  (paper \"A4\"\n";
    fs::write(project.schematic_path(), truncated).unwrap();

    match Project::load(project.dir()) {
        Err(CoreError::Parse {
            path, line, column, ..
        }) => {
            assert_eq!(path, project.schematic_path());
            assert_eq!((line, column), (3, 3));
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn creating_into_a_non_empty_folder_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("taken");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("notes.txt"), "keep me").unwrap();

    assert!(matches!(
        Project::create(temp.path(), "taken", golden_uuid()),
        Err(CoreError::AlreadyExists(_))
    ));
    assert_eq!(read(&dir.join("notes.txt")), "keep me");
}

#[test]
fn a_folder_without_a_project_is_reported() {
    let temp = tempfile::tempdir().unwrap();
    let result = Project::load(temp.path());
    assert!(matches!(result, Err(CoreError::NotAProject(_))));
}

#[test]
fn a_manifest_from_a_newer_version_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let project = Project::create(temp.path(), "future", golden_uuid()).unwrap();
    let manifest = r#"{"formatVersion": 99, "name": "future"}"#;
    fs::write(project.manifest_path(), manifest).unwrap();
    assert!(matches!(
        Project::load(project.dir()),
        Err(CoreError::UnsupportedVersion { found: 99, .. })
    ));
}
