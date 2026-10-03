# FileForge

Desktop app for compressing and converting files — PDFs first, then images, video and audio — entirely on your
computer. macOS, Windows and Linux.

## Install

Download the installer for your system from the GitHub release:

| OS | File |
|---|---|
| macOS (Apple Silicon / Intel) | `FileForge_<version>_aarch64.dmg` / `FileForge_<version>_x64.dmg` |
| Windows | `FileForge_<version>_x64-setup.exe` or `.msi` |
| Linux | `.AppImage`, `.deb` or `.rpm` |

Builds are not signed yet:
- **macOS**: on first launch right-click the app → **Open**, or allow it in System Settings → Privacy & Security.
- **Windows**: SmartScreen → **More info** → **Run anyway**.

## Develop

Requirements: Rust (stable, 1.88+), Node.js LTS, pnpm 10, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm tauri dev          # run the app
pnpm test               # UI tests
cargo test --workspace  # Rust tests
pnpm tauri build        # installers for the current OS → target/release/bundle/
```

Installers for all platforms are built by GitHub Actions (`.github/workflows/release.yml`).

Project docs: `docs/` (state, roadmap, architecture, decisions). Contributor and agent rules: `AGENTS.md`.

---

Developed by [Chepio](https://chepio.tech).
