# AVIF evaluation results — 2026-10-25

**Decision: keep avif-rust 0.1.2 outside the application.** The isolated prototype is reproducible, but the
candidate passes only **50 of 56 acceptance checks**. Color, premultiplied alpha, decoding time and cancellation
need further work. A complete memory bound is also unproven. HEIC remains deferred for separate licensing review.

## Method

Run the commands in [README.md](README.md). The measured environment was macOS 26.6.2 on ARM64, Rust 1.99.0,
Python 3.14.6 and Pillow 12.3.0/libavif 1.4.2. The independent AOM encoder used one thread, quality 90 and speed 6.
Fixtures are generated from owned deterministic gradients, edges and alpha values; they are not downloaded data.

The final run used release binary SHA-256
`d50639dc33f920d26d639749778f789a912aa89d02fbe22aaa144a234fbff3dc`.
Every generated report records its own binary/input hashes, library versions, errors, timings and peak RSS.
Timings below are individual observations, not statistical benchmarks. The runner exits 1 for this result;
the thresholds were not relaxed to turn decoder failures into passes.

## Pixel quality

The gate requires identical dimensions and alpha, mean visible RGB channel error at most 1 and maximum error
at most 8 on a 0–255 scale, compared with Pillow/libavif decoding of the same AVIF. Fully transparent RGB is
excluded. Small fixtures are 97 × 65 pixels to exercise odd dimensions.

| Fixture | Mean RGB error | Maximum RGB error | Result |
| --- | ---: | ---: | --- |
| RGB 4:4:4, full range | 0.196 | 1 | Pass |
| RGB 4:4:4, limited range | 0.236 | 2 | Pass |
| Straight alpha, 4:4:4 | 0.199 | 1 | Pass |
| Monochrome | 0 | 0 | Pass |
| Seven rotations/reflections | 0.196 each | 1 | Pass |
| RGB 4:2:0 | 5.543 | 82 | Fail |
| RGB 4:2:2 | 3.693 | 56 | Fail |
| Straight alpha, 4:2:0 | 5.399 | 82 | Fail |
| Premultiplied alpha, 4:4:4 | 52.707 | 192 | Fail |
| RGB 4:4:4, 1024 × 768 | 0.195 | 1 | Pass |
| RGB 4:4:4, 2048 × 1536 | Not measured | Not measured | 20-second deadline exceeded |

Alpha samples matched exactly for every completed pixel comparison. Subsampled RGB differences may come from
different chroma reconstruction filters: source inspection shows simple nearest/averaged reconstruction in
this candidate. This is an inference, not evidence that the underlying AV1 planes are necessarily wrong.
A deliberate color reconstruction policy and native-plane comparison are needed before accepting these files.

The bounded decoder returns ordinary still-image pixels without applying their transforms. The probe applies
rotation and reflection in property-association order, using the current mirror-axis definitions; all seven
orientation fixtures then match. It also rejects a container/frame dimension disagreement that the native call
otherwise accepts.

For premultiplied alpha, both the returned frame and container projection incorrectly report `false` for the
generated fixture. Inspection found its `prem` reference inside `iref`, while the candidate derives the flag
from an item property. Copying the reported flag into the frame cannot recover this missing information.
The resulting semi-transparent colors are too dark; this remains a failing gate.

## Time, cancellation and memory

| Case | Observation | Result |
| --- | --- | --- |
| 1024 × 768 still | Decode 1,013.823 ms; peak RSS 28,868,608 bytes | Completes; approximately one second for 0.79 MP |
| 2048 × 1536 still | Killed at 20 seconds during decode; peak RSS 66,502,656 bytes | Fails the probe deadline |
| Cancel requested after 5 ms on 1024 × 768 | Returned at 1,015.924 ms; estimated delay 1,010.924 ms | Fails the exploratory 250 ms target |
| Native allocation budget of one byte | Refused before decode output | Pass for this allocation path |
| Input over 256 MiB / pixel cap of 100 | Refused, no output | Pass |

A separate diagnostic run allowed 60 seconds for the same large fixture and completed decoding in 19,378.545 ms
with peak RSS 70,139,904 bytes. It does not replace the failed 20-second gate or establish an infinite loop.
Its input SHA-256 was `6321df48ada66441b0b4fe5d394985ed38bc97db27ed05b1b6937a92e01b88d6`, identical to the
final run's large fixture.

The API has no cancellation callback. The probe can observe cancellation before/after decoding, but cannot
interrupt the codec itself. The 250 ms target is a feasibility criterion, not an established application SLO.

The native allocation ledger explicitly excludes some deep tile/entropy, reference-frame and filter scratch
allocations. RGBA conversion and the transform buffer also sit outside that ledger. Peak RSS includes process
baseline and measures only the sampled files; it does not prove the configured 512 MiB bound. No host-exhausting
test or allocation-failure injection was performed. A production integration needs a complete accounting
strategy or a separately approved isolation design.

## Refusal and original protection

- All seven malformed cases were refused: empty/non-AVIF input, truncated header/payload, oversized box length,
  huge container geometry and container/frame dimension mismatch.
- All four limit/pre-cancel cases passed. Animated AVIF with two frames was also refused.
- All 24 seeded bit mutations completed without a panic, crash or timeout. These smoke checks are not fuzzing
  coverage or an AV1 conformance/security proof.
- Existing destinations and the input itself were not overwritten. Every input retained its SHA-256.

High-bit-depth/HDR, arbitrary ICC color management, clean aperture, full grids and complete application
metadata/provenance policy were not validated. The bounded API's unsupported paths are not retried through an
unbounded API.

## Validation and next gate

The harness passed 7 Rust unit tests, 4 Python unit tests, formatting, Clippy with warnings denied and the
release build. The production avif-rust library separately compiled with `--forbid unsafe_code`. The independent
dependency graph contains the three approved MIT packages; the application's dependency graph is unchanged.
CI checks the harness, while the local Pillow comparison records candidate acceptance failures explicitly.

Before integration, resolve the premultiplication reference, establish acceptable subsampled-color behavior,
improve or bound decoding/cancellation time, and account for all allocations. Repeat the same gates and broaden
the corpus before introducing the application IPC/UI changes.

Source references: [native limits](https://docs.rs/crate/avif-rust/0.1.2/source/src/limits.rs),
[bounded frame decoding](https://docs.rs/crate/avif-rust/0.1.2/source/src/decoder/frame.rs),
[container metadata](https://docs.rs/crate/avif-rust/0.1.2/source/src/container.rs),
[AV1 reconstruction](https://docs.rs/crate/avif-rust/0.1.2/source/src/av1/reconstruct.rs),
[libavif transform definitions](https://github.com/AOMediaCodec/libavif/blob/v1.4.2/include/avif/avif.h).
