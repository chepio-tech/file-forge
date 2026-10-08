# ADR-0020: WebP compression with image-webp decoding and an isolated libwebp encoder crate

## Status
Accepted

## Date
2026-10-08

## Context
ADR-0019 planned WebP as a second step: `image-webp` decodes, libwebp encodes pixels only. The only permissive
WebP encoders that compress well are libwebp's (C); `image-webp`'s own encoder is lossless-only and fast, not small,
and `zenwebp` is AGPL-3.0. libwebp is reached through `libwebp-sys` 0.14.4 (libwebp 1.6.0), whose functions are raw C
calls that need `unsafe`. The workspace forbids `unsafe` code (`unsafe_code = "forbid"`).

The ready-made safe wrapper `webp` 0.3.1 avoids our own `unsafe`, but depends on the `libwebp-sys` 0.9 line
(libwebp 1.3.1 with the CVE-2023-4863 fix backported), which has had no release since 2024-09, so later libwebp fixes
would reach us only through a new `webp` release. It also offers no way to stop an encode that is running.

## Decision
- **Isolated `unsafe`:** a new crate `crates/fileforge-webp` is the only crate allowed to contain `unsafe` code. It
  exposes one safe function that encodes validated RGB or RGBA pixels to WebP (lossless or lossy) and can be stopped
  through a callback. Every `unsafe` block carries a `// SAFETY:` comment (`clippy::undocumented_unsafe_blocks` and
  `unsafe_op_in_unsafe_fn` are denied). Input is validated in safe Rust before any C call (1–16383 pixels per side,
  exact buffer length); guards free libwebp's picture and output buffer on every path; the progress callback never
  lets a panic unwind into C. `fileforge-core` keeps `forbid(unsafe_code)` and does not use `libwebp-sys` directly.
- **No C decoding:** untrusted WebP files are parsed only by our RIFF reader and `image-webp`
  (`forbid(unsafe_code)`). libwebp only encodes pixels `image-webp` produced; its decoder is linked but never called
  by the app.
- **Kept byte for byte:** animated WebP, files with a `C2PA` chunk (Content Credentials) and files with bytes after
  the RIFF data. The report names the reason.
- **Lossless WebP stays lossless in every preset**, as PNG does: libwebp re-encodes it with `exact` (transparent
  pixels keep their colors), and the result must decode to the same RGBA samples.
- **Lossy WebP** keeps its pixels unless a WebP quality is set (Balanced, Maximum, Custom). A re-encode replaces the
  best lossless result only when at least 2% smaller (ADR-0015 rule); alpha is encoded losslessly and must decode
  to the same values. With no quality and no metadata removal the report says `lossyEncoding`.
- **Container:** the result keeps the original chunk order with new image chunks; VP8X flags and canvas size are
  recomputed. Metadata follows ADR-0019: kept by default; "Remove metadata" reduces EXIF to its orientation and
  removes XMP and unknown chunks; the ICC profile stays.

## Rationale
Keeping all untrusted parsing in safe Rust follows ADR-0015 and ADR-0019; libwebp's attack surface is limited to
pixels the engine produced. Our own small wrapper gives current libwebp, full encoder settings and cancellation
inside an encode, at the cost of about 150 lines of `unsafe` code that only tests can check. Putting that code in its
own crate keeps the exception visible, reviewable and out of the engine.

## Alternatives considered
- **`webp` 0.3.1:** no own `unsafe`, but old libwebp, an unmaintained base and no cancellation inside an encode.
- **`image-webp` encoder only:** pure Rust, but lossless-only and weaker than libwebp; most WebP files would not
  shrink, and lossy WebP could not be re-encoded.
- **Allowing `unsafe` in `fileforge-core`:** smaller change, but the exception would cover the whole engine that
  parses hostile files.
- **Converting lossless WebP to lossy in lossy presets:** larger savings, but screenshots, text and line art would
  get artifacts; reliable detection (as in ADR-0016 for PDFs) is separate work.

## Consequences
- Measured (runtime.md): lossless app WebPs −6.7% in total, a lossless UI image −24%; lossy app WebPs −14% at
  Balanced and −31% at Maximum; q90 photos −23…53% at Balanced. Lossless uses libwebp level 9 up to 2048×2048 pixels
  (up to about 12 s) and level 7 above; lossy uses method 6, or 5 with alpha, where method 6 costs 10–30× the time.
- New dependencies: `image-webp` (MIT/Apache-2.0) with `byteorder-lite` (Unlicense/MIT) and `quick-error`;
  `libwebp-sys` (MIT; libwebp BSD-3-Clause with Google's patent grant) with build-time `cc`, `glob`, `pkg-config`.
  libwebp is compiled from source with `cc` on every runner; Tauri already needs a C toolchain.
- The image tool accepts `.webp`; result files keep the extension.
- A bug in the wrapper's `unsafe` code could corrupt memory; tests, the small surface and review are the only guards.

## Validation / fitness criteria
- `crates/fileforge-webp` tests: lossless round trips return identical RGB/RGBA pixels at edge sizes; invalid input
  is refused before C runs; a stop request ends an encode; a panicking callback does not abort the process;
  libwebp's decoder and `image-webp` agree on the encoder's output.
- `tests/raster_webp.rs`: lossless WebP decodes to identical RGBA (including colors under full transparency) and
  shrinks; lossy re-encodes keep alpha exactly; metadata is kept or removed as specified; animated, signed and
  extended files are kept; broken files are refused without panicking; cancellation produces nothing.
- No `unsafe` outside `crates/fileforge-webp` (`unsafe_code = "forbid"` in the workspace lints).

## Reconsider when
- A permissive pure-Rust WebP encoder matches libwebp's size, or `libwebp-sys` gains a maintained safe API.
- Users want lossless WebP converted to lossy, or animated WebP compressed.

## References
- WebP container specification (RFC 9649), VP8L bitstream specification
- https://github.com/image-rs/image-webp; https://github.com/NoXF/libwebp-sys; libwebp `encode.h`
- C2PA 2.1, embedding in RIFF formats
