# ADR-0007: Check filesystem identity when protecting originals on Unix

## Status
Accepted; extends ADR-0006.

## Date
2026-10-03

## Context
On the tested macOS volume, `input.pdf` and `INPUT.pdf` refer to the same file, while their canonical paths differ.
Path equality alone can therefore miss an original when the native save dialog uses different letter casing.

## Decision
On Unix, retain each registered input's device and inode identifiers for the session. Reject a destination with
the same filesystem identity before applying ADR-0006's canonical-path check. Keep both checks: path protection
also covers an externally deleted input. Windows continues to use canonical paths.

## Rationale
Device/inode identity recognizes a file independently of letter case, symbolic links and hard links. The existing
Rust standard-library metadata API provides it without dependencies or unsafe code.

## Alternatives considered
- Lowercase paths: can conflate distinct files on a case-sensitive volume and mishandle Unicode filename rules.
- Compare content hashes: unnecessarily rejects separate copies with identical content and reads entire files.

## Consequences
Unix hard-link aliases are conservatively refused as output names too. Session identities can outlive externally
removed inputs; an inode reused during that session can also be conservatively refused. Choose another output name.

## Validation / fitness criteria
`cargo test -p fileforge --locked`: hard-link protection on Unix and filename-case protection on case-insensitive
macOS volumes, in addition to ADR-0006's path/alias/removal tests.

## Reconsider when
Windows canonical-path protection shows an identity-related gap, or persistent input identities are required
across sessions. Hostile concurrent filesystem mutation remains outside ADR-0006's threat model.

## References
- ADR-0006: `docs/decisions/ADR-0006-safe-result-publication.md`
- Rust Unix metadata: https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html
