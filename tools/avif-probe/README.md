# AVIF decoder evaluation

This standalone tool evaluates `avif-rust` before considering it for File Forge. It is a separate Cargo
workspace: the desktop app does not link these dependencies or accept AVIF input. The probe is an evaluation
harness, not a production decoder boundary. The current candidate does **not** pass the integration gates.

## Reproduce

```sh
cargo build --manifest-path tools/avif-probe/Cargo.toml --release --locked
cargo test --manifest-path tools/avif-probe/Cargo.toml --locked
cargo clippy --manifest-path tools/avif-probe/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path tools/avif-probe/Cargo.toml --check
python3 -m unittest discover -s tools/avif-probe -p 'test_*.py'
python3 tools/avif-probe/verify.py --output /tmp/fileforge-avif-evaluation
```

The last command needs an existing Pillow installation with AVIF and the `aom` encoder. The plane oracle requires
libavif 1.4.2's public functions to be exported by Pillow's native extension, as in the measured Pillow 12.3.0
installation. It uses Python's standard-library `ctypes` with opaque handles, only on small owned fixtures;
malformed cases and user files never enter that oracle. It installs nothing. Use a **new** output directory; it creates owned synthetic
fixtures, decoded RGBA files, process logs and `report.json` there. No downloaded photographs or private files
are used. CI runs the Rust and standard-library Python tests; the Pillow comparison is a separate local gate.

`verify.py` exits **1** if any candidate acceptance check fails. A failure must not be ignored when deciding
whether to integrate the codec. Unit tests validate the harness and can pass while the candidate fails.

## Direct probe

```sh
tools/avif-probe/target/release/fileforge-avif-probe input.avif /tmp/new-output.rgba \
  --max-pixels 32000000 --allocation-budget 536870912 --cancel-after-ms 5
```

Omit `--cancel-after-ms` for an ordinary decode. Output is tightly packed RGBA8, with width/height and timings
on stdout as `key=value`. Existing outputs and aliases of the input are never overwritten. Exit status 2 is a
reported error/refusal, 3 is a caught decoder panic. The comparison runner also records crashes and kills its
own child after a 20-second deadline. It processes fixtures sequentially.

The tool uses only `decode_frame_bytes_strict_with_limits`; it never falls back to an unbounded decoder.
It checks container/frame dimensions and reads the primary item's `prem` reference from bounded `meta`/`iref`
boxes. The target must be the single alpha item linked back to the primary image; duplicate, contradictory,
truncated and excessive references are refused. This corrects the pinned decoder's missing reference projection
without patching or replacing that dependency.

The probe reconstructs 8-bit sRGB from validated native planes, including monochrome alpha. It accepts explicit
sRGB primaries/transfer, BT.601 or BT.709 matrix coefficients, full/limited range and centered 4:2:0/4:2:2 or
4:4:4 samples. It refuses ICC profiles, unknown color properties, conflicting container/bitstream signaling,
other transfer functions/matrices, higher precision and untested explicit chroma siting.
Centered interpolation, horizontal endpoint extension and fixed-point RGB quantization follow the libyuv policy
used by the measured Pillow/libavif reference. Premultiplication is undone after RGB byte quantization, which
matters at very low alpha. This is an explicit comparison policy, not proof of a unique AVIF rendering.

Rotation/mirror properties are then applied in association order. Mirror axes follow
ISO/IEC 23008-12:2022: 0 exchanges top/bottom, 1 left/right. Clean aperture cropping
is explicitly refused. The native API returns untransformed pixels for ordinary still images, so the adapter
must apply these properties. This does not implement File Forge's complete provenance/HDR/metadata policy.

## Bounds and measurement

- Input: regular file, at most 256 MiB, including files that grow during reading.
- Geometry: at most 32 million pixels; optional CLI limits can only tighten this ceiling.
- Native API: bounded item/property counts, metadata, ICC, derivation depth and one frame; configurable
  allocation accounting capped at 512 MiB. This is **not** a total process-memory limit.
- The decoder documents incomplete accounting for deep tile/entropy, reference-frame and filter scratch.
  RGBA conversion and the probe's transform buffer are outside its native allocation ledger. The new RGBA
  allocation independently checks geometry/byte count against the CLI ceiling and uses fallible reservation;
  this does not bound combined live memory.
- Cancellation: a timer sets a flag, checked before/after decode and color conversion. The codec exposes no
  cancellation callback in this API. Discarding a late result does not interrupt the decoding work.
- On macOS/Linux, `wait4` records each child's peak RSS, including its runtime baseline. It does not measure
  live allocation ownership or prove an upper bound. On other systems the runner leaves peak RSS unavailable.

The pixel gate requires identical dimensions/alpha, mean visible-channel RGB error ≤1 and maximum ≤8 against
Pillow/libavif. Fully transparent RGB is excluded; alpha errors are always checked. Subsampled chroma can use
different reconstruction filters, so a mismatch is an integration-quality finding, not by itself proof that
AV1 source planes are decoded incorrectly. The exploratory cancellation target is 250 ms after the request;
it is not an established desktop-app SLO. Synthetic cases and 24 seeded mutations are not a conformance corpus
or a security proof. High-bit-depth/HDR, arbitrary ICC profiles and full grid support remain outside this probe.

`--diagnostic-planes` adds native plane dimensions and hexadecimal samples to stdout for images of at most
65,536 pixels. The comparison runner uses this only for small generated fixtures and requires exact geometry,
plane set and byte equality with libavif before drawing conclusions about color reconstruction. Diagnostic
logs contain pixels; use this option only for non-private fixtures. Reports retain all original 56 gates, add
color/alpha/edge cases and native-plane comparisons, and record libyuv/codec versions as well as input/binary hashes.

## Results

See [RESULTS.md](RESULTS.md) for the measured gates and the integration decision.

## Dependency review

The independent lockfile contains exactly three third-party packages, all MIT: `avif-rust` 0.1.2,
`avif-image-buffer` 0.1.0 and `bin-rs` 0.0.10. Encoding is disabled. Checksums are pinned in `Cargo.lock`.
The probe forbids unsafe code. The production `avif-rust` library also compiles with `--forbid unsafe_code`;
its allocation observer uses unsafe code only in its own tests. `bin-rs` includes a redundant unsafe block in
an integer-reading helper, with no unsafe operation inside. This is not a claim that the whole dependency
chain was compiled with that lint or formally audited.

Sources: [published avif-rust limits](https://docs.rs/crate/avif-rust/0.1.2/source/src/limits.rs),
[libavif transform definitions](https://github.com/AOMediaCodec/libavif/blob/v1.4.2/include/avif/avif.h).
