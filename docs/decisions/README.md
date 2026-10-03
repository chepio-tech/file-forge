# Architecture Decision Records

| ADR | Title | Status |
|---|---|---|
| [ADR-0001](ADR-0001-tauri-react-stack.md) | Tauri 2 with a Rust core and a React + TypeScript UI | Accepted |
| [ADR-0002](ADR-0002-pdf-engine-lopdf.md) | Pure-Rust PDF compression with lopdf | Accepted |
| [ADR-0003](ADR-0003-compression-presets.md) | Compression presets: lossless by default, never larger, originals untouched | Accepted |
| [ADR-0004](ADR-0004-untrusted-file-isolation.md) | Rust owns all file access; the webview sees ids only | Accepted |
| [ADR-0005](ADR-0005-unsigned-ci-releases.md) | Installers built by GitHub Actions, unsigned until certificates exist | Accepted |
| [ADR-0006](ADR-0006-safe-result-publication.md) | Protect session inputs and publish complete results without batch overwrites | Accepted |
| [ADR-0007](ADR-0007-original-file-identity.md) | Check filesystem identity when protecting originals on Unix | Accepted |
