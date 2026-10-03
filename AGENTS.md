# AGENTS.md

## Sources of truth
- Product intent and design principles: `docs/PRODUCT.md`
- Design tokens (colors, type, spacing, motion): `src/App/App.css`
- IPC contract: `src-tauri/src/commands.rs` + `src-tauri/src/drag_drop.rs` (Rust) ↔ `src/services/fileforgeApi.ts` (TS)
- Webview permissions: `src-tauri/capabilities/default.json`; app and bundle config: `src-tauri/tauri.conf.json`
- Tools and their groups: `src/features/featureCatalog.ts`
- Decisions: `docs/decisions/` · State and plan: `docs/CURRENT_STATE.md`, `docs/ROADMAP.md`
- Versions: `Cargo.lock`, `pnpm-lock.yaml`, `.github/workflows/`

## Commands
- Install: `pnpm install`
- Run the app: `pnpm tauri dev`
- Frontend tests / one file: `pnpm test` / `pnpm vitest run src/path/to/File.test.tsx`
- Typecheck: `pnpm typecheck`
- Rust tests: `cargo test --workspace` · core only (fast, no Tauri): `cargo test -p fileforge-core`
- Measure the PDF engine on real files (read-only): `cargo run --release -p fileforge-core --example measure_pdf -- <files>`
- Rust lint: `cargo clippy --workspace --all-targets -- -D warnings` · format: `cargo fmt --all`
- Installers for this OS: `pnpm tauri build` (all three OSes: `.github/workflows/release.yml`)

## Global invariants
- The webview never receives or sends filesystem paths and has no fs/dialog permissions; files are identified by
  registry ids and all file I/O happens in Rust (ADR-0004).
- `crates/fileforge-core` must not depend on `tauri` or any UI crate: processing stays testable without a window.
- Originals are never modified; results go to a temp file until the user saves them (ADR-0003).
- A processing result is never larger than its input; lossless is the default preset (ADR-0003).
- The UI is English-only. Every user-visible string lives in `src/messages/messages.ts`; never hard-code UI text.
- The Chepio credit is `<ChepioTechFooter />`, the last element of the app footer.

## Documentation routing
Read only what the task needs:
- module boundaries, dependency directions → `docs/architecture/components.md`
- IPC commands, events, error codes → `docs/architecture/interfaces.md`
- trust boundaries, capabilities, untrusted input → `docs/architecture/security.md`
- limits, failure behavior, concurrency, measurements → `docs/architecture/runtime.md`
- engine guarantees (never larger, lossless, refused files) → `docs/domain/invariants.md`
- packaging, signing, releases → `docs/architecture/deployment.md`
- UI work → `docs/PRODUCT.md`, tokens in `src/App/App.css`, rules in `src/AGENTS.md`
- Rust shell rules → `src-tauri/AGENTS.md` · core engine rules → `crates/fileforge-core/AGENTS.md`
- why something is built this way → `docs/decisions/`

## Validation
- While iterating, run the narrowest check above; before finishing, run every check the change affects.
- An IPC change is done only when both sides and `docs/architecture/interfaces.md` agree.
- Never disable or weaken a check to make a change pass.

## Planning
Large work (a new engine, several subsystems, security-sensitive changes) → write a plan in `plans/` first.

## Documentation maintenance
In the same commit, update docs when a change alters the IPC contract, a capability, a component boundary, an
invariant, packaging, or a decision. Tick the step in `docs/ROADMAP.md` and refresh `docs/CURRENT_STATE.md`.

## Safety boundaries
- Needs explicit approval: new dependencies, new webview permissions, publishing releases, signing setup.
- Never: grant the webview `fs:*` or `dialog:*` permissions; set `panic = "abort"`; upload user files anywhere.
