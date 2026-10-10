//! Background removal over decoded pixels; the segmenter is a port implemented outside core.
mod refine;

// Core
use image::{RgbaImage, imageops};
use jpeg_decoder::{Decoder, PixelFormat};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io::Cursor;
// Domain
use super::{RasterError, RasterFormat, exif};
use crate::control::Control;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub format: OutputFormat,
    pub background: Option<[u8; 3]>,
    pub crop: bool,
    /// A point in the oriented original image, never in the cropped result.
    pub point: Option<[f32; 2]>,
}
impl Default for Options {
    fn default() -> Self {
        Self { format: OutputFormat::Png, background: None, crop: false, point: None }
    }
}
impl Options {
    pub fn validate(&self) -> Result<(), RasterError> {
        if self.point.is_some_and(|p| p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))) {
            return Err(RasterError::InvalidOptions("point must be finite and in 0..1".into()));
        }
        Ok(())
    }
}

pub struct DecodedImage {
    pub pixels: RgbaImage,
    pub icc: Option<Vec<u8>>,
}
/// Mask logits at any bounded resolution; sigmoid is not an alpha matte.
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub logits: Vec<f32>,
}
pub trait Segmenter {
    type Embedding;
    fn embed(&self, image: &RgbaImage) -> Result<Self::Embedding, RasterError>;
    fn mask(&self, embedding: &Self::Embedding, point: [f32; 2], automatic: bool) -> Result<Mask, RasterError>;
}

fn malformed(e: impl std::fmt::Display) -> RasterError {
    RasterError::Malformed(e.to_string())
}
fn limits(w: u32, h: u32) -> Result<(), RasterError> {
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
                let mut buf = vec![0; reader.output_buffer_size()];
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

fn orient(pixels: RgbaImage, orientation: u16) -> RgbaImage {
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

/// Bilinear logits used by color-guided edge refinement.
/// No f32 image resize: that clamps logits to 0..1.
fn sample(mask: &Mask, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (mask.width - 1) as f32);
    let y = y.clamp(0.0, (mask.height - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(mask.width as usize - 1);
    let y1 = (y0 + 1).min(mask.height as usize - 1);
    let dx = x - x0 as f32;
    let dy = y - y0 as f32;
    let w = mask.width as usize;
    let top = mask.logits[y0 * w + x0] * (1.0 - dx) + mask.logits[y0 * w + x1] * dx;
    let bottom = mask.logits[y1 * w + x0] * (1.0 - dx) + mask.logits[y1 * w + x1] * dx;
    top * (1.0 - dy) + bottom * dy
}

pub fn render(
    image: &DecodedImage,
    mask: &Mask,
    options: &Options,
    control: &dyn Control,
) -> Result<RgbaImage, RasterError> {
    options.validate()?;
    checkpoint(control)?;
    limits(image.pixels.width(), image.pixels.height())?;
    if mask.width == 0
        || mask.height == 0
        || u64::from(mask.width) * u64::from(mask.height) > 1_048_576
        || mask.logits.len() != mask.width as usize * mask.height as usize
        || mask.logits.iter().any(|v| !v.is_finite())
    {
        return Err(RasterError::Internal("invalid segmentation mask".into()));
    }
    let refinement = refine::Refinement::new(&image.pixels, mask, control)?;
    let (w, h) = image.pixels.dimensions();
    let mut out = image.pixels.clone();
    let sx = mask.width as f32 / w as f32;
    let sy = mask.height as f32 / h as f32;
    let mut bounds = (w, h, 0, 0);
    let mut visible = false;
    for y in 0..h {
        checkpoint(control)?;
        for x in 0..w {
            let mx = (x as f32 + 0.5) * sx - 0.5;
            let my = (y as f32 + 0.5) * sy - 0.5;
            let z = sample(mask, mx, my);
            let source = out.get_pixel(x, y).0;
            let (color, alpha) = refinement.apply(source, x, y, w, h, z);
            let pixel = out.get_pixel_mut(x, y);
            pixel.0[..3].copy_from_slice(&color);
            pixel[3] = (f32::from(pixel[3]) * alpha).round() as u8;
            if pixel[3] == 0 {
                pixel.0 = [0; 4];
            } else {
                visible = true;
                bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            }
        }
    }
    if !visible {
        return Err(RasterError::NoSubject);
    }
    if options.crop {
        let pad = (w.max(h) as f32 * 0.02).ceil() as u32;
        let left = bounds.0.saturating_sub(pad);
        let top = bounds.1.saturating_sub(pad);
        let right = (bounds.2.saturating_add(pad).saturating_add(1)).min(w);
        let bottom = (bounds.3.saturating_add(pad).saturating_add(1)).min(h);
        out = imageops::crop_imm(&out, left, top, right - left, bottom - top).to_image();
    }
    if let Some(color) = options.background {
        for pixel in out.pixels_mut() {
            let a = u32::from(pixel[3]);
            for c in 0..3 {
                pixel[c] = ((u32::from(pixel[c]) * a + u32::from(color[c]) * (255 - a) + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
    checkpoint(control)?;
    Ok(out)
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
    let verified = decode(&bytes)?;
    if verified.pixels != *pixels || verified.icc.as_deref() != icc {
        return Err(RasterError::Internal("cutout verification failed".into()));
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

pub fn remove<S: Segmenter>(
    input: &[u8],
    options: &Options,
    segmenter: &S,
    control: &dyn Control,
) -> Result<Vec<u8>, RasterError> {
    options.validate()?;
    checkpoint(control)?;
    let image = decode(input)?;
    let embedding = segmenter.embed(&image.pixels)?;
    checkpoint(control)?;
    let mask = segmenter.mask(&embedding, options.point.unwrap_or([0.5, 0.5]), options.point.is_none())?;
    checkpoint(control)?;
    let pixels = render(&image, &mask, options, control)?;
    encode(&pixels, image.icc.as_deref(), options.format, control)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> DecodedImage {
        DecodedImage { pixels: RgbaImage::from_pixel(8, 8, image::Rgba([12, 34, 56, 128])), icc: None }
    }
    #[test]
    fn preserves_opaque_colors_and_source_alpha_and_zeros_background() {
        let image = image();
        let mask = Mask { width: 2, height: 2, logits: vec![-10.0, 10.0, -10.0, 10.0] };
        let out = render(&image, &mask, &Options::default(), &()).expect("render");
        assert_eq!(out.get_pixel(0, 0).0, [0; 4]);
        assert_eq!(out.get_pixel(7, 0).0, [12, 34, 56, 128]);
        assert!(out.pixels().all(|p| p[3] <= 128));
    }
    #[test]
    fn crop_and_solid_background_and_both_encodings() {
        let mut image = image();
        image.pixels = RgbaImage::from_pixel(16, 16, image::Rgba([12, 34, 56, 255]));
        let mut logits = vec![-10.0; 256];
        for y in 5..11 {
            for x in 5..11 {
                logits[y * 16 + x] = 10.0;
            }
        }
        let mask = Mask { width: 16, height: 16, logits };
        let out = render(&image, &mask, &Options { crop: true, ..Options::default() }, &()).expect("crop");
        assert!(out.width() < 16);
        for format in [OutputFormat::Png, OutputFormat::Webp] {
            assert_eq!(decode(&encode(&out, None, format, &()).expect("encode")).expect("decode").pixels, out);
        }
        let solid =
            render(&image, &mask, &Options { background: Some([255; 3]), ..Options::default() }, &()).expect("solid");
        assert_eq!(solid.get_pixel(0, 0).0, [255; 4]);
    }
    #[test]
    fn rejects_hostile_inputs_masks_and_points() {
        for input in [b"not an image".as_slice(), b"\xff\xd8\xff".as_slice(), b"\x89PNG\r\n\x1a\n".as_slice()] {
            assert!(decode(input).is_err());
        }
        let mask = Mask { width: 1, height: 1, logits: vec![f32::NAN] };
        assert!(render(&image(), &mask, &Options::default(), &()).is_err());
        for point in [[f32::NAN, 0.5], [0.5, 2.0]] {
            assert!(Options { point: Some(point), ..Options::default() }.validate().is_err());
        }
    }
    #[test]
    fn refuses_webp_headers_that_would_drop_source_alpha() {
        let pixels = RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 128]));
        let encoded = encode(&pixels, None, OutputFormat::Webp, &()).expect("encode");
        let mut hostile = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
        hostile.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
        hostile.extend_from_slice(&encoded[12..]);
        let size = (hostile.len() - 8) as u32;
        hostile[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(matches!(decode(&hostile), Err(RasterError::Unsupported(_))));
        hostile[20] = 0x10;
        assert_eq!(decode(&hostile).expect("honest flag").pixels, pixels);
    }
    #[test]
    fn orientation_and_cancellation() {
        let image = RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let rotated = orient(image, 6);
        assert_eq!(rotated.dimensions(), (2, 3));
        assert_eq!(rotated.get_pixel(1, 0).0, [0, 0, 0, 255]);
        struct Stop;
        impl Control for Stop {
            fn is_cancelled(&self) -> bool {
                true
            }
        }
        assert_eq!(
            render(&self::image(), &Mask { width: 1, height: 1, logits: vec![10.0] }, &Options::default(), &Stop),
            Err(RasterError::Cancelled)
        );
    }
    #[test]
    fn both_outputs_preserve_icc_and_every_pixel() {
        let source = image();
        let profile: Vec<u8> = (0..600u32).map(|i| (i * 13 % 251) as u8).collect();
        for format in [OutputFormat::Png, OutputFormat::Webp] {
            let bytes = encode(&source.pixels, Some(&profile), format, &()).expect("profile output");
            let decoded = decode(&bytes).expect("profile input");
            assert_eq!(decoded.icc.as_deref(), Some(profile.as_slice()));
            assert_eq!(decoded.pixels, source.pixels);
        }
    }
    #[test]
    fn decoding_applies_metadata_orientation_once_and_removes_it_from_output() {
        let source = RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let tiff = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut input = Vec::new();
        let mut encoder = png::Encoder::new(&mut input, 3, 2);
        encoder.set_color(png::ColorType::Rgba);
        {
            let mut writer = encoder.write_header().expect("header");
            writer.write_chunk(png::chunk::eXIf, tiff).expect("orientation");
            writer.write_image_data(source.as_raw()).expect("pixels");
        }
        let decoded = decode(&input).expect("oriented input");
        assert_eq!(decoded.pixels, imageops::rotate90(&source));
        let output = encode(&decoded.pixels, None, OutputFormat::Png, &()).expect("output");
        let reader = png::Decoder::new(Cursor::new(&output)).read_info().expect("metadata");
        assert!(reader.info().exif_metadata.is_none());
        assert_eq!(decode(&output).expect("upright output").pixels, decoded.pixels);
    }
    #[test]
    fn reduces_sixteen_bit_png_and_refuses_animation_and_large_headers() {
        let mut input = Vec::new();
        let mut encoder = png::Encoder::new(&mut input, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder.write_header().expect("header").write_image_data(&[12, 99, 34, 88, 56, 77, 128, 66]).expect("pixels");
        assert_eq!(decode(&input).expect("16-bit input").pixels.get_pixel(0, 0).0, [12, 34, 56, 128]);
        let mut animated = Vec::new();
        let mut encoder = png::Encoder::new(&mut animated, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_animated(1, 0).expect("animation");
        encoder.write_header().expect("header").write_image_data(&[12, 34, 56, 255]).expect("pixels");
        assert!(matches!(decode(&animated), Err(RasterError::Unsupported(_))));
        let mut large = Vec::new();
        {
            let mut writer = png::Encoder::new(&mut large, 6400, 5001).write_header().expect("large header");
            writer.write_chunk(png::chunk::IDAT, &[]).expect("empty data");
        }
        assert!(matches!(decode(&large), Err(RasterError::TooManyPixels { .. })));
    }
    #[test]
    fn an_empty_subject_never_encodes_a_result() {
        let mask = Mask { width: 1, height: 1, logits: vec![-20.0] };
        assert_eq!(render(&image(), &mask, &Options::default(), &()), Err(RasterError::NoSubject));
    }
    #[test]
    fn fake_segmenter_pipeline() {
        struct Fake;
        impl Segmenter for Fake {
            type Embedding = ();
            fn embed(&self, _: &RgbaImage) -> Result<(), RasterError> {
                Ok(())
            }
            fn mask(&self, _: &(), point: [f32; 2], automatic: bool) -> Result<Mask, RasterError> {
                assert_eq!(point, [0.5, 0.5]);
                assert!(automatic);
                Ok(Mask { width: 1, height: 1, logits: vec![10.0] })
            }
        }
        let input = png_bytes(&image().pixels, None).expect("input");
        let output = remove(&input, &Options::default(), &Fake, &()).expect("pipeline");
        assert_eq!(decode(&output).expect("output").pixels, image().pixels);
    }
}
