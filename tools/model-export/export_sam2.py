"""Exports SAM 2.1 hiera-tiny's image encoder and mask decoder to ONNX for File Forge's background removal (ADR-0022).

The two graphs are exactly what `SAM2ImagePredictor.set_image` and `SAM2ImagePredictor._predict` compute after the
image transform, rewritten where tract (the app's inference engine) cannot run SAM 2's original export:
- Hiera's bicubic positional-embedding interpolation is stored as a constant (the input size is fixed).
- The mask decoder runs for one image and one prompt set, without SAM 2's batch-size `expand`s.
- Point positional encoding avoids in-place slice writes (they export as ScatterND).
Large weights are stored as float16 followed by a Cast to float32, which halves the download; computation stays f32.

After exporting, the script checks the ONNX files with ONNX Runtime against SAM 2 in PyTorch on the given images and
writes reference tensors for comparing the app's engine (tract) against. See README.md for the release procedure.

Usage:
  python export_sam2.py --sam2 <facebookresearch/sam2 checkout> --checkpoint sam2.1_hiera_tiny.pt --out <dir>
      [--image <png/jpg> ...]
"""

import argparse
import hashlib
import json
import os
import sys

import numpy as np
import onnx
import onnxruntime
import torch
import torch.nn.functional as F
from onnx import TensorProto, helper, numpy_helper
from PIL import Image
from torch import nn

# The contract the app's engine checks; bump it with any change to inputs, outputs or their meaning.
CONTRACT = "sam2.1-hiera-tiny/1"
IMAGE_SIZE = 1024
MEAN = (0.485, 0.456, 0.406)
STD = (0.229, 0.224, 0.225)
# Tensors with at least this many elements are stored as float16.
F16_MIN_ELEMENTS = 1024


def load_model(sam2_dir: str, checkpoint: str) -> nn.Module:
    sys.path.insert(0, os.path.abspath(sam2_dir))
    from hydra import compose, initialize_config_dir
    from hydra.core.global_hydra import GlobalHydra
    from hydra.utils import instantiate
    from omegaconf import OmegaConf

    GlobalHydra.instance().clear()
    initialize_config_dir(config_dir=os.path.abspath(os.path.join(sam2_dir, "sam2")), version_base="1.2")
    cfg = compose(config_name="configs/sam2.1/sam2.1_hiera_t.yaml")
    OmegaConf.resolve(cfg)
    model = instantiate(cfg.model, _recursive_=True)
    state = torch.load(checkpoint, map_location="cpu", weights_only=True)
    state = state["model"] if "model" in state else state
    state = {k: v.float() if v.is_floating_point() else v for k, v in state.items()}
    # Strict: every weight must land where SAM 2 expects it.
    model.load_state_dict(state, strict=True)
    return model.eval()


