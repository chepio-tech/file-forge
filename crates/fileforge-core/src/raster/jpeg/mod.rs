//! JPEG compression: a lossless transcode (same coefficients, optimal Huffman tables) and, in lossy presets, a
//! re-encode with mozjpeg-rs. Untrusted bytes are read only by `segments`/`scans` and `jpeg-decoder`, all safe Rust.
//! Each candidate is decoded again before it is used; a lossless one must decode to exactly the original pixels.

mod huffman;
mod scans;
mod segments;

// Core
use std::hash::{DefaultHasher, Hasher};

use jpeg_decoder::{Decoder, PixelFormat};
use mozjpeg_rs::{Encoder, Preset, Subsampling};
// Domain
use super::error::{RasterError, malformed};
use super::exif;
use super::limits::Limits;
use super::options::RasterOptions;
use super::{Candidate, Kept, Outcome};
use crate::control::{Control, Progress, Stage};
use scans::{Coefficients, EncodedScan, Layout, Scan, Tables};
use segments::{APP0, APP1, APP2, APP11, APP14, APP15, COM, DHT, DRI, Frame, Item, SOF0, SOF1, SOS, Structure};

/// A lossy re-encode replaces the best lossless result only if at least 2% smaller (as for PDF images, ADR-0015).
const MIN_LOSSY_GAIN: f64 = 0.98;

pub(super) fn compress(
    input: &[u8],
    options: &RasterOptions,
    limits: &Limits,
    control: &dyn Control,
) -> Result<Outcome, RasterError> {
    let structure = segments::parse(input)?;
    let frame = Frame::find(&structure)?;
    let size = (u32::from(frame.width), u32::from(frame.height));
    if frame.pixels() > limits.max_pixels {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let kept = |reason| Ok(Outcome { size, candidate: None, kept: reason });
    if !structure.trailing.is_empty() {
        return kept(Kept::ExtraData);
    }
    if structure.segments().any(|(marker, payload)| is_content_credentials(marker, payload)) {
        return kept(Kept::Signed);
    }
    // Arithmetic-coded, lossless and hierarchical JPEGs: jpeg-decoder cannot read them, so nothing can be verified.
    if !matches!(frame.marker, SOF0 | SOF1 | 0xC2) {
        return kept(Kept::UnsupportedEncoding);
    }

    let original = decode(input, limits)?;
    if original.size != size {
        return Err(malformed("frame size differs from the decoded size"));
    }
    let original_hash = original.hash;

    let mut lossy = None;
    if let Some(quality) = options.jpeg_quality {
        cancelled(control)?;
        control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });
        lossy = reencode(&original, &frame, &structure, quality, options.strip_metadata, control)?;
    }
    drop(original);

    cancelled(control)?;
    control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });
    let transcodable = matches!(frame.marker, SOF0 | SOF1) && frame.precision == 8;
    let lossless = if transcodable {
        transcode(&structure, &frame, options.strip_metadata, limits, control)?
    } else if options.strip_metadata {
        // Progressive and 12-bit JPEGs keep their scans; only metadata segments change.
        rebuild(&structure, options.strip_metadata, None)?
    } else {
        None
    };

    cancelled(control)?;
    control.report(Progress { stage: Stage::Verifying, done: 0, total: 0 });
    // A transcoder bug must never reach the user: the candidate is dropped unless every pixel matches.
    let lossless = lossless
        .filter(|encoded| decode(&encoded.candidate.bytes, limits).is_ok_and(|decoded| decoded.hash == original_hash));
    let lossy_attempted = lossy.is_some();
    let lossy = lossy.filter(|encoded| {
        decode(&encoded.candidate.bytes, limits)
            .is_ok_and(|decoded| decoded.size == size && decoded.format == encoded.format)
    });

    // Nothing to compare against means the encoding was not one the engine can rewrite.
    let kept = if lossless.is_some() || lossy_attempted { Kept::NotSmaller } else { Kept::UnsupportedEncoding };
    let reference = lossless.as_ref().map_or(input.len(), |encoded| encoded.candidate.bytes.len());
    let lossy = lossy.filter(|encoded| encoded.candidate.bytes.len() as f64 <= reference as f64 * MIN_LOSSY_GAIN);
    let candidate = lossy.or(lossless).map(|encoded| encoded.candidate);
    Ok(Outcome { size, candidate, kept })
}

fn cancelled(control: &dyn Control) -> Result<(), RasterError> {
    if control.is_cancelled() { Err(RasterError::Cancelled) } else { Ok(()) }
}

/// C2PA Content Credentials (JUMBF in APP11) sign the file's bytes; any rewrite would invalidate them.
fn is_content_credentials(marker: u8, payload: &[u8]) -> bool {
    marker == APP11 && payload.starts_with(b"JP") && payload.windows(4).any(|window| window == b"c2pa")
}

