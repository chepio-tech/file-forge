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
- `set_drop_kinds(kinds)` — the shown tool's file kinds; dropped folders contribute only files with their
  extensions (ADR-0017). Until set, folders contribute nothing.
- `remove_file` — forget a registered file and its unsaved result.
- `compress_pdf(id, options, onProgress)` → `PdfReport`. Options:
  `{ images: null | { jpegQuality, maxDpi | null, compressFlatePhotos }, stripMetadata, stripEditingData }`;
  `compressFlatePhotos` defaults to false when omitted and enables the conservative Flate photograph conversion
  defined in ADR-0016. Maximum enables it; Custom retains it when editing Maximum's numbers. `images: null` is lossless,
  and the two removal flags (default `false` when omitted) work with any preset (ADR-0012). Ranges are defined in
  `crates/fileforge-core/src/pdf/options.rs` and validated there; the UI mirrors them in
  `src/features/PdfCompress/pdfPresets.ts`. The report adds `metadataRemoved`, `metadataKeptForStandard`,
  `thumbnailsRemoved` and `editingDataRemoved` to the size and image counts; a kept original reports no removal.
  `onProgress` is a Tauri `Channel` receiving `{ stage, done, total }` (stages: `loading`, `structure`, `images`,
  `streams`, `saving`, `verifying`; `total: 0` = not counted), throttled to stage changes, stage completion and one
  update per 100 ms.
  Progress may arrive after the command settles; the UI ignores it then (ADR-0011).
- `compress_image(id, options, onProgress)` → `RasterReport` (ADR-0019, ADR-0020). Options:
  `{ jpegQuality: 30–95 | null, webpQuality?: 30–95 | null, pngLevel: 0–6, pngZopfli?, stripMetadata? }`; a `null`
  quality keeps that format's pixels exactly (lossless WebPs always stay lossless), omitted `webpQuality` means `null`,
  the two flags default to `false`. Ranges are defined and validated in `crates/fileforge-core/src/raster/options.rs`;
  the UI mirrors them in `src/features/ImageCompress/imagePresets.ts`.
  The report: `{ format: "jpeg" | "png" | "webp", width, height, originalSize, outputSize, kept, reencoded,
  metadataRemoved }`; `kept` is `null` for a new result, otherwise why the original stays: `notSmaller`, `extraData`,
  `signed`, `animated`, `unsupportedEncoding` or `lossyEncoding` (a lossy WebP and no WebP quality). Progress stages:
  `loading`, `encoding`, `verifying`. The format comes from the file's content, and the result keeps it.
- `cancel_compression` — cancels every compression (PDF or image) already started at its next checkpoint; those reject with
  `cancelled`. Later calls are unaffected; a no-op when nothing runs.
- `save_result(id)` — native save dialog next to the original, filtered to the result's format → saved file name,
  or `null` if cancelled.
  Choosing any session input (including aliases, macOS case variants and files removed from the list) returns `originalTarget`.
- `save_results_to_folder(ids)` — folder picker, `<name>-compressed.<ext>` without overwriting → `SavedFile[]` or
  `null`. `<ext>` is `pdf`, `png`, `webp`, or `jpg` (`jpeg` when the original used it).
- `reveal_result(id)` — show the last saved copy in the file manager.
- `check_for_update` → `{ currentVersion, availableVersion | null }`; remembers the found update in Rust (ADR-0013).
- `install_update(discardUnsaved)` — downloads, verifies and installs the update found by the last check, then
  restarts, so it settles only on failure: `busy` while the work slot is held, `unsavedResults` when results were
  never saved and `discardUnsaved` is `false`, `update` for network, signature or installer failures.
- Event `files-added` — files dropped on the window, already registered → `RegisterOutcome`. Dropped folders
  arrive expanded; `folders: { folders, added, ignored, truncated }` is present only when the drop contained
  folders (ADR-0017).
- The UI also listens to the webview's drag-enter/leave events for hover feedback only (paths ignored).

Error codes: `unknownFile`, `noResult`, `originalTarget`, `notAFile`, `pdfTooLarge`, `pdfEncrypted`, `pdfSigned`, `pdfMalformed`,
`imageTooLarge` (bytes or pixels, named in `detail`), `imageUnsupported` (not JPEG, PNG or WebP by content; `detail` names the
format when known), `imageMalformed`,
`invalidOptions`, `cancelled`, `busy`, `unsavedResults`, `update`, `io`, `internal`.

## Compatibility
UI and shell ship in one binary, so there is no versioning between them; a contract change is a single commit that
updates both sides, their tests and this page.