class Encoder(nn.Module):
    """`image` 1×3×1024×1024 (RGB resized to 1024×1024, /255, ImageNet mean/std) → decoder features."""

    def __init__(self, model: nn.Module):
        super().__init__()
        self.model = model
        trunk = model.image_encoder.trunk
        with torch.no_grad():
            # Hiera's patch embedding has stride 4: 1024 → 256 tokens per side.
            pos_embed = trunk._get_pos_embed((IMAGE_SIZE // 4, IMAGE_SIZE // 4)).clone()
        trunk._get_pos_embed = lambda hw: pos_embed

    def forward(self, image):
        backbone_out = self.model.forward_image(image)
        _, vision_feats, _, _ = self.model._prepare_backbone_features(backbone_out)
        vision_feats[-1] = vision_feats[-1] + self.model.no_mem_embed
        sizes = [(IMAGE_SIZE // 4, IMAGE_SIZE // 4), (IMAGE_SIZE // 8, IMAGE_SIZE // 8), (IMAGE_SIZE // 16, IMAGE_SIZE // 16)]
        feats = [f.permute(1, 2, 0).reshape(1, -1, h, w) for f, (h, w) in zip(vision_feats, sizes)]
        return feats[2], feats[0], feats[1]


class Decoder(nn.Module):
    """Prompts → all four mask logit maps, their predicted IoUs and the object score.

    `point_coords` 1×N×2 are (x, y) in the 1024×1024 input frame and must already end with SAM's padding point
    (0, 0) labelled −1. `point_labels` 1×N: −1 padding, 0 background, 1 foreground, 2/3 box corners.
    `mask_input` 1×1×256×256 holds a previous prediction's logits, used when `has_mask_input` is 1.
    Outputs: `masks` 1×4×256×256 logits (token 0: single-mask output, 1–3: multimask), `iou_predictions` 1×4,
    `object_score_logits` 1×1.
    """

    def __init__(self, model: nn.Module):
        super().__init__()
        self.prompt = model.sam_prompt_encoder
        self.decoder = model.sam_mask_decoder
        self.register_buffer("image_pe", self.prompt.get_dense_pe(), persistent=False)

    def embed_points(self, points, labels):
        points = points + 0.5
        height, width = self.prompt.input_image_size
        scale = torch.tensor([1.0 / width, 1.0 / height], dtype=points.dtype)
        embedding = self.prompt.pe_layer._pe_encoding(points * scale)
        labels = labels.unsqueeze(-1)
        not_a_point = (labels == -1).float()
        embedding = embedding * (1 - not_a_point) + self.prompt.not_a_point_embed.weight * not_a_point
        for value in range(4):
            embedding = embedding + self.prompt.point_embeddings[value].weight * (labels == value).float()
        return embedding

    def predict_masks(self, image_embed, sparse, dense, high_res_0, high_res_1):
        d = self.decoder
        output_tokens = torch.cat([d.obj_score_token.weight, d.iou_token.weight, d.mask_tokens.weight], dim=0)
        tokens = torch.cat((output_tokens.unsqueeze(0), sparse), dim=1)
        src = image_embed + dense
        b, c, h, w = src.shape
        hs, src = d.transformer(src, self.image_pe, tokens)
        iou_token_out = hs[:, 1, :]
        mask_tokens_out = hs[:, 2 : 2 + d.num_mask_tokens, :]
        src = src.transpose(1, 2).reshape(b, c, h, w)
        dc1, ln1, act1, dc2, act2 = d.output_upscaling
        upscaled = act1(ln1(dc1(src) + high_res_1))
        upscaled = act2(dc2(upscaled) + high_res_0)
        hyper_in = torch.stack(
            [d.output_hypernetworks_mlps[i](mask_tokens_out[:, i, :]) for i in range(d.num_mask_tokens)], dim=1
        )
        b, c, h, w = upscaled.shape
        masks = (hyper_in @ upscaled.reshape(b, c, h * w)).reshape(b, -1, h, w)
        return masks, d.iou_prediction_head(iou_token_out), d.pred_obj_score_head(hs[:, 0, :])

    def forward(self, image_embed, high_res_0, high_res_1, point_coords, point_labels, mask_input, has_mask_input):
        sparse = self.embed_points(point_coords, point_labels)
        no_mask = self.prompt.no_mask_embed.weight.reshape(1, -1, 1, 1)
        dense = has_mask_input * self.prompt.mask_downscaling(mask_input) + (1 - has_mask_input) * no_mask
        return self.predict_masks(image_embed, sparse, dense, high_res_0, high_res_1)


def reference_decoder(model, feats, coords, labels):
    """What SAM2ImagePredictor._predict computes (multimask output) for the same prompt, without the padding point."""
    embed, hr0, hr1 = feats
    sparse, dense = model.sam_prompt_encoder(points=(coords, labels.int()), boxes=None, masks=None)
    masks, iou, _, obj = model.sam_mask_decoder(
        image_embeddings=embed,
        image_pe=model.sam_prompt_encoder.get_dense_pe(),
        sparse_prompt_embeddings=sparse,
        dense_prompt_embeddings=dense,
        multimask_output=True,
        repeat_image=False,
        high_res_features=[hr0, hr1],
    )
    return masks, iou, obj


def store_weights_as_f16(path: str) -> int:
    """Rewrites large float32 initializers and Constant tensors as float16 followed by a Cast to float32."""
    model = onnx.load(path)
    graph = model.graph
    nodes, initializers, converted = [], [], 0
    for init in graph.initializer:
        if init.data_type == TensorProto.FLOAT and int(np.prod(init.dims)) >= F16_MIN_ELEMENTS:
            half = numpy_helper.from_array(numpy_helper.to_array(init).astype(np.float16), init.name + "_f16")
            initializers.append(half)
            nodes.append(helper.make_node("Cast", [half.name], [init.name], to=TensorProto.FLOAT))
            converted += 1
        else:
            initializers.append(init)
    for node in graph.node:
        value = next((a for a in node.attribute if a.name == "value"), None) if node.op_type == "Constant" else None
        if value is not None and value.t.data_type == TensorProto.FLOAT and int(np.prod(value.t.dims)) >= F16_MIN_ELEMENTS:
            output = node.output[0]
            half = numpy_helper.from_array(numpy_helper.to_array(value.t).astype(np.float16))
            nodes.append(helper.make_node("Constant", [], [output + "_f16"], value=half))
            nodes.append(helper.make_node("Cast", [output + "_f16"], [output], to=TensorProto.FLOAT))
            converted += 1
        else:
            nodes.append(node)
    del graph.initializer[:]
    graph.initializer.extend(initializers)
    del graph.node[:]
    graph.node.extend(nodes)
    model.producer_name = "file-forge/tools/model-export"
    onnx.checker.check_model(model)
    onnx.save(model, path)
    return converted


def preprocess(path):
    image = np.asarray(Image.open(path).convert("RGB"))
    x = torch.from_numpy(image.copy()).permute(2, 0, 1).float().unsqueeze(0) / 255.0
    # torchvision's Resize on tensors (SAM2Transforms) is F.interpolate bilinear with antialias.
    x = F.interpolate(x, (IMAGE_SIZE, IMAGE_SIZE), mode="bilinear", align_corners=False, antialias=True)
    mean = torch.tensor(MEAN).view(1, 3, 1, 1)
    std = torch.tensor(STD).view(1, 3, 1, 1)
    return image, (x - mean) / std


def mask_iou(a, b):
    a, b = a > 0, b > 0
    union = np.logical_or(a, b).sum()
    return 1.0 if union == 0 else np.logical_and(a, b).sum() / union


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def write_f32(path, array):
    np.ascontiguousarray(array, dtype="<f4").tofile(path)


def check(model, out_dir, images):
    """ONNX Runtime vs PyTorch on each image (center click and a three-point prompt); writes tract reference data."""
    options = onnxruntime.SessionOptions()
    options.graph_optimization_level = onnxruntime.GraphOptimizationLevel.ORT_DISABLE_ALL
    encoder = onnxruntime.InferenceSession(os.path.join(out_dir, "encoder.onnx"), options)
    decoder = onnxruntime.InferenceSession(os.path.join(out_dir, "decoder.onnx"), options)
    torch_encoder = Encoder(model).eval()
    report = []
    for image_path in images:
        name = os.path.splitext(os.path.basename(image_path))[0]
        image, x = preprocess(image_path)
        h, w = image.shape[:2]
        with torch.no_grad():
            feats = torch_encoder(x)
        onnx_feats = encoder.run(None, {"image": x.numpy()})
        reference_dir = os.path.join(out_dir, "reference", name)
        os.makedirs(reference_dir, exist_ok=True)
        write_f32(os.path.join(reference_dir, "image.f32"), x.numpy())
        for tensor_name, array in zip(["image_embed", "high_res_feats_0", "high_res_feats_1"], onnx_feats):
            write_f32(os.path.join(reference_dir, f"{tensor_name}.f32"), array)
        prompts = {
            "center": [(w / 2, h / 2, 1.0)],
            "three": [(w * 0.4, h * 0.4, 1.0), (w * 0.6, h * 0.6, 1.0), (w * 0.05, h * 0.05, 0.0)],
        }
        for prompt_name, points in prompts.items():
            coords = torch.tensor([[[px / w * IMAGE_SIZE, py / h * IMAGE_SIZE] for px, py, _ in points]])
            labels = torch.tensor([[label for _, _, label in points]])
            with torch.no_grad():
                ref_masks, ref_iou, ref_obj = reference_decoder(model, feats, coords, labels)
            padded_coords = np.concatenate([coords.numpy(), np.zeros((1, 1, 2), np.float32)], axis=1)
            padded_labels = np.concatenate([labels.numpy(), -np.ones((1, 1), np.float32)], axis=1)
            masks, iou, obj = decoder.run(
                None,
                {
                    "image_embed": onnx_feats[0],
                    "high_res_feats_0": onnx_feats[1],
                    "high_res_feats_1": onnx_feats[2],
                    "point_coords": padded_coords.astype(np.float32),
                    "point_labels": padded_labels.astype(np.float32),
                    "mask_input": np.zeros((1, 1, 256, 256), np.float32),
                    "has_mask_input": np.zeros((1,), np.float32),
                },
            )
            ious = [mask_iou(masks[0, k + 1], ref_masks[0, k].numpy()) for k in range(3)]
            entry = {
                "image": name,
                "prompt": prompt_name,
                "mask_iou_vs_pytorch": [round(float(v), 5) for v in ious],
                "iou_prediction_max_abs_diff": float(np.abs(iou[0, 1:] - ref_iou[0].numpy()).max()),
                "object_score_abs_diff": float(abs(obj[0, 0] - ref_obj[0, 0].item())),
            }
            report.append(entry)
            print(json.dumps(entry))
            # f16-stored weights round once; masks must still agree almost everywhere with full-precision SAM 2.
            if min(ious) < 0.98 or entry["iou_prediction_max_abs_diff"] > 0.02:
                raise SystemExit(f"ONNX output differs from PyTorch: {entry}")
            base = os.path.join(reference_dir, prompt_name)
            write_f32(base + ".point_coords.f32", padded_coords)
            write_f32(base + ".point_labels.f32", padded_labels)
            write_f32(base + ".masks.f32", masks)
            write_f32(base + ".iou_predictions.f32", iou)
            write_f32(base + ".object_score_logits.f32", obj)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sam2", required=True, help="facebookresearch/sam2 checkout")
    parser.add_argument("--checkpoint", required=True, help="sam2.1_hiera_tiny.pt")
    parser.add_argument("--out", required=True)
    parser.add_argument("--image", action="append", default=[], help="image for the parity check (repeatable)")
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    torch.manual_seed(0)
    model = load_model(args.sam2, args.checkpoint)

    encoder = Encoder(model).eval()
    image = torch.zeros(1, 3, IMAGE_SIZE, IMAGE_SIZE)
    with torch.no_grad():
        embed, hr0, hr1 = encoder(image)
    encoder_path = os.path.join(args.out, "encoder.onnx")
    torch.onnx.export(
        encoder,
        (image,),
        encoder_path,
        input_names=["image"],
        output_names=["image_embed", "high_res_feats_0", "high_res_feats_1"],
        opset_version=17,
        do_constant_folding=True,
        dynamo=False,
    )

    decoder = Decoder(model).eval()
    example = (
        embed,
        hr0,
        hr1,
        torch.tensor([[[512.0, 512.0], [0.0, 0.0]]]),
        torch.tensor([[1.0, -1.0]]),
        torch.zeros(1, 1, 256, 256),
        torch.zeros(1),
    )
    decoder_path = os.path.join(args.out, "decoder.onnx")
    torch.onnx.export(
        decoder,
        example,
        decoder_path,
        input_names=[
            "image_embed",
            "high_res_feats_0",
            "high_res_feats_1",
            "point_coords",
            "point_labels",
            "mask_input",
            "has_mask_input",
        ],
        output_names=["masks", "iou_predictions", "object_score_logits"],
        dynamic_axes={"point_coords": {1: "points"}, "point_labels": {1: "points"}},
        opset_version=17,
        do_constant_folding=True,
        dynamo=False,
    )
    for path in (encoder_path, decoder_path):
        print(f"{os.path.basename(path)}: {store_weights_as_f16(path)} tensors stored as float16")

    report = check(model, args.out, args.image)
    manifest = {
        "contract": CONTRACT,
        "checkpoint_sha256": sha256(args.checkpoint),
        "files": {
            name: {"size": os.path.getsize(os.path.join(args.out, name)), "sha256": sha256(os.path.join(args.out, name))}
            for name in ("encoder.onnx", "decoder.onnx")
        },
        "parity": report,
    }
    with open(os.path.join(args.out, "manifest.json"), "w") as file:
        json.dump(manifest, file, indent=2)
    print(json.dumps(manifest["files"], indent=2))


if __name__ == "__main__":
    main()
