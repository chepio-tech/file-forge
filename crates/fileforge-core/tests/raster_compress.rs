//! Image engine guarantees (`docs/domain/invariants.md`, ADR-0019) on JPEG and PNG fixtures built in code.

mod support;

// Core
use fileforge_core::raster::{Control, Kept, RasterError, RasterFormat, RasterOptions, compress, compress_controlled};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use png::chunk::ChunkType;
// Utils
use support::{decode_jpeg, mean_error, photo};

const LOSSLESS: RasterOptions = RasterOptions::LOSSLESS;
const STRIP: RasterOptions = RasterOptions { strip_metadata: true, ..RasterOptions::LOSSLESS };
const BALANCED: RasterOptions = RasterOptions { jpeg_quality: Some(85), png_level: 4, ..RasterOptions::LOSSLESS };
const MAXIMUM: RasterOptions =
    RasterOptions { jpeg_quality: Some(75), png_level: 6, png_zopfli: true, ..RasterOptions::LOSSLESS };

// ---- JPEG fixtures ----

struct JpegSpec {
    width: u16,
    height: u16,
    quality: u8,
    gray: bool,
    sampling: Option<SamplingFactor>,
    restart_interval: Option<u16>,
    progressive: bool,
}

const RGB: JpegSpec = JpegSpec {
    width: 160,
    height: 120,
    quality: 90,
    gray: false,
    sampling: None,
    restart_interval: None,
    progressive: false,
};

/// A JPEG with standard Huffman tables, as most cameras and libraries write it, plus extra APP segments.
fn jpeg_with(spec: &JpegSpec, segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, spec.quality);
    if let Some(sampling) = spec.sampling {
        encoder.set_sampling_factor(sampling);
    }
    if let Some(interval) = spec.restart_interval {
        encoder.set_restart_interval(interval);
    }
    encoder.set_progressive(spec.progressive);
    for (number, data) in segments {
        encoder.add_app_segment(*number, data.clone()).expect("valid APP segment");
    }
    let color = if spec.gray { ColorType::Luma } else { ColorType::Rgb };
    let pixels = photo(u32::from(spec.width), u32::from(spec.height), spec.gray);
    encoder.encode(&pixels, spec.width, spec.height, color).expect("test JPEG encodes");
    out
}

fn jpeg(spec: &JpegSpec) -> Vec<u8> {
    jpeg_with(spec, &[])
}

/// Inserts a marker segment right after SOI.
fn insert_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let length = u16::try_from(payload.len() + 2).expect("small segment");
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// Marker segments before the first scan, as `(marker, payload)`.
fn jpeg_segments(jpeg: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut segments = Vec::new();
    let mut pos = 2;
    while pos + 4 <= jpeg.len() && jpeg[pos] == 0xFF {
        let marker = jpeg[pos + 1];
        let length = usize::from(u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]));
        segments.push((marker, jpeg[pos + 4..pos + 2 + length].to_vec()));
        if marker == 0xDA {
            break;
        }
        pos += 2 + length;
    }
    segments
}

/// EXIF with orientation 6 (rotate 90°) and a Software tag.
fn exif() -> Vec<u8> {
    let mut tiff = b"Exif\0\0MM\0*\0\0\0\x08\0\x02".to_vec();
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
    tiff.extend_from_slice(&[0x01, 0x31, 0, 2, 0, 0, 0, 4, b'a', b'b', b'c', 0]);
    tiff.extend_from_slice(&[0, 0, 0, 0]);
    tiff
}

fn xmp() -> Vec<u8> {
    b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:RDF><dc:creator>Someone</dc:creator></rdf:RDF></x:xmpmeta>".to_vec()
}

fn icc() -> Vec<u8> {
    let mut profile = b"ICC_PROFILE\0\x01\x01".to_vec();
    profile.extend((0..600u32).map(|i| (i * 7 % 251) as u8));
    profile
}

/// A JPEG carrying EXIF, XMP, an ICC profile and a comment.
fn jpeg_with_metadata(spec: &JpegSpec) -> Vec<u8> {
    let jpeg = jpeg_with(spec, &[(1, exif()[..].to_vec()), (1, xmp()), (2, icc())]);
    insert_segment(&jpeg, 0xFE, b"made by a test")
}

fn has_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> bool {
    jpeg_segments(jpeg).iter().any(|(m, p)| *m == marker && p == payload)
}

