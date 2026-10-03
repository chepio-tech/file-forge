# Interfaces (IPC)

Source of truth: `src-tauri/src/commands.rs`, `src-tauri/src/drag_drop.rs`, `src-tauri/src/error.rs` (Rust) and
`src/services/fileforgeApi.ts` (TS). This page holds the rules, not a copy of the signatures.

## Conventions
- Command names are `snake_case` in Rust and invoked with the same name; arguments are `camelCase` in JS and
  `snake_case` in Rust (Tauri's default mapping). Payload structs use `#[serde(rename_all = "camelCase")]`.
- Files are referenced by `FileId` (a session-scoped number), never by path (ADR-0004).
- Commands that may block (dialogs, disk, CPU) are `async` and run their work in `spawn_blocking`.
- Errors are `AppError`, serialized as `{ code, detail }`. The UI shows a localized message per `code`
  (`errors.*` in `src/i18n/*.ts`); `detail` is for diagnostics only and never shown verbatim.

## Commands and events today
- `pick_files` — native open dialog filtered to file kinds → `RegisterOutcome`.
- `remove_file` — forget a registered file.
- Event `files-added` — files dropped on the window, already registered → `RegisterOutcome`.
- The UI also listens to the webview's drag-enter/leave events for hover feedback only (paths ignored).

## Compatibility
UI and shell ship in one binary, so there is no versioning between them; a contract change is a single commit that
updates both sides, their tests and this page.
