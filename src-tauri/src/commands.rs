//! IPC commands callable from the webview. Keep them thin: look up ids, call `fileforge-core`, map errors.
//! The TypeScript side of this contract is `src/services/fileforgeApi.ts`; change both together.

// Core
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use fileforge_core::FileKind;
use fileforge_core::pdf::{self, MAX_INPUT_BYTES, PdfOptions, PdfReport, Progress};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
// Types
use crate::error::AppError;
use crate::file_registry::{FileId, FileRegistry, RegisterOutcome};
use crate::job_control::{Cancellation, JobControl};
use crate::results::{ResultStore, copy_result, copy_result_to_folder, output_name};

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
pub async fn remove_file(app: AppHandle, id: FileId) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        app.state::<FileRegistry>().remove(id);
        results.remove(id);
    })
    .await?;
    Ok(())
}

/// Compresses one registered PDF into a temp result, sending throttled progress to `on_progress`. The original file
/// is only read. Ends with `cancelled` when `cancel_compression` is called before the result is ready.
#[tauri::command]
pub async fn compress_pdf(
    app: AppHandle,
    id: FileId,
    options: PdfOptions,
    on_progress: Channel<Progress>,
) -> Result<PdfReport, AppError> {
    options.validate()?;
    // Taken before waiting for the work slot, so a cancel also reaches a job that has not started yet.
    let ticket = app.state::<Cancellation>().ticket();
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        // One document in memory at a time, whatever the UI sends.
        let _slot = results.work_slot();
        let cancellation = app.state::<Cancellation>();
        if cancellation.is_cancelled(ticket) {
            return Err(AppError::Cancelled);
        }
        let file = app.state::<FileRegistry>().get(id)?;
        let input = read_pdf_input(&file.path, MAX_INPUT_BYTES)?;
        // A closed webview cannot receive progress; the compression itself still completes.
        let control = JobControl::new(&cancellation, ticket, |progress| drop(on_progress.send(progress)));
        let output = pdf::compress_controlled(&input, &options, &control)?;
        results.put(id, &output.bytes)?;
        Ok(output.report)
    })
    .await?
}

/// Cancels every compression already started, at its next checkpoint; later compressions are unaffected.
/// A no-op when nothing is running.
#[tauri::command]
pub fn cancel_compression(cancellation: State<'_, Cancellation>) {
    cancellation.cancel_started();
}

/// Asks where to save one result (next to the original by default) and writes it there.
/// Returns the saved file name, or `None` when the user cancels.
#[tauri::command]
pub async fn save_result(app: AppHandle, id: FileId) -> Result<Option<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let registry = app.state::<FileRegistry>();
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        let file = registry.get(id)?;
        let result = results.get(id).ok_or(AppError::NoResult(id))?;
        let mut dialog = app.dialog().file().set_file_name(output_name(&file.info.name)).add_filter("PDF", &["pdf"]);
        if let Some(parent) = file.path.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(target) = dialog.blocking_save_file() else { return Ok(None) };
        let target = target.into_path().map_err(|error| AppError::Io(error.to_string()))?;
        registry.check_save_target(&target)?;
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
        let _slot = results.work_slot();
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
            let target = copy_result_to_folder(&result, &folder, &output_name(&file.info.name))?;
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

/// Limit the actual read as well as metadata: a file can grow after registration or after the size check.
fn read_pdf_input(path: &Path, limit: u64) -> Result<Vec<u8>, AppError> {
    if !path.metadata()?.is_file() {
        return Err(AppError::NotAFile(display_name(path)));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::NotAFile(display_name(path)));
    }
    if metadata.len() > limit {
        return Err(AppError::PdfTooLarge(limit));
    }
    read_limited(file, limit)
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, AppError> {
    let mut input = Vec::new();
    reader.take(limit.saturating_add(1)).read_to_end(&mut input)?;
    if input.len() as u64 > limit {
        return Err(AppError::PdfTooLarge(limit));
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_reads_accept_the_limit_and_reject_larger_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("input.pdf");
        std::fs::write(&path, b"%PDF-1.7").expect("input");

        assert_eq!(read_pdf_input(&path, 8).expect("exact limit"), b"%PDF-1.7");
        assert!(matches!(read_pdf_input(&path, 7), Err(AppError::PdfTooLarge(7))));
        assert!(matches!(read_pdf_input(dir.path(), 8), Err(AppError::NotAFile(_))));
    }

    #[test]
    fn a_growing_input_cannot_read_past_the_limit_plus_one() {
        let mut reader = std::io::Cursor::new(b"more bytes than the limit");

        assert!(matches!(read_limited(&mut reader, 8), Err(AppError::PdfTooLarge(8))));
        assert_eq!(reader.position(), 9);
    }
}
