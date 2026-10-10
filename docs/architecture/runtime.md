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

## Critical path: compress one image (ADR-0019, ADR-0020)
1. UI calls `compress_image(id, options)`; the shell validates options, takes the work slot and reads the file as for
   PDFs, with the image input limit.
2. `fileforge_core::raster::compress_controlled`: detect JPEG, PNG or WebP by content → check the pixel count from
   the header → keep files with data after the image, Content Credentials, PNG signatures or animation → decode
   (`jpeg-decoder`, `png`, `image-webp`) → build candidates → decode each candidate again → keep the smallest valid
   one if it is smaller.
   - JPEG: lossy presets re-encode with `mozjpeg-rs` first and free the pixels; then the lossless transcoder
     (sequential Huffman 8-bit) or, with metadata removal only, a segment rewrite. A lossless candidate must decode to
     the same pixel hash; a lossy one must be at least 2% smaller than the best lossless result.
   - PNG: oxipng optimizes the decoded rows (`RawImage`); the result must decode to the same 16-bit RGBA samples.
   - WebP: lossless files are re-encoded by libwebp (`fileforge-webp`) at level 9 up to 2048×2048 pixels, level 7
     above, with exact transparent colors, and must decode to the same RGBA samples. Lossy files keep their image
     chunks unless a WebP quality is set; then libwebp re-encodes them (method 6, or 5 with alpha) and the result must
     keep the size and alpha values and be at least 2% smaller. The container keeps the original chunk order.
3. Stages reported: `loading`, `encoding`, `verifying`. The shell stores the result with the format's extension.

## Cancellation (ADR-0011)
`compress_pdf` takes a ticket from the shell's cancellation counter when it starts, before waiting for the work
slot; `cancel_compression` advances the counter. The shell checks the ticket after taking the slot (before reading
the file), and the engine checks it before and after loading, after the structure pass, before each image and
stream, after the image and stream passes, and before verification. Parsing and saving are single lopdf calls and
cannot be interrupted, so a cancel takes effect at the next checkpoint. Images check between stages, every 4,096 MCUs
of the JPEG transcoder and inside `mozjpeg-rs` and libwebp (its progress callback); oxipng cannot be interrupted, but
its filter trials stop after 60 s
and Zopfli runs only on small images. The UI stops starting files as soon as
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
| Image input file | 256 MiB (`raster::MAX_INPUT_BYTES`) | `crates/fileforge-core/src/raster/limits.rs` |
| Pixels per image file | 120 MP (covers 100 MP cameras; a 4:4:4 JPEG at the limit needs ≈ 0.7 GB of coefficients) | same |
| Decoded PNG rows (all channels and bit depths) | 512 MiB | same |
| PNG rows Zopfli runs on (Maximum) | 512 KiB (≈ 2.5 s for a 256×256 RGBA icon) | same |
| oxipng filter trials per image | 60 s, then the best result so far | same |
| Lossless WebP pixels encoded at libwebp's slowest level 9 | 2048 × 2048 (≈ 12 s measured at 3.7 MP); larger images use level 7 | same |
| Concurrent compressions | 1 | `ResultStore::work_slot` |
| Folder levels searched below a dropped folder | 16 | `src-tauri/src/folder_scan.rs` (`ScanLimits`) |
| Directory entries examined per drop | 10,000 | same |
| Files added from dropped folders per drop | 1,000 | same |

Peak memory ≈ input + parsed document + up to 4 decoded bitmaps (JPEG 2000 decoding: up to ~8 bytes per sample).
Images: input + decoded pixels (or JPEG coefficients) + one candidate; pixels are freed before the transcoder runs.
WebP: input + decoded pixels + libwebp's working copy (ARGB, about the same again) + one candidate.

## Failure modes
| Failure | Behavior |
|---|---|
| Malformed / truncated file | `pdfMalformed` for that file; others continue |
| Dropped folder cannot be read | Named in the intake notice; the rest of the drop is added |
| Dropped folders exceed a search limit | Files found so far are added; the notice says some were not (ADR-0017) |
| Encrypted or signed | `pdfEncrypted` / `pdfSigned`, file untouched |
| Not a JPEG, PNG or WebP by content (e.g. HEIC or GIF renamed) | `imageUnsupported` for that file |
| Damaged image | `imageMalformed`; a JPEG the strict transcoder cannot read but `jpeg-decoder` can is kept (`unsupportedEncoding`) |
| Image above the byte or pixel limit | `imageTooLarge` |
| Signed (C2PA, `dSIG`), animated PNG or WebP, or data after the image | Original returned byte for byte with the reason (`kept`) |
| Lossy WebP without a WebP quality | Original returned byte for byte (`lossyEncoding`) unless metadata is removed |
| WebP whose VP8X header denies the alpha its image data has | Original returned byte for byte (`unsupportedEncoding`): re-encoding could drop the transparency |
| An image candidate decodes differently | Candidate dropped, original kept; never surfaced as an error |
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

