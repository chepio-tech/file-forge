# ADR-0011: Cooperative cancellation and channel-based progress for compressions

## Status
Accepted

## Date
2026-10-03

## Context
A compression of a large scanned PDF runs for seconds to minutes, and the UI could neither stop it nor show where it
was. Compression runs in a blocking thread inside the app process (ADR-0004). Rust cannot safely kill a thread,
and lopdf parses and saves a document in single calls with no cancellation hook. Progress has to reach the webview
without flooding the IPC: the stream pass alone can visit hundreds of thousands of objects.

## Decision
- `fileforge-core` defines a `Control` trait (`is_cancelled`, `report(Progress)`) that `compress_controlled` checks
  at fixed checkpoints: before loading, after loading, after the structure pass, before each image, after the
  image pass, before each stream, after the stream pass and before verification. A cancelled run returns
  `PdfError::Cancelled` and produces nothing. Progress is a stage plus `done`/`total` counts, monotonic within
  a stage.
- The shell keeps one cancellation generation counter. Each `compress_pdf` call takes a ticket when it starts.
  `cancel_compression` advances the counter, which cancels every compression started earlier and none started
  later, without tracking jobs.
- Progress goes to the calling webview through a Tauri `Channel` argument of `compress_pdf`, throttled in the
  shell to stage changes, stage completion and one count update per 100 ms.

## Rationale
Checkpoints keep the engine a pure function with no thread or process management. The generation counter has
no per-job state to clean up, and a late cancel cannot hit the next file. A channel is scoped to one call and
ordered. Its data fetch is exempt from the capability ACL, so the webview needs no new permission.

## Alternatives considered
- Separate worker process killed on cancel: immediate stop, even inside parsing. Costs a sidecar binary, IPC for
  1 GiB inputs, packaging and signing work. Worth it only if parse-time latency becomes a real complaint.
- Global Tauri event with file ids: needs id matching and listener lifetimes in the UI; a channel already belongs
  to one call.
- Per-job cancellation tokens in a map: same behavior with cleanup code and a race between registration and cancel.
- Overall percentage: needs guessed stage weights; counts per stage are honest.

## Consequences
- Cancel latency is bounded by the longest uninterruptible step: parsing or saving a document of up to 1 GiB, or
  up to four images already being encoded.
- If a compression finishes before it reaches a checkpoint after the cancel, its result is kept and shown.
- `compress` without control stays available for tests and tools such as `measure_pdf`.

## Validation / fitness criteria
- `crates/fileforge-core/tests/pdf_control.rs`: stage order and counts, monotonic counts, cancel before
  start/during images/during streams/while saving.
- `src-tauri/src/job_control.rs` tests: generation semantics and throttling.
- `src/features/PdfCompress/PdfCompress.test.tsx`: cancel keeps finished results and does not start later files.
- Measured cancel latency stays well under a second on 86–160 MB files (`docs/architecture/runtime.md`).

## Reconsider when
- Users report long waits after Cancel on large files: move parsing into a killable worker process.
- lopdf gains incremental parsing or saving with a cancellation hook.

## References
- https://v2.tauri.app/develop/calling-frontend/#channels
