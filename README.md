# FileForge

<img src="docs/assets/app-icon.png" alt="FileForge app icon" width="112" height="112" />

Desktop PDF compression, entirely on your computer. Built for macOS, Windows and Linux.
Image, video and audio tools are planned; they are shown as **Soon** in the app.

## What works

- Add PDFs through a native file picker or drag and drop a batch onto the window.
- Choose Lossless, Balanced, Maximum or Custom settings with exact JPEG quality and DPI values.
- Compare original/result sizes per file and for the batch, then save one result or all results to a folder.
- Keep originals untouched. Results are never larger; encrypted and digitally signed PDFs are refused.
- Work locally, with system light/dark themes and no file uploads.

## Install

Open [FileForge releases](https://github.com/denys-chepiha/fileforge/releases) for installer packages:

[![Download for macOS](docs/assets/download-macos.svg)](https://github.com/denys-chepiha/fileforge/releases)
[![Download for Windows](docs/assets/download-windows.svg)](https://github.com/denys-chepiha/fileforge/releases)
[![Download for Linux](docs/assets/download-linux.svg)](https://github.com/denys-chepiha/fileforge/releases)

The buttons open the releases page so you can choose the correct architecture and package. Until a release is
published, use the source-build instructions below.

| OS | File |
|---|---|
| macOS (Apple Silicon / Intel) | `FileForge_<version>_aarch64.dmg` / `FileForge_<version>_x64.dmg` |
| Windows | `FileForge_<version>_x64-setup.exe` or `.msi` |
| Linux | `.AppImage`, `.deb` or `.rpm` |

The app requires macOS 11+, Windows 10+ (WebView2), or Linux with the
[Tauri runtime prerequisites](https://v2.tauri.app/start/prerequisites/).

Builds are not signed yet:
- **macOS**: on first launch right-click the app → **Open**, or allow it in System Settings → Privacy & Security.
- **Windows**: SmartScreen → **More info** → **Run anyway**.

## Develop

Requirements: Rust (stable, 1.88+), Node.js LTS, pnpm 10, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
git clone https://github.com/denys-chepiha/fileforge.git
cd fileforge
pnpm install
pnpm tauri dev          # run the app
pnpm test               # UI tests
cargo test --workspace  # Rust tests
pnpm tauri build        # installers for the current OS → target/release/bundle/
```

Checks before contributing:

```sh
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

GitHub Actions checks pull requests and `main`. The release workflow builds installers for macOS Apple Silicon,
macOS Intel, Windows x64 and Linux x64 and attaches them to a draft release. Publishing requires a separate review;
see [deployment](docs/architecture/deployment.md).

## Current limits

- PDF inputs are limited to 1 GiB; compression runs one document at a time.
- A running document cannot be cancelled yet. Folder drops are skipped.
- Unsupported image encodings are preserved; lossy presets do not guarantee additional savings on every PDF.
- Batch saving requires a destination filesystem that supports hard links (for example APFS, NTFS or ext4).
- The application's license has not been chosen. Dependency licenses remain their respective authors' licenses.

Project docs: [current state](docs/CURRENT_STATE.md), [roadmap](docs/ROADMAP.md), [architecture](docs/architecture/components.md)
and [decisions](docs/decisions/README.md). Contributor and agent rules: [AGENTS.md](AGENTS.md).

---

Developed by [Chepio](https://chepio.tech).
