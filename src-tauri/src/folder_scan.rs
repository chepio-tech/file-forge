//! Expands folders dropped on the window into the files inside them (ADR-0017). Traversal is bounded and cannot loop:
//! symlinked folders are not followed, hidden entries and macOS packages are not entered, and limits cap the work of
//! one drop.

// Core
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use fileforge_core::FileKind;
use serde::Serialize;

/// macOS shows these folders as single apps or documents. Their insides (a `.pages` preview PDF, a photo library) are
/// not files the user meant to drop.
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "band",
    "bundle",
    "fcpbundle",
    "framework",
    "imovielibrary",
    "key",
    "kext",
    "logicx",
    "musiclibrary",
    "numbers",
    "pages",
    "photoslibrary",
    "playground",
    "plugin",
    "rtfd",
    "scptd",
    "sparsebundle",
    "tvlibrary",
    "xcodeproj",
    "xcworkspace",
];

/// What expanding the dropped folders of one drop did, for the UI's notice.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderScan {
    /// Folders dropped, not counting their subfolders.
    pub folders: u32,
    /// Files found in them and registered.
    pub added: u32,
    /// Files and packages whose type the active tool does not accept.
    pub ignored: u32,
    /// A limit stopped the search, so some files were not added.
    pub truncated: bool,
}

/// Bounds for one drop, shared by all folders in it.
#[derive(Debug, Clone, Copy)]
pub struct ScanLimits {
    /// Folder levels searched below a dropped folder.
    pub max_depth: usize,
    /// Directory entries examined, hidden ones included.
    pub max_entries: usize,
    /// Files collected.
    pub max_files: usize,
}

impl ScanLimits {
    pub const DEFAULT: Self = Self { max_depth: 16, max_entries: 10_000, max_files: 1_000 };
}

/// Finds the files the active tool accepts in the folders of one drop. Accepted means the extension belongs to one of
/// its kinds, as in the open dialog; registration then checks the content.
pub struct FolderScanner {
    extensions: Vec<&'static str>,
    limits: ScanLimits,
    entries: usize,
    files: usize,
    /// Canonical folders already searched: a folder dropped twice, or together with its parent, is searched once.
    visited: HashSet<PathBuf>,
    pub summary: FolderScan,
}

impl FolderScanner {
    pub fn new(kinds: &[FileKind], limits: ScanLimits) -> Self {
        let extensions = kinds.iter().flat_map(|kind| kind.extensions()).copied().collect();
        Self { extensions, limits, entries: 0, files: 0, visited: HashSet::new(), summary: FolderScan::default() }
    }

    /// Accepted files inside `folder`, depth first with entries in case-insensitive name order. Folders that cannot be
    /// read are appended to `unreadable`.
    pub fn scan(&mut self, folder: &Path, unreadable: &mut Vec<PathBuf>) -> Vec<PathBuf> {
        self.summary.folders = self.summary.folders.saturating_add(1);
        let mut found = Vec::new();
        self.walk(folder, 0, &mut found, unreadable);
        found
    }

