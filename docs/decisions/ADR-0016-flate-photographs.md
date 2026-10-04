# ADR-0016: Convert suitable Flate photographs to JPEG in Maximum

## Status
Accepted

## Date
2026-10-04

## Context
Lossy presets could downsample Flate images but kept their lossless encoding. A generated 1024×768 RGB photograph
already at Maximum's 150 DPI shrank only 0.2% (2,098 → 2,094 KB); changing encoding has much more potential.
JPEG can damage text, flat UI panels and line art even when it makes their stream smaller, so size alone is
insufficient. The owner selected this optimization and its branch on 2026-10-04.

## Decision
- Add the boolean image option `compressFlatePhotos`, defaulting to false when omitted. Maximum enables it;
  Custom keeps it when editing Maximum's quality/DPI. Other presets keep it disabled, including Lossless.
- Convert only single-filter Flate images with validated 8-bit gray/RGB layouts, no Decode array or mask, and
  photographic source pixels. Keep all existing size, pixel, concurrency and cancellation limits.
- Before resizing, inspect every source tile with a conservative screen-content veto in `pdf/photos.rs`.
  Repeated tones, flat regions and dense sharp transitions veto the whole image; overlapping final tiles include
  right/bottom edges. Small or uncertain images keep Flate. The scan uses fixed working storage and linear work.
- JPEG must save at least 20% against both the original stream and the Deflate encoding of the same output
  pixels. Otherwise use the existing Deflate downsampling path or preserve the stream. The document-level
  never-larger fallback stays unchanged.
- Accept direct TIFF/PNG predictor dictionaries only when their declared layout matches the image. Skip
  unsupported predictors, indirect/array parameters and malformed data; decode within the existing stream limit
  and the expected sample count plus at most one PNG filter byte per row.
- Explain the extra image conversion in the settings hint whenever enabled; include it in the result settings key.

## Rationale
An explicit option prevents an accidental behavior change in older callers or other presets. Comparing JPEG
against equivalent Deflate prevents a lossless-to-lossy conversion just because the input used inefficient Flate.
Inspecting source pixels keeps downsampling from hiding text and UI edges. Vetoing the entire image protects
photo-plus-interface composites without segmentation, a model, dependencies or network access.

## Alternatives considered
- JPEG for every Flate image when smaller: size savings do not justify artifacts around screen text.
- Only a global histogram or a sparse sample: a large photo can hide a small UI/text region.
- A trained classifier or page segmentation: new dependencies/data and complexity without a measured need.
- Infer the policy from quality/DPI numbers: Custom could unexpectedly change behavior when crossing a number.

## Consequences
- Generated RGB/gray photographs at 150 DPI shrink by 97.1%/88.0%; these are illustrative fixtures, not promises
  for real-world PDFs. Measurements are in `docs/architecture/runtime.md`.
- Classification deliberately preserves some actual photographs, especially flat scenes, borders or text overlays.
  It is a heuristic, not proof of image origin: an identical fullscreen screenshot of a photograph cannot be
  distinguished from the photograph. Other unusual screen content may also pass; use Lossless when pixels must
  remain identical. Detected screen content retains Deflate but can still undergo the preset's existing DPI resize.
- Maximum does extra bounded scanning and Deflate encoding. No new dependencies, permissions or network flows.

## Validation / fitness criteria
- `pdf_flate_photos.rs`: RGB/gray conversion at target DPI, downsampling, repeated compression, unchanged page
  content, disabled option, UI/text/gradient/composite vetoes, equivalent-Deflate comparison, TIFF/PNG predictors,
  malformed layouts and streams, never larger.
- `pdf::images` unit tests: exact 20% boundary and bounded stream reads; `pdf::photos`: partial edge tiles and
  invalid layouts. Removing the screen-content veto or equivalent-Deflate comparison fails integration tests.
- Options serde tests: omitted flag remains false, camelCase round trip, invalid flag types refused.
- Frontend tests: Maximum enables/explains conversion, other presets disable it, Custom retains it, settings keys
  distinguish identical numeric settings with different conversion policies.

## Reconsider when
Representative real photos/screenshots show unacceptable misses; add regression fixtures before changing thresholds.
Users request independent control or the same behavior in Screen/Balanced; change those presets explicitly.

## References
- ADR-0003, ADR-0015.
- Ghostscript's automatic image filtering also examines image suitability before choosing JPEG or lossless
  compression: https://ghostscript.readthedocs.io/en/latest/VectorDevices.html#distiller-parameters
