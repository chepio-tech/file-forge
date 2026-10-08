//! WebP guarantees (`docs/domain/invariants.md`, ADR-0020) on fixtures built in code.

mod support;

// Core
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

use fileforge_core::raster::{
    Control, Kept, Progress, RasterError, RasterFormat, RasterOptions, Stage, compress, compress_controlled,
};
use fileforge_webp::{Layout, Mode};
use image_webp::{ColorType, WebPDecoder, WebPEncoder};
// Utils
use support::{mean_error, photo};

const LOSSLESS: RasterOptions = RasterOptions::LOSSLESS;
const STRIP: RasterOptions = RasterOptions { strip_metadata: true, ..RasterOptions::LOSSLESS };
const BALANCED: RasterOptions =
    RasterOptions { jpeg_quality: Some(85), webp_quality: Some(85), png_level: 4, ..RasterOptions::LOSSLESS };
const MAXIMUM: RasterOptions = RasterOptions {
    jpeg_quality: Some(75),
    webp_quality: Some(75),
    png_level: 6,
    png_zopfli: true,
    ..RasterOptions::LOSSLESS
};

type Chunks = Vec<([u8; 4], Vec<u8>)>;

// ---- Fixtures ----

/// `photo` plus, with alpha, opaque, half-transparent and fully transparent blocks; transparent pixels keep colors.
fn picture(width: u32, height: u32, layout: Layout) -> Vec<u8> {
    let rgb = photo(width, height, false);
    match layout {
        Layout::Rgb => rgb,
        Layout::Rgba => rgb
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .flat_map(|(index, p)| {
                let (x, y) = (index as u32 % width, index as u32 / width);
                let alpha = [255, 128, 0][((x / 16 + y / 16) % 3) as usize];
                [p[0], p[1], p[2], alpha]
            })
            .collect(),
    }
}

/// A lossless WebP from image-webp's fast encoder: valid, but larger than libwebp's best.
fn weak_lossless(width: u32, height: u32, layout: Layout) -> Vec<u8> {
    let mut out = Vec::new();
    let color = if layout == Layout::Rgba { ColorType::Rgba8 } else { ColorType::Rgb8 };
    WebPEncoder::new(&mut out).encode(&picture(width, height, layout), width, height, color).expect("encodes");
    out
}

fn lossy(width: u32, height: u32, layout: Layout, quality: u8) -> Vec<u8> {
    let mode = Mode::Lossy { quality, method: 4 };
    fileforge_webp::encode(&picture(width, height, layout), width, height, layout, mode, &|| false).expect("encodes")
}

fn chunks(webp: &[u8]) -> Chunks {
    assert_eq!((&webp[..4], &webp[8..12]), (&b"RIFF"[..], &b"WEBP"[..]));
    let size = u32::from_le_bytes([webp[4], webp[5], webp[6], webp[7]]) as usize;
    assert_eq!(size + 8, webp.len(), "RIFF size");
    let mut out = Vec::new();
    let mut pos = 12;
    while pos < webp.len() {
        let id = [webp[pos], webp[pos + 1], webp[pos + 2], webp[pos + 3]];
        let length = u32::from_le_bytes([webp[pos + 4], webp[pos + 5], webp[pos + 6], webp[pos + 7]]) as usize;
        out.push((id, webp[pos + 8..pos + 8 + length].to_vec()));
        pos += 8 + length + length % 2;
    }
    out
}

