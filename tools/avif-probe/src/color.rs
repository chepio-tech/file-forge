//! Explicit sRGB reconstruction for the evaluated 8-bit SDR subset.
// Core
use avif_rust::av1::{ChromaSamplePosition, ColorRange, PlaneBuffer};
use avif_rust::{ColorInformationSet, DecodedFrame};
// Domain
use crate::buffers::{Budget, Image};

fn validate_plane(plane: &PlaneBuffer, index: u8, width: usize, height: usize, sx: u8, sy: u8) -> Result<(), String> {
    let layout = plane.layout;
    let count = width.checked_mul(height).ok_or("plane dimensions overflow")?;
    if layout.plane != index
        || layout.width != width
        || layout.height != height
        || layout.subsampling_x != sx
        || layout.subsampling_y != sy
        || layout.sample_count != count
        || plane.samples.len() != count
        || plane.samples.iter().any(|sample| *sample > 255)
    {
        return Err("invalid 8-bit plane layout or samples".into());
    }
    Ok(())
}

/// Centered chroma samples are one quarter of a chroma interval from each luma sample.
/// Extend horizontal endpoints as in the reference; quantization happens before the color matrix.
fn sample(plane: &PlaneBuffer, x: usize, y: usize, output_width: usize) -> f64 {
    fn axis(position: usize, size: usize, subsampled: u8) -> (usize, usize, f64) {
        if subsampled == 0 {
            return (position, position, 0.0);
        }
        let base = position / 2;
        let neighbor = if position.is_multiple_of(2) { base.saturating_sub(1) } else { (base + 1).min(size - 1) };
        (base, neighbor, 0.25)
    }
    let layout = plane.layout;
    let (x0, mut x1, wx) = axis(x, layout.width, layout.subsampling_x);
    // Match libyuv's endpoint extension, including the final column of an odd width.
    if x == output_width - 1 {
        x1 = x0;
    }
    let (y0, y1, wy) = axis(y, layout.height, layout.subsampling_y);
    let at = |x, y| f64::from(plane.samples[y * layout.width + x]);
    let top = at(x0, y0) * (1.0 - wx) + at(x1, y0) * wx;
    let bottom = at(x0, y1) * (1.0 - wx) + at(x1, y1) * wx;
    top * (1.0 - wy) + bottom * wy
}

