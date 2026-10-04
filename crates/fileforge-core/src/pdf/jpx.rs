//! JPEG 2000 (`JPXDecode`) decoding for the lossy image pass. Only images whose decoded samples match the PDF
//! dictionary exactly are returned; anything unusual is left to the caller to keep as is.

// Core
use std::panic::{AssertUnwindSafe, catch_unwind};

use hayro_jpeg2000::{ColorSpace, DecodeSettings, DecoderContext, Image};

/// 8-bit samples of a JPEG 2000 image with `components` channels (1 = gray, 3 = RGB) at `width`×`height`, interleaved,
/// or `None` when the image does not match that layout, exceeds `max_samples`, or fails to decode.
///
/// The decoder is young and reads untrusted bytes, so a panic inside it only skips this image (ADR-0015).
pub(crate) fn decode(data: &[u8], width: u32, height: u32, components: usize, max_samples: u64) -> Option<Vec<u8>> {
    catch_unwind(AssertUnwindSafe(|| decode_unchecked(data, width, height, components, max_samples))).ok().flatten()
}

fn decode_unchecked(data: &[u8], width: u32, height: u32, components: usize, max_samples: u64) -> Option<Vec<u8>> {
    let image = Image::new(data, &DecodeSettings::default()).ok()?;
    // Gray and RGB only; CMYK and unknown spaces would need a color conversion the PDF does not ask for.
    let channels = match image.color_space() {
        ColorSpace::Gray => 1,
        ColorSpace::RGB => 3,
        ColorSpace::Icc { num_channels, .. } => usize::from(*num_channels),
        ColorSpace::CMYK | ColorSpace::Unknown { .. } => return None,
    };
    // The header is parsed before any pixel buffer exists, so oversized images cost nothing.
    let samples = u64::from(width) * u64::from(height) * components as u64;
    if image.has_alpha() || channels != components || (image.width(), image.height()) != (width, height) {
        return None;
    }
    if samples > max_samples {
        return None;
    }
    let mut context = DecoderContext::default();
    let decoded = image.decode(&mut context).ok()?;
    // Deeper samples would be rounded to 8 bits, which the engine never does silently.
    if decoded.components().len() != components || decoded.components().iter().any(|c| c.bit_depth() != 8) {
        return None;
    }
    let data = decoded.data_u8();
    (data.len() as u64 == samples).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16×12 gray, irreversible 9/7 wavelet, raw codestream. Generated with OpenJPEG 2.5.4 (Pillow
    /// `save(..., "JPEG2000", no_jp2=True, quality_mode="rates", quality_layers=[4], irreversible=True)`) from
    /// `tests/support::photo(16, 12, true)`.
    const GRAY_97: &str = "\
    ff4fff5100290000000000100000000c0000000000000000000000100000000c00000000000000000001070101ff52000c00000001000304\
    040000ff5c001742673867506750676850055005504757d357d35762ff640025000143726561746564206279204f70656e4a504547207665\
    7273696f6e20322e352e34ff90000a00000000002b0001ff93c7d21007b53907c3e306000cb59fc891c1801f6b1b2856c48033df87f4ffd9";

    /// `GRAY_97` as OpenJPEG decodes it. hayro-jpeg2000 0.4.0 differed by up to 62 levels (upstream fix #1340).
    const GRAY_97_PIXELS: &str = "\
    030c1d3144515d697996b6cfe5fbffff0a1323364a576371819ab5d1e9f1fca9171f2f4154626f7f8fa1b5d3ecd9d412262e3c4d606f7d8e\
    9faab5d6e6ba7404363d4a5a6c7c8c9faeb3b7d7df891c11434a5766778899acbab9b9daed1f15024f5662708194a6b7c3bfbdd3e9002900\
    5b616e7b8b9fb3bdc5d1d3ae8900412e646a768292a9bfc8ccdbd6812e465d62686c768292aecbeaf5c2835b44656f75696c747f91b1d5ff\
    ffa62e406e787c7e6a6c747f91b2d7ffffa01b38747e8081";

    fn hex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
        digits
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16).expect("hex digit"))
            .collect()
    }

    #[test]
    fn decodes_irreversible_images_like_openjpeg() {
        let pixels = decode(&hex(GRAY_97), 16, 12, 1, u64::MAX).expect("decodes");
        let expected = hex(GRAY_97_PIXELS);
        assert_eq!(pixels.len(), expected.len());
        let worst = pixels.iter().zip(&expected).map(|(a, b)| a.abs_diff(*b)).max();
        assert!(worst <= Some(1), "max difference from OpenJPEG: {worst:?}");
    }

    #[test]
    fn layouts_that_do_not_match_the_dictionary_are_refused() {
        let data = hex(GRAY_97);
        assert_eq!(decode(&data, 16, 12, 3, u64::MAX), None, "gray data declared as RGB");
        assert_eq!(decode(&data, 12, 16, 1, u64::MAX), None, "swapped dimensions");
        assert_eq!(decode(&data, 16, 12, 1, 16 * 12 - 1), None, "above the sample limit");
    }

    #[test]
    fn broken_codestreams_never_panic() {
        let data = hex(GRAY_97);
        for cut in 0..data.len() {
            let _ = decode(&data[..cut], 16, 12, 1, u64::MAX);
        }
        let mut seed: u32 = 0x1234_5678;
        for _ in 0..2000 {
            let mut corrupted = data.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let position = seed as usize % corrupted.len();
                corrupted[position] = (seed >> 8) as u8;
            }
            let _ = decode(&corrupted, 16, 12, 1, u64::MAX);
        }
    }
}
