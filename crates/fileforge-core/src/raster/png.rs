//! PNG compression: decode with the `png` crate (safe Rust), optimize the decoded rows with oxipng, and use the
//! result only if it decodes to the same pixels. oxipng's libdeflate (C) never inflates the untrusted file: it gets
//! decoded rows, and the ICC profile as png decoded it.

// Core
use std::hash::{DefaultHasher, Hasher};
use std::io::Cursor;

use oxipng::{BitDepth, ColorType, Deflater, Options, RGB16, RGBA8, RawImage, StripChunks, ZopfliOptions};
use png::{Decoder, DecodingError, Transformations};
// Domain
use super::error::{RasterError, malformed};
use super::exif;
use super::limits::Limits;
use super::options::RasterOptions;
use super::{Candidate, Kept, Outcome};
use crate::control::{Control, Progress, Stage};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// png 0.17 does not populate eXIf; read its bounded borrowed payload without allocating a chunk list.
pub(super) fn background_orientation(input: &[u8]) -> Result<u16, RasterError> {
    let mut pos = SIGNATURE.len();
    let mut orientation = 1;
    loop {
        let header = input.get(pos..pos + 8).ok_or_else(|| malformed("truncated PNG chunk"))?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let end = pos
            .checked_add(12 + length)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| malformed("truncated PNG chunk"))?;
        if &header[4..8] == b"eXIf" {
            orientation = exif::rotation(&input[pos + 8..end - 4]).map(|o| o.value).unwrap_or(1);
        }
        if &header[4..8] == b"IEND" {
            return Ok(orientation);
        }
        pos = end;
    }
}

