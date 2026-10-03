//! Lossy image pass (Balanced / Maximum presets). Every change is conservative: unusual encodings are skipped, and
//! a new encoding is kept only if it is clearly smaller than the original.

// Core
use std::collections::HashMap;

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use jpeg_encoder::{ColorType, Encoder};
use lopdf::{Document, Object, ObjectId, Stream};
use rayon::prelude::*;
// Domain
use super::limits::Limits;
use super::objects::{dict_integer, dict_name, filters, resolve};
use super::options::ImageOptions;
use super::placement::DisplaySize;
use super::streams::deflate;

/// Downsample only when the image is more than 15% above the target resolution; smaller gains are not worth a
/// resample.
const DOWNSAMPLE_THRESHOLD: f64 = 1.15;
/// A re-encoded JPEG must be at least 2% smaller to replace the original.
const MIN_JPEG_GAIN: f64 = 0.98;
/// Images decoded at once. Each holds its full bitmap (up to ~450 MB at the pixel limit), so this bounds peak memory;
/// measured: four 36 MP photos take ~1 s each on one core.
const MAX_PARALLEL_IMAGES: usize = 4;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImageStats {
    pub recompressed: u32,
    pub downsampled: u32,
    pub skipped: u32,
}

/// Pixel layout of an image the pass can handle: 8 bits per component, gray or RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pixels {
    Gray,
    Rgb,
}

impl Pixels {
    fn components(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
        }
    }
}

/// A new encoding for one image, computed in parallel and applied afterwards.
struct Replacement {
    id: ObjectId,
    content: Vec<u8>,
    filter: &'static str,
    size: (u32, u32),
    downsampled: bool,
}

struct Candidate {
    id: ObjectId,
    pixels: Pixels,
    width: u32,
    height: u32,
    target: Option<(u32, u32)>,
}

pub(crate) fn optimize_images(
    doc: &mut Document,
    options: &ImageOptions,
    sizes: &HashMap<ObjectId, DisplaySize>,
    limits: &Limits,
) -> ImageStats {
    let mut stats = ImageStats::default();
    let candidates: Vec<Candidate> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Stream(stream) if dict_name(doc, &stream.dict, b"Subtype") == Some(b"Image") => Some((*id, stream)),
            _ => None,
        })
        .filter_map(|(id, stream)| match candidate(doc, id, stream, options, sizes, limits) {
            Some(candidate) => Some(candidate),
            None => {
                stats.skipped += 1;
                None
            }
        })
        .collect();

    let encode_all = || -> Vec<Option<Replacement>> {
        candidates
            .par_iter()
            .map(|candidate| match doc.objects.get(&candidate.id) {
                Some(Object::Stream(stream)) => encode(stream, candidate, options, limits),
                _ => None,
            })
            .collect()
    };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_PARALLEL_IMAGES);
    let replacements = match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
        Ok(pool) => pool.install(encode_all),
        Err(_) => encode_all(),
    };

    for replacement in replacements {
        let Some(replacement) = replacement else {
            stats.skipped += 1;
            continue;
        };
        let Some(Object::Stream(stream)) = doc.objects.get_mut(&replacement.id) else { continue };
        stream.dict.set("Filter", Object::Name(replacement.filter.as_bytes().to_vec()));
        stream.dict.remove(b"DecodeParms");
        stream.dict.set("Width", i64::from(replacement.size.0));
        stream.dict.set("Height", i64::from(replacement.size.1));
        stream.set_content(replacement.content);
        stats.recompressed += 1;
        stats.downsampled += u32::from(replacement.downsampled);
    }
    stats
}

/// Checks everything that needs the whole document (indirect values, color space objects) before mutation.
fn candidate(
    doc: &Document,
    id: ObjectId,
    stream: &Stream,
    options: &ImageOptions,
    sizes: &HashMap<ObjectId, DisplaySize>,
    limits: &Limits,
) -> Option<Candidate> {
    let dict = &stream.dict;
    let is_mask = matches!(dict.get(b"ImageMask").ok().and_then(|v| resolve(doc, v)), Some(Object::Boolean(true)));
    // Color-key masks compare exact sample values, which lossy re-encoding would break.
    let color_key_mask = matches!(dict.get(b"Mask").ok().and_then(|v| resolve(doc, v)), Some(Object::Array(_)));
    if is_mask || color_key_mask || dict_integer(doc, dict, b"BitsPerComponent") != Some(8) {
        return None;
    }
    let width = u32::try_from(dict_integer(doc, dict, b"Width")?).ok().filter(|w| *w > 0)?;
    let height = u32::try_from(dict_integer(doc, dict, b"Height")?).ok().filter(|h| *h > 0)?;
    if u64::from(width) * u64::from(height) > limits.max_image_pixels {
        return None;
    }
    let pixels = pixels(doc, dict.get(b"ColorSpace").ok()?)?;
    let target = options.max_dpi.and_then(|dpi| target_size(width, height, sizes.get(&id).copied(), dpi));
    Some(Candidate { id, pixels, width, height, target })
}

