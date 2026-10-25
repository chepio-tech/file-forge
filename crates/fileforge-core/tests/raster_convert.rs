//! Conversion contracts on generated fixtures; no external files or model data.
// Core
use fileforge_core::raster::background::{self, decode};
use fileforge_core::raster::convert::{Options, OutputFormat, convert};
use fileforge_core::raster::{Control, Progress, RasterError, RasterFormat, Stage};
use image::{Rgba, RgbaImage, imageops};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

fn picture() -> RgbaImage {
    RgbaImage::from_fn(24, 16, |x, y| Rgba([(x * 7) as u8, (y * 11) as u8, 63, [0, 128, 255][(x % 3) as usize]]))
}
fn png(pixels: &RgbaImage) -> Vec<u8> {
    background::png_bytes(pixels, None).expect("fixture")
}
fn options(format: OutputFormat) -> Options {
    Options { format, ..Options::default() }
}
fn chunked_png(name: [u8; 4], data: &[u8]) -> Vec<u8> {
    let pixels = picture();
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, pixels.width(), pixels.height());
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().expect("header");
    writer.write_chunk(png::chunk::ChunkType(name), data).expect("chunk");
    writer.write_image_data(pixels.as_raw()).expect("pixels");
    drop(writer);
    bytes
}

#[test]
fn converts_every_input_to_every_output_and_keeps_lossless_pixels() {
    let source = picture();
    let png = png(&source);
    let webp = background::encode(&source, None, background::OutputFormat::Webp, &()).expect("webp");
    let jpeg = convert(&png, &options(OutputFormat::Jpeg), &()).expect("jpeg").bytes;
    for input in [png, webp, jpeg] {
        let before = decode(&input).expect("source");
        for format in [OutputFormat::Png, OutputFormat::Webp, OutputFormat::Jpeg] {
            let out = convert(&input, &options(format), &()).expect("conversion");
            let after = decode(&out.bytes).expect("output");
            assert_eq!(out.report.format, format);
            assert_eq!(out.report.original_size, input.len() as u64);
            assert_eq!(out.report.output_size, out.bytes.len() as u64);
            assert_eq!(after.pixels.dimensions(), before.pixels.dimensions());
            if format != OutputFormat::Jpeg {
                assert_eq!(after.pixels, before.pixels);
            }
            assert_eq!(
                RasterFormat::detect(&out.bytes).expect("format"),
                match format {
                    OutputFormat::Jpeg => RasterFormat::Jpeg,
                    OutputFormat::Png => RasterFormat::Png,
                    OutputFormat::Webp => RasterFormat::Webp,
                }
            );
        }
    }
}

#[test]
fn jpeg_uses_the_requested_background_and_reports_flattening() {
    let input = png(&RgbaImage::from_pixel(16, 16, Rgba([255, 0, 0, 0])));
    for background in [[255; 3], [0; 3], [50, 100, 200]] {
        let out = convert(&input, &Options { background, ..options(OutputFormat::Jpeg) }, &()).expect("jpeg");
        assert!(out.report.flattened);
        let decoded = decode(&out.bytes).expect("decode");
        for pixel in decoded.pixels.pixels() {
            for c in 0..3 {
                assert!(pixel[c].abs_diff(background[c]) <= 3);
            }
            assert_eq!(pixel[3], 255);
        }
    }
}

#[test]
fn orientation_is_applied_once_and_other_metadata_is_removed() {
    let tiff = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let input = chunked_png(*b"eXIf", tiff);
    for format in [OutputFormat::Png, OutputFormat::Webp] {
        let output = convert(&input, &options(format), &()).expect("convert");
        let decoded = decode(&output.bytes).expect("decode");
        assert_eq!(decoded.pixels, imageops::rotate90(&picture()));
        assert_eq!((output.report.width, output.report.height), (16, 24));
        let again = convert(&output.bytes, &options(OutputFormat::Png), &()).expect("again");
        assert_eq!(decode(&again.bytes).expect("again decoded").pixels, decoded.pixels);
    }
    let input = chunked_png(*b"tEXt", b"Comment\0private location");
    let out = convert(&input, &Options::default(), &()).expect("strip");
    let reader = png::Decoder::new(Cursor::new(out.bytes)).read_info().expect("read");
    assert!(reader.info().uncompressed_latin1_text.is_empty());
}

