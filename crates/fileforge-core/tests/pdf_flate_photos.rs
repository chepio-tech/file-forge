//! Flate photograph conversion and screen-content preservation (ADR-0016); fixtures are generated in code.

mod support;

// Core
use std::io::Write;

use fileforge_core::pdf::{ImageOptions, PdfOptions, compress};
use flate2::{Compression, write::ZlibEncoder};
use image::{DynamicImage, GrayImage, RgbImage, imageops::FilterType};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};
// Utils
use support::{PdfBuilder, decode_jpeg, mean_error};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const MAXIMUM: PdfOptions = PdfOptions {
    images: Some(ImageOptions { jpeg_quality: 70, max_dpi: Some(150), compress_flate_photos: true }),
    ..PdfOptions::LOSSLESS
};

fn photograph(width: u32, height: u32, gray: bool) -> Vec<u8> {
    let channels = if gray { 1 } else { 3 };
    let mut seed = 0x2545_f491_u32;
    let mut samples = Vec::with_capacity((width * height) as usize * channels);
    for y in 0..height {
        for x in 0..width {
            for channel in 0..channels {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let base = 64 + x * 96 / width + y * 48 / height + channel as u32 * 16;
                samples.push((base as i32 + (seed % 24) as i32 - 12) as u8);
            }
        }
    }
    samples
}

fn deflate(samples: &[u8], level: Compression) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), level);
    encoder.write_all(samples).expect("test data");
    encoder.finish().expect("deflated data")
}

fn page(content: Vec<u8>, gray: bool, dpi: u32, edit: impl FnOnce(&mut Dictionary)) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let image = pdf.image(WIDTH, HEIGHT, if gray { "DeviceGray" } else { "DeviceRGB" }, Some("FlateDecode"), content);
    edit(pdf.dict_mut(image));
    let (width_pt, height_pt) = (WIDTH as f64 * 72.0 / dpi as f64, HEIGHT as f64 * 72.0 / dpi as f64);
    pdf.page(&format!("BT (Keep this text) Tj ET q {width_pt} 0 0 {height_pt} 0 0 cm /P Do Q"), &[("P", image)]);
    pdf.bytes()
}

fn image(doc: &Document) -> &Stream {
    doc.objects
        .values()
        .find_map(|object| match object {
            Object::Stream(stream) if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") => {
                Some(stream)
            }
            _ => None,
        })
        .expect("image stream")
}

fn filter(stream: &Stream) -> &[u8] {
    stream.dict.get(b"Filter").and_then(Object::as_name).expect("image filter")
}

fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("valid output")
}

#[test]
fn maximum_converts_rgb_and_gray_photographs_already_at_target_dpi() {
    for gray in [false, true] {
        let samples = photograph(WIDTH, HEIGHT, gray);
        let original_stream = deflate(&samples, Compression::best());
        let input = page(original_stream.clone(), gray, 150, |_| {});
        let output = compress(&input, &MAXIMUM).expect("compression");
        assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 0));
        assert!(output.bytes.len() < input.len() / 4, "{} → {}", input.len(), output.bytes.len());
        let result = load(&output.bytes);
        let stream = image(&result);
        assert_eq!(filter(stream), b"DCTDecode");
        assert!(stream.content.len() * 5 <= original_stream.len() * 4);
        let (width, height, decoded) = decode_jpeg(&stream.content);
        assert_eq!((width as u32, height as u32), (WIDTH, HEIGHT));
        assert!(mean_error(&samples, &decoded) < 8.0);
        let original = load(&input);
        assert_eq!(original.get_pages().len(), result.get_pages().len());
        assert_eq!(
            original.get_page_content(*original.get_pages().values().next().expect("page")),
            result.get_page_content(*result.get_pages().values().next().expect("page"))
        );
        let twice = compress(&output.bytes, &MAXIMUM).expect("recompression");
        assert!(twice.bytes.len() <= output.bytes.len());
        let twice_doc = load(&twice.bytes);
        assert!(mean_error(&samples, &decode_jpeg(&image(&twice_doc).content).2) < 10.0);
    }
}

#[test]
fn conversion_disabled_keeps_photograph_pixels_identical() {
    let samples = photograph(WIDTH, HEIGHT, false);
    let input = page(deflate(&samples, Compression::best()), false, 150, |_| {});
    for options in [
        PdfOptions::LOSSLESS,
        PdfOptions {
            images: Some(ImageOptions { jpeg_quality: 85, max_dpi: Some(200), compress_flate_photos: false }),
            ..MAXIMUM
        },
        PdfOptions {
            images: Some(ImageOptions { compress_flate_photos: false, ..MAXIMUM.images.expect("images") }),
            ..MAXIMUM
        },
    ] {
        let output = compress(&input, &options).expect("compression");
        let result = load(&output.bytes);
        assert_eq!(filter(image(&result)), b"FlateDecode");
        assert_eq!(image(&result).decompressed_content().expect("pixels"), samples);
        assert_eq!(output.report.images_recompressed, 0);
    }
}