/// Gray and RGB color spaces only; CMYK, Lab, Indexed, Separation and DeviceN are left alone.
fn pixels(doc: &Document, color_space: &Object) -> Option<Pixels> {
    match resolve(doc, color_space)? {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"G" => Some(Pixels::Gray),
            b"DeviceRGB" | b"RGB" => Some(Pixels::Rgb),
            _ => None,
        },
        Object::Array(items) => {
            let family = items.first()?.as_name().ok()?;
            match family {
                b"CalGray" => Some(Pixels::Gray),
                b"CalRGB" => Some(Pixels::Rgb),
                b"ICCBased" => {
                    let Some(Object::Stream(profile)) = items.get(1).and_then(|p| resolve(doc, p)) else { return None };
                    match dict_integer(doc, &profile.dict, b"N")? {
                        1 => Some(Pixels::Gray),
                        3 => Some(Pixels::Rgb),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Pixel size needed to show the image at `max_dpi` at its largest placement, or `None` when no downsampling is
/// warranted. Aspect ratio is kept; the axis that needs more pixels decides.
pub(crate) fn target_size(width: u32, height: u32, display: Option<DisplaySize>, max_dpi: u16) -> Option<(u32, u32)> {
    let display = display?;
    let dpi = f64::from(max_dpi);
    let needed_width = display.width_pt / 72.0 * dpi;
    let needed_height = display.height_pt / 72.0 * dpi;
    if !(needed_width > 0.0 && needed_height > 0.0) {
        return None;
    }
    let scale = (needed_width / f64::from(width)).max(needed_height / f64::from(height));
    if scale * DOWNSAMPLE_THRESHOLD >= 1.0 {
        return None;
    }
    let scaled = |value: u32| ((f64::from(value) * scale).round() as u32).max(1);
    Some((scaled(width), scaled(height)))
}

/// A smaller encoding of the image, or `None` to leave it untouched.
fn encode(stream: &Stream, candidate: &Candidate, options: &ImageOptions, limits: &Limits) -> Option<Replacement> {
    let chain = filters(stream)?;
    let (content, filter, size) = match chain.as_slice() {
        [b"DCTDecode"] if !stream.dict.has(b"DecodeParms") => {
            let (bytes, size) = reencode_jpeg(&stream.content, candidate, options.jpeg_quality)?;
            let limit = (stream.content.len() as f64 * MIN_JPEG_GAIN) as usize;
            (bytes.len() < limit).then_some((bytes, "DCTDecode", size))?
        }
        // Raw pixels are already lossless; only resolution can be reduced.
        [] | [b"FlateDecode"] if candidate.target.is_some() => {
            let (bytes, size) = downsample_raw(stream, candidate, limits)?;
            (bytes.len() < stream.content.len()).then_some((bytes, "FlateDecode", size))?
        }
        _ => return None,
    };
    Some(Replacement {
        id: candidate.id,
        content,
        filter,
        size,
        downsampled: size != (candidate.width, candidate.height),
    })
}

fn reencode_jpeg(jpeg: &[u8], candidate: &Candidate, quality: u8) -> Option<(Vec<u8>, (u32, u32))> {
    let decoded = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).ok()?;
    let matches_dict = matches!(
        (candidate.pixels, &decoded),
        (Pixels::Gray, DynamicImage::ImageLuma8(_)) | (Pixels::Rgb, DynamicImage::ImageRgb8(_))
    );
    if !matches_dict || (decoded.width(), decoded.height()) != (candidate.width, candidate.height) {
        return None;
    }
    let image = resize(decoded, candidate.target);
    let size = (image.width(), image.height());
    let (data, color) = match candidate.pixels {
        Pixels::Gray => (image.into_luma8().into_raw(), ColorType::Luma),
        Pixels::Rgb => (image.into_rgb8().into_raw(), ColorType::Rgb),
    };
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, quality);
    encoder.set_optimized_huffman_tables(true);
    encoder.encode(&data, u16::try_from(size.0).ok()?, u16::try_from(size.1).ok()?, color).ok()?;
    Some((out, size))
}

fn downsample_raw(stream: &Stream, candidate: &Candidate, limits: &Limits) -> Option<(Vec<u8>, (u32, u32))> {
    let raw = stream.get_plain_content_with_limit(limits.max_stream_bytes).ok()?;
    let expected = candidate.width as usize * candidate.height as usize * candidate.pixels.components();
    if raw.len() != expected {
        return None;
    }
    let image = match candidate.pixels {
        Pixels::Gray => DynamicImage::ImageLuma8(GrayImage::from_raw(candidate.width, candidate.height, raw)?),
        Pixels::Rgb => DynamicImage::ImageRgb8(RgbImage::from_raw(candidate.width, candidate.height, raw)?),
    };
    let image = resize(image, candidate.target);
    let size = (image.width(), image.height());
    let data = match candidate.pixels {
        Pixels::Gray => image.into_luma8().into_raw(),
        Pixels::Rgb => image.into_rgb8().into_raw(),
    };
    Some((deflate(&data)?, size))
}

fn resize(image: DynamicImage, target: Option<(u32, u32)>) -> DynamicImage {
    match target {
        Some((width, height)) => image.resize_exact(width, height, FilterType::Lanczos3),
        None => image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(width_in: f64, height_in: f64) -> Option<DisplaySize> {
        Some(DisplaySize { width_pt: width_in * 72.0, height_pt: height_in * 72.0 })
    }

    #[test]
    fn downsamples_to_the_target_dpi() {
        // 2400×1800 px shown at 4×3 in = 600 DPI → 200 DPI needs 800×600.
        assert_eq!(target_size(2400, 1800, shown(4.0, 3.0), 200), Some((800, 600)));
    }

    #[test]
    fn keeps_images_close_to_the_target() {
        // 220 DPI is within 15% of 200 DPI.
        assert_eq!(target_size(880, 660, shown(4.0, 3.0), 200), None);
        assert_eq!(target_size(400, 300, shown(4.0, 3.0), 200), None);
    }

    #[test]
    fn stretched_images_keep_the_resolution_of_the_denser_axis() {
        // Shown 2 in wide but 6 in tall: height needs 1200 px at 200 DPI, so scale is 1200/1800.
        assert_eq!(target_size(2400, 1800, shown(2.0, 6.0), 200), Some((1600, 1200)));
    }

    #[test]
    fn unknown_or_degenerate_placements_never_downsample() {
        assert_eq!(target_size(4000, 3000, None, 150), None);
        assert_eq!(target_size(4000, 3000, shown(0.0, 3.0), 150), None);
    }
}
