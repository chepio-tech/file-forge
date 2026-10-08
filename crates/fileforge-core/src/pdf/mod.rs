//! PDF compression engine (ADR-0002, ADR-0003).
//!
//! Pipeline: load with decode limits → refuse encrypted and signed files → (optional) remove metadata and editing
//! data → merge identical streams → drop
//! unreachable objects → empty CFF subroutines no glyph calls → (lossy presets) re-encode and downsample images →
//! re-deflate streams → save with object
//! and cross-reference streams → reload to verify → fall back to the original bytes unless the result is smaller.
//! Every stage reports progress and checks for cancellation through a [`Control`].

mod cff;
mod dedupe;
mod error;
mod fonts;
mod guards;
mod images;
mod jpx;
mod limits;
mod metadata;
mod objects;
mod options;
mod photos;
mod placement;
mod streams;

// Core
use lopdf::xref::XrefType;
use lopdf::{Document, LoadOptions, SaveOptions};
use serde::Serialize;

pub use crate::control::{Control, Progress, Stage};
pub use error::PdfError;
pub use limits::MAX_INPUT_BYTES;
pub use options::{ImageOptions, JPEG_QUALITY_RANGE, MAX_DPI_RANGE, PdfOptions};

// Domain
use limits::Limits;

/// XMP packets are small; anything bigger is not worth reading to detect PDF/A.
const MAX_METADATA_BYTES: usize = 4 << 20;

/// What the engine did, shown to the user next to the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfReport {
    pub original_size: u64,
    pub output_size: u64,
    /// The rewrite was not smaller, so the output is the original file byte for byte.
    pub kept_original: bool,
    pub pages: u32,
    pub images_recompressed: u32,
    pub images_downsampled: u32,
    pub duplicates_merged: u32,
    pub unused_objects_removed: u32,
    /// Info, XMP or both were removed (only with `strip_metadata`).
    pub metadata_removed: bool,
    /// `strip_metadata` kept the catalog XMP and Info because the file declares PDF/A, PDF/UA, PDF/X, PDF/E or PDF/VT.
    pub metadata_kept_for_standard: bool,
    pub thumbnails_removed: u32,
    /// Objects whose `/PieceInfo` was removed (only with `strip_editing_data`).
    pub editing_data_removed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    pub report: PdfReport,
}

/// Compresses a PDF held in memory. Never returns something larger than `input`, never panics on malformed input
/// by design (callers still isolate panics from dependencies, see ADR-0004).
pub fn compress(input: &[u8], options: &PdfOptions) -> Result<PdfOutput, PdfError> {
    compress_controlled(input, options, &())
}

/// Like [`compress`], but reports progress to `control` and ends with [`PdfError::Cancelled`] at the next
/// checkpoint once `control` asks to stop. Parsing and saving cannot be interrupted from inside.
pub fn compress_controlled(input: &[u8], options: &PdfOptions, control: &dyn Control) -> Result<PdfOutput, PdfError> {
    compress_with_limits(input, options, &Limits::DEFAULT, control)
}

