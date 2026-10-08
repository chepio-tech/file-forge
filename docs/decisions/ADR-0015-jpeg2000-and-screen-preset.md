# ADR-0015: JPEG 2000 images become JPEG, a Screen preset, and reference-accurate image decoders

## Status
Accepted; decoder moved from the git pin to crates.io 0.4.1 on 2026-10-08 (see Follow-up)

## Date
2026-10-03

## Context
A scanned 192-page textbook (55.3 MB) shrank by only 7.6% with Maximum, while an online compressor reached 41.9 MB.
The file holds 1,101 JPEG 2000 (`JPXDecode`) images: 49.1 MB, 89% of the file, already stored at 150 DPI and about
1 bit per pixel. The engine skipped JPEG 2000 entirely. Measurements on that file (Pillow/OpenJPEG, then the engine):

| Images | File |
|---|---|
| Kept as JPEG 2000 (before) | 51.1 MB |
| JPEG quality 70, 150 DPI (Maximum) | 46.2 MB |
| JPEG quality 70, 100 DPI | 27.1 MB |
| JPEG quality 65, 100 DPI | 24.8 MB |

Because these images are already at 150 DPI, decoding them alone barely helps. Large savings need a lower
resolution than any preset offered.

Re-encoding to WebP or AVIF is not an option. PDF readers only decode JPEG, JPEG 2000, JBIG2, CCITT and Flate image
streams, so such images would show as blank areas. The PDF Association plans to add JPEG XL, but readers do not
support it yet. Rasterizing whole pages loses text selection, search and vector sharpness.

Two decoder defects turned up:
- hayro-jpeg2000 0.4.0 (crates.io) loses detail in irreversible (9/7) images. Its mean difference from OpenJPEG
  and Apple ImageIO was 1.6–6.4 levels per sample, up to 65. Upstream commit #1340 fixes it (difference ≤ 0.002),
  but no release includes the fix yet.
- zune-jpeg 0.5.15, the `image` crate's JPEG backend, misdecodes the JPEGs jpeg-encoder writes with optimized
  Huffman tables, while libjpeg, Apple ImageIO and jpeg-decoder read them correctly. Compressing a File Forge result
  a second time therefore saved garbage pixels (mean error 54–65 per sample). Released v0.1.0 is affected.

## Decision
- **JPEG 2000 → JPEG** in every lossy preset. An image qualifies when:
  - its dictionary declares gray or RGB, 8-bit or no `BitsPerComponent`, and no `Decode` or nonzero `SMaskInData`;
  - its codestream decodes to exactly that size and channel count, at 8 bits and without alpha, in a gray, RGB or
    ICC color space.

  It is downsampled like other images and replaced only if the JPEG is at least 2% smaller. Everything else stays
  byte-identical.
- **Decoder:** `hayro-jpeg2000`, pure Rust with `forbid(unsafe_code)` (its SIMD comes from `fearless_simd`), pinned
  to git commit `ced00dd` until a crates.io release contains #1340. Images above 100 M samples are skipped; the
  decoder peaks at about 8 bytes per sample. A panic inside it skips that image (`catch_unwind`) instead of failing
  the file.
- **JPEG decoding** uses `jpeg-decoder` with `platform_independent` (no unsafe code). The `image` crate stays for
  resizing only, with no codec features.
- **Screen preset:** JPEG quality 65, 100 DPI, labeled for on-screen reading. It shows the same exact numbers as the
  other presets (ADR-0003). Balanced and Maximum keep their values.

## Rationale
JPEG output opens in every reader, and the never-larger rule decides per image whether conversion pays off. A
fourth preset gives the large savings that scans need without changing what existing Maximum users get. A
memory-safe decoder matters more than speed for untrusted files. Decoding the way the reference decoders do keeps
repeated compression safe.

## Alternatives considered
- **OpenJPEG through `jpeg2k`:** mature and exact, but it parses untrusted files in C and needs a C toolchain on
  all three CI platforms.
- **Waiting for a hayro-jpeg2000 release:** leaves JPEG 2000 files unsupported for an unknown time. Shipping
  0.4.0 instead would blur every irreversible image.
- **Re-encoding as JPEG 2000:** about 21 MB at 0.4 bits per pixel on the textbook, but no maintained Rust encoder
  exists.
- **Lowering Maximum to 120 DPI or quality 60:** silently changes results for current users and printing.
- **Turning off optimized Huffman tables:** avoids the zune-jpeg defect for new output, but v0.1.0 results would
  still be misread.

## Consequences
- The textbook drops to 46.2 MB with Maximum and 24.8 MB with Screen (−55%), against 41.9 MB online.
- Balanced and Maximum spend time decoding JPEG 2000 images even when the JPEG turns out larger: 4.2 s instead of
  0.5 s on the textbook. JPEG decoding is 7–15% slower on a 36 MP photo.
- The git dependency is not updated by Dependabot. Check upstream releases by hand.
- Minimum Rust version 1.92 (hayro-jpeg2000).
- Peak memory can reach about 4 × 0.8 GB while four maximum-size JPEG 2000 images decode in parallel.

## Validation / fitness criteria
- `pdf::jpx` unit tests: an irreversible 16×12 codestream decodes within 1 level of OpenJPEG (0.4.0 was off by
  62); mismatched layouts and broken codestreams are refused without panicking.
- `tests/pdf_compress.rs`: JPEG 2000 becomes JPEG with exactly the source pixels' encoding, downsamples to the
  target DPI, and unusual variants stay byte-identical; compressing the engine's own output again keeps the pixels.
- `PdfCompress.test.tsx`: Screen sends quality 65 and 100 DPI and explains its trade-off.

## Reconsider when
- A crates.io release of hayro-jpeg2000 includes #1340: switch back from the git pin.
- zune-jpeg reads jpeg-encoder output correctly: compare speed against jpeg-decoder.
- Readers support JPEG XL in PDF, or a Rust JPEG 2000 encoder matures.
- Users rarely pick Screen, or ask for a print-oriented middle ground.

## Follow-up (2026-10-08)
hayro-jpeg2000 0.4.1 (crates.io, 2026-10-04) contains #1340: its tag is `ced00dd` plus a version bump, and the
published `src/` is byte-identical to the pinned commit. The dependency now requires `0.4.1` from crates.io, so
Dependabot proposes its updates; `decodes_irreversible_images_like_openjpeg` guards against a regression.

## References
- ISO 32000-2, 7.4.9 JPXDecode filter, 8.9.5 Image dictionaries (`SMaskInData`, `Decode`)
- hayro-jpeg2000 fix: https://github.com/LaurenzV/hayro (commit `49037586`, #1340)
- PDF Association on JPEG XL: https://www.theregister.com/2025/11/10/another_chance_for_jpeg_xl/
