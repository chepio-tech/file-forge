# Security

## Trust boundaries
1. **User files → core engines.** Every input file is untrusted (malformed, hostile, or huge). Engines return
   `Result`, bound allocations, and run in `spawn_blocking`; release builds unwind on panic so one bad file cannot
   crash the app (ADR-0004).
2. **Webview → shell.** The webview is treated as less trusted than Rust. It holds no filesystem or dialog
   permissions and passes only ids (ADR-0004).
3. **App → network.** Only the update check and download (ADR-0013): HTTPS GETs from Rust to the
   `chepio-tech/file-forge` GitHub releases, sending no files, names or identifiers. Opening `https://chepio.tech`
   in the system browser is allow-listed in `src-tauri/capabilities/default.json`. Files are never uploaded.
4. **Release → installed app.** An update is installed only if its minisign signature verifies with
   `plugins.updater.pubkey` and is bound to the version the manifest announces (`requireSignedVersion`). The webview
   cannot choose what is installed: it only asks Rust to check or to install the update Rust found.

## Webview hardening
- CSP in `src-tauri/tauri.conf.json`: `default-src 'self'`, IPC only, images from self/data. No remote scripts.
- Context menu disabled in production builds (`src/main.tsx`).
- React escapes all file names; never render file-derived strings as HTML.

## Dropped folders (ADR-0017)
Folder layouts are untrusted too. The search never follows links to folders, searches a folder reached twice once
(by canonical path), skips names starting with `.` and does not enter macOS packages. Depth, entries examined and
files added are capped per drop (`docs/architecture/runtime.md`). Found paths stay in Rust; the webview receives the
registered files and counts only.

## Data handling
- Registry entries and temp results live for the session only and are not persisted.
- Originals are opened read-only; input paths stay protected for the whole session, even when removed from the
  list. Save destinations are resolved on the Rust side and refused if they refer to any protected input. On Unix,
  device/inode identity also protects hard links and macOS filename case variants that canonical paths can miss.

## Untrusted PDFs
Limits on file size, decoded stream size and image pixels (`docs/architecture/runtime.md`); encrypted and signed
files are refused rather than rewritten (`docs/domain/invariants.md`); the engine is exercised with corrupted inputs
in `crates/fileforge-core/tests/pdf_compress.rs`.

Image decoders read attacker-controlled bytes, so they are memory-safe Rust (ADR-0015). `jpeg-decoder` runs with
`platform_independent` (no unsafe code). `hayro-jpeg2000` forbids unsafe code itself; its SIMD comes from
`fearless_simd`. It comes from crates.io (at least 0.4.1, ADR-0015); Dependabot updates must keep the `pdf::jpx`
OpenJPEG comparison test passing. JPEG 2000 images are checked against the dictionary and the sample limit before
decoding, and a panic inside that decoder only skips the image. Broken codestreams are fuzzed in the `pdf::jpx` unit
tests.

Flate photograph conversion (ADR-0016) validates predictor layout before invoking the bounded stream decoder;
unsupported/ambiguous parameters are skipped. Its screen-content scan allocates only a fixed histogram, inspects
already bounded pixels and introduces no model, downloads or additional threads.

CFF font programs are parsed by the engine's own code (`pdf::cff`, ADR-0018): every read is bounds-checked, the
charstring scan is capped at 64 M interpreted bytes and 10 nested calls, and the call is wrapped in `catch_unwind`.
A rewritten font is used only after parsing it again proves that every glyph executes the same bytes.
Corrupted fonts are fuzzed in the `pdf::cff` unit tests.

## Untrusted images (ADR-0019, ADR-0020)
JPEG, PNG and WebP files are parsed only by safe Rust: `jpeg-decoder`, the `png` crate, `image-webp`
(`forbid(unsafe_code)`) and the engine's own JPEG marker and scan reader (`raster::jpeg`) and RIFF chunk reader
(`raster::webp`), all bounds-checked; scan decoding is strict and fails rather than guessing. The format comes from
the content, never the extension. Pixel counts are checked from the header before decoding.
Two C libraries are linked, both on the encoding side only: libdeflate (through oxipng) compresses rows and an ICC
profile the `png` crate already decoded, and libwebp encodes pixels `image-webp` decoded. Neither ever reads the
user's file; libwebp's decoder is linked but only tests call it. Every candidate is decoded again before use.
Signed files (C2PA in JPEG, PNG and WebP; PNG `dSIG`) are returned unchanged. Corrupted JPEGs, PNGs and WebPs are
fuzzed by byte flips in `tests/raster_compress.rs` and `tests/raster_webp.rs`.

`unsafe` code is forbidden in every crate except `crates/fileforge-webp` (ADR-0020), which calls libwebp's encoder.
It checks sizes and buffer lengths before any C call, frees libwebp's memory through guards on every path, never
lets a panic unwind into C, and documents each `unsafe` block (`clippy::undocumented_unsafe_blocks` is denied).
Its tests round-trip pixels at edge sizes and cross-check libwebp's decoder against `image-webp`.

## Temp results
Stored in the app cache directory (`…/tech.chepio.fileforge/results`), deleted when a file is removed, on exit and at
the next start. Staging files are created exclusively, flushed and closed before publication, and removed on failure.
Individual saves use an atomic rename after checking the destination. Batch saves publish with a hard link to
the complete staging file, retrying numbered names on collisions; this never overwrites a concurrent output.
