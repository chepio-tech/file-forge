# FileForge

## Download

<p>
  <a href="https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-macOS-AppleSilicon.dmg"><img src="docs/assets/download-macos/download-macos.png" alt="Download for macOS" width="260" height="80" /></a>
  <a href="https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Windows-x64-setup.exe"><img src="docs/assets/download-windows/download-windows.png" alt="Download for Windows" width="260" height="80" /></a>
  <a href="https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Linux-x64.AppImage"><img src="docs/assets/download-linux/download-linux.png" alt="Download for Linux" width="260" height="80" /></a>
</p>

The buttons start the download right away: macOS for Apple Silicon, the Windows installer and the Linux AppImage.
Every installer is on the [latest release](https://github.com/chepio-tech/file-forge/releases/latest) page;
download another package here:

| OS | Download |
|---|---|
| macOS | [Apple Silicon (.dmg)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-macOS-AppleSilicon.dmg) · [Intel (.dmg)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-macOS-Intel.dmg) |
| Windows (x64) | [Installer (.exe)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Windows-x64-setup.exe) · [MSI package (.msi)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Windows-x64.msi) |
| Linux (x64) | [AppImage](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Linux-x64.AppImage) · [Debian/Ubuntu (.deb)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Linux-x64.deb) · [Fedora/openSUSE (.rpm)](https://github.com/chepio-tech/file-forge/releases/latest/download/FileForge-Linux-x64.rpm) |

Until the first release is published, the links return "Not Found"; use the source-build instructions below.

The app requires macOS 11+, Windows 10+ (WebView2), or Linux with the
[Tauri runtime prerequisites](https://v2.tauri.app/start/prerequisites/).

Builds are not signed yet:
- **macOS**: on first launch right-click the app → **Open**, or allow it in System Settings → Privacy & Security.
- **Windows**: SmartScreen → **More info** → **Run anyway**.

## About

<img src="docs/assets/app-icon.png" alt="FileForge app icon" width="112" height="112" />

Desktop PDF compression, entirely on your computer. Built for macOS, Windows and Linux.
Image, video and audio tools are planned; they are shown as **Soon** in the app.

## What works

- Add PDFs through a native file picker or drag and drop a batch onto the window.
- Choose Lossless, Balanced, Maximum or Custom settings with exact JPEG quality and DPI values.
- Compare original/result sizes per file and for the batch, then save one result or all results to a folder.
- Keep originals untouched. Results are never larger; encrypted and digitally signed PDFs are refused.
- Work locally, with system light/dark themes and no file uploads.

## Develop

Requirements: Rust (stable, 1.88+), Node.js LTS, pnpm 10, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
git clone https://github.com/chepio-tech/file-forge.git
cd file-forge
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
macOS Intel, Windows x64 and Linux x64 and publishes them as a GitHub release when a `v*` tag is pushed;
see [deployment](docs/architecture/deployment.md).

## Current limits

- PDF inputs are limited to 1 GiB; compression runs one document at a time.
- Cancel stops a running document at its next checkpoint; parsing and saving a document are not interrupted.
  Folder drops are skipped.
- Unsupported image encodings are preserved; lossy presets do not guarantee additional savings on every PDF.
- Batch saving requires a destination filesystem that supports hard links (for example APFS, NTFS or ext4).
- The application's license has not been chosen. Dependency licenses remain their respective authors' licenses.

Project docs: [architecture](docs/architecture/components.md) and [decisions](docs/decisions/README.md).

---

Developed by [Chepio](https://chepio.tech).
