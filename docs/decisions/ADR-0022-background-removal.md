# ADR-0022: Background removal with SAM 2.1 hiera-tiny in tract, downloaded on first use

## Status
Proposed — awaiting the owner's review. Written on 2026-10-09 by the cloud agent implementing steps A–F, which lands
them in separate pull requests. The owner's plan file (`plans/0016-background-removal.md`) was not available to it,
so every decision beyond the roadmap line (model, tract, download on first use, automatic subject plus click,
transparent PNG/WebP or solid color, optional crop) is the agent's proposal and needs confirmation.

## Date
2026-10-09

## Context
The owner asked for "Remove background" on 2026-10-08 and fixed its shape: SAM 2.1 hiera-tiny run through tract, the
model downloaded on first use, an automatic subject with a click to pick another, a transparent PNG/WebP or a solid
color, and an optional crop. ADR-0021 excluded ISNet, BiRefNet and their derivatives because of their training data.

SAM 2.1 hiera-tiny (Meta) has Apache-2.0 weights trained on Meta's own SA-1B and SA-V data, which ADR-0021 accepts. It
is a promptable segmenter, not a background remover: it needs points or a box and has no notion of "the subject".
tract (Sonos, MIT/Apache-2.0) is a pure-Rust inference engine; the app already forbids `unsafe` code in its own crates
(ADR-0020) and parses untrusted files only in safe Rust (ADR-0015, ADR-0019).

A spike on 2026-10-09 (cloud container, 4 vCPU Xeon) established:
- SAM 2's own ONNX export does not run in tract 0.23.8 (its batch-size `expand`s crash tract's axis optimizer; in-place
  coordinate writes become ScatterND). Three wrapper rewrites with identical math fix it; the graphs then match
  PyTorch within 6·10⁻⁵ on mask logits, for any number of prompt points.
- Load ≈ 0.9 s (both graphs), image encoder 5–7 s per image on that VM (1–4 threads), mask decoder 125–170 ms per
  prompt. Desktop CPUs are expected to be faster; no Apple Silicon measurement exists yet.
- Storing the weights as float16 (computing in float32) makes the download 82 MB instead of about 150 MB; masks
  stay within IoU 0.999 of full precision.
- tract materializes attention matrices: Hiera's three global-attention blocks (4,096 tokens, 4 heads) need 256 MiB
  each, packed twice, and set the encoder's peak at about 0.8 GB. Computing them in 8 query chunks gives identical
  outputs with about 0.45 GB less peak memory (measured on one block; the full encoder is an estimate).

## Decision
- **Model files (step A).** `tools/model-export/export_sam2.py` exports the image encoder and mask decoder of
  `SAM2ImagePredictor` from Meta's checkpoint (sam2 commit `2b90b9f`) to ONNX opset 17 with the three tract rewrites,
  chunked global attention and float16-stored weights (contract `sam2.1-hiera-tiny/1`, see the tool's README). It checks ONNX Runtime against
  PyTorch and writes reference tensors. The manual `model-release` workflow downloads the checkpoint from Meta and from
  Hugging Face, requires identical bytes, exports, checks ONNX Runtime and the app's own engine
  (`tests/background_model.rs`), and creates a **draft prerelease** `models/sam2.1-hiera-tiny-v1` with the two files,
  `manifest.json`, `SHA256SUMS` and SAM 2's Apache-2.0 license. The owner reviews and publishes it as a prerelease,
  so `releases/latest` stays the app release that the README buttons and the updater read (ADR-0009, ADR-0013).
- **Delivery (step D).** The app pins each file's size and SHA-256 (`src-tauri/src/models.rs`). The model is
  downloaded only when the user clicks "Download model" in the tool, over HTTPS from the release, into the app's
  local data folder, as a temp file checked for its exact size and hash before it is renamed into place. Files are
  verified again once per session before loading. Until the release is pinned, release builds show the model as not
  available; debug builds can use a local export through `FILEFORGE_BACKGROUND_MODEL_DIR` for the smoke test.
- **Engine (step B).** `fileforge-core::background` decodes JPEG, PNG and WebP in safe Rust (orientation applied, CMYK
  and animated files refused, at most 64 MP), resizes to 1024×1024 like SAM 2, runs the encoder once per image and
  the decoder per prompt in tract, with matmul threads from a pool owned by the engine (no global state). Mask choice
  follows `SAM2ImagePredictor`: a single click takes the best of the three multimask outputs by predicted IoU;
  several points take the single-mask output unless it is unstable.
- **Automatic subject and clicks (step B).** See "Automatic subject" below. A click replaces the subject with the
  object under the pointer.
- **Cutout (step C).** Mask logits become alpha with SAM 2's small-island and small-hole cleanup on the 256×256 grid
  and a one-pixel anti-aliased ramp computed from the logits' gradient (exact for straight edges, no blur). Pixels
  with zero alpha get zero color, so the removed background cannot be recovered from the file. Output: PNG (lossless)
  or WebP (lossless or lossy with exact alpha) with the ICC profile; other metadata is dropped. A solid color is
  composited in 8-bit sRGB. "Crop to subject" crops to the alpha's bounding box plus 2% of the longer side. Every
  result is decoded again before use.
- **Shell and UI (steps E, F).** A "Remove background" tool in the Images group with its own file list. Files are
  registered per tool, so the compression tool and this tool never share ids or results. The selected file shows a
  preview on a checkerboard; clicking it picks the subject; "Automatic" returns to the automatic subject. Results are
  rendered when saved, as `<name>-cutout.png|webp`, through the existing save and reveal commands.
- **Not done (needs the owner).** An edge-aware refinement (fast guided filter, gated, plus "blur-fusion" color
  decontamination) measured large gains on hair and fur in synthetic tests (solid-edge alpha MSE 25.6·10⁻³ → 0.6·10⁻³).
  Its patent status could not be verified — a related US 9286663 ("filtering an image using a guidance image")
  exists — so it is not shipped.

### Automatic subject
SAM 2 needs prompts, so "the subject" is chosen by running the decoder on a fixed set of prompts and ranking the
candidate masks by SAM's own predicted quality and stability plus image-independent cues (how much of the frame
border a mask touches, how central and how large it is). The exact prompts, weights and thresholds are fixed with the
engine, measured on commercially usable images only (ADR-0021), and recorded here then. When no candidate is
confident, the tool says so and asks for a click.

