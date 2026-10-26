//! The bounded native API returns unrotated pixels; apply container properties in order.
// Core
use avif_rust::NativePropertyRecord;
// Domain
use crate::buffers::Image;

pub fn orient(mut image: Image, properties: &[NativePropertyRecord]) -> Result<Image, String> {
    if image.width.checked_mul(image.height).and_then(|n| n.checked_mul(4)) != Some(image.rgba.len()) {
        return Err("invalid RGBA dimensions".into());
    }
    for property in properties {
        match property {
            NativePropertyRecord::Rotation(rotation) => {
                if rotation.angle > 3 {
                    return Err("invalid rotation".into());
                }
                for _ in 0..rotation.angle {
                    let (w, h) = (image.width, image.height);
                    let mut rotated = image.rgba.zeroed_sibling(image.rgba.len())?;
                    for y in 0..h {
                        for x in 0..w {
                            let source = (y * w + x) * 4;
                            let destination = ((w - 1 - x) * h + y) * 4;
                            rotated[destination..destination + 4].copy_from_slice(&image.rgba[source..source + 4]);
                        }
                    }
                    image = Image { width: h, height: w, rgba: rotated };
                }
            }
            NativePropertyRecord::Mirror(mirror) => {
                if mirror.axis > 1 {
                    return Err("invalid mirror axis".into());
                }
                let (w, h) = (image.width, image.height);
                for y in 0..h {
                    for x in 0..w {
                        let source = (y * w + x) * 4;
                        // ISO/IEC 23008-12:2022: axis 0 exchanges top/bottom; axis 1 exchanges left/right.
                        let destination =
                            if mirror.axis == 1 { (y * w + w - 1 - x) * 4 } else { ((h - 1 - y) * w + x) * 4 };
                        if source < destination {
                            for channel in 0..4 {
                                image.rgba.swap(source + channel, destination + channel);
                            }
                        }
                    }
                }
            }
            NativePropertyRecord::CleanAperture(_) => {
                return Err("prototype does not apply clean aperture cropping".into());
            }
            _ => {}
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    // Core
    use avif_rust::{ImageMirror, ImageRotation};
    // Domain
    use super::*;
    use crate::buffers::Budget;

    fn pixels_with(budget: &Budget) -> Image {
        let mut rgba = budget.empty(24).expect("pixels");
        for n in 1..=6 {
            rgba.append(&[n, n, n, 255]).expect("pixel");
        }
        Image { width: 2, height: 3, rgba }
    }

    fn pixels() -> Image {
        pixels_with(&Budget::new(48))
    }

    fn labels(image: &Image) -> Vec<u8> {
        image.rgba.as_chunks::<4>().0.iter().map(|pixel| pixel[0]).collect()
    }

    #[test]
    fn rotation_preserves_samples_and_swaps_rectangular_dimensions() {
        let image = orient(pixels(), &[NativePropertyRecord::Rotation(ImageRotation { angle: 1 })]).expect("rotate");
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(labels(&image), [2, 4, 6, 1, 3, 5]);
    }

    #[test]
    fn mirror_and_rotation_follow_association_order() {
        let mirror = NativePropertyRecord::Mirror(ImageMirror { axis: 1 });
        let rotate = NativePropertyRecord::Rotation(ImageRotation { angle: 1 });
        let first = orient(pixels(), &[mirror.clone(), rotate.clone()]).expect("mirror then rotate");
        let second = orient(pixels(), &[rotate, mirror]).expect("rotate then mirror");
        assert_eq!(labels(&first), [1, 3, 5, 2, 4, 6]);
        assert_eq!(labels(&second), [6, 4, 2, 5, 3, 1]);
    }

    #[test]
    fn rejects_invalid_pixel_storage_before_transforming() {
        let image = Image { width: 2, height: 2, rgba: Budget::new(4).zeroed(4).expect("pixels") };
        assert!(orient(image, &[]).is_err());
    }

    #[test]
    fn horizontal_axis_mirrors_top_and_bottom() {
        let image = orient(pixels(), &[NativePropertyRecord::Mirror(ImageMirror { axis: 0 })]).expect("mirror");
        assert_eq!(labels(&image), [5, 6, 3, 4, 1, 2]);
    }

    #[test]
    fn rotations_share_budget_and_release_each_previous_buffer() {
        let rotation = NativePropertyRecord::Rotation(ImageRotation { angle: 3 });
        let budget = Budget::new(48);
        let result = orient(pixels_with(&budget), &[rotation.clone(), rotation.clone()]).expect("rotations");
        assert_eq!(labels(&result), [6, 5, 4, 3, 2, 1]);
        assert_eq!(budget.peak(), 48);
        assert_eq!(budget.live(), 24);
        drop(result);
        assert_eq!(budget.live(), 0);

        let budget = Budget::new(47);
        assert!(orient(pixels_with(&budget), &[rotation]).is_err());
        assert_eq!(budget.live(), 0);
        let image = pixels_with(&budget);
        assert!(orient(image, &[NativePropertyRecord::Mirror(ImageMirror { axis: 0 })]).is_ok());
        assert_eq!(budget.live(), 0);
    }
}
