//! JPEG marker structure (ITU-T T.81, Annex B): the file as ordered marker segments and entropy-coded scans.

// Domain
use crate::raster::error::{RasterError, malformed};

pub(crate) const SOI: u8 = 0xD8;
pub(crate) const EOI: u8 = 0xD9;
pub(crate) const SOS: u8 = 0xDA;
pub(crate) const DHT: u8 = 0xC4;
pub(crate) const DRI: u8 = 0xDD;
pub(crate) const COM: u8 = 0xFE;
pub(crate) const APP0: u8 = 0xE0;
pub(crate) const APP1: u8 = 0xE1;
pub(crate) const APP2: u8 = 0xE2;
pub(crate) const APP11: u8 = 0xEB;
pub(crate) const APP14: u8 = 0xEE;
pub(crate) const APP15: u8 = 0xEF;
/// Baseline and extended sequential DCT, Huffman-coded: the frames the lossless transcoder rewrites.
pub(crate) const SOF0: u8 = 0xC0;
pub(crate) const SOF1: u8 = 0xC1;

/// One element of the file between SOI and EOI, borrowing from the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item<'a> {
    /// A marker segment; `payload` excludes the marker and the length field.
    Segment { marker: u8, payload: &'a [u8] },
    /// A start-of-scan header and the entropy-coded data after it, restart markers included.
    Scan { header: &'a [u8], data: &'a [u8] },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Structure<'a> {
    pub items: Vec<Item<'a>>,
    /// Bytes after EOI, such as MPF secondary images or HDR gain maps; empty for a plain JPEG.
    pub trailing: &'a [u8],
}

impl Structure<'_> {
    pub(crate) fn segments(&self) -> impl Iterator<Item = (u8, &[u8])> {
        self.items.iter().filter_map(|item| match *item {
            Item::Segment { marker, payload } => Some((marker, payload)),
            Item::Scan { .. } => None,
        })
    }
}

/// Splits a JPEG into segments and scans. Refuses anything that does not end with EOI.
pub(crate) fn parse(input: &[u8]) -> Result<Structure<'_>, RasterError> {
    if !input.starts_with(&[0xFF, SOI]) {
        return Err(malformed("missing start-of-image marker"));
    }
    let mut items = Vec::new();
    let mut pos = 2;
    loop {
        let marker = next_marker(input, &mut pos)?;
        match marker {
            EOI => return Ok(Structure { items, trailing: &input[pos..] }),
            SOI | 0x01 | 0xD0..=0xD7 => return Err(malformed("unexpected standalone marker")),
            _ => {}
        }
        let payload = segment_payload(input, &mut pos)?;
        if marker == SOS {
            let start = pos;
            pos = scan_end(input, pos);
            items.push(Item::Scan { header: payload, data: &input[start..pos] });
        } else {
            items.push(Item::Segment { marker, payload });
        }
    }
}

/// Reads `0xFF` (and any fill bytes) plus the marker code at `pos`.
fn next_marker(input: &[u8], pos: &mut usize) -> Result<u8, RasterError> {
    if input.get(*pos) != Some(&0xFF) {
        return Err(malformed("expected a marker"));
    }
    while input.get(*pos) == Some(&0xFF) {
        *pos += 1;
    }
    let marker = *input.get(*pos).ok_or_else(|| malformed("missing end-of-image marker"))?;
    if marker == 0x00 {
        return Err(malformed("expected a marker"));
    }
    *pos += 1;
    Ok(marker)
}

fn segment_payload<'a>(input: &'a [u8], pos: &mut usize) -> Result<&'a [u8], RasterError> {
    let length = match input.get(*pos..*pos + 2) {
        Some(&[high, low]) => usize::from(u16::from_be_bytes([high, low])),
        _ => return Err(malformed("truncated segment")),
    };
    if length < 2 {
        return Err(malformed("segment length below 2"));
    }
    let payload = input.get(*pos + 2..*pos + length).ok_or_else(|| malformed("truncated segment"))?;
    *pos += length;
    Ok(payload)
}

/// Position of the first marker after entropy-coded data that is not a restart marker, or the end of the input.
fn scan_end(input: &[u8], mut pos: usize) -> usize {
    while pos < input.len() {
        if input[pos] != 0xFF {
            pos += 1;
            continue;
        }
        match input.get(pos + 1) {
            Some(0x00 | 0xD0..=0xD7) => pos += 2,
            // A fill byte before the next marker.
            Some(0xFF) => pos += 1,
            _ => return pos,
        }
    }
    pos
}

