//! Read the primary item's `prem` relationship; never infer it from pixel values.
// Consts
const MAX_BOXES: usize = 4096;
const MAX_REFERENCES: usize = 128;
const MAX_METADATA: usize = 16 * 1024 * 1024;

type BoxSlice<'a> = (&'a [u8], &'a [u8]);

struct Boxes<'a> {
    remaining: &'a [u8],
    count: usize,
}

impl<'a> Boxes<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes, count: 0 }
    }

    fn next(&mut self) -> Result<Option<BoxSlice<'a>>, String> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        self.count += 1;
        if self.count > MAX_BOXES || self.remaining.len() < 8 {
            return Err("invalid or excessive container boxes".into());
        }
        let bytes = self.remaining;
        let size = u32::from_be_bytes(bytes[..4].try_into().map_err(|_| "box size")?);
        let (size, header) = match size {
            0 => (bytes.len(), 8),
            1 => {
                let extended = bytes.get(8..16).ok_or("truncated extended box")?;
                let size = u64::from_be_bytes(extended.try_into().map_err(|_| "extended size")?);
                (usize::try_from(size).map_err(|_| "box size overflows")?, 16)
            }
            n => (n as usize, 8),
        };
        if size < header || size > bytes.len() {
            return Err("box exceeds its parent".into());
        }
        self.remaining = &bytes[size..];
        Ok(Some((&bytes[4..8], &bytes[header..size])))
    }
}

fn full_box(bytes: &[u8], max_version: u8) -> Result<(u8, &[u8]), String> {
    if bytes.len() < 4 || bytes[0] > max_version || bytes[1..4] != [0, 0, 0] {
        return Err("unsupported full box version or flags".into());
    }
    Ok((bytes[0], &bytes[4..]))
}

fn id(bytes: &[u8]) -> Result<u32, String> {
    match bytes.len() {
        2 => Ok(u32::from(u16::from_be_bytes(bytes.try_into().map_err(|_| "item id")?))),
        4 => Ok(u32::from_be_bytes(bytes.try_into().map_err(|_| "item id")?)),
        _ => Err("invalid item id width".into()),
    }
}

pub fn premultiplied(bytes: &[u8], primary: u32, alpha: Option<u32>) -> Result<bool, String> {
    let mut top = Boxes::new(bytes);
    let mut meta = None;
    while let Some((kind, payload)) = top.next()? {
        if kind == b"meta" && (meta.replace(payload).is_some() || payload.len() > MAX_METADATA) {
            return Err("duplicate or oversized meta box".into());
        }
    }
    let (_, metadata) = full_box(meta.ok_or("missing meta box")?, 0)?;
    let mut children = Boxes::new(metadata);
    let (mut declared_primary, mut references) = (None, None);
    while let Some((kind, payload)) = children.next()? {
        match kind {
            b"pitm" => {
                let (version, payload) = full_box(payload, 1)?;
                if payload.len() != if version == 0 { 2 } else { 4 } || declared_primary.replace(id(payload)?).is_some()
                {
                    return Err("invalid or duplicate primary item".into());
                }
            }
            b"iref" => {
                if references.is_some() {
                    return Err("duplicate item reference box".into());
                }
                references = Some(payload);
            }
            _ => {}
        }
    }
    if declared_primary != Some(primary) {
        return Err("primary item disagrees with native decoder".into());
    }
    let Some(references) = references else {
        return if alpha.is_some() { Err("missing alpha auxiliary reference".into()) } else { Ok(false) };
    };
    let (version, references) = full_box(references, 1)?;
    let id_bytes = if version == 0 { 2 } else { 4 };
    let mut entries = Boxes::new(references);
    let (mut prem, mut alpha_links, mut total_targets) = (false, 0, 0);
    while let Some((kind, payload)) = entries.next()? {
        if entries.count > MAX_REFERENCES || payload.len() < id_bytes + 2 {
            return Err("invalid or excessive item references".into());
        }
        let from = id(&payload[..id_bytes])?;
        let count = u16::from_be_bytes(payload[id_bytes..id_bytes + 2].try_into().map_err(|_| "reference count")?);
        total_targets += usize::from(count);
        if total_targets > MAX_REFERENCES || payload.len() != id_bytes + 2 + usize::from(count) * id_bytes {
            return Err("invalid or excessive reference targets".into());
        }
        let targets = &payload[id_bytes + 2..];
        if kind == b"prem" && from == primary {
            if prem || count != 1 || Some(id(targets)?) != alpha || alpha == Some(primary) {
                return Err("prem reference must identify the primary image's single alpha item".into());
            }
            prem = true;
        }
        if kind == b"auxl" && Some(from) == alpha {
            if count != 1 || id(targets)? != primary {
                return Err("alpha auxiliary reference disagrees with primary image".into());
            }
            alpha_links += 1;
        }
    }
    if alpha.is_some() && alpha_links != 1 {
        return Err("missing or duplicate alpha auxiliary reference".into());
    }
    Ok(prem)
}

