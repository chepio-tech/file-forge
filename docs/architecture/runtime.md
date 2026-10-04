# Runtime

## Critical path: compress one PDF
1. UI calls `compress_pdf(id, options)`; options are validated at the IPC boundary.
2. Shell resolves the id to a path (`FileRegistry`), takes the single **work slot** (one compression at a time),
   checks that the opened file is regular, checks its size, and reads at most the input limit plus one byte.
   Files that grow beyond the limit during the read are refused too.
3. `fileforge_core::pdf::compress_controlled` (in `spawn_blocking`): load → refuse encrypted/signed → (on request)
   remove metadata, thumbnails and editing data → merge duplicate streams → drop unreachable objects → empty CFF
   subroutines no glyph calls (every preset, ADR-0018) → image pass (lossy presets, ≤ 4 images in parallel; JPEG
   and JPEG 2000 → JPEG, raw → downsampled Deflate; suitable Flate photographs → JPEG with `compressFlatePhotos`,
   Maximum, ADR-0016) → re-deflate →
   save (object + cross-reference streams, or classic for PDF/A-1) → reload and check page count → keep only if
   smaller. Each stage reports progress to the UI through the call's channel.
4. Shell writes the result atomically to the temp store and returns the `PdfReport`.

Flate screen-content checks run before resizing, and the JPEG must beat equivalent Deflate (ADR-0016).

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

## Updates (ADR-0013)
The UI calls `check_for_update` once at startup and when the user asks; Rust keeps the found update. On "Restart to
update", `install_update` refuses early if the work slot is taken or results are unsaved, downloads and verifies
the artifact without holding the slot, then takes the slot, checks again, installs and restarts. The slot stays held
until the process exits, so no compression or save starts in between. Restart goes through `RunEvent::Exit`, which
clears temp results; on Windows the plugin exits from the installer hook, which clears them first.

## Limits (bounded resources)
| Resource | Limit | Where |
|---|---|---|
| Input file | 1 GiB (`MAX_INPUT_BYTES`) | `crates/fileforge-core/src/pdf/limits.rs` |
| Decoded size of any stream read | 512 MiB | same |
| Pixels per image touched | 150 MP | same |
| Samples per JPEG 2000 image decoded (pixels × channels) | 100 M (≈ 0.8 GB while decoding) | same |
| Images decoded concurrently | 4 | `crates/fileforge-core/src/pdf/images.rs` |
| Charstring bytes interpreted per CFF font (scan budget) | 64 M; beyond it the font stays as it is | `crates/fileforge-core/src/pdf/cff.rs` |
| Concurrent compressions | 1 | `ResultStore::work_slot` |
| Folder levels searched below a dropped folder | 16 | `src-tauri/src/folder_scan.rs` (`ScanLimits`) |
| Directory entries examined per drop | 10,000 | same |
| Files added from dropped folders per drop | 1,000 | same |

Peak memory ≈ input + parsed document + up to 4 decoded bitmaps (JPEG 2000 decoding: up to ~8 bytes per sample).

## Failure modes
| Failure | Behavior |
|---|---|
| Malformed / truncated file | `pdfMalformed` for that file; others continue |
| Dropped folder cannot be read | Named in the intake notice; the rest of the drop is added |
| Dropped folders exceed a search limit | Files found so far are added; the notice says some were not (ADR-0017) |
| Encrypted or signed | `pdfEncrypted` / `pdfSigned`, file untouched |
| One image fails to decode or re-encode | Image left as is, document still compressed |
| The JPEG 2000 decoder panics on one image | Caught for that image (ADR-0015); image left as is, document still compressed |
| Output fails to reload or loses pages | `internal`, nothing stored |
| User cancels | `cancelled` for the current file, nothing stored, later files not started, earlier results kept |
| Panic inside a dependency | Caught by `spawn_blocking` (release builds unwind), `internal` for that file |
| Disk full while saving | `io`; the partial file is removed, the temp result stays for a retry |
| Save dialog chooses an original | `originalTarget`; choose a different output name, input stays untouched |
| Batch output name becomes taken | Retry with the next numbered name; no existing file is replaced |
| Destination does not support hard links | Batch save returns `io`; use individual saves or a supported filesystem |
| File manager cannot reveal a saved result | UI notice; the saved result remains available |
| App quits | Temp results are deleted (`RunEvent::Exit`); also cleared at the next start |
| Offline or GitHub unreachable at startup | Update check times out after 15 s and the UI stays quiet; a manual check says so |
| Update download stalls | `update` after 10 min; nothing installed, results kept |
| Update signature or signed version invalid | `update`; nothing installed (ADR-0013) |
| Restart to update while work runs | `busy`; the update stays available |
| Restart to update with unsaved results | `unsavedResults`; the UI asks before installing with `discardUnsaved` |

