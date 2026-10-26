#!/usr/bin/env python3
"""Prepare an offline, checksum-verified AVIF merge experiment in a new directory."""
# Core
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tomllib

HERE = Path(__file__).resolve().parent
PROBE = HERE.parent
VERSION = "0.1.2"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_pin(lock):
    matches = [package for package in lock["package"] if package["name"] == "avif-rust"]
    if len(matches) != 1 or matches[0]["version"] != VERSION or "checksum" not in matches[0]:
        raise ValueError("experiment requires the registry pin avif-rust 0.1.2")
    return matches[0]["checksum"]


def code_member(name):
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or len(path.parts) < 2 or path.parts[0] != f"avif-rust-{VERSION}":
        raise ValueError("unexpected source archive path")
    relative = Path(*path.parts[1:])
    if relative.parts[0] == "src" or str(relative) in ("Cargo.toml", "LICENSE"):
        return relative
    return None


def validate_lock(baseline, patched):
    expected = json.loads(json.dumps(baseline))
    for package in expected["package"]:
        if package["name"] == "avif-rust":
            del package["source"]
            del package["checksum"]
    if expected != patched:
        raise ValueError("the experiment changed dependencies beyond the approved local source patch")


def prepare(archive, output):
    original_lock = tomllib.loads((PROBE / "Cargo.lock").read_text())
    checksum = source_pin(original_lock)
    if archive.stat().st_size > 16 * 1024 * 1024 or digest(archive) != checksum:
        raise ValueError("source archive does not match the pinned checksum")
    output.mkdir(parents=True, exist_ok=False)
    codec = output / "codec"
    extracted = 0
    with tarfile.open(archive) as source:
        members = source.getmembers()
        if len(members) > 4096:
            raise ValueError("source archive has too many entries")
        for member in members:
            if member.isdir():
                continue
            relative = code_member(member.name)
            if relative is None:
                continue
            extracted += member.size
            if not member.isfile() or extracted > 32 * 1024 * 1024:
                raise ValueError("unsupported source archive entry")
            target = codec / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as stream, target.open("xb") as destination:
                shutil.copyfileobj(stream, destination)
    subprocess.run(["git", "apply", "--check", str(HERE / "avifMerge.patch")], cwd=codec, check=True)
    subprocess.run(["git", "apply", str(HERE / "avifMerge.patch")], cwd=codec, check=True)
    helper = codec / "src/av1/tile_decode/post_filter_state/indexed_merge.rs"
    helper.parent.mkdir(exist_ok=True)
    shutil.copy2(HERE / "indexedMerge.rs", helper)
    for variant in ("baseline", "patched"):
        destination = output / variant
        shutil.copytree(PROBE / "src", destination / "src")
        for filename in ("Cargo.toml", "Cargo.lock"):
            shutil.copy2(PROBE / filename, destination / filename)
    with (output / "patched/Cargo.toml").open("a") as manifest:
        manifest.write('\n[patch.crates-io]\navif-rust = { path = "../codec" }\n')
    subprocess.run(["cargo", "metadata", "--offline", "--format-version", "1", "--manifest-path",
                    str(output / "patched/Cargo.toml")], stdout=subprocess.PIPE, check=True)
    validate_lock(original_lock, tomllib.loads((output / "patched/Cargo.lock").read_text()))
    checks = output / "checks"
    checks.mkdir()
    shutil.copy2(HERE / "checks.rs", checks / "checks.rs")
    (checks / "Cargo.toml").write_text(
        '[package]\nname = "avif-merge-checks"\nversion = "0.0.0"\nedition = "2024"\n'
        'publish = false\n\n[workspace]\n\n[lib]\npath = "checks.rs"\n'
    )
    provenance = {"archive_sha256": checksum, "license": "MIT", "excluded": "all upstream test fixtures",
                  "experiment_files": {path.name: digest(path) for path in HERE.iterdir() if path.is_file()},
                  "baseline_sources": {str(path.relative_to(PROBE)): digest(path)
                                       for path in sorted((PROBE / "src").glob("*.rs"))}}
    (output / "preparation.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="already cached avif-rust-0.1.2.crate; default: Cargo cache")
    parser.add_argument("--output", required=True, type=Path, help="new scratch directory")
    arguments = parser.parse_args()
    archive = arguments.archive
    if archive is None:
        cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/cache"
        checksum = source_pin(tomllib.loads((PROBE / "Cargo.lock").read_text()))
        archive = next((path for path in sorted(cache.glob(f"*/avif-rust-{VERSION}.crate"))
                        if digest(path) == checksum), None)
        if archive is None:
            parser.error("build the ordinary probe first or pass the pinned cached --archive")
    prepare(archive.resolve(strict=True), arguments.output.resolve())
