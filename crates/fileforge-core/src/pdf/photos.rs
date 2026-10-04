//! Conservative screen-content veto for Flate-to-JPEG conversion. A single non-photographic tile keeps the
//! entire image in Deflate, including screenshots whose main area is a photograph. This is a pixel heuristic;
//! a fullscreen screenshot of a photograph is indistinguishable from the photograph itself (ADR-0016).

const TILE: usize = 16;
const MIN_SIDE: usize = 64;

/// Inspect all pixels in overlapping edge tiles, before downsampling. Working storage is one 256-bin histogram;
/// work is linear in the already bounded sample count, with no content-driven allocation.
pub(super) fn is_photographic(samples: &[u8], width: u32, height: u32, channels: usize) -> bool {
    let (width, height) = (width as usize, height as usize);
    if !matches!(channels, 1 | 3)
        || width < MIN_SIDE
        || height < MIN_SIDE
        || width.checked_mul(height).and_then(|n| n.checked_mul(channels)) != Some(samples.len())
    {
        return false;
    }
    for y in (0..height).step_by(TILE) {
        for x in (0..width).step_by(TILE) {
            if !photographic_tile(samples, width, channels, x.min(width - TILE), y.min(height - TILE)) {
                return false;
            }
        }
    }
    true
}

fn photographic_tile(samples: &[u8], width: usize, channels: usize, left: usize, top: usize) -> bool {
    let mut tones = [0_u16; 256];
    let mut flat = 0;
    let mut sharp = 0;
    let mut pairs = 0;
    let stride = width * channels;
    for y in top..top + TILE {
        for x in left..left + TILE {
            let index = (y * width + x) * channels;
            let pixel = &samples[index..index + channels];
            let tone = match pixel {
                [gray] => usize::from(*gray),
                [r, g, b] => (77 * usize::from(*r) + 150 * usize::from(*g) + 29 * usize::from(*b)) >> 8,
                _ => return false,
            };
            tones[tone] += 1;
            for next in
                [(x + 1 < left + TILE).then_some(index + channels), (y + 1 < top + TILE).then_some(index + stride)]
                    .into_iter()
                    .flatten()
            {
                let difference =
                    pixel.iter().zip(&samples[next..next + channels]).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
                flat += usize::from(difference <= 2);
                sharp += usize::from(difference >= 48);
                pairs += 1;
            }
        }
    }
    // Repeated tones, flat/gradient UI backgrounds and dense high-contrast strokes each veto JPEG conversion.
    // These deliberately favor false negatives; thresholds are covered by generated photo/UI regressions.
    !tones.iter().any(|count| usize::from(*count) * 4 >= TILE * TILE)
        && flat * 100 < pairs * 35
        && sharp * 100 < pairs * 15
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photograph(width: u32, height: u32) -> Vec<u8> {
        let mut seed = 0x2545_f491_u32;
        (0..width * height * 3)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                100 + (seed % 24) as u8
            })
            .collect()
    }

    #[test]
    fn inspects_the_last_partial_tile_of_a_photo_with_a_ui_corner() {
        let (width, height) = (79, 67);
        let mut samples = photograph(width, height);
        assert!(is_photographic(&samples, width, height, 3));
        for y in height - 12..height {
            for x in width - 12..width {
                let index = ((y * width + x) * 3) as usize;
                samples[index..index + 3].fill(if x % 3 == 0 { 0 } else { 255 });
            }
        }
        assert!(!is_photographic(&samples, width, height, 3));
    }

    #[test]
    fn tiny_and_malformed_layouts_are_refused_without_panicking() {
        for (width, height, channels) in [(63, 80, 3), (80, 63, 3), (80, 80, 4), (u32::MAX, u32::MAX, 3)] {
            assert!(!is_photographic(&photograph(63, 80), width, height, channels));
        }
        assert!(!is_photographic(&[], 80, 80, 1));
        assert!(!is_photographic(&photograph(80, 80)[..10], 80, 80, 3));
    }
}