#[cfg(test)]
mod tests {
    // Domain
    use super::*;

    fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        [(payload.len() as u32 + 8).to_be_bytes().as_slice(), kind, payload].concat()
    }

    fn fixture(version: u8, prem: &[(u32, u32)]) -> Vec<u8> {
        let encode = |n: u32| if version == 0 { (n as u16).to_be_bytes().to_vec() } else { n.to_be_bytes().to_vec() };
        let mut refs = vec![version, 0, 0, 0];
        refs.extend(boxed(b"auxl", &[encode(2), vec![0, 1], encode(1)].concat()));
        for &(from, to) in prem {
            refs.extend(boxed(b"prem", &[encode(from), vec![0, 1], encode(to)].concat()));
        }
        let pitm = boxed(b"pitm", &[vec![version, 0, 0, 0], encode(1)].concat());
        boxed(b"meta", &[vec![0; 4], pitm, boxed(b"iref", &refs)].concat())
    }

    #[test]
    fn recognizes_both_reference_id_widths_and_straight_alpha() {
        for version in [0, 1] {
            assert_eq!(premultiplied(&fixture(version, &[(1, 2)]), 1, Some(2)), Ok(true));
            assert_eq!(premultiplied(&fixture(version, &[]), 1, Some(2)), Ok(false));
            assert_eq!(premultiplied(&fixture(version, &[(3, 2)]), 1, Some(2)), Ok(false));
        }
    }

    #[test]
    fn rejects_wrong_targets_duplicate_links_and_disagreement() {
        for refs in [vec![(1, 3)], vec![(1, 1)], vec![(1, 2), (1, 2)]] {
            assert!(premultiplied(&fixture(0, &refs), 1, Some(2)).is_err());
        }
        let bytes = fixture(0, &[(1, 2)]);
        assert!(premultiplied(&bytes, 1, None).is_err());
        assert!(premultiplied(&bytes, 9, Some(2)).is_err());
        assert!(premultiplied(&[bytes.clone(), bytes].concat(), 1, Some(2)).is_err());
    }

    #[test]
    fn ignores_tag_text_in_payload_and_rejects_truncation() {
        let bytes = fixture(0, &[]);
        let with_data = [bytes.clone(), boxed(b"mdat", b"prem\0\x01\0\x01\0\x02")].concat();
        assert_eq!(premultiplied(&with_data, 1, Some(2)), Ok(false));
        for end in 0..bytes.len() {
            assert!(premultiplied(&bytes[..end], 1, Some(2)).is_err());
        }
    }

    #[test]
    fn bounds_box_and_reference_work_and_checks_extended_sizes() {
        let too_many = boxed(b"free", &[]).repeat(MAX_BOXES + 1);
        assert!(premultiplied(&too_many, 1, None).is_err());
        assert!(premultiplied(&fixture(0, &vec![(3, 2); MAX_REFERENCES]), 1, Some(2)).is_err());
        let bytes = fixture(0, &[(1, 2)]);
        let extended =
            [1u32.to_be_bytes().as_slice(), b"meta", (bytes.len() as u64 + 8).to_be_bytes().as_slice(), &bytes[8..]]
                .concat();
        assert_eq!(premultiplied(&extended, 1, Some(2)), Ok(true));
        let mut bad = extended;
        bad[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
        assert!(premultiplied(&bad, 1, Some(2)).is_err());
    }
}
