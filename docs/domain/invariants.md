# Domain invariants

Rules the code must keep. Each has a test; change the test and this file together.

| Invariant | Why | Enforced by |
|---|---|---|
| Originals are never written. Results live in a temp file until the user saves them. Save dialogs refuse all session input paths. | Users must be able to trust the tool with their only copy. | `file_registry.rs` → `saving_cannot_overwrite_any_input_even_after_removal`, `saving_through_an_alias_to_an_input_is_refused`; `results.rs` (store + atomic copy) |
| A result is never larger than its input; otherwise the output is the original, byte for byte (`keptOriginal`). | "Compress" must never make a file worse. | `crates/fileforge-core/tests/pdf_compress.rs` → `output_is_never_larger…`, `corrupted_files_never_panic_and_never_grow` |
| Lossless keeps decoded content identical: page content streams and image bytes are unchanged. | ADR-0003: lossless means lossless. | `lossless_shrinks_structure_and_keeps_every_byte_of_content` |
| Encrypted PDFs are refused, including owner-password-only files that open without a password. | Rewriting would silently drop the protection. | `encrypted_pdfs_are_refused_even_without_a_user_password` |
| Digitally signed PDFs are refused. | Any rewrite invalidates the signature. | `signed_pdfs_are_refused` |
| PDF/A-1 files keep a classic cross-reference table and readable XMP. | PDF/A-1 forbids object and cross-reference streams. | `pdfa1_files_keep_a_classic_cross_reference_table` |
| Images are downsampled only from placements found on pages, to the densest axis, and only when >15% over the target DPI. | Never make a visible image blurrier than requested. | `images.rs` unit tests, `balanced_downsamples…`, `placement_inside_form_xobjects…`, `images_drawn_nowhere…` |
| Unsupported image encodings (CMYK, Lab, Indexed, 16-bit, masks, color-key masks, JPX, JBIG2) are left byte-identical. | Better no gain than a corrupted color. | `unsupported_color_spaces_are_left_byte_identical` |
| Batch saving never overwrites existing files (`name (2).pdf`), even on simultaneous name collisions. | Saving into a folder must be safe without a dialog per file. | `results.rs` → `unique_paths_never_reuse_existing_names`, `concurrent_batch_saves_never_overwrite_each_other` |
