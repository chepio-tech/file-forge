# Deployment

## Artifacts
`pnpm tauri build` produces installers for the host OS under `target/release/bundle/`:
macOS `.app` + `.dmg`; Windows `.msi` + NSIS `.exe`; Linux `.deb`, `.rpm`, `.AppImage`.
Bundle settings (identifier `tech.chepio.fileforge`, icons, category, macOS minimum 11.0) are in
`src-tauri/tauri.conf.json`.

## Releases (ADR-0005)
`.github/workflows/release.yml` builds all targets on native runners and uploads them to a **draft** GitHub release
named after the workspace version in `Cargo.toml`. Trigger: push a `v*` tag or run the workflow manually.
Publishing the draft is a manual, approval-requiring step.

Release checklist:
1. Bump `version` under `[workspace.package]` in `Cargo.toml` — the only version source (Tauri falls back to it).
2. CI green on `main`.
3. Tag `vX.Y.Z`, push, wait for the draft, smoke-test one installer per OS, publish.

## Signing
- macOS: ad-hoc (`signingIdentity: "-"`). Not notarized: first launch needs right-click → Open.
- Windows: unsigned; SmartScreen shows "More info → Run anyway".
- To add later: Apple Developer ID + notarization secrets, Windows certificate; see Tauri's signing guides.

## Icons
Master artwork: `icon.png` (repo root, white background). `src-tauri/icons/app-icon.png` is the transparent-background
derivative used to generate all platform icons: `pnpm tauri icon src-tauri/icons/app-icon.png`, then delete the
generated `android/` and `ios/` folders. `public/app-icon.png` is a copy of `128x128@2x.png` for the UI.

## CI
`.github/workflows/ci.yml`: typecheck + Vitest; `cargo fmt --check`, clippy with `-D warnings`, `cargo test`.
