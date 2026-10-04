# Architecture Decision Records

| ADR | Title | Status |
|---|---|---|
| [ADR-0001](ADR-0001-tauri-react-stack.md) | Tauri 2 with a Rust core and a React + TypeScript UI | Accepted |
| [ADR-0002](ADR-0002-pdf-engine-lopdf.md) | Pure-Rust PDF compression with lopdf | Accepted |
| [ADR-0003](ADR-0003-compression-presets.md) | Compression presets: lossless by default, never larger, originals untouched | Accepted; Screen preset added by ADR-0015 |
| [ADR-0004](ADR-0004-untrusted-file-isolation.md) | Rust owns all file access; the webview sees ids only | Accepted |
| [ADR-0005](ADR-0005-unsigned-ci-releases.md) | Installers built by GitHub Actions, unsigned until certificates exist | Accepted; distribution superseded by ADR-0009 |
| [ADR-0006](ADR-0006-safe-result-publication.md) | Protect session inputs and publish complete results without batch overwrites | Accepted |
| [ADR-0007](ADR-0007-original-file-identity.md) | Check filesystem identity when protecting originals on Unix | Accepted |
| [ADR-0008](ADR-0008-installers-folder.md) | Commit installers to `installers/` under stable names | Superseded by ADR-0009 |
| [ADR-0009](ADR-0009-github-release-downloads.md) | Publish installers as GitHub release assets under stable names | Accepted; asset replacement superseded by ADR-0010 |
| [ADR-0010](ADR-0010-immutable-releases.md) | Published release assets and tags are immutable | Accepted; asset set extended by ADR-0013 |
| [ADR-0011](ADR-0011-cooperative-cancellation.md) | Cooperative cancellation and channel-based progress for compressions | Accepted |
| [ADR-0012](ADR-0012-metadata-removal.md) | Optional removal of metadata, thumbnails and editing data | Accepted |
| [ADR-0013](ADR-0013-in-app-updates.md) | In-app updates from signed GitHub release artifacts | Accepted |
| [ADR-0014](ADR-0014-product-name.md) | The product name is "File Forge" and stays fixed once updates ship | Accepted |
| [ADR-0015](ADR-0015-jpeg2000-and-screen-preset.md) | JPEG 2000 images become JPEG, a Screen preset, and reference-accurate image decoders | Accepted |
| [ADR-0016](ADR-0016-flate-photographs.md) | Convert suitable Flate photographs to JPEG in Maximum | Accepted |
| [ADR-0017](ADR-0017-folder-drops.md) | Dropped folders are searched in Rust for the active tool's file types | Accepted |
| [ADR-0018](ADR-0018-cff-subroutine-pruning.md) | Unreachable CFF subroutines are removed in every preset | Accepted |
