//! IPC commands callable from the webview. Keep them thin: validate, delegate to the registry or core, map errors.
//! The TypeScript side of this contract is `src/services/fileforgeApi.ts`; change both together.

// Core
use fileforge_core::FileKind;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
// Types
use crate::error::AppError;
use crate::file_registry::{FileId, FileRegistry, RegisterOutcome};

/// Opens the native "open files" dialog filtered to `kinds` and registers the picked files.
/// Returns an empty outcome when the user cancels.
#[tauri::command]
pub async fn pick_files(
    app: AppHandle,
    kinds: Vec<FileKind>,
    filter_name: String,
) -> Result<RegisterOutcome, AppError> {
    let extensions: Vec<&str> = kinds.iter().flat_map(|kind| kind.extensions()).copied().collect();
    let mut dialog = app.dialog().file();
    if !extensions.is_empty() {
        dialog = dialog.add_filter(filter_name, &extensions);
    }

    // The dialog blocks until the user answers and registration touches the disk: keep both off the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        let paths =
            dialog.blocking_pick_files().unwrap_or_default().into_iter().filter_map(|file| file.into_path().ok());
        app.state::<FileRegistry>().register_all(paths)
    })
    .await
    .map_err(AppError::from)
}

/// Forgets a file the user removed from the list.
#[tauri::command]
pub fn remove_file(registry: State<'_, FileRegistry>, id: FileId) {
    registry.remove(id);
}
