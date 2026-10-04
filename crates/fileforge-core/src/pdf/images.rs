//! Lossy image pass (Balanced / Maximum / Screen presets). Every change is conservative: unusual encodings are skipped,
//! and a new encoding is kept only if it is clearly smaller than the original. JPEG and JPEG 2000 images become JPEG;
//! Flate photographs may become JPEG on explicit request; detected screen content keeps Deflate (ADR-0016).

// Core
use std::collections::HashMap;

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, RgbImage};
use jpeg_decoder::{Decoder, PixelFormat};
use jpeg_encoder::{ColorType, Encoder};
use lopdf::{Document, Object, ObjectId, Stream};
use rayon::prelude::*;
// Domain
use super::control::{Control, Counter, Stage};
use super::jpx;
use super::limits::Limits;
use super::objects::{dict_integer, dict_name, filters, resolve};
use super::options::ImageOptions;
use super::photos;
use super::placement::DisplaySize;
use super::streams::deflate;

/// Downsample only when the image is more than 15% above the target resolution; smaller gains are not worth a
/// resample.
const DOWNSAMPLE_THRESHOLD: f64 = 1.15;
/// A lossy original (JPEG or JPEG 2000) is replaced only by a JPEG at least 2% smaller.
const MIN_LOSSY_GAIN: f64 = 0.98;
/// Trading lossless pixels for JPEG requires at least 20% savings over both the input and equivalent Deflate.
const MIN_PHOTO_GAIN: f64 = 0.80;
/// Images decoded at once. Each holds its full bitmap (up to ~450 MB at the pixel limit, ~0.8 GB while decoding JPEG
/// 2000 at its sample limit), so this bounds peak memory; measured: four 36 MP photos take ~1 s each on one core.
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
    control: &dyn Control,
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

    let counter = Counter::start(control, Stage::Images, candidates.len());
    let encode_all = || -> Vec<Option<Replacement>> {
        candidates
            .par_iter()
            .map(|candidate| {
                // The caller discards the document once cancelled, so remaining images are not worth decoding.
                if control.is_cancelled() {
                    return None;
                }
                let replacement = match doc.objects.get(&candidate.id) {
                    Some(Object::Stream(stream)) => encode(stream, candidate, options, limits),
                    _ => None,
                };
                counter.advance();
                replacement
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
        // JPEG 2000 may omit the bit depth and carry `SMaskInData 0`; neither belongs to the new encoding.
        stream.dict.remove(b"SMaskInData");
        stream.dict.set("BitsPerComponent", 8);
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
    let is_jpx = filters(stream).is_some_and(|chain| chain == [b"JPXDecode"]);
    // JPEG 2000 takes its bit depth from the codestream, so the dictionary may omit it.
    let bits_ok = match dict_integer(doc, dict, b"BitsPerComponent") {
        Some(bits) => bits == 8,
        None => is_jpx,
    };
    // PDF ignores `Decode` for JPEG 2000 but would apply it to the JPEG replacing it; `SMaskInData` makes the
    // codestream's alpha channel the image's soft mask.
    let jpx_extras =
        is_jpx && (dict.has(b"Decode") || dict_integer(doc, dict, b"SMaskInData").is_some_and(|value| value != 0));
    if is_mask || color_key_mask || !bits_ok || jpx_extras {
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
        [filter @ (b"DCTDecode" | b"JPXDecode")] if !stream.dict.has(b"DecodeParms") => {
            let image = if *filter == b"DCTDecode" {
                decode_jpeg(&stream.content, candidate)?
            } else {
                decode_jpx(&stream.content, candidate, limits)?
            };
            let (bytes, size) = encode_jpeg(image, candidate, options.jpeg_quality)?;
            let limit = (stream.content.len() as f64 * MIN_LOSSY_GAIN) as usize;
            (bytes.len() < limit).then_some((bytes, "DCTDecode", size))?
        }
        [] | [b"FlateDecode"] if candidate.target.is_some() || (options.compress_flate_photos && !chain.is_empty()) => {
            encode_raw(stream, candidate, options, limits)?
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

/// The JPEG's pixels, if their layout matches the image dictionary. The header is checked before any pixel buffer
/// exists.
fn decode_jpeg(jpeg: &[u8], candidate: &Candidate) -> Option<DynamicImage> {
    let mut decoder = Decoder::new(jpeg);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    let matches_dict = matches!(
        (candidate.pixels, info.pixel_format),
        (Pixels::Gray, PixelFormat::L8) | (Pixels::Rgb, PixelFormat::RGB24)
    );
    if !matches_dict || (u32::from(info.width), u32::from(info.height)) != (candidate.width, candidate.height) {
        return None;
    }
    bitmap(candidate, decoder.decode().ok()?)
}

fn decode_jpx(jpx: &[u8], candidate: &Candidate, limits: &Limits) -> Option<DynamicImage> {
    let components = candidate.pixels.components();
    let samples = jpx::decode(jpx, candidate.width, candidate.height, components, limits.max_jpx_samples)?;
    bitmap(candidate, samples)
}

/// Downsamples to the candidate's target, if any, and encodes as JPEG.
fn encode_jpeg(image: DynamicImage, candidate: &Candidate, quality: u8) -> Option<(Vec<u8>, (u32, u32))> {
    let image = resize(image, candidate.target);
    let size = (image.width(), image.height());
    Some((jpeg_bytes(&image, candidate.pixels, quality)?, size))
}

fn jpeg_bytes(image: &DynamicImage, pixels: Pixels, quality: u8) -> Option<Vec<u8>> {
    let (data, color) = match pixels {
        Pixels::Gray => (image.as_luma8()?.as_raw(), ColorType::Luma),
        Pixels::Rgb => (image.as_rgb8()?.as_raw(), ColorType::Rgb),
    };
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, quality);
    encoder.set_optimized_huffman_tables(true);
    encoder.encode(data, u16::try_from(image.width()).ok()?, u16::try_from(image.height()).ok()?, color).ok()?;
    Some(out)
}

type RawEncoding = (Vec<u8>, &'static str, (u32, u32));

fn encode_raw(stream: &Stream, candidate: &Candidate, options: &ImageOptions, limits: &Limits) -> Option<RawEncoding> {
    let image = bitmap(candidate, raw_samples(stream, candidate, limits)?)?;
    let can_convert = options.compress_flate_photos
        && filters(stream)? == [b"FlateDecode"]
        && ![b"Decode".as_slice(), b"Mask", b"SMask"].iter().any(|key| stream.dict.has(key))
        && photos::is_photographic(image.as_bytes(), candidate.width, candidate.height, candidate.pixels.components());
    // Inspect source pixels first: resampling would hide sharp text/UI edges.
    if !can_convert && candidate.target.is_none() {
        return None;
    }
    let image = resize(image, candidate.target);
    let size = (image.width(), image.height());
    let lossless = deflate(image.as_bytes())?;
    if can_convert
        && let Some(jpeg) = jpeg_bytes(&image, candidate.pixels, options.jpeg_quality)
        && photo_gain_is_worthwhile(jpeg.len(), stream.content.len(), lossless.len())
    {
        return Some((jpeg, "DCTDecode", size));
    }
    // A photo veto or an expensive JPEG keeps the existing lossless downsampling path.
    (candidate.target.is_some() && lossless.len() < stream.content.len()).then_some((lossless, "FlateDecode", size))
}

fn photo_gain_is_worthwhile(jpeg: usize, original: usize, deflate: usize) -> bool {
    jpeg as f64 <= original.min(deflate) as f64 * MIN_PHOTO_GAIN
}

/// lopdf handles direct predictor dictionaries only. Validate their layout before decoding; indirect/array
/// parameters or oversized/mismatched predictor rows must not be silently interpreted as plain pixels.
fn raw_samples(stream: &Stream, candidate: &Candidate, limits: &Limits) -> Option<Vec<u8>> {
    if let Ok(params) = stream.dict.get(b"DecodeParms") {
        match params {
            Object::Null => {}
            Object::Dictionary(params) => {
                let integer = |key: &[u8], default: i64| -> Option<i64> {
                    match params.get(key).ok() {
                        Some(Object::Integer(value)) => Some(*value),
                        None => Some(default),
                        _ => None,
                    }
                };
                let predictor = integer(b"Predictor", 1)?;
                if !matches!(predictor, 1 | 2 | 10..=15) {
                    return None;
                }
                if predictor != 1
                    && (integer(b"Columns", 1)? != i64::from(candidate.width)
                        || integer(b"Colors", 1)? != candidate.pixels.components() as i64
                        || integer(b"BitsPerComponent", 8)? != 8)
                {
                    return None;
                }
            }
            _ => return None,
        }
    }
    let expected = (candidate.width as usize)
        .checked_mul(candidate.height as usize)?
        .checked_mul(candidate.pixels.components())?;
    // PNG predictors add one filter byte per row before their reversal.
    let bound = expected.checked_add(candidate.height as usize)?.min(limits.max_stream_bytes);
    stream.get_plain_content_with_limit(bound).ok()
}

/// Interleaved 8-bit samples as an image, if their count matches the candidate's size and layout.
fn bitmap(candidate: &Candidate, samples: Vec<u8>) -> Option<DynamicImage> {
    let (width, height) = (candidate.width, candidate.height);
    if samples.len() != width as usize * height as usize * candidate.pixels.components() {
        return None;
    }
    match candidate.pixels {
        Pixels::Gray => GrayImage::from_raw(width, height, samples).map(DynamicImage::ImageLuma8),
        Pixels::Rgb => RgbImage::from_raw(width, height, samples).map(DynamicImage::ImageRgb8),
    }
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

    #[test]
    fn trading_lossless_pixels_for_jpeg_requires_twenty_percent_savings_against_both_encodings() {
        assert!(photo_gain_is_worthwhile(80, 100, 200));
        assert!(photo_gain_is_worthwhile(80, 200, 100));
        assert!(!photo_gain_is_worthwhile(81, 100, 200));
        assert!(!photo_gain_is_worthwhile(81, 200, 100));
        assert!(!photo_gain_is_worthwhile(90, 100, 100));
    }

    #[test]
    fn flate_decoding_honors_the_stream_limit_and_expected_sample_count() {
        let candidate = Candidate { id: (1, 0), pixels: Pixels::Rgb, width: 64, height: 64, target: None };
        let samples = vec![42; 64 * 64 * 3];
        let stream = Stream::new(lopdf::dictionary! { "Filter" => "FlateDecode" }, deflate(&samples).expect("deflate"));
        assert_eq!(raw_samples(&stream, &candidate, &Limits::DEFAULT), Some(samples));
        let limits = Limits { max_stream_bytes: 100, ..Limits::DEFAULT };
        assert_eq!(raw_samples(&stream, &candidate, &limits), None);
        let oversized = vec![42; 64 * 64 * 3 + 65];
        let stream =
            Stream::new(lopdf::dictionary! { "Filter" => "FlateDecode" }, deflate(&oversized).expect("deflate"));
        assert_eq!(raw_samples(&stream, &candidate, &Limits::DEFAULT), None);
    }
}
