//! The encoder's guarantees (ADR-0020): exact lossless pixels, lossless alpha, input checked before libwebp runs,
//! stoppable encodes, no unwinding into C.

// Core
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use fileforge_webp::{EncodeError, Layout, MAX_DIMENSION, Mode, encode};
use image_webp::WebPDecoder;

const NEVER: &(dyn Fn() -> bool + Sync) = &|| false;
const LOSSLESS: Mode = Mode::Lossless { level: 6 };
const LOSSY: Mode = Mode::Lossy { quality: 75, method: 4 };

/// Smooth gradients with a little noise. With alpha: opaque, half-transparent and fully transparent areas, where
/// fully transparent pixels still carry colors.
fn picture(width: u32, height: u32, layout: Layout) -> Vec<u8> {
    let mut noise = 0x2545_F491_u32;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * layout.channels());
    for y in 0..height {
        for x in 0..width {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let jitter = (noise % 7) as u8;
            pixels.push(((x * 255) / width.max(1)) as u8 ^ jitter);
            pixels.push(((y * 255) / height.max(1)) as u8);
            pixels.push(((x + y) % 256) as u8 / 2 + jitter);
            if layout == Layout::Rgba {
                pixels.push(match (x / 8 + y / 8) % 3 {
                    0 => 255,
                    1 => (x * 7 % 256) as u8,
                    _ => 0,
                });
            }
        }
    }
    pixels
}

/// Width, height and RGBA samples as image-webp decodes them (opaque when it reports no alpha).
fn decode(webp: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = WebPDecoder::new(Cursor::new(webp)).expect("valid WebP header");
    let (width, height) = decoder.dimensions();
    let alpha = decoder.has_alpha();
    let mut pixels = vec![0; decoder.output_buffer_size().expect("size fits memory")];
    decoder.read_image(&mut pixels).expect("decodes");
    let layout = if alpha { Layout::Rgba } else { Layout::Rgb };
    (width, height, rgba(&pixels, layout))
}

fn rgba(pixels: &[u8], layout: Layout) -> Vec<u8> {
    match layout {
        Layout::Rgba => pixels.to_vec(),
        Layout::Rgb => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
    }
}

/// RGBA samples as libwebp's own decoder returns them. Test-only: the app never decodes with libwebp.
fn decode_with_libwebp(webp: &[u8]) -> Vec<u8> {
    let (mut width, mut height) = (0, 0);
    // SAFETY: `webp` is a valid slice for the duration of the call; width and height are writable locals.
    let data = unsafe { libwebp_sys::WebPDecodeRGBA(webp.as_ptr(), webp.len(), &mut width, &mut height) };
    assert!(!data.is_null(), "libwebp decodes the output");
    let length = usize::try_from(width * height * 4).expect("positive size");
    // SAFETY: libwebp returned a buffer of width × height RGBA pixels that stays valid until freed below.
    let pixels = unsafe { std::slice::from_raw_parts(data, length) }.to_vec();
    // SAFETY: `data` was allocated by libwebp and is not used afterwards.
    unsafe { libwebp_sys::WebPFree(data.cast()) };
    pixels
}

fn alpha(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).map(|p| p[3]).collect()
}

#[test]
fn lossless_keeps_every_sample_at_edge_sizes() {
    let sizes = [(1, 1), (2, 3), (37, 23), (300, 7), (MAX_DIMENSION, 1), (1, MAX_DIMENSION)];
    for layout in [Layout::Rgb, Layout::Rgba] {
        for (width, height) in sizes {
            let pixels = picture(width, height, layout);
            let webp = encode(&pixels, width, height, layout, LOSSLESS, NEVER).expect("encodes");
            let (w, h, decoded) = decode(&webp);
            assert_eq!((w, h), (width, height), "{layout:?} {width}×{height}");
            assert!(decoded == rgba(&pixels, layout), "{layout:?} {width}×{height}: pixels changed");
        }
    }
}

#[test]
fn every_lossless_level_is_exact_and_the_highest_is_not_larger_than_the_lowest() {
    let pixels = picture(96, 64, Layout::Rgba);
    let sizes: Vec<usize> = [0, 3, 6, 9]
        .into_iter()
        .map(|level| {
            let webp = encode(&pixels, 96, 64, Layout::Rgba, Mode::Lossless { level }, NEVER).expect("encodes");
            assert!(decode(&webp).2 == pixels, "level {level}: pixels changed");
            webp.len()
        })
        .collect();
    assert!(sizes[3] <= sizes[0], "{sizes:?}");
}

