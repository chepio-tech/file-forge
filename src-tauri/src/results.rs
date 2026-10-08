//! Compressed results waiting to be saved. They live in a per-app temp directory until the user saves them, so the
//! original files are never written to (ADR-0003). The directory is emptied at startup and on exit.

// Core
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

// Types
use crate::file_registry::FileId;

#[derive(Debug, Clone)]
pub struct StoredResult {
    pub temp_path: PathBuf,
    pub saved_path: Option<PathBuf>,
    /// File extension of the result's format, without the dot: `pdf`, `jpg`, `jpeg`, `png` or `webp`.
    pub extension: &'static str,
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

    pub fn put(&self, id: FileId, bytes: &[u8], extension: &'static str) -> io::Result<()> {
        let temp_path = self.dir.join(format!("{id}.{extension}"));
        write_atomically(&temp_path, |file| file.write_all(bytes), |tmp| fs::rename(tmp, &temp_path))?;
        // A new result of another format replaces the old one under a different temp name.
        if let Some(previous) =
            self.lock().insert(id, StoredResult { temp_path: temp_path.clone(), saved_path: None, extension })
            && previous.temp_path != temp_path
        {
            let _ = fs::remove_file(previous.temp_path);
        }
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
        let removed = self.lock().remove(&id);
        if let Some(result) = removed {
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

    /// The work slot if nothing holds it: `None` while a compression, save or removal runs.
    pub fn try_work_slot(&self) -> Option<MutexGuard<'_, ()>> {
        match self.work.try_lock() {
            Ok(slot) => Some(slot),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// Results that would be lost on exit: compressed but never saved.
    pub fn unsaved_count(&self) -> usize {
        self.lock().values().filter(|result| result.saved_path.is_none()).count()
    }

    /// Plain data without cross-field invariants: recovering from a poisoned lock is safe.
    fn lock(&self) -> MutexGuard<'_, HashMap<FileId, StoredResult>> {
        self.results.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `report.pdf` → `report-compressed.pdf`, with the result's extension.
pub fn output_name(input_name: &str, extension: &str) -> String {
    let stem = Path::new(input_name).file_stem().map_or_else(|| "document".into(), |s| s.to_string_lossy());
    format!("{stem}-compressed.{extension}")
}

/// `dir/name`, or `dir/name (2)`, `(3)`… when taken, so saving a batch never overwrites existing files.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if fs::symlink_metadata(&candidate).is_err() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map_or_else(|| name.into(), |s| s.to_string_lossy());
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or(candidate)
}

/// Copies a stored result to where the user chose, never leaving a half-written file behind.
pub fn copy_result(result: &StoredResult, target: &Path) -> io::Result<()> {
    write_atomically(target, |file| copy_bytes(result, file), |tmp| fs::rename(tmp, target))
}

/// Publishes a complete batch output without replacing even a file created after the name was selected.
pub fn copy_result_to_folder(result: &StoredResult, dir: &Path, name: &str) -> io::Result<PathBuf> {
    let mut target = unique_path(dir, name);
    write_atomically(
        &dir.join(name),
        |file| copy_bytes(result, file),
        |tmp| loop {
            match fs::hard_link(tmp, &target) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => target = unique_path(dir, name),
                Err(error) => return Err(error),
            }
        },
    )?;
    Ok(target)
}

fn copy_bytes(result: &StoredResult, file: &mut File) -> io::Result<()> {
    io::copy(&mut File::open(&result.temp_path)?, file).map(|_| ())
}

/// An exclusively created staging file in the destination directory; cleanup also runs on errors/unwinding.
struct PartialFile(PathBuf);

impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_atomically(
    target: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    publish: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    static NEXT_PARTIAL: AtomicU64 = AtomicU64::new(0);
    let (partial, mut file) = loop {
        let sequence = NEXT_PARTIAL.fetch_add(1, Ordering::Relaxed);
        let tmp = target.with_file_name(format!(".fileforge-{}-{sequence}.partial", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (PartialFile(tmp), file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let written = write(&mut file).and_then(|()| file.sync_all());
    // Windows does not allow renaming an open staging file.
    drop(file);
    written?;
    publish(&partial.0)
}

#[cfg(test)]
fn remaining_partials(dir: &Path) -> usize {
    fs::read_dir(dir)
        .expect("readable directory")
        .filter(|entry| entry.as_ref().expect("entry").file_name().to_string_lossy().ends_with(".partial"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_batch_saves_never_overwrite_each_other() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("store");
        store.put(1, b"%PDF-new", "pdf").expect("result");
        let stored = store.get(1).expect("result");
        let existing = dir.path().join("out.pdf");
        fs::write(&existing, b"keep existing").expect("existing file");
        let saved = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| copy_result_to_folder(&stored, dir.path(), "out.pdf").expect("saved")))
                .collect();
            handles.into_iter().map(|handle| handle.join().expect("thread")).collect::<Vec<_>>()
        });

        assert_eq!(saved.iter().collect::<std::collections::HashSet<_>>().len(), 8);
        assert_eq!(fs::read(existing).expect("existing"), b"keep existing");
        for path in saved {
            assert_eq!(fs::read(path).expect("saved copy"), b"%PDF-new");
        }
        assert_eq!(remaining_partials(dir.path()), 0);
    }

    #[test]
    fn failed_writes_leave_existing_files_unchanged_and_clean_up_staging() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("out.pdf");
        fs::write(&target, b"keep existing").expect("existing");
        let written = write_atomically(
            &target,
            |file| {
                file.write_all(b"partial")?;
                Err(io::Error::other("disk full"))
            },
            |tmp| fs::rename(tmp, &target),
        );

        assert!(written.is_err());
        assert_eq!(fs::read(target).expect("existing"), b"keep existing");
        assert_eq!(remaining_partials(dir.path()), 0);
    }

    #[test]
    fn output_names_keep_the_stem() {
        assert_eq!(output_name("Annual report.pdf", "pdf"), "Annual report-compressed.pdf");
        assert_eq!(output_name("scan.PDF", "pdf"), "scan-compressed.pdf");
        assert_eq!(output_name("no-extension", "pdf"), "no-extension-compressed.pdf");
        assert_eq!(output_name("IMG_0001.JPG", "jpg"), "IMG_0001-compressed.jpg");
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

    #[cfg(unix)]
    #[test]
    fn batch_saving_skips_broken_symbolic_links() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("store");
        store.put(1, b"%PDF-new", "pdf").expect("result");
        let stored = store.get(1).expect("result");
        let taken = dir.path().join("out.pdf");
        let missing = dir.path().join("missing.pdf");
        std::os::unix::fs::symlink(&missing, &taken).expect("broken symlink");

        let saved = copy_result_to_folder(&stored, dir.path(), "out.pdf").expect("saved");

        assert_eq!(saved, dir.path().join("out (2).pdf"));
        assert_eq!(fs::read(saved).expect("saved"), b"%PDF-new");
        assert_eq!(fs::read_link(taken).expect("symlink intact"), missing);
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
        store.put(1, b"%PDF-small", "pdf").expect("stores");
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
    fn the_work_slot_is_only_offered_when_free() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("opens");
        let slot = store.work_slot();
        assert!(store.try_work_slot().is_none());
        drop(slot);
        assert!(store.try_work_slot().is_some());
    }

    #[test]
    fn only_results_never_saved_count_as_unsaved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("opens");
        assert_eq!(store.unsaved_count(), 0);
        store.put(1, b"%PDF-1", "pdf").expect("stores");
        store.put(2, b"%PDF-2", "pdf").expect("stores");
        assert_eq!(store.unsaved_count(), 2);
        store.mark_saved(1, dir.path().join("1.pdf"));
        assert_eq!(store.unsaved_count(), 1);
        store.remove(2);
        assert_eq!(store.unsaved_count(), 0);
    }

    #[test]
    fn a_failed_copy_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = StoredResult { temp_path: dir.path().join("gone.pdf"), saved_path: None, extension: "pdf" };
        let target = dir.path().join("out.pdf");

        assert!(copy_result(&missing, &target).is_err());
        assert_eq!(fs::read_dir(dir.path()).expect("dir").count(), 0);
    }
}
