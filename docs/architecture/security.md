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
`fearless_simd`. It is pinned to a reviewed git commit, so Dependabot does not update it. JPEG 2000 images are
checked against the dictionary and the sample limit before decoding, and a panic inside that decoder only skips
the image. Broken codestreams are fuzzed in the `pdf::jpx` unit tests.

Flate photograph conversion (ADR-0016) validates predictor layout before invoking the bounded stream decoder;
unsupported/ambiguous parameters are skipped. Its screen-content scan allocates only a fixed histogram, inspects
already bounded pixels and introduces no model, downloads or additional threads.

CFF font programs are parsed by the engine's own code (`pdf::cff`, ADR-0018): every read is bounds-checked, the
charstring scan is capped at 64 M interpreted bytes and 10 nested calls, and the call is wrapped in `catch_unwind`.
A rewritten font is used only after parsing it again proves that every glyph executes the same bytes.
Corrupted fonts are fuzzed in the `pdf::cff` unit tests.

## Temp results
Stored in the app cache directory (`…/tech.chepio.fileforge/results`), deleted when a file is removed, on exit and at
the next start. Staging files are created exclusively, flushed and closed before publication, and removed on failure.
Individual saves use an atomic rename after checking the destination. Batch saves publish with a hard link to
the complete staging file, retrying numbered names on collisions; this never overwrites a concurrent output.