#[test]
fn maximum_downsamples_a_photograph_before_jpeg_encoding() {
    let samples = photograph(WIDTH, HEIGHT, false);
    let input = page(deflate(&samples, Compression::best()), false, 300, |_| {});
    let output = compress(&input, &MAXIMUM).expect("compression");
    assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 1));
    let result = load(&output.bytes);
    assert_eq!(filter(image(&result)), b"DCTDecode");
    let (width, height, decoded) = decode_jpeg(&image(&result).content);
    assert_eq!((width, height), (160, 120));
    let expected = DynamicImage::ImageRgb8(RgbImage::from_raw(WIDTH, HEIGHT, samples).expect("pixels"))
        .resize_exact(160, 120, FilterType::Lanczos3)
        .into_rgb8()
        .into_raw();
    assert!(mean_error(&expected, &decoded) < 5.0);
}

fn screen_content(kind: u8, gray: bool) -> Vec<u8> {
    let channels = if gray { 1 } else { 3 };
    let mut samples = photograph(WIDTH, HEIGHT, gray);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let value = match kind {
                // Flat panels, hard text strokes and anti-aliased edges.
                0 => Some(if (x / 8 + y / 12) % 5 == 0 {
                    30
                } else if x % 8 == 1 {
                    160
                } else {
                    245
                }),
                // A continuous gradient background; no limited exact-color palette.
                1 => Some((40 + x * 150 / WIDTH) as u8),
                // A small UI patch in the final partial tile of an otherwise photographic image.
                2 if x >= WIDTH - 12 && y >= HEIGHT - 12 => Some(if x % 3 == 0 { 20 } else { 245 }),
                // Thin black/white line art.
                3 => Some(if x % 7 == 0 || y % 11 == 0 { 0 } else { 255 }),
                _ => None,
            };
            if let Some(value) = value {
                let index = ((y * WIDTH + x) as usize) * channels;
                samples[index..index + channels].fill(value);
            }
        }
    }
    samples
}

#[test]
fn screen_content_stays_deflate_and_keeps_pixels_at_target_dpi() {
    for gray in [false, true] {
        for kind in 0..4 {
            let samples = screen_content(kind, gray);
            let input = page(deflate(&samples, Compression::best()), gray, 150, |_| {});
            let output = compress(&input, &MAXIMUM).expect("compression");
            let result = load(&output.bytes);
            assert_eq!(filter(image(&result)), b"FlateDecode", "screen kind {kind}, gray {gray}");
            assert_eq!(image(&result).decompressed_content().expect("pixels"), samples);
            assert_eq!(output.report.images_recompressed, 0);
            assert!(output.bytes.len() <= input.len());
        }
    }
}

#[test]
fn high_dpi_screen_content_keeps_deflate_with_exact_resampled_pixels() {
    for gray in [false, true] {
        let samples = screen_content(0, gray);
        let image = if gray {
            DynamicImage::ImageLuma8(GrayImage::from_raw(WIDTH, HEIGHT, samples.clone()).expect("pixels"))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_raw(WIDTH, HEIGHT, samples.clone()).expect("pixels"))
        };
        let expected = image.resize_exact(160, 120, FilterType::Lanczos3);
        let input = page(deflate(&samples, Compression::none()), gray, 300, |_| {});
        let output = compress(&input, &MAXIMUM).expect("compression");
        assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 1));
        let result = load(&output.bytes);
        assert_eq!(filter(self::image(&result)), b"FlateDecode");
        assert_eq!(self::image(&result).decompressed_content().expect("pixels"), expected.as_bytes());
    }
}

#[test]
fn jpeg_must_beat_equivalent_deflate_even_if_the_original_is_uncompressed_deflate() {
    let tile = photograph(16, 16, false);
    let samples: Vec<u8> = (0..HEIGHT)
        .flat_map(|y| {
            let tile = &tile;
            (0..WIDTH).flat_map(move |x| {
                let index = ((y % 16 * 16 + x % 16) * 3) as usize;
                tile[index..index + 3].iter().copied()
            })
        })
        .collect();
    let input = page(deflate(&samples, Compression::none()), false, 150, |_| {});
    let output = compress(&input, &MAXIMUM).expect("compression");
    let result = load(&output.bytes);
    assert_eq!(filter(image(&result)), b"FlateDecode");
    assert_eq!(image(&result).decompressed_content().expect("pixels"), samples);
    assert!(output.bytes.len() < input.len() / 4);
}

