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

For this private repository, sign in to GitHub with an account that has repository access before downloading.

The app requires macOS 11+, Windows 10+ (WebView2), or Linux with the
[Tauri runtime prerequisites](https://v2.tauri.app/start/prerequisites/).

Builds are not signed yet:
- **macOS**: on first launch right-click the app → **Open**, or allow it in System Settings → Privacy & Security.
- **Windows**: SmartScreen → **More info** → **Run anyway**.

## About

<img src="docs/assets/app-icon.png" alt="FileForge app icon" width="112" height="112" />

Desktop PDF and image compression, entirely on your computer. Built for macOS, Windows and Linux.
Format conversion, video and audio tools are planned; they are shown as **Soon** in the app.

## What works

- Add PDFs through a native file picker or drag and drop files and folders onto the window. Folders are searched
  for PDFs, including subfolders; hidden files and app or document packages are left out.
- Choose Lossless, Balanced, Maximum, Screen or Custom settings with exact JPEG quality and DPI values. JPEG and
  JPEG 2000 images are re-encoded as JPEG; Screen (100 DPI) makes scans much smaller for on-screen reading.
- Maximum also converts suitable losslessly stored photographs to JPEG when it saves at least 20%. Detected
  screenshots and line art keep lossless image encoding; the preset's DPI downsampling still applies.
- Even Lossless trims unused data from embedded CFF fonts (common in PDFs saved on macOS) without changing a
  single glyph.
- Optionally remove metadata, page thumbnails and Illustrator/Photoshop editing data; PDF/A, PDF/UA and PDF/X
  files keep the metadata they require.
- Compress JPEG and PNG images. Lossless keeps every pixel: JPEGs get optimal Huffman tables, PNGs better filters
  and compression. Balanced (JPEG quality 85) and Maximum (75) re-encode JPEGs only when that saves at least 2%;
  PNGs always stay lossless. Metadata stays unless you remove it; color profiles and photo orientation always stay.
  Signed (Content Credentials), animated and HDR gain-map files are returned unchanged.
- Follow each file's progress, cancel a running batch, and compare original/result sizes per file and for the
  batch, then save one result or all results to a folder.
- Keep originals untouched. Results are never larger; encrypted and digitally signed PDFs are refused.
- Work locally, with system light/dark themes and no file uploads.
- Update from inside the app: FileForge checks the latest GitHub release at startup or on request and installs a
  newer, signature-verified version only when you choose **Restart to update**. The check sends no files.

## Develop

Requirements: Rust (stable, 1.92+), Node.js LTS, pnpm 10, and the
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

- PDF inputs are limited to 1 GiB, images to 256 MiB and 120 megapixels; compression runs one file at a time.
- Lossless leaves progressive JPEGs as they are unless metadata is removed. WebP, HEIC, GIF and other image formats
  are not compressed yet.
- Cancel stops a running document at its next checkpoint; parsing and saving a document are not interrupted.
- One drop adds up to 1,000 files from folders, searched up to 16 levels deep.
- Unsupported image encodings are preserved; lossy presets do not guarantee additional savings on every PDF.
- Photo/screenshot detection uses pixel heuristics and may misclassify unusual content; Lossless preserves every
  image's pixels. A screenshot displaying only a photograph is indistinguishable from that photograph.
- Batch saving requires a destination filesystem that supports hard links (for example APFS, NTFS or ext4).
- The application's license has not been chosen. Dependency licenses remain their respective authors' licenses.

Project docs: [architecture](docs/architecture/components.md) and [decisions](docs/decisions/README.md).

---

Developed by [Chepio](https://chepio.tech).
