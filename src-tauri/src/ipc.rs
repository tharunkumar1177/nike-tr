use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, PackageInfo, State, Window};
use tron_core::kicad::Schematic;
use tron_core::kicad_value::text_arg;
use tron_core::sexpr::SExpr;
use tron_core::{
    Changes, FieldEdit, Junction, Label, LabelKind, LabelShape, Mirror, Nm, NoConnect,
    PlacedSymbol, Placement, Point, Project, Session, SymbolPreview, TITLE_BLOCK_COMMENTS,
    TitleBlock, Wire, Xy,
};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{AppError, AppErrorKind};
use crate::library::{LibraryState, read_index};
use crate::settings::{self, Settings};
use crate::shell;

pub const DOCUMENT_CHANGED_EVENT: &str = "document-changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

impl From<&PackageInfo> for AppInfo {
    fn from(package: &PackageInfo) -> Self {
        AppInfo {
            name: package.name.clone(),
            version: package.version.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TitleBlockFields {
    pub title: String,
    pub date: String,
    pub revision: String,
    pub company: String,
    /// Always nine entries; index 0 is comment 1.
    pub comments: Vec<String>,
}

impl From<&TitleBlock> for TitleBlockFields {
    fn from(block: &TitleBlock) -> Self {
        TitleBlockFields {
            title: block.title.clone(),
            date: block.date.clone(),
            revision: block.revision.clone(),
            company: block.company.clone(),
            comments: block.comments.to_vec(),
        }
    }
}

impl TryFrom<TitleBlockFields> for TitleBlock {
    type Error = AppError;

    fn try_from(fields: TitleBlockFields) -> Result<Self, AppError> {
        let comments: [String; TITLE_BLOCK_COMMENTS] =
            fields.comments.try_into().map_err(|_| {
                AppError::new(
                    AppErrorKind::InvalidEdit,
                    format!("The title block must have exactly {TITLE_BLOCK_COMMENTS} comments."),
                    None,
                )
            })?;
        Ok(TitleBlock {
            title: fields.title,
            date: fields.date,
            revision: fields.revision,
            company: fields.company,
            comments,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SheetSize {
    #[ts(type = "number")]
    pub width_nm: i64,
    #[ts(type = "number")]
    pub height_nm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryStatus {
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

impl From<&Session> for HistoryStatus {
    fn from(session: &Session) -> Self {
        HistoryStatus {
            dirty: session.is_dirty(),
            can_undo: session.can_undo(),
            can_redo: session.can_redo(),
            undo_label: session.undo_label().map(str::to_owned),
            redo_label: session.redo_label().map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectSnapshot {
    pub revision: u32,
    pub name: String,
    pub directory: String,
    pub title_block: TitleBlockFields,
    #[ts(type = "number")]
    pub grid_size_nm: i64,
    pub sheet: SheetSize,
    /// The embedded symbol definitions as previews, keyed by library ID.
    pub definitions: BTreeMap<String, SymbolPreview>,
    /// Every symbol placed on the schematic.
    pub instances: Vec<SymbolInstance>,
    pub wires: Vec<WireSegment>,
    pub junctions: Vec<JunctionDot>,
    pub labels: Vec<NetLabel>,
    pub no_connects: Vec<NoConnectFlag>,
    pub history: HistoryStatus,
}

impl ProjectSnapshot {
    fn new(session: &Session, revision: u32) -> ProjectSnapshot {
        let project = session.project();
        let page = project.page_size();
        ProjectSnapshot {
            revision,
            name: project.name().to_owned(),
            directory: project.dir().display().to_string(),
            title_block: TitleBlockFields::from(&project.title_block()),
            grid_size_nm: project.grid_size().0,
            sheet: SheetSize {
                width_nm: page.width.0,
                height_nm: page.height.0,
            },
            definitions: symbol_definitions(project),
            instances: symbol_instances(project),
            wires: wire_segments(project),
            junctions: junction_dots(project),
            labels: net_labels(project),
            no_connects: no_connect_flags(project),
            history: HistoryStatus::from(session),
        }
    }
}

/// The embedded symbol definitions as previews, keyed by library ID. A definition that
/// fails to read is left out (and logged); the file itself is untouched.
fn symbol_definitions(project: &Project) -> BTreeMap<String, SymbolPreview> {
    let schematic = project.schematic();
    let mut definitions = BTreeMap::new();
    for lib_id in schematic.lib_symbol_ids() {
        match schematic.symbol_preview(lib_id) {
            Some(Ok(preview)) => {
                definitions.insert(lib_id.to_owned(), preview);
            }
            Some(Err(error)) => {
                eprintln!("could not read the embedded symbol {lib_id:?}: {error}");
            }
            None => {}
        }
    }
    definitions
}

/// Every symbol placed on the schematic, in file order.
fn symbol_instances(project: &Project) -> Vec<SymbolInstance> {
    let schematic = project.schematic();
    schematic
        .symbols()
        .iter()
        .map(|symbol| SymbolInstance::new(symbol, schematic))
        .collect()
}

fn wire_segments(project: &Project) -> Vec<WireSegment> {
    let wires = project.schematic().wires();
    wires.iter().map(WireSegment::from).collect()
}

fn junction_dots(project: &Project) -> Vec<JunctionDot> {
    let junctions = project.schematic().junctions();
    junctions.iter().map(JunctionDot::from).collect()
}

fn net_labels(project: &Project) -> Vec<NetLabel> {
    let labels = project.schematic().labels();
    labels.iter().map(NetLabel::from).collect()
}

fn no_connect_flags(project: &Project) -> Vec<NoConnectFlag> {
    let flags = project.schematic().no_connects();
    flags.iter().map(NoConnectFlag::from).collect()
}

/// The parts of the document that changed between two revisions.
/// Fields that didn't change are `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentDiff {
    pub base_revision: u32,
    pub revision: u32,
    pub title_block: Option<TitleBlockFields>,
    #[ts(type = "number | null")]
    pub grid_size_nm: Option<i64>,
    /// The full definitions map when it changed: new definitions appear, unused ones
    /// disappear.
    pub definitions: Option<BTreeMap<String, SymbolPreview>>,
    /// Every placed symbol when the placement set changed.
    pub instances: Option<Vec<SymbolInstance>>,
    /// Every wire, junction, label, and no-connect flag when the wiring changed.
    pub wires: Option<Vec<WireSegment>>,
    pub junctions: Option<Vec<JunctionDot>>,
    pub labels: Option<Vec<NetLabel>>,
    pub no_connects: Option<Vec<NoConnectFlag>>,
    pub history: HistoryStatus,
}

/// Builds the diff that takes the UI from `base_revision` to `revision`.
fn document_diff(
    session: &Session,
    base_revision: u32,
    revision: u32,
    changes: Changes,
) -> DocumentDiff {
    let project = session.project();
    DocumentDiff {
        base_revision,
        revision,
        title_block: changes
            .title_block
            .then(|| TitleBlockFields::from(&project.title_block())),
        grid_size_nm: changes.grid_size.then(|| project.grid_size().0),
        definitions: changes.definitions.then(|| symbol_definitions(project)),
        instances: changes.symbols.then(|| symbol_instances(project)),
        wires: changes.wiring.then(|| wire_segments(project)),
        junctions: changes.wiring.then(|| junction_dots(project)),
        labels: changes.wiring.then(|| net_labels(project)),
        no_connects: changes.wiring.then(|| no_connect_flags(project)),
        history: HistoryStatus::from(session),
    }
}

/// The Reference or Value text of a placed symbol, positioned in schematic coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SymbolText {
    pub text: String,
    #[ts(type = "number")]
    pub x_nm: i64,
    #[ts(type = "number")]
    pub y_nm: i64,
    /// Tenths of a degree, counterclockwise, in `[0, 3600)`.
    pub rotation: i32,
    pub visible: bool,
}

/// The axis a symbol is mirrored across: `x` flips it horizontally, `y` vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum MirrorAxis {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "y")]
    Y,
}

impl From<MirrorAxis> for Mirror {
    fn from(axis: MirrorAxis) -> Mirror {
        match axis {
            MirrorAxis::X => Mirror::X,
            MirrorAxis::Y => Mirror::Y,
        }
    }
}

impl From<Mirror> for MirrorAxis {
    fn from(mirror: Mirror) -> MirrorAxis {
        match mirror {
            Mirror::X => MirrorAxis::X,
            Mirror::Y => MirrorAxis::Y,
        }
    }
}

/// A symbol placed on the schematic, as the canvas draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SymbolInstance {
    pub uuid: Uuid,
    pub lib_id: String,
    #[ts(type = "number")]
    pub x_nm: i64,
    #[ts(type = "number")]
    pub y_nm: i64,
    /// Tenths of a degree, counterclockwise, in `[0, 3600)`.
    pub rotation: i32,
    pub mirror: Option<MirrorAxis>,
    /// Absent when the symbol has no such property.
    pub reference: Option<SymbolText>,
    pub value: Option<SymbolText>,
    /// Where each pin connects, hidden pins included, in schematic nanometers.
    pub pins: Vec<Xy>,
}

impl SymbolInstance {
    fn new(symbol: &PlacedSymbol, schematic: &Schematic) -> SymbolInstance {
        let pins: Vec<Xy> = match schematic.lib_symbol(&symbol.lib_id) {
            Some(definition) => match symbol.pin_connections(definition) {
                Ok(pins) => pins.into_iter().map(|pin| pin.position.into()).collect(),
                Err(error) => {
                    eprintln!("could not read the pins of {:?}: {error}", symbol.lib_id);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        let text = |key: &str| {
            symbol
                .properties
                .iter()
                .find(|property| property.key == key)
                .map(|property| SymbolText {
                    text: property.value.clone(),
                    x_nm: property.position.x.0,
                    y_nm: property.position.y.0,
                    rotation: property.rotation.decidegrees(),
                    visible: !property.hidden,
                })
        };
        SymbolInstance {
            uuid: symbol.uuid,
            lib_id: symbol.lib_id.clone(),
            x_nm: symbol.position.x.0,
            y_nm: symbol.position.y.0,
            rotation: symbol.rotation.decidegrees(),
            mirror: symbol.mirror.map(MirrorAxis::from),
            reference: text("Reference"),
            value: text("Value"),
            pins,
        }
    }
}

/// One straight wire segment, as the canvas draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WireSegment {
    pub uuid: Uuid,
    pub start: Xy,
    pub end: Xy,
}

impl From<&Wire> for WireSegment {
    fn from(wire: &Wire) -> WireSegment {
        WireSegment {
            uuid: wire.uuid,
            start: Xy::from(wire.start),
            end: Xy::from(wire.end),
        }
    }
}

/// A junction dot where wires meet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JunctionDot {
    pub uuid: Uuid,
    pub position: Xy,
}

impl From<&Junction> for JunctionDot {
    fn from(junction: &Junction) -> JunctionDot {
        JunctionDot {
            uuid: junction.uuid,
            position: Xy::from(junction.position),
        }
    }
}

/// One field of a placed symbol, as the properties dialog edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SymbolField {
    pub name: String,
    pub value: String,
    pub visible: bool,
}

impl From<FieldEdit> for SymbolField {
    fn from(field: FieldEdit) -> SymbolField {
        SymbolField {
            name: field.name,
            value: field.value,
            visible: field.visible,
        }
    }
}

impl From<SymbolField> for FieldEdit {
    fn from(field: SymbolField) -> FieldEdit {
        FieldEdit {
            name: field.name,
            value: field.value,
            visible: field.visible,
        }
    }
}

/// A no-connect flag, drawn as an X over the point it marks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NoConnectFlag {
    pub uuid: Uuid,
    pub position: Xy,
}

impl From<&NoConnect> for NoConnectFlag {
    fn from(flag: &NoConnect) -> NoConnectFlag {
        NoConnectFlag {
            uuid: flag.uuid,
            position: Xy::from(flag.position),
        }
    }
}

/// The outline of a global label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlobalLabelShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
}

impl From<GlobalLabelShape> for LabelShape {
    fn from(shape: GlobalLabelShape) -> LabelShape {
        match shape {
            GlobalLabelShape::Input => LabelShape::Input,
            GlobalLabelShape::Output => LabelShape::Output,
            GlobalLabelShape::Bidirectional => LabelShape::Bidirectional,
            GlobalLabelShape::TriState => LabelShape::TriState,
            GlobalLabelShape::Passive => LabelShape::Passive,
        }
    }
}

impl From<LabelShape> for GlobalLabelShape {
    fn from(shape: LabelShape) -> GlobalLabelShape {
        match shape {
            LabelShape::Input => GlobalLabelShape::Input,
            LabelShape::Output => GlobalLabelShape::Output,
            LabelShape::Bidirectional => GlobalLabelShape::Bidirectional,
            LabelShape::TriState => GlobalLabelShape::TriState,
            LabelShape::Passive => GlobalLabelShape::Passive,
        }
    }
}

/// A local or global net label, as the canvas draws it. It connects at `position`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NetLabel {
    pub uuid: Uuid,
    pub text: String,
    /// The outline of a global label, or `null` for a local one.
    pub shape: Option<GlobalLabelShape>,
    pub position: Xy,
    /// Tenths of a degree, counterclockwise, a multiple of 900.
    pub rotation: i32,
}

impl From<&Label> for NetLabel {
    fn from(label: &Label) -> NetLabel {
        let shape = match label.kind {
            LabelKind::Local => None,
            LabelKind::Global(shape) => Some(GlobalLabelShape::from(shape)),
        };
        NetLabel {
            uuid: label.uuid,
            text: label.text.clone(),
            shape,
            position: Xy::from(label.position),
            rotation: label.rotation.decidegrees(),
        }
    }
}

/// The `place_label` argument: the name, the kind, and where the anchor goes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LabelPlacement {
    pub text: String,
    /// The outline of a global label, or `null` for a local one.
    pub shape: Option<GlobalLabelShape>,
    #[ts(type = "number")]
    pub x_nm: i64,
    #[ts(type = "number")]
    pub y_nm: i64,
    /// Rotation in 90° steps counterclockwise on screen.
    pub rotation: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectProperties {
    pub title_block: TitleBlockFields,
    #[ts(type = "number")]
    pub grid_size_nm: i64,
}

/// The `place_symbol` arguments: which library symbol to place, where, and how oriented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SymbolPlacement {
    /// The symbol's ID in the library index, from a search hit.
    #[ts(type = "number")]
    pub library_id: i64,
    /// The symbol origin in schematic coordinates.
    #[ts(type = "number")]
    pub x_nm: i64,
    #[ts(type = "number")]
    pub y_nm: i64,
    /// Rotation in 90° steps counterclockwise on screen (the KiCad angle convention).
    pub rotation: i32,
    /// A flip applied before the rotation.
    pub mirror: Option<MirrorAxis>,
}

/// The `transform_symbols` argument, applied to each symbol about its own origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum SymbolTransform {
    /// Quarter turns counterclockwise on screen (the KiCad angle convention).
    Rotate {
        steps: i32,
    },
    Mirror {
        axis: MirrorAxis,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub exists: bool,
}

#[derive(Default)]
pub struct AppState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    session: Option<Session>,
    /// Increases on every change, across projects, so a diff meant for one
    /// project can never apply to another.
    revision: u32,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_dirty(&self) -> bool {
        self.lock().session.as_ref().is_some_and(Session::is_dirty)
    }
}

impl Inner {
    fn session_mut(&mut self) -> Result<&mut Session, AppError> {
        self.session.as_mut().ok_or_else(AppError::no_project)
    }

    /// Bumps the revision and tells the UI what changed.
    fn publish(&mut self, app: &AppHandle, changes: Changes) -> Result<(), AppError> {
        let base_revision = self.revision;
        self.revision = self.revision.wrapping_add(1);
        let session = self.session.as_ref().ok_or_else(AppError::no_project)?;
        let diff = document_diff(session, base_revision, self.revision, changes);
        app.emit(DOCUMENT_CHANGED_EVENT, &diff)
            .map_err(AppError::window)?;
        shell::sync_session(app, Some(session)).map_err(AppError::window)
    }

    /// Makes `session` the open project and returns its first snapshot.
    fn open(&mut self, app: &AppHandle, session: Session) -> Result<ProjectSnapshot, AppError> {
        self.revision = self.revision.wrapping_add(1);
        let snapshot = ProjectSnapshot::new(&session, self.revision);
        self.session = Some(session);
        shell::sync_session(app, self.session.as_ref()).map_err(AppError::window)?;
        Ok(snapshot)
    }
}

pub fn recent_projects(app: &AppHandle) -> Result<Vec<RecentProject>, AppError> {
    let settings = Settings::read(&settings::settings_path(app)?)?;
    Ok(settings
        .recent_projects
        .iter()
        .map(|dir| RecentProject {
            path: dir.display().to_string(),
            name: dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.display().to_string()),
            exists: dir.is_dir(),
        })
        .collect())
}

fn update_recent(
    app: &AppHandle,
    change: impl FnOnce(&mut Settings),
) -> Result<Vec<RecentProject>, AppError> {
    let path = settings::settings_path(app)?;
    let mut settings = Settings::read(&path)?;
    change(&mut settings);
    settings.write(&path)?;
    let recent = recent_projects(app)?;
    shell::rebuild_recent(app, &recent).map_err(AppError::window)?;
    Ok(recent)
}

/// Adds a project to the recent list. The list is a convenience: if it can't
/// be updated, the project still opens, and the welcome screen reports the
/// settings error the next time it loads the list.
fn remember_recent(app: &AppHandle, dir: &Path) {
    let _ = update_recent(app, |settings| settings.remember(dir));
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo::from(app.package_info())
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>) -> Option<ProjectSnapshot> {
    let inner = state.lock();
    inner
        .session
        .as_ref()
        .map(|session| ProjectSnapshot::new(session, inner.revision))
}

#[tauri::command]
pub fn create_project(
    app: AppHandle,
    state: State<'_, AppState>,
    parent: String,
    name: String,
) -> Result<ProjectSnapshot, AppError> {
    let project = Project::create(Path::new(&parent), &name, Uuid::new_v4())?;
    let dir = project.dir().to_path_buf();
    let snapshot = state.lock().open(&app, Session::new(project))?;
    remember_recent(&app, &dir);
    Ok(snapshot)
}

#[tauri::command]
pub fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ProjectSnapshot, AppError> {
    let project = Project::load(Path::new(&path))?;
    let dir = project.dir().to_path_buf();
    let snapshot = state.lock().open(&app, Session::new(project))?;
    remember_recent(&app, &dir);
    Ok(snapshot)
}

#[tauri::command]
pub fn save_project(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut inner = state.lock();
    inner.session_mut()?.save()?;
    inner.publish(&app, Changes::default())
}

#[tauri::command]
pub fn save_project_as(
    app: AppHandle,
    state: State<'_, AppState>,
    parent: String,
    name: String,
) -> Result<ProjectSnapshot, AppError> {
    let (snapshot, dir) = {
        let mut inner = state.lock();
        let session = inner.session_mut()?;
        session.save_as(Path::new(&parent), &name)?;
        let dir = session.project().dir().to_path_buf();
        inner.revision = inner.revision.wrapping_add(1);
        let session = inner.session.as_ref().ok_or_else(AppError::no_project)?;
        let snapshot = ProjectSnapshot::new(session, inner.revision);
        shell::sync_session(&app, Some(session)).map_err(AppError::window)?;
        (snapshot, dir)
    };
    remember_recent(&app, &dir);
    Ok(snapshot)
}

#[tauri::command]
pub fn close_project(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut inner = state.lock();
    inner.session = None;
    inner.revision = inner.revision.wrapping_add(1);
    shell::sync_session(&app, None).map_err(AppError::window)
}

#[tauri::command]
pub fn edit_project_properties(
    app: AppHandle,
    state: State<'_, AppState>,
    properties: ProjectProperties,
) -> Result<(), AppError> {
    let title_block = TitleBlock::try_from(properties.title_block)?;
    let mut inner = state.lock();
    let changes = inner
        .session_mut()?
        .edit_properties(title_block, Nm(properties.grid_size_nm))?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

#[tauri::command]
pub fn undo(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut inner = state.lock();
    let changes = inner.session_mut()?.undo()?;
    inner.publish(&app, changes)
}

#[tauri::command]
pub fn redo(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut inner = state.lock();
    let changes = inner.session_mut()?.redo()?;
    inner.publish(&app, changes)
}

/// Places a library symbol on the schematic as one undo step. The definition comes from
/// the cached library index, so raw KiCad text never crosses IPC.
#[tauri::command]
pub fn place_symbol(
    app: AppHandle,
    state: State<'_, AppState>,
    library: State<'_, LibraryState>,
    placement: SymbolPlacement,
) -> Result<(), AppError> {
    let definition = read_index(&app, &library, |index| {
        index.symbol_definition(placement.library_id)
    })?;
    let (lib_id, definition) = named_definition(definition, placement.library_id)?;
    let pin_count = tron_core::symbol::definition_pin_numbers(&definition).len();
    let pin_uuids: Vec<Uuid> = (0..pin_count).map(|_| Uuid::new_v4()).collect();
    let placement = Placement {
        lib_id: &lib_id,
        position: Point::new(Nm(placement.x_nm), Nm(placement.y_nm)),
        rotation_steps: placement.rotation,
        mirror: placement.mirror.map(Mirror::from),
        uuid: Uuid::new_v4(),
        pin_uuids: &pin_uuids,
    };
    let mut inner = state.lock();
    let changes = inner.session_mut()?.place_symbol(&definition, placement)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// The `<lib>:<name>` and node of the definition a library symbol ID resolves to.
fn named_definition(
    definition: Option<SExpr>,
    library_id: i64,
) -> Result<(String, SExpr), AppError> {
    let Some(definition) = definition else {
        return Err(AppError::new(
            AppErrorKind::Library,
            "The symbol is not in the component library.",
            Some(format!("The index has no symbol with ID {library_id}.")),
        ));
    };
    let Some(lib_id) = text_arg(&definition, 0).map(str::to_owned) else {
        return Err(AppError::new(
            AppErrorKind::Library,
            "The component library returned a malformed symbol definition.",
            Some(format!("The symbol with ID {library_id} has no name.")),
        ));
    };
    Ok((lib_id, definition))
}

/// Moves every symbol in `uuids` by the same offset as one undo step.
#[tauri::command]
pub fn move_symbols(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
    dx_nm: i64,
    dy_nm: i64,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let changes = inner
        .session_mut()?
        .move_symbols(&uuids, Nm(dx_nm), Nm(dy_nm))?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Rotates or mirrors every symbol in `uuids` about its own origin as one undo step.
#[tauri::command]
pub fn transform_symbols(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
    transform: SymbolTransform,
) -> Result<(), AppError> {
    let transform = match transform {
        SymbolTransform::Rotate { steps } => tron_core::SymbolTransform::Rotate(steps),
        SymbolTransform::Mirror { axis } => tron_core::SymbolTransform::Mirror(axis.into()),
    };
    let mut inner = state.lock();
    let changes = inner.session_mut()?.transform_symbols(&uuids, transform)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Deletes every symbol in `uuids` as one undo step. Definitions no longer used by any
/// remaining symbol leave the schematic within the same step.
#[tauri::command]
pub fn delete_symbols(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let changes = inner.session_mut()?.delete_symbols(&uuids)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

fn to_point(Xy(x, y): Xy) -> Point {
    Point::new(Nm(x), Nm(y))
}

/// Draws a wire along `points` as one undo step. Each segment must be horizontal or
/// vertical.
#[tauri::command]
pub fn draw_wire(
    app: AppHandle,
    state: State<'_, AppState>,
    points: Vec<Xy>,
) -> Result<(), AppError> {
    let points: Vec<Point> = points.into_iter().map(to_point).collect();
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.draw_wire(&points, &mut Uuid::new_v4)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Places a local or global label as one undo step.
#[tauri::command]
pub fn place_label(
    app: AppHandle,
    state: State<'_, AppState>,
    label: LabelPlacement,
) -> Result<(), AppError> {
    let kind = match label.shape {
        None => LabelKind::Local,
        Some(shape) => LabelKind::Global(shape.into()),
    };
    let position = Point::new(Nm(label.x_nm), Nm(label.y_nm));
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.place_label(
        kind,
        &label.text,
        position,
        label.rotation,
        &mut Uuid::new_v4,
    )?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// The fields of a placed symbol: the mandatory ones first, then the rest in file order.
#[tauri::command]
pub fn get_symbol_fields(
    state: State<'_, AppState>,
    uuid: Uuid,
) -> Result<Vec<SymbolField>, AppError> {
    let mut inner = state.lock();
    let fields = inner.session_mut()?.symbol_fields(uuid)?;
    Ok(fields.into_iter().map(SymbolField::from).collect())
}

/// Replaces the fields of a placed symbol, in order, as one undo step.
#[tauri::command]
pub fn edit_symbol_fields(
    app: AppHandle,
    state: State<'_, AppState>,
    uuid: Uuid,
    fields: Vec<SymbolField>,
) -> Result<(), AppError> {
    let fields: Vec<FieldEdit> = fields.into_iter().map(FieldEdit::from).collect();
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.edit_symbol_fields(uuid, &fields)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Sets one field's value on a placed symbol as one undo step.
#[tauri::command]
pub fn set_symbol_field(
    app: AppHandle,
    state: State<'_, AppState>,
    uuid: Uuid,
    name: String,
    value: String,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.set_symbol_field(uuid, &name, &value)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Places a no-connect flag at a point as one undo step.
#[tauri::command]
pub fn place_no_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    position: Xy,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.place_no_connect(to_point(position), &mut Uuid::new_v4)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Numbers every unannotated symbol as one undo step and returns how many changed.
#[tauri::command]
pub fn annotate_schematic(app: AppHandle, state: State<'_, AppState>) -> Result<u32, AppError> {
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let (changes, count) = session.annotate();
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Moves the wires, labels, and no-connect flags in `uuids` by the same offset as one
/// undo step.
#[tauri::command]
pub fn move_wiring(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
    dx_nm: i64,
    dy_nm: i64,
) -> Result<(), AppError> {
    let (dx, dy) = (Nm(dx_nm), Nm(dy_nm));
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.move_wiring(&uuids, dx, dy, &mut Uuid::new_v4)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Rotates the labels in `uuids` about their anchors as one undo step.
#[tauri::command]
pub fn rotate_labels(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
    steps: i32,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.rotate_labels(&uuids, steps, &mut Uuid::new_v4)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

/// Deletes the wires, labels, and no-connect flags in `uuids`, with junctions no longer
/// needed, as one undo step.
#[tauri::command]
pub fn delete_wiring(
    app: AppHandle,
    state: State<'_, AppState>,
    uuids: Vec<Uuid>,
) -> Result<(), AppError> {
    let mut inner = state.lock();
    let session = inner.session_mut()?;
    let changes = session.delete_wiring(&uuids, &mut Uuid::new_v4)?;
    if changes.any() {
        inner.publish(&app, changes)?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_recent_projects(app: AppHandle) -> Result<Vec<RecentProject>, AppError> {
    recent_projects(&app)
}

#[tauri::command]
pub fn remove_recent_project(app: AppHandle, path: String) -> Result<Vec<RecentProject>, AppError> {
    update_recent(&app, |settings| settings.forget(Path::new(&path)))
}

/// Closes the window without the unsaved-changes check. The UI calls this
/// only after the user has answered the prompt.
#[tauri::command]
pub fn close_window(window: Window) -> Result<(), AppError> {
    window.destroy().map_err(AppError::window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tron_library::Index;

    fn session() -> Session {
        Session::new(Project::new(PathBuf::from("amp"), "amp", Uuid::nil()).unwrap())
    }

    /// A real index built from the KiCad fixtures, the same one the library tests use.
    fn fixture_index(dir: &Path) -> Index {
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/library/tests/fixtures/kicad");
        let built = dir.join("index.db");
        tron_library::build_index(
            &fixtures.join("symbols"),
            &fixtures.join("footprints"),
            "10.0.7",
            &built,
        )
        .unwrap();
        Index::open(&built).unwrap()
    }

    fn resistor_definition(index: &Index) -> (String, SExpr) {
        let hit = index.search("R", 1).unwrap().remove(0);
        let definition = index.symbol_definition(hit.id).unwrap();
        named_definition(definition, hit.id).unwrap()
    }

    /// Places the fixture resistor through the same path the command uses.
    fn place_resistor(session: &mut Session, index: &Index) -> Changes {
        let (lib_id, definition) = resistor_definition(index);
        let pin_count = tron_core::symbol::definition_pin_numbers(&definition).len();
        let pin_uuids: Vec<Uuid> = (0..pin_count).map(|_| Uuid::new_v4()).collect();
        session
            .place_symbol(
                &definition,
                Placement {
                    lib_id: &lib_id,
                    position: Point::new(Nm(100_000_000), Nm(50_000_000)),
                    rotation_steps: 0,
                    mirror: None,
                    uuid: Uuid::new_v4(),
                    pin_uuids: &pin_uuids,
                },
            )
            .unwrap()
    }

    #[test]
    fn app_info_serializes_with_the_ipc_field_names() {
        let info = AppInfo {
            name: "Tron".to_owned(),
            version: "0.1.0".to_owned(),
        };
        let json = serde_json::to_value(&info).expect("AppInfo serializes");
        assert_eq!(
            json,
            serde_json::json!({ "name": "Tron", "version": "0.1.0" })
        );
    }

    #[test]
    fn a_snapshot_describes_a_new_project() {
        let json = serde_json::to_value(ProjectSnapshot::new(&session(), 7)).unwrap();
        assert_eq!(json["revision"], 7);
        assert_eq!(json["name"], "amp");
        assert_eq!(json["gridSizeNm"], 1_270_000);
        assert_eq!(json["sheet"]["widthNm"], 297_000_000);
        assert_eq!(json["sheet"]["heightNm"], 210_000_000);
        assert_eq!(json["titleBlock"]["comments"].as_array().unwrap().len(), 9);
        assert_eq!(json["definitions"], serde_json::json!({}));
        assert_eq!(json["instances"], serde_json::json!([]));
        assert_eq!(
            json["history"],
            serde_json::json!({
                "dirty": false,
                "canUndo": false,
                "canRedo": false,
                "undoLabel": null,
                "redoLabel": null
            })
        );
    }

    #[test]
    fn title_block_fields_convert_both_ways() {
        let mut block = TitleBlock {
            title: "Amp".to_owned(),
            ..TitleBlock::default()
        };
        block.comments[8] = "Last".to_owned();
        let fields = TitleBlockFields::from(&block);
        assert_eq!(TitleBlock::try_from(fields).unwrap(), block);
    }

    #[test]
    fn title_block_fields_need_nine_comments() {
        let fields = TitleBlockFields {
            title: String::new(),
            date: String::new(),
            revision: String::new(),
            company: String::new(),
            comments: vec![String::new(); 3],
        };
        let error = TitleBlock::try_from(fields).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::InvalidEdit);
    }

    #[test]
    fn properties_deserialize_from_the_ui_payload() {
        let payload = serde_json::json!({
            "titleBlock": {
                "title": "Amp",
                "date": "",
                "revision": "A",
                "company": "",
                "comments": ["", "", "", "", "", "", "", "", ""]
            },
            "gridSizeNm": 2_540_000
        });
        let properties: ProjectProperties = serde_json::from_value(payload).unwrap();
        assert_eq!(properties.grid_size_nm, 2_540_000);
        assert_eq!(properties.title_block.revision, "A");
    }

    #[test]
    fn symbol_payloads_deserialize_from_the_ui_arguments() {
        let payload = serde_json::json!({
            "libraryId": 42,
            "xNm": 100_000_000,
            "yNm": 50_000_000,
            "rotation": 1,
            "mirror": "x"
        });
        let placement: SymbolPlacement = serde_json::from_value(payload).unwrap();
        assert_eq!(placement.library_id, 42);
        assert_eq!(placement.rotation, 1);
        assert_eq!(placement.mirror, Some(MirrorAxis::X));

        let rotate: SymbolTransform =
            serde_json::from_value(serde_json::json!({ "type": "rotate", "steps": 3 })).unwrap();
        assert_eq!(rotate, SymbolTransform::Rotate { steps: 3 });
        let mirror: SymbolTransform =
            serde_json::from_value(serde_json::json!({ "type": "mirror", "axis": "y" })).unwrap();
        assert_eq!(
            mirror,
            SymbolTransform::Mirror {
                axis: MirrorAxis::Y
            }
        );
    }

    #[test]
    fn a_library_symbol_resolves_to_its_named_definition() {
        let temp = tempfile::tempdir().unwrap();
        let index = fixture_index(temp.path());
        let (lib_id, definition) = resistor_definition(&index);
        assert_eq!(lib_id, "Device:R");
        assert_eq!(definition.head(), Some("symbol"));
    }

    #[test]
    fn an_unknown_library_symbol_is_a_clear_error() {
        let temp = tempfile::tempdir().unwrap();
        let index = fixture_index(temp.path());
        let definition = index.symbol_definition(9_999).unwrap();
        let error = named_definition(definition, 9_999).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::Library);
        assert!(
            error.message.contains("not in the component library"),
            "{error:?}"
        );
    }

    #[test]
    fn a_snapshot_carries_instances_and_definitions() {
        let temp = tempfile::tempdir().unwrap();
        let index = fixture_index(temp.path());
        let mut session = session();
        place_resistor(&mut session, &index);
        let json = serde_json::to_value(ProjectSnapshot::new(&session, 3)).unwrap();

        let instance = &json["instances"][0];
        assert_eq!(instance["libId"], "Device:R");
        assert_eq!(instance["xNm"], 100_000_000);
        assert_eq!(instance["yNm"], 50_000_000);
        assert_eq!(instance["rotation"], 0);
        assert_eq!(instance["mirror"], serde_json::Value::Null);
        assert_eq!(instance["uuid"].as_str().unwrap().len(), 36);
        assert_eq!(
            instance["reference"],
            serde_json::json!({
                "text": "R?",
                "xNm": 102_032_000,
                "yNm": 50_000_000,
                "rotation": 900,
                "visible": true
            })
        );
        assert_eq!(instance["value"]["text"], "R");
        assert_eq!(instance["value"]["visible"], true);

        let preview = &json["definitions"]["Device:R"];
        assert_eq!(preview["pins"].as_array().unwrap().len(), 2);
        assert!(preview["shapes"].is_array());
        assert_eq!(instance["pins"].as_array().unwrap().len(), 2);
        assert_eq!(json["wires"], serde_json::json!([]));
        assert_eq!(json["labels"], serde_json::json!([]));
        assert_eq!(json["noConnects"], serde_json::json!([]));
    }

    #[test]
    fn symbol_fields_round_trip_through_the_ui_payload() {
        let payload = serde_json::json!({ "name": "MPN", "value": "RC0603", "visible": true });
        let field: SymbolField = serde_json::from_value(payload.clone()).unwrap();
        let edit = FieldEdit::from(field.clone());
        assert_eq!((edit.name.as_str(), edit.visible), ("MPN", true));
        let json = serde_json::to_value(SymbolField::from(edit)).unwrap();
        assert_eq!(json, payload);
    }

    #[test]
    fn a_diff_carries_no_connect_flags() {
        let mut session = session();
        let at = Point::new(Nm(2_540_000), Nm(1_270_000));
        let changes = session.place_no_connect(at, &mut Uuid::new_v4).unwrap();
        let json = serde_json::to_value(document_diff(&session, 1, 2, changes)).unwrap();
        let flag = &json["noConnects"][0];
        assert_eq!(flag["position"], serde_json::json!([2_540_000, 1_270_000]));
        assert_eq!(flag["uuid"].as_str().unwrap().len(), 36);
        assert_eq!(json["history"]["undoLabel"], "Place No-Connect Flag");
    }

    #[test]
    fn a_diff_carries_the_wiring_when_it_changed() {
        let mut session = session();
        let points = [Point::ORIGIN, Point::new(Nm(2_540_000), Nm::ZERO)];
        let changes = session.draw_wire(&points, &mut Uuid::new_v4).unwrap();
        let json = serde_json::to_value(document_diff(&session, 1, 2, changes)).unwrap();
        assert_eq!(json["wires"][0]["start"], serde_json::json!([0, 0]));
        assert_eq!(json["wires"][0]["end"], serde_json::json!([2_540_000, 0]));
        assert_eq!(json["junctions"], serde_json::json!([]));
        assert_eq!(json["instances"], serde_json::Value::Null);

        let kind = LabelKind::Global(LabelShape::TriState);
        let at = Point::new(Nm(2_540_000), Nm::ZERO);
        let changes = session
            .place_label(kind, "VIN", at, 1, &mut Uuid::new_v4)
            .unwrap();
        let json = serde_json::to_value(document_diff(&session, 2, 3, changes)).unwrap();
        let label = &json["labels"][0];
        assert_eq!(label["text"], "VIN");
        assert_eq!(label["shape"], "triState");
        assert_eq!(label["position"], serde_json::json!([2_540_000, 0]));
        assert_eq!(label["rotation"], 900);
    }

    #[test]
    fn label_payloads_deserialize_from_the_ui_arguments() {
        let payload = serde_json::json!({
            "text": "OUT",
            "shape": null,
            "xNm": 1_270_000,
            "yNm": 2_540_000,
            "rotation": 2
        });
        let label: LabelPlacement = serde_json::from_value(payload).unwrap();
        assert_eq!(label.shape, None);
        assert_eq!((label.x_nm, label.rotation), (1_270_000, 2));
        let input = serde_json::json!("input");
        let shape: GlobalLabelShape = serde_json::from_value(input).unwrap();
        assert_eq!(shape, GlobalLabelShape::Input);
    }

    #[test]
    fn a_diff_carries_only_the_parts_that_changed() {
        let temp = tempfile::tempdir().unwrap();
        let index = fixture_index(temp.path());
        let mut session = session();
        let changes = place_resistor(&mut session, &index);
        let json = serde_json::to_value(document_diff(&session, 1, 2, changes)).unwrap();
        assert_eq!(json["baseRevision"], 1);
        assert_eq!(json["revision"], 2);
        assert!(json["instances"].is_array());
        assert!(json["definitions"]["Device:R"].is_object());
        assert_eq!(json["titleBlock"], serde_json::Value::Null);

        let uuid = session.project().schematic().symbols()[0].uuid;
        let changes = session
            .move_symbols(&[uuid], Nm(1_270_000), Nm::ZERO)
            .unwrap();
        let json = serde_json::to_value(document_diff(&session, 2, 3, changes)).unwrap();
        assert_eq!(json["instances"][0]["xNm"], 101_270_000);
        assert_eq!(json["definitions"], serde_json::Value::Null);

        let changes = session
            .transform_symbols(&[uuid], tron_core::SymbolTransform::Rotate(1))
            .unwrap();
        let json = serde_json::to_value(document_diff(&session, 3, 4, changes)).unwrap();
        assert_eq!(json["instances"][0]["rotation"], 900);

        let changes = session
            .transform_symbols(&[uuid], tron_core::SymbolTransform::Mirror(Mirror::Y))
            .unwrap();
        let json = serde_json::to_value(document_diff(&session, 4, 5, changes)).unwrap();
        assert_eq!(json["instances"][0]["mirror"], "y");

        let changes = session.delete_symbols(&[uuid]).unwrap();
        let json = serde_json::to_value(document_diff(&session, 5, 6, changes)).unwrap();
        assert_eq!(json["instances"], serde_json::json!([]));
        assert_eq!(json["definitions"], serde_json::json!({}));
    }
}