pub(super) fn compress(
    input: &[u8],
    options: &RasterOptions,
    limits: &Limits,
    control: &dyn Control,
) -> Result<Outcome, RasterError> {
    let chunks = Chunks::parse(input)?;
    let size = chunks.size()?;
    if u64::from(size.0) * u64::from(size.1) > limits.max_pixels {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let kept = |reason| Ok(Outcome { size, candidate: None, kept: reason });
    if !chunks.trailing.is_empty() {
        return kept(Kept::ExtraData);
    }
    if chunks.has(b"acTL") {
        return kept(Kept::Animated);
    }
    // C2PA Content Credentials and digital signatures cover the file's bytes; any rewrite would invalidate them.
    if chunks.has(b"caBX") || chunks.has(b"dSIG") {
        return kept(Kept::Signed);
    }

    let decoded = decode_rows(input, limits)?;
    let original_hash = pixel_hash(input, limits)?;
    cancelled(control)?;
    control.report(Progress { stage: Stage::Encoding, done: 0, total: 0 });

    let mut raw = RawImage::new(size.0, size.1, decoded.color_type, decoded.bit_depth, decoded.rows)
        .map_err(|error| RasterError::Internal(format!("PNG optimizer input: {error}")))?;
    if let Some(icc) = &decoded.icc {
        raw.add_icc_profile(icc);
    }
    let mut metadata_removed = false;
    for &(name, data) in &chunks.list {
        match chunk_action(name, data, options.strip_metadata) {
            Action::Keep => raw.add_png_chunk(name, data.to_vec()),
            Action::Replace(data) => {
                metadata_removed = true;
                raw.add_png_chunk(name, data);
            }
            Action::Drop { metadata } => metadata_removed |= metadata,
            Action::Handled => {}
        }
    }
    let mut optimizer = Options::from_preset(options.png_level);
    // Chunks are filtered above; transparent pixels keep their colors so verification can compare them exactly.
    optimizer.strip = StripChunks::None;
    optimizer.optimize_alpha = false;
    optimizer.timeout = Some(limits.png_timeout);
    optimizer.max_decompressed_size = Some(limits.max_png_bytes);
    if options.png_zopfli && decoded.row_bytes <= limits.max_zopfli_bytes {
        optimizer.deflater = Deflater::Zopfli(ZopfliOptions::default());
    }
    let bytes = raw
        .create_optimized_png(&optimizer)
        .map_err(|error| RasterError::Internal(format!("PNG optimizer: {error}")))?;
    drop(raw);

    cancelled(control)?;
    control.report(Progress { stage: Stage::Verifying, done: 0, total: 0 });
    let same_pixels = pixel_hash(&bytes, limits).is_ok_and(|hash| hash == original_hash);
    let candidate = same_pixels.then_some(Candidate { bytes, reencoded: false, metadata_removed });
    Ok(Outcome { size, candidate, kept: Kept::NotSmaller })
}

fn cancelled(control: &dyn Control) -> Result<(), RasterError> {
    if control.is_cancelled() { Err(RasterError::Cancelled) } else { Ok(()) }
}

/// The chunk list of a PNG (ISO/IEC 15948, 5.3), up to IEND.
struct Chunks<'a> {
    list: Vec<([u8; 4], &'a [u8])>,
    /// Bytes after IEND; empty for a plain PNG.
    trailing: &'a [u8],
}

impl<'a> Chunks<'a> {
    fn parse(input: &'a [u8]) -> Result<Self, RasterError> {
        if !input.starts_with(&SIGNATURE) {
            return Err(malformed("missing PNG signature"));
        }
        let mut list = Vec::new();
        let mut pos = SIGNATURE.len();
        loop {
            let header = input.get(pos..pos + 8).ok_or_else(|| malformed("truncated chunk"))?;
            let length = usize::try_from(u32::from_be_bytes([header[0], header[1], header[2], header[3]]))
                .map_err(|_| malformed("chunk too long"))?;
            let name = [header[4], header[5], header[6], header[7]];
            let data = pos
                .checked_add(8 + length)
                .and_then(|end| input.get(pos + 8..end))
                .ok_or_else(|| malformed("truncated chunk"))?;
            pos += 12 + length;
            if pos > input.len() {
                return Err(malformed("truncated chunk"));
            }
            if &name == b"IEND" {
                return Ok(Self { list, trailing: &input[pos..] });
            }
            list.push((name, data));
        }
    }

    fn has(&self, name: &[u8; 4]) -> bool {
        self.list.iter().any(|(chunk, _)| chunk == name)
    }

    fn size(&self) -> Result<(u32, u32), RasterError> {
        match self.list.first() {
            Some((name, data)) if name == b"IHDR" && data.len() >= 8 => Ok((
                u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
                u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            )),
            _ => Err(malformed("missing image header")),
        }
    }
}

enum Action {
    Keep,
    Replace(Vec<u8>),
    /// Left out; `metadata` tells whether that was "Remove metadata" at work.
    Drop {
        metadata: bool,
    },
    /// Rebuilt from the decoded image: header, palette, transparency, data, ICC profile.
    Handled,
}

/// What happens to an ancillary chunk. Color and density chunks always stay. "Remove metadata" removes text, time,
/// EXIF (except orientation) and unknown chunks. Unknown chunks that are not safe to copy (5th bit of the last
/// letter clear) are always dropped: they may depend on the image data, which changes.
fn chunk_action(name: [u8; 4], data: &[u8], strip: bool) -> Action {
    match &name {
        b"IHDR" | b"PLTE" | b"tRNS" | b"IDAT" | b"iCCP" => Action::Handled,
        b"gAMA" | b"cHRM" | b"sRGB" | b"cICP" | b"mDCV" | b"mDCv" | b"cLLI" | b"cLLi" | b"sBIT" | b"bKGD" | b"hIST"
        | b"pHYs" | b"sPLT" => Action::Keep,
        _ if !strip => {
            if matches!(&name, b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" | b"eXIf") || name[3] & 0x20 != 0 {
                Action::Keep
            } else {
                Action::Drop { metadata: false }
            }
        }
        b"eXIf" => match exif::rotation(data) {
            Some(orientation) => Action::Replace(orientation.tiff()),
            None => Action::Drop { metadata: true },
        },
        _ => Action::Drop { metadata: true },
    }
}

struct DecodedRows {
    color_type: ColorType,
    bit_depth: BitDepth,
    icc: Option<Vec<u8>>,
    rows: Vec<u8>,
    row_bytes: usize,
}

/// Decodes the image without transformations: the samples as stored, one packed row after another.
fn decode_rows(input: &[u8], limits: &Limits) -> Result<DecodedRows, RasterError> {
    let mut decoder = Decoder::new_with_limits(Cursor::new(input), png::Limits { bytes: limits.max_png_bytes });
    decoder.set_transformations(Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|error| decoding_error(error, limits))?;
    let length = reader.output_buffer_size();
    if length > limits.max_png_bytes {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let mut rows = vec![0; length];
    let frame = reader.next_frame(&mut rows).map_err(|error| decoding_error(error, limits))?;
    rows.truncate(frame.buffer_size());
    let info = reader.info();
    let trns = info.trns.as_deref();
    // png keeps a transparent gray or RGB key as stored for 16-bit images, and as one byte per sample below that.
    let wide = info.bit_depth == png::BitDepth::Sixteen;
    let key = |samples: usize| -> Option<Vec<u16>> {
        let t = trns?;
        if wide {
            (t.len() >= samples * 2)
                .then(|| t.as_chunks::<2>().0.iter().take(samples).map(|&b| u16::from_be_bytes(b)).collect())
        } else {
            (t.len() >= samples).then(|| t[..samples].iter().map(|&b| u16::from(b)).collect())
        }
    };
    let color_type = match info.color_type {
        png::ColorType::Grayscale => ColorType::Grayscale { transparent_shade: key(1).map(|k| k[0]) },
        png::ColorType::Rgb => ColorType::RGB { transparent_color: key(3).map(|k| RGB16::new(k[0], k[1], k[2])) },
        png::ColorType::Indexed => {
            let palette = info.palette.as_deref().ok_or_else(|| malformed("indexed image without palette"))?;
            ColorType::Indexed {
                palette: palette
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .enumerate()
                    .map(|(i, rgb)| {
                        RGBA8::new(rgb[0], rgb[1], rgb[2], trns.and_then(|t| t.get(i)).copied().unwrap_or(255))
                    })
                    .collect(),
            }
        }
        png::ColorType::GrayscaleAlpha => ColorType::GrayscaleAlpha,
        png::ColorType::Rgba => ColorType::RGBA,
    };
    let bit_depth = match info.bit_depth {
        png::BitDepth::One => BitDepth::One,
        png::BitDepth::Two => BitDepth::Two,
        png::BitDepth::Four => BitDepth::Four,
        png::BitDepth::Eight => BitDepth::Eight,
        png::BitDepth::Sixteen => BitDepth::Sixteen,
    };
    let icc = info.icc_profile.as_deref().map(<[u8]>::to_vec);
    Ok(DecodedRows { color_type, bit_depth, icc, row_bytes: rows.len(), rows })
}

fn decoding_error(error: DecodingError, limits: &Limits) -> RasterError {
    match error {
        DecodingError::LimitsExceeded => RasterError::TooManyPixels { limit: limits.max_pixels },
        other => RasterError::Malformed(other.to_string()),
    }
}

/// Hash of every pixel as 16-bit RGBA, so files that differ only in color type, bit depth, palette or interlacing
/// compare equal exactly when they show the same samples.
fn pixel_hash(input: &[u8], limits: &Limits) -> Result<u64, RasterError> {
    let mut decoder = Decoder::new_with_limits(Cursor::new(input), png::Limits { bytes: limits.max_png_bytes });
    decoder.set_transformations(Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|error| decoding_error(error, limits))?;
    let length = reader.output_buffer_size();
    if length > limits.max_png_bytes {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels });
    }
    let mut samples = vec![0; length];
    let frame = reader.next_frame(&mut samples).map_err(|error| decoding_error(error, limits))?;
    samples.truncate(frame.buffer_size());
    let (color, depth) = reader.output_color_type();
    let channels = color.samples();
    let wide = depth == png::BitDepth::Sixteen;
    let sample_bytes = if wide { 2 } else { 1 };
    let mut hasher = DefaultHasher::new();
    hasher.write_u32(frame.width);
    hasher.write_u32(frame.height);
    let mut buffer = Vec::with_capacity(1 << 16);
    for pixel in samples.chunks_exact(channels * sample_bytes) {
        let sample = |index: usize| {
            if wide {
                u16::from_be_bytes([pixel[index * 2], pixel[index * 2 + 1]])
            } else {
                u16::from(pixel[index]) * 257
            }
        };
        let rgba = match channels {
            1 => [sample(0), sample(0), sample(0), u16::MAX],
            2 => [sample(0), sample(0), sample(0), sample(1)],
            3 => [sample(0), sample(1), sample(2), u16::MAX],
            _ => [sample(0), sample(1), sample(2), sample(3)],
        };
        for value in rgba {
            buffer.extend_from_slice(&value.to_be_bytes());
        }
        if buffer.len() >= 1 << 16 {
            hasher.write(&buffer);
            buffer.clear();
        }
    }
    hasher.write(&buffer);
    Ok(hasher.finish())
}
