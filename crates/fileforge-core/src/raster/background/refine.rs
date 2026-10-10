//! Original implementation of color-guided local regression (He et al., eq. 14–16), with a
//! bounded coefficient grid and foreground recovery from I = alpha F + (1-alpha) B.
//! No third-party filter or foreground-estimation source code is incorporated.
// Core
use image::{RgbaImage, imageops};
// Domain
use super::{Mask, checkpoint, sample};
use crate::control::Control;
// Types
use crate::raster::RasterError;

const MAX_SIDE: u32 = 1024;
const EPS: f64 = 0.001;
pub(super) struct Refinement {
    w: usize,
    h: usize,
    coefficients: Vec<[f32; 4]>,
    colors: Vec<[f32; 8]>,
}

/// Integral sums accumulate in f64 to avoid subtracting large, nearly equal f32 sums.
fn integral<const N: usize>(
    w: usize,
    h: usize,
    values: impl Fn(usize) -> [f64; N],
    control: &dyn Control,
) -> Result<Vec<[f64; N]>, RasterError> {
    let mut sums = vec![[0.0; N]; (w + 1) * (h + 1)];
    for y in 0..h {
        checkpoint(control)?;
        let mut row = [0.0; N];
        for x in 0..w {
            let v = values(y * w + x);
            for c in 0..N {
                row[c] += v[c];
                sums[(y + 1) * (w + 1) + x + 1][c] = sums[y * (w + 1) + x + 1][c] + row[c];
            }
        }
    }
    Ok(sums)
}
fn mean<const N: usize>(sums: &[[f64; N]], w: usize, h: usize, x: usize, y: usize, r: usize) -> [f64; N] {
    let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
    let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
    let area = ((x1 - x0) * (y1 - y0)) as f64;
    std::array::from_fn(|c| {
        (sums[y1 * (w + 1) + x1][c] - sums[y0 * (w + 1) + x1][c] - sums[y1 * (w + 1) + x0][c]
            + sums[y0 * (w + 1) + x0][c])
            / area
    })
}
fn pixel_rgb(image: &RgbaImage, i: usize) -> [f64; 3] {
    let p = &image.as_raw()[i * 4..i * 4 + 3];
    std::array::from_fn(|c| f64::from(p[c]) / 255.0)
}
fn probability(z: f32) -> f64 {
    1.0 / (1.0 + f64::from(-z.clamp(-30.0, 30.0)).exp())
}

