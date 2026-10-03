# ADR-0009: Publish installers as GitHub release assets under stable names

## Status
Accepted; supersedes ADR-0008 and the draft-release distribution in ADR-0005. ADR-0005's native-runner builds and
unsigned status remain.
Asset replacement on a repeated tag is superseded by ADR-0010; the stable download URLs and publication model remain.

## Date
2026-10-03

## Context
README download buttons must start a download on click. ADR-0008 committed installers to `installers/` on `main`,
which adds roughly 100+ MB of binaries to Git history for every version and is limited to 100 MiB per file. GitHub
recommends releases for distributing binaries. Its `releases/latest/download/<asset>` URL downloads an asset of the
latest published release directly. Tauri puts the version in installer file names, so those names cannot be linked
across versions.

## Decision
`.github/workflows/release.yml` builds on the four native runners and copies the installers to stable names:
`FileForge-macOS-AppleSilicon.dmg`, `FileForge-macOS-Intel.dmg`, `FileForge-Windows-x64-setup.exe`,
`FileForge-Windows-x64.msi`, `FileForge-Linux-x64.AppImage`, `FileForge-Linux-x64.deb`, `FileForge-Linux-x64.rpm`.
When a pushed `v*` tag matches the `Cargo.toml` version and every build succeeds, a final job publishes the GitHub
release `vX.Y.Z` with those assets. It is not a draft. A manual run only builds workflow artifacts. README links use
`https://github.com/chepio-tech/file-forge/releases/latest/download/<name>`. Re-running a tag replaces its assets.

## Rationale
Release assets stay out of Git history, allow up to 2 GiB per file, and show the version on each release page.
Stable names keep the README links valid for every version. Publishing only after all builds succeed keeps every
release complete. The Tauri updater can later read its manifest from the same releases.

## Alternatives considered
- Installers committed to `installers/` (ADR-0008): permanent history growth and a 100 MiB per-file limit.
- Draft release published by hand: allows a manual check, but needs a step for every release. The links stay
  broken until someone publishes.
- Git LFS: keeps the folder, but every download consumes a paid bandwidth quota.
- Own storage (e.g. Cloudflare R2) with a download page: needed only for public distribution of closed source.

## Consequences
- Pushing a `v*` tag publishes a release immediately, so a tag push needs the same approval as a publication.
- `releases/latest/download/` follows the release marked Latest; prereleases and drafts are never used.
- Until the first release is published, the README links return "Not Found".
- While the repository is private, only signed-in members with access can download.

## Validation / fitness criteria
- `pnpm vitest run docs/readmeDownloads.test.ts` checks that the README links exactly the assets the workflow
  publishes.
- After a tag push, the release has seven assets and every README link starts a download.

## Reconsider when
- The app is distributed publicly while the source stays closed: publish from a separate public repository or own
  storage.
- Signing (ADR-0005) or auto-update is added: attach signatures and the updater manifest to the same release.

## References
- https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases
- https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github
