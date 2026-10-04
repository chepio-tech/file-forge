# ADR-0017: Dropped folders are searched in Rust for the active tool's file types

## Status
Accepted

## Date
2026-10-04

## Context
Until now, a folder dropped on the window was skipped with "Skipped (not a file)". People keep scans and invoices in
folders and expect the folder's PDFs to be added. A recursive search reads untrusted filesystem layouts: symbolic
link loops, huge trees, network drives, hidden system files (`.DS_Store`, AppleDouble `._report.pdf` files on
non-Apple volumes) and macOS packages, which Finder shows as single documents but which are folders on disk (a
`.pages` document contains a preview PDF). Paths must stay in Rust (ADR-0004), and the drop event reaches Rust before
the webview, so Rust does not know which tool is shown.

## Decision
- **Kinds come from the shown tool.** `useFileIntake` calls `set_drop_kinds(kinds)` when its tool becomes active.
  Rust keeps the list and selects folder contents by extension of those kinds, the same rule as the open dialog.
  Registration then checks the content: a `.pdf` file that is not a PDF arrives with kind `other`, and the UI
  rejects it by name. Directly dropped files are registered as before, whatever their kind.
- **Search rules.** Depth first, entries sorted case-insensitively by name. Names starting with `.` are skipped
  silently. Symbolic links to files count; links to folders are not followed. A folder reached twice (dropped twice,
  or dropped together with its parent) is searched once, by canonical path. Folders with macOS package extensions
  (`app`, `pages`, `numbers`, `key`, `photoslibrary`, … in `src-tauri/src/folder_scan.rs`) are counted as ignored
  files and never entered; a dropped package is skipped like a file that cannot be read. Unreadable folders are
  reported by name in `skipped`.
- **Limits per drop**, shared by all its folders: 16 levels below a dropped folder, 10,000 directory entries
  examined, 1,000 files added. Reaching any of them stops that part of the search and sets `truncated`.
- **Reporting.** `RegisterOutcome.folders` (present only when the drop contained folders) carries
  `{ folders, added, ignored, truncated }`. The UI says when no accepted file was found, how many files of other
  types were left out, and when a limit stopped the search.

## Rationale
Selecting by kind in Rust keeps other files from using up the file limit and keeps the notice to one line instead
of hundreds of rejected names. Not following folder links makes loops impossible and keeps the search inside what
the user dropped; the canonical-path set covers the remaining ways to reach a folder twice. The limits bound time
and memory on any tree, including network drives, and the search runs off the event-loop thread.

## Alternatives considered
- **Register everything and filter in the UI:** files of other kinds use up the file limit before the PDFs are
  reached, and the notice would list hundreds of names.
- **Two-step drop (event with a drop id, then `register_drop(id, kinds)`):** no shared "current kinds" state, but it
  changes the event contract and every drop test. The race it avoids, switching tools during a drop, only changes
  which files are offered; the UI still filters by its own kinds.
- **Following folder links with a visited set:** reaches files outside the dropped folder, which is surprising and
  harder to explain.
- **Asking macOS whether a folder is a package (Launch Services):** exact, but needs platform-specific code and a
  new dependency; the extension list covers the packages people keep among documents.

## Consequences
- Folder drops add files; the open dialog still picks files only.
- Windows files marked hidden by attribute (not by a leading dot) are searched; their types are usually not
  accepted anyway.
- Files with the same name from different subfolders look alike in the list; batch saving numbers the copies.
- The package list is a heuristic: a folder named like a package (for example `Plans.key`) is not searched.

## Validation / fitness criteria
- `src-tauri/src/folder_scan.rs` tests: order, hidden entries, packages, links and loops, unreadable folders, each
  limit, a folder reached twice.
- `src-tauri/src/file_registry.rs` tests: drop order, kinds, content check, package skipped, summary shape.
- `src/hooks/useFileIntake.test.tsx`: kinds sent when shown, notices for ignored, none found and truncated.

## Reconsider when
- Users ask for folders in the open dialog, or for relative paths in the file list.
- Drops of large folders hit the limits often.
- A tool needs packages or hidden files as input.
