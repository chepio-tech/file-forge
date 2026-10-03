# ADR-0006: Protect session inputs and publish complete results without batch overwrites

## Status
Accepted

## Date
2026-10-03

## Context
ADR-0003 promises that originals are never modified and batch saves never overwrite existing files. A native
save dialog still lets a user choose an input path. Checking whether a batch name exists before renaming also
leaves a race: another save can create the same path between those operations. Predictable staging paths can
overwrite an unrelated file before publication.

## Decision
- Remember canonical input paths for the full session and refuse them as individual save destinations, including
  symbolic aliases and inputs removed from the list.
- Exclusively create a distinct staging file in the destination directory, write and flush its contents, close
  the handle, then publish. Clean staging files up on success, failure and unwinding.
- Individual saves rename the complete staging file to the user-selected destination after checking input paths.
- Batch saves create a hard link to the complete staging file and retry numbered output names on `AlreadyExists`.
- Serialize compression, saving and removal with the existing work slot.

## Rationale
Rust's standard library provides these operations on the supported desktop platforms. A hard link publishes
an already complete file and fails when the destination exists, so it preserves both guarantees without a new
dependency or platform-specific unsafe code.

## Alternatives considered
- Check existence then rename: can overwrite a competing output.
- Exclusively create the destination and stream directly into it: does not publish the complete file atomically.
- Add platform-specific no-replace rename APIs: more dependencies/unsafe code for the first version.

## Consequences
- Original protection also applies after removing an input from the list; choose another name to save a result.
- Batch destinations must support hard links (APFS, NTFS, ext4 and similar). On unsupported filesystems, saving
  returns `io`; individual saves remain available. No unsafe overwrite fallback is used.
- Save dialogs hold the work slot; compression/removal waits until saving finishes or is cancelled.
- This protects the app's own save operations; it does not defend against a separate hostile process changing
  filesystem paths between a destination check and publication.

## Validation / fitness criteria
`cargo test -p fileforge --locked`: original/alias protection, concurrent batch saves, failure cleanup and unchanged
existing outputs. `pnpm vitest run src/features/PdfCompress`: pending-save controls, cancellation, safe-save retry
and reveal failure feedback.

## Reconsider when
Users need atomic batch saves on filesystems without hard-link support, or the threat model includes hostile local
processes mutating filesystem paths concurrently.

## References
- ADR-0003: `docs/decisions/ADR-0003-compression-presets.md`
- Rust rename: https://doc.rust-lang.org/std/fs/fn.rename.html
- Rust hard links: https://doc.rust-lang.org/std/fs/fn.hard_link.html
