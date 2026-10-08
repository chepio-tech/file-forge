# ADR-0019: Image compression with an own lossless JPEG transcoder, mozjpeg-rs and oxipng

## Status
Accepted

## Date
2026-10-08

## Context
Phase 2 adds a "Compress images" tool. It needs the same guarantees as the PDF tool (ADR-0003): lossless by default,
originals untouched, never larger, exact numbers. Image files are untrusted input, and ADR-0015 made memory-safe
decoding of untrusted bytes a rule.

The candidates on crates.io (checked 2026-10-08):
- Lossless JPEG optimization (jpegtran: rebuild Huffman tables without touching coefficients) exists in Rust only in
  `libjpeg-turbo-rs` 0.8: one maintainer since 2026-03, `unsafe` code in its Huffman and progressive decoders, about
  780 unchecked SIMD operations.
- `mozjpeg-rs` 0.9 (BSD-3) is a safe Rust port of the mozjpeg encoder (`forbid(unsafe_code)`, trellis quantization,
  byte-identical to C mozjpeg). It encodes pixels only.
- `oxipng` 10.2 (MIT) is the reference PNG optimizer. It requires libdeflate (C). `optimize_from_memory` inflates the
  untrusted file with libdeflate; `RawImage` takes decoded rows and only compresses them.
- `zenjpeg`, `zenwebp` and `jpegli-rs` are AGPL-3.0 or commercial; `imagequant` (lossy PNG) is GPL-3.0. The app has
  no license yet, so these would decide it.

## Decision
- **Engine:** `crates/fileforge-core/src/raster/` for JPEG and PNG, detected by content. Images with more than 120 M
  pixels or files above 256 MB are refused.
- **Lossless JPEG:** an own safe transcoder for baseline and extended sequential 8-bit Huffman JPEGs. It decodes the
  scans to quantized coefficients and encodes them again with optimal Huffman tables (Annex K.2), keeping every
  other segment and the scan structure. Decoding is strict: anything it would have to guess about keeps the original.
  Progressive, 12-bit and arithmetic-coded JPEGs keep their scans.
- **Lossy JPEG:** `mozjpeg-rs` (progressive, trellis) re-encodes pixels decoded by `jpeg-decoder`, keeping the
  source's chroma subsampling and metadata segments. It replaces the best lossless result only when at least 2%
  smaller, as for PDF images (ADR-0015). CMYK and 16-bit JPEGs are not re-encoded.
- **PNG:** the `png` crate decodes the file, including the ICC profile; `oxipng` gets the decoded rows through
  `RawImage`. libdeflate therefore only compresses data the engine produced. Optimizations are lossless only;
  transparent pixels keep their colors.
- **Verification:** every candidate is decoded again. A lossless JPEG must decode to exactly the original pixels;
  a PNG must decode to the same 16-bit RGBA samples. Failing candidates are dropped and the original is kept.
- **Kept byte for byte:** data after the image (MPF, HDR gain maps), Content Credentials (C2PA) and PNG digital
  signatures (`dSIG`), animated PNG. The report names the reason.
- **Metadata:** kept by default. "Remove metadata" removes EXIF except orientation, XMP, IPTC, comments, PNG text
  and time chunks and unknown chunks; ICC profiles, JFIF density, the Adobe segment and PNG color chunks stay.
- **Presets:** Lossless (JPEG lossless, PNG level 2), Balanced (JPEG 85, PNG level 4), Maximum (JPEG 75, PNG
  level 6 plus Zopfli for rows up to 512 KiB), Custom. Lossy PNG is not offered.
- **WebP** follows as a second step with `image-webp` decoding and `libwebp-sys` encoding pixels only.

## Rationale
The transcoder keeps untrusted parsing in safe Rust, and the decode-equality check means a transcoder bug costs
savings, never pixels. mozjpeg-rs gives mozjpeg-class size without C. Feeding oxipng decoded rows keeps its C part
on the encoding side. Keeping files whose bytes are signed or carry extra images avoids silent breakage.

## Alternatives considered
- **`libjpeg-turbo-rs` transform:** complete today, including progressive output, but its decoder parses hostile
  bytes with `unsafe` code and has a single maintainer.
- **C mozjpeg/jpegtran (`mozjpeg-sys`):** mature, but C parses untrusted files, and builds need cmake and nasm.
- **`jpeg-encoder` (already used) for lossy JPEG:** no new dependency, but no trellis quantization; files are larger
  at equal quality.
- **`optimize_from_memory`:** simpler, but libdeflate would inflate the untrusted IDAT and iCCP data in C.
- **Zopfli for every PNG in Maximum:** measured 40 s per 1–2-megapixel screenshot for 1–2.6% over level 6 alone.

## Consequences
- Measured on macOS system images (runtime.md): baseline JPEGs 0.8–6.1% smaller losslessly, 40–87% at Balanced and
  Maximum from high-quality sources; PNGs 5–50% smaller losslessly.
- Progressive JPEGs gain nothing in Lossless unless metadata is removed. Progressive output is a follow-up.
- libdeflate (C) is compiled with `cc` on every runner; Tauri already needs a C toolchain.
- New dependencies: `mozjpeg-rs`, `enough` (its cancellation trait), `oxipng` (with `libdeflater`, `zopfli`), `png`.
  All transitive licenses are MIT, Apache-2.0, Zlib or CC0. Dependabot tracks them.
- oxipng cannot be cancelled from inside; a 60 s timeout bounds its filter trials and Zopfli is capped by size.
- The image tool takes `.jpg`, `.jpeg` and `.png` files; the open dialog still lists every image kind.

## Validation / fitness criteria
- `tests/raster_compress.rs`: lossless JPEG (4:4:4, 4:2:0, 4:2:2, gray, partial MCUs, restart intervals) and PNG
  (RGBA, palette reduction, 16-bit, transparency keys, indexed alpha) decode to identical pixels and shrink;
  optimized files are returned byte for byte; metadata survives or is removed as specified; signed, animated and
  extended files are kept; broken files are refused without panicking; cancellation produces nothing.
- Mutating the transcoder's zero-run coding makes the verification drop the result, and with verification also
  disabled the pixel test fails.
- `src/features/ImageCompress`: presets send their exact numbers; kept reasons are shown; HEIC and other kinds are
  rejected at intake.

## Reconsider when
- Progressive JPEG output or input is wanted in Lossless: extend the transcoder or re-evaluate `libjpeg-turbo-rs`.
- The app gets a license compatible with AGPL, or a permissive pure-Rust WebP or lossy PNG encoder matures.
- oxipng gains cancellation, or users report Maximum as too slow.

## References
- ITU-T T.81 (JPEG): Annex B (syntax), F (sequential Huffman coding), K.2–K.3 (optimal tables)
- ISO/IEC 15948 (PNG), chunk ordering and safe-to-copy rules; C2PA 2.1 (JUMBF in JPEG APP11, PNG `caBX`)
- libjpeg `jpeg_gen_optimal_table`; https://github.com/imazen/mozjpeg-rs; https://github.com/oxipng/oxipng
