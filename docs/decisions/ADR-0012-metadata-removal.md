# ADR-0012: Optional removal of metadata, thumbnails and editing data

## Status
Accepted

## Date
2026-10-03

## Context
Exported PDFs carry data that no page shows: the Info dictionary and XMP packets (author, software, dates, edit
history), page thumbnails, and `/PieceInfo` private data. With "Preserve Illustrator Editing Capabilities",
Illustrator stores a copy of the `.ai` document there, which can make up most of the file. Removing these is useful
for privacy and size. It changes the document, though, so it cannot be part of Lossless (ADR-0003), and some
standards require document metadata: PDF/A, PDF/UA, PDF/X, PDF/E and PDF/VT declare conformance in the catalog XMP.
PDF/X-1a and PDF/X-3 declare it in the Info dictionary.

## Decision
- Two independent options, off by default, available with every preset: `stripMetadata` (Info, XMP on any object,
  page `/Thumb`) and `stripEditingData` (`/PieceInfo` on any object).
- They run after the encryption and signature guards and before deduplication and pruning. The existing prune pass
  then deletes the XMP streams, thumbnail images and private data that became unreachable.
- If the catalog XMP contains a conformance identifier (`pdfaid:part`, `pdfuaid:part`, `pdfxid:GTS_PDFXVersion`,
  `pdfe:ISO_PDFEVersion`, `pdfvtid:GTS_PDFVTVersion`), or the Info dictionary has `GTS_PDFXVersion`, the catalog XMP
  and the Info dictionary stay. The report says `metadataKeptForStandard`. XMP on other objects and thumbnails are
  still removed, because no standard requires them.
- Never removed: structure tree and `MarkInfo` (accessibility), output intents, annotations, attachments, optional
  content.
- "Never larger" stays absolute. If the rewritten file is not smaller, the original is kept byte for byte with
  its metadata. The UI then says "original kept, nothing removed" instead of only "Already optimal".

## Rationale
Separate options keep consequences readable: metadata removal is mostly about privacy and saves kilobytes, while
removing editing data can save most of a design export but stops Illustrator from editing the file natively.
Keeping document metadata for declared standards avoids silently breaking an archival, accessibility or print
claim. Reusing the prune pass avoids a second object-graph walk.

## Alternatives considered
- One checkbox for everything: hides the editability loss behind a privacy option.
- Strip metadata even when the result is not smaller: would break the "never larger" guarantee (ADR-0003).
- Refuse removal for standard-conforming files: blocks thumbnail and editing-data removal that is still safe.
- Rewrite XMP to keep only the conformance identifiers: correct XMP editing needs an XML parser and validation
  against each standard's schema; not worth it for kilobytes.

## Consequences
- A file the detector does not recognize as conforming (for example a claim made only in an XMP extension schema)
  loses its metadata when the user asks for removal.
- The kept original of a not-smaller file still contains its metadata; users who need guaranteed removal see it
  in the status text.

## Validation / fitness criteria
- `crates/fileforge-core/tests/pdf_metadata.rs`: nothing removed by default; removal on request with identical page
  content; editing data and its private streams removed; PDF/A, PDF/UA, PDF/X (XMP and Info) keep document metadata;
  structure tree survives; never larger.
- `src/features/PdfCompress/PdfCompress.test.tsx`: options off by default, kept across presets, reported per file.

## Reconsider when
- Users need removal in files with declared standards: add a schema-aware XMP rewrite.
- Other private data (for example `/AIPrivateData` outside `/PieceInfo`) turns up in real files.

## References
- ISO 32000-2, 14.3 Metadata, 14.5 Page-piece dictionaries, 12.3.4 Thumbnail images
- ISO 19005 (PDF/A), ISO 14289 (PDF/UA), ISO 15930 (PDF/X)