fn riff(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
    for (id, data) in chunks {
        out.extend_from_slice(id);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
    }
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

fn vp8x(flags: u8, width: u32, height: u32) -> ([u8; 4], Vec<u8>) {
    let mut data = vec![flags, 0, 0, 0];
    data.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    data.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    (*b"VP8X", data)
}

/// `image` (a simple WebP) in an extended container: VP8X, `before`, the image chunks, `after`.
fn extended(image: &[u8], flags: u8, size: (u32, u32), before: &Chunks, after: &Chunks) -> Vec<u8> {
    let mut list = vec![vp8x(flags, size.0, size.1)];
    list.extend(before.iter().cloned());
    list.extend(chunks(image).into_iter().filter(|(id, _)| matches!(id, b"ALPH" | b"VP8 " | b"VP8L")));
    list.extend(after.iter().cloned());
    riff(&list)
}

/// EXIF as a WebP EXIF chunk holds it (a TIFF structure): orientation 6 and a Software tag.
fn exif() -> Vec<u8> {
    let mut tiff = b"MM\0*\0\0\0\x08\0\x02".to_vec();
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
    tiff.extend_from_slice(&[0x01, 0x31, 0, 2, 0, 0, 0, 4, b'a', b'b', b'c', 0]);
    tiff.extend_from_slice(&[0, 0, 0, 0]);
    tiff
}

fn icc() -> Vec<u8> {
    (0..600u32).map(|i| (i * 7 % 251) as u8).collect()
}

fn metadata() -> (Chunks, Chunks) {
    let before = vec![(*b"ICCP", icc())];
    let after = vec![
        (*b"EXIF", exif()),
        (*b"XMP ", b"<x:xmpmeta><dc:creator>Someone</dc:creator></x:xmpmeta>".to_vec()),
        (*b"prVt", b"private data".to_vec()),
    ];
    (before, after)
}

/// Width, height and RGBA samples (opaque without an alpha channel), decoded by image-webp.
fn decode(webp: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = WebPDecoder::new(Cursor::new(webp)).expect("valid WebP");
    let (width, height) = decoder.dimensions();
    let alpha = decoder.has_alpha();
    let mut pixels = vec![0; decoder.output_buffer_size().expect("fits")];
    decoder.read_image(&mut pixels).expect("decodes");
    if !alpha {
        pixels = pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    }
    (width, height, pixels)
}

fn alpha(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect()
}

fn ids(webp: &[u8]) -> Vec<[u8; 4]> {
    chunks(webp).into_iter().map(|(id, _)| id).collect()
}

// ---- Lossless ----

#[test]
fn lossless_webp_keeps_every_sample_and_shrinks_in_every_preset() {
    for layout in [Layout::Rgb, Layout::Rgba] {
        for (width, height) in [(160, 120), (45, 31)] {
            let input = weak_lossless(width, height, layout);
            for options in [LOSSLESS, MAXIMUM] {
                let output = compress(&input, &options).expect("compresses");
                let case = format!("{layout:?} {width}×{height} {:?}", options.webp_quality);
                assert_eq!(output.report.kept, None, "{case}");
                assert_eq!(output.report.format, RasterFormat::Webp);
                assert_eq!((output.report.width, output.report.height), (width, height));
                assert!(!output.report.reencoded, "{case}: lossless WebP must stay lossless");
                assert!(output.bytes.len() < input.len(), "{case}: {} → {}", input.len(), output.bytes.len());
                assert_eq!(output.report.output_size, output.bytes.len() as u64);
                assert!(decode(&output.bytes) == decode(&input), "{case}: pixels changed");
                assert_eq!(ids(&output.bytes), vec![*b"VP8L"], "{case}: a simple file stays simple");
            }
        }
    }
}

#[test]
fn colors_under_full_transparency_survive() {
    let input = weak_lossless(64, 48, Layout::Rgba);
    let (_, _, before) = decode(&input);
    assert!(before.as_chunks::<4>().0.iter().any(|p| p[3] == 0 && p[..3] != [0, 0, 0]), "fixture has hidden colors");
    let output = compress(&input, &LOSSLESS).expect("compresses");
    assert!(decode(&output.bytes).2 == before);
}

#[test]
fn an_optimized_webp_is_returned_byte_for_byte() {
    let once = compress(&weak_lossless(160, 120, Layout::Rgba), &LOSSLESS).expect("compresses").bytes;
    let twice = compress(&once, &LOSSLESS).expect("compresses again");
    assert_eq!(twice.report.kept, Some(Kept::NotSmaller));
    assert_eq!(twice.bytes, once);
}

// ---- Lossy ----

#[test]
fn lossy_webp_keeps_its_pixels_without_a_webp_quality() {
    let input = lossy(160, 120, Layout::Rgb, 95);
    for options in [LOSSLESS, RasterOptions { jpeg_quality: Some(75), ..LOSSLESS }] {
        let output = compress(&input, &options).expect("compresses");
        assert_eq!(output.report.kept, Some(Kept::LossyEncoding));
        assert_eq!(output.bytes, input);
    }
}

#[test]
fn lossy_presets_reencode_high_quality_webps() {
    let input = lossy(320, 240, Layout::Rgb, 95);
    for options in [BALANCED, MAXIMUM] {
        let output = compress(&input, &options).expect("compresses");
        assert_eq!(output.report.kept, None);
        assert!(output.report.reencoded);
        assert!(output.bytes.len() as f64 <= input.len() as f64 * 0.98);
        let (width, height, decoded) = decode(&output.bytes);
        assert_eq!((width, height), (320, 240));
        // Garbage pixels give errors around 50; a quality-75 re-encode of this noisy fixture stays far below.
        let error = mean_error(&decoded, &decode(&input).2);
        assert!(error < 8.0, "mean error {error:.1}");
    }
}

#[test]
fn lossy_reencoding_keeps_transparency_exactly() {
    let input = lossy(160, 120, Layout::Rgba, 95);
    assert_eq!(ids(&input), vec![*b"VP8X", *b"ALPH", *b"VP8 "]);
    let output = compress(&input, &MAXIMUM).expect("compresses");
    assert!(output.report.reencoded);
    assert_eq!(ids(&output.bytes), vec![*b"VP8X", *b"ALPH", *b"VP8 "]);
    assert_eq!(alpha(&decode(&output.bytes).2), alpha(&decode(&input).2));
    assert_eq!(chunks(&output.bytes)[0].1[0], 0x10, "VP8X alpha flag");
}

#[test]
fn lossy_presets_keep_low_quality_webps() {
    let input = lossy(160, 120, Layout::Rgb, 40);
    let output = compress(&input, &BALANCED).expect("compresses");
    assert_eq!(output.report.kept, Some(Kept::NotSmaller));
    assert_eq!(output.bytes, input);
}

// ---- Metadata ----

#[test]
fn webp_metadata_survives_by_default_in_order() {
    let (before, after) = metadata();
    let image = weak_lossless(96, 64, Layout::Rgba);
    let input = extended(&image, 0x20 | 0x10 | 0x08 | 0x04, (96, 64), &before, &after);
    let output = compress(&input, &LOSSLESS).expect("compresses");
    assert_eq!(output.report.kept, None);
    assert!(!output.report.metadata_removed);
    let kept = chunks(&output.bytes);
    assert_eq!(ids(&output.bytes), vec![*b"VP8X", *b"ICCP", *b"VP8L", *b"EXIF", *b"XMP ", *b"prVt"]);
    assert_eq!(kept[0].1, vp8x(0x20 | 0x10 | 0x08 | 0x04, 96, 64).1);
    for (id, data) in before.iter().chain(&after) {
        assert!(kept.iter().any(|(i, d)| i == id && d == data), "{} changed", String::from_utf8_lossy(id));
    }
    assert!(decode(&output.bytes) == decode(&input));
    let mut decoder = WebPDecoder::new(Cursor::new(&output.bytes)).expect("valid");
    assert_eq!(decoder.icc_profile().expect("readable"), Some(icc()));
}

#[test]
fn removing_webp_metadata_keeps_the_profile_and_orientation() {
    let (before, after) = metadata();
    let image = weak_lossless(96, 64, Layout::Rgb);
    let input = extended(&image, 0x20 | 0x08 | 0x04, (96, 64), &before, &after);
    let output = compress(&input, &STRIP).expect("compresses");
    assert!(output.report.metadata_removed);
    assert_eq!(ids(&output.bytes), vec![*b"VP8X", *b"ICCP", *b"VP8L", *b"EXIF"]);
    let kept = chunks(&output.bytes);
    assert_eq!(kept[0].1[0], 0x20 | 0x08, "VP8X flags: ICC and EXIF only");
    assert_eq!(kept[1].1, icc());
    let exif = &kept[3].1;
    assert!(exif.len() == 26 && !exif.windows(3).any(|w| w == b"abc"), "only orientation stays: {exif:?}");
    assert_eq!((&exif[..4], &exif[18..20]), (&b"MM\0*"[..], &[0, 6][..]));
    assert!(decode(&output.bytes) == decode(&input));
}

#[test]
fn exif_with_the_jpeg_prefix_keeps_it() {
    let image = weak_lossless(48, 32, Layout::Rgb);
    let prefixed = [b"Exif\0\0".as_slice(), &exif()].concat();
    let input = extended(&image, 0x08, (48, 32), &vec![], &vec![(*b"EXIF", prefixed)]);
    let output = compress(&input, &STRIP).expect("compresses");
    let exif = &chunks(&output.bytes)[2].1;
    assert_eq!((&exif[..6], exif.len()), (&b"Exif\0\0"[..], 32));
}

#[test]
fn removing_metadata_from_a_lossy_webp_keeps_its_image_data() {
    let image = lossy(96, 64, Layout::Rgb, 90);
    let xmp = vec![(*b"XMP ", vec![b'x'; 3000])];
    let input = extended(&image, 0x04, (96, 64), &vec![], &xmp);
    let output = compress(&input, &STRIP).expect("compresses");
    assert!(output.report.metadata_removed && !output.report.reencoded);
    assert_eq!(output.bytes, image, "back to the simple file");
    let with_profile = extended(&image, 0x20 | 0x04, (96, 64), &vec![(*b"ICCP", icc())], &xmp);
    let output = compress(&with_profile, &STRIP).expect("compresses");
    assert_eq!(ids(&output.bytes), vec![*b"VP8X", *b"ICCP", *b"VP8 "]);
    assert_eq!(chunks(&output.bytes)[2], chunks(&image)[0], "image data untouched");
}

// ---- Kept byte for byte ----

#[test]
fn animated_signed_inconsistent_and_extended_webps_are_never_rewritten() {
    let image = weak_lossless(48, 32, Layout::Rgba);
    let frame = vec![(*b"ANIM", vec![0; 6]), (*b"ANMF", vec![0; 16])];
    let mut trailing = image.clone();
    trailing.extend_from_slice(b"appended data");
    let cases = [
        (extended(&image, 0x10 | 0x02, (48, 32), &vec![], &frame), Kept::Animated),
        (extended(&image, 0x10, (48, 32), &vec![], &vec![(*b"C2PA", b"manifest".to_vec())]), Kept::Signed),
        (trailing, Kept::ExtraData),
        (extended(&image, 0, (48, 32), &vec![], &vec![]), Kept::UnsupportedEncoding),
    ];
    for (index, (input, reason)) in cases.into_iter().enumerate() {
        let output = compress(&input, &MAXIMUM).expect("compresses");
        assert_eq!(output.report.kept, Some(reason), "case {index}");
        assert_eq!(output.bytes, input, "case {index}");
    }
}

#[test]
fn broken_webps_are_refused_without_panicking() {
    for input in [weak_lossless(64, 48, Layout::Rgba), lossy(64, 48, Layout::Rgba, 80)] {
        assert!(matches!(compress(&input[..input.len() / 2], &LOSSLESS), Err(RasterError::Malformed(_))));
        assert!(matches!(compress(&input[..14], &LOSSLESS), Err(RasterError::Malformed(_))));
        for position in (4..input.len()).step_by(29) {
            let mut broken = input.clone();
            broken[position] ^= 0x5A;
            match compress(&broken, &MAXIMUM) {
                Ok(output) => assert!(output.bytes.len() <= broken.len()),
                Err(error) => assert!(
                    matches!(error, RasterError::Malformed(_) | RasterError::TooManyPixels { .. }),
                    "position {position}: {error:?}"
                ),
            }
        }
    }
}

// ---- Control ----

/// Cancels as soon as the engine starts encoding, so only libwebp's own progress callback can notice.
#[derive(Default)]
struct CancelWhenEncoding(AtomicBool);

impl Control for CancelWhenEncoding {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn report(&self, progress: Progress) {
        if progress.stage == Stage::Encoding {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}

#[test]
fn a_compression_cancelled_inside_the_encoder_produces_nothing() {
    for (input, options) in
        [(weak_lossless(256, 192, Layout::Rgba), LOSSLESS), (lossy(256, 192, Layout::Rgb, 95), MAXIMUM)]
    {
        let control = CancelWhenEncoding::default();
        assert_eq!(compress_controlled(&input, &options, &control), Err(RasterError::Cancelled));
    }
}
