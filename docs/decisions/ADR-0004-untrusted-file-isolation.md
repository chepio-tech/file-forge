# ADR-0004: Rust owns all file access; the webview sees ids only

## Status
Accepted

## Date
2026-10-03

## Context
The app reads arbitrary user files and writes results anywhere the user chooses. If the webview could pass paths to
commands, any script running in it (a bug, a malicious file name rendered unsafely, a future remote resource) could
read or overwrite any file the user can.

## Decision
- The webview has no `fs:*` or `dialog:*` permissions (`src-tauri/capabilities/default.json`).
- Files enter only through Rust: the native open dialog and the window's drop event. Rust registers them in a
  session registry and gives the webview opaque numeric ids plus display data (name, size, kind).
- Results are written by Rust after a native save dialog; commands take ids, never paths.
- Strict CSP; no remote content.
- Release builds keep `panic = "unwind"` so a parser panic on a malformed file becomes an error for that file
  instead of terminating the app.

## Rationale
Least privilege at the trust boundary between the UI and the filesystem, at the cost of a little plumbing.

## Alternatives considered
- Webview-side `@tauri-apps/plugin-dialog` + `plugin-fs` with scopes: less Rust code, but scopes must be widened to
  wherever users keep files, which is effectively everywhere.

## Consequences
- Every new file operation needs a Rust command and a registry lookup.
- Dropped folders are reported as skipped until folder expansion is implemented in Rust.

## Validation / fitness criteria
- `src-tauri/capabilities/default.json` contains no `fs:` or `dialog:` permission.
- No command in `src-tauri/src/commands.rs` takes a `PathBuf`/`String` path argument.

## Reconsider when
- Never for convenience alone; only if Tauri offers a mechanism with the same guarantee and less code.