    fn walk(&mut self, dir: &Path, depth: usize, found: &mut Vec<PathBuf>, unreadable: &mut Vec<PathBuf>) {
        if self.entries >= self.limits.max_entries {
            self.summary.truncated = true;
            return;
        }
        let Ok(canonical) = dir.canonicalize() else {
            unreadable.push(dir.to_path_buf());
            return;
        };
        if !self.visited.insert(canonical) {
            return;
        }
        let Ok(listing) = fs::read_dir(dir) else {
            unreadable.push(dir.to_path_buf());
            return;
        };
        let mut entries = Vec::new();
        for entry in listing {
            // Counted before reading the next entry, so a folder with millions of entries costs at most the budget.
            if self.entries >= self.limits.max_entries {
                self.summary.truncated = true;
                break;
            }
            self.entries += 1;
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    unreadable.push(dir.to_path_buf());
                    break;
                }
            }
        }
        entries.sort_by_cached_key(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.to_lowercase(), name)
        });

        for entry in entries {
            // `.DS_Store`, AppleDouble `._report.pdf` files on non-Apple volumes, `.git` and the like.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                unreadable.push(path);
                continue;
            };
            let (is_dir, is_file) = if file_type.is_symlink() {
                // Linked files count; linked folders are not followed, so the search cannot loop or leave the folder.
                (false, fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()))
            } else {
                (file_type.is_dir(), file_type.is_file())
            };
            if is_dir {
                if is_package(&path) {
                    self.summary.ignored = self.summary.ignored.saturating_add(1);
                } else if depth >= self.limits.max_depth {
                    self.summary.truncated = true;
                } else {
                    self.walk(&path, depth + 1, found, unreadable);
                }
            } else if is_file {
                if !self.accepts(&path) {
                    self.summary.ignored = self.summary.ignored.saturating_add(1);
                } else if self.files >= self.limits.max_files {
                    self.summary.truncated = true;
                    return;
                } else {
                    self.files += 1;
                    found.push(path);
                }
            }
            // Sockets, pipes and devices are not documents; they are skipped without a word.
        }
    }

    fn accepts(&self, path: &Path) -> bool {
        has_extension(path, &self.extensions)
    }
}

