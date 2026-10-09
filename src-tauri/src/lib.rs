mod ipc;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc::get_app_info])
        .run(tauri::generate_context!())
        .expect("the Tron desktop runtime failed to start");
}
