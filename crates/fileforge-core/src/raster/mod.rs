//! Image compression engine for JPEG, PNG and WebP files (plan 0014, ADR-0019, ADR-0020).
//!
//! Pipeline: refuse oversized files → detect the format by content → refuse more pixels than the limit → keep files
//! whose bytes must not change (signed, animated, extra data after the image) → decode in safe Rust → build
//! candidates (JPEG: lossless transcode, lossy re-encode; PNG: oxipng; WebP: libwebp lossless, or lossy re-encode of
//! lossy files) → decode each candidate again to verify →
//! use the smallest valid candidate only if it is smaller than the file, otherwise return the original bytes.
//! Every stage reports progress and checks for cancellation through a [`Control`].

pub mod background;
mod error;
mod exif;
mod jpeg;
mod limits;
mod options;
mod png;
mod webp;

// Core
use serde::Serialize;

pub use crate::control::{Control, Progress, Stage};
pub use error::RasterError;
pub use limits::MAX_INPUT_BYTES;
pub use options::{JPEG_QUALITY_RANGE, PNG_LEVEL_RANGE, RasterOptions, WEBP_QUALITY_RANGE};

// Domain
use limits::Limits;

/// Image formats the engine compresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RasterFormat {
    Jpeg,
    Png,
    Webp,
}

impl RasterFormat {
    /// Recognizes JPEG, PNG and WebP by their signatures, never by extension.
    pub fn detect(input: &[u8]) -> Option<Self> {
        if input.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if input.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Self::Png)
        } else if input.get(..4) == Some(b"RIFF") && input.get(8..12) == Some(b"WEBP") {
            Some(Self::Webp)
        } else {
            None
        }
    }
}

/// Why the output is the original file byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kept {
    /// No candidate was smaller (for a lossy re-encode: at least 2% smaller than the best lossless result).
    NotSmaller,
    /// Data follows the image (for example an HDR gain map or depth map) that a rewrite would break.
    ExtraData,
    /// Content Credentials (C2PA) or a digital signature cover the file's bytes.
    Signed,
    /// Animated PNG or WebP.
    Animated,
    /// A coding the engine does not rewrite: arithmetic, lossless, hierarchical or not strictly readable JPEGs, and
    /// WebPs whose header denies the transparency their image data has.
    UnsupportedEncoding,
    /// A lossy WebP and no WebP quality: its pixels stay, and there is no lossless way to recode them.
    LossyEncoding,
}

/// What the engine did, shown to the user next to the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterReport {
    pub format: RasterFormat,
    pub width: u32,
    pub height: u32,
    pub original_size: u64,
    pub output_size: u64,
    /// Set when the output is the original file byte for byte.
    pub kept: Option<Kept>,
    /// The pixels were re-encoded with the requested JPEG or WebP quality; otherwise they are exactly the original
    /// ones.
    pub reencoded: bool,
    /// Metadata was removed (only with `strip_metadata`).
    pub metadata_removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterOutput {
    pub bytes: Vec<u8>,
    pub report: RasterReport,
}

/// A rewritten file that passed verification.
pub(crate) struct Candidate {
    bytes: Vec<u8>,
    reencoded: bool,
    metadata_removed: bool,
}

/// A format module's result: the best candidate, if any, and why the original stays otherwise.
pub(crate) struct Outcome {
    size: (u32, u32),
    candidate: Option<Candidate>,
    kept: Kept,
}

/// Compresses a JPEG, PNG or WebP held in memory. Never returns something larger than `input`.
pub fn compress(input: &[u8], options: &RasterOptions) -> Result<RasterOutput, RasterError> {
    compress_controlled(input, options, &())
}

/// Like [`compress`], but reports progress to `control` and ends with [`RasterError::Cancelled`] at the next
/// checkpoint once `control` asks to stop.
pub fn compress_controlled(
    input: &[u8],
    options: &RasterOptions,
    control: &dyn Control,
) -> Result<RasterOutput, RasterError> {
    compress_with_limits(input, options, &Limits::DEFAULT, control)
}

