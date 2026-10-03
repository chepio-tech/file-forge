# Interfaces (IPC)

Source of truth: `src-tauri/src/commands.rs`, `src-tauri/src/drag_drop.rs`, `src-tauri/src/error.rs` (Rust) and
`src/services/fileforgeApi.ts` (TS). This page holds the rules, not a copy of the signatures.

## Conventions
- Command names are `snake_case` in Rust and invoked with the same name; arguments are `camelCase` in JS and
  `snake_case` in Rust (Tauri's default mapping). Payload structs use `#[serde(rename_all = "camelCase")]`.
- Files are referenced by `FileId` (a session-scoped number), never by path (ADR-0004).
- Commands that may block (dialogs, disk, CPU) are `async` and run their work in `spawn_blocking`.
- Errors are `AppError`, serialized as `{ code, detail }`. The UI shows a localized message per `code`
  (`errors.*` in `src/messages/messages.ts`); `detail` is for diagnostics only and never shown verbatim.

## Commands and events today
- `pick_files` — native open dialog filtered to file kinds → `RegisterOutcome`.
- `remove_file` — forget a registered file and its unsaved result.
- `compress_pdf(id, options, onProgress)` → `PdfReport`. Options: `{ images: null }` (lossless) or
  `{ images: { jpegQuality, maxDpi | null } }`. Ranges are defined in `crates/fileforge-core/src/pdf/options.rs`
  and validated there; the UI mirrors them in `src/features/PdfCompress/pdfPresets.ts`. `onProgress` is a Tauri
  `Channel` receiving `{ stage, done, total }` (stages: `loading`, `structure`, `images`, `streams`, `saving`,
  `verifying`; `total: 0` = not counted), throttled to stage changes, stage completion and one update per 100 ms.
  Progress may arrive after the command settles; the UI ignores it then (ADR-0011).
- `cancel_compression` — cancels every compression already started at its next checkpoint; those reject with
  `cancelled`. Later calls are unaffected; a no-op when nothing runs.
- `save_result(id)` — native save dialog next to the original → saved file name, or `null` if cancelled.
  Choosing any session input (including aliases, macOS case variants and files removed from the list) returns `originalTarget`.
- `save_results_to_folder(ids)` — folder picker, `<name>-compressed.pdf` without overwriting → `SavedFile[]` or `null`.
- `reveal_result(id)` — show the last saved copy in the file manager.
- Event `files-added` — files dropped on the window, already registered → `RegisterOutcome`.
- The UI also listens to the webview's drag-enter/leave events for hover feedback only (paths ignored).

Error codes: `unknownFile`, `noResult`, `originalTarget`, `notAFile`, `pdfTooLarge`, `pdfEncrypted`, `pdfSigned`, `pdfMalformed`,
`invalidOptions`, `cancelled`, `io`, `internal`.

## Compatibility
UI and shell ship in one binary, so there is no versioning between them; a contract change is a single commit that
updates both sides, their tests and this page.
