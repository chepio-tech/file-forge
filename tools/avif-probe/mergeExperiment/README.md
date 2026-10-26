# Isolated AVIF merge experiment

This experiment patches **only a scratch copy** of MIT-licensed avif-rust 0.1.2. The ordinary probe, its pinned
lockfile and the application retain the registry decoder. See [RESULTS.md](RESULTS.md) for the measured outcome.

## Reproduce

First build the ordinary probe and generate its owned corpus as described in [the probe README](../README.md).
Preparation uses the already cached crate, verifies its checksum against the ordinary lockfile and installs
nothing. All output directories below must be new. Python 3.11 or newer is required.

```sh
python3 tools/avif-probe/mergeExperiment/prepare.py --output /tmp/fileforge-avif-merge
cargo test --manifest-path /tmp/fileforge-avif-merge/checks/Cargo.toml --offline
cargo clippy --manifest-path /tmp/fileforge-avif-merge/checks/Cargo.toml --all-targets --offline --locked -- -D warnings
cargo clippy --manifest-path /tmp/fileforge-avif-merge/patched/Cargo.toml -p avif-rust --lib --offline --locked -- -D warnings
cargo test --manifest-path /tmp/fileforge-avif-merge/patched/Cargo.toml --offline --locked
cargo build --manifest-path /tmp/fileforge-avif-merge/baseline/Cargo.toml --release --offline --locked
cargo build --manifest-path /tmp/fileforge-avif-merge/patched/Cargo.toml --release --offline --locked
python3 tools/avif-probe/mergeExperiment/compare.py \
  --baseline /tmp/fileforge-avif-merge/baseline/target/release/fileforge-avif-probe \
  --patched /tmp/fileforge-avif-merge/patched/target/release/fileforge-avif-probe \
  --corpus /tmp/fileforge-avif-evaluation --output /tmp/fileforge-avif-merge-comparison
python3 tools/avif-probe/verify.py \
  --binary /tmp/fileforge-avif-merge/patched/target/release/fileforge-avif-probe \
  --output /tmp/fileforge-avif-merge-acceptance
```

Use `--archive /path/to/avif-rust-0.1.2.crate` if the cache is outside `CARGO_HOME`. Preparation records source,
patch, helper and harness hashes, and rejects any lockfile change beyond replacing the approved codec's source.
`compare.py` validates corpus hashes, compares complete output bytes and dimensions, and alternates three
baseline/patched pairs for medium and large inputs. Its 90-second **diagnostic** deadline lets the baseline finish;
the separate acceptance runner retains its original 20-second and 250-millisecond gates. Incomplete pairs fail
comparison and are distinguished from pixel mismatches. Partial evidence is saved after each pair.

## Change and limits

`PostFilterState::merge` originally searches the growing destination linearly for every incoming record. The
patched bounded path builds a temporary index, sorts by the full key and original position, resolves each first
match, then restores the index's original order before applying updates. Destination vector order, existing
duplicates, last incoming values, and the empty-state fast path are preserved. The legacy merge is unchanged.
Keys retain plane and transform size where the original equality included them.

The index uses two `usize` values per record, with checked counts and a fallible allocation admitted through the
codec's existing ledger. One allocation is reused across the five collections and dropped before its charge is
released, including an operation error. Actual capacity is checked. The sorting primitive does not allocate
another buffer ([Rust slice documentation](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_unstable_by)).
This improves merge complexity at the cost of temporary memory; it does not repair unrelated allocation gaps,
add cancellation checkpoints or establish production readiness.

The fixture-free checks compile the actual merge helper and the pinned codec's allocation primitives and test
observer. They compare against a linear first-match oracle, exercise duplicates/order/empty inputs, and observe
real allocation refusal, excess capacity, rollback and destruction before ledger release. The unchanged upstream
observer uses unsafe code only to observe the test allocator; the new merge implementation forbids unsafe code.
No third-party image fixtures are copied or used: the upstream inventory includes separately licensed images.
The complete upstream test suite is not run by this experiment. End-to-end inputs are the probe's owned fixtures.

The original MIT notice is retained in [LICENSE.avif-rust](LICENSE.avif-rust) and in the extracted source copy.
CI prepares the scratch copy offline and runs its targeted checks and the patched probe's unit tests. Pillow
acceptance and repeated timings remain local evidence, not a cross-platform guarantee or conformance certification.
