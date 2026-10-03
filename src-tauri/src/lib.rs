//! FileForge desktop shell: wires plugins, state, IPC commands and window events. Processing lives in
//! `fileforge-core`.

mod commands;
mod drag_drop;
mod error;
mod file_registry;

// Types
use crate::file_registry::FileRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(FileRegistry::default())
        .invoke_handler(tauri::generate_handler![commands::pick_files, commands::remove_file])
        .on_window_event(drag_drop::handle_window_event)
        .run(tauri::generate_context!())
        .expect("Tauri must start: the bundled config and capabilities are validated at build time");
}
