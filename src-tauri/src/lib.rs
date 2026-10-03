//! FileForge desktop shell: wires plugins, state, IPC commands and window events. Processing lives in
//! `fileforge-core`.

mod commands;
mod drag_drop;
mod error;
mod file_registry;
mod job_control;
mod results;

// Core
use tauri::{Manager, RunEvent};
// Types
use crate::file_registry::FileRegistry;
use crate::job_control::Cancellation;
use crate::results::ResultStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(FileRegistry::default())
        .manage(Cancellation::default())
        .setup(|app| {
            let dir = app.path().app_cache_dir()?.join("results");
            app.manage(ResultStore::open(dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pick_files,
            commands::remove_file,
            commands::compress_pdf,
            commands::cancel_compression,
            commands::save_result,
            commands::save_results_to_folder,
            commands::reveal_result,
        ])
        .on_window_event(drag_drop::handle_window_event)
        .build(tauri::generate_context!())
        .expect("Tauri must start: the bundled config and capabilities are validated at build time");

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(results) = handle.try_state::<ResultStore>()
        {
            results.clear();
        }
    });
}