## Rationale
The model and engine were fixed by the owner; the rest follows the app's existing rules. Downloading on demand keeps
the installers small and never contacts the network without a user action. Pinned hashes make the download host
irrelevant to integrity and tie every installed version to exact model bytes. Keeping preprocessing, mask choice
and alpha in Rust leaves the graphs as plain networks that ONNX Runtime and PyTorch can check.

## Alternatives considered
- **Bundling the model:** +82 MB in every installer and update for a feature not every user needs.
- **SAM 2's own export or ONNX Runtime:** ONNX Runtime is a C++ binary dependency outside the app's safe-Rust rule;
  the original export does not run in tract.
- **A sigmoid of the logits as alpha:** measured 20–260 px of blur on large photos; the gradient ramp is exact.
- **Hosting the model elsewhere (Hugging Face, a separate repository):** another host or repository to secure; the
  pinned hash makes GitHub releases sufficient.

## Consequences
- New dependencies: `tract-onnx` 0.23.8 and `tract-linalg` (MIT/Apache-2.0); tract contains SIMD `unsafe` code. It
  only receives tensors the engine built and model files pinned by hash.
- The app makes a second kind of network request, only on the user's click: the model download from GitHub.
- Background removal needs about 1 GB of memory for a 48 MP photo; images above 64 MP are refused.
- A new export or model is a new release tag and new pins; published model files never change.

## Validation / fitness criteria
- `tests/background_model.rs` (run by the model-release workflow and locally with a model): tract matches ONNX
  Runtime's reference tensors; the app's preprocessing gives the reference masks; the automatic subject on the CC0
  fixtures is confident and plausible; PNG and WebP render end to end.
- Engine unit tests without a model: decoding and limits, coordinate transforms, mask choice, the alpha ramp and
  cleanup against exact geometry, output rules (zero color under zero alpha, opaque pixels unchanged).
- Shell tests: download verification (size, hash, cancel, partial files removed), per-tool file scoping, result
  naming.

## Reconsider when
- The owner clears the guided-filter refinement, or a commercially usable matting model appears.
- Apple Silicon or Windows measurements show the encoder too slow; then consider a smaller input size or a faster
  engine.

## References
- SAM 2: https://github.com/facebookresearch/sam2 (code, checkpoints, Apache-2.0)
- tract: https://github.com/sonos/tract
- K. He, J. Sun, X. Tang, "Guided Image Filtering", ECCV 2010 / TPAMI 2013; M. Forte, F. Pitié, "Approximate Fast
  Foreground Colour Estimation", ICIP 2021 (not implemented, see Decision)
