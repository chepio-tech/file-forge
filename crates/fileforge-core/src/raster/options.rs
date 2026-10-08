// Core
use std::ops::RangeInclusive;

use serde::Deserialize;
// Domain
use super::error::RasterError;

/// JPEG quality the UI offers for re-encoding, the same range as for PDF images.
pub const JPEG_QUALITY_RANGE: RangeInclusive<u8> = 30..=95;
/// WebP quality for re-encoding lossy WebPs, the same range as for JPEG.
pub const WEBP_QUALITY_RANGE: RangeInclusive<u8> = 30..=95;
/// oxipng optimization levels: more filter and compression trials at higher levels, never a lossy step.
pub const PNG_LEVEL_RANGE: RangeInclusive<u8> = 0..=6;

/// What a compression may do. Every field maps to a visible control; presets live in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterOptions {
    /// Re-encode JPEGs at this quality when that is at least 2% smaller; `None` keeps their pixels exactly.
    pub jpeg_quality: Option<u8>,
    /// oxipng level for PNGs, see [`PNG_LEVEL_RANGE`].
    pub png_level: u8,
    /// Re-encode lossy WebPs at this quality when that is at least 2% smaller; `None` keeps their pixels exactly.
    /// Lossless WebPs always stay lossless.
    #[serde(default)]
    pub webp_quality: Option<u8>,
    /// Finish small PNGs with Zopfli: a few percent smaller, many times slower.
    #[serde(default)]
    pub png_zopfli: bool,
    /// Remove EXIF (except orientation), XMP, IPTC, comments and text chunks. Color profiles, color and density
    /// information always stay.
    #[serde(default)]
    pub strip_metadata: bool,
}

impl RasterOptions {
    pub const LOSSLESS: Self =
        Self { jpeg_quality: None, png_level: 2, webp_quality: None, png_zopfli: false, strip_metadata: false };

    pub fn validate(&self) -> Result<(), RasterError> {
        if let Some(quality) = self.jpeg_quality
            && !JPEG_QUALITY_RANGE.contains(&quality)
        {
            return Err(RasterError::InvalidOptions(format!(
                "JPEG quality {quality} is outside {JPEG_QUALITY_RANGE:?}"
            )));
        }
        if let Some(quality) = self.webp_quality
            && !WEBP_QUALITY_RANGE.contains(&quality)
        {
            return Err(RasterError::InvalidOptions(format!(
                "WebP quality {quality} is outside {WEBP_QUALITY_RANGE:?}"
            )));
        }
        if !PNG_LEVEL_RANGE.contains(&self.png_level) {
            return Err(RasterError::InvalidOptions(format!(
                "PNG level {} is outside {PNG_LEVEL_RANGE:?}",
                self.png_level
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_outside_the_ranges_are_refused() {
        assert_eq!(RasterOptions::LOSSLESS.validate(), Ok(()));
        let quality = RasterOptions { jpeg_quality: Some(96), ..RasterOptions::LOSSLESS };
        assert!(matches!(quality.validate(), Err(RasterError::InvalidOptions(_))));
        let level = RasterOptions { png_level: 7, ..RasterOptions::LOSSLESS };
        assert!(matches!(level.validate(), Err(RasterError::InvalidOptions(_))));
        for webp_quality in [29, 96] {
            let webp = RasterOptions { webp_quality: Some(webp_quality), ..RasterOptions::LOSSLESS };
            assert!(matches!(webp.validate(), Err(RasterError::InvalidOptions(_))));
        }
    }

    #[test]
    fn optional_flags_default_to_off() {
        let options = serde_json::from_str::<RasterOptions>(r#"{ "jpegQuality": 80, "pngLevel": 4 }"#);
        assert_eq!(
            options.ok(),
            Some(RasterOptions { jpeg_quality: Some(80), png_level: 4, ..RasterOptions::LOSSLESS })
        );
    }
}
