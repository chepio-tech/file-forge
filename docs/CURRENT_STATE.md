# Current state

_Updated: 2026-10-03_

## Works now
- Tauri 2 desktop shell (macOS, Windows, Linux) with the FileForge icon, macOS overlay title bar, strict CSP.
- Sidebar grouping tools by file type: Documents, Images, Video, Audio. Only "Compress PDF" is selectable; the rest
  are marked "Soon".
- File intake for the PDF tool: native open dialog and drag & drop onto the window, registered on the Rust side;
  non-PDF files and folders are skipped with an explanation; list with sizes, remove and clear.
- English and Russian UI, following the system language with a manual switch (remembered).
- Light and dark themes following the system. Chepio developer credit in the footer.
- CI workflow (typecheck, Vitest, fmt, clippy, cargo test) and a release workflow building installers for all OSes.

## In progress
- PDF compression engine and save flow: ROADMAP Phase 1.

## Known issues
- Builds are not signed or notarized (ADR-0005): Gatekeeper/SmartScreen warnings on first launch.
- No automated end-to-end UI test on macOS (no WKWebView WebDriver); dialog and drag & drop are verified manually.
- Dropped folders are skipped, not expanded.

## Open questions
- License for the app itself: not chosen yet (no `license` field in `Cargo.toml`).
- GitHub repository for CI and releases is not set up yet.

## Next step
ROADMAP → Phase 1, step 1.
