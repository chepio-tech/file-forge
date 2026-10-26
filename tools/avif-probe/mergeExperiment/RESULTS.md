# AVIF merge experiment — 2026-10-26

**The isolated patch substantially reduces decoding time on this corpus, with higher peak memory use.**
All 45 owned images produce byte-identical RGBA and dimensions against the unchanged decoder. The separate
acceptance run passes 127/127 checks, but cancellation still waits for native decoding to finish and complete
allocation accounting remains unproven. Keep AVIF outside the application.

## Repeated comparison

Reproduce with [README.md](README.md). The run used macOS 26.6.2 ARM64, Rust 1.99.0, Python 3.14.6,
Pillow 12.3.0/libavif 1.4.2, libyuv 1924, dav1d 1.5.3 and AOM 3.14.1. The baseline binary is unchanged from
the color/alpha follow-up. The experiment changes only a scratch copy of the pinned MIT decoder; all other
dependency versions/checksums remain unchanged. Neither the application nor the ordinary probe uses this patch.

The initial five-second profile placed 3,866 of 4,008 main-thread samples (96.5%) in `PostFilterState::merge`.
The patch replaces repeated linear searches on the bounded merge path with a sorted temporary index, preserving
full equality keys, first matches, last incoming values and vector order. The legacy path remains unchanged.

| Owned fixture | Baseline median decode | Patched median decode | Ratio | Baseline maximum RSS | Patched maximum RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1024×768 | 1038.038 ms | 183.742 ms | 5.65× | 28,999,680 bytes | 33,325,056 bytes |
| 2048×1536 | 19,859.022 ms | 1076.709 ms | 18.44× | 70,057,984 bytes | 112,951,296 bytes |

Each size has three sequential baseline/patched pairs, with order alternating between pairs. Large-image
decode times were 19,447.300 / 22,107.810 / 19,859.022 ms before and 1208.684 / 1076.709 / 936.004 ms after.
Small fixtures were compared once, producing **49 successful pairs across 45 distinct images**. Every output
byte and dimension matches; every source hash is unchanged. No downloaded photographs or user files were used.

The diagnostic child deadline was 90 seconds so the baseline could complete. The ordinary acceptance deadline
remains 20 seconds. An earlier exploratory run under different load had a baseline timeout at 45 seconds;
that pair was incomplete, not a pixel mismatch. Timing varies with machine load and these measurements establish
neither a cross-platform SLO nor a bound for every valid image. Peak RSS includes runtime baseline and allocator
retention; it measures the observed process peak, not live ownership or the exact size of the index.

## Acceptance and cancellation

The unchanged acceptance runner reports **127/127 passes** for the patched binary:

- All 45 pixel cases pass, with maximum RGB error 1/255 and exact alpha against Pillow/libavif.
- All 43 small native-plane cases match the reference byte-for-byte.
- The large image completes its decode in 995.884 ms and passes the 20-second gate.
- The existing medium-image cancellation check reports 190.077 ms response delay, under its 250 ms target.
- Malformed input, limits, output protection, animation refusal and 24 seeded mutations pass.

Still-image and malformed fixture hashes match the color/alpha corpus. The regenerated animation differs in
encoded bytes but is again verified as two frames and refused. No threshold or check was relaxed.

**Cancellation was not implemented inside the decoder.** A separate cancellation request after 5 ms on the
2048×1536 image is observed only after 1053.301 ms of decoding, with estimated response delay **1050.954 ms**.
It returns `cancelled` without an output and preserves the input. The medium gate now passes because that image
finishes quickly; the larger diagnostic still exceeds the same 250 ms target. This is not prompt cancellation.

The index is fallibly allocated and charged at actual capacity through the native ledger. Its storage is dropped
before releasing that charge. The decoder's pre-existing accounting gaps and the probe's combined live-buffer
bound are unchanged. Observed large-image RSS increases by about 43 MB, so the speed improvement has a memory cost.

## Verification and provenance

Eight fixture-free Rust checks pass, including 600 deterministic differential cases, repeated scratch reuse
without extra allocation, size overflow, budget refusal before allocation, actual allocator failure, excess
capacity rollback, and destruction before release on success/error. The checks compile the real helper and the
pinned allocation primitives/observer. No separately licensed upstream image fixtures are copied; the complete
upstream fixture suite was not run.

The patched probe's 17 Rust tests and all 11 Python harness tests pass, along with warning-free Clippy for the
patched library and checks, formatting, both release builds and 20 workflow/security checks. CI includes the
offline preparation, helper checks and patched-probe tests. Full application suites were not rerun locally for
this isolated experiment.

- Source archive SHA-256: `fe0b5ee8b4698179b3bbb8b0fd86930846c9eb932451035f02ec2e723b1a4086`.
- Baseline binary SHA-256: `1d242219f44e1ffcee1f55d83a23b291b3c212fb187ca850e3ef61563edeb183`.
- Patched binary SHA-256: `97ef644a7d73bd2498bcb5738d97dc3509b4105cc3ad388e53c988dc098fe670`.

Preparation records source and experiment file hashes; the comparison and acceptance reports record binary/input
hashes, per-child logs, times and RSS. Before integration, resolve cancellation and complete memory bounds, assess
the additional memory cost on broader inputs/platforms, and validate the remaining metadata/provenance policy.
