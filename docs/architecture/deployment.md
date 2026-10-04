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
`src-tauri/tauri.conf.json`. `productName` "File Forge" names the bundles and the Windows install folder; it stays
fixed (ADR-0014), because a change would make Windows updates install a second copy.

### Installer appearance
Chepio.tech is the package publisher. `src-tauri/branding/installer-branding/` holds vector layouts and native
installer artwork. The Windows NSIS installer uses a right-aligned header signature and a welcome/
completion sidebar; the MSI uses a banner and dialog image with the native text area kept white. A small NSIS
include sets the header alignment and proportional image scaling. Windows artwork is rendered from vectors at
2× resolution for high DPI displays; the native layout proportions and MSI text area are preserved.
NSIS's aspect-fit helper changes the control size without filtering the bitmap. The include keeps that layout,
then resamples the header and welcome/finish bitmaps with GDI HALFTONE to the control's physical pixel size at
page creation. This prevents aliasing at fractional scales such as 150%; an exact-size source stays unchanged.
The result is 24-bit RGB, with temporary GDI resources freed and the existing bitmap retained on API failure.
The Windows release job runs `.github/scripts/installerDpiTest/installerDpiTest.ps1` after bundling. It executes
the actual NSIS helper with a checker fixture at 100%, 150% and 200%, verifies filtered pixels and bitmap sizes,
and repeats 120 resizes to check resource cleanup. To reproduce on Windows, run the same script after a bundle
build; its optional `-Makensis` argument selects the existing NSIS compiler.
Setup and uninstall explicitly use the application ICO for their window and executable icons, including
32-bit frames at native small-icon sizes; omitting these settings falls back to NSIS's stock icons.
Tauri's installer templates and installation behavior remain the defaults.
The macOS DMG uses a company signature in its bottom strip, below the standard app and Applications drag targets.
Its background TIFF contains standard and Retina representations; the signature stays clear of the bottom edge
after Finder accounts for its title bar.
The release build sets `TAURI_BUNDLER_DMG_IGNORE_CI=true` so Tauri also configures Finder's background and icon
positions on the macOS runners; otherwise Tauri skips that step on CI.
Linux AppImage has no installation wizard; Debian/RPM installation windows belong to the system package manager.
Asset formats, dimensions and the MSI text area are checked by `docs/installerBranding.test.ts`.
See the artwork folder's README for source files and regeneration commands.

## Releases (ADR-0005, ADR-0009, ADR-0010)
`.github/workflows/release.yml` builds all targets on native runners and copies the installers to stable,
version-free names. On a pushed `v*` tag that matches the `Cargo.toml` version, and only when every build succeeds,
it publishes the GitHub release `vX.Y.Z` with those assets. The release is not a draft. A manual run only builds
workflow artifacts. The README download links point to
`https://github.com/chepio-tech/file-forge/releases/latest/download/<name>`; the matrix `installers` entries in the
workflow are the only list of names, and `docs/readmeDownloads.test.ts` keeps the README in sync with it.
Workflow installer artifacts expire after one day; published release assets remain available.
The publication step runs `.github/scripts/publishRelease.sh`. It creates a release only after an explicit API
`404`, rejects version/tag mismatches and missing assets, and refuses to overwrite an existing release or draft.
Authentication and network failures stop publication. `gh release create` uploads assets to a draft before
publishing the complete immutable release.

### Updater artifacts (ADR-0013)
The release build adds `--config src-tauri/tauri.release.conf.json`, which turns on `createUpdaterArtifacts`, and
signs the updater artifacts with the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets.
Local `pnpm tauri build` stays unsigned and needs no key. The matrix `updates` entries name each updater target and
its asset: the macOS `.app.tar.gz` archives are extra assets; the AppImage, `.deb`, `.rpm`, NSIS and MSI installers
are reused. Before publication, `.github/scripts/updaterManifest.mjs` writes `latest.json` and fails the release if
any signature does not verify with `plugins.updater.pubkey` or is not bound to the tagged version
(`docs/updaterManifest.test.ts` covers it with throwaway-key fixtures). Installed apps read
`releases/latest/download/latest.json`, so the repository must be public.