## Measured: images (Apple M-series, release build, 2026-10-08, ADR-0019)
Non-private macOS system and app images, read-only (`measure_raster`):
| File | Lossless | Balanced | Maximum | Metadata removed (lossless) |
|---|---|---|---|---|
| Wallpaper JPEG 3840×2160, 5,617.7 KB | −4.9% (0.52 s) | −81.5% (1.2 s) | −87.2% (1.2 s) | −5.0% |
| Progressive JPEG 400×800, 72.9 KB | kept (encoding) | kept (not 2% smaller) | −10.6% | −0.3% |
| Baseline JPEGs 256–800 px, 67–100 KB | −2.2…6.1% | −40.2…78.7% | −54.2…84.1% | −2.4…6.2% |
| Screenshots PNG 1234×834–1602×844, 351–375 KB | −44.5…50.1% (0.5–1.0 s) | −45.4…52.8% (1.0–1.7 s) | −46.6…53.3% (1.5–2.2 s) | −44.7…50.1% |
| App icons PNG 256×256–2048×2048, 51–2,383 KB | −5.4…34.0% (0.05–2.9 s) | −5.9…34.9% | −6.6…34.9% (up to 7.9 s) | −6.5…34.0% |
| PNG named `.jpg`, 214×130, 70.4 KB | −0.8% | −1.2% | −2.0% | −2.9% |

With Zopfli on every image, Maximum took 40–48 s per 1–2-megapixel PNG for 1–2.6% more than level 6 alone; it now
runs only on rows up to 512 KiB (256×256 icons: 2.4–2.8 s, 0.6–0.9% smaller than level 6). Level 6 is not always
smaller than level 4 (a 2048×2048 icon: +0.3 KB); both stay below the input. The engine detects formats by content:
one `.jpg` in the set was a PNG and was optimized as one.

Reproduce: `cargo run --release -p fileforge-core --example measure_raster -- <files>`.

## Measured: WebP (Apple M-series, release build, 2026-10-08, ADR-0020)
Read-only `measure_raster` runs on non-private files: WebPs shipped in app bundles (mostly Android Studio device
art), and macOS wallpapers, an iOS Simulator sample photo and system UI images converted with cwebp 1.6.0.
| Input | Lossless | Balanced (85) | Maximum (75) |
|---|---|---|---|
| 95 distinct lossless app WebPs, 11.6 MB, up to 7.3 MP | −6.7% in total (per file 0…70.5%, median 5.9%; 15 kept) | same | same |
| 59 distinct lossy app WebPs, 42 of them with alpha, 3.6 MB | kept (`lossyEncoding`) | −14.1% (21 kept: under 2%) | −30.5% (median 27%) |
| cwebp q90 photos 3840×2160–4288×2848, 0.85–1.48 MB | kept | −23.2…53.3% (1.1–1.7 s) | −53.4…81.2% (0.9–1.2 s) |
| cwebp q75 photos (cwebp's default), 0.33–0.55 MB | kept | kept (under 2%) | −7.7…18.4% (0.8–1.0 s) |
| cwebp lossless UI image 2560×1440, 561 KB | −24.3% (5.9 s) | same | same |
| cwebp lossless icons 1024×1024 with alpha, 288–324 KB | −1.8…2.6% (1.5–1.7 s) | same | same |
| cwebp lossless photo 3840×2160, 7.2 MB (level 7) | −0.1% (5.4 s) | same | same |
| cwebp q75 icons and UI with alpha, 12–25 KB | kept | 0…2.2% | 0…6.3% |

Times are from a sequential run; the app-bundle set ran six processes in parallel, so its times are not reported.
Encoder settings were chosen with cwebp 1.6.0 on these files: lossless level 9 took 12 s for −24% on the UI image
(levels 6–8: at most −2.8%) and 43 s on the lossless photo for a larger file than level 7. Lossy method 6 was 2–16%
smaller than method 4 on photos for 1.3–1.6× the time; with exact alpha it took 10–30× longer (5.2 s on a 3.5 MP
frame) for 4–7%, so images with alpha use method 5. Maximum re-encodes files saved at cwebp's default quality 75 again
at 75: 8–18% smaller, at the cost of a second generation of loss; Balanced leaves them.

## Background removal (ADR-0022)
The shell takes the existing work slot, reads at most 256 MB plus one byte, decodes at most 32 MP with orientation,
and hashes the source. SAM 2.1 runs on a dedicated pool of at most four threads. The last source's embedding is
retained by content hash, so selecting another subject runs only the decoder. Automatic selection uses the center
point and largest multimask; clicks use predicted IoU. Logits are sampled bilinearly without clamping, and a
bounded-grid color-guided regression produces antialiased alpha, with soft-edge foreground color recovery. Existing
alpha is multiplied, hidden RGB zeroed, then crop and solid composition are applied. Lossless PNG/WebP output is decoded again before atomic temp storage.

Cancellation is checked before/after inference, for each render row and during WebP encoding. Individual tract
inference and PNG codec calls finish before the next checkpoint. Failed downloads clean their partial file;
a successfully downloaded first graph can be reused after a second-graph failure. Download cancellation is checked
between chunks; a stalled request reaches its 15-second connect/read timeout before returning. Hiding the tool unloads the
model and last embedding after current work finishes; removing the model preserves completed temp results.

On the owner's M1 Pro, debug engine with optimized dependencies, release model files, two public test fixtures:
graph load 0.56 s, encoder 2.43–2.50 s, decoder 0.09–0.16 s. These are fixture measurements, not a broad quality or
performance benchmark. The model download is 82,537,778 bytes; graph files are not bundled in installers.

Real-model plus refinement peak RSS measured on the two small fixtures: 1,932,902,400 bytes (about 1.8 GiB).
Larger images add source/output buffers; the 32 MP limit is conservative and is not a measured worst-case guarantee.
Refinement coefficients use at most a 1024-pixel grid; cancellation is checked on every statistics/render row.
