# ADR-0008: Commit installers to `installers/` under stable names

## Status
Superseded by ADR-0009 before its first run; no installer was committed to Git.

## Date
2026-10-03

## Context
The README download buttons opened the releases page, where visitors still had to pick a file. The project owner
wants every installer in a root `installers/` folder and wants a README link to start the download.
GitHub serves a committed file as a download at `https://github.com/<owner>/<repo>/raw/<branch>/<path>`; a relative
README link opens a file view instead. Tauri puts the version in installer file names, so links to those names would
change with each version.

## Decision
`.github/workflows/release.yml` builds on the same four native runners and copies the installers to these names:
`FileForge-macOS-AppleSilicon.dmg`, `FileForge-macOS-Intel.dmg`, `FileForge-Windows-x64-setup.exe`,
`FileForge-Windows-x64.msi`, `FileForge-Linux-x64.AppImage`, `FileForge-Linux-x64.deb`, `FileForge-Linux-x64.rpm`.
After every build succeeds, a final job replaces the files in `installers/` on `main` and pushes one commit.
It runs only from `main` or a `v*` tag. README links use the `raw/main/installers/` URLs. The workflow no longer creates
GitHub releases.

## Rationale
Stable names keep the README links valid for every version. Committing only after all builds succeed keeps one
consistent set. CI produces all platforms reproducibly; this Mac cannot build Windows and Linux installers reliably.

## Alternatives considered
- GitHub Releases with `releases/latest/download/<file>`: GitHub's recommended channel for binaries and it keeps the
  Git history small. Rejected because the owner wants the installers inside the repository.
- CI uploads one archive, and a person commits it: no CI writes to `main`, but every release needs a manual step.
- Local builds on macOS (Windows through `cargo-xwin`, Linux through Docker): slow and not reproducible (ADR-0005).

## Consequences
- Each version adds roughly 100+ MB of binaries to Git history permanently, mostly the Linux AppImage. Clones grow
  accordingly. Removing old versions needs a history rewrite.
- GitHub rejects files over 100 MiB; the commit job fails with a clear error before pushing such a file.
- The workflow writes to `main` with `GITHUB_TOKEN`. Branch protection that blocks direct pushes would stop it.
  Commits pushed with `GITHUB_TOKEN` do not trigger other workflows.
- File names no longer show the version; the commit message records it (`Update installers to vX.Y.Z`).
- While the repository is private, the links work only for signed-in members with access.

## Validation / fitness criteria
- `pnpm vitest run docs/readmeDownloads.test.ts` checks that the README links exactly the installers the workflow
  commits.
- After a workflow run, `installers/` contains the seven files and every README link starts a download.

## Reconsider when
- Repository size slows clones or pushes, or a single installer approaches 100 MiB: move to GitHub Releases or Git LFS.
- Signing is added (ADR-0005): signed installers replace the unsigned ones under the same names.

## References
- https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github
- https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases
