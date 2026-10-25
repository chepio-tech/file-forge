//! Bounded, oriented RGBA decoding and verified lossless encoding shared by image editing tools.
// Core
use image::{RgbaImage, imageops};
use jpeg_decoder::{Decoder, PixelFormat};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io::Cursor;
// Domain
use super::{RasterError, RasterFormat, exif};
use crate::control::{Control, Progress, Stage};

pub const MAX_PIXELS: u64 = 32_000_000;
pub const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    Png,
    Webp,
}
impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

pub struct DecodedImage {
    pub pixels: RgbaImage,
    pub icc: Option<Vec<u8>>,
}
fn malformed(e: impl std::fmt::Display) -> RasterError {
    RasterError::Malformed(e.to_string())
}
pub(super) fn limits(w: u32, h: u32) -> Result<(), RasterError> {
    if w == 0 || h == 0 {
        return Err(malformed("empty image"));
    }
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(RasterError::TooManyPixels { limit: MAX_PIXELS });
    }
    Ok(())
}
pub fn checkpoint(control: &dyn Control) -> Result<(), RasterError> {
    if control.is_cancelled() { Err(RasterError::Cancelled) } else { Ok(()) }
}

/// Safe Rust decoders; metadata is bounded by the input cap and PNG's decoder budget.
pub fn decode(input: &[u8]) -> Result<DecodedImage, RasterError> {
    if input.len() as u64 > MAX_INPUT_BYTES {
        return Err(RasterError::TooLarge { limit: MAX_INPUT_BYTES });
    }
    let (pixels, icc, orientation) =
        match RasterFormat::detect(input).ok_or_else(|| RasterError::Unsupported("format".into()))? {
            RasterFormat::Jpeg => {
                let mut decoder = Decoder::new(Cursor::new(input));
                decoder.read_info().map_err(malformed)?;
                let info = decoder.info().ok_or_else(|| malformed("missing JPEG header"))?;
                let (w, h) = (u32::from(info.width), u32::from(info.height));
                limits(w, h)?;
                if !matches!(info.pixel_format, PixelFormat::L8 | PixelFormat::RGB24) {
                    return Err(RasterError::Unsupported("CMYK/16-bit JPEG".into()));
                }
                let samples = decoder.decode().map_err(malformed)?;
                let channels = if info.pixel_format == PixelFormat::L8 { 1 } else { 3 };
                let rgba = samples
                    .chunks_exact(channels)
                    .flat_map(|p| if channels == 1 { [p[0], p[0], p[0], 255] } else { [p[0], p[1], p[2], 255] })
                    .collect();
                let pixels = RgbaImage::from_raw(w, h, rgba).ok_or_else(|| malformed("JPEG size"))?;
                let orientation = decoder
                    .exif_data()
                    .and_then(|data| exif::rotation(data.strip_prefix(b"Exif\0\0").unwrap_or(data)))
                    .map(|o| o.value)
                    .unwrap_or(1);
                (pixels, decoder.icc_profile(), orientation)
            }
            RasterFormat::Png => {
                let mut decoder =
                    png::Decoder::new_with_limits(Cursor::new(input), png::Limits { bytes: 256 * 1024 * 1024 });
                decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
                let mut reader = decoder.read_info().map_err(malformed)?;
                let info = reader.info();
                limits(info.width, info.height)?;
                if info.animation_control.is_some() {
                    return Err(RasterError::Unsupported("animated PNG".into()));
                }
                let icc = info.icc_profile.as_ref().map(|p| p.to_vec());
                let orientation = super::png::background_orientation(input)?;
                let len = reader.output_buffer_size().ok_or(RasterError::TooManyPixels { limit: MAX_PIXELS })?;
                let mut buf = vec![0; len];
                let frame = reader.next_frame(&mut buf).map_err(malformed)?;
                let rgba = buf[..frame.buffer_size()]
                    .chunks_exact(frame.color_type.samples())
                    .flat_map(|p| match frame.color_type {
                        png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                        png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                        png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                        png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                        _ => [0; 4],
                    })
                    .collect();
                (
                    RgbaImage::from_raw(frame.width, frame.height, rgba).ok_or_else(|| malformed("PNG size"))?,
                    icc,
                    orientation,
                )
            }
            RasterFormat::Webp => {
                let declared = super::webp::background_dimensions(input)?;
                limits(declared.0, declared.1)?;
                let mut decoder = image_webp::WebPDecoder::new(Cursor::new(input)).map_err(malformed)?;
                decoder.set_memory_limit((MAX_PIXELS * 4) as usize);
                let (w, h) = decoder.dimensions();
                limits(w, h)?;
                if declared != (w, h) {
                    return Err(malformed("WebP canvas differs from decoded dimensions"));
                }
                if decoder.is_animated() {
                    return Err(RasterError::Unsupported("animated WebP".into()));
                }
                let icc = decoder.icc_profile().map_err(malformed)?;
                let orientation = decoder
                    .exif_metadata()
                    .map_err(malformed)?
                    .as_deref()
                    .and_then(|data| exif::rotation(data.strip_prefix(b"Exif\0\0").unwrap_or(data)))
                    .map(|o| o.value)
                    .unwrap_or(1);
                let len = decoder.output_buffer_size().ok_or_else(|| malformed("WebP size"))?;
                let mut samples = vec![0; len];
                decoder.read_image(&mut samples).map_err(malformed)?;
                let rgba = if decoder.has_alpha() {
                    samples
                } else {
                    samples.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
                };
                (RgbaImage::from_raw(w, h, rgba).ok_or_else(|| malformed("WebP size"))?, icc, orientation)
            }
        };
    Ok(DecodedImage { pixels: orient(pixels, orientation), icc })
}

pub(super) fn orient(pixels: RgbaImage, orientation: u16) -> RgbaImage {
    match orientation {
        2 => imageops::flip_horizontal(&pixels),
        3 => imageops::rotate180(&pixels),
        4 => imageops::flip_vertical(&pixels),
        5 => imageops::rotate90(&imageops::flip_vertical(&pixels)),
        6 => imageops::rotate90(&pixels),
        7 => imageops::rotate90(&imageops::flip_horizontal(&pixels)),
        8 => imageops::rotate270(&pixels),
        _ => pixels,
    }
}

pub fn encode(
    pixels: &RgbaImage,
    icc: Option<&[u8]>,
    format: OutputFormat,
    control: &dyn Control,
) -> Result<Vec<u8>, RasterError> {
    checkpoint(control)?;
    let bytes = match format {
        OutputFormat::Png => png_bytes(pixels, icc)?,
        OutputFormat::Webp => {
            let raw = fileforge_webp::encode(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                fileforge_webp::Layout::Rgba,
                fileforge_webp::Mode::Lossless { level: 3 },
                &|| control.is_cancelled(),
            )
            .map_err(|e| {
                if control.is_cancelled() { RasterError::Cancelled } else { RasterError::Internal(e.to_string()) }
            })?;
            webp_icc(raw, icc, pixels.width(), pixels.height())?
        }
    };
    checkpoint(control)?;
    control.report(Progress { stage: Stage::Verifying, done: 0, total: 0 });
    checkpoint(control)?;
    let verified = decode(&bytes)?;
    if verified.pixels != *pixels || verified.icc.as_deref() != icc {
        return Err(RasterError::Internal("pixel verification failed".into()));
    }
    Ok(bytes)
}

pub fn png_bytes(pixels: &RgbaImage, icc: Option<&[u8]>) -> Result<Vec<u8>, RasterError> {
    let mut info = png::Info::with_size(pixels.width(), pixels.height());
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = png::BitDepth::Eight;
    info.icc_profile = icc.map(Cow::Borrowed);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::with_info(&mut bytes, info).map_err(malformed)?;
    encoder.set_compression(png::Compression::Fast);
    {
        let mut writer = encoder.write_header().map_err(malformed)?;
        writer.write_image_data(pixels.as_raw()).map_err(malformed)?;
    }
    Ok(bytes)
}
fn webp_icc(raw: Vec<u8>, icc: Option<&[u8]>, w: u32, h: u32) -> Result<Vec<u8>, RasterError> {
    let Some(icc) = icc else {
        return Ok(raw);
    };
    let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
    let mut header = vec![0x30, 0, 0, 0];
    header.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
    header.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
    fn chunk(out: &mut Vec<u8>, id: &[u8; 4], bytes: &[u8]) -> Result<(), RasterError> {
        out.extend_from_slice(id);
        out.extend_from_slice(&u32::try_from(bytes.len()).map_err(malformed)?.to_le_bytes());
        out.extend_from_slice(bytes);
        if bytes.len() % 2 == 1 {
            out.push(0);
        }
        Ok(())
    }
    chunk(&mut out, b"VP8X", &header)?;
    chunk(&mut out, b"ICCP", icc)?;
    let mut offset = 12;
    while offset + 8 <= raw.len() {
        let n = u32::from_le_bytes(raw[offset + 4..offset + 8].try_into().map_err(malformed)?) as usize;
        let end = offset
            .checked_add(8 + n + (n % 2))
            .filter(|end| *end <= raw.len())
            .ok_or_else(|| malformed("encoded WebP chunk"))?;
        if &raw[offset..offset + 4] != b"VP8X" {
            out.extend_from_slice(&raw[offset..end]);
        }
        offset = end;
    }
    let size = u32::try_from(out.len() - 8).map_err(malformed)?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}
