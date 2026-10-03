//! Session registry of files the user handed to the app.
//!
//! The webview only ever sees a [`FileId`]; paths stay on the Rust side (ADR-0004). Entries live for the app session
//! and are never persisted.

// Core
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use fileforge_core::FileKind;
use fileforge_core::file_kind::SNIFF_LEN;
use serde::Serialize;
// Types
use crate::error::AppError;

pub type FileId = u64;

/// What the UI knows about a registered file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: FileId,
    pub name: String,
    pub size: u64,
    pub kind: FileKind,
}

/// Result of registering a batch: accepted files plus the names of entries that were skipped (folders, unreadable).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterOutcome {
    pub files: Vec<FileInfo>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RegisteredFile {
    pub path: PathBuf,
    pub info: FileInfo,
}

#[derive(Default)]
pub struct FileRegistry {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next_id: FileId,
    files: HashMap<FileId, RegisteredFile>,
}

impl FileRegistry {
    /// Registers every path, skipping the ones that are not readable regular files.
    pub fn register_all(&self, paths: impl IntoIterator<Item = PathBuf>) -> RegisterOutcome {
        let mut outcome = RegisterOutcome::default();
        for path in paths {
            match self.register(path.clone()) {
                Ok(info) => outcome.files.push(info),
                Err(_) => outcome.skipped.push(display_name(&path)),
            }
        }
        outcome
    }

    /// Registers one file. Registering the same file twice returns the existing entry, so the UI can de-duplicate by
    /// id.
    pub fn register(&self, path: PathBuf) -> Result<FileInfo, AppError> {
        let path = path.canonicalize()?;
        let metadata = path.metadata()?;
        if !metadata.is_file() {
            return Err(AppError::NotAFile(display_name(&path)));
        }
        let kind = FileKind::detect(path.extension().and_then(|e| e.to_str()), &read_head(&path)?);

        let mut inner = self.lock();
        if let Some(existing) = inner.files.values().find(|f| f.path == path) {
            return Ok(existing.info.clone());
        }
        inner.next_id += 1;
        let info = FileInfo { id: inner.next_id, name: display_name(&path), size: metadata.len(), kind };
        inner.files.insert(info.id, RegisteredFile { path, info: info.clone() });
        Ok(info)
    }

    /// Forgets a file. Removing an unknown id is not an error: the UI may race with itself on double clicks.
    pub fn remove(&self, id: FileId) {
        self.lock().files.remove(&id);
    }

    /// The registry holds plain data with no invariants spanning several fields, so a panic in another thread
    /// cannot leave it half-updated; recovering from poisoning is safe.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn read_head(path: &Path) -> Result<Vec<u8>, AppError> {
    let mut head = Vec::with_capacity(SNIFF_LEN);
    File::open(path)?.take(SNIFF_LEN as u64).read_to_end(&mut head)?;
    Ok(head)
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("test file must be writable");
        path
    }

    #[test]
    fn registers_a_pdf_with_size_and_kind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write(dir.path(), "report.pdf", b"%PDF-1.7\n%%EOF");
        let registry = FileRegistry::default();

        let info = registry.register(path).expect("pdf registers");

        assert_eq!(info.name, "report.pdf");
        assert_eq!(info.size, 14);
        assert_eq!(info.kind, FileKind::Pdf);
    }

    #[test]
    fn same_file_registered_twice_keeps_one_id() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write(dir.path(), "a.pdf", b"%PDF-1.4");
        let registry = FileRegistry::default();

        let first = registry.register(path.clone()).expect("first");
        let second = registry.register(dir.path().join(".").join("a.pdf")).expect("second");

        assert_eq!(first.id, second.id);
    }

    #[test]
    fn folders_and_missing_files_are_skipped_by_name() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf = write(dir.path(), "ok.pdf", b"%PDF-1.4");
        let folder = dir.path().join("Scans");
        std::fs::create_dir(&folder).expect("sub dir");
        let missing = dir.path().join("gone.pdf");
        let registry = FileRegistry::default();

        let outcome = registry.register_all([pdf, folder, missing]);

        assert_eq!(outcome.files.len(), 1);
        assert_eq!(outcome.skipped, vec!["Scans".to_owned(), "gone.pdf".to_owned()]);
    }

    #[test]
    fn removed_file_gets_a_new_id_when_added_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write(dir.path(), "x.png", b"\x89PNG");
        let registry = FileRegistry::default();
        let info = registry.register(path.clone()).expect("png");

        registry.remove(info.id);
        registry.remove(info.id);

        let again = registry.register(path).expect("png again");
        assert_ne!(again.id, info.id);
        assert_eq!(again.kind, FileKind::Image);
    }
}
