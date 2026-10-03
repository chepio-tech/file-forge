# ADR-0005: Installers built by GitHub Actions, unsigned until certificates exist

## Status
Accepted; distribution through draft GitHub releases superseded by ADR-0008.

## Date
2026-10-03

## Context
Tauri cannot cross-compile reliably to every target: macOS builds need macOS, Linux builds need WebKitGTK. Signing
needs an Apple Developer ID (USD 99/year, plus notarization) and a Windows code-signing certificate; neither exists yet.

## Decision
`.github/workflows/release.yml` builds on native runners (macOS arm64 + x64, Windows x64, Linux x64) with
`tauri-action` and attaches `.dmg`/`.app`, `.msi`/`.exe` (NSIS), `.deb`/`.rpm`/`.AppImage` to a draft GitHub release.
macOS builds are ad-hoc signed (`signingIdentity: "-"`) so Apple Silicon does not reject them as damaged.

## Rationale
Free, reproducible, one workflow for all targets; signing can be added later without changing the pipeline shape.

## Alternatives considered
- Local builds on three machines: not reproducible, slow.
- Cross-compiling Windows from macOS with `cargo-xwin`: experimental in Tauri, still no Linux.

## Consequences
- macOS: first launch shows a Gatekeeper warning; users must right-click → Open (or allow in System Settings →
  Privacy & Security). Windows: SmartScreen warns. Both are documented in the README.
- The repository must be on GitHub for releases.

## Validation / fitness criteria
- A draft release contains installers for all four matrix entries.

## Reconsider when
- Before any public distribution: add Developer ID signing + notarization and Windows signing secrets.

## References
- https://v2.tauri.app/distribute/pipelines/github/
