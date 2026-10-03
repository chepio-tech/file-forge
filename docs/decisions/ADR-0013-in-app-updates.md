# ADR-0013: In-app updates from signed GitHub release artifacts

## Status
Accepted. Extends ADR-0010's release asset set; changes the "App → network: none" boundary in
`docs/architecture/security.md`.

## Date
2026-10-03

## Context
Users install FileForge from GitHub release assets (ADR-0009). Without an updater, each version stays installed
until the user notices a release and reinstalls it by hand, and the updater only works from the first version that
contains it. Updating requires the app to contact the network, which it never did before, and to replace itself.
An update channel can also deliver malware to every installed copy if its artifacts are not authenticated.

## Decision
Use the official `tauri-plugin-updater` (2.x) from Rust only; the webview gets no updater permission.
- **Source.** Endpoint `https://github.com/chepio-tech/file-forge/releases/latest/download/latest.json`. The
  manifest lists one artifact per `<os>-<arch>-<bundle type>` target (`.app.tar.gz`, AppImage, `.deb`, `.rpm`, NSIS,
  MSI), so every installed package updates in its own format. Each entry points at its own release's asset URL.
  The repository must be public for the endpoint to be reachable; until then checks fail quietly.
- **Authenticity.** Updater artifacts are signed with a minisign key whose public half is
  `plugins.updater.pubkey`; the plugin refuses unsigned or mismatched artifacts. `requireSignedVersion` binds each
  signature to the version announced by the manifest, which blocks downgrades to older genuine releases. The
  private key and its password live in the owner's offline backup and in the `TAURI_SIGNING_PRIVATE_KEY*` Actions
  secrets only. Only release builds create updater artifacts (`src-tauri/tauri.release.conf.json`).
- **Release gate.** `.github/scripts/updaterManifest.mjs` writes `latest.json` in the publish job and refuses when
  any artifact would be rejected by installed apps: wrong key, invalid signature, missing or different signed
  version. The Tauri CLI only warns about a key mismatch.
- **Consent and timing.** One check at startup (15 s timeout) whose failures and "up to date" result are not shown;
  a manual check reports both. Nothing is downloaded until the user clicks "Restart to update". Installing is
  refused while a compression, save or removal holds the work slot, and asks before discarding unsaved results.
  The work slot stays held from installing until the restart. The download times out after 10 minutes.
- **Request content.** The check is a plain HTTPS GET whose only identifying header names the plugin and its
  version; no user files, file names or identifiers are sent.

## Rationale
The official plugin verifies signatures before installing and supports every bundle type FileForge ships. GitHub
releases are already the distribution channel, immutable (ADR-0010) and free; no server is needed. Version-bound
signatures and the release gate turn the two silent failure modes (downgrade, stranded installs) into errors.

## Alternatives considered
- Manual check only: keeps the app offline by default, but most users would stay on old versions.
- A separate public releases repository: keeps the source private, but adds a cross-repository token and two
  places to keep consistent.
- Cloudflare R2 or another host: independent of GitHub, but adds infrastructure and credentials.
- Wait for OS code signing first: the updater's signature does not depend on it, and every release shipped without
  the updater needs a manual reinstall later.
- Sign updater artifacts in a separate step so build scripts never see the key: smaller exposure, but it
  reimplements the bundler's macOS archive and signing; reconsider if dependency risk grows.
- Hot-loading UI code from the network: breaks the CSP and the trust model; most logic is in Rust anyway.

## Consequences
- The app makes one outbound HTTPS request per check to GitHub, which sees the user's IP address, the plugin name
  and version, and the time. Offline use is unaffected.
- Losing the private key or its password strands installed copies: they accept no further update and need a
  manual reinstall of a build with a new key. Rotating the key needs one release signed with the old key that
  ships the new public key. A leaked key lets an attacker sign updates, but they still need to serve them through
  the release endpoint.
- `.deb`/`.rpm` updates run `dpkg -i`/`rpm -U` with administrator rights through the plugin (`pkexec` prompt,
  then a password dialog). MSI updates may show a UAC prompt; NSIS installs per user show only progress.
- macOS builds stay ad-hoc signed (ADR-0005); an updated app may ask again for folder access it was granted.
- Releases gain `latest.json` and two macOS `.app.tar.gz` archives besides the seven installers.

## Validation / fitness criteria
- `pnpm vitest run docs/updaterManifest.test.ts src/components/UpdateStatus` and `cargo test -p fileforge` pass.
- A release build with `--config src-tauri/tauri.release.conf.json` writes signatures with `version:<version>`.
- After the first two public releases, an installed copy of the older one offers and installs the newer one on
  each OS.

## Reconsider when
- OS code signing is added: sign the bundles before the updater signs its artifacts.
- Distribution moves away from public GitHub releases.
- Users ask for a way to disable automatic checks (needs persisted settings).

## References
- https://v2.tauri.app/plugin/updater/
- https://docs.rs/tauri-plugin-updater/2.12.0/tauri_plugin_updater/
- https://jedisct1.github.io/minisign/