fn pixels(jpeg: &[u8]) -> Vec<u8> {
    decode_jpeg(jpeg).2
}

// ---- JPEG: lossless ----

#[test]
fn lossless_jpeg_keeps_every_pixel_and_shrinks() {
    let specs = [
        RGB,
        JpegSpec { quality: 80, sampling: Some(SamplingFactor::R_4_2_0), ..RGB },
        JpegSpec { gray: true, ..RGB },
        // Not a multiple of the MCU size: partial MCUs at the right and bottom edges.
        JpegSpec { width: 37, height: 23, sampling: Some(SamplingFactor::R_4_2_2), ..RGB },
        JpegSpec { width: 45, height: 31, sampling: Some(SamplingFactor::R_4_2_0), restart_interval: Some(3), ..RGB },
        JpegSpec { width: 33, height: 9, gray: true, restart_interval: Some(1), ..RGB },
    ];
    for (index, spec) in specs.iter().enumerate() {
        let input = jpeg(spec);
        let output = compress(&input, &LOSSLESS).expect("compresses");
        assert_eq!(output.report.kept, None, "case {index}");
        assert_eq!(output.report.format, RasterFormat::Jpeg);
        assert_eq!((output.report.width, output.report.height), (u32::from(spec.width), u32::from(spec.height)));
        assert!(!output.report.reencoded);
        assert!(output.bytes.len() < input.len(), "case {index}: {} → {}", input.len(), output.bytes.len());
        assert_eq!(output.report.output_size, output.bytes.len() as u64);
        assert_eq!(pixels(&output.bytes), pixels(&input), "case {index}: pixels changed");
    }
}

#[test]
fn an_optimized_jpeg_is_returned_byte_for_byte() {
    let once = compress(&jpeg(&RGB), &LOSSLESS).expect("compresses").bytes;
    let twice = compress(&once, &LOSSLESS).expect("compresses again");
    assert_eq!(twice.report.kept, Some(Kept::NotSmaller));
    assert_eq!(twice.bytes, once);
    assert_eq!(twice.report.output_size, twice.report.original_size);
}

#[test]
fn metadata_survives_by_default() {
    let input = jpeg_with_metadata(&RGB);
    let output = compress(&input, &LOSSLESS).expect("compresses");
    assert!(output.bytes.len() < input.len());
    assert!(!output.report.metadata_removed);
    for (marker, payload) in jpeg_segments(&input).into_iter().filter(|(m, _)| (0xE0..=0xEF).contains(m) || *m == 0xFE)
    {
        assert!(has_segment(&output.bytes, marker, &payload), "segment {marker:#x} lost");
    }
}

#[test]
fn removing_metadata_keeps_the_profile_density_and_orientation() {
    let input = jpeg_with_metadata(&RGB);
    let output = compress(&input, &STRIP).expect("compresses");
    assert!(output.report.metadata_removed);
    assert!(has_segment(&output.bytes, 0xE2, &icc()), "ICC profile removed");
    assert!(jpeg_segments(&output.bytes).iter().any(|(m, p)| *m == 0xE0 && p.starts_with(b"JFIF\0")), "JFIF removed");
    assert!(!has_segment(&output.bytes, 0xE1, &xmp()), "XMP kept");
    assert!(!jpeg_segments(&output.bytes).iter().any(|(m, _)| *m == 0xFE), "comment kept");
    let exif: Vec<Vec<u8>> = jpeg_segments(&output.bytes)
        .into_iter()
        .filter(|(m, p)| *m == 0xE1 && p.starts_with(b"Exif"))
        .map(|s| s.1)
        .collect();
    assert_eq!(exif.len(), 1);
    assert!(exif[0].len() < 40 && !exif[0].windows(3).any(|w| w == b"abc"), "only orientation stays: {:?}", exif[0]);
    assert_eq!(&exif[0][6..10], b"MM\0*", "original byte order");
    assert_eq!(&exif[0][24..26], &[0, 6], "orientation 6");
    assert_eq!(pixels(&output.bytes), pixels(&input));
}

#[test]
fn removing_metadata_without_orientation_drops_exif_entirely() {
    let mut upright = exif();
    upright[25] = 1;
    let input = jpeg_with(&RGB, &[(1, upright)]);
    let output = compress(&input, &STRIP).expect("compresses");
    assert!(!jpeg_segments(&output.bytes).iter().any(|(m, _)| *m == 0xE1));
}

