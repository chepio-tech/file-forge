#!/usr/bin/env python3
"""Generate owned fixtures and evaluate the pinned decoder against Pillow/libavif.

Exit 1 means a decoder acceptance gate failed; it must not be interpreted as a
successful integration check. Nothing in this script edits application files.
"""

# Core
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import subprocess
import sys
import time
# Domain
from referencePlanes import compare_planes, decode_planes, oracle_versions

MAX_MEAN_RGB_ERROR = 1.0
MAX_RGB_ERROR = 8
MAX_CANCEL_LATENCY_MS = 250
MIB = 1024 * 1024


def pattern(width, height, alpha=False):
    """Original deterministic gradients, edges and transparency; no external corpus."""
    # Core
    from PIL import Image
    pixels = bytearray(width * height * (4 if alpha else 3))
    channels = 4 if alpha else 3
    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * channels
            pixels[offset:offset + 3] = bytes((
                x * 255 // max(1, width - 1),
                y * 255 // max(1, height - 1),
                (x * 7 + y * 13 + (80 if (x // 9 + y // 7) % 2 else 0)) % 256,
            ))
            if alpha:
                pixels[offset + 3] = (0, 64, 128, 255)[(x // 7 + y // 5) % 4]
    return Image.frombytes("RGBA" if alpha else "RGB", (width, height), bytes(pixels))


def pixel_metrics(actual, expected):
    if len(actual) != len(expected) or len(actual) % 4:
        raise ValueError("RGBA byte counts differ")
    rgb_total = rgb_max = rgb_samples = alpha_max = 0
    for offset in range(0, len(actual), 4):
        alpha_max = max(alpha_max, abs(actual[offset + 3] - expected[offset + 3]))
        # Fully transparent RGB does not describe a displayed color.
        if expected[offset + 3]:
            for channel in range(3):
                error = abs(actual[offset + channel] - expected[offset + channel])
                rgb_total += error
                rgb_max = max(rgb_max, error)
                rgb_samples += 1
    return {"rgb_mean_error": rgb_total / max(1, rgb_samples), "rgb_max_error": rgb_max,
            "alpha_max_error": alpha_max}


def file_hash(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def execute(binary, source, output, logs, extra=(), timeout=20):
    """One child at a time; wait4 gives its own peak RSS instead of a cumulative maximum."""
    before = file_hash(source)
    start = time.monotonic()
    timed_out = False
    peak = None
    with logs.with_suffix(".stdout").open("x") as stdout, logs.with_suffix(".stderr").open("x") as stderr:
        proc = subprocess.Popen([str(binary), str(source), str(output), *extra], stdout=stdout, stderr=stderr)
        try:
            if hasattr(os, "wait4"):
                while True:
                    pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
                    if pid:
                        proc.returncode = os.waitstatus_to_exitcode(status)
                        peak = usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
                        break
                    if time.monotonic() - start > timeout:
                        timed_out = True
                        proc.kill()
                        _, status, usage = os.wait4(proc.pid, 0)
                        proc.returncode = os.waitstatus_to_exitcode(status)
                        peak = usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
                        break
                    time.sleep(0.01)
            else:
                try:
                    proc.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    proc.kill()
                    proc.wait()
        finally:
            if proc.returncode is None:
                proc.kill()
                proc.wait()
    output_text = logs.with_suffix(".stdout").read_text()
    fields = dict(line.split("=", 1) for line in output_text.splitlines() if "=" in line)
    after = file_hash(source)
    return {"exit_code": proc.returncode, "timed_out": timed_out, "peak_rss_bytes": peak,
            "wall_ms": (time.monotonic() - start) * 1000, "fields": fields,
            "error": logs.with_suffix(".stderr").read_text().strip(),
            "original_unchanged": before == after, "source_sha256": before}


def refused(run, output):
    return (run["exit_code"] == 2 and not run["timed_out"] and not output.exists()
            and run["original_unchanged"] and run["fields"].get("status") == "error")


def main():
    # Core
    from PIL import AvifImagePlugin, Image, ImageOps, features
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new directory for generated evidence")
    parser.add_argument("--binary", type=Path,
                        default=Path(__file__).parent / "target/release/fileforge-avif-probe")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not features.check("avif"):
        parser.error("the existing Pillow installation must include AVIF support")
    reference_versions = oracle_versions()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixture_dir = root / "fixtures"
    fixture_dir.mkdir()
    logs = root / "logs"
    logs.mkdir()
    report = {"platform": platform.platform(), "python": platform.python_version(),
              "pillow": Image.__version__, "libavif": features.version("avif"),
              "plane_oracle": reference_versions,
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "thresholds": {"rgb_mean": MAX_MEAN_RGB_ERROR, "rgb_max": MAX_RGB_ERROR,
                             "alpha_max": 0, "cancel_latency_ms": MAX_CANCEL_LATENCY_MS},
              "cases": [], "limitations": [
                  "Synthetic fixtures do not establish complete AV1 conformance.",
                  "Peak RSS includes process baseline; it is not live-allocation accounting.",
                  "The decoder's allocation budget does not cover every scratch allocation.",
                  "Cancellation is observed between decoder/color calls, not inside the codec.",
              ]}
    # Use one reference thread and a named independent AV1 codec for reproducibility.
    AvifImagePlugin.MAX_THREADS = 1
    specs = [("rgb-444", 97, 65, False, "4:4:4", "full", 1),
             ("rgb-420", 97, 65, False, "4:2:0", "full", 1),
             ("rgb-422", 97, 65, False, "4:2:2", "full", 1),
             ("rgb-limited", 97, 65, False, "4:4:4", "limited", 1),
             ("rgba-444", 97, 65, True, "4:4:4", "full", 1),
             ("rgba-420", 97, 65, True, "4:2:0", "full", 1)]
    specs += [(f"orientation-{o}", 97, 65, False, "4:4:4", "full", o) for o in range(2, 9)]
    specs += [("gray", 97, 65, False, "4:0:0", "full", 1),
              ("premultiplied-alpha", 97, 65, True, "4:4:4", "full", 1)]
    specs += [("rgb-420-limited", 97, 65, False, "4:2:0", "limited", 1),
              ("rgb-422-limited", 97, 65, False, "4:2:2", "limited", 1),
              ("rgba-422", 97, 65, True, "4:2:2", "full", 1),
              ("premultiplied-alpha-420", 97, 65, True, "4:2:0", "full", 1),
              ("premultiplied-alpha-422", 97, 65, True, "4:2:2", "full", 1),
              ("premultiplied-alpha-limited", 97, 65, True, "4:4:4", "limited", 1),
              ("premultiplied-alpha-low", 97, 65, True, "4:4:4", "full", 1),
              ("premultiplied-alpha-low-420", 97, 65, True, "4:2:0", "full", 1),
              ("premultiplied-alpha-low-422", 97, 65, True, "4:2:2", "full", 1),
              ("premultiplied-alpha-low-limited", 97, 65, True, "4:4:4", "limited", 1),
              ("gray-alpha", 97, 65, True, "4:0:0", "full", 1),
              ("premultiplied-alpha-gray", 97, 65, True, "4:0:0", "full", 1),
              ("premultiplied-alpha-low-gray", 97, 65, True, "4:0:0", "full", 1)]
    for width, height in [(1, 1), (1, 7), (7, 1), (2, 2), (96, 64)]:
        for subsampling in ["4:4:4", "4:2:2", "4:2:0"]:
            specs.append((f"edge-{width}x{height}-{subsampling.replace(':', '')}",
                          width, height, False, subsampling, "full", 1))
    specs += [("medium", 1024, 768, False, "4:4:4", "full", 1),
              ("large", 2048, 1536, False, "4:4:4", "full", 1)]

    def record(name, kind, result, passed):
        report["cases"].append({"name": name, "kind": kind, **result, "passed": passed})
        detail = result.get("error", "")
        if result.get("timed_out"):
            detail = "20-second deadline exceeded"
        elif not passed and "rgb_mean_error" in result:
            detail = f'RGB mean {result["rgb_mean_error"]:.3f}, max {result["rgb_max_error"]}; alpha max {result["alpha_max_error"]}'
        print(f'{"PASS" if passed else "FAIL"} {name}: {detail}', flush=True)

    for name, width, height, alpha, subsampling, range_, orientation in specs:
        source = fixture_dir / f"{name}.avif"
        owned = pattern(width, height, alpha)
        if name.startswith("premultiplied-alpha-low"):
            owned.putalpha(Image.frombytes("L", (width, height),
                                          bytes((0, 1, 2, 8, 16, 64, 128, 254, 255)[i % 9]
                                                for i in range(width * height))))
        if name == "gray":
            owned = owned.convert("L")
        exif = Image.Exif()
        exif[274] = orientation
        owned.save(source, quality=90, speed=6, subsampling=subsampling, range=range_,
                   codec="aom", max_threads=1, exif=exif, alpha_premultiplied=name.startswith("premultiplied-alpha"))
        with Image.open(source) as reference:
            oracle = ImageOps.exif_transpose(reference).convert("RGBA")
            expected = oracle.tobytes()
            expected_size = oracle.size
        output = root / f"{name}.rgba"
        diagnostic = width * height <= 65536
        run = execute(binary, source, output, logs / name, ("--diagnostic-planes",) if diagnostic else ())
        good = run["exit_code"] == 0 and run["original_unchanged"]
        if good:
            actual_size = (int(run["fields"]["width"]), int(run["fields"]["height"]))
            run["expected_size"] = expected_size
            run["actual_size"] = actual_size
            good = actual_size == expected_size
            if good:
                metrics = pixel_metrics(output.read_bytes(), expected)
                run.update(metrics)
                good = (metrics["rgb_mean_error"] <= MAX_MEAN_RGB_ERROR
                        and metrics["rgb_max_error"] <= MAX_RGB_ERROR and metrics["alpha_max_error"] == 0)
        record(name, "pixels", run, good)
        if diagnostic:
            reference = decode_planes(source.read_bytes())
            equal = compare_planes(run["fields"], reference)
            record(name + "-planes", "native-planes", {"exact_match": equal, "source_sha256": run["source_sha256"]}, equal)

    animation = fixture_dir / "animated.avif"
    frame = pattern(32, 24)
    frame.save(animation, save_all=True, append_images=[ImageOps.mirror(frame)], duration=[100, 100],
               quality=90, speed=6, codec="aom", max_threads=1)
    with Image.open(animation) as oracle:
        if oracle.n_frames != 2:
            raise RuntimeError("animation fixture must contain two frames")
    output = root / "animated.rgba"
    run = execute(binary, animation, output, logs / "animated")
    record("animated", "unsupported", run, refused(run, output))

    original = fixture_dir / "rgb-444.avif"
    encoded = original.read_bytes()
    mutated = {"empty": b"", "not-avif": b"not an AVIF file", "truncated-header": encoded[:12],
               "truncated-payload": encoded[:-8], "oversized-box": b"\xff" * 4 + encoded[4:]}
    # ispe is a FullBox followed by width/height. These tests intentionally disagree with AV1 geometry.
    pos = encoded.find(b"ispe")
    if pos < 0:
        raise RuntimeError("reference encoder emitted no ispe property")
    huge = bytearray(encoded)
    huge[pos + 8:pos + 16] = (0xFFFFFFFF).to_bytes(4, "big") * 2
    mutated["huge-container-dimensions"] = bytes(huge)
    tiny = bytearray(encoded)
    tiny[pos + 8:pos + 16] = (1).to_bytes(4, "big") * 2
    mutated["inconsistent-dimensions"] = bytes(tiny)
    for name, data in mutated.items():
        source = fixture_dir / f"{name}.avif"
        source.write_bytes(data)
        output = root / f"{name}.rgba"
        run = execute(binary, source, output, logs / name, ("--max-pixels", "1048576"))
        record(name, "malformed", run, refused(run, output))

    for name, extra in [("pixel-limit", ("--max-pixels", "100")),
                        ("allocation-limit", ("--allocation-budget", "1")),
                        ("cancel-before-start", ("--cancel-after-ms", "0"))]:
        output = root / f"{name}.rgba"
        run = execute(binary, original, output, logs / name, extra)
        record(name, "limits", run, refused(run, output))

    oversized = fixture_dir / "oversized-input.avif"
    with oversized.open("xb") as sparse:
        sparse.truncate(256 * MIB + 1)
    output = root / "oversized-input.rgba"
    run = execute(binary, oversized, output, logs / "oversized-input")
    record("oversized-input", "limits", run, refused(run, output))

    output = root / "cancel-during-decode.rgba"
    run = execute(binary, fixture_dir / "medium.avif", output, logs / "cancel-during-decode",
                  ("--cancel-after-ms", "5"))
    total = run["fields"].get("total_ms")
    run["cancel_latency_ms"] = max(0.0, float(total) - 5) if total is not None else None
    record("cancel-during-decode", "cancellation", run,
           refused(run, output) and run["error"] == "cancelled"
           and run["cancel_latency_ms"] is not None and run["cancel_latency_ms"] <= MAX_CANCEL_LATENCY_MS)

    # Existing destination and original-path writes must fail even after a valid decode.
    output = root / "existing.rgba"
    output.write_bytes(b"keep this output")
    run = execute(binary, original, output, logs / "existing-output")
    record("existing-output", "output-safety", run,
           run["exit_code"] == 2 and output.read_bytes() == b"keep this output" and run["original_unchanged"])
    run = execute(binary, original, original, logs / "original-output")
    record("original-output", "output-safety", run, run["exit_code"] == 2 and run["original_unchanged"])

    # Deterministic small corruptions: a mutation may remain valid; any panic/crash/timeout is a failure.
    rng = random.Random(0xA71F)
    for index in range(24):
        data = bytearray(encoded)
        position = rng.randrange(len(data))
        data[position] ^= 1 << rng.randrange(8)
        name = f"mutation-{index:02d}"
        source = fixture_dir / f"{name}.avif"
        source.write_bytes(data)
        output = root / f"{name}.rgba"
        run = execute(binary, source, output, logs / name, ("--max-pixels", "1048576"))
        record(name, "mutation-smoke", run,
               run["exit_code"] in (0, 2) and not run["timed_out"] and run["original_unchanged"])

    report["passed"] = all(case["passed"] for case in report["cases"])
    report["passed_count"] = sum(case["passed"] for case in report["cases"])
    report["case_count"] = len(report["cases"])
    (root / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(f'{report["passed_count"]}/{report["case_count"]} acceptance checks passed; {root / "report.json"}')
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