pub fn rgba(
    frame: &DecodedFrame,
    metadata: &ColorInformationSet,
    premultiplied: bool,
    max_pixels: usize,
    budget: &Budget,
) -> Result<Image, String> {
    let config = frame.color_config;
    let description = config.color_description.ok_or("prototype requires explicit sRGB color description")?;
    if frame.bit_depth != 8
        || config.bit_depth != 8
        || config.high_bitdepth
        || config.twelve_bit
        || description.color_primaries != 1
        || description.transfer_characteristics != 13
        || !matches!(description.matrix_coefficients, 1 | 6)
        || metadata.icc_profile.is_some()
        || metadata.icc_color_type.is_some()
        || !metadata.unknown_colr.is_empty()
    {
        return Err("prototype supports only 8-bit sRGB with BT.601/BT.709 matrix and no ICC profile".into());
    }
    let full = config.color_range == ColorRange::Full;
    if let Some(nclx) = &metadata.nclx
        && (nclx.color_primaries != u16::from(description.color_primaries)
            || nclx.transfer_characteristics != u16::from(description.transfer_characteristics)
            || nclx.matrix_coefficients != u16::from(description.matrix_coefficients)
            || nclx.full_range_flag != full)
    {
        return Err("container and bitstream color descriptions disagree".into());
    }
    if !config.monochrome
        && (config.subsampling_x || config.subsampling_y)
        && (!config.subsampling_x
            || !matches!(config.chroma_sample_position, None | Some(ChromaSamplePosition::Unknown)))
    {
        return Err("prototype supports only centered 4:2:0/4:2:2 chroma".into());
    }
    let (width, height) = (frame.width, frame.height);
    let pixels =
        width.checked_mul(height).filter(|n| *n > 0 && *n <= max_pixels).ok_or("output pixel limit exceeded")?;
    let bytes = pixels.checked_mul(4).ok_or("RGBA byte count overflow")?;
    if (frame.buffers.width, frame.buffers.height) != (width, height) {
        return Err("frame and plane dimensions disagree".into());
    }
    let planes = &frame.buffers.planes;
    let color_count = if config.monochrome { 1 } else { 3 };
    // Native alpha is appended after the color planes, with channel id 3.
    if planes.len() != color_count && planes.len() != color_count + 1 {
        return Err("unexpected native plane count".into());
    }
    validate_plane(&planes[0], 0, width, height, 0, 0)?;
    if !config.monochrome {
        let (sx, sy) = (u8::from(config.subsampling_x), u8::from(config.subsampling_y));
        for (index, plane) in planes.iter().enumerate().take(3).skip(1) {
            validate_plane(plane, index as u8, width.div_ceil(1 << sx), height.div_ceil(1 << sy), sx, sy)?;
        }
    }
    let alpha = planes.get(color_count);
    if let Some(alpha) = alpha {
        validate_plane(alpha, 3, width, height, 0, 0)?;
    }
    if premultiplied && alpha.is_none() {
        return Err("premultiplied image has no alpha plane".into());
    }
    // Six fractional bits and the 16-bit luma multiplier match the reference's
    // libyuv conversion. A one-byte RGB discrepancy is magnified at low alpha.
    let (blue_u, green_u, green_v, red_v) = match (description.matrix_coefficients, full) {
        (6, true) => (113, 22, 46, 90),
        (6, false) => (128, 25, 52, 102),
        (1, true) => (119, 12, 30, 101),
        (1, false) => (128, 14, 34, 115),
        _ => return Err("unsupported color matrix".into()),
    };
    let (luma_gain, luma_bias) = if full { (16320, 32) } else { (18997, -1160) };
    let mut result = budget.zeroed(bytes)?;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let luma = ((i32::from(planes[0].samples[index]) * 257 * luma_gain) >> 16) + luma_bias;
            let a = alpha.map_or(255, |plane| plane.samples[index] as u8);
            let rgb = if config.monochrome {
                [luma; 3]
            } else {
                let cb = sample(&planes[1], x, y, width).round() as i32 - 128;
                let cr = sample(&planes[2], x, y, width).round() as i32 - 128;
                [luma + red_v * cr, luma - green_u * cb - green_v * cr, luma + blue_u * cb]
            };
            for (channel_index, channel) in rgb.into_iter().enumerate() {
                let quantized = (channel >> 6).clamp(0, 255) as u32;
                // Match the reference's byte RGB -> straight-alpha order. Keeping a
                // fractional color here produces different near-transparent samples.
                let straight = if !premultiplied {
                    quantized
                } else if a == 0 {
                    0
                } else {
                    (quantized * 255 + u32::from(a) / 2) / u32::from(a)
                };
                result[index * 4 + channel_index] = straight.min(255) as u8;
            }
            result[index * 4 + 3] = a;
        }
    }
    Ok(Image { width, height, rgba: result })
}

#[cfg(test)]
mod tests {
    // Core
    use avif_rust::NclxColorInformation;
    use avif_rust::av1::{ColorConfig, ColorDescription, FrameBuffers, PlaneLayout};
    // Domain
    use super::*;