#[test]
fn progressive_jpegs_keep_their_scans_and_can_still_lose_metadata() {
    let input = insert_segment(&jpeg(&JpegSpec { progressive: true, ..RGB }), 0xFE, &[b'x'; 2000]);
    let kept = compress(&input, &LOSSLESS).expect("compresses");
    assert_eq!(kept.report.kept, Some(Kept::UnsupportedEncoding));
    assert_eq!(kept.bytes, input);
    let stripped = compress(&input, &STRIP).expect("compresses");
    assert_eq!(stripped.report.kept, None);
    assert_eq!(stripped.bytes.len(), input.len() - 2004);
    assert_eq!(pixels(&stripped.bytes), pixels(&input));
}

// ---- JPEG: lossy ----

#[test]
fn lossy_presets_reencode_high_quality_sources_and_keep_metadata() {
    let input = jpeg_with_metadata(&JpegSpec { quality: 95, width: 320, height: 240, ..RGB });
    let lossless = compress(&input, &LOSSLESS).expect("compresses");
    let output = compress(&input, &MAXIMUM).expect("compresses");
    assert!(output.report.reencoded);
    assert!(output.bytes.len() as f64 <= lossless.bytes.len() as f64 * 0.98);
    assert!(has_segment(&output.bytes, 0xE2, &icc()));
    assert!(has_segment(&output.bytes, 0xE1, &exif()));
    assert!(has_segment(&output.bytes, 0xE1, &xmp()));
    let (width, height, decoded) = decode_jpeg(&output.bytes);
    assert_eq!((width, height), (320, 240));
    // Garbage pixels (a misread JPEG) give errors around 50; quality 75 on this noisy fixture stays near 6.
    let error = mean_error(&decoded, &pixels(&input));
    assert!(error < 10.0, "mean error {error:.1}");
}

#[test]
fn compressing_our_own_output_again_keeps_the_pixels() {
    let original = jpeg(&JpegSpec { quality: 95, width: 320, height: 240, ..RGB });
    let once = compress(&original, &MAXIMUM).expect("first pass");
    let twice = compress(&once.bytes, &RasterOptions { jpeg_quality: Some(60), ..MAXIMUM }).expect("second pass");
    assert!(twice.report.reencoded);
    let error = mean_error(&pixels(&twice.bytes), &photo(320, 240, false));
    assert!(error < 10.0, "mean error {error:.1} per sample after two passes");
}

#[test]
fn lossy_presets_keep_low_quality_sources_lossless() {
    let input = jpeg(&JpegSpec { quality: 50, ..RGB });
    let output = compress(&input, &BALANCED).expect("compresses");
    assert!(!output.report.reencoded, "re-encoding a quality-50 JPEG at 85 only grows it");
    assert_eq!(pixels(&output.bytes), pixels(&input));
}

#[test]
fn grayscale_and_subsampled_sources_keep_their_layout_when_reencoded() {
    for spec in [
        JpegSpec { gray: true, quality: 95, ..RGB },
        JpegSpec { quality: 95, sampling: Some(SamplingFactor::R_4_2_0), ..RGB },
    ] {
        let input = jpeg(&spec);
        let output = compress(&input, &MAXIMUM).expect("compresses");
        assert!(output.report.reencoded);
        let components = |jpeg: &[u8]| {
            jpeg_segments(jpeg).into_iter().find(|(m, _)| matches!(m, 0xC0..=0xC2)).map(|(_, p)| p[6..].to_vec())
        };
        let sampling = |p: Option<Vec<u8>>| p.map(|p| p.chunks(3).map(|c| c[1]).collect::<Vec<_>>());
        assert_eq!(sampling(components(&output.bytes)), sampling(components(&input)));
    }
}

// ---- JPEG: files that stay as they are ----

#[test]
fn extra_data_after_the_image_is_never_rewritten() {
    let mut input = jpeg(&RGB);
    input.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE1, 0, 4, 1, 2, 0xFF, 0xD9]);
    let output = compress(&input, &MAXIMUM).expect("compresses");
    assert_eq!(output.report.kept, Some(Kept::ExtraData));
    assert_eq!(output.bytes, input);
}