pub(crate) fn compress_with_limits(
    input: &[u8],
    options: &PdfOptions,
    limits: &Limits,
    control: &dyn Control,
) -> Result<PdfOutput, PdfError> {
    options.validate()?;
    let original_size = input.len() as u64;
    if original_size > limits.max_input_bytes {
        return Err(PdfError::TooLarge { limit: limits.max_input_bytes });
    }

    checkpoint(control, Stage::Loading)?;
    let mut doc = load(input, limits)?;
    let pages = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
    if pages == 0 {
        return Err(PdfError::Malformed("the document has no pages".into()));
    }
    if guards::is_signed(&doc) {
        return Err(PdfError::Signed);
    }
    let classic_layout = guards::is_pdfa1(&doc, MAX_METADATA_BYTES);

    let mut report = PdfReport { original_size, pages, ..PdfReport::default() };
    checkpoint(control, Stage::Structure)?;
    // Before dedupe and prune, so the objects these entries pointed to are dropped as unreachable.
    if options.strip_metadata {
        let keep_document_metadata = guards::declares_standard(&doc, MAX_METADATA_BYTES);
        let stats = metadata::strip_metadata(&mut doc, keep_document_metadata);
        report.metadata_removed = stats.removed;
        report.metadata_kept_for_standard = stats.kept_for_standard;
        report.thumbnails_removed = stats.thumbnails_removed;
    }
    if options.strip_editing_data {
        report.editing_data_removed = metadata::strip_editing_data(&mut doc);
    }
    report.duplicates_merged = dedupe::merge_identical_streams(&mut doc);
    report.unused_objects_removed = objects::prune_unreachable(&mut doc);
    // Every preset: glyphs execute exactly as before, only subroutines no glyph calls go (ADR-0018).
    fonts::prune_font_programs(&mut doc, limits.max_stream_bytes, control);
    if let Some(image_options) = &options.images {
        // Finding placements walks every page; it is reported as the uncounted start of the image stage.
        checkpoint(control, Stage::Images)?;
        let sizes = placement::image_display_sizes(&doc, limits.max_stream_bytes);
        let stats = images::optimize_images(&mut doc, image_options, &sizes, limits, control);
        report.images_recompressed = stats.recompressed;
        report.images_downsampled = stats.downsampled;
    }
    cancelled(control)?;
    streams::recompress_streams(&mut doc, limits.max_stream_bytes, control);
    cancelled(control)?;
    doc.renumber_objects();

    control.report(Progress { stage: Stage::Saving, done: 0, total: 0 });
    let bytes = save(&mut doc, classic_layout)?;
    checkpoint(control, Stage::Verifying)?;
    verify(&bytes, pages, limits)?;

    if bytes.len() >= input.len() {
        return Ok(PdfOutput {
            bytes: input.to_vec(),
            report: PdfReport {
                original_size,
                output_size: original_size,
                kept_original: true,
                pages,
                ..PdfReport::default()
            },
        });
    }
    report.output_size = bytes.len() as u64;
    Ok(PdfOutput { bytes, report })
}

fn cancelled(control: &dyn Control) -> Result<(), PdfError> {
    if control.is_cancelled() { Err(PdfError::Cancelled) } else { Ok(()) }
}

/// Stops if cancelled, otherwise reports the start of an uncounted stage.
fn checkpoint(control: &dyn Control, stage: Stage) -> Result<(), PdfError> {
    cancelled(control)?;
    control.report(Progress { stage, done: 0, total: 0 });
    Ok(())
}

fn load(input: &[u8], limits: &Limits) -> Result<Document, PdfError> {
    let options = LoadOptions { max_decompressed_size: Some(limits.max_stream_bytes), ..LoadOptions::default() };
    let doc = Document::load_mem_with_options(input, options).map_err(|error| match error {
        lopdf::Error::Decryption(_) => PdfError::Encrypted,
        other => PdfError::Malformed(other.to_string()),
    })?;
    // lopdf opens owner-password-only files transparently and would save them unprotected.
    if doc.was_encrypted() || doc.is_encrypted() {
        return Err(PdfError::Encrypted);
    }
    Ok(doc)
}

fn save(doc: &mut Document, classic_layout: bool) -> Result<Vec<u8>, PdfError> {
    let mut out = Vec::new();
    let result = if classic_layout {
        // lopdf defaults to cross-reference streams even for plain saves; PDF/A-1 needs a table.
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        doc.save_to(&mut out)
    } else {
        // Object streams need PDF 1.5.
        if doc.version.parse::<f32>().map_or(true, |version| version < 1.5) {
            doc.version = "1.5".into();
        }
        let options =
            SaveOptions::builder().use_object_streams(true).use_xref_streams(true).compression_level(9).build();
        doc.save_with_options(&mut out, options)
    };
    result.map_err(|error| PdfError::Internal(format!("save failed: {error}")))?;
    Ok(out)
}

/// The output must load and keep every page; otherwise it is discarded.
fn verify(bytes: &[u8], pages: u32, limits: &Limits) -> Result<(), PdfError> {
    let reloaded = load(bytes, limits).map_err(|error| PdfError::Internal(format!("verification failed: {error}")))?;
    let reloaded_pages = reloaded.get_pages().len();
    if reloaded_pages != pages as usize {
        return Err(PdfError::Internal(format!("verification failed: {reloaded_pages} of {pages} pages")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_above_the_size_limit_are_refused_before_parsing() {
        let limits = Limits { max_input_bytes: 8, ..Limits::DEFAULT };
        assert_eq!(
            compress_with_limits(b"%PDF-1.7 and more", &PdfOptions::LOSSLESS, &limits, &()),
            Err(PdfError::TooLarge { limit: 8 })
        );
    }
}
