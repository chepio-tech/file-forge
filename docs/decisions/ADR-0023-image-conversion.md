# ADR-0023: Local JPEG, PNG and WebP conversion

## Status
Accepted

## Date
2026-10-24

## Context
Users need a format-changing operation distinct from compression. A PNG made from a JPEG can be larger while
still being the requested result. The existing safe decoders and encoders already support these three formats.
The owner approved delivering JPEG/PNG/WebP first, leaving AVIF and HEIC for separate codec and license review.

## Decision
- Keep the existing Rust/React stack and dependencies. Share bounded, oriented RGBA decoding and verified
  lossless encoding between background removal and conversion through `raster::pixels`.
- Offer PNG by default, lossless WebP, and JPEG at quality 30–95 (default 90), with 4:4:4 chroma. JPEG replaces
  transparency with an explicitly selected white or black background; the core accepts an RGB triplet.
- Apply orientation before encoding, preserve RGB ICC profiles, and remove other metadata. Disclose this before
  processing. Refuse non-RGB ICC profiles instead of attaching them to RGB samples.
- Refuse animated, signed, extended/multi-image inputs, 16-bit PNG, unsupported JPEG encodings and PNG color
  descriptions that cannot be represented by the shared decoder. Do not silently discard frames or precision.
- Re-decode every result. PNG/WebP must match every decoded RGBA sample and ICC byte; JPEG must retain dimensions
  and ICC. Conversion may increase file size; show actual sizes without a savings percentage.
- Give conversion a separate registry scope and `-converted` output suffix. Use the existing work slot,
  cancellation tickets, temporary storage and original-protecting save paths.

## Rationale
This delivers useful conversion without new dependencies, permissions, network access or changes to compression
invariants. Reusing pixel codecs avoids divergent orientation and alpha handling between editing tools.

## Alternatives considered
- Apply the compression "never larger" rule: rejected because it can return the wrong format.
- Add AVIF/HEIC in the same release: deferred by the owner until dependencies and licensing are reviewed.
- Decode through native C libraries: conflicts with the existing untrusted-file boundary.
- Silently reduce 16-bit inputs: rejected because the lossless targets would hide precision loss.

## Consequences
Conversion strips non-color metadata even for same-format output, as disclosed in the UI. JPEG is always a lossy
re-encode, even at the highest offered quality. Advanced color descriptions and high-bit-depth images need a
future color-aware path. No metadata-preservation toggle or resizing is introduced here.

## Validation / fitness criteria
`cargo test -p fileforge-core --test raster_convert --locked` checks all format pairs, orientation, alpha,
profiles, larger results, limits, cancellation and malformed input. Shell conversion tests exercise independent
results, original protection and non-overwriting batch saves. `pnpm vitest run src/features/ImageConvert` checks
intake, settings, results, saving, cancellation and errors.

## Reconsider when
AVIF/HEIC dependencies are approved, users need high-bit-depth/color-managed conversion, or measurements show
that the encoder effort needs adjustment.
