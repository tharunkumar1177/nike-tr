mod error;
mod ipc;
mod library;
mod settings;
mod shell;

use tauri::{Emitter, Manager, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ipc::AppState::default())
        .setup(|app| {
            let handle = app.handle();
            let (menu, handles) = shell::build_menu(handle)?;
            app.set_menu(menu)?;
            app.manage(handles);
            // A damaged settings file leaves the menu's recent list empty; the
            // welcome screen shows the error when it loads the same list.
            let recent = ipc::recent_projects(handle).unwrap_or_default();
            shell::rebuild_recent(handle, &recent)?;
            app.manage(library::LibraryState::load(handle));
            library::resume(handle);
            Ok(())
        })
        .on_menu_event(|app, event| {
            if let Err(error) = app.emit(shell::MENU_EVENT, event.id().0.as_str()) {
                eprintln!("could not deliver menu action {}: {error}", event.id().0);
            }
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.state::<ipc::AppState>().is_dirty()
            {
                api.prevent_close();
                if let Err(error) = window.emit(shell::CLOSE_REQUESTED_EVENT, ()) {
                    eprintln!("could not ask to save before closing: {error}");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_app_info,
            ipc::get_project,
            ipc::create_project,
            ipc::open_project,
            ipc::save_project,
            ipc::save_project_as,
            ipc::close_project,
            ipc::edit_project_properties,
            ipc::undo,
            ipc::redo,
            ipc::place_symbol,
            ipc::move_symbols,
            ipc::transform_symbols,
            ipc::delete_symbols,
            ipc::draw_wire,
            ipc::place_label,
            ipc::move_wiring,
            ipc::rotate_labels,
            ipc::delete_wiring,
            ipc::place_no_connect,
            ipc::annotate_schematic,
            ipc::run_erc,
            ipc::get_symbol_fields,
            ipc::edit_symbol_fields,
            ipc::set_symbol_field,
            ipc::list_recent_projects,
            ipc::remove_recent_project,
            ipc::close_window,
            library::get_library_status,
            library::download_library,
            library::decline_library_download,
            library::cancel_library_download,
            library::search_library,
            library::get_library_symbol,
            library::get_library_footprint,
        ])
        .run(tauri::generate_context!())
        .expect("the Tron desktop runtime failed to start");
}
