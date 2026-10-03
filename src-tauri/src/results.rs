//! Compressed results waiting to be saved. They live in a per-app temp directory until the user saves them, so the
//! original files are never written to (ADR-0003). The directory is emptied at startup and on exit.

// Core
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

// Types
use crate::file_registry::FileId;

#[derive(Debug, Clone)]
pub struct StoredResult {
    pub temp_path: PathBuf,
    pub saved_path: Option<PathBuf>,
}

pub struct ResultStore {
    dir: PathBuf,
    results: Mutex<HashMap<FileId, StoredResult>>,
    /// Held for the whole of one compression: at most one document is in memory at a time.
    work: Mutex<()>,
}

impl ResultStore {
    /// Starts from an empty directory, discarding results left by a previous session.
    pub fn open(dir: PathBuf) -> io::Result<Self> {
        match fs::remove_dir_all(&dir) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        fs::create_dir_all(&dir)?;
        Ok(Self { dir, results: Mutex::default(), work: Mutex::default() })
    }

    pub fn put(&self, id: FileId, bytes: &[u8]) -> io::Result<()> {
        let temp_path = self.dir.join(format!("{id}.pdf"));
        write_atomically(&temp_path, |tmp| fs::write(tmp, bytes))?;
        self.lock().insert(id, StoredResult { temp_path, saved_path: None });
        Ok(())
    }

    pub fn get(&self, id: FileId) -> Option<StoredResult> {
        self.lock().get(&id).cloned()
    }

    pub fn mark_saved(&self, id: FileId, path: PathBuf) {
        if let Some(result) = self.lock().get_mut(&id) {
            result.saved_path = Some(path);
        }
    }

    pub fn remove(&self, id: FileId) {
        if let Some(result) = self.lock().remove(&id) {
            let _ = fs::remove_file(result.temp_path);
        }
    }

    /// Deletes every temp result; called on exit.
    pub fn clear(&self) {
        self.lock().clear();
        let _ = fs::remove_dir_all(&self.dir);
    }

    pub fn work_slot(&self) -> MutexGuard<'_, ()> {
        self.work.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Plain data without cross-field invariants: recovering from a poisoned lock is safe.
    fn lock(&self) -> MutexGuard<'_, HashMap<FileId, StoredResult>> {
        self.results.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `report.pdf` → `report-compressed.pdf`.
pub fn output_name(input_name: &str) -> String {
    let stem = Path::new(input_name).file_stem().map_or_else(|| "document".into(), |s| s.to_string_lossy());
    format!("{stem}-compressed.pdf")
}

/// `dir/name`, or `dir/name (2)`, `(3)`… when taken, so saving a batch never overwrites existing files.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map_or_else(|| name.into(), |s| s.to_string_lossy());
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(candidate)
}

/// Copies a stored result to where the user chose, never leaving a half-written file behind.
pub fn copy_result(result: &StoredResult, target: &Path) -> io::Result<()> {
    write_atomically(target, |tmp| fs::copy(&result.temp_path, tmp).map(|_| ()))
}

/// Writes next to `target` under a temporary name, then renames over it (atomic on the same volume).
fn write_atomically(target: &Path, write: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let file_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = target.with_file_name(format!(".{file_name}.fileforge-partial"));
    if let Err(error) = write(&tmp).and_then(|()| fs::rename(&tmp, target)) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names_keep_the_stem() {
        assert_eq!(output_name("Annual report.pdf"), "Annual report-compressed.pdf");
        assert_eq!(output_name("scan.PDF"), "scan-compressed.pdf");
        assert_eq!(output_name("no-extension"), "no-extension-compressed.pdf");
    }

    #[test]
    fn unique_paths_never_reuse_existing_names() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = unique_path(dir.path(), "a-compressed.pdf");
        assert_eq!(first, dir.path().join("a-compressed.pdf"));
        fs::write(&first, b"x").expect("write");
        let second = unique_path(dir.path(), "a-compressed.pdf");
        assert_eq!(second, dir.path().join("a-compressed (2).pdf"));
        fs::write(&second, b"x").expect("write");
        assert_eq!(unique_path(dir.path(), "a-compressed.pdf"), dir.path().join("a-compressed (3).pdf"));
    }

    #[test]
    fn opening_discards_previous_session_results() {
        let dir = tempfile::tempdir().expect("temp dir");
        let results_dir = dir.path().join("results");
        fs::create_dir_all(&results_dir).expect("dir");
        fs::write(results_dir.join("7.pdf"), b"old").expect("write");

        let store = ResultStore::open(results_dir.clone()).expect("opens");

        assert_eq!(fs::read_dir(&results_dir).expect("readable").count(), 0);
        assert!(store.get(7).is_none());
    }

    #[test]
    fn results_are_stored_copied_and_removed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("opens");
        store.put(1, b"%PDF-small").expect("stores");
        let stored = store.get(1).expect("present");
        assert_eq!(fs::read(&stored.temp_path).expect("temp file"), b"%PDF-small");

        let target = dir.path().join("out.pdf");
        copy_result(&stored, &target).expect("copies");
        store.mark_saved(1, target.clone());
        assert_eq!(fs::read(&target).expect("saved"), b"%PDF-small");
        assert_eq!(store.get(1).and_then(|r| r.saved_path), Some(target));
        assert!(
            fs::read_dir(dir.path()).expect("dir").all(|e| !e
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains("partial"))
        );

        store.remove(1);
        assert!(store.get(1).is_none());
        assert!(!stored.temp_path.exists());
    }

    #[test]
    fn a_failed_copy_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = StoredResult { temp_path: dir.path().join("gone.pdf"), saved_path: None };
        let target = dir.path().join("out.pdf");

        assert!(copy_result(&missing, &target).is_err());
        assert_eq!(fs::read_dir(dir.path()).expect("dir").count(), 0);
    }
}
