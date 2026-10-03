# Plan: Complete the first FileForge version

## Goal
Finish the existing PDF compression version, verify a native installer, and push the tested project to
https://github.com/chepio-tech/file-forge using the organization's dedicated SSH identity.
Image, video and audio engines remain separate roadmap phases requiring their own engine/dependency decisions.

## Context
ROADMAP Phases 0 and 1 are implemented. The existing `feature/downloads` branch has unfinished README assets.
ADR-0003 requires originals to remain untouched; ADR-0004 keeps paths and file I/O in Rust; ADR-0005 requires
approval before publishing a release.

## Constraints
- Keep the existing Rust/Tauri/React stack and dependencies.
- Do not publish a release or change signing/permissions as part of a Git push.
- Preserve all input files and existing unrelated work.
- UI strings and typed errors must agree across Rust, TypeScript and interface docs.

## Affected areas
Rust result saving and input reads, PDF UI failure handling, README, CI, state and interface/runtime/domain docs.

## Steps
- [x] Inspect the current roadmap, code, Git state and baseline tests.
- [x] Prevent saving over registered originals and prevent batch output name collisions from overwriting files.
- [x] Bound input reads even when a file changes after registration; test the boundary behavior.
- [x] Handle save/reveal failures and pending saves in the UI; add regression tests.
- [x] Finish README download navigation and verify the installer/CI setup.
- [x] Run affected checks, build a macOS installer, and smoke-test the app.
- [ ] Refresh state/roadmap/contract docs, commit the tested changes, merge into main and push to the organization repository.

## Validation
`pnpm typecheck`, `pnpm test`, `pnpm build`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`,
`pnpm tauri build`, native app smoke test, and remote Git commit verification.

## Risks
- GitHub CLI's HTTPS authentication is invalid. The dedicated organization SSH key is authenticated as
  `denys-chepiha`; the remote uses `github-chepio-tech` and the new repository is empty.
- Installers are unsigned/not notarized; publishing remains an explicit separate action.
- Windows/Linux installer execution requires their native runners; report what was actually verified.

## Rollback
Revert the completion commit; no data migrations or new dependencies are involved.

## Progress
Implementation, docs, 36 frontend tests, 52 macOS Rust tests, typecheck, production build, format and clippy are green.
The macOS Apple Silicon app/DMG builds; native intake/compression/safe save/batch save checks passed. The verified
implementation (`f16935b`) passed GitHub CI in the previous repository. Remaining: commit the repository-link
updates, push to the organization repository and verify its CI.

## Discoveries
- The repository was initially delivered to `denys-chepiha/fileforge`. The user then selected
  `chepio-tech/file-forge` as the destination; the remote and README now use the organization repository.
- The dedicated `github-chepio-tech` SSH identity works; no key regeneration or global SSH override was needed.
- Safe result publication and its filesystem tradeoff are recorded in ADR-0006.
- macOS canonical paths retain requested filename casing on the tested case-insensitive volume. Original
  protection also compares Unix device/inode identities (ADR-0007); case-variant and hard-link tests cover this.