fn predicted(samples: &[u8], channels: usize, predictor: i64) -> Vec<u8> {
    let stride = WIDTH as usize * channels;
    let mut encoded = Vec::new();
    for (y, row) in samples.chunks(stride).enumerate() {
        if predictor == 15 {
            encoded.push(2); // PNG Up.
        }
        for (index, value) in row.iter().enumerate() {
            let previous = match predictor {
                2 if index >= channels => row[index - channels],
                15 if y > 0 => samples[(y - 1) * stride + index],
                _ => 0,
            };
            encoded.push(value.wrapping_sub(previous));
        }
    }
    encoded
}

#[test]
fn tiff_and_png_predictors_decode_to_the_same_photo_before_conversion() {
    for gray in [false, true] {
        let channels = if gray { 1 } else { 3 };
        let samples = photograph(WIDTH, HEIGHT, gray);
        let plain = page(deflate(&samples, Compression::best()), gray, 150, |_| {});
        let plain_result = compress(&plain, &MAXIMUM).expect("plain compression");
        let plain_doc = load(&plain_result.bytes);
        assert_eq!(filter(image(&plain_doc)), b"DCTDecode");
        for predictor in [2, 15] {
            let input =
                page(deflate(&predicted(&samples, channels, predictor), Compression::best()), gray, 150, |dict| {
                    dict.set(
                        "DecodeParms",
                        dictionary! { "Predictor" => predictor, "Colors" => channels as i64,
                        "Columns" => i64::from(WIDTH), "BitsPerComponent" => 8 },
                    );
                });
            let output = compress(&input, &MAXIMUM).expect("predictor compression");
            let result = load(&output.bytes);
            let stream = image(&result);
            assert_eq!(filter(stream), b"DCTDecode");
            assert!(!stream.dict.has(b"DecodeParms"));
            assert_eq!(decode_jpeg(&stream.content).2, decode_jpeg(&image(&plain_doc).content).2);
        }
    }
}

#[test]
fn unusual_or_malformed_flate_images_are_not_converted() {
    let samples = photograph(WIDTH, HEIGHT, false);
    let content = deflate(&samples, Compression::best());
    type Edit = fn(&mut Dictionary);
    let cases: &[(&str, Edit)] = &[
        ("decode", |d| d.set("Decode", vec![1.into(), 0.into(), 1.into(), 0.into(), 1.into(), 0.into()])),
        ("soft mask", |d| d.set("SMask", Object::Name(b"None".to_vec()))),
        ("color-key mask", |d| d.set("Mask", vec![0.into(), 0.into(), 0.into(), 0.into(), 0.into(), 0.into()])),
        ("indirect parameters", |d| d.set("DecodeParms", Object::Reference((99, 0)))),
        ("array parameters", |d| {
            d.set(
                "DecodeParms",
                vec![Object::Dictionary(dictionary! { "Predictor" => 2, "Columns" => 320, "Colors" => 3 })],
            )
        }),
        ("unknown predictor", |d| d.set("DecodeParms", dictionary! { "Predictor" => 99 })),
        ("oversized predictor", |d| {
            d.set("DecodeParms", dictionary! { "Predictor" => 15, "Columns" => i64::MAX, "Colors" => i64::MAX })
        }),
        ("mismatched predictor", |d| {
            d.set("DecodeParms", dictionary! { "Predictor" => 2, "Columns" => 160, "Colors" => 3 })
        }),
        ("noninteger predictor", |d| d.set("DecodeParms", dictionary! { "Predictor" => Object::string_literal("15") })),
        ("unsupported filter chain", |d| {
            d.set("Filter", vec![Object::Name(b"FlateDecode".to_vec()), Object::Name(b"FlateDecode".to_vec())])
        }),
        ("pixel limit", |d| d.set("Width", i64::MAX)),
        ("tiny image", |d| {
            d.set("Width", 16);
            d.set("Height", 16);
        }),
    ];
    for (label, edit) in cases {
        let input = page(content.clone(), false, 150, edit);
        let output = compress(&input, &MAXIMUM).expect(label);
        assert_eq!(output.report.images_recompressed, 0, "{label}");
        assert!(output.bytes.len() <= input.len());
        let result = load(&output.bytes);
        assert_ne!(
            image(&result).dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(&b"DCTDecode"[..]),
            "{label}"
        );
    }
    for broken in [
        b"invalid deflate".to_vec(),
        deflate(&samples[..100], Compression::best()),
        deflate(&vec![23; samples.len() + HEIGHT as usize + 1], Compression::best()),
    ] {
        let input = page(broken.clone(), false, 150, |_| {});
        let output = compress(&input, &MAXIMUM).expect("invalid stream is skipped");
        assert_eq!(output.report.images_recompressed, 0);
        assert!(output.bytes.len() <= input.len());
    }
}
