//! WebP compression (ADR-0020): our RIFF reader and `image-webp` (safe Rust) read the untrusted file; libwebp, through
//! `fileforge-webp`, only encodes the decoded pixels. Lossless files stay lossless; lossy files are re-encoded only at
//! a requested WebP quality. Each candidate is decoded again before it is used.

// Core
use std::hash::{DefaultHasher, Hasher};
use std::io::Cursor;

use fileforge_webp::{EncodeError, Layout, Mode};
use image_webp::{DecodingError, WebPDecoder};
// Domain
use super::error::{RasterError, malformed};
use super::exif;
use super::limits::Limits;
use super::options::RasterOptions;
use super::{Candidate, Kept, Outcome};
use crate::control::{Control, Progress, Stage};

/// A lossy re-encode replaces the best lossless result only if at least 2% smaller (ADR-0015, as for JPEG).
const MIN_LOSSY_GAIN: f64 = 0.98;
/// libwebp lossless level (`cwebp -z`) for images up to [`Limits::max_webp_effort_pixels`], and above it. Measured
/// 2026-10-08 (runtime.md): only level 9 finds real savings on graphics (−24% on a 2560×1440 UI image, 12 s), but it
/// took 43 s on a 3840×2160 photo for nothing; level 7 costs about as much as level 6 and is rarely larger.
const LOSSLESS_LEVEL: u8 = 9;
const LARGE_LOSSLESS_LEVEL: u8 = 7;
/// libwebp lossy method. 6 is the smallest (2–16% below 4 on photos, 1.3–1.6× the time), but with exact alpha it
/// also runs libwebp's slowest alpha search (10–30× the time for 4–7%), so images with alpha get 5.
const LOSSY_METHOD: u8 = 6;
const LOSSY_METHOD_WITH_ALPHA: u8 = 5;

const VP8X: [u8; 4] = *b"VP8X";
const VP8: [u8; 4] = *b"VP8 ";
const VP8L: [u8; 4] = *b"VP8L";
const ALPH: [u8; 4] = *b"ALPH";
const ICCP: [u8; 4] = *b"ICCP";
const EXIF: [u8; 4] = *b"EXIF";
const XMP: [u8; 4] = *b"XMP ";
const ANIM: [u8; 4] = *b"ANIM";
const ANMF: [u8; 4] = *b"ANMF";
/// Content Credentials in RIFF files (C2PA 2.1, Appendix A).
const C2PA: [u8; 4] = *b"C2PA";

/// VP8X flags (RFC 9649, 2.7).
const FLAG_ICC: u8 = 0x20;
const FLAG_ALPHA: u8 = 0x10;
const FLAG_EXIF: u8 = 0x08;
const FLAG_XMP: u8 = 0x04;
const FLAG_ANIMATION: u8 = 0x02;

/// Validate the container before editing: the decoder must not silently discard source alpha.
pub(super) fn background_dimensions(input: &[u8]) -> Result<(u32, u32), RasterError> {
    let riff = Riff::parse(input)?;
    let flags = riff.flags();
    if flags & FLAG_ANIMATION != 0 || riff.has(ANIM) || riff.has(ANMF) {
        return Err(RasterError::Unsupported("animated WebP".into()));
    }
    let image = riff.image()?;
    if riff.chunks.first().is_some_and(|chunk| chunk.id == VP8X) && flags & FLAG_ALPHA == 0 && image.alpha {
        return Err(RasterError::Unsupported("WebP alpha flag differs from image data".into()));
    }
    riff.size()
}

