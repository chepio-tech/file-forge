//! Files dropped on the window are registered on the Rust side and announced to the webview by event, so the webview
//! never has to send paths back (ADR-0004).

// Core
use std::path::PathBuf;

use tauri::{DragDropEvent, Emitter, Manager, Runtime, Window, WindowEvent};
// Types
use crate::file_registry::FileRegistry;

/// Event carrying a [`crate::file_registry::RegisterOutcome`]. Mirrored in `src/services/fileforgeApi.ts`.
pub const FILES_ADDED_EVENT: &str = "files-added";

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
        register_dropped(window.app_handle().clone(), paths.clone());
    }
}

fn register_dropped<R: Runtime>(app: tauri::AppHandle<R>, paths: Vec<PathBuf>) {
    // Registration reads file headers; never do disk I/O on the event-loop thread.
    tauri::async_runtime::spawn_blocking(move || {
        let outcome = app.state::<FileRegistry>().register_all(paths);
        // Emitting only fails while the app is shutting down, when nobody is listening anyway.
        let _ = app.emit(FILES_ADDED_EVENT, outcome);
    });
}