    fn frame(sx: bool, sy: bool, full: bool) -> DecodedFrame {
        let mut planes = Vec::new();
        for index in 0..3 {
            let (sx, sy) = if index == 0 { (0, 0) } else { (u8::from(sx), u8::from(sy)) };
            let (width, height) = (3usize.div_ceil(1 << sx), 3usize.div_ceil(1 << sy));
            planes.push(PlaneBuffer {
                layout: PlaneLayout {
                    plane: index,
                    width,
                    height,
                    subsampling_x: sx,
                    subsampling_y: sy,
                    sample_count: width * height,
                },
                samples: vec![128; width * height],
            });
        }
        DecodedFrame {
            width: 3,
            height: 3,
            render_width: 3,
            render_height: 3,
            bit_depth: 8,
            color_config: ColorConfig {
                high_bitdepth: false,
                twelve_bit: false,
                bit_depth: 8,
                monochrome: false,
                color_description: Some(ColorDescription {
                    color_primaries: 1,
                    transfer_characteristics: 13,
                    matrix_coefficients: 6,
                }),
                color_range: if full { ColorRange::Full } else { ColorRange::Studio },
                subsampling_x: sx,
                subsampling_y: sy,
                chroma_sample_position: Some(ChromaSamplePosition::Unknown),
                separate_uv_delta_q: false,
            },
            color_information: None,
            alpha_premultiplied: false,
            buffers: FrameBuffers { width: 3, height: 3, planes },
        }
    }

    fn convert(frame: &DecodedFrame, prem: bool) -> Result<Image, String> {
        rgba(frame, &ColorInformationSet::default(), prem, 100, &Budget::new(1000))
    }

    #[test]
    fn interpolates_centered_chroma_in_both_axes_at_odd_edges() {
        let mut first = frame(true, true, true);
        let plane = &mut first.buffers.planes[1];
        plane.samples = vec![0, 80, 160, 240];
        assert_eq!(sample(plane, 0, 0, 3), 0.0);
        assert_eq!(sample(plane, 1, 0, 3), 20.0);
        assert_eq!(sample(plane, 1, 1, 3), 60.0);
        assert_eq!(sample(plane, 2, 2, 3), 200.0);
        let mut frame = frame(true, false, true);
        frame.buffers.planes[1].samples = vec![0, 80, 160, 240, 20, 60];
        assert_eq!(sample(&frame.buffers.planes[1], 1, 1, 3), 180.0);
    }

    #[test]
    fn range_endpoints_neutral_color_and_both_matrices() {
        for full in [true, false] {
            for matrix in [1, 6] {
                let mut frame = frame(false, false, full);
                frame.color_config.color_description.as_mut().expect("description").matrix_coefficients = matrix;
                frame.buffers.planes[0].samples[..3].copy_from_slice(if full {
                    &[0, 128, 255]
                } else {
                    &[16, 126, 235]
                });
                let result = convert(&frame, false).expect("RGB");
                assert_eq!(&result.rgba[..12], &[0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]);
            }
        }
        let mut colored = frame(false, false, true);
        colored.buffers.planes[0].samples[0] = 76;
        colored.buffers.planes[1].samples[0] = 85;
        colored.buffers.planes[2].samples[0] = 255;
        assert_eq!(&convert(&colored, false).expect("BT601").rgba[..4], &[255, 0, 0, 255]);
        colored.color_config.color_description.as_mut().expect("description").matrix_coefficients = 1;
        assert_eq!(&convert(&colored, false).expect("BT709").rgba[..4], &[255, 25, 0, 255]);
    }

    #[test]
    fn monochrome_alpha_follows_luma_and_retains_channel_id_three() {
        let mut frame = frame(false, false, true);
        frame.color_config.monochrome = true;
        frame.buffers.planes.truncate(1);
        let mut alpha = frame.buffers.planes[0].clone();
        alpha.layout.plane = 3;
        alpha.samples.fill(128);
        frame.buffers.planes.push(alpha);
        assert_eq!(&convert(&frame, false).expect("gray alpha").rgba[..4], &[128, 128, 128, 128]);
        assert_eq!(&convert(&frame, true).expect("gray premultiplied").rgba[..4], &[255, 255, 255, 128]);
    }

