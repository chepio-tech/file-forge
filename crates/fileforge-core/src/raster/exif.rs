//! The EXIF orientation tag: the one EXIF field "Remove metadata" keeps, so photos taken sideways do not turn.

const ORIENTATION: u16 = 0x0112;
const SHORT: u16 = 3;

/// Orientation (2–8) of a TIFF structure, the body of a JPEG `Exif\0\0` segment or a PNG `eXIf` chunk. `None` for the
/// default orientation 1, a missing tag or an unreadable structure.
pub(crate) fn rotation(tiff: &[u8]) -> Option<Orientation> {
    let big_endian = match tiff.get(..4)? {
        b"MM\0*" => true,
        b"II*\0" => false,
        _ => return None,
    };
    let u16_at = |at: usize| {
        let bytes = [*tiff.get(at)?, *tiff.get(at + 1)?];
        Some(if big_endian { u16::from_be_bytes(bytes) } else { u16::from_le_bytes(bytes) })
    };
    let u32_at = |at: usize| {
        let bytes = [*tiff.get(at)?, *tiff.get(at + 1)?, *tiff.get(at + 2)?, *tiff.get(at + 3)?];
        Some(if big_endian { u32::from_be_bytes(bytes) } else { u32::from_le_bytes(bytes) })
    };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let entries = usize::from(u16_at(ifd)?);
    let value = (0..entries).find_map(|index| {
        let entry = ifd.checked_add(2 + index * 12)?;
        (u16_at(entry)? == ORIENTATION && u16_at(entry + 2)? == SHORT).then(|| u16_at(entry + 8))?
    })?;
    (2..=8).contains(&value).then_some(Orientation { value, big_endian })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Orientation {
    value: u16,
    big_endian: bool,
}

impl Orientation {
    /// A TIFF structure holding only this orientation, in the original byte order (26 bytes).
    pub(crate) fn tiff(self) -> Vec<u8> {
        let u16_bytes = |value: u16| if self.big_endian { value.to_be_bytes() } else { value.to_le_bytes() };
        let u32_bytes = |value: u32| if self.big_endian { value.to_be_bytes() } else { value.to_le_bytes() };
        let mut tiff = Vec::with_capacity(26);
        tiff.extend_from_slice(if self.big_endian { b"MM\0*" } else { b"II*\0" });
        tiff.extend_from_slice(&u32_bytes(8));
        tiff.extend_from_slice(&u16_bytes(1));
        tiff.extend_from_slice(&u16_bytes(ORIENTATION));
        tiff.extend_from_slice(&u16_bytes(SHORT));
        tiff.extend_from_slice(&u32_bytes(1));
        tiff.extend_from_slice(&u16_bytes(self.value));
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&u32_bytes(0));
        tiff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_orientation_in_both_byte_orders_and_writes_it_back() {
        for big_endian in [true, false] {
            let orientation = Orientation { value: 6, big_endian };
            let tiff = orientation.tiff();
            assert_eq!(tiff.len(), 26);
            assert_eq!(rotation(&tiff), Some(orientation));
        }
    }

    #[test]
    fn default_missing_and_broken_orientations_are_none() {
        let mut upright = Orientation { value: 6, big_endian: false }.tiff();
        upright[18] = 1;
        assert_eq!(rotation(&upright), None);
        let mut other_tag = Orientation { value: 6, big_endian: false }.tiff();
        other_tag[10] = 0x13;
        assert_eq!(rotation(&other_tag), None);
        let truncated = Orientation { value: 6, big_endian: true }.tiff();
        assert_eq!(rotation(&truncated[..17]), None);
        assert_eq!(rotation(b"not a tiff"), None);
        let mut far_ifd = Orientation { value: 6, big_endian: true }.tiff();
        far_ifd[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(rotation(&far_ifd), None);
    }
}
