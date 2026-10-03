# fileforge-core — processing engines (Rust)

Scope: file classification and processing engines as pure functions over bytes/paths.

## Rules
- No `tauri`, no UI, no global state, no threads it does not own. Inputs are untrusted user files: return
  `Result`, never panic on malformed input, and bound every allocation driven by file content.
- Every engine guarantees: output is never larger than input, and the lossless preset never changes decoded
  content. Each guarantee has a test.
- Tests build their fixtures in code (see `src/file_kind.rs` tests); do not commit binary sample files.

## Checks
- `cargo test -p fileforge-core` · `cargo clippy -p fileforge-core --all-targets -- -D warnings`