    #[test]
    fn unpremultiplies_quantized_rgb_and_handles_zero_alpha() {
        let mut frame = frame(false, false, true);
        let mut alpha = frame.buffers.planes[0].clone();
        alpha.layout.plane = 3;
        alpha.samples = vec![0, 1, 64, 128, 255, 64, 128, 255, 255];
        frame.buffers.planes[0].samples = vec![0, 1, 32, 64, 128, 255, 255, 255, 0];
        frame.buffers.planes.push(alpha);
        let result = convert(&frame, true).expect("unpremultiply");
        assert_eq!(
            &result.rgba[..20],
            &[0, 0, 0, 0, 255, 255, 255, 1, 128, 128, 128, 64, 128, 128, 128, 128, 128, 128, 128, 255]
        );
        assert_eq!(convert(&frame, false).expect("straight").rgba[8], 32);
        frame.buffers.planes[0].samples[0] = 0;
        frame.buffers.planes[1].samples[0] = 132;
        frame.buffers.planes[2].samples[0] = 124;
        frame.buffers.planes[3].samples[0] = 2;
        // Independent libavif fixture: premature floating-point unpremultiplication
        // or a different matrix rounding changes this green channel by over 100.
        assert_eq!(&convert(&frame, true).expect("low alpha").rgba[..4], &[0, 255, 255, 2]);
    }

    #[test]
    fn refuses_unsupported_signaling_and_container_disagreement() {
        let base = frame(true, true, true);
        let mut bad = base.clone();
        bad.color_config.chroma_sample_position = Some(ChromaSamplePosition::Colocated);
        assert!(convert(&bad, false).is_err());
        bad = base.clone();
        bad.color_config.color_description.as_mut().expect("description").transfer_characteristics = 16;
        assert!(convert(&bad, false).is_err());
        let mut metadata = ColorInformationSet { icc_profile: Some(vec![1]), ..Default::default() };
        assert!(rgba(&base, &metadata, false, 100, &Budget::new(1000)).is_err());
        metadata.icc_profile = None;
        metadata.nclx = Some(NclxColorInformation {
            color_primaries: 1,
            transfer_characteristics: 13,
            matrix_coefficients: 6,
            full_range_flag: false,
        });
        assert!(rgba(&base, &metadata, false, 100, &Budget::new(1000)).is_err());
    }

    #[test]
    fn rejects_malformed_planes_and_bounds_output_allocation() {
        let base = frame(true, true, true);
        assert!(rgba(&base, &ColorInformationSet::default(), false, 8, &Budget::new(1000)).is_err());
        assert!(rgba(&base, &ColorInformationSet::default(), false, 100, &Budget::new(35)).is_err());
        assert!(convert(&base, true).is_err());
        for change in 0..4 {
            let mut bad = base.clone();
            match change {
                0 => {
                    bad.buffers.planes[1].samples.pop();
                }
                1 => {
                    bad.buffers.planes[0].samples[0] = 256;
                }
                2 => {
                    bad.buffers.planes[2].layout.width = 0;
                }
                _ => {
                    bad.width = usize::MAX;
                }
            }
            assert!(convert(&bad, false).is_err());
        }
    }

    #[test]
    fn conversion_uses_remaining_shared_capacity_and_releases_ownership() {
        let frame = frame(false, false, true);
        let metadata = ColorInformationSet::default();
        let budget = Budget::new(40);
        let held = budget.empty(5).expect("other adapter owner");
        assert!(rgba(&frame, &metadata, false, 100, &budget).is_err());
        assert_eq!(budget.live(), 5);
        drop(held);
        let held = budget.empty(4).expect("exact remaining capacity");
        let image = rgba(&frame, &metadata, false, 100, &budget).expect("convert");
        assert_eq!(budget.live(), 40);
        drop(image);
        assert_eq!(budget.live(), 4);
        drop(held);
        assert_eq!(budget.live(), 0);
    }
}
