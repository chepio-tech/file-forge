# Model export: SAM 2.1 hiera-tiny for "Remove background"

`export_sam2.py` turns Meta's SAM 2.1 hiera-tiny checkpoint into the two ONNX files the app downloads on first use
(ADR-0022): `encoder.onnx` (image → features, run once per image) and `decoder.onnx` (features + clicked points →
masks, run per prompt). The app runs them with tract on the CPU; nothing here ships inside the app.

The model and its weights are Meta's, under Apache-2.0 (`LICENSE` of facebookresearch/sam2), trained on Meta's own
SA-1B and SA-V data, which ADR-0021 accepts. Test images must allow commercial use too (ADR-0021); the two fixtures
in `crates/fileforge-core/tests/fixtures/` are CC0 and public domain.

## The contract (`sam2.1-hiera-tiny/1`)
| File | Inputs | Outputs |
|---|---|---|
| `encoder.onnx` | `image` f32 [1,3,1024,1024]: RGB resized to 1024×1024 (aspect ratio not kept, antialiased bilinear), /255, ImageNet mean/std | `image_embed` [1,256,64,64], `high_res_feats_0` [1,32,256,256], `high_res_feats_1` [1,64,128,128] |
| `decoder.onnx` | the three features; `point_coords` [1,N,2] (x, y in the 1024 frame, ending with the padding point (0,0)); `point_labels` [1,N] (−1 padding, 0 background, 1 foreground, 2/3 box corners); `mask_input` [1,1,256,256]; `has_mask_input` [1] | `masks` [1,4,256,256] logits (0: single-mask output, 1–3: multimask), `iou_predictions` [1,4], `object_score_logits` [1,1] |

Both graphs equal `SAM2ImagePredictor` (`set_image`, then `_predict` with all four mask tokens); the script's
docstring lists the rewrites tract needs and the chunked global attention that halves the encoder's peak memory. Large weights are stored as float16 and cast to float32 at load, which
halves the download (about 82 MB instead of 150 MB). The v1 export measured a minimum mask IoU of 0.9986 against
full-precision PyTorch on the two fixtures; the export gate requires at least 0.98 for every tested mask.

## Run it locally
```sh
python3.13 -m venv .venv
.venv/bin/pip install --index-url https://download.pytorch.org/whl/cpu torch==2.14.1
.venv/bin/pip install -r tools/model-export/requirements.txt
git clone https://github.com/facebookresearch/sam2.git /tmp/sam2
git -C /tmp/sam2 checkout 2b90b9f5ceec907a1c18123530e92e794ad901a4
curl -LO https://dl.fbaipublicfiles.com/segment_anything_2/092824/sam2.1_hiera_tiny.pt
.venv/bin/python tools/model-export/export_sam2.py --sam2 /tmp/sam2 --checkpoint sam2.1_hiera_tiny.pt --out /tmp/model \
  --image crates/fileforge-core/tests/fixtures/chelsea.png --image crates/fileforge-core/tests/fixtures/rocket.jpg
```
The script fails if ONNX Runtime's masks differ from PyTorch's. It also writes `reference/<image>/…` tensors (raw
little-endian f32) for the app engine's parity test, which comes with the engine.

## Release procedure (owner)
1. Actions → **model-release** → Run workflow (tag `models/sam2.1-hiera-tiny-v1`). It downloads the checkpoint from
   Meta and from Hugging Face and requires identical bytes, exports, runs the check and creates a **draft
   prerelease** with `sam2.1-hiera-tiny-encoder.onnx`, `sam2.1-hiera-tiny-decoder.onnx`, `manifest.json`,
   `LICENSE-SAM2.txt` and `SHA256SUMS`.
2. Review the draft, then publish it **as a prerelease** (never "Set as the latest release": `releases/latest` must
   stay the app release that the README buttons and the updater use).
3. Pin the published files' sizes and SHA-256 values in the app (ADR-0022) in a pull request. The app downloads
   only files matching its pins.

A new export (other SAM 2 commit, opset, wrappers) gets a new tag (`-v2`) and new pins; published files never change.