/// A folder macOS presents as one app or document.
pub fn is_package(path: &Path) -> bool {
    has_extension(path, PACKAGE_EXTENSIONS)
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.iter().any(|candidate| candidate.eq_ignore_ascii_case(extension)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("test folder");
        }
        fs::write(path, b"%PDF-1.7").expect("test file");
    }

    fn names(root: &Path, files: &[PathBuf]) -> Vec<String> {
        files
            .iter()
            .map(|file| file.strip_prefix(root).expect("inside root").to_string_lossy().replace('\\', "/"))
            .collect()
    }

    fn pdf_scanner(limits: ScanLimits) -> FolderScanner {
        FolderScanner::new(&[FileKind::Pdf], limits)
    }

    #[test]
    fn finds_accepted_files_depth_first_in_name_order() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Scans");
        for name in ["b.pdf", "A.PDF", "notes.txt", "2024/c.pdf", "2024/deep/d.pdf", "Archive/e.pdf", "photo.jpg"] {
            touch(&root.join(name));
        }
        let mut unreadable = Vec::new();
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let files = scanner.scan(&root, &mut unreadable);

        assert_eq!(names(&root, &files), ["2024/c.pdf", "2024/deep/d.pdf", "A.PDF", "Archive/e.pdf", "b.pdf"]);
        assert_eq!(scanner.summary, FolderScan { folders: 1, added: 0, ignored: 2, truncated: false });
        assert!(unreadable.is_empty());
    }

    #[test]
    fn hidden_entries_are_skipped_without_counting() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Docs");
        for name in ["._report.pdf", ".DS_Store", ".git/inner.pdf", "report.pdf"] {
            touch(&root.join(name));
        }
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let files = scanner.scan(&root, &mut Vec::new());

        assert_eq!(names(&root, &files), ["report.pdf"]);
        assert_eq!(scanner.summary.ignored, 0);
    }

    #[test]
    fn packages_are_counted_as_ignored_and_never_entered() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Work");
        for name in ["Budget.numbers/preview.pdf", "Letter.PAGES/QuickLook/Preview.pdf", "Tool.app/Contents/x.pdf"] {
            touch(&root.join(name));
        }
        touch(&root.join("v1.2/real.pdf"));
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let files = scanner.scan(&root, &mut Vec::new());

        assert_eq!(names(&root, &files), ["v1.2/real.pdf"], "a dot in a folder name is not a package");
        assert_eq!(scanner.summary.ignored, 3);
        assert!(is_package(&root.join("Letter.PAGES")));
    }

    #[test]
    fn no_accepted_kinds_finds_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        touch(&dir.path().join("a.pdf"));
        let mut scanner = FolderScanner::new(&[], ScanLimits::DEFAULT);

        assert!(scanner.scan(dir.path(), &mut Vec::new()).is_empty());
        assert_eq!(scanner.summary.ignored, 1);
    }

    #[test]
    fn a_folder_reached_twice_is_searched_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Scans");
        touch(&root.join("inner/a.pdf"));
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let first = scanner.scan(&root.join("inner"), &mut Vec::new());
        let second = scanner.scan(&root, &mut Vec::new());
        let third = scanner.scan(&root.join(".").join("inner"), &mut Vec::new());

        assert_eq!((first.len(), second.len(), third.len()), (1, 0, 0));
        assert_eq!(scanner.summary.folders, 3);
    }

    #[test]
    fn depth_limit_stops_the_search_and_says_so() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Deep");
        touch(&root.join("1/one.pdf"));
        touch(&root.join("1/2/two.pdf"));
        let mut scanner = pdf_scanner(ScanLimits { max_depth: 1, ..ScanLimits::DEFAULT });

        let files = scanner.scan(&root, &mut Vec::new());

        assert_eq!(names(&root, &files), ["1/one.pdf"]);
        assert!(scanner.summary.truncated);
    }

    #[test]
    fn file_limit_keeps_the_first_files_and_says_so() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Many");
        for index in 0..5 {
            touch(&root.join(format!("{index}.pdf")));
        }
        let mut scanner = pdf_scanner(ScanLimits { max_files: 3, ..ScanLimits::DEFAULT });

        let files = scanner.scan(&root, &mut Vec::new());

        assert_eq!(names(&root, &files), ["0.pdf", "1.pdf", "2.pdf"]);
        assert!(scanner.summary.truncated);
    }

    #[test]
    fn entry_limit_is_shared_by_every_folder_of_a_drop() {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in ["First/a.pdf", "First/b.pdf", "First/c.pdf", "Second/d.pdf"] {
            touch(&dir.path().join(name));
        }
        let mut scanner = pdf_scanner(ScanLimits { max_entries: 3, ..ScanLimits::DEFAULT });

        let first = scanner.scan(&dir.path().join("First"), &mut Vec::new());
        assert!(!scanner.summary.truncated, "exactly at the budget");
        let second = scanner.scan(&dir.path().join("Second"), &mut Vec::new());

        assert_eq!((first.len(), second.len()), (3, 0));
        assert!(scanner.summary.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn linked_files_count_but_linked_folders_are_not_followed() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Linked");
        touch(&root.join("inner/a.pdf"));
        touch(&dir.path().join("outside/b.pdf"));
        symlink(&root, root.join("inner/loop")).expect("folder link");
        symlink(dir.path().join("outside"), root.join("outside")).expect("folder link");
        symlink(dir.path().join("outside/b.pdf"), root.join("b-link.pdf")).expect("file link");
        symlink(dir.path().join("missing.pdf"), root.join("broken.pdf")).expect("broken link");
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let files = scanner.scan(&root, &mut Vec::new());

        assert_eq!(names(&root, &files), ["b-link.pdf", "inner/a.pdf"]);
        assert!(!scanner.summary.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folders_are_reported_and_the_rest_is_searched() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("Mixed");
        touch(&root.join("locked/secret.pdf"));
        touch(&root.join("open.pdf"));
        let locked = root.join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock folder");
        // A superuser reads any folder, so the lock proves nothing there.
        let locked_out = fs::read_dir(&locked).is_err();
        let mut unreadable = Vec::new();
        let mut scanner = pdf_scanner(ScanLimits::DEFAULT);

        let files = scanner.scan(&root, &mut unreadable);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock folder for cleanup");

        if locked_out {
            assert_eq!(names(&root, &files), ["open.pdf"]);
            assert_eq!(unreadable, [locked]);
        }
    }
}