pub(super) fn compress(
    input: &[u8],
    options: &RasterOptions,
    limits: &Limits,
    control: &dyn Control,
) -> Result<Outcome, RasterError> {
    let riff = Riff::parse(input)?;
    let size = riff.size()?;
    if u64::from(size.0) * u64::from(size.1) > limits.max_pixels {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let kept = |reason| Ok(Outcome { size, candidate: None, kept: reason });
    if !riff.trailing.is_empty() {
        return kept(Kept::ExtraData);
    }
    let flags = riff.flags();
    if flags & FLAG_ANIMATION != 0 || riff.has(ANIM) || riff.has(ANMF) {
        return kept(Kept::Animated);
    }
    // Content Credentials cover the file's bytes; any rewrite would invalidate them.
    if riff.has(C2PA) {
        return kept(Kept::Signed);
    }
    let image = riff.image()?;
    // image-webp takes transparency from the VP8X flag; if the image data has alpha the flag denies, decoding would
    // drop it.
    if riff.chunks.first().is_some_and(|chunk| chunk.id == VP8X) && flags & FLAG_ALPHA == 0 && image.alpha {
        return kept(Kept::UnsupportedEncoding);
    }

    let original = decode(input, limits)?;
    if original.size != size {
        return Err(malformed("canvas size differs from the decoded size"));
    }
    let original_hashes = original.hashes();
    let layout = if original.alpha { Layout::Rgba } else { Layout::Rgb };

    let mut lossy = None;
    let lossless = if image.lossy {
        if let Some(quality) = options.webp_quality {
            cancelled(control)?;
            control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });
            let method = if original.alpha { LOSSY_METHOD_WITH_ALPHA } else { LOSSY_METHOD };
            let mode = Mode::Lossy { quality, method };
            let encoded = encode(&original, layout, mode, control)?;
            lossy = rebuild(&riff, &encoded.image_chunks()?, options.strip_metadata, size, true)?;
        }
        // The pixels stay; only metadata can change.
        rebuild(&riff, &image.chunks, options.strip_metadata, size, false)?
    } else {
        cancelled(control)?;
        control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });
        let pixels = u64::from(size.0) * u64::from(size.1);
        let level = if pixels <= limits.max_webp_effort_pixels { LOSSLESS_LEVEL } else { LARGE_LOSSLESS_LEVEL };
        let encoded = encode(&original, layout, Mode::Lossless { level }, control)?;
        rebuild(&riff, &encoded.image_chunks()?, options.strip_metadata, size, false)?
    };
    drop(original);

    cancelled(control)?;
    control.report(Progress { stage: Stage::Verifying, done: 0, total: 0 });
    // Lossless candidates must show exactly the original samples; lossy ones keep the size and the transparency.
    let lossless = lossless.filter(|candidate| {
        decode(&candidate.bytes, limits).is_ok_and(|decoded| decoded.hashes().rgba == original_hashes.rgba)
    });
    let lossy = lossy.filter(|candidate| {
        decode(&candidate.bytes, limits)
            .is_ok_and(|decoded| decoded.size == size && decoded.hashes().alpha == original_hashes.alpha)
    });

    let pixels_kept = image.lossy && options.webp_quality.is_none();
    let kept = if pixels_kept && lossless.is_none() { Kept::LossyEncoding } else { Kept::NotSmaller };
    let reference = lossless.as_ref().map_or(input.len(), |candidate| candidate.bytes.len());
    let lossy = lossy.filter(|candidate| candidate.bytes.len() as f64 <= reference as f64 * MIN_LOSSY_GAIN);
    let candidate = lossy.or(lossless);
    Ok(Outcome { size, candidate, kept })
}

fn cancelled(control: &dyn Control) -> Result<(), RasterError> {
    if control.is_cancelled() { Err(RasterError::Cancelled) } else { Ok(()) }
}

/// One RIFF chunk: its FourCC and data, without the padding byte.
#[derive(Clone, Copy)]
struct Chunk<'a> {
    id: [u8; 4],
    data: &'a [u8],
}

/// The chunk list of a WebP file (RFC 9649, 2.4–2.7).
struct Riff<'a> {
    chunks: Vec<Chunk<'a>>,
    /// Bytes after the RIFF data; empty for a plain file.
    trailing: &'a [u8],
}