#[test]
fn lossy_keeps_the_size_and_stores_alpha_losslessly() {
    let pixels = picture(64, 48, Layout::Rgba);
    let webp = encode(&pixels, 64, 48, Layout::Rgba, LOSSY, NEVER).expect("encodes");
    let (width, height, decoded) = decode(&webp);
    assert_eq!((width, height), (64, 48));
    assert_eq!(alpha(&decoded), alpha(&pixels));
    let opaque = picture(64, 48, Layout::Rgb);
    let webp = encode(&opaque, 64, 48, Layout::Rgb, LOSSY, NEVER).expect("encodes");
    assert!(alpha(&decode(&webp).2).iter().all(|&a| a == 255));
}

#[test]
fn libwebp_decodes_the_output_as_image_webp_does() {
    for (layout, mode) in [(Layout::Rgba, LOSSLESS), (Layout::Rgb, LOSSLESS), (Layout::Rgba, LOSSY)] {
        let pixels = picture(45, 31, layout);
        let webp = encode(&pixels, 45, 31, layout, mode, NEVER).expect("encodes");
        assert!(decode_with_libwebp(&webp) == decode(&webp).2, "{layout:?} {mode:?}");
    }
}

#[test]
fn encoding_is_deterministic() {
    let pixels = picture(120, 80, Layout::Rgba);
    for mode in [LOSSLESS, LOSSY] {
        let first = encode(&pixels, 120, 80, Layout::Rgba, mode, NEVER);
        assert_eq!(first, encode(&pixels, 120, 80, Layout::Rgba, mode, NEVER), "{mode:?}");
    }
}

#[test]
fn invalid_input_is_refused_before_libwebp_runs() {
    let calls = AtomicUsize::new(0);
    let counting = || {
        calls.fetch_add(1, Ordering::Relaxed);
        false
    };
    let rgb = picture(4, 4, Layout::Rgb);
    let wide = vec![0; (MAX_DIMENSION as usize + 1) * 3];
    let cases: [(&[u8], u32, u32, Layout, Mode); 8] = [
        (&rgb, 0, 4, Layout::Rgb, LOSSLESS),
        (&rgb, 4, 0, Layout::Rgb, LOSSLESS),
        (&wide, MAX_DIMENSION + 1, 1, Layout::Rgb, LOSSLESS),
        (&rgb[..47], 4, 4, Layout::Rgb, LOSSLESS),
        (&rgb, 4, 4, Layout::Rgba, LOSSLESS),
        (&rgb, 4, 4, Layout::Rgb, Mode::Lossless { level: 10 }),
        (&rgb, 4, 4, Layout::Rgb, Mode::Lossy { quality: 101, method: 4 }),
        (&rgb, 4, 4, Layout::Rgb, Mode::Lossy { quality: 75, method: 7 }),
    ];
    for (index, (pixels, width, height, layout, mode)) in cases.into_iter().enumerate() {
        let result = encode(pixels, width, height, layout, mode, &counting);
        assert!(matches!(result, Err(EncodeError::InvalidInput(_))), "case {index}: {result:?}");
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn a_stop_request_ends_the_encoding() {
    let pixels = picture(256, 256, Layout::Rgba);
    for mode in [LOSSLESS, LOSSY] {
        assert_eq!(encode(&pixels, 256, 256, Layout::Rgba, mode, &|| true), Err(EncodeError::Stopped), "{mode:?}");
        let calls = AtomicUsize::new(0);
        let later = || calls.fetch_add(1, Ordering::Relaxed) >= 2;
        assert_eq!(encode(&pixels, 256, 256, Layout::Rgba, mode, &later), Err(EncodeError::Stopped), "{mode:?}");
    }
}

#[test]
fn a_panicking_stop_callback_stops_the_encoding_without_aborting() {
    let pixels = picture(64, 64, Layout::Rgb);
    let panicking = || -> bool { panic!("callback failure") };
    assert_eq!(encode(&pixels, 64, 64, Layout::Rgb, LOSSY, &panicking), Err(EncodeError::Stopped));
    assert!(encode(&pixels, 64, 64, Layout::Rgb, LOSSY, NEVER).is_ok(), "the encoder still works");
}