#[test]
fn content_credentials_are_never_rewritten() {
    let mut jumbf = b"JP\0\x01\0\0\0\x01\0\0\0\x20jumb\0\0\0\x18jumdc2pa".to_vec();
    jumbf.extend_from_slice(&[0; 16]);
    let input = jpeg_with(&RGB, &[(11, jumbf)]);
    for options in [LOSSLESS, STRIP, MAXIMUM] {
        let output = compress(&input, &options).expect("compresses");
        assert_eq!(output.report.kept, Some(Kept::Signed));
        assert_eq!(output.bytes, input);
    }
}

#[test]
fn broken_jpegs_are_refused_or_handled_without_panicking() {
    let input = jpeg(&JpegSpec { restart_interval: Some(2), ..RGB });
    assert!(matches!(compress(&input[..input.len() / 2], &LOSSLESS), Err(RasterError::Malformed(_))));
    assert!(matches!(compress(&input[..3], &LOSSLESS), Err(RasterError::Malformed(_))));
    let scan = input.windows(2).rposition(|w| w == [0xFF, 0xDA]).expect("has a scan") + 14;
    for position in (scan..input.len() - 2).step_by(37) {
        for value in [0x00, 0xFF, 0x5A] {
            let mut broken = input.clone();
            broken[position] = value;
            for options in [LOSSLESS, MAXIMUM] {
                match compress(&broken, &options) {
                    Ok(output) => {
                        assert!(output.bytes.len() <= broken.len());
                        if output.report.kept.is_none() && !output.report.reencoded {
                            assert_eq!(pixels(&output.bytes), pixels(&broken), "byte {position} = {value:#x}");
                        }
                    }
                    Err(error) => assert!(matches!(error, RasterError::Malformed(_)), "{error:?}"),
                }
            }
        }
    }
}

// ---- PNG fixtures ----

struct PngSpec<'a> {
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    palette: Option<Vec<u8>>,
    trns: Option<Vec<u8>>,
    chunks: &'a [([u8; 4], &'a [u8])],
}

/// An unfiltered, fast-compressed PNG with extra chunks before the image data.
fn png_bytes(spec: &PngSpec, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, spec.width, spec.height);
        encoder.set_color(spec.color);
        encoder.set_depth(spec.depth);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::FilterType::NoFilter);
        if let Some(palette) = &spec.palette {
            encoder.set_palette(palette.clone());
        }
        if let Some(trns) = &spec.trns {
            encoder.set_trns(trns.clone());
        }
        let mut writer = encoder.write_header().expect("header");
        for (name, chunk) in spec.chunks {
            writer.write_chunk(ChunkType(*name), chunk).expect("chunk");
        }
        writer.write_image_data(data).expect("image data");
        writer.finish().expect("finish");
    }
    out
}

fn rgba_png(chunks: &[([u8; 4], &[u8])]) -> Vec<u8> {
    let rgb = photo(96, 64, false);
    let rgba: Vec<u8> = rgb.chunks(3).enumerate().flat_map(|(i, p)| [p[0], p[1], p[2], (i % 256) as u8]).collect();
    let spec = PngSpec {
        width: 96,
        height: 64,
        color: png::ColorType::Rgba,
        depth: png::BitDepth::Eight,
        palette: None,
        trns: None,
        chunks,
    };
    png_bytes(&spec, &rgba)
}

/// Every pixel as 16-bit RGBA, decoded independently of the engine.
fn png_pixels(bytes: &[u8]) -> (u32, u32, Vec<u16>) {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("valid PNG");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).expect("decodes");
    let (color, depth) = reader.output_color_type();
    let channels = color.samples();
    let samples: Vec<u16> = if depth == png::BitDepth::Sixteen {
        buffer[..frame.buffer_size()].chunks(2).map(|b| u16::from_be_bytes([b[0], b[1]])).collect()
    } else {
        buffer[..frame.buffer_size()].iter().map(|&b| u16::from(b) * 257).collect()
    };
    let rgba = samples
        .chunks(channels)
        .flat_map(|s| match s.len() {
            1 => [s[0], s[0], s[0], u16::MAX],
            2 => [s[0], s[0], s[0], s[1]],
            3 => [s[0], s[1], s[2], u16::MAX],
            _ => [s[0], s[1], s[2], s[3]],
        })
        .collect();
    (frame.width, frame.height, rgba)
}

fn png_chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut chunks = Vec::new();
    let mut pos = 8;
    while pos + 8 <= bytes.len() {
        let length = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let name = [bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]];
        chunks.push((name, bytes[pos + 8..pos + 8 + length].to_vec()));
        pos += 12 + length;
    }
    chunks
}