struct Decoded {
    size: (u32, u32),
    format: PixelFormat,
    pixels: Vec<u8>,
    hash: u64,
}

fn decode(jpeg: &[u8], limits: &Limits) -> Result<Decoded, RasterError> {
    let mut decoder = Decoder::new(jpeg);
    // Four bytes per pixel (CMYK) at the pixel limit.
    decoder.set_max_decoding_buffer_size(usize::try_from(limits.max_pixels * 4).unwrap_or(usize::MAX));
    let pixels = decoder.decode().map_err(|error| RasterError::Malformed(error.to_string()))?;
    let info = decoder.info().ok_or_else(|| malformed("no image information"))?;
    let mut hasher = DefaultHasher::new();
    hasher.write(&pixels);
    Ok(Decoded {
        size: (u32::from(info.width), u32::from(info.height)),
        format: info.pixel_format,
        hash: hasher.finish(),
        pixels,
    })
}

struct Encoded {
    candidate: Candidate,
    /// What the candidate decodes to; compared for lossy candidates only.
    format: PixelFormat,
}

/// Rewrites every scan with optimal Huffman tables, keeping the coefficients. `None` if the scans cannot be read
/// strictly; the original then stays.
fn transcode(
    structure: &Structure,
    frame: &Frame,
    strip_metadata: bool,
    limits: &Limits,
    control: &dyn Control,
) -> Result<Option<Encoded>, RasterError> {
    let Ok(layout) = Layout::new(frame) else { return Ok(None) };
    // The pixel limit bounds this too; checked so a malformed sampling layout cannot ask for more.
    if layout.coefficient_bytes() > limits.max_pixels * 4 * 128 / 64 {
        return Ok(None);
    }
    let mut coefficients = Coefficients::new(&layout);
    let mut tables = Tables::default();
    let mut restart_interval = 0;
    let mut scanned = [false; 4];
    let mut scans = Vec::new();
    for item in &structure.items {
        let result = match *item {
            Item::Segment { marker: DHT, payload } => tables.define(payload),
            Item::Segment { marker: DRI, payload } => match payload {
                [high, low] => {
                    restart_interval = usize::from(u16::from_be_bytes([*high, *low]));
                    Ok(())
                }
                _ => Err(malformed("bad restart interval")),
            },
            Item::Segment { .. } => Ok(()),
            Item::Scan { header, data } => Scan::parse(header, &layout, restart_interval).and_then(|scan| {
                // Each component belongs to exactly one sequential scan.
                for index in scan.component_indices() {
                    if std::mem::replace(&mut scanned[index], true) {
                        return Err(malformed("component in several scans"));
                    }
                }
                scans::decode(&scan, data, &tables, &layout, &mut coefficients, control)?;
                scans.push(scan);
                Ok(())
            }),
        };
        match result {
            Ok(()) => {}
            Err(RasterError::Cancelled) => return Err(RasterError::Cancelled),
            Err(_) => return Ok(None),
        }
    }
    let encoded = scans::encode(&scans, &layout, &coefficients, control)?;
    drop(coefficients);
    rebuild(structure, strip_metadata, Some(&encoded))
}

/// The file again, with metadata segments filtered and, when `scans` is given, its DHT segments and scan data
/// replaced. `None` if this changes nothing.
fn rebuild(
    structure: &Structure,
    strip_metadata: bool,
    scans: Option<&[EncodedScan]>,
) -> Result<Option<Encoded>, RasterError> {
    let mut out = vec![0xFF, segments::SOI];
    let mut metadata_removed = false;
    let mut encoded = scans.map(|scans| scans.iter());
    for item in &structure.items {
        match *item {
            Item::Segment { marker: DHT, .. } if encoded.is_some() => {}
            Item::Segment { marker, payload } => match metadata_action(marker, payload, strip_metadata) {
                Action::Keep => segments::write_segment(&mut out, marker, payload)?,
                Action::Drop => metadata_removed = true,
                Action::Replace(payload) => {
                    metadata_removed = true;
                    segments::write_segment(&mut out, marker, &payload)?;
                }
            },
            Item::Scan { header, data } => {
                let data = match encoded.as_mut() {
                    Some(scans) => {
                        let scan = scans.next().ok_or_else(|| RasterError::Internal("scan count".into()))?;
                        segments::write_segment(&mut out, DHT, &scan.dht)?;
                        scan.data.as_slice()
                    }
                    None => data,
                };
                segments::write_segment(&mut out, SOS, header)?;
                out.extend_from_slice(data);
            }
        }
    }
    out.extend_from_slice(&[0xFF, segments::EOI]);
    if scans.is_none() && !metadata_removed {
        return Ok(None);
    }
    let candidate = Candidate { bytes: out, reencoded: false, metadata_removed };
    Ok(Some(Encoded { candidate, format: PixelFormat::L8 }))
}