No network, no retries, no timeouts needed: everything is local and user-initiated.

## Measured (Apple M-series, release build, 2026-10-03)
| File | Lossless | Balanced | Maximum | Screen |
|---|---|---|---|---|
| 4-page scan, 4 × 36 MP JPEG at 300 DPI, 18.7 MB | 0% | −84.6% (2.3 s) | −95.5% (1.0 s) | not measured |
| Text, 231 pages, 1.28 MB | −33% (0.24 s) | same | same | same |
| Text licences, 0.55–0.77 MB | −10…27% | same | same | same |
| Vector artwork, 0.8 MB | −4.9% | same | same | same |
| Scanned textbook, 192 pages, 1,101 JPEG 2000 strips at 150 DPI, 55.3 MB (2026-10-03, ADR-0015) | −0.1% | −1.7% (4.2 s) | −16.4% (4.3 s) | −55.2% (5.7 s) |

JPEG decoding moved to `jpeg-decoder` (ADR-0015). On a generated 4-page PDF repeating one 36 MP JPEG it takes 1.05 s
(Balanced) and 0.76 s (Maximum), against 0.98 s and 0.66 s before.

CFF subroutine pruning (ADR-0018, 2026-10-04, measured on `main` `2848f15` with and without it): on 277 distinct
non-private PDFs from macOS and app bundles (54.6 MB), Lossless saves 18.7% instead of 15.3% (Balanced 19.2/15.8%,
Maximum 19.4/15.9%, Screen 19.5/16.0%); no file grew and the run time stayed 4.8 s. macOS icon PDFs with SF Pro
subsets shrink by 93–95% (344 → 17 KB); a one-page Quartz PDF (macOS 26) with STIX and Noto text 61 → 27 KB.
Rendered with Quartz at 2×, all 1,372 pages match the originals (Lossless) and the Maximum results without pruning.

Reproduce: `cargo run --release -p fileforge-core --example measure_pdf -- <files>`.

Generated Flate photo/screen fixtures, release build, 2026-10-04 (ADR-0016; not a real-world corpus):
| Input | Maximum before | Maximum with photo conversion | Time after |
|---|---|---|---|
| RGB photograph, 1024×768 at 150 DPI, 2,098.0 KB | 2,094.3 KB (−0.2%) | 60.3 KB (−97.1%), same resolution | 62 ms |
| Gray photograph, 1024×768 at 150 DPI, 654.5 KB | 654.4 KB (−0.0%) | 78.4 KB (−88.0%), same resolution | 25 ms |
| RGB photograph, 1600×1200 at 300 DPI, 5,119.8 KB | 1,241.2 KB (−75.8%) | 23.5 KB (−99.5%), 800×600 pixels | 88 ms |
| Photo with flat/sharp UI panels, 1024×768 at 150 DPI, 1,662.3 KB | 1,661.3 KB | 1,661.3 KB, identical pixels | 190 ms |

Fixtures combine deterministic noise and smooth sinusoidal gradients; the screen fixture adds flat panels and
sharp strokes. These especially compressible generated images illustrate the encoding gap and conservative
veto, rather than estimate savings on ordinary photographs. No real photo/screenshot corpus was measured.

Cancel latency (generated files, release build, cancel requested at 10–90% of the run, 2026-10-03):
| File | Full run | Cancel took effect after |
|---|---|---|
| 24 × 12 MP JPEG photos, 160 MB, Balanced | 1.55 s (image pass 1.50 s) | 0.07–0.22 s |
| 20,000 text pages, 86 MB, Lossless | 1.13 s (parse 0.04 s, streams 0.88 s, save 0.09 s) | 0.01–0.27 s |