impl Refinement {
    pub fn new(image: &RgbaImage, mask: &Mask, control: &dyn Control) -> Result<Self, RasterError> {
        checkpoint(control)?;
        let scale = (MAX_SIDE as f64 / image.width().max(image.height()) as f64).min(1.0);
        let w = (image.width() as f64 * scale).round().max(1.0) as u32;
        let h = (image.height() as f64 * scale).round().max(1.0) as u32;
        let guide = imageops::resize(image, w, h, imageops::FilterType::Triangle);
        let (w, h) = (w as usize, h as usize);
        let r = ((w.max(h) as f64 * 8.0 / 1024.0).round() as usize).max(2);
        let p: Vec<f64> = (0..w * h)
            .map(|i| {
                probability(sample(
                    mask,
                    ((i % w) as f32 + 0.5) * mask.width as f32 / w as f32 - 0.5,
                    ((i / w) as f32 + 0.5) * mask.height as f32 / h as f32 - 0.5,
                ))
            })
            .collect();
        // E[R,G,B,p, RR,RG,RB,GG,GB,BB, Rp,Gp,Bp].
        let sums = integral(
            w,
            h,
            |i| {
                let [a, b, c] = pixel_rgb(&guide, i);
                let p = p[i];
                [a, b, c, p, a * a, a * b, a * c, b * b, b * c, c * c, a * p, b * p, c * p]
            },
            control,
        )?;
        let mut coefficients = vec![[0.0; 4]; w * h];
        for y in 0..h {
            checkpoint(control)?;
            for x in 0..w {
                let m = mean(&sums, w, h, x, y, r);
                let cov = [m[10] - m[0] * m[3], m[11] - m[1] * m[3], m[12] - m[2] * m[3]];
                let (rr, rg, rb, gg, gb, bb) = (
                    m[4] - m[0] * m[0] + EPS,
                    m[5] - m[0] * m[1],
                    m[6] - m[0] * m[2],
                    m[7] - m[1] * m[1] + EPS,
                    m[8] - m[1] * m[2],
                    m[9] - m[2] * m[2] + EPS,
                );
                let inv = [
                    gg * bb - gb * gb,
                    gb * rb - rg * bb,
                    rg * gb - gg * rb,
                    rr * bb - rb * rb,
                    rb * rg - rr * gb,
                    rr * gg - rg * rg,
                ];
                let det = rr * inv[0] + rg * inv[1] + rb * inv[2];
                if !det.is_finite() || det <= 0.0 {
                    return Err(RasterError::Internal("invalid guided-filter covariance".into()));
                }
                let a = [
                    (cov[0] * inv[0] + cov[1] * inv[1] + cov[2] * inv[2]) / det,
                    (cov[0] * inv[1] + cov[1] * inv[3] + cov[2] * inv[4]) / det,
                    (cov[0] * inv[2] + cov[1] * inv[4] + cov[2] * inv[5]) / det,
                ];
                coefficients[y * w + x] =
                    [a[0] as f32, a[1] as f32, a[2] as f32, (m[3] - a[0] * m[0] - a[1] * m[1] - a[2] * m[2]) as f32];
            }
        }
        drop(sums);
        let sums = integral(w, h, |i| coefficients[i].map(f64::from), control)?;
        for y in 0..h {
            checkpoint(control)?;
            for x in 0..w {
                coefficients[y * w + x] = mean(&sums, w, h, x, y, r).map(|v| v as f32);
            }
        }
        drop(sums);
        // Confident local foreground/background colors become priors for the alpha-compositing identity.
        let sums = integral(
            w,
            h,
            |i| {
                let rgb = pixel_rgb(&guide, i);
                let alpha = f64::from(guide.as_raw()[i * 4 + 3]) / 255.0;
                let fg = if p[i] >= 0.98 { alpha } else { 0.0 };
                let bg = if p[i] <= 0.02 { alpha } else { 0.0 };
                [fg, bg, fg * rgb[0], fg * rgb[1], fg * rgb[2], bg * rgb[0], bg * rgb[1], bg * rgb[2]]
            },
            control,
        )?;
        let mut colors = vec![[0.0; 8]; w * h];
        for y in 0..h {
            checkpoint(control)?;
            for x in 0..w {
                colors[y * w + x] = mean(&sums, w, h, x, y, r * 3).map(|v| v as f32);
            }
        }
        Ok(Self { w, h, coefficients, colors })
    }
    fn grid<const N: usize>(&self, values: &[[f32; N]], x: f32, y: f32) -> [f32; N] {
        let x = x.clamp(0.0, (self.w - 1) as f32);
        let y = y.clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (dx, dy) = (x - x0 as f32, y - y0 as f32);
        std::array::from_fn(|c| {
            (values[y0 * self.w + x0][c] * (1.0 - dx) + values[y0 * self.w + x1][c] * dx) * (1.0 - dy)
                + (values[y1 * self.w + x0][c] * (1.0 - dx) + values[y1 * self.w + x1][c] * dx) * dy
        })
    }
    pub fn apply(&self, source: [u8; 4], x: u32, y: u32, w: u32, h: u32, logit: f32) -> ([u8; 3], f32) {
        let p = probability(logit);
        // Confidence gates prevent the refinement leaking into definite background or changing opaque interiors.
        if p <= 0.02 {
            return ([source[0], source[1], source[2]], 0.0);
        }
        if p >= 0.98 {
            return ([source[0], source[1], source[2]], 1.0);
        }
        let gx = (x as f32 + 0.5) * self.w as f32 / w as f32 - 0.5;
        let gy = (y as f32 + 0.5) * self.h as f32 / h as f32 - 0.5;
        let coeff = self.grid(&self.coefficients, gx, gy);
        let rgb: [f32; 3] = std::array::from_fn(|c| f32::from(source[c]) / 255.0);
        let q = (coeff[0] * rgb[0] + coeff[1] * rgb[1] + coeff[2] * rgb[2] + coeff[3]).clamp(0.0, 1.0);
        let alpha = if q <= 0.02 {
            0.0
        } else if q >= 0.98 {
            1.0
        } else {
            q
        };
        if alpha == 0.0 || alpha == 1.0 || source[3] != 255 {
            return ([source[0], source[1], source[2]], alpha);
        }
        let means = self.grid(&self.colors, gx, gy);
        if means[0] < 0.001 || means[1] < 0.001 {
            return ([source[0], source[1], source[2]], alpha);
        }
        let recovered = std::array::from_fn(|c| {
            let foreground = means[c + 2] / means[0];
            let background = means[c + 5] / means[1];
            let regularization = 0.01;
            let value = (alpha * (rgb[c] - (1.0 - alpha) * background) + regularization * foreground)
                / (alpha * alpha + regularization);
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        });
        (recovered, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_guidance_follows_a_fine_edge_and_preserves_definite_regions() {
        let image = RgbaImage::from_fn(128, 64, |x, _| {
            image::Rgba(if x < 64 { [20, 40, 220, 255] } else { [220, 40, 20, 255] })
        });
        let mask =
            Mask { width: 16, height: 8, logits: (0..128).map(|i| if i % 16 < 8 { -6.0 } else { 6.0 }).collect() };
        let refine = Refinement::new(&image, &mask, &()).expect("filter");
        let (_, left) = refine.apply(image.get_pixel(63, 32).0, 63, 32, 128, 64, -0.1);
        let (_, right) = refine.apply(image.get_pixel(64, 32).0, 64, 32, 128, 64, 0.1);
        assert!(right > left + 0.3, "guided edge contrast: {left} {right}");
        assert_eq!(refine.apply([12, 34, 56, 255], 0, 0, 128, 64, 10.0), ([12, 34, 56], 1.0));
        assert_eq!(refine.apply([12, 34, 56, 255], 0, 0, 128, 64, -10.0).1, 0.0);
    }
    #[test]
    fn regularized_foreground_recovery_removes_background_color_without_altering_opaque_rgb() {
        let refine = Refinement {
            w: 1,
            h: 1,
            coefficients: vec![[0.0, 0.0, 0.0, 0.5]],
            colors: vec![[0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5]],
        };
        let (color, alpha) = refine.apply([128, 0, 128, 255], 0, 0, 1, 1, 0.0);
        assert_eq!(alpha, 0.5);
        assert!(color[0] > 245 && color[2] < 5, "color: {color:?}");
        assert_eq!(refine.apply([128, 0, 128, 255], 0, 0, 1, 1, 10.0).0, [128, 0, 128]);
        assert_eq!(refine.apply([128, 0, 128, 128], 0, 0, 1, 1, 0.0).0, [128, 0, 128]);
    }
    #[test]
    fn integral_means_match_direct_windows_and_single_pixels() {
        let sums = integral(3, 2, |i| [i as f64], &()).expect("integral");
        assert_eq!(mean(&sums, 3, 2, 0, 0, 1), [2.0]);
        assert_eq!(mean(&sums, 3, 2, 2, 1, 1), [3.0]);
        let sums = integral(1, 1, |_| [0.25], &()).expect("integral");
        assert_eq!(mean(&sums, 1, 1, 0, 0, 8), [0.25]);
    }
    #[test]
    fn cancelled_refinement_stops_before_allocation_work() {
        struct Stop;
        impl Control for Stop {
            fn is_cancelled(&self) -> bool {
                true
            }
        }
        assert!(matches!(
            Refinement::new(&RgbaImage::new(1, 1), &Mask { width: 1, height: 1, logits: vec![0.0] }, &Stop),
            Err(RasterError::Cancelled)
        ));
    }
}
