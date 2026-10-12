//! The native menu bar and window title, kept in step with the open project.

use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Manager, Wry};
use tron_core::Session;

use crate::ipc::RecentProject;

/// Emitted with the menu item id whenever the user picks a menu item.
pub const MENU_EVENT: &str = "menu-action";
/// Emitted when the user closes the window while the project has unsaved changes.
pub const CLOSE_REQUESTED_EVENT: &str = "close-requested";
pub const OPEN_RECENT_PREFIX: &str = "open-recent:";
const MAIN_WINDOW: &str = "main";
const APP_NAME: &str = "Tron";

/// Menu items whose state depends on the open project.
pub struct MenuHandles {
    project_items: Vec<MenuItem<Wry>>,
    undo: MenuItem<Wry>,
    redo: MenuItem<Wry>,
    recent: Submenu<Wry>,
}

fn item(
    app: &AppHandle,
    id: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
) -> tauri::Result<MenuItem<Wry>> {
    let mut builder = MenuItemBuilder::with_id(id, text).enabled(enabled);
    if let Some(accelerator) = accelerator {
        builder = builder.accelerator(accelerator);
    }
    builder.build(app)
}

pub fn build_menu(app: &AppHandle) -> tauri::Result<(Menu<Wry>, MenuHandles)> {
    let new_project = item(app, "new-project", "&New Project...", Some("CmdOrCtrl+N"), true)?;
    let open_project = item(app, "open-project", "&Open Project...", Some("CmdOrCtrl+O"), true)?;
    let recent = SubmenuBuilder::new(app, "Open &Recent").build()?;
    let save = item(app, "save", "&Save", Some("CmdOrCtrl+S"), false)?;
    let save_as = item(app, "save-as", "Save &As...", Some("CmdOrCtrl+Shift+S"), false)?;
    let properties = item(app, "project-properties", "Project &Properties...", None, false)?;
    let close_project = item(app, "close-project", "&Close Project", Some("CmdOrCtrl+W"), false)?;
    let exit = item(app, "exit", "E&xit", None, true)?;
    let undo = item(app, "undo", "&Undo", Some("CmdOrCtrl+Z"), false)?;
    let redo = item(app, "redo", "&Redo", Some("CmdOrCtrl+Y"), false)?;
    let zoom_in = item(app, "zoom-in", "Zoom &In", Some("CmdOrCtrl+="), false)?;
    let zoom_out = item(app, "zoom-out", "Zoom &Out", Some("CmdOrCtrl+-"), false)?;
    let zoom_fit = item(app, "zoom-fit", "Zoom to &Fit", Some("CmdOrCtrl+0"), false)?;
    let zoom_actual = item(app, "zoom-actual", "&Actual Size", Some("CmdOrCtrl+1"), false)?;

    let file = SubmenuBuilder::new(app, "&File")
        .item(&new_project)
        .item(&open_project)
        .item(&recent)
        .separator()
        .item(&save)
        .item(&save_as)
        .separator()
        .item(&properties)
        .separator()
        .item(&close_project)
        .item(&exit)
        .build()?;
    let edit = SubmenuBuilder::new(app, "&Edit")
        .item(&undo)
        .item(&redo)
        .build()?;
    let view = SubmenuBuilder::new(app, "&View")
        .item(&zoom_in)
        .item(&zoom_out)
        .separator()
        .item(&zoom_fit)
        .item(&zoom_actual)
        .build()?;
    let menu = MenuBuilder::new(app)
        .item(&file)
        .item(&edit)
        .item(&view)
        .build()?;

    let handles = MenuHandles {
        project_items: vec![
            save,
            save_as,
            properties,
            close_project,
            zoom_in,
            zoom_out,
            zoom_fit,
            zoom_actual,
        ],
        undo,
        redo,
        recent,
    };
    Ok((menu, handles))
}

pub fn window_title(session: Option<&Session>) -> String {
    match session {
        Some(session) if session.is_dirty() => {
            format!("*{} - {APP_NAME}", session.project().name())
        }
        Some(session) => format!("{} - {APP_NAME}", session.project().name()),
        None => APP_NAME.to_owned(),
    }
}

fn history_text(verb: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => format!("&{verb} {label}"),
        None => format!("&{verb}"),
    }
}

/// Updates menu enablement, undo and redo labels, and the window title.
pub fn sync_session(app: &AppHandle, session: Option<&Session>) -> tauri::Result<()> {
    let handles = app.state::<MenuHandles>();
    for item in &handles.project_items {
        item.set_enabled(session.is_some())?;
    }
    handles
        .undo
        .set_enabled(session.is_some_and(Session::can_undo))?;
    handles
        .undo
        .set_text(history_text("Undo", session.and_then(Session::undo_label)))?;
    handles
        .redo
        .set_enabled(session.is_some_and(Session::can_redo))?;
    handles
        .redo
        .set_text(history_text("Redo", session.and_then(Session::redo_label)))?;
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        window.set_title(&window_title(session))?;
    }
    Ok(())
}

/// Replaces the items of the Open Recent submenu.
pub fn rebuild_recent(app: &AppHandle, recent: &[RecentProject]) -> tauri::Result<()> {
    let handles = app.state::<MenuHandles>();
    while handles.recent.remove_at(0)?.is_some() {}
    if recent.is_empty() {
        let empty = MenuItemBuilder::with_id("no-recent-projects", "No Recent Projects")
            .enabled(false)
            .build(app)?;
        handles.recent.append(&empty)?;
    }
    for project in recent {
        // A single `&` marks a mnemonic in Windows menus.
        let label = project.path.replace('&', "&&");
        let entry = MenuItemBuilder::with_id(format!("{OPEN_RECENT_PREFIX}{}", project.path), label)
            .build(app)?;
        handles.recent.append(&entry)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tron_core::{Project, TitleBlock};
    use uuid::Uuid;

    #[test]
    fn the_window_title_shows_the_project_and_unsaved_changes() {
        assert_eq!(window_title(None), "Tron");
        let project = Project::new(PathBuf::from("amp"), "amp", Uuid::nil()).unwrap();
        let mut session = Session::new(project);
        assert_eq!(window_title(Some(&session)), "amp - Tron");
        let grid = session.project().grid_size();
        let block = TitleBlock {
            title: "Amp".to_owned(),
            ..TitleBlock::default()
        };
        session.edit_properties(block, grid).unwrap();
        assert_eq!(window_title(Some(&session)), "*amp - Tron");
    }

    #[test]
    fn history_items_name_the_command() {
        assert_eq!(history_text("Undo", None), "&Undo");
        assert_eq!(
            history_text("Undo", Some("Project Properties")),
            "&Undo Project Properties"
        );
    }
}