fn has_chunk(bytes: &[u8], name: &[u8; 4]) -> bool {
    png_chunks(bytes).iter().any(|(n, _)| n == name)
}

// ---- PNG ----

#[test]
fn lossless_png_keeps_every_pixel_and_shrinks() {
    let few_colors: Vec<u8> = (0..64 * 48).flat_map(|i| [(i % 4) as u8 * 60, 20, 200]).collect();
    let gray16: Vec<u8> = (0..40 * 30u32).flat_map(|i| ((i * 37 % 4096) as u16 * 16).to_be_bytes()).collect();
    let gray_with_key: Vec<u8> = (0..40 * 30u32).map(|i| (i % 7) as u8 * 30).collect();
    let indexed: Vec<u8> = (0..32 * 32u32).map(|i| (i / 3 % 4) as u8).collect();
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (rgba_png(&[]), "RGBA"),
        (
            png_bytes(
                &PngSpec {
                    width: 64,
                    height: 48,
                    color: png::ColorType::Rgb,
                    depth: png::BitDepth::Eight,
                    palette: None,
                    trns: None,
                    chunks: &[],
                },
                &few_colors,
            ),
            "RGB with four colors (palette reduction)",
        ),
        (
            png_bytes(
                &PngSpec {
                    width: 40,
                    height: 30,
                    color: png::ColorType::Grayscale,
                    depth: png::BitDepth::Sixteen,
                    palette: None,
                    trns: None,
                    chunks: &[],
                },
                &gray16,
            ),
            "16-bit gray",
        ),
        (
            png_bytes(
                &PngSpec {
                    width: 40,
                    height: 30,
                    color: png::ColorType::Grayscale,
                    depth: png::BitDepth::Eight,
                    palette: None,
                    trns: Some(vec![0, 60]),
                    chunks: &[],
                },
                &gray_with_key,
            ),
            "gray with a transparent shade",
        ),
        (
            png_bytes(
                &PngSpec {
                    width: 64,
                    height: 48,
                    color: png::ColorType::Rgb,
                    depth: png::BitDepth::Eight,
                    palette: None,
                    trns: Some(vec![0, 60, 0, 20, 0, 200]),
                    chunks: &[],
                },
                &few_colors,
            ),
            "RGB with a transparent color",
        ),
        (
            png_bytes(
                &PngSpec {
                    width: 32,
                    height: 32,
                    color: png::ColorType::Indexed,
                    depth: png::BitDepth::Eight,
                    palette: Some(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9, 7, 7, 7]),
                    trns: Some(vec![255, 128]),
                    chunks: &[],
                },
                &indexed,
            ),
            "indexed with alpha and an unused entry",
        ),
    ];
    for (input, name) in cases {
        for options in [LOSSLESS, MAXIMUM] {
            let output = compress(&input, &options).expect("compresses");
            assert_eq!(output.report.format, RasterFormat::Png);
            assert_eq!(output.report.kept, None, "{name}");
            assert!(output.bytes.len() < input.len(), "{name}: {} → {}", input.len(), output.bytes.len());
            assert_eq!(png_pixels(&output.bytes), png_pixels(&input), "{name}: pixels changed");
        }
    }
}

#[test]
fn an_optimized_png_is_returned_byte_for_byte() {
    let once = compress(&rgba_png(&[]), &MAXIMUM).expect("compresses").bytes;
    let twice = compress(&once, &LOSSLESS).expect("compresses again");
    assert_eq!(twice.report.kept, Some(Kept::NotSmaller));
    assert_eq!(twice.bytes, once);
}

const ORIENTATION_6: &[u8] = b"MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";

#[test]
fn png_metadata_survives_by_default_and_unsafe_unknown_chunks_go() {
    let chunks: &[([u8; 4], &[u8])] = &[
        (*b"gAMA", &[0, 0, 0xB1, 0x8F]),
        (*b"pHYs", &[0, 0, 0x0B, 0x13, 0, 0, 0x0B, 0x13, 1]),
        (*b"tEXt", b"Author\0Someone"),
        (*b"eXIf", ORIENTATION_6),
        (*b"prVt", b"safe to copy"),
        (*b"prVT", b"depends on the image data"),
    ];
    let input = rgba_png(chunks);
    let output = compress(&input, &LOSSLESS).expect("compresses");
    assert!(!output.report.metadata_removed);
    let kept = png_chunks(&output.bytes);
    for (name, data) in &chunks[..5] {
        assert!(kept.iter().any(|(n, d)| n == name && d == data), "{} lost", String::from_utf8_lossy(name));
    }
    assert!(!has_chunk(&output.bytes, b"prVT"));
}