pub(crate) fn compress_with_limits(
    input: &[u8],
    options: &RasterOptions,
    limits: &Limits,
    control: &dyn Control,
) -> Result<RasterOutput, RasterError> {
    options.validate()?;
    let original_size = input.len() as u64;
    if original_size > limits.max_input_bytes {
        return Err(RasterError::TooLarge { limit: limits.max_input_bytes });
    }
    if control.is_cancelled() {
        return Err(RasterError::Cancelled);
    }
    control.report(Progress { stage: Stage::Loading, done: 0, total: 0 });
    let format = RasterFormat::detect(input).ok_or_else(|| RasterError::Unsupported(describe(input).into()))?;
    let outcome = match format {
        RasterFormat::Jpeg => jpeg::compress(input, options, limits, control)?,
        RasterFormat::Png => png::compress(input, options, limits, control)?,
        RasterFormat::Webp => webp::compress(input, options, limits, control)?,
    };
    let (width, height) = outcome.size;
    let report = RasterReport {
        format,
        width,
        height,
        original_size,
        output_size: original_size,
        kept: Some(outcome.kept),
        reencoded: false,
        metadata_removed: false,
    };
    match outcome.candidate {
        Some(candidate) if candidate.bytes.len() < input.len() => Ok(RasterOutput {
            report: RasterReport {
                output_size: candidate.bytes.len() as u64,
                kept: None,
                reencoded: candidate.reencoded,
                metadata_removed: candidate.metadata_removed,
                ..report
            },
            bytes: candidate.bytes,
        }),
        _ => Ok(RasterOutput { bytes: input.to_vec(), report }),
    }
}

/// A short name for formats the engine recognizes but does not compress (yet), for the error detail.
fn describe(input: &[u8]) -> &'static str {
    if input.get(4..12).is_some_and(|brand| brand.starts_with(b"ftyp")) {
        "HEIF/AVIF"
    } else if input.starts_with(b"GIF8") {
        "GIF"
    } else if input.starts_with(b"II*\0") || input.starts_with(b"MM\0*") {
        "TIFF"
    } else if input.starts_with(b"BM") {
        "BMP"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_above_the_size_limit_are_refused_before_parsing() {
        let limits = Limits { max_input_bytes: 8, ..Limits::DEFAULT };
        assert_eq!(
            compress_with_limits(&[0xFF, 0xD8, 0xFF, 0, 0, 0, 0, 0, 0], &RasterOptions::LOSSLESS, &limits, &()),
            Err(RasterError::TooLarge { limit: 8 })
        );
    }

    #[test]
    fn images_above_the_pixel_limit_are_refused_before_decoding() {
        let mut jpeg = Vec::new();
        let pixels = vec![128; 64 * 48];
        let encoded = jpeg_encoder::Encoder::new(&mut jpeg, 90).encode(&pixels, 64, 48, jpeg_encoder::ColorType::Luma);
        assert!(encoded.is_ok());
        let mut png = Vec::new();
        {
            let mut encoder = ::png::Encoder::new(&mut png, 64, 48);
            encoder.set_color(::png::ColorType::Grayscale);
            let written = encoder.write_header().and_then(|mut writer| writer.write_image_data(&pixels));
            assert!(written.is_ok());
        }
        let rgb = vec![128; 64 * 48 * 3];
        let lossless = fileforge_webp::Mode::Lossless { level: 0 };
        let webp = fileforge_webp::encode(&rgb, 64, 48, fileforge_webp::Layout::Rgb, lossless, &|| false)
            .expect("test WebP encodes");
        let limits = Limits { max_pixels: 64 * 48 - 1, ..Limits::DEFAULT };
        for input in [jpeg, png, webp] {
            let result = compress_with_limits(&input, &RasterOptions::LOSSLESS, &limits, &());
            assert_eq!(result.map(|output| output.report), Err(RasterError::TooManyPixels { limit: 64 * 48 - 1 }));
        }
    }

    #[test]
    fn other_formats_are_named_in_the_error() {
        let heic = b"\0\0\0\x18ftypheic\0\0\0\0";
        assert_eq!(compress(heic, &RasterOptions::LOSSLESS), Err(RasterError::Unsupported("HEIF/AVIF".into())));
        assert_eq!(compress(b"GIF89a\x01\0", &RasterOptions::LOSSLESS), Err(RasterError::Unsupported("GIF".into())));
        assert_eq!(compress(b"%PDF-1.7", &RasterOptions::LOSSLESS), Err(RasterError::Unsupported("unknown".into())));
    }
}