impl<'a> Riff<'a> {
    fn parse(input: &'a [u8]) -> Result<Self, RasterError> {
        if input.get(..4) != Some(b"RIFF") || input.get(8..12) != Some(b"WEBP") {
            return Err(malformed("missing WebP signature"));
        }
        let size = usize::try_from(le32(&input[4..8])).map_err(|_| malformed("RIFF size"))?;
        let end = size.checked_add(8).filter(|&end| end <= input.len() && size >= 4);
        let end = end.ok_or_else(|| malformed("truncated RIFF data"))?;
        let mut chunks = Vec::new();
        let mut pos = 12;
        while pos < end {
            let header = input.get(pos..pos + 8).filter(|_| pos + 8 <= end);
            let header = header.ok_or_else(|| malformed("truncated chunk"))?;
            let id = [header[0], header[1], header[2], header[3]];
            let length = usize::try_from(le32(&header[4..8])).map_err(|_| malformed("chunk too long"))?;
            let data_end = (pos + 8).checked_add(length).filter(|&data_end| data_end <= end);
            let data_end = data_end.ok_or_else(|| malformed("truncated chunk"))?;
            chunks.push(Chunk { id, data: &input[pos + 8..data_end] });
            // Odd chunks are padded to an even size; a missing pad byte at the very end is tolerated.
            pos = (data_end + (length & 1)).min(end);
        }
        Ok(Self { chunks, trailing: &input[end..] })
    }

    fn has(&self, id: [u8; 4]) -> bool {
        self.chunks.iter().any(|chunk| chunk.id == id)
    }

    /// The VP8X flags, or 0 for a simple file.
    fn flags(&self) -> u8 {
        match self.chunks.first() {
            Some(chunk) if chunk.id == VP8X => chunk.data.first().copied().unwrap_or(0),
            _ => 0,
        }
    }

    /// The canvas size of an extended file, otherwise the size in the image data's header.
    fn size(&self) -> Result<(u32, u32), RasterError> {
        if let Some(chunk) = self.chunks.first().filter(|chunk| chunk.id == VP8X) {
            let canvas = chunk.data.get(4..10).ok_or_else(|| malformed("short VP8X chunk"))?;
            return Ok((le24(&canvas[..3]) + 1, le24(&canvas[3..]) + 1));
        }
        let chunk = self.chunks.iter().find(|chunk| chunk.id == VP8 || chunk.id == VP8L);
        let chunk = chunk.ok_or_else(|| malformed("no image data"))?;
        if chunk.id == VP8L {
            let bits = lossless_header(chunk.data)?;
            Ok(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        } else {
            match chunk.data.get(3..10) {
                Some([0x9D, 0x01, 0x2A, w0, w1, h0, h1]) => Ok((
                    u32::from(u16::from_le_bytes([*w0, *w1]) & 0x3FFF),
                    u32::from(u16::from_le_bytes([*h0, *h1]) & 0x3FFF),
                )),
                _ => Err(malformed("bad VP8 frame header")),
            }
        }
    }

    /// The still image's chunks: `VP8L`, or `VP8 ` with an optional `ALPH` before it.
    fn image(&self) -> Result<Image<'a>, RasterError> {
        let chunks: Vec<Chunk<'a>> =
            self.chunks.iter().copied().filter(|chunk| matches!(chunk.id, ALPH | VP8 | VP8L)).collect();
        match chunks.as_slice() {
            [chunk] if chunk.id == VP8L => {
                let alpha = (lossless_header(chunk.data)? >> 28) & 1 != 0;
                Ok(Image { lossy: false, alpha, chunks })
            }
            [chunk] if chunk.id == VP8 => Ok(Image { lossy: true, alpha: false, chunks }),
            [alpha, chunk] if alpha.id == ALPH && chunk.id == VP8 => Ok(Image { lossy: true, alpha: true, chunks }),
            _ => Err(malformed("unexpected image chunks")),
        }
    }
}

/// The image data of a still WebP.
struct Image<'a> {
    lossy: bool,
    /// The data itself declares transparency: an `ALPH` chunk or the VP8L alpha bit.
    alpha: bool,
    chunks: Vec<Chunk<'a>>,
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn le24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

