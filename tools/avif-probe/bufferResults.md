# AVIF adapter buffer follow-up — 2026-10-26

The standalone probe now accounts for input, RGBA and rotation vector capacities in one shared adapter budget
and releases compressed bytes/native planes earlier. **AVIF remains outside the application.** The decoder's
separate, incomplete ledger is unchanged; this is not a total process-memory bound.

## Ownership and refusal

The CLI allocation option supplies two separate ceilings: the existing native decoder ledger and the new
adapter ledger. Each is capped at 512 MiB. Adapter admission checks requested capacity before a fallible reserve
and actual returned capacity before accepting the owner. Old/new vectors remain charged together during input
growth or rotation. Growth uses a separate allocation/copy, so no unaccounted in-place reallocation occurs.
The vector is destroyed before its charge on ordinary return, error and unwind. Public mutation exposes only
a slice; appending cannot exceed admitted capacity.

The input is dropped after the last `prem` reference check, before RGBA allocation. Native planes are dropped
after conversion/optional diagnostics, before orientation. The 256 MiB input byte limit still applies when
metadata is stale or a file grows; an 8 KiB stack buffer reads at most one sentinel byte beyond that limit.
A growing file may be refused below 256 MiB when the combined old/new capacities exceed the adapter budget.

An allocator can return excess capacity before the post-allocation check rejects it. Allocator overhead, fixed
read scratch, metadata, runtime, native planes and untracked decoder internals are outside the adapter count.
Freed allocation ownership does not imply that the allocator returns pages to the OS. No host-exhaustion test,
OS memory cap or production-readiness claim is made.

## Verification

The ordinary probe and the regenerated accelerated scratch probe each pass **29 Rust tests**, strict Clippy
and formatting with the project's `rustfmt.toml`. The unchanged merge helper passes its 8 checks; all 11 Python harness tests pass. New tests
cover exact/one-under shared limits, actual capacity, allocation errors/overflow/unwind, destruction before
charge release, failed growth preserving the existing owner, stale input hints, bounded growth, interrupted
reads, I/O failure recovery, RGBA admission and overlapping repeated rotations. No dependency, application
source, IPC or release-version change is part of this follow-up.

Using the existing owned corpus and unchanged comparison runner, **49 comparisons across 45 images are
byte-identical** between the previous accelerated probe and the new accelerated probe. Every successful new
run reports zero live adapter capacity after dropping output. Observed peaks are 3,145,728 bytes for the
1024×768 image and 12,582,912 bytes for 2048×1536, equal to one RGBA vector. A 97×65 quarter-turn peaks at
50,440 bytes (two vectors); its mirrored counterpart peaks at 25,220 bytes (one vector).

Full acceptance retains its original thresholds and returns nonzero on failure:

| Variant | Gates | Remaining failures |
| --- | ---: | --- |
| Ordinary registry decoder | 125/127 | Large decode exceeds 20 s; single-process cancellation 1,619.765 ms |
| Accelerated scratch decoder | 126/127 | Single-process cancellation 534.023 ms exceeds 250 ms |

All completed pixel/alpha, native-plane, malformed-input, output-protection and mutation checks pass. The
one-byte allocation-limit case now refuses at adapter input admission, before the native call. A targeted
three-pair cancellation repeat also fails the 250 ms gate for both accelerated binaries: previous
285.954/397.998/402.959 ms, current 315.729/411.035/403.110 ms. These checkpoint-only measurements retain the
known absence of decoder interruption; earlier passing medium-image timing was not a cancellation guarantee.
No gate was relaxed or rerun result substituted for the recorded full acceptance result.

The unchanged process supervisor with the new workers passes **73/73 checks**, including 49 exact direct/
supervised output comparisons, 20 cancellation attempts and recovery after cancellation/refusal. Maximum
cancel-to-cleanup latency is 5.415 ms with the accelerated worker and 6.488 ms with the ordinary worker.
This local result is not a cross-platform production SLO or an OS sandbox guarantee.

## Observed RSS and time

Three alternating old/new accelerated pairs per size, on macOS 26.6.2 ARM64 with Rust 1.99.0:

| Fixture | Previous maximum RSS | Current maximum RSS | Previous/current median decode |
| --- | ---: | ---: | ---: |
| 1024×768 | 32,161,792 bytes | 27,852,800 bytes | 305.298 / 424.916 ms |
| 2048×1536 | 113,131,520 bytes | 111,329,280 bytes | 1,343.983 / 1,428.559 ms |

RSS is lower in this small sample, but timings and RSS vary substantially across runs on the shared host.
These measurements do not prove a performance improvement or a configured upper bound. The deterministic
evidence is allocation ownership, aggregate refusal and unchanged pixels. Complete native accounting and
production process lifecycle remain separate work.

## Reproduction and evidence

For scratch formatting, pass `-- --config-path /path/to/fileforge/rustfmt.toml` to `cargo fmt --check`;
the temporary workspace does not inherit the repository's formatter configuration.

Use the ordinary [probe commands](README.md#reproduce), then regenerate the approved scratch variant with
[merge experiment preparation](mergeExperiment/README.md#reproduce). Compare a retained previous accelerated
binary against the newly prepared accelerated binary using the same `compare.py` and corpus. Run `verify.py`
separately for ordinary and accelerated probes, preserving failed acceptance reports. Use the unchanged
[process measurement commands](../avif-process-probe/README.md) with the new workers for compatibility checks.
Inputs remain owned synthetic fixtures; no private files or third-party image datasets are used.

| Evidence | SHA-256 |
| --- | --- |
| Previous accelerated binary | `97ef644a7d73bd2498bcb5738d97dc3509b4105cc3ad388e53c988dc098fe670` |
| Current accelerated binary | `f3696c507a39d3eddae4dd25d5fed2e83ba6b5a9a45ad05e01f209727633b1a9` |
| Current ordinary binary | `fe6a01d93a67bde8bee384816a0481fcc750a467b9d0ece81a3153a34071c091` |
| Old/new byte-comparison report | `cf0c065115f3c9753f328c25f0573e78dd8ad558824e9a168877b1e4d2304e5a` |
| Accelerated acceptance report | `50b42c5d3385077a2269abca4edbb0d880f7aea5cd2721091c1b6c55c993c0d6` |
| Ordinary acceptance report | `ea3e345e217990cea25f34c6c3ebb508ff9778a137588269dd1346ed6ded9db3` |
| Process compatibility report | `0d37399f5441ad182a3055c5d69f7da42fae9220c07c949b62a2e3cac4d9f049` |
| Targeted cancellation report | `259c234bba7bf2be5538883818a78ba245c8185a697b0a59f08f83aa9fa67ddf` |

Hashes identify local evidence, not reproducible cross-platform executable hashes. The pinned decoder remains
MIT-licensed avif-rust 0.1.2; the three probe dependency pins and existing scratch merge patch are unchanged.
