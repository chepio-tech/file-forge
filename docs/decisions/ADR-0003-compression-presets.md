# ADR-0003: Compression presets: lossless by default, never larger, originals untouched

## Status
Accepted; ADR-0015 adds a fourth lossy PDF preset, Screen (quality 65, 100 DPI).

## Date
2026-10-03

## Context
The brief asked for compression "without quality loss". Strictly lossless compression of PDFs, PNG and JPEG
usually saves 5–30%; video barely shrinks at all. Large savings need lossy encoding that is visually close to the
original ("visually lossless", e.g. JPEG quality ~85, x264 CRF ~18).

## Decision
Every compression tool offers three presets, each showing its exact parameters:
- **Lossless** (default): structure only; decoded content is bit-identical.
- **Balanced**: visually lossless lossy settings (PDF: JPEG quality 85, images above 200 DPI downsampled).
- **Maximum**: smaller files with visible but acceptable loss (PDF: quality 70, 150 DPI).
Professionals can adjust the numbers. Regardless of preset: a result that is not smaller than the input is
replaced by the original bytes, and originals are never written; results stay in a temp file until saved.

## Rationale
Honest naming plus visible numbers lets professionals choose; safe defaults protect everyone else.

## Alternatives considered
- Lossless only: honest but often useless for scans and photo-heavy files.
- A single "smart" mode: hides what changed, which conflicts with "numbers over adjectives".

## Consequences
- Each engine needs a lossless path and a parameterized lossy path, plus tests for both guarantees.

## Validation / fitness criteria
- Engine tests: lossless output decodes to identical content; any preset's output is ≤ input size.

## Reconsider when
- Users consistently pick Balanced; then it may become the default for that tool.
