//! Explicit format conversion. Unlike compression, a valid result may be larger than its input.
// Core
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use serde::{Deserialize, Serialize};
// Domain
use super::{RasterError, RasterFormat, jpeg, pixels, png, webp};
use crate::control::{Control, Progress, Stage};

pub use super::pixels::{MAX_INPUT_BYTES, MAX_PIXELS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    Jpeg,
    Png,
    Webp,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub format: OutputFormat,
    pub jpeg_quality: u8,
    /// JPEG has no alpha: composite on this color before encoding. Ignored for PNG and WebP.
    pub background: [u8; 3],
}

impl Default for Options {
    fn default() -> Self {
        Self { format: OutputFormat::Png, jpeg_quality: 90, background: [255; 3] }
    }
}

impl Options {
    pub fn validate(&self) -> Result<(), RasterError> {
        if !(30..=95).contains(&self.jpeg_quality) {
            return Err(RasterError::InvalidOptions("JPEG quality must be in 30..95".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub format: OutputFormat,
    pub width: u32,
    pub height: u32,
    pub original_size: u64,
    pub output_size: u64,
    pub flattened: bool,
}

#[derive(Debug)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub report: Report,
}

pub fn convert(input: &[u8], options: &Options, control: &dyn Control) -> Result<Output, RasterError> {
    options.validate()?;
    pixels::checkpoint(control)?;
    if input.len() as u64 > MAX_INPUT_BYTES {
        return Err(RasterError::TooLarge { limit: MAX_INPUT_BYTES });
    }
    control.report(Progress { stage: Stage::Loading, done: 0, total: 0 });
    pixels::checkpoint(control)?;
    match RasterFormat::detect(input).ok_or_else(|| RasterError::Unsupported("format".into()))? {
        RasterFormat::Jpeg => jpeg::validate_conversion(input)?,
        RasterFormat::Png => png::validate_conversion(input)?,
        RasterFormat::Webp => webp::validate_conversion(input)?,
    }
    let image = pixels::decode(input)?;
    // RGBA output cannot carry a gray/CMYK profile without changing its meaning. Do not guess a color transform.
    if image.icc.as_deref().is_some_and(|icc| icc.get(16..20) != Some(b"RGB ")) {
        return Err(RasterError::Unsupported("non-RGB color profile".into()));
    }
    pixels::checkpoint(control)?;
    let (width, height) = image.pixels.dimensions();
    if options.format == OutputFormat::Webp
        && (width > fileforge_webp::MAX_DIMENSION || height > fileforge_webp::MAX_DIMENSION)
    {
        return Err(RasterError::Unsupported("WebP dimensions exceed 16383".into()));
    }
    let flattened = options.format == OutputFormat::Jpeg && image.pixels.pixels().any(|p| p[3] != 255);
    control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });
    let bytes = match options.format {
        OutputFormat::Png | OutputFormat::Webp => pixels::encode(
            &image.pixels,
            image.icc.as_deref(),
            if options.format == OutputFormat::Png { pixels::OutputFormat::Png } else { pixels::OutputFormat::Webp },
            control,
        )?,
        OutputFormat::Jpeg => {
            let width =
                u16::try_from(width).map_err(|_| RasterError::Unsupported("JPEG width exceeds 65535".into()))?;
            let height =
                u16::try_from(height).map_err(|_| RasterError::Unsupported("JPEG height exceeds 65535".into()))?;
            let rgb: Vec<u8> = image.pixels.pixels().flat_map(|p| flatten(p.0, options.background)).collect();
            pixels::checkpoint(control)?;
            let mut bytes = Vec::new();
            let mut encoder = Encoder::new(&mut bytes, options.jpeg_quality);
            encoder.set_sampling_factor(SamplingFactor::F_1_1);
            encoder.set_optimized_huffman_tables(true);
            if let Some(icc) = &image.icc {
                encoder.add_icc_profile(icc).map_err(|e| RasterError::Internal(e.to_string()))?;
            }
            encoder.encode(&rgb, width, height, ColorType::Rgb).map_err(|e| RasterError::Internal(e.to_string()))?;
            bytes
        }
    };
    pixels::checkpoint(control)?;
    // PNG/WebP already verify every sample in pixels::encode. JPEG is lossy: verify dimensions and ICC.
    if options.format == OutputFormat::Jpeg {
        control.report(Progress { stage: Stage::Verifying, done: 0, total: 0 });
        pixels::checkpoint(control)?;
        let verified = pixels::decode(&bytes)?;
        if verified.pixels.dimensions() != (width, height) || verified.icc != image.icc {
            return Err(RasterError::Internal("conversion verification failed".into()));
        }
    }
    pixels::checkpoint(control)?;
    Ok(Output {
        report: Report {
            format: options.format,
            width,
            height,
            original_size: input.len() as u64,
            output_size: bytes.len() as u64,
            flattened,
        },
        bytes,
    })
}

fn flatten(pixel: [u8; 4], background: [u8; 3]) -> [u8; 3] {
    let alpha = u32::from(pixel[3]);
    std::array::from_fn(|c| {
        ((u32::from(pixel[c]) * alpha + u32::from(background[c]) * (255 - alpha) + 127) / 255) as u8
    })
}

#[cfg(test)]
mod tests {
    // Domain
    use super::*;

    #[test]
    fn alpha_compositing_uses_the_requested_color_and_rounds() {
        assert_eq!(flatten([12, 34, 56, 0], [200, 100, 50]), [200, 100, 50]);
        assert_eq!(flatten([12, 34, 56, 255], [255; 3]), [12, 34, 56]);
        assert_eq!(flatten([0, 100, 200, 128], [255; 3]), [127, 177, 227]);
    }
}
