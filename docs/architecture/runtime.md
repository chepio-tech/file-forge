# Runtime

## Critical path: compress one PDF
1. UI calls `compress_pdf(id, options)`; options are validated at the IPC boundary.
2. Shell resolves the id to a path (`FileRegistry`), takes the single **work slot** (one compression at a time),
   checks the file size, reads it into memory.
3. `fileforge_core::pdf::compress` (in `spawn_blocking`): load → refuse encrypted/signed → merge duplicate streams →
   drop unreachable objects → image pass (lossy presets, ≤ 4 images in parallel) → re-deflate → save (object +
   cross-reference streams, or classic for PDF/A-1) → reload and check page count → keep only if smaller.
4. Shell writes the result atomically to the temp store and returns the `PdfReport`.

The UI runs files strictly one after another; the work slot enforces the same in Rust.

## Limits (bounded resources)
| Resource | Limit | Where |
|---|---|---|
| Input file | 1 GiB (`MAX_INPUT_BYTES`) | `crates/fileforge-core/src/pdf/limits.rs` |
| Decoded size of any stream read | 512 MiB | same |
| Pixels per image touched | 150 MP | same |
| Images decoded concurrently | 4 | `crates/fileforge-core/src/pdf/images.rs` |
| Concurrent compressions | 1 | `ResultStore::work_slot` |

Peak memory ≈ input + parsed document + up to 4 decoded bitmaps.

## Failure modes
| Failure | Behavior |
|---|---|
| Malformed / truncated file | `pdfMalformed` for that file; others continue |
| Encrypted or signed | `pdfEncrypted` / `pdfSigned`, file untouched |
| One image fails to decode or re-encode | Image left as is, document still compressed |
| Output fails to reload or loses pages | `internal`, nothing stored |
| Panic inside a dependency | Caught by `spawn_blocking` (release builds unwind), `internal` for that file |
| Disk full while saving | `io`; the partial file is removed, the temp result stays for a retry |
| App quits | Temp results are deleted (`RunEvent::Exit`); also cleared at the next start |

No network, no retries, no timeouts needed: everything is local and user-initiated. Cancellation of a running
compression is not supported yet (ROADMAP).

## Measured (Apple M-series, release build, 2026-10-03)
| File | Lossless | Balanced | Maximum |
|---|---|---|---|
| 4-page scan, 4 × 36 MP JPEG at 300 DPI, 18.7 MB | 0% | −84.6% (2.3 s) | −95.5% (1.0 s) |
| Text, 231 pages, 1.28 MB | −33% (0.24 s) | same | same |
| Text licences, 0.55–0.77 MB | −10…27% | same | same |
| Vector artwork, 0.8 MB | −4.9% | same | same |

Reproduce: `cargo run --release -p fileforge-core --example measure_pdf -- <files>`.
