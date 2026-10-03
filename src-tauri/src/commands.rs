//! IPC commands callable from the webview. Keep them thin: look up ids, call `fileforge-core`, map errors.
//! The TypeScript side of this contract is `src/services/fileforgeApi.ts`; change both together.

// Core
use std::fs;
use std::path::PathBuf;

use fileforge_core::FileKind;
use fileforge_core::pdf::{self, MAX_INPUT_BYTES, PdfOptions, PdfReport};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
// Types
use crate::error::AppError;
use crate::file_registry::{FileId, FileRegistry, RegisterOutcome};
use crate::results::{ResultStore, copy_result, output_name, unique_path};

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

/// Forgets a file the user removed from the list, together with its unsaved result.
#[tauri::command]
pub fn remove_file(registry: State<'_, FileRegistry>, results: State<'_, ResultStore>, id: FileId) {
    registry.remove(id);
    results.remove(id);
}

/// Compresses one registered PDF into a temp result. The original file is only read.
#[tauri::command]
pub async fn compress_pdf(app: AppHandle, id: FileId, options: PdfOptions) -> Result<PdfReport, AppError> {
    options.validate()?;
    tauri::async_runtime::spawn_blocking(move || {
        let file = app.state::<FileRegistry>().get(id)?;
        let results = app.state::<ResultStore>();
        // One document in memory at a time, whatever the UI sends.
        let _slot = results.work_slot();
        let size = fs::metadata(&file.path)?.len();
        if size > MAX_INPUT_BYTES {
            return Err(AppError::PdfTooLarge(MAX_INPUT_BYTES));
        }
        let input = fs::read(&file.path)?;
        let output = pdf::compress(&input, &options)?;
        results.put(id, &output.bytes)?;
        Ok(output.report)
    })
    .await?
}

/// Asks where to save one result (next to the original by default) and writes it there.
/// Returns the saved file name, or `None` when the user cancels.
#[tauri::command]
pub async fn save_result(app: AppHandle, id: FileId) -> Result<Option<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = app.state::<FileRegistry>().get(id)?;
        let results = app.state::<ResultStore>();
        let result = results.get(id).ok_or(AppError::NoResult(id))?;
        let mut dialog = app.dialog().file().set_file_name(output_name(&file.info.name)).add_filter("PDF", &["pdf"]);
        if let Some(parent) = file.path.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(target) = dialog.blocking_save_file() else { return Ok(None) };
        let target = target.into_path().map_err(|error| AppError::Io(error.to_string()))?;
        copy_result(&result, &target)?;
        let name = display_name(&target);
        results.mark_saved(id, target);
        Ok(Some(name))
    })
    .await?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    pub id: FileId,
    pub name: String,
}

/// Asks for a folder and saves every given result into it as `<name>-compressed.pdf`, never overwriting existing
/// files. Returns what was saved, or `None` when the user cancels.
#[tauri::command]
pub async fn save_results_to_folder(app: AppHandle, ids: Vec<FileId>) -> Result<Option<Vec<SavedFile>>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let registry = app.state::<FileRegistry>();
        let results = app.state::<ResultStore>();
        let mut dialog = app.dialog().file().set_can_create_directories(true);
        if let Some(parent) =
            ids.first().and_then(|id| registry.get(*id).ok()).and_then(|f| f.path.parent().map(PathBuf::from))
        {
            dialog = dialog.set_directory(parent);
        }
        let Some(folder) = dialog.blocking_pick_folder() else { return Ok(None) };
        let folder = folder.into_path().map_err(|error| AppError::Io(error.to_string()))?;

        let mut saved = Vec::with_capacity(ids.len());
        for id in ids {
            let file = registry.get(id)?;
            let result = results.get(id).ok_or(AppError::NoResult(id))?;
            let target = unique_path(&folder, &output_name(&file.info.name));
            copy_result(&result, &target)?;
            saved.push(SavedFile { id, name: display_name(&target) });
            results.mark_saved(id, target);
        }
        Ok(Some(saved))
    })
    .await?
}

/// Shows the last saved copy of a result in Finder / Explorer / the file manager.
#[tauri::command]
pub fn reveal_result(app: AppHandle, results: State<'_, ResultStore>, id: FileId) -> Result<(), AppError> {
    let path = results.get(id).and_then(|result| result.saved_path).ok_or(AppError::NoResult(id))?;
    app.opener().reveal_item_in_dir(path).map_err(|error| AppError::Io(error.to_string()))
}

fn display_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}
