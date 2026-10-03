# Plan: PDF compression (ROADMAP Phase 1)

## Goal
A user adds PDFs, picks Lossless / Balanced / Maximum (or exact JPEG quality and max DPI), compresses the list, sees
before → after per file and in total, and saves results one by one or all into a folder.

## Context
ADR-0002 (lopdf engine), ADR-0003 (presets, never larger, originals untouched), ADR-0004 (ids, not paths).

## Constraints
- Lossless preset: decoded content identical; only structure, encoding of non-image streams and duplicates change.
- Output never larger than input: otherwise the original bytes are returned (`keptOriginal`).
- Never silently strip protection or break signatures: encrypted and digitally signed PDFs are rejected.
- PDF/A-1 files keep classic xref (object streams are not allowed in PDF/A-1).
- Untrusted input: size limit on the file, per-stream decode limits, pixel limit per image; a bad image is skipped,
  never fails the document; panics end as an error for that file.
- One compression at a time (memory bound); the UI runs files sequentially.

## Affected areas
`crates/fileforge-core/src/pdf/` (new), `src-tauri/src/{commands,error,results}.rs`, `src/features/PdfCompress/`,
`src/services/fileforgeApi.ts`, `src/messages/messages.ts`, docs.

## Steps
- [x] Core: options + validation, errors, limits, report types
- [x] Core: guards (encrypted, signed, PDF/A-1), dedupe identical streams, prune, re-deflate, save with object streams
- [x] Core: image placement (CTM through q/Q/cm/Do, nested forms) → effective DPI
- [x] Core: image pass (JPEG re-encode, downsample JPEG/Flate 8-bit Gray/RGB, keep-if-smaller)
- [x] Core: verification (reload, page count), never-larger fallback; tests incl. malformed-input fuzz loop
- [x] Measure on real PDFs (size, time); decide on parallel image work only if needed
- [x] Shell: result store (temp dir, cleanup on start/exit/remove), `compress_pdf`, `save_result`,
      `save_results_to_folder`, `reveal_result`; error mapping; tests
- [x] UI: presets + exact parameters, compress all (sequential), per-file status/result, totals, save actions; tests
- [x] Docs: interfaces, runtime (limits, failure modes), domain invariants, CURRENT_STATE, ROADMAP

## Validation
`cargo test -p fileforge-core`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`pnpm typecheck && pnpm test`, manual run of `pnpm tauri dev` on real PDFs.

## Risks
- Unusual image encodings (CMYK, Indexed, 16-bit, JPX, JBIG2): skipped untouched.
- Huge PDFs: whole document in memory → input limit 1 GiB.
- lopdf bugs on exotic files: verification reload + never-larger fallback + per-file errors.

## Rollback
The feature is additive; revert the branch merge.

## Progress
Done: all steps. Engine measured (`docs/architecture/runtime.md`); image work parallelized (≤ 4) after measuring
~1 s per 36 MP image on one core.

## Discoveries
- lopdf silently decrypts owner-password-only PDFs and drops `/Encrypt` on save → must reject `was_encrypted()`.
- Rewriting a PDF invalidates digital signatures → reject documents with signature dictionaries (`/ByteRange`).
- lopdf's `traverse_objects`/`prune_objects` track visited ids in a `Vec` (quadratic) → own hash-set traversal.
- lopdf writes cross-reference streams even from `save_to` by default → PDF/A-1 must force `CrossReferenceTable`.
