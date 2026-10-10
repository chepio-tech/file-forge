# ADR-0022: Local background removal with SAM 2.1 and a model downloaded on demand

## Status
Accepted. The owner approved SAM 2.1, automatic selection plus click, download-on-click delivery and tract in
October 2026; tract-onnx was explicitly approved on 2026-10-10 to use the existing reproducible export.

## Date
2026-10-10

## Context
Background removal changes content and can turn a small JPEG into a larger transparent PNG. Compression's
never-larger rule remains unchanged; this tool needs previews and actual sizes. The model is large enough that
bundling it would dominate every installer and updater download. ADR-0021 requires commercial-use rights.

## Decision
Use Meta's SAM 2.1 hiera-tiny (Apache-2.0 weights, trained on Meta's own SA-1B and SA-V data) through tract-onnx
0.23.8 and tract-linalg (MIT/Apache-2.0). Core defines a segmenter port without ML dependencies; a separate
fileforge-segment crate implements normalized 1024-square input, encoder embeddings and prompted mask decoding.
An owned pool caps inference at four threads. One content-keyed embedding is cached and cleared when the tool is
hidden or the model removed.

Automatic selection uses a positive center point and the largest of the three multimasks, as chosen after the
owner's spike. A click selects the multimask with highest predicted IoU. The original preview and keyboard
coordinates always refer to the full oriented image, including when the output is cropped. Originals can be
previewed and clicked before inference, so an off-center subject does not depend on automatic success.

The existing export tool builds two ONNX opset-17 graphs from Meta's official checkpoint and checks them against
unmodified PyTorch. Chunked attention reduces memory; float16-stored weights compute in float32. Model files live
in the dedicated immutable prerelease models/sam2.1-hiera-tiny-v1. The app pins their exact sizes and SHA-256 and
downloads them only on an explicit click, into app-local data via verified staging files. The owner approved direct
rustls with the already locked ring provider (MIT/Apache-2.0/ISC) on 2026-10-10; model downloads initialize TLS
independently of update checks. The download sends no
user-file information. Prereleases never replace the latest desktop release.

Safe Rust decoders apply orientation and bound inputs to 256 MB/32 MP. Animation, CMYK and 16-bit JPEG are refused;
16-bit PNG is reduced to 8-bit. WebP containers that would silently discard source alpha are refused. Bilinear
logits and color-guided local regression give antialiased alpha. Coefficients are computed on a maximum 1024-pixel
grid and evaluated with original-resolution colors. Confident mask regions remain fixed.
Soft-edge RGB is recovered with local foreground/background priors and a regularized alpha-compositing equation.
Source alpha is multiplied; fully transparent RGB is zero and fully retained RGB stays exact. Optional crop adds 2%
padding; a solid color is composited afterwards. Lossless PNG/WebP output keeps ICC, drops other metadata and is
decoded again before use. Fine hair and translucent surfaces remain a visible limitation; there is no separate
matting model or brush editor in this version.

The UI follows the existing batch tool layout. Registry ids and temp results are independent from compression.
Results use the suffix -cutout and existing protected, atomic save/reveal commands. Previews are PNG byte arrays
bounded to 512 pixels, converted to data URLs by the typed client. Cutout sizes can exceed input sizes; no savings
percentage is shown. Subject choices persist per file when output settings change. Failed status and preview
requests can be retried. Model management, inference and saves share one work slot, also blocking restart for updates.

## Rationale
The export already exists and the owner approved tract-onnx instead of an additional NNEF conversion. CPU-only
inference works across all desktop targets. Download-on-demand keeps installers small and supports offline work
once installed. Exact pins prevent incorrect, modified or partial graphs from reaching the parser. A cached
embedding makes another subject selection inexpensive.

## Alternatives considered
- NNEF: a smaller parser, but another conversion/validation step before release.
- Bundled weights: about 83 MB added to every installer and update, even when the tool is unused.
- ONNX Runtime: another native runtime and FFI boundary; tract keeps the adapter in safe Rust.
- Third-party guided-filter and foreground-estimation source: excluded where commercial-use licensing is absent.
  The implementation is original Rust from the mathematical filter and compositing equations.
- Highest predicted IoU for automatic selection: the owner's spike picked parts of the subject; largest-mask
  selection found whole subjects. Clicks provide an explicit correction for ambiguous or off-center photos.

## Consequences
A first use needs a model download. Images and result sizes are bounded, but segmentation cannot identify every
intended subject or preserve all fine hair; the UI exposes the original, result and correction controls. The last
embedding and graphs consume memory while this tool is visible. A different export requires a new model tag and
new app pins; published assets never change. The only new network action is a model download after a click.

## Validation / fitness criteria
Core fake-segmenter tests cover alpha, opaque RGB, hidden RGB, orientation, crop, solid composition, output codecs,
cancellation, malformed images/masks and invalid points. Shell tests cover scoped ids, cutout names and partial,
corrupt, oversized and cancelled downloads. UI tests cover explicit model download, offline/error states, batch
processing, coordinate prompts, output changes and saving. The optional background_model test runs the release
files and compares raw tract masks with ONNX Runtime references when present; real-model tests must pass before
publication. Standard CI has no model files.

## Reconsider when
A commercially usable matting model or validated refinement can substantially improve hair and translucent
objects; platform measurements justify a faster inference engine or changed input/memory limits.

## References
- SAM 2 and its license: https://github.com/facebookresearch/sam2
- tract and licenses: https://github.com/sonos/tract
- Model contract and reproducible export: tools/model-export/README.md
- Commercial-use policy: ADR-0021

- Color-guided filtering: https://arxiv.org/abs/1505.00996; He, Sun and Tang, Guided Image Filtering, TPAMI 2013.
