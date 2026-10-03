# Security

## Trust boundaries
1. **User files → core engines.** Every input file is untrusted (malformed, hostile, or huge). Engines return
   `Result`, bound allocations, and run in `spawn_blocking`; release builds unwind on panic so one bad file cannot
   crash the app (ADR-0004).
2. **Webview → shell.** The webview is treated as less trusted than Rust. It holds no filesystem or dialog
   permissions and passes only ids (ADR-0004).
3. **App → network.** None. The only outbound action is opening `https://chepio.tech` in the system browser,
   allow-listed in `src-tauri/capabilities/default.json`. Files are never uploaded.

## Webview hardening
- CSP in `src-tauri/tauri.conf.json`: `default-src 'self'`, IPC only, images from self/data. No remote scripts.
- Context menu disabled in production builds (`src/main.tsx`).
- React escapes all file names; never render file-derived strings as HTML.

## Data handling
- Registry entries and temp results live for the session only and are not persisted.
- Originals are opened read-only; nothing writes to an input path.

## Planned
- Temp result files: per-session directory, deleted on removal and on exit (PDF compression task).
- Input size limits per engine (PDF compression task).
