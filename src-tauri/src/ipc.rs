use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, PackageInfo, State, Window};
use tron_core::{Changes, Nm, Project, Session, TITLE_BLOCK_COMMENTS, TitleBlock};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{AppError, AppErrorKind};
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
            history: HistoryStatus::from(session),
        }
    }
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
    pub history: HistoryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectProperties {
    pub title_block: TitleBlockFields,
    #[ts(type = "number")]
    pub grid_size_nm: i64,
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
        let project = session.project();
        let diff = DocumentDiff {
            base_revision,
            revision: self.revision,
            title_block: changes
                .title_block
                .then(|| TitleBlockFields::from(&project.title_block())),
            grid_size_nm: changes.grid_size.then(|| project.grid_size().0),
            history: HistoryStatus::from(session),
        };
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
    use std::path::PathBuf;

    fn session() -> Session {
        Session::new(Project::new(PathBuf::from("amp"), "amp", Uuid::nil()).unwrap())
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
}
