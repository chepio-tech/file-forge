//! Files dropped on the window are registered on the Rust side and announced to the webview by event, so the webview
//! never has to send paths back (ADR-0004). Dropped folders are searched for the kinds the active tool accepts
//! (ADR-0017).

// Core
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use fileforge_core::FileKind;
use tauri::{DragDropEvent, Emitter, Manager, Runtime, Window, WindowEvent};
// Types
use crate::file_registry::FileRegistry;

/// Event carrying a [`crate::file_registry::RegisterOutcome`]. Mirrored in `src/services/fileforgeApi.ts`.
pub const FILES_ADDED_EVENT: &str = "files-added";

/// Kinds of files the active tool accepts, set by `set_drop_kinds`. Only they are taken from dropped folders; until a
/// tool sets them, folders contribute nothing.
#[derive(Default)]
pub struct DropKinds(Mutex<Vec<FileKind>>);

impl DropKinds {
    pub fn set(&self, kinds: Vec<FileKind>) {
        // A plain value replaced as a whole: a poisoned lock still holds a valid list.
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = kinds;
    }

    pub fn get(&self) -> Vec<FileKind> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
        register_dropped(window.app_handle().clone(), paths.clone());
    }
}

fn register_dropped<R: Runtime>(app: tauri::AppHandle<R>, paths: Vec<PathBuf>) {
    // Registration reads file headers and searches folders; never do disk I/O on the event-loop thread.
    tauri::async_runtime::spawn_blocking(move || {
        let kinds = app.state::<DropKinds>().get();
        let outcome = app.state::<FileRegistry>().register_dropped(paths, &kinds);
        // Emitting only fails while the app is shutting down, when nobody is listening anyway.
        let _ = app.emit(FILES_ADDED_EVENT, outcome);
    });
}
