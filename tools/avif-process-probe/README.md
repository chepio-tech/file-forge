# AVIF decoder-process experiment

A separate Rust supervisor evaluates cancellation by terminating one trusted AVIF probe process and waiting for
it to exit. The supervisor uses only the standard library and forbids unsafe code. Its independent Cargo
workspace adds no dependencies to File Forge. See [RESULTS.md](RESULTS.md) for measurements and remaining limits.

## Reproduce

```sh
cargo test --manifest-path tools/avif-process-probe/Cargo.toml --offline --locked
cargo clippy --manifest-path tools/avif-process-probe/Cargo.toml --all-targets --offline --locked -- -D warnings
cargo fmt --manifest-path tools/avif-process-probe/Cargo.toml --check
cargo build --manifest-path tools/avif-process-probe/Cargo.toml --release --offline --locked
python3 -m unittest discover -s tools/avif-process-probe -p 'test_*.py'
```

First prepare the owned corpus with the [ordinary probe](../avif-probe/README.md) and build both scratch binaries
using the [merge experiment](../avif-probe/mergeExperiment/README.md). Then use a new evidence directory:

```sh
python3 tools/avif-process-probe/measure.py \
  --supervisor tools/avif-process-probe/target/release/fileforge-avif-process-probe \
  --decoder /tmp/fileforge-avif-merge/patched/target/release/fileforge-avif-probe \
  --unpatched /tmp/fileforge-avif-merge/baseline/target/release/fileforge-avif-probe \
  --corpus /tmp/fileforge-avif-evaluation --output /tmp/fileforge-avif-process
```

The measurement script uses Python 3.11+ and existing owned fixtures; it installs nothing and does not load a
native image oracle. It reuses corpus/hash/byte-comparison helpers from the ordinary experiment. It compares
direct and supervised output, alternates three timing pairs for medium/large images, and cancels each decoder
five times after each of 5 and 100 ms on the large image. A 25-second diagnostic watchdog owns a separate POSIX
session and kills that session if the supervisor fails to return. This watchdog is not part of the Rust tool;
the script is limited to macOS/Linux. Rust lifecycle tests are configured for Linux, macOS and Windows in CI.
Neither the ordinary decoder's acceptance checks nor their thresholds are modified.

## Direct use and reports

```sh
tools/avif-process-probe/target/release/fileforge-avif-process-probe \
  --decoder /tmp/fileforge-avif-merge/patched/target/release/fileforge-avif-probe \
  input.avif /tmp/new-output.rgba --cancel-after-decode-ms 5
```

Omit cancellation flags for ordinary completion. `--cancel-before-start` refuses to start a child.
`--timeout-ms` can tighten the default 20-second whole-operation deadline, including a silent startup.
`--max-pixels` and `--allocation-budget` only tighten the probe's existing ceilings of 32 million pixels and
512 MiB of accounted allocations. Input must be a regular file of at most 256 MiB; the decoder still performs
its own bounded read and validation. Pass a known built probe executable, not an arbitrary command.

Stdout is `key=value`. Status/exit codes are `ok`/0, `cancelled`/2, `deadline`/3 and `error`/4. A started child
has `child_pid`; `child_reaped=true` means its process handle was waited on, not just signalled. `published`,
`cleanup_ok`, `elapsed_ms`, successful geometry and bounded escaped error text describe the outcome. A cleanup
failure is an error even if publication already succeeded; `published=true` then describes a complete output
that is preserved. No success is reported for a partially written destination.

`cancel_reap_ms` measures cancellation to completed wait. `cancel_settle_ms` additionally includes pipe-reader
joins and staging cleanup. The scheduled cancellation clock starts when the stdout reader observes the flushed
`stage=decode` line. Latency is charged from the scheduled deadline, including the supervisor's polling delay;
this is conservative relative to the later instant when it issues the kill. The library's `Cancellation` token
also accepts an external request. A child already reaped before that request has zero remaining reap latency.
Pre-cancellation has no child or reap metric. The exploratory target is 250 ms for both measured latencies.

## Ownership and publication

One operation owns one child and two bounded pipe readers. Stdout is capped at 64 KiB total and 4096 bytes per
line; stderr at 8 KiB. Duplicate or invalid stage/status/geometry fields, invalid UTF-8, excess output and read
failures prevent publication. Unknown bounded diagnostic fields are ignored. Resource/deadline monitoring polls
every 2 ms. Cancellation or a detected failure kills the owned child, waits for exit and joins readers.
[Rust's Child documentation](https://doc.rust-lang.org/std/process/struct.Child.html) explains why dropping its
handle alone is insufficient; the ownership guard also kills and waits during ordinary unwinding.

The decoder writes inside an exclusively created temporary directory beside the destination (mode 0700 on
Unix, inherited directory permissions on Windows). Only exit success, a valid successful protocol and a regular
file of exactly `width × height × 4` bytes permit publication. A same-filesystem
[hard link](https://doc.rust-lang.org/std/fs/fn.hard_link.html) atomically creates the final name without replacing
an existing file. Unsupported hard links fail; there is no overwrite/copy fallback. Existing destinations,
input aliases and a destination created during decoding remain untouched.

Publication and accepting cancellation share a mutex: cancellation accepted first prevents publication; a
completed publication wins against a later cancellation. Cleanup removes only this operation's staging directory
after child exit. Concurrent operations use distinct directories, including when competing for the same output.
The lifecycle tests exercise partial writes, startup/decode stalls, process failure, protocol flooding, original
protection, publication races, cancellation recovery and independent operations. The ignored `worker_fixture`
test is the child entry point invoked explicitly by these tests, not an unverified skipped assertion.

## Limits

This is process supervision, not an OS filesystem/network sandbox or a total-memory limit. The trusted probe
does not spawn descendants. Process trees, OS job objects, supervisor forced termination, crash recovery,
installation/signing and application IPC remain outside this experiment. A killed supervisor can leave a child
or staging directory behind. No app input support, capabilities, packaging or release behavior changes here.

OS scheduling, process launch, filesystem operations and kill/wait calls can exceed polling intervals or the
requested deadline; the measurements are not a hard real-time guarantee. Output bounds reject excess files but
do not impose an OS disk quota. Decoder allocation accounting remains incomplete, and process separation does
not repair it. Production integration needs its own memory/lifecycle design and evidence on supported platforms.
