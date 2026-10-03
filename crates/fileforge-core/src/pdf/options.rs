// Core
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
// Domain
use super::error::PdfError;

/// Accepted JPEG quality values. The UI mirrors this range in `src/features/PdfCompress/pdfPresets.ts`.
pub const JPEG_QUALITY_RANGE: RangeInclusive<u8> = 30..=95;
/// Accepted target resolutions in dots per inch. Mirrored in the UI like [`JPEG_QUALITY_RANGE`].
pub const MAX_DPI_RANGE: RangeInclusive<u16> = 72..=600;

/// What the engine may change. `images: None` is the lossless preset (ADR-0003).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOptions {
    pub images: Option<ImageOptions>,
}

/// Lossy image settings: JPEG images are re-encoded at `jpeg_quality`; images shown at more than `max_dpi` are
/// downsampled to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageOptions {
    pub jpeg_quality: u8,
    pub max_dpi: Option<u16>,
}

impl PdfOptions {
    pub const LOSSLESS: Self = Self { images: None };

    pub fn validate(&self) -> Result<(), PdfError> {
        let Some(images) = self.images else { return Ok(()) };
        if !JPEG_QUALITY_RANGE.contains(&images.jpeg_quality) {
            return Err(PdfError::InvalidOptions(format!(
                "JPEG quality {} is outside {JPEG_QUALITY_RANGE:?}",
                images.jpeg_quality
            )));
        }
        if let Some(dpi) = images.max_dpi
            && !MAX_DPI_RANGE.contains(&dpi)
        {
            return Err(PdfError::InvalidOptions(format!("max DPI {dpi} is outside {MAX_DPI_RANGE:?}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lossy(jpeg_quality: u8, max_dpi: Option<u16>) -> PdfOptions {
        PdfOptions { images: Some(ImageOptions { jpeg_quality, max_dpi }) }
    }

    #[test]
    fn lossless_and_in_range_options_are_valid() {
        assert_eq!(PdfOptions::LOSSLESS.validate(), Ok(()));
        assert_eq!(lossy(85, Some(200)).validate(), Ok(()));
        assert_eq!(lossy(30, Some(72)).validate(), Ok(()));
        assert_eq!(lossy(95, None).validate(), Ok(()));
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        assert!(matches!(lossy(0, None).validate(), Err(PdfError::InvalidOptions(_))));
        assert!(matches!(lossy(100, None).validate(), Err(PdfError::InvalidOptions(_))));
        assert!(matches!(lossy(85, Some(50)).validate(), Err(PdfError::InvalidOptions(_))));
        assert!(matches!(lossy(85, Some(2400)).validate(), Err(PdfError::InvalidOptions(_))));
    }

    #[test]
    fn deserializes_the_ui_shape() {
        let json = r#"{"images":{"jpegQuality":70,"maxDpi":150}}"#;
        assert_eq!(serde_json::from_str::<PdfOptions>(json).ok(), Some(lossy(70, Some(150))));
        assert_eq!(serde_json::from_str::<PdfOptions>(r#"{"images":null}"#).ok(), Some(PdfOptions::LOSSLESS));
    }
}
