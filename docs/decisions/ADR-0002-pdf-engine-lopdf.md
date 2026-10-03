# ADR-0002: Pure-Rust PDF compression with lopdf

## Status
Accepted

## Date
2026-10-03

## Context
PDF compression is the first feature. The app ships as a closed-source-compatible installer on three OSes.

## Decision
Use `lopdf` (MIT) inside `fileforge-core`: structural optimization (object streams, cross-reference streams,
re-deflating streams, removing unused objects) plus optional image recompression and downsampling with pure-Rust
codecs. No external binaries.

## Rationale
- No sidecar binaries to build, sign and ship per OS.
- Permissive license; the app's own license stays free to choose.
- Everything is testable with fixtures generated in code.

## Alternatives considered
- Ghostscript sidecar: best compression (font subsetting, full re-render), but AGPL-3.0: distributing it requires
  releasing the whole app under AGPL or buying a commercial license from Artifex; plus 50–80 MB of binaries per OS.
- qpdf sidecar (Apache-2.0): lossless only, typically 5–20% on already-compressed files, no image recompression,
  still a per-OS binary.

## Consequences
- Weaker than Ghostscript on font-heavy and vector-heavy PDFs: no font subsetting.
- The whole document is loaded into memory: an input size limit is required (see `docs/architecture/runtime.md`).
- We own the image pipeline and its correctness (color spaces, masks, filters).

## Validation / fitness criteria
- No external executables in `src-tauri/tauri.conf.json` (`bundle.externalBin` absent).
- Engine tests cover: output never larger, lossless preset keeps decoded content identical, page count preserved.

## Reconsider when
- Users regularly report weak results on text-heavy PDFs, or a commercial Ghostscript license becomes acceptable.

## References
- Ghostscript licensing: https://ghostscript.com/licensing
- lopdf: https://crates.io/crates/lopdf
