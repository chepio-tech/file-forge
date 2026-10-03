# fileforge-core — processing engines (Rust)

Scope: file classification and processing engines as pure functions over bytes/paths.

## Rules
- No `tauri`, no UI, no global state, no threads it does not own. Inputs are untrusted user files: return
  `Result`, never panic on malformed input, and bound every allocation driven by file content.
- Every engine guarantees what `docs/domain/invariants.md` lists; each guarantee has a test. Canonical engine:
  `src/pdf/mod.rs` (pipeline), tests in `tests/pdf_compress.rs` with fixtures from `tests/support/mod.rs`.
- Do not use lopdf's `traverse_objects` / `prune_objects` / `delete_object`: they are quadratic in the object
  count. Use `src/pdf/objects.rs`.
- Measure before optimizing: `cargo run --release -p fileforge-core --example measure_pdf -- <files>`.
- Tests build their fixtures in code; do not commit binary sample files.

## Checks
- `cargo test -p fileforge-core` · `cargo clippy -p fileforge-core --all-targets -- -D warnings`
