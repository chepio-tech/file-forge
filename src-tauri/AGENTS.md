# src-tauri — desktop shell (Rust)

Scope: Tauri wiring, IPC commands, window events and session state. No processing logic here.

## Rules
- Commands stay thin: look up registry ids, call `fileforge-core`, map errors to `AppError`. Example:
  `src/commands.rs`.
- Blocking work (dialogs, disk I/O, compression) runs in `tauri::async_runtime::spawn_blocking`, never on the
  event-loop thread or directly in an async command. Example: `src/drag_drop.rs`.
- Commands accept ids, never paths, from the webview (ADR-0004). Paths enter only through Rust-side dialogs and
  window drop events.
- `AppError` is the only error type crossing IPC; a new variant needs a message under `errors` in
  `src/messages/messages.ts` and a line in `docs/architecture/interfaces.md`.
- Shared state uses `std::sync::Mutex` with short critical sections and no `.await` while locked.

## Checks
- `cargo test -p fileforge` · `cargo clippy -p fileforge --all-targets -- -D warnings`
