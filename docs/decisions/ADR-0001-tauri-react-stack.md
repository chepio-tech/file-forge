# ADR-0001: Tauri 2 with a Rust core and a React + TypeScript UI

## Status
Accepted

## Date
2026-10-03

## Context
FileForge is a desktop app for macOS, Windows and Linux that compresses and converts PDFs, images, video and audio
locally. The work is CPU-heavy and reads untrusted files. The owner prefers Rust where it fits.

## Decision
Tauri 2 (Rust) for the shell and processing; React 19 + TypeScript + Vite for the UI; plain CSS with co-located
files and design tokens. Processing lives in a separate crate, `crates/fileforge-core`, with no Tauri dependency.
Cargo workspace at the repo root.

## Rationale
- Installers around 10 MB and low memory use versus 100+ MB and 150–300 MB RAM for Electron.
- Compression runs natively; Rust's memory safety matters for parsers of untrusted files.
- React matches the owner's other projects and the `ChepioTechFooter` component.
- A Tauri-free core crate compiles and tests in seconds and keeps engines reusable (CLI, other shells).

## Alternatives considered
- Tauri + Svelte: smaller UI code, but fewer libraries and the shared footer would need a port.
- Electron + React: faster prototyping, but heavy installers and memory, and CPU work would still need native
  modules or WASM.

## Consequences
- Two languages and toolchains (Rust, Node/pnpm); first Rust build takes minutes.
- macOS uses WKWebView, Windows WebView2 (Chromium), Linux WebKitGTK: CSS must work on all three; relative
  color syntax is not used because WebKitGTK support is inconsistent.
- No WebDriver for WKWebView: end-to-end UI tests on macOS are not available; UI is tested with Vitest + Testing
  Library and the IPC layer is mocked.

## Validation / fitness criteria
- `crates/fileforge-core/Cargo.toml` has no `tauri` dependency (reviewed on every dependency change).

## Reconsider when
- A required engine exists only as a large native SDK that Tauri cannot bundle as a sidecar.

## References
- Tauri app size guide: https://tauri.app/concept/size/
