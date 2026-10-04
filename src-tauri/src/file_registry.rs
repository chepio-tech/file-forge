//! Session registry of files the user handed to the app.
//!
//! The webview only ever sees a [`FileId`]; paths stay on the Rust side (ADR-0004). Entries live for the app session
//! and are never persisted.

// Core
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use fileforge_core::FileKind;
use fileforge_core::file_kind::SNIFF_LEN;
use serde::Serialize;
// Types
use crate::error::AppError;
use crate::folder_scan::{FolderScan, FolderScanner, ScanLimits, is_package};

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

/// Result of registering a batch: accepted files plus the names of entries that were skipped (unreadable, not regular
/// files, packages).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterOutcome {
    pub files: Vec<FileInfo>,
    pub skipped: Vec<String>,
    /// Present when the drop contained folders (ADR-0017).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folders: Option<FolderScan>,
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
    /// Keep input paths protected for the entire session, even after removal from the UI.
    originals: HashSet<PathBuf>,
    /// `realpath` can preserve the requested letter case on a case-insensitive macOS volume.
    #[cfg(unix)]
    original_file_ids: HashSet<(u64, u64)>,
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

    /// Registers dropped files as they are, and from dropped folders the files whose extension belongs to `kinds`
    /// (ADR-0017). Packages count as files: a dropped package is skipped, not searched.
    pub fn register_dropped(&self, paths: impl IntoIterator<Item = PathBuf>, kinds: &[FileKind]) -> RegisterOutcome {
        self.register_dropped_with_limits(paths, kinds, ScanLimits::DEFAULT)
    }

    fn register_dropped_with_limits(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        kinds: &[FileKind],
        limits: ScanLimits,
    ) -> RegisterOutcome {
        let mut outcome = RegisterOutcome::default();
        let mut scanner = FolderScanner::new(kinds, limits);
        let mut unreadable = Vec::new();
        for path in paths {
            if !path.is_dir() || is_package(&path) {
                match self.register(path.clone()) {
                    Ok(info) => outcome.files.push(info),
                    Err(_) => outcome.skipped.push(display_name(&path)),
                }
                continue;
            }
            for file in scanner.scan(&path, &mut unreadable) {
                match self.register(file.clone()) {
                    Ok(info) => {
                        scanner.summary.added = scanner.summary.added.saturating_add(1);
                        outcome.files.push(info);
                    }
                    Err(_) => outcome.skipped.push(display_name(&file)),
                }
            }
        }
        outcome.skipped.extend(unreadable.iter().map(|path| display_name(path)));
        outcome.folders = (scanner.summary.folders > 0).then_some(scanner.summary);
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
        inner.originals.insert(path.clone());
        #[cfg(unix)]
        inner.original_file_ids.insert((metadata.dev(), metadata.ino()));
        inner.files.insert(info.id, RegisteredFile { path, info: info.clone() });
        Ok(info)
    }

    pub fn get(&self, id: FileId) -> Result<RegisteredFile, AppError> {
        self.lock().files.get(&id).cloned().ok_or(AppError::UnknownFile(id))
    }

    /// Resolve aliases before saving so a native dialog cannot overwrite any input file.
    pub fn check_save_target(&self, target: &Path) -> Result<(), AppError> {
        #[cfg(unix)]
        if let Ok(metadata) = target.metadata()
            && self.lock().original_file_ids.contains(&(metadata.dev(), metadata.ino()))
        {
            return Err(AppError::OriginalTarget);
        }
        let resolved = match target.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = target.parent().ok_or(error)?;
                let name = target.file_name().ok_or(AppError::OriginalTarget)?;
                parent.canonicalize()?.join(name)
            }
            Err(error) => return Err(error.into()),
        };
        if self.lock().originals.contains(&resolved) {
            return Err(AppError::OriginalTarget);
        }
        Ok(())
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
        assert_eq!(registry.get(info.id).map(|f| f.info).ok(), Some(info));
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
    fn dropped_folders_add_accepted_files_in_drop_order() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = write(dir.path(), "first.pdf", b"%PDF-1.4");
        let scans = dir.path().join("Scans");
        std::fs::create_dir_all(scans.join("2024")).expect("sub dirs");
        write(&scans, "b.pdf", b"%PDF-1.7");
        write(&scans.join("2024"), "a.pdf", b"%PDF-1.7");
        write(&scans, "fake.pdf", b"PK\x03\x04");
        write(&scans, "photo.jpg", b"\xFF\xD8\xFF");
        let package = dir.path().join("Letter.pages");
        std::fs::create_dir(&package).expect("package");
        write(&package, "preview.pdf", b"%PDF-1.7");
        let direct_image = write(dir.path(), "direct.jpg", b"\xFF\xD8\xFF");
        let registry = FileRegistry::default();

        let outcome = registry.register_dropped([first, scans, package, direct_image], &[FileKind::Pdf]);

        let names: Vec<_> = outcome.files.iter().map(|file| (file.name.as_str(), file.kind)).collect();
        assert_eq!(
            names,
            [
                ("first.pdf", FileKind::Pdf),
                ("a.pdf", FileKind::Pdf),
                ("b.pdf", FileKind::Pdf),
                // The extension selects it; the content decides its kind, so the UI rejects it by name.
                ("fake.pdf", FileKind::Other),
                // Dropped directly: registered whatever its kind, as before folders were expanded.
                ("direct.jpg", FileKind::Image),
            ]
        );
        assert_eq!(outcome.skipped, ["Letter.pages"]);
        assert_eq!(outcome.folders, Some(FolderScan { folders: 1, added: 3, ignored: 1, truncated: false }));
    }

    #[test]
    fn drops_without_folders_report_no_folder_summary() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf = write(dir.path(), "only.pdf", b"%PDF-1.4");
        let registry = FileRegistry::default();

        let outcome = registry.register_dropped([pdf, dir.path().join("gone.pdf")], &[FileKind::Pdf]);

        assert_eq!((outcome.files.len(), outcome.skipped.as_slice()), (1, &["gone.pdf".to_owned()][..]));
        assert_eq!(outcome.folders, None);
        let json = serde_json::to_value(&outcome).expect("serializes");
        assert!(json.get("folders").is_none(), "omitted for the UI: {json}");
    }

    #[test]
    fn folder_summary_serializes_for_the_ui() {
        let dir = tempfile::tempdir().expect("temp dir");
        for index in 0..3 {
            write(dir.path(), &format!("{index}.pdf"), b"%PDF-1.4");
        }
        let registry = FileRegistry::default();
        let limits = ScanLimits { max_files: 2, ..ScanLimits::DEFAULT };

        let outcome = registry.register_dropped_with_limits([dir.path().to_path_buf()], &[FileKind::Pdf], limits);

        assert_eq!(outcome.files.len(), 2);
        assert_eq!(
            serde_json::to_value(&outcome).expect("serializes")["folders"],
            serde_json::json!({ "folders": 1, "added": 2, "ignored": 0, "truncated": true })
        );
    }

    #[test]
    fn removed_file_gets_a_new_id_when_added_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write(dir.path(), "x.png", b"\x89PNG");
        let registry = FileRegistry::default();
        let info = registry.register(path.clone()).expect("png");

        registry.remove(info.id);
        registry.remove(info.id);

        assert!(matches!(registry.get(info.id), Err(AppError::UnknownFile(id)) if id == info.id));
        let again = registry.register(path).expect("png again");
        assert_ne!(again.id, info.id);
        assert_eq!(again.kind, FileKind::Image);
    }

    #[test]
    fn saving_cannot_overwrite_any_input_even_after_removal() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = write(dir.path(), "first.pdf", b"%PDF-first");
        let second = write(dir.path(), "second.pdf", b"%PDF-second");
        let registry = FileRegistry::default();
        let info = registry.register(first.clone()).expect("first");
        registry.register(second.clone()).expect("second");
        registry.remove(info.id);

        for path in [&first, &second, &dir.path().join(".").join("first.pdf")] {
            assert!(matches!(registry.check_save_target(path), Err(AppError::OriginalTarget)));
        }
        assert!(registry.check_save_target(&dir.path().join("new.pdf")).is_ok());
        std::fs::remove_file(&first).expect("external deletion");
        assert!(matches!(registry.check_save_target(&first), Err(AppError::OriginalTarget)));
        assert_eq!(std::fs::read(second).expect("original"), b"%PDF-second");
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_an_alias_to_an_input_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let original = write(dir.path(), "input.pdf", b"%PDF-original");
        let alias = dir.path().join("alias.pdf");
        std::os::unix::fs::symlink(&original, &alias).expect("symlink");
        let registry = FileRegistry::default();
        registry.register(original).expect("input");

        assert!(matches!(registry.check_save_target(&alias), Err(AppError::OriginalTarget)));
    }

    #[cfg(unix)]
    #[test]
    fn saving_to_a_hard_link_of_an_input_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let original = write(dir.path(), "input.pdf", b"%PDF-original");
        let alias = dir.path().join("hard-link.pdf");
        std::fs::hard_link(&original, &alias).expect("hard link");
        let registry = FileRegistry::default();
        registry.register(original).expect("input");

        assert!(matches!(registry.check_save_target(&alias), Err(AppError::OriginalTarget)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn saving_to_a_case_variant_of_an_input_is_refused_on_insensitive_volumes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let original = write(dir.path(), "input.pdf", b"%PDF-original");
        let alias = dir.path().join("INPUT.pdf");
        let registry = FileRegistry::default();
        registry.register(original).expect("input");

        if alias.exists() {
            assert!(matches!(registry.check_save_target(&alias), Err(AppError::OriginalTarget)));
        } else {
            assert!(registry.check_save_target(&alias).is_ok());
        }
    }
}
