//! PDF compression engine (ADR-0002, ADR-0003).
//!
//! Pipeline: load with decode limits → refuse encrypted and signed files → merge identical streams → drop
//! unreachable objects → (lossy presets) re-encode and downsample images → re-deflate streams → save with object
//! and cross-reference streams → reload to verify → fall back to the original bytes unless the result is smaller.

mod dedupe;
mod error;
mod guards;
mod images;
mod limits;
mod objects;
mod options;
mod placement;
mod streams;

// Core
use lopdf::xref::XrefType;
use lopdf::{Document, LoadOptions, SaveOptions};
use serde::Serialize;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    pub report: PdfReport,
}

/// Compresses a PDF held in memory. Never returns something larger than `input`, never panics on malformed input
/// by design (callers still isolate panics from dependencies, see ADR-0004).
pub fn compress(input: &[u8], options: &PdfOptions) -> Result<PdfOutput, PdfError> {
    compress_with_limits(input, options, &Limits::DEFAULT)
}

pub(crate) fn compress_with_limits(input: &[u8], options: &PdfOptions, limits: &Limits) -> Result<PdfOutput, PdfError> {
    options.validate()?;
    let original_size = input.len() as u64;
    if original_size > limits.max_input_bytes {
        return Err(PdfError::TooLarge { limit: limits.max_input_bytes });
    }

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
    report.duplicates_merged = dedupe::merge_identical_streams(&mut doc);
    report.unused_objects_removed = objects::prune_unreachable(&mut doc);
    if let Some(image_options) = &options.images {
        let sizes = placement::image_display_sizes(&doc, limits.max_stream_bytes);
        let stats = images::optimize_images(&mut doc, image_options, &sizes, limits);
        report.images_recompressed = stats.recompressed;
        report.images_downsampled = stats.downsampled;
    }
    streams::recompress_streams(&mut doc, limits.max_stream_bytes);
    doc.renumber_objects();

    let bytes = save(&mut doc, classic_layout)?;
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
            compress_with_limits(b"%PDF-1.7 and more", &PdfOptions::LOSSLESS, &limits),
            Err(PdfError::TooLarge { limit: 8 })
        );
    }
}
