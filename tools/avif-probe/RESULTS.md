# AVIF evaluation results — 2026-10-25

**Decision: keep avif-rust 0.1.2 outside the application.** The color/alpha follow-up passes **125 of 127**
acceptance checks. All completed pixel comparisons pass; large-image time and cancellation still fail, and a
complete live-memory bound remains unproven. Application behavior and the three pinned MIT dependencies are
unchanged. HEIC remains deferred for separate licensing review.

## Scope and method

Reproduce with [README.md](README.md). Measurements used macOS 26.6.2 ARM64, Rust 1.99.0, Python 3.14.6,
Pillow 12.3.0/libavif 1.4.2, libyuv 1924, dav1d 1.5.3 and AOM 3.14.1. Owned deterministic gradients, sharp
edges and transparency are encoded with AOM, quality 90, speed 6 and one thread. No private or downloaded
fixtures are used. The raw-plane oracle calls the installed libavif's public opaque-handle APIs only for small
valid generated images; malformed cases run only through the safe Rust probe.

Release probe SHA-256: `1d242219f44e1ffcee1f55d83a23b291b3c212fb187ca850e3ef61563edeb183`.
Each report retains input hashes, native logs, pixel errors, reference versions and peak RSS. Timing values are
individual observations, not benchmarks or evidence of a performance change. Small-image diagnostics include
stdout plane serialization; the medium/large timing cases do not. The runner exits **1** because integration
criteria still fail. No threshold was relaxed.

All original 56 gate names remain: **54/56 now pass**, compared with **50/56 initially**. Still-image and malformed
fixture hashes match the initial run. The regenerated two-frame animation has different encoded bytes but is
again verified as two frames and refused. There are 28 additional pixel cases and 43 exact native-plane checks.

## Color and transparency

The gate requires identical geometry and alpha, mean visible RGB error at most 1 and maximum at most 8 on a
0–255 scale. Fully transparent RGB is excluded; alpha is always checked. **All 44 completed pixel comparisons
pass**, with a maximum observed RGB error of **1** and exact alpha. The 45th pixel case exceeds its deadline.

| Fixture | Mean RGB error | Maximum RGB error | Result |
| --- | ---: | ---: | --- |
| RGB 4:2:0 | 0.000 | 0 | Pass |
| RGB 4:2:2 | 0.000 | 0 | Pass |
| Straight alpha 4:2:0 | 0.000 | 0 | Pass |
| Premultiplied alpha 4:4:4 | 0.310 | 1 | Pass |
| Low alpha 4:4:4 | 0.124 | 1 | Pass |
| Low alpha 4:2:0 | 0.094 | 1 | Pass |
| Low alpha 4:2:2 | 0.115 | 1 | Pass |
| Low alpha, limited range | 0.122 | 1 | Pass |
| Monochrome with alpha | 0.000 | 0 | Pass |

Additional passing cases cover full/limited range, seven rotations/reflections, monochrome, premultiplied
4:2:0/4:2:2, near-zero alpha, and 1×1, 1×7, 7×1, 2×2 and 96×64 edges in 4:4:4/4:2:2/4:2:0.
The original 97×65 fixtures retain odd-width/height coverage. Low-alpha inputs include 0, 1, 2, 8, 16, 64,
128, 254 and 255; the lossy alpha encoder can introduce additional decoded values, which are also compared.

**All 43 small-fixture native-plane comparisons are byte-exact**, including geometry, channel set and every
Y/U/V/alpha sample. This localizes the observed discrepancies to container interpretation and RGB reconstruction
for this corpus; it does not establish general AV1 conformance.

The probe now:

- Reads `prem` from the primary image's item references, validates its target against the single alpha item and
  its `auxl` relationship, and rejects malformed/duplicate/contradictory references. The pinned decoder's
  property-based premultiplication flag remains false on these fixtures; the dependency was not patched.
- Reconstructs centered chroma with bilinear interpolation and the reference's horizontal endpoint extension,
  including the last column of odd-width images. The original candidate used nearest/vertical-only filtering.
- Uses the reference libyuv fixed-point RGB quantization before undoing premultiplication. Tests with near-zero
  alpha exposed why a fractional RGB implementation or different rounding could differ by over 100 levels.