/// Appends a marker segment. Payloads come from a parsed file or are small tables, so they always fit.
pub(crate) fn write_segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) -> Result<(), RasterError> {
    let length = u16::try_from(payload.len() + 2)
        .map_err(|_| RasterError::Internal(format!("segment of {} bytes", payload.len())))?;
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

/// Frame header (SOFn): sample precision, size and components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub marker: u8,
    pub precision: u8,
    pub width: u16,
    pub height: u16,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Component {
    pub id: u8,
    pub h: u8,
    pub v: u8,
}

/// Start-of-frame markers: C0–CF except DHT (C4), JPG (C8) and DAC (CC).
fn is_sof(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xCF) && !matches!(marker, DHT | 0xC8 | 0xCC)
}

impl Frame {
    /// The only frame header of the file; hierarchical files (several frames) are refused.
    pub(crate) fn find(structure: &Structure) -> Result<Self, RasterError> {
        let mut frames = structure.segments().filter(|(marker, _)| is_sof(*marker));
        let (marker, payload) = frames.next().ok_or_else(|| malformed("no frame header"))?;
        if frames.next().is_some() {
            return Err(malformed("several frame headers"));
        }
        let [precision, h1, h0, w1, w0, count, rest @ ..] = payload else {
            return Err(malformed("truncated frame header"));
        };
        let count = usize::from(*count);
        if count == 0 || count > 4 || rest.len() != count * 3 {
            return Err(malformed("bad frame component count"));
        }
        let components: Vec<Component> =
            rest.as_chunks::<3>().0.iter().map(|c| Component { id: c[0], h: c[1] >> 4, v: c[1] & 0x0F }).collect();
        if components.iter().any(|c| !(1..=4).contains(&c.h) || !(1..=4).contains(&c.v)) {
            return Err(malformed("bad sampling factor"));
        }
        if components.iter().enumerate().any(|(i, c)| components[..i].iter().any(|other| other.id == c.id)) {
            return Err(malformed("duplicate component id"));
        }
        let frame = Self {
            marker,
            precision: *precision,
            width: u16::from_be_bytes([*w1, *w0]),
            height: u16::from_be_bytes([*h1, *h0]),
            components,
        };
        if frame.width == 0 {
            return Err(malformed("zero width"));
        }
        Ok(frame)
    }

    pub(crate) fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        assert!(write_segment(&mut out, marker, payload).is_ok());
        out
    }

    #[test]
    fn splits_segments_scans_restart_markers_and_trailing_bytes() {
        let mut jpeg = vec![0xFF, SOI];
        jpeg.extend(segment(APP0, b"JFIF\0"));
        jpeg.extend(segment(SOS, &[1, 1, 0, 0, 63, 0]));
        let data = [0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF];
        jpeg.extend(data);
        jpeg.extend([0xFF, EOI, 9, 9]);
        let structure = parse(&jpeg);
        assert_eq!(
            structure,
            Ok(Structure {
                items: vec![
                    Item::Segment { marker: APP0, payload: b"JFIF\0" },
                    Item::Scan { header: &[1, 1, 0, 0, 63, 0], data: &data },
                ],
                trailing: &[9, 9],
            })
        );
    }

    #[test]
    fn truncated_and_unterminated_files_are_refused() {
        assert!(parse(b"not a jpeg").is_err());
        assert!(parse(&[0xFF, SOI, 0xFF, APP0, 0x00]).is_err());
        assert!(parse(&[0xFF, SOI, 0xFF, APP0, 0x00, 0x08, 1]).is_err());
        assert!(parse(&[0xFF, SOI, 0xFF, APP0, 0x00, 0x01]).is_err());
        let mut no_eoi = vec![0xFF, SOI];
        no_eoi.extend(segment(SOS, &[1, 1, 0, 0, 63, 0]));
        no_eoi.extend([0x12, 0x34]);
        assert!(parse(&no_eoi).is_err());
        no_eoi.push(0xFF);
        assert!(parse(&no_eoi).is_err());
    }

    #[test]
    fn reads_the_frame_and_refuses_impossible_ones() {
        let frame = |payload: &[u8]| {
            let mut jpeg = vec![0xFF, SOI];
            jpeg.extend(segment(SOF0, payload));
            jpeg.extend([0xFF, EOI]);
            let structure = parse(&jpeg)?;
            Frame::find(&structure)
        };
        let gray = frame(&[8, 0, 16, 0, 32, 1, 1, 0x11, 0]);
        assert_eq!(
            gray.map(|f| (f.width, f.height, f.components)),
            Ok((32, 16, vec![Component { id: 1, h: 1, v: 1 }]))
        );
        assert!(frame(&[8, 0, 16, 0, 32, 0]).is_err(), "no components");
        assert!(frame(&[8, 0, 16, 0, 32, 1, 1, 0x51, 0]).is_err(), "sampling factor 5");
        assert!(frame(&[8, 0, 16, 0, 32, 2, 1, 0x11, 0, 1, 0x11, 0]).is_err(), "duplicate id");
        assert!(frame(&[8, 0, 16, 0, 0, 1, 1, 0x11, 0]).is_err(), "zero width");
        assert!(frame(&[8, 0, 16]).is_err(), "truncated");
    }
}
