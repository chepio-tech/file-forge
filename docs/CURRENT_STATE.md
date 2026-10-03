# Current state

_Updated: 2026-10-03_

## Works now
- Tauri 2 desktop shell (macOS, Windows, Linux) with the FileForge icon, macOS overlay title bar, strict CSP.
- Sidebar grouping tools by file type: Documents, Images, Video, Audio. Only "Compress PDF" is selectable; the rest
  are marked "Soon".
- File intake for the PDF tool: native open dialog and drag & drop onto the window, registered on the Rust side;
  non-PDF files and folders are skipped with an explanation; list with sizes, remove and clear.
- PDF compression: Lossless / Balanced / Maximum / Custom (JPEG quality, max DPI); files compressed one by one with
  per-file status, before → after and savings; save one (dialog next to the original) or all into a folder; show
  saved file. Encrypted and signed PDFs are refused with an explanation. Measured results:
  `docs/architecture/runtime.md`.
- English-only UI; all strings in `src/messages/messages.ts`.
- Light and dark themes following the system. Chepio developer credit in the footer.
- CI workflow (typecheck, Vitest, fmt, clippy, cargo test) and a release workflow building installers for all OSes.
- Safe saving: all session inputs are protected, including removed files and aliases; batch outputs never overwrite
  existing files, including simultaneous collisions and broken symbolic links (ADR-0006).
- PDF reads remain bounded if an input grows after registration. Saving/removal/compression are serialized in Rust;
  the UI shows pending saves and reports save/reveal errors without discarding results.
- README with platform download navigation, source-build commands, feature scope and current limits.
- GitHub repository: https://github.com/denys-chepiha/fileforge; the local remote uses SSH.

## In progress
- Push the verified first version and verify GitHub CI.

## Known issues
- Builds are not signed or notarized (ADR-0005): Gatekeeper/SmartScreen warnings on first launch.
- No automated end-to-end UI test on macOS (no WKWebView WebDriver); dialogs, drag & drop and the IPC round trip
  are verified manually.
- A running compression cannot be cancelled; very large documents show no progress within the file.
- Dropped folders are skipped, not expanded.
- Atomic batch saving requires a destination filesystem with hard-link support; individual saves remain available.
- Windows/Linux and macOS Intel installers have not been executed locally; the release matrix provides their builds.

## Open questions
- License for the app itself: not chosen yet (no `license` field in `Cargo.toml`).

## Verification (2026-10-03)
- `pnpm typecheck`, `pnpm test` (36 tests), `pnpm build`: passed.
- `cargo fmt --all -- --check`, clippy with `-D warnings`, locked workspace tests (50 tests): passed.
- `pnpm tauri build`: macOS Apple Silicon `.app` and `.dmg` built successfully.
- Native app smoke test on a generated PDF: pick → lossless compression (74,600 → 825 bytes) → refused original
  overwrite → individual save → numbered batch save. Original checksum unchanged; output copies identical;
  no staging files left behind. UI mechanical check: no findings.

## Next step
Finish the ROADMAP release-readiness delivery step; then Platform track (signing) or Phase 1.1.
