# Roadmap

Product features, engine quality and platform work are separate tracks. Take the next unchecked step; update this
file before deviating.

## Phase 0 — App foundation ✅
Scope: Tauri shell, UI shell with feature groups, file intake (dialog + drop), themes, docs, CI, packaging.
Non-scope: any processing.
Definition of Done: app runs on macOS; intake works; tests green; installers build in CI config.
Verification: `pnpm typecheck && pnpm test && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

- [x] Cargo workspace: `fileforge-core` (no Tauri) + `src-tauri` shell
- [x] Icons generated from the master artwork
- [x] File registry, `pick_files` / `remove_file`, drop handling (ADR-0004)
- [x] Sidebar with groups, PDF tool panel with intake, notices, drop overlay
- [x] Light/dark themes, Chepio footer, English-only UI strings in one module
- [x] CI + release workflows (ADR-0005), knowledge base

## Phase 1 — PDF compression (first feature)
Scope: lossless structural optimization; Balanced/Maximum presets with JPEG recompression and downsampling;
per-file results; save one / save all to a folder; temp-file lifecycle.
Non-scope: font subsetting, OCR, encryption, PDF/A, progress per page, cancellation.
Definition of Done: engine guarantees tested (never larger, lossless keeps content, page count kept, malformed and
encrypted inputs rejected with typed errors); UI shows before/after and ratio; results saved via native dialog;
docs updated (interfaces, runtime, invariants).
Dependencies: Phase 0. Risks: memory on huge PDFs (input limit), unusual image color spaces (skip, never corrupt).
Verification: `cargo test -p fileforge-core && pnpm vitest run src/features/PdfCompress`

- [ ] Engine: lossless rewrite (prune, dedupe, object + xref streams, re-deflate) with tests
- [ ] Engine: image pass (JPEG re-encode, DPI-based downsampling, keep-if-smaller) with tests
- [ ] Engine: limits and typed errors (too large, encrypted, malformed), panic isolation
- [ ] Shell: `compress_pdf`, temp results, `save_result`, `save_results_to_folder`, `reveal_result`
- [ ] UI: preset picker with exact parameters, compress all, per-file status and results, save actions
- [ ] Docs: interfaces, runtime (limits, failure modes), domain invariants, CURRENT_STATE

## Phase 2 — Images
- [ ] Compress: JPEG (mozjpeg-class encoder), PNG (oxipng), WebP; lossless default
- [ ] Convert between JPEG, PNG, WebP, AVIF; HEIC input (decoder licensing to be checked first)

## Phase 3 — Video and audio
- [ ] Decide engine: bundled ffmpeg sidecar (LGPL build) vs alternatives — ADR first (licensing, size, signing)
- [ ] Compress video (CRF presets), convert containers/codecs; convert audio

## Platform track
- [ ] Code signing + notarization (macOS), Windows signing — before public distribution
- [ ] Auto-update (Tauri updater) after signing
- [ ] Logging to a file (`tauri-plugin-log`) for bug reports
- [ ] Folder drops expanded recursively in Rust
- [ ] `docs/DESIGN.md` via `/impeccable document` once Phase 1 UI settles