#[test]
fn preserves_rgb_icc_in_every_target_and_rejects_non_rgb_profiles() {
    let mut profile = vec![0; 160];
    profile[0..4].copy_from_slice(&160u32.to_be_bytes());
    profile[16..20].copy_from_slice(b"RGB ");
    for input_format in [background::OutputFormat::Png, background::OutputFormat::Webp] {
        let input = background::encode(&picture(), Some(&profile), input_format, &()).expect("profile fixture");
        for target in [OutputFormat::Jpeg, OutputFormat::Png, OutputFormat::Webp] {
            let output = convert(&input, &options(target), &()).expect("profile conversion");
            assert_eq!(decode(&output.bytes).expect("profile decoded").icc, Some(profile.clone()));
        }
    }
    profile[16..20].copy_from_slice(b"GRAY");
    let input = background::png_bytes(&picture(), Some(&profile)).expect("profile fixture");
    assert!(matches!(convert(&input, &Options::default(), &()), Err(RasterError::Unsupported(_))));
}

#[test]
fn refuses_precision_loss_animation_signatures_extra_images_and_custom_gamma() {
    for name in [*b"acTL", *b"caBX", *b"dSIG"] {
        let input = chunked_png(name, &[0; 8]);
        assert!(matches!(convert(&input, &Options::default(), &()), Err(RasterError::Unsupported(_))));
    }
    let input = chunked_png(*b"gAMA", &100000u32.to_be_bytes());
    assert!(matches!(convert(&input, &Options::default(), &()), Err(RasterError::Unsupported(_))));
    let mut sixteen = Vec::new();
    let mut encoder = png::Encoder::new(&mut sixteen, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder.write_header().expect("header").write_image_data(&[0; 8]).expect("pixels");
    assert!(matches!(convert(&sixteen, &Options::default(), &()), Err(RasterError::Unsupported(_))));
    let source = png(&picture());
    for format in [OutputFormat::Png, OutputFormat::Webp, OutputFormat::Jpeg] {
        let mut bytes = convert(&source, &options(format), &()).expect("fixture").bytes;
        bytes.extend_from_slice(b"extra image");
        assert!(matches!(convert(&bytes, &Options::default(), &()), Err(RasterError::Unsupported(_))));
    }
}

#[test]
fn refuses_oversized_headers_and_target_dimensions() {
    let mut large = Vec::new();
    {
        let mut writer = png::Encoder::new(&mut large, 10000, 10000).write_header().expect("header");
        writer.write_chunk(png::chunk::IDAT, &[]).expect("empty data");
    }
    assert!(matches!(convert(&large, &Options::default(), &()), Err(RasterError::TooManyPixels { .. })));
    for (width, format) in [(16384, OutputFormat::Webp), (65536, OutputFormat::Jpeg)] {
        let input = png(&RgbaImage::from_pixel(width, 1, Rgba([0; 4])));
        assert!(matches!(convert(&input, &options(format), &()), Err(RasterError::Unsupported(_))));
    }
}

#[test]
fn invalid_options_and_cancellation_never_return_a_result() {
    let input = png(&picture());
    assert!(matches!(
        convert(&input, &Options { jpeg_quality: 0, ..Options::default() }, &()),
        Err(RasterError::InvalidOptions(_))
    ));
    assert!(
        serde_json::from_str::<Options>(r#"{"format":"png","jpegQuality":90,"background":[0,0,0],"path":"input.png"}"#)
            .is_err()
    );
    struct Stop {
        stopped: AtomicBool,
        stage: Stage,
    }
    impl Control for Stop {
        fn is_cancelled(&self) -> bool {
            self.stopped.load(Ordering::Relaxed)
        }
        fn report(&self, p: Progress) {
            if p.stage == self.stage {
                self.stopped.store(true, Ordering::Relaxed);
            }
        }
    }
    for stage in [Stage::Loading, Stage::Encoding, Stage::Verifying] {
        let control = Stop { stopped: AtomicBool::new(false), stage };
        assert!(matches!(convert(&input, &Options::default(), &control), Err(RasterError::Cancelled)));
    }
}

#[test]
fn truncated_and_mutated_inputs_never_panic() {
    let source = png(&picture());
    for format in [OutputFormat::Png, OutputFormat::Jpeg, OutputFormat::Webp] {
        let source = convert(&source, &options(format), &()).expect("fixture").bytes;
        for len in (0..source.len()).step_by((source.len() / 32).max(1)) {
            assert!(convert(&source[..len], &Options::default(), &()).is_err());
        }
        for i in (0..source.len()).step_by((source.len() / 32).max(1)) {
            let mut input = source.clone();
            input[i] ^= 0xa5;
            let _ = convert(&input, &Options::default(), &());
        }
    }
}

#[test]
fn conversion_keeps_the_requested_format_even_when_output_is_larger() {
    let mut jpeg = Vec::new();
    let pixels: Vec<u8> = (0..64 * 64 * 3).map(|i| ((i * 73 + i / 17) % 251) as u8).collect();
    jpeg_encoder::Encoder::new(&mut jpeg, 30).encode(&pixels, 64, 64, jpeg_encoder::ColorType::Rgb).expect("jpeg");
    let out = convert(&jpeg, &Options::default(), &()).expect("convert");
    assert!(out.bytes.len() > jpeg.len());
    assert_eq!(RasterFormat::detect(&out.bytes), Some(RasterFormat::Png));
}

#[test]
fn all_eight_orientations_keep_the_expected_pixel_positions() {
    let expected: &[&[u8]] = &[
        &[0, 1, 2, 3, 4, 5],
        &[2, 1, 0, 5, 4, 3],
        &[5, 4, 3, 2, 1, 0],
        &[3, 4, 5, 0, 1, 2],
        &[0, 3, 1, 4, 2, 5],
        &[3, 0, 4, 1, 5, 2],
        &[5, 2, 4, 1, 3, 0],
        &[2, 5, 1, 4, 0, 3],
    ];
    for orientation in 1..=8u8 {
        let source = RgbaImage::from_fn(3, 2, |x, y| Rgba([(y * 3 + x) as u8, 0, 0, 255]));
        let mut tiff = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x01\0\0\0\0\0\0\0".to_vec();
        tiff[18] = orientation;
        let mut input = Vec::new();
        let mut encoder = png::Encoder::new(&mut input, 3, 2);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().expect("header");
        writer.write_chunk(png::chunk::eXIf, &tiff).expect("exif");
        writer.write_image_data(source.as_raw()).expect("pixels");
        drop(writer);
        let output = convert(&input, &Options::default(), &()).expect("convert");
        let decoded = decode(&output.bytes).expect("decode");
        assert_eq!(decoded.pixels.pixels().map(|p| p[0]).collect::<Vec<_>>(), expected[usize::from(orientation - 1)]);
    }
}

#[test]
fn signed_jpeg_and_webp_are_refused_before_decoding() {
    let mut jpeg = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, 90);
    encoder.add_app_segment(11, b"JP\0\x01\0\0\0\x01jumbc2pa".to_vec()).expect("signature marker");
    encoder.encode(&[100; 8 * 8 * 3], 8, 8, jpeg_encoder::ColorType::Rgb).expect("jpeg");
    assert!(matches!(convert(&jpeg, &Options::default(), &()), Err(RasterError::Unsupported(_))));
    let mut webp = background::encode(&picture(), None, background::OutputFormat::Webp, &()).expect("webp");
    webp.extend_from_slice(b"C2PA\x04\0\0\0test");
    let size = (webp.len() - 8) as u32;
    webp[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(matches!(convert(&webp, &Options::default(), &()), Err(RasterError::Unsupported(_))));
}
