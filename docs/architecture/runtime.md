# Runtime

## Critical path: compress one PDF
1. UI calls `compress_pdf(id, options)`; options are validated at the IPC boundary.
2. Shell resolves the id to a path (`FileRegistry`), takes the single **work slot** (one compression at a time),
   checks that the opened file is regular, checks its size, and reads at most the input limit plus one byte.
   Files that grow beyond the limit during the read are refused too.
3. `fileforge_core::pdf::compress_controlled` (in `spawn_blocking`): load → refuse encrypted/signed → merge
   duplicate streams → drop unreachable objects → image pass (lossy presets, ≤ 4 images in parallel) → re-deflate →
   save (object + cross-reference streams, or classic for PDF/A-1) → reload and check page count → keep only if
   smaller. Each stage reports progress to the UI through the call's channel.
4. Shell writes the result atomically to the temp store and returns the `PdfReport`.

## Cancellation (ADR-0011)
`compress_pdf` takes a ticket from the shell's cancellation counter when it starts, before waiting for the work
slot; `cancel_compression` advances the counter. The shell checks the ticket after taking the slot (before reading
the file), and the engine checks it before and after loading, after the structure pass, before each image and
stream, after the image and stream passes, and before verification. Parsing and saving are single lopdf calls and
cannot be interrupted, so a cancel takes effect at the next checkpoint. The UI stops starting files as soon as
Cancel is pressed, even if the IPC call fails; a file that finishes before reaching a checkpoint keeps its result.

The UI runs files strictly one after another; the work slot enforces the same in Rust. Saving and removal also
take the work slot so a result cannot be replaced or deleted while it is being saved. The UI shows pending saves
and blocks compression, removal and settings changes until the save completes or the dialog is cancelled.

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
| User cancels | `cancelled` for the current file, nothing stored, later files not started, earlier results kept |
| Panic inside a dependency | Caught by `spawn_blocking` (release builds unwind), `internal` for that file |
| Disk full while saving | `io`; the partial file is removed, the temp result stays for a retry |
| Save dialog chooses an original | `originalTarget`; choose a different output name, input stays untouched |
| Batch output name becomes taken | Retry with the next numbered name; no existing file is replaced |
| Destination does not support hard links | Batch save returns `io`; use individual saves or a supported filesystem |
| File manager cannot reveal a saved result | UI notice; the saved result remains available |
| App quits | Temp results are deleted (`RunEvent::Exit`); also cleared at the next start |

No network, no retries, no timeouts needed: everything is local and user-initiated.

## Measured (Apple M-series, release build, 2026-10-03)
| File | Lossless | Balanced | Maximum |
|---|---|---|---|
| 4-page scan, 4 × 36 MP JPEG at 300 DPI, 18.7 MB | 0% | −84.6% (2.3 s) | −95.5% (1.0 s) |
| Text, 231 pages, 1.28 MB | −33% (0.24 s) | same | same |
| Text licences, 0.55–0.77 MB | −10…27% | same | same |
| Vector artwork, 0.8 MB | −4.9% | same | same |

Reproduce: `cargo run --release -p fileforge-core --example measure_pdf -- <files>`.

Cancel latency (generated files, release build, cancel requested at 10–90% of the run, 2026-10-03):
| File | Full run | Cancel took effect after |
|---|---|---|
| 24 × 12 MP JPEG photos, 160 MB, Balanced | 1.55 s (image pass 1.50 s) | 0.07–0.22 s |
| 20,000 text pages, 86 MB, Lossless | 1.13 s (parse 0.04 s, streams 0.88 s, save 0.09 s) | 0.01–0.27 s |