Updater key:
- Create it once: `pnpm tauri signer generate -w ~/.tauri/fileforge-updater.key` with a password. Keep the key file
  and the password in an offline backup; losing either strands every installed copy (ADR-0013).
- Put the `.pub` file's content into `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`, and the key file's
  content and the password into the two repository secrets.
- Rotating: ship one release signed with the old key that contains the new public key, then switch the secrets.

Release checklist:
1. Bump `version` under `[workspace.package]` in `Cargo.toml` — the only version source (Tauri falls back to it).
2. CI green on `main`.
3. Tag `vX.Y.Z` on `main` and push it. This publishes the release, so it needs approval. The repository must be
   public and the updater secrets present.
4. Smoke-test one installer per OS from the README links, and that the previous version offers this update. A rerun for an existing release fails without changing it;
   fix a published release with a new patch version (ADR-0010).
5. If publication was interrupted and a draft remains, review it before an approved cleanup or retry.

## Repository settings
- The repository is public on GitHub Free, so release downloads and the updater endpoint need no GitHub sign-in
  (ADR-0013). The active `Protect main` ruleset blocks deletion and force pushes, with no bypass actors, and
  requires a pull request with successful `frontend` and `rust` checks against the latest `main`.
  GitHub is the source of truth for the live rules: https://github.com/chepio-tech/file-forge/rules/24429918.
- Pull requests use squash only, with the PR title as the commit title and commit messages as its body.
  Use `[fileforge]: …` titles. Merged source branches are automatically deleted.
- Organization members need explicit repository access. Only organization owners may create repositories.
- Dependabot vulnerability alerts and automated security fixes are enabled. `.github/dependabot.yml` configures
  weekly grouped version updates for GitHub Actions, Cargo and npm/pnpm with the `[fileforge]` commit prefix.
- The default Actions token has read-only access and cannot approve PR reviews. Both workflows explicitly request
  `contents: read`; only the release publication job requests `contents: write`.
- Allowed actions: GitHub-owned actions plus `pnpm/action-setup@*`, `dtolnay/rust-toolchain@*` and
  `swatinem/rust-cache@*`. Update this policy when adding an action from another owner.
- Every workflow action is pinned to a full commit SHA with a version comment; the Rust toolchain remains `stable`
  through its explicit input. `docs/workflowSecurity.test.ts` rejects mutable action references.
  After these workflows land on `main`, enable the repository's full-SHA requirement and verify a fresh CI run.
- Release immutability is enabled. See ADR-0010 for the asset/tag guarantees and recovery policy.

## Signing
- macOS: ad-hoc (`signingIdentity: "-"`). Not notarized: first launch needs right-click → Open.
- Windows: unsigned; SmartScreen shows "More info → Run anyway".
- Updater artifacts: minisign key, independent of OS signing (see "Updater artifacts" above).
- To add later: Apple Developer ID + notarization secrets, Windows certificate; see Tauri's signing guides.

## Icons
Master artwork: `public/icon.png`, transparent, no drop shadow, artwork centred on a square canvas. Generate all
platform icons from it: `pnpm tauri icon public/icon.png`, then delete the generated `android/` and `ios/` folders.
`public/app-icon.png` (UI) and `docs/assets/app-icon.png` (README) are copies of `128x128@2x.png`.
Check with `pnpm vitest run src-tauri/icons/appIcon.test.ts`. The company logo is `public/images/chepio-tech/main_logo.svg`.

## CI
`.github/workflows/ci.yml`: typecheck + Vitest + production frontend build; `cargo fmt --check`, clippy with
`-D warnings`, `cargo test`. Rust checks use the committed lockfile.
`docs/workflowSecurity.test.ts` also exercises publication against a local CLI stub: new release, existing release,
API failures, missing assets, version mismatch and failed publication. These tests perform no GitHub writes.