/// The 32 bits after the VP8L signature: width − 1, height − 1, alpha bit, version (VP8L specification, 3.2).
fn lossless_header(data: &[u8]) -> Result<u32, RasterError> {
    match data.get(..5) {
        Some([0x2F, rest @ ..]) => Ok(le32(rest)),
        _ => Err(malformed("bad VP8L header")),
    }
}

/// The file again with `image` as its image chunks, metadata filtered, and the VP8X header recomputed. `None` if this
/// gives back the input unchanged. `reencoded` tells whether `image` holds lossy re-encoded pixels.
fn rebuild(
    riff: &Riff,
    image: &[Chunk],
    strip_metadata: bool,
    size: (u32, u32),
    reencoded: bool,
) -> Result<Option<Candidate>, RasterError> {
    let original_image = riff.chunks.iter().filter(|chunk| matches!(chunk.id, ALPH | VP8 | VP8L));
    let same_image = original_image.map(|chunk| chunk.data).eq(image.iter().map(|chunk| chunk.data));
    let mut body: Vec<(Chunk, Option<Vec<u8>>)> = Vec::new();
    let mut image_written = false;
    let mut metadata_removed = false;
    for chunk in &riff.chunks {
        match chunk.id {
            VP8X => {}
            ALPH | VP8 | VP8L => {
                if !std::mem::replace(&mut image_written, true) {
                    body.extend(image.iter().map(|chunk| (*chunk, None)));
                }
            }
            _ => match metadata_action(*chunk, strip_metadata) {
                Action::Keep => body.push((*chunk, None)),
                Action::Replace(data) => {
                    metadata_removed = true;
                    body.push((*chunk, Some(data)));
                }
                Action::Drop => metadata_removed = true,
            },
        }
    }
    if same_image && !metadata_removed {
        return Ok(None);
    }

    let alpha = image.iter().any(|chunk| match chunk.id {
        ALPH => true,
        VP8L => lossless_header(chunk.data).is_ok_and(|bits| (bits >> 28) & 1 != 0),
        _ => false,
    });
    let extras = body.iter().filter(|(chunk, _)| !matches!(chunk.id, ALPH | VP8 | VP8L));
    let mut flags = 0;
    let mut extended = image.iter().any(|chunk| chunk.id == ALPH);
    for (chunk, _) in extras {
        extended = true;
        flags |= match chunk.id {
            ICCP => FLAG_ICC,
            EXIF => FLAG_EXIF,
            XMP => FLAG_XMP,
            _ => 0,
        };
    }
    let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
    if extended {
        if alpha {
            flags |= FLAG_ALPHA;
        }
        let mut header = vec![flags, 0, 0, 0];
        header.extend_from_slice(&(size.0 - 1).to_le_bytes()[..3]);
        header.extend_from_slice(&(size.1 - 1).to_le_bytes()[..3]);
        write_chunk(&mut out, VP8X, &header)?;
    }
    for (chunk, replacement) in &body {
        write_chunk(&mut out, chunk.id, replacement.as_deref().unwrap_or(chunk.data))?;
    }
    let riff_size = u32::try_from(out.len() - 8).map_err(|_| RasterError::Internal("WebP too large".into()))?;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(Some(Candidate { bytes: out, reencoded, metadata_removed }))
}