enum Action {
    Keep,
    Drop,
    Replace(Vec<u8>),
}

/// What "Remove metadata" does with a segment. Kept always: JFIF (density), ICC profiles, the Adobe segment (it
/// decides how colors decode), everything that is not an application segment or comment. EXIF shrinks to its
/// orientation.
fn metadata_action(marker: u8, payload: &[u8], strip: bool) -> Action {
    if !strip {
        return Action::Keep;
    }
    match marker {
        APP0 if payload.starts_with(b"JFIF\0") => Action::Keep,
        APP2 if payload.starts_with(b"ICC_PROFILE\0") => Action::Keep,
        APP14 if payload.starts_with(b"Adobe") => Action::Keep,
        APP1 if payload.starts_with(b"Exif\0\0") => match exif::rotation(&payload[6..]) {
            Some(orientation) => Action::Replace([b"Exif\0\0".as_slice(), &orientation.tiff()].concat()),
            None => Action::Drop,
        },
        APP0..=APP15 | COM => Action::Drop,
        _ => Action::Keep,
    }
}

/// Re-encodes the decoded pixels with mozjpeg-rs at `quality`, keeping the source's chroma subsampling and its
/// metadata segments. `None` for CMYK and 16-bit images.
fn reencode(
    original: &Decoded,
    frame: &Frame,
    structure: &Structure,
    quality: u8,
    strip_metadata: bool,
    control: &dyn Control,
) -> Result<Option<Encoded>, RasterError> {
    let subsampling = match (original.format, frame.components.as_slice()) {
        (PixelFormat::L8, _) => Subsampling::Gray,
        (PixelFormat::RGB24, [luma, chroma @ ..]) if chroma.iter().all(|c| c.h == 1 && c.v == 1) => {
            match (luma.h, luma.v) {
                (1, 1) => Subsampling::S444,
                (2, 1) => Subsampling::S422,
                (1, 2) => Subsampling::S440,
                _ => Subsampling::S420,
            }
        }
        (PixelFormat::RGB24, _) => Subsampling::S420,
        _ => return Ok(None),
    };
    let encoder = Encoder::new(Preset::ProgressiveBalanced).quality(quality).subsampling(subsampling);
    let stop = Stop(control);
    let (width, height) = original.size;
    let result = if original.format == PixelFormat::L8 {
        encoder.encode_gray_with_stop(&original.pixels, width, height, &stop)
    } else {
        encoder.encode_rgb_with_stop(&original.pixels, width, height, &stop)
    };
    let encoded = match result {
        Ok(encoded) => encoded,
        Err(_) if control.is_cancelled() => return Err(RasterError::Cancelled),
        Err(error) => return Err(RasterError::Internal(format!("JPEG encoder: {error}"))),
    };
    let fresh = segments::parse(&encoded)?;
    let mut out = vec![0xFF, segments::SOI];
    let mut metadata_removed = false;
    // The source's metadata replaces the encoder's application segments. Its Adobe segment is left out: it
    // describes the source's color transform, not the new YCbCr data.
    for (marker, payload) in structure.segments() {
        let adobe = marker == APP14 && payload.starts_with(b"Adobe");
        if !matches!(marker, APP0..=APP15 | COM) || adobe {
            continue;
        }
        match metadata_action(marker, payload, strip_metadata) {
            Action::Keep => segments::write_segment(&mut out, marker, payload)?,
            Action::Drop => metadata_removed = true,
            Action::Replace(payload) => {
                metadata_removed = true;
                segments::write_segment(&mut out, marker, &payload)?;
            }
        }
    }
    for item in &fresh.items {
        match *item {
            Item::Segment { marker: APP0..=APP15 | COM, .. } => {}
            Item::Segment { marker, payload } => segments::write_segment(&mut out, marker, payload)?,
            Item::Scan { header, data } => {
                segments::write_segment(&mut out, SOS, header)?;
                out.extend_from_slice(data);
            }
        }
    }
    out.extend_from_slice(&[0xFF, segments::EOI]);
    let candidate = Candidate { bytes: out, reencoded: true, metadata_removed };
    Ok(Some(Encoded { candidate, format: original.format }))
}

/// Lets mozjpeg-rs stop at the caller's cancellation.
struct Stop<'a>(&'a dyn Control);

impl enough::Stop for Stop<'_> {
    fn check(&self) -> Result<(), enough::StopReason> {
        if self.0.is_cancelled() { Err(enough::StopReason::Cancelled) } else { Ok(()) }
    }
}
