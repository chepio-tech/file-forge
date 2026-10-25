# ADR-0024: Own temporary results per app instance

## Status
Accepted

## Date
2026-10-25

## Context
Each app instance had independent registry ids but used the same temporary directory. Starting another copy
deleted existing results; matching ids could replace their contents; exiting either copy removed the other's
working directory. This was reproduced during v0.4.0 verification and again with the actual ResultStore.
The owner prioritized instance isolation and safe crash cleanup before the next release.

## Decision
Use `result-sessions-v1` under the app cache directory. Each ResultStore exclusively creates a `session-*`
directory and a sibling `.lock` file. It holds an exclusive standard-library file lock for the session's
lifetime. Names combine process id, time and a counter only for uniqueness; they do not establish liveness.

A persistent `.coordinator.lock` serializes session creation and cleanup. Acquisition uses nonblocking lock
attempts with a five-second contention deadline. The guard explicitly unlocks on drop so a descriptor briefly
inherited by a concurrent subprocess cannot unnecessarily prolong the critical section. The coordinator file
is never removed while the cache remains in use.

Startup examines at most 4,096 cache entries. It removes only recognized session directories whose lease can
be acquired, or whose creator died before creating a lease. Holding the coordinator makes incomplete creation
distinguishable from a live creator. Locked, inaccessible, linked or unrecognized entries are preserved.
Abandoned sibling lease files can also be removed. Handles close before deletion for Windows compatibility.

Normal exit, the updater's direct Windows exit and ordinary store destruction clean only the owning session.
Closing a store serializes with publication and rejects subsequent writes. Cleanup failures retain files for
a future startup. Setup and normal exit wait for filesystem work on a blocking worker. Original inputs and
saved copies remain outside this lifecycle.

The former `results` directory is not migrated or deleted: an older running release may still own it. Older
releases cannot reach the new namespace through their legacy recursive cleanup.

## Rationale
OS locks report actual ownership across processes and release after abnormal termination. PID existence, PID
reuse and elapsed-time guesses cannot reliably prove that a session is abandoned. All supported platforms have
the required safe standard-library API within the project's minimum Rust version; no dependency is added.

## Alternatives considered
- A single-instance plugin would change launch behavior, add a dependency and still need crash cleanup.
- Unique directories alone prevent collisions but leave results after every abnormal exit.
- Deleting directories by age or process id can remove a slow or otherwise live instance's results.
- Reusing the former root leaves new copies vulnerable to cleanup performed by an older version.

## Consequences
Multiple copies can keep and save independent results. Abandoned results are discarded, not restored into a
new UI session. Collection is best effort and bounded; it is not a guarantee that all old cache files disappear
on the next startup. Legacy leftovers may remain until cleaned by a legacy copy or manually after all copies exit.
If coordination is unavailable, opening a new session fails instead of bypassing ownership checks.

The cache must support local filesystem locks. This protocol coordinates cooperating app instances; it is not
a security boundary against malicious software running as the same OS user. No cross-process processing queue,
database, service or recovery UI is needed now.

## Validation / fitness criteria
- Actual ResultStore tests cover identical ids in simultaneous instances, continued writes after another exit,
  preservation of saved copies, publication racing with clear and refusal of late writes.
- Separate test processes verify live ownership, forced termination, abandoned initialization and normal exit.
- Invalid/linked leases, linked directories, foreign entries and collection/lock bounds are tested.
- Updater tests retain another instance's unsaved results and consent checks.
- `result-sessions` CI runs the standard-library-only session tests on Linux, macOS and Windows with Rust 1.92.0.

## Reconsider when
The application needs result recovery across restarts, coordination across user accounts, a shared/network
cache, or data showing that the bounded startup collection leaves problematic amounts of temporary data.

## References
- [Rust File locking](https://doc.rust-lang.org/stable/std/fs/struct.File.html#method.try_lock)
- ADR-0003: originals stay untouched; ADR-0006: complete output publication; ADR-0013: update consent and exit.
