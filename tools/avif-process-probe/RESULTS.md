# AVIF decoder-process experiment — 2026-10-26

**Separate-process cancellation meets the 250 ms exploratory target on the measured owned fixture.**
All 73 process checks pass: 49 exact output comparisons across 45 images, 20 real decode cancellations,
pre-cancellation, a whole-operation deadline, decoder-limit refusal and a successful operation after failures.
The supervisor remains outside File Forge; complete memory bounds and production lifecycle design are unresolved.

## Measured cancellation

Environment: macOS 26.6.2 ARM64, Rust 1.99.0, Python 3.14.6. Reproduce with [README.md](README.md).
The input is the existing owned 2048×1536 AVIF with SHA-256
`6321df48ada66441b0b4fe5d394985ed38bc97db27ed05b1b6937a92e01b88d6`.
Both pinned decoder variants are tested five times after each scheduled delay of 5 and 100 ms, measured from
the supervisor's observation of `stage=decode`. Timing includes polling delay before issuing the kill.

| Supervised decoder | Runs passing | Median to reaped exit | Maximum to reaped exit | Median through cleanup | Maximum through cleanup |
| --- | ---: | ---: | ---: | ---: | ---: |
| Merge-patched | 10/10 | 4.875 ms | 5.466 ms | 5.267 ms | 6.491 ms |
| Unpatched | 10/10 | 4.009 ms | 7.274 ms | 4.808 ms | 7.608 ms |

Every cancellation reports an exited/waited child, no published output and completed staging cleanup. The
measurement runner independently checks that the reported child PID no longer exists. All source hashes remain
unchanged. The 100 ms whole-operation deadline returned after 111.999 ms with the child reaped and no output.
Pre-cancellation created no child; an over-limit image was refused and cleaned. A subsequent complete large
decode produced exactly the expected 12,582,912 bytes.

The earlier single-process merge experiment waited roughly 1051 ms for native decoding to finish after
cancellation. This supervisor terminates that decoding process; it does not add cooperative decoder checkpoints.
Two earlier exploratory runs also passed all 73 checks, with maximum reap/cleanup latency of 5.912/12.332 ms.
Variation between runs illustrates why these are local measurements, not a hard real-time or cross-platform SLO.

## Successful output and time

All 45 owned images have identical RGBA bytes and dimensions through direct and supervised execution of the
merge-patched probe. Medium/large inputs have three alternating pairs, for 49 complete comparisons. This checks
the supervisor's effect on output; the prior [color/alpha and codec acceptance evidence](../avif-probe/mergeExperiment/RESULTS.md)
remains separate. The ordinary 127-check decoder suite was neither changed nor relabelled as process evidence.

| Fixture | Direct median operation | Supervised median operation |
| --- | ---: | ---: |
| 1024×768 | 441.839 ms | 354.779 ms |
| 2048×1536 | 2277.988 ms | 2041.053 ms |

Direct operation time is the decoder's `total_ms`; supervised time is `elapsed_ms`, including child launch,
decoding, publication and cleanup. Neither includes its own executable's initial OS startup. Large direct times
were 2995.342/2277.988/1822.320 ms and supervised times 2041.053/2368.663/2038.983 ms. This small, variable sample
does not isolate supervision overhead. All complete within the unchanged 20-second operation limit. Peak memory
was not measured by this experiment, and no total-memory guarantee is inferred from killing the child.

## Verification and remaining work

- 17 Rust tests pass on local macOS, including publication races, external/scheduled cancellation, partial writes,
  silent startup, child failure, malformed/flooding protocol, aliases, concurrency, recovery and cleanup failure.
  One ignored entry point is explicitly spawned as the worker fixture by those tests.
- Four Python tests verify evidence acceptance, including missing/non-finite/slow latency and false reaping or
  cleanup success. Formatting, strict Clippy and release build pass. Windows cross-target Clippy is checked;
  native Linux/macOS lifecycle CI passed 17 Rust tests each and Windows passed 15 (two are Unix-only), with
  four Python tests per OS in [run 40303593339](https://github.com/chepio-tech/file-forge/actions/runs/40303593339).
- Workflow security checks pass. The application, ordinary probe, merge patch and their dependency pins are
  unchanged; the supervisor adds no third-party package.

Process separation does not restrict filesystem/network permissions or enforce the decoder's total live memory.
The trusted worker has no descendants. Forced supervisor termination, crash recovery, process trees, installation,
signing and application integration remain outside this experiment. The lifecycle tests are prepared for all
three desktop systems; local timing evidence covers only this Mac and owned corpus.

Evidence fingerprints for the final measured executables:

- Supervisor: `4f509d08c2e4dc77b4cf0aa3e33d9e3fd5f714fcc13afca2fe80f7359468dca0`.
- Merge-patched probe: `97ef644a7d73bd2498bcb5738d97dc3509b4105cc3ad388e53c988dc098fe670`.
- Unpatched probe: `1d242219f44e1ffcee1f55d83a23b291b3c212fb187ca850e3ef61563edeb183`.
- Structured result report: `f928cdee42e3da9fcdbdd4661041bf5666aee55f8591f3bed95b9536a1e2b0fc`.