#[test]
fn removing_png_metadata_keeps_color_density_and_orientation() {
    let mut exif = ORIENTATION_6.to_vec();
    exif.extend_from_slice(b"padding that is not orientation");
    let chunks: &[([u8; 4], &[u8])] = &[
        (*b"gAMA", &[0, 0, 0xB1, 0x8F]),
        (*b"pHYs", &[0, 0, 0x0B, 0x13, 0, 0, 0x0B, 0x13, 1]),
        (*b"tEXt", b"Author\0Someone"),
        (*b"iTXt", b"Comment\0\0\0\0\0hello"),
        (*b"tIME", &[0x07, 0xEA, 10, 8, 12, 0, 0]),
        (*b"eXIf", &exif),
        (*b"prVt", b"private"),
    ];
    let input = rgba_png(chunks);
    let output = compress(&input, &STRIP).expect("compresses");
    assert!(output.report.metadata_removed);
    assert!(has_chunk(&output.bytes, b"gAMA") && has_chunk(&output.bytes, b"pHYs"));
    for name in [b"tEXt", b"iTXt", b"tIME", b"prVt"] {
        assert!(!has_chunk(&output.bytes, name), "{} kept", String::from_utf8_lossy(name));
    }
    let exif: Vec<Vec<u8>> = png_chunks(&output.bytes).into_iter().filter(|(n, _)| n == b"eXIf").map(|c| c.1).collect();
    assert_eq!(exif, vec![ORIENTATION_6.to_vec()]);
}

#[test]
fn png_color_profiles_survive() {
    let profile: Vec<u8> = (0..2000u32).map(|i| (i * 13 % 251) as u8).collect();
    let mut iccp = b"test profile\0\0".to_vec();
    let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut deflate, &profile).expect("deflates");
    iccp.extend(deflate.finish().expect("deflates"));
    let input = rgba_png(&[(*b"iCCP", &iccp)]);
    for options in [LOSSLESS, STRIP] {
        let output = compress(&input, &options).expect("compresses");
        let mut decoder = png::Decoder::new(&output.bytes[..]);
        decoder.set_transformations(png::Transformations::IDENTITY);
        let reader = decoder.read_info().expect("valid PNG");
        assert_eq!(reader.info().icc_profile.as_deref(), Some(&profile[..]));
    }
}

#[test]
fn animated_signed_and_extended_pngs_are_never_rewritten() {
    let mut trailing = rgba_png(&[]);
    trailing.extend_from_slice(b"appended data");
    let cases = [
        (rgba_png(&[(*b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0])]), Kept::Animated),
        (rgba_png(&[(*b"caBX", b"c2pa manifest")]), Kept::Signed),
        (rgba_png(&[(*b"dSIG", b"signature")]), Kept::Signed),
        (trailing, Kept::ExtraData),
    ];
    for (input, reason) in cases {
        let output = compress(&input, &MAXIMUM).expect("compresses");
        assert_eq!(output.report.kept, Some(reason));
        assert_eq!(output.bytes, input);
    }
}

#[test]
fn broken_pngs_are_refused_without_panicking() {
    let input = rgba_png(&[]);
    assert!(matches!(compress(&input[..input.len() / 2], &LOSSLESS), Err(RasterError::Malformed(_))));
    assert!(matches!(compress(&input[..20], &LOSSLESS), Err(RasterError::Malformed(_))));
    for position in (8..input.len()).step_by(53) {
        let mut broken = input.clone();
        broken[position] ^= 0x5A;
        match compress(&broken, &LOSSLESS) {
            Ok(output) => assert!(output.bytes.len() <= broken.len()),
            Err(error) => {
                assert!(matches!(error, RasterError::Malformed(_) | RasterError::TooManyPixels { .. }), "{error:?}")
            }
        }
    }
}

// ---- Control ----

struct Cancelled;

impl Control for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn a_cancelled_compression_produces_nothing() {
    for input in [jpeg(&RGB), rgba_png(&[])] {
        assert_eq!(compress_controlled(&input, &MAXIMUM, &Cancelled), Err(RasterError::Cancelled));
    }
}