- Handles appended monochrome alpha by its channel id rather than assuming the fourth vector entry exists.
- Checks plane layouts/sample ranges and uses checked byte counts and fallible RGBA allocation. Unsupported
  color descriptions are refused explicitly; the accepted subset is documented in README.md.

This rendering policy matches the measured reference; it is not a claim that AVIF mandates one unique chroma
filter or integer matrix. BT.709 matrix behavior has deterministic unit coverage; the generated end-to-end corpus
uses the reference encoder's BT.601 matrix. Wider color management and other chroma locations remain unvalidated.

## Time, cancellation and memory

| Case | Current observation | Result |
| --- | --- | --- |
| 1024×768 still | Decode 1368.769 ms; peak RSS 28,196,864 bytes | Pass |
| 2048×1536 still | Killed at 20 seconds; peak RSS 61,112,320 bytes | Fail |
| Cancel after 5 ms on 1024×768 | Estimated response delay 2,056.300 ms | Fail against 250 ms target |
| Native allocation budget of one byte | Refused with no output | Pass for this path |
| Input over 256 MiB / pixel cap of 100 | Refused with no output | Pass |

The codec still has no cancellation callback. Before/after checkpoints discard a late result but do not interrupt
AV1 decoding. The 250 ms criterion remains exploratory, not an established application SLO.

The native ledger excludes deep tile/entropy, reference-frame and filter scratch allocations. Probe RGBA and
transform buffers are also outside it. The new RGBA byte cap does not account for combined live buffers.
Peak RSS includes process baseline and cannot prove the configured 512 MiB limit. No host-exhaustion experiment
or allocation-failure injection was performed.

## Refusal, safety and validation

All seven original malformed cases, four limit/pre-cancel cases, animation refusal and two output-protection
checks still pass. All 24 seeded mutations complete without a panic/crash/timeout; this is smoke coverage,
not fuzzing or a security proof. Every input retains its hash. High precision, HDR, ICC color management,
clean aperture and full grids are not validated as supported output paths.

The harness passes 17 Rust and 5 Python unit tests, formatting, warning-free Clippy and the release build.
Workflow checks pass. New tests cover malformed/duplicate references, both item-id widths, extended box sizes,
truncation, reference/box count bounds, chroma edges, alpha quantization, monochrome alpha, color-signaling
refusals, malformed plane storage and allocation limits. No application dependencies, IPC, UI or release version
were changed; the application test suites were not rerun for these isolated-tool changes.

Before integration, resolve decoding/cancellation time and full allocation accounting, then broaden the corpus
and assess the rest of the application metadata/provenance policy. Color/alpha repair alone is not approval to
ship AVIF support.

## Initial baseline

The initial probe passed 50/56 checks. RGB 4:2:0 had mean/max errors 5.543/82, RGB 4:2:2 3.693/56,
straight alpha 4:2:0 5.399/82, and premultiplied alpha 4:4:4 52.707/192. The 20-second large-image gate and
250 ms cancellation gate failed. A separate initial 60-second diagnostic completed the large decode in
19,378.545 ms; it did not replace the failed gate. That historical result and current timings do not show
that decoder performance has been fixed.

## Sources

- [AVIF item reference requirements, including prem](https://aomediacodec.github.io/av1-avif/v1.2.0.html)
- [Pinned decoder frame API](https://docs.rs/crate/avif-rust/0.1.2/source/src/decoder/frame.rs)
- [Pinned decoder allocation limits](https://docs.rs/crate/avif-rust/0.1.2/source/src/limits.rs)
- [libavif 1.4.2 public plane APIs](https://github.com/AOMediaCodec/libavif/blob/v1.4.2/include/avif/avif.h)
- [libavif 1.4.2 RGB reconstruction](https://github.com/AOMediaCodec/libavif/blob/v1.4.2/src/reformat.c)
- [libyuv matrix quantization](https://chromium.googlesource.com/libyuv/libyuv/+/refs/heads/main/source/row_common.cc)
- [libyuv interpolation endpoints](https://chromium.googlesource.com/libyuv/libyuv/+/refs/heads/main/source/scale_any.cc)
