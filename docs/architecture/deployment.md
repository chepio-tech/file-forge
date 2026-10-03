# Deployment

## Repository access
Canonical repository: https://github.com/chepio-tech/file-forge. On the user's Mac, `origin` uses
`git@github-chepio-tech:chepio-tech/file-forge.git`, selecting the organization's dedicated SSH identity from
`~/.ssh/config`. Git's organization-only `url.*.insteadOf` settings also route standard `chepio-tech/` URLs to that
alias. Preserve personal `github.com` routing and keep commit author/signing settings independent.

## Artifacts
`pnpm tauri build` produces installers for the host OS under `target/release/bundle/`:
macOS `.app` + `.dmg`; Windows `.msi` + NSIS `.exe`; Linux `.deb`, `.rpm`, `.AppImage`.
Bundle settings (identifier `tech.chepio.fileforge`, icons, category, macOS minimum 11.0) are in
`src-tauri/tauri.conf.json`.

## Releases (ADR-0005, ADR-0009)
`.github/workflows/release.yml` builds all targets on native runners and copies the installers to stable,
version-free names. On a pushed `v*` tag that matches the `Cargo.toml` version, and only when every build succeeds,
it publishes the GitHub release `vX.Y.Z` with those assets. The release is not a draft. A manual run only builds
workflow artifacts. The README download links point to
`https://github.com/chepio-tech/file-forge/releases/latest/download/<name>`; the matrix `installers` entries in the
workflow are the only list of names, and `docs/readmeDownloads.test.ts` keeps the README in sync with it.

Release checklist:
1. Bump `version` under `[workspace.package]` in `Cargo.toml` — the only version source (Tauri falls back to it).
2. CI green on `main`.
3. Tag `vX.Y.Z` on `main` and push it. This publishes the release, so it needs approval.
4. Smoke-test one installer per OS from the README links. Running the workflow again for the tag replaces the assets.

## Signing
- macOS: ad-hoc (`signingIdentity: "-"`). Not notarized: first launch needs right-click → Open.
- Windows: unsigned; SmartScreen shows "More info → Run anyway".
- To add later: Apple Developer ID + notarization secrets, Windows certificate; see Tauri's signing guides.

## Icons
Master artwork: `icon.png` (repo root, white background). `src-tauri/icons/app-icon.png` is the transparent-background
derivative used to generate all platform icons: `pnpm tauri icon src-tauri/icons/app-icon.png`, then delete the
generated `android/` and `ios/` folders. `public/app-icon.png` is a copy of `128x128@2x.png` for the UI.

## CI
`.github/workflows/ci.yml`: typecheck + Vitest + production frontend build; `cargo fmt --check`, clippy with
`-D warnings`, `cargo test`. Rust checks use the committed lockfile.