fn write_chunk(out: &mut Vec<u8>, id: [u8; 4], data: &[u8]) -> Result<(), RasterError> {
    let length = u32::try_from(data.len()).map_err(|_| RasterError::Internal("WebP chunk too large".into()))?;
    out.extend_from_slice(&id);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

enum Action {
    Keep,
    Drop,
    Replace(Vec<u8>),
}

/// What "Remove metadata" does with a chunk other than the image and VP8X: the ICC profile stays, EXIF shrinks to its
/// orientation, XMP and unknown chunks go (ADR-0019 policy).
fn metadata_action(chunk: Chunk, strip: bool) -> Action {
    if !strip || chunk.id == ICCP {
        return Action::Keep;
    }
    if chunk.id != EXIF {
        return Action::Drop;
    }
    // Some writers start the chunk with the JPEG APP1 prefix; it is kept as found.
    let prefix: &[u8] = if chunk.data.starts_with(b"Exif\0\0") { b"Exif\0\0" } else { b"" };
    match exif::rotation(&chunk.data[prefix.len()..]) {
        Some(orientation) => Action::Replace([prefix, &orientation.tiff()].concat()),
        None => Action::Drop,
    }
}

/// libwebp's output: a WebP file whose image chunks replace the original's.
struct Encoded(Vec<u8>);

impl Encoded {
    fn image_chunks(&self) -> Result<Vec<Chunk<'_>>, RasterError> {
        let riff =
            Riff::parse(&self.0).map_err(|error| RasterError::Internal(format!("WebP encoder output: {error}")))?;
        riff.image()
            .map(|image| image.chunks)
            .map_err(|error| RasterError::Internal(format!("WebP encoder output: {error}")))
    }
}

fn encode(original: &Decoded, layout: Layout, mode: Mode, control: &dyn Control) -> Result<Encoded, RasterError> {
    let (width, height) = original.size;
    let stop = || control.is_cancelled();
    match fileforge_webp::encode(&original.pixels, width, height, layout, mode, &stop) {
        Ok(bytes) => Ok(Encoded(bytes)),
        Err(EncodeError::Stopped) if control.is_cancelled() => Err(RasterError::Cancelled),
        Err(error) => Err(RasterError::Internal(format!("WebP encoder: {error}"))),
    }
}

struct Decoded {
    size: (u32, u32),
    alpha: bool,
    /// RGB or, with `alpha`, RGBA samples.
    pixels: Vec<u8>,
}

struct Hashes {
    /// Every pixel as RGBA, opaque when the image has no alpha channel.
    rgba: u64,
    alpha: u64,
}

impl Decoded {
    fn hashes(&self) -> Hashes {
        let channels = if self.alpha { 4 } else { 3 };
        let mut rgba = DefaultHasher::new();
        let mut alpha = DefaultHasher::new();
        rgba.write_u32(self.size.0);
        rgba.write_u32(self.size.1);
        let mut buffer = Vec::with_capacity(1 << 16);
        let mut alphas = Vec::with_capacity(1 << 14);
        for pixel in self.pixels.chunks_exact(channels) {
            let a = pixel.get(3).copied().unwrap_or(u8::MAX);
            buffer.extend_from_slice(&[pixel[0], pixel[1], pixel[2], a]);
            alphas.push(a);
            if buffer.len() >= 1 << 16 {
                rgba.write(&buffer);
                alpha.write(&alphas);
                buffer.clear();
                alphas.clear();
            }
        }
        rgba.write(&buffer);
        alpha.write(&alphas);
        Hashes { rgba: rgba.finish(), alpha: alpha.finish() }
    }
}

fn decode(webp: &[u8], limits: &Limits) -> Result<Decoded, RasterError> {
    let mut decoder = WebPDecoder::new(Cursor::new(webp)).map_err(|error| decoding_error(error, limits))?;
    let byte_limit = usize::try_from(limits.max_pixels * 4).unwrap_or(usize::MAX);
    decoder.set_memory_limit(byte_limit);
    let size = decoder.dimensions();
    if u64::from(size.0) * u64::from(size.1) > limits.max_pixels {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let length = decoder.output_buffer_size().filter(|&length| length <= byte_limit);
    let length = length.ok_or(RasterError::TooManyPixels { limit: limits.max_pixels })?;
    let mut pixels = vec![0; length];
    decoder.read_image(&mut pixels).map_err(|error| decoding_error(error, limits))?;
    Ok(Decoded { size, alpha: decoder.has_alpha(), pixels })
}

fn decoding_error(error: DecodingError, limits: &Limits) -> RasterError {
    match error {
        DecodingError::ImageTooLarge | DecodingError::MemoryLimitExceeded => {
            RasterError::TooManyPixels { limit: limits.max_pixels }
        }
        other => RasterError::Malformed(other.to_string()),
    }
}
