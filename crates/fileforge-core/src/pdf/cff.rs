//! Lossless pruning of CFF font programs (ADR-0018). Subsets often keep the original font's subroutines although no
//! glyph calls them. Every unreachable subroutine becomes a single `return`; counts, charstrings, charset, encoding
//! and FDSelect stay byte-identical, so every glyph executes exactly as before. Anything unusual keeps the font as is.

// Core
use std::collections::HashSet;

/// Type 2 `return`: what an unreachable subroutine is replaced with.
const RETURN: u8 = 11;
/// Type 2 limits: operand stack depth and subroutine nesting.
const MAX_STACK: usize = 48;
const MAX_NESTING: usize = 10;
/// Charstring bytes interpreted per font. Real fonts need a few million at most (a 6,760-glyph math font: well below
/// 10 M); hostile fonts with fan-out calls would otherwise take exponential time.
const MAX_STEPS: u64 = 64_000_000;

const OP_CHARSET: u16 = 15;
const OP_ENCODING: u16 = 16;
const OP_CHARSTRINGS: u16 = 17;
const OP_PRIVATE: u16 = 18;
const OP_SUBRS: u16 = 19;
const OP_CHARSTRING_TYPE: u16 = 12 << 8 | 6;
const OP_FD_ARRAY: u16 = 12 << 8 | 36;
const OP_FD_SELECT: u16 = 12 << 8 | 37;
const OP_ROS: u16 = 12 << 8 | 30;

/// The font program with every subroutine no glyph calls replaced by `return`, if that is smaller and verifiably
/// executes identically; `None` keeps the original.
pub(crate) fn prune_subroutines(cff: &[u8]) -> Option<Vec<u8>> {
    let font = Font::parse(cff)?;
    let reach = reachable(&font)?;
    let global = prune(&font.global, &reach.global);
    let locals: Vec<Vec<&[u8]>> =
        font.fds.iter().enumerate().map(|(fd, dict)| prune(&dict.subrs, &reach.local[fd])).collect();
    let out = write(&font, &global, &locals)?;
    (out.len() < cff.len() && verify(&font, &reach, &out)).then_some(out)
}

fn prune<'a>(subrs: &[&'a [u8]], reachable: &HashSet<usize>) -> Vec<&'a [u8]> {
    subrs.iter().enumerate().map(|(i, subr)| if reachable.contains(&i) { *subr } else { &[RETURN][..] }).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Parsing

fn be16(data: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(*data.get(at)?) << 8 | usize::from(*data.get(at.checked_add(1)?)?))
}

/// A CFF INDEX: its items and the offset just past it.
struct Index<'a> {
    items: Vec<&'a [u8]>,
    end: usize,
}

fn parse_index(data: &[u8], at: usize) -> Option<Index<'_>> {
    let count = be16(data, at)?;
    if count == 0 {
        return Some(Index { items: Vec::new(), end: at + 2 });
    }
    let size = usize::from(*data.get(at + 2)?);
    if !(1..=4).contains(&size) {
        return None;
    }
    let offsets_at = at + 3;
    let data_at = offsets_at.checked_add((count + 1).checked_mul(size)?)?.checked_sub(1)?;
    let offset = |i: usize| -> Option<usize> {
        let start = offsets_at + i * size;
        data.get(start..start + size)?.iter().try_fold(0usize, |value, byte| Some(value << 8 | usize::from(*byte)))
    };
    if offset(0)? != 1 {
        return None;
    }
    let mut items = Vec::with_capacity(count);
    let mut previous = 1;
    for i in 1..=count {
        let next = offset(i)?;
        if next < previous {
            return None;
        }
        items.push(data.get(data_at.checked_add(previous)?..data_at.checked_add(next)?)?);
        previous = next;
    }
    Some(Index { items, end: data_at + previous })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Number {
    Int(i64),
    Real,
}

/// A DICT entry; `raw` holds the operands' original bytes so entries that are not offsets are copied verbatim.
#[derive(Debug, Clone, PartialEq)]
struct Entry<'a> {
    op: u16,
    operands: Vec<Number>,
    raw: &'a [u8],
}

impl Entry<'_> {
    fn int(&self, position: usize) -> Option<i64> {
        match self.operands.get(position)? {
            Number::Int(value) => Some(*value),
            Number::Real => None,
        }
    }

    fn offset(&self, position: usize) -> Option<usize> {
        usize::try_from(self.int(position)?).ok()
    }
}

fn parse_dict(data: &[u8]) -> Option<Vec<Entry<'_>>> {
    let mut entries = Vec::new();
    let mut operands = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        match b0 {
            0..=21 => {
                let op = if b0 == 12 { 12 << 8 | u16::from(*data.get(i + 1)?) } else { u16::from(b0) };
                entries.push(Entry { op, operands: std::mem::take(&mut operands), raw: &data[start..i] });
                i += if b0 == 12 { 2 } else { 1 };
                start = i;
            }
            28 => {
                operands.push(Number::Int(i64::from(be16(data, i + 1)? as u16 as i16)));
                i += 3;
            }
            29 => {
                let bytes: [u8; 4] = data.get(i + 1..i + 5)?.try_into().ok()?;
                operands.push(Number::Int(i64::from(i32::from_be_bytes(bytes))));
                i += 5;
            }
            30 => {
                i += 1;
                loop {
                    let nibbles = *data.get(i)?;
                    i += 1;
                    if nibbles & 0x0f == 0x0f || nibbles >> 4 == 0x0f {
                        break;
                    }
                }
                operands.push(Number::Real);
            }
            32..=246 => {
                operands.push(Number::Int(i64::from(b0) - 139));
                i += 1;
            }
            247..=250 => {
                operands.push(Number::Int((i64::from(b0) - 247) * 256 + i64::from(*data.get(i + 1)?) + 108));
                i += 2;
            }
            251..=254 => {
                operands.push(Number::Int(-(i64::from(b0) - 251) * 256 - i64::from(*data.get(i + 1)?) - 108));
                i += 2;
            }
            _ => return None,
        }
        if operands.len() > MAX_STACK {
            return None;
        }
    }
    operands.is_empty().then_some(entries)
}

fn find<'e, 'a>(entries: &'e [Entry<'a>], op: u16) -> Option<&'e Entry<'a>> {
    entries.iter().find(|entry| entry.op == op)
}

/// charset or Encoding: a predefined id kept in the DICT, or data that moves with the font.
#[derive(Debug, PartialEq)]
enum Section<'a> {
    Predefined,
    Data { op: u16, bytes: &'a [u8] },
}

/// One set of local subroutines: the top-level Private DICT of a name-keyed font, or one FDArray entry of a CID font.
struct FontDict<'a> {
    /// The FDArray Font DICT (empty for a name-keyed font, whose Private is referenced from the Top DICT).
    entries: Vec<Entry<'a>>,
    private: Vec<Entry<'a>>,
    subrs: Vec<&'a [u8]>,
}

struct Font<'a> {
    prefix: &'a [u8],
    top: Vec<Entry<'a>>,
    strings: &'a [u8],
    global: Vec<&'a [u8]>,
    charstrings: Vec<&'a [u8]>,
    charstrings_raw: &'a [u8],
    charset: Section<'a>,
    encoding: Section<'a>,
    fd_select: Option<&'a [u8]>,
    fds: Vec<FontDict<'a>>,
    fd_of_glyph: Vec<usize>,
}

impl<'a> Font<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        // Version 1 only; CFF2 lives in OpenType fonts, never bare in `FontFile3`.
        if *data.first()? != 1 {
            return None;
        }
        let header_size = usize::from(*data.get(2)?);
        let names = parse_index(data, header_size)?;
        let tops = parse_index(data, names.end)?;
        let strings = parse_index(data, tops.end)?;
        let global = parse_index(data, strings.end)?;
        // A FontSet with several fonts never occurs in PDF font programs.
        let (&[_], &[top]) = (names.items.as_slice(), tops.items.as_slice()) else { return None };
        let top = parse_dict(top)?;
        if find(&top, OP_CHARSTRING_TYPE).is_some_and(|entry| entry.int(0) != Some(2)) {
            return None;
        }
        let charstrings_at = find(&top, OP_CHARSTRINGS)?.offset(0)?;
        let charstrings_index = parse_index(data, charstrings_at)?;
        let glyphs = charstrings_index.items.len();
        if glyphs == 0 {
            return None;
        }
        let cid = find(&top, OP_ROS).is_some();
        let charset = match find(&top, OP_CHARSET).map(|entry| entry.offset(0)) {
            None => Section::Predefined,
            Some(Some(0..=2)) if !cid => Section::Predefined,
            Some(Some(at)) if at > 2 => Section::Data { op: OP_CHARSET, bytes: charset_bytes(data, at, glyphs)? },
            Some(_) => return None,
        };
        let encoding = match find(&top, OP_ENCODING).map(|entry| entry.offset(0)) {
            None => Section::Predefined,
            Some(Some(0 | 1)) if !cid => Section::Predefined,
            Some(Some(at)) if at > 1 && !cid => Section::Data { op: OP_ENCODING, bytes: encoding_bytes(data, at)? },
            Some(_) => return None,
        };

        let (fds, fd_select, fd_of_glyph) = if cid {
            if find(&top, OP_PRIVATE).is_some() {
                return None;
            }
            let fd_array = parse_index(data, find(&top, OP_FD_ARRAY)?.offset(0)?)?;
            let fds = fd_array
                .items
                .iter()
                .map(|dict| {
                    let entries = parse_dict(dict)?;
                    let (private, subrs) = private_dict(data, find(&entries, OP_PRIVATE))?;
                    Some(FontDict { entries, private, subrs })
                })
                .collect::<Option<Vec<_>>>()?;
            let (select, fd_of_glyph) = fd_select(data, find(&top, OP_FD_SELECT)?.offset(0)?, glyphs, fds.len())?;
            (fds, Some(select), fd_of_glyph)
        } else {
            if find(&top, OP_FD_ARRAY).is_some() || find(&top, OP_FD_SELECT).is_some() {
                return None;
            }
            let (private, subrs) = private_dict(data, find(&top, OP_PRIVATE))?;
            (vec![FontDict { entries: Vec::new(), private, subrs }], None, vec![0; glyphs])
        };

        Some(Self {
            prefix: data.get(..names.end)?,
            top,
            strings: data.get(tops.end..strings.end)?,
            global: global.items,
            charstrings: charstrings_index.items,
            charstrings_raw: data.get(charstrings_at..charstrings_index.end)?,
            charset,
            encoding,
            fd_select,
            fds,
            fd_of_glyph,
        })
    }
}

/// A Private DICT and its local subroutines; a missing Private has neither.
fn private_dict<'a>(data: &'a [u8], entry: Option<&Entry<'a>>) -> Option<(Vec<Entry<'a>>, Vec<&'a [u8]>)> {
    let Some(entry) = entry else { return Some((Vec::new(), Vec::new())) };
    let (size, at) = (entry.offset(0)?, entry.offset(1)?);
    let private = parse_dict(data.get(at..at.checked_add(size)?)?)?;
    let subrs = match find(&private, OP_SUBRS) {
        Some(subrs) => parse_index(data, at.checked_add(subrs.offset(0)?)?)?.items,
        None => Vec::new(),
    };
    Some((private, subrs))
}

fn charset_bytes(data: &[u8], at: usize, glyphs: usize) -> Option<&[u8]> {
    let format = *data.get(at)?;
    let len = match format {
        0 => 1 + 2 * (glyphs - 1),
        1 | 2 => {
            let range = if format == 1 { 3 } else { 4 };
            let (mut covered, mut len) = (1, 1);
            while covered < glyphs {
                let left = if format == 1 { usize::from(*data.get(at + len + 2)?) } else { be16(data, at + len + 2)? };
                covered += left + 1;
                len += range;
            }
            len
        }
        _ => return None,
    };
    data.get(at..at.checked_add(len)?)
}

fn encoding_bytes(data: &[u8], at: usize) -> Option<&[u8]> {
    let format = *data.get(at)?;
    let count = usize::from(*data.get(at + 1)?);
    let mut len = match format & 0x7f {
        0 => 2 + count,
        1 => 2 + 2 * count,
        _ => return None,
    };
    if format & 0x80 != 0 {
        len += 1 + 3 * usize::from(*data.get(at + len)?);
    }
    data.get(at..at.checked_add(len)?)
}

/// FDSelect bytes and the Font DICT of every glyph.
fn fd_select(data: &[u8], at: usize, glyphs: usize, fds: usize) -> Option<(&[u8], Vec<usize>)> {
    let mut fd_of_glyph = vec![0; glyphs];
    let len = match *data.get(at)? {
        0 => {
            for (glyph, fd) in fd_of_glyph.iter_mut().enumerate() {
                *fd = usize::from(*data.get(at + 1 + glyph)?);
            }
            1 + glyphs
        }
        3 => {
            let ranges = be16(data, at + 1)?;
            let mut first = be16(data, at + 3)?;
            if first != 0 || ranges == 0 {
                return None;
            }
            for range in 0..ranges {
                let fd = usize::from(*data.get(at + 5 + range * 3)?);
                let next = be16(data, at + 6 + range * 3)?;
                if next <= first || next > glyphs {
                    return None;
                }
                fd_of_glyph.get_mut(first..next)?.fill(fd);
                first = next;
            }
            if first != glyphs {
                return None;
            }
            5 + ranges * 3
        }
        _ => return None,
    };
    if fd_of_glyph.iter().any(|fd| *fd >= fds) {
        return None;
    }
    Some((data.get(at..at.checked_add(len)?)?, fd_of_glyph))
}

// ---------------------------------------------------------------------------------------------------------------
// Reachability

#[derive(Debug, PartialEq)]
struct Reach {
    global: HashSet<usize>,
    /// One set per Font DICT.
    local: Vec<HashSet<usize>>,
}

fn bias(count: usize) -> i64 {
    match count {
        0..1240 => 107,
        1240..33900 => 1131,
        _ => 32768,
    }
}

struct Scanner<'f, 'a> {
    font: &'f Font<'a>,
    reach: Reach,
    stack: Vec<f64>,
    stems: usize,
    steps: u64,
}

/// What one charstring run ended with.
enum Flow {
    Return,
    EndChar,
}

impl Scanner<'_, '_> {
    fn run(&mut self, code: &[u8], fd: usize, depth: usize) -> Option<Flow> {
        if depth > MAX_NESTING {
            return None;
        }
        self.steps = self.steps.checked_add(code.len() as u64)?;
        if self.steps > MAX_STEPS {
            return None;
        }
        let mut i = 0;
        while i < code.len() {
            let b0 = code[i];
            match b0 {
                28 => {
                    self.stack.push(f64::from(be16(code, i + 1)? as u16 as i16));
                    i += 3;
                }
                32..=246 => {
                    self.stack.push(f64::from(b0) - 139.0);
                    i += 1;
                }
                247..=250 => {
                    self.stack.push((f64::from(b0) - 247.0) * 256.0 + f64::from(*code.get(i + 1)?) + 108.0);
                    i += 2;
                }
                251..=254 => {
                    self.stack.push(-(f64::from(b0) - 251.0) * 256.0 - f64::from(*code.get(i + 1)?) - 108.0);
                    i += 2;
                }
                255 => {
                    let bytes: [u8; 4] = code.get(i + 1..i + 5)?.try_into().ok()?;
                    self.stack.push(f64::from(i32::from_be_bytes(bytes)) / 65536.0);
                    i += 5;
                }
                // hstem, vstem, hstemhm, vstemhm
                1 | 3 | 18 | 23 => {
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    i += 1;
                }
                // hintmask, cntrmask: pending operands are an implicit vstem; one mask bit per stem follows.
                19 | 20 => {
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    i += 1 + self.stems.div_ceil(8);
                    if i > code.len() {
                        return None;
                    }
                }
                // callsubr, callgsubr
                10 | 29 => {
                    let number = self.stack.pop()?;
                    if number.fract() != 0.0 {
                        return None;
                    }
                    let font = self.font;
                    let subrs = if b0 == 10 { &font.fds.get(fd)?.subrs } else { &font.global };
                    let index = usize::try_from(number as i64 + bias(subrs.len())).ok()?;
                    let subr = *subrs.get(index)?;
                    if b0 == 10 {
                        self.reach.local.get_mut(fd)?.insert(index);
                    } else {
                        self.reach.global.insert(index);
                    }
                    if let Flow::EndChar = self.run(subr, fd, depth + 1)? {
                        return Some(Flow::EndChar);
                    }
                    i += 1;
                }
                11 => return Some(Flow::Return),
                14 => return Some(Flow::EndChar),
                12 => match *code.get(i + 1)? {
                    // dotsection (deprecated), hflex, flex, hflex1, flex1
                    0 | 34..=37 => {
                        self.stack.clear();
                        i += 2;
                    }
                    // Arithmetic and storage operators make subroutine numbers data-dependent.
                    _ => return None,
                },
                // Path construction operators.
                4..=8 | 21 | 22 | 24..=27 | 30 | 31 => {
                    self.stack.clear();
                    i += 1;
                }
                _ => return None,
            }
            if self.stack.len() > MAX_STACK {
                return None;
            }
        }
        // Falling off the end acts as `return` (subroutines) or an unfinished glyph (renderers stop there too).
        Some(Flow::Return)
    }
}

fn reachable(font: &Font<'_>) -> Option<Reach> {
    let mut scanner = Scanner {
        font,
        reach: Reach { global: HashSet::new(), local: vec![HashSet::new(); font.fds.len()] },
        stack: Vec::new(),
        stems: 0,
        steps: 0,
    };
    for (glyph, code) in font.charstrings.iter().enumerate() {
        scanner.stack.clear();
        scanner.stems = 0;
        scanner.run(code, *font.fd_of_glyph.get(glyph)?, 0)?;
    }
    Some(scanner.reach)
}

// ---------------------------------------------------------------------------------------------------------------
// Writing

fn index_len(items: &[&[u8]]) -> usize {
    if items.is_empty() {
        return 2;
    }
    let data: usize = items.iter().map(|item| item.len()).sum();
    3 + (items.len() + 1) * offset_size(data + 1) + data
}

fn offset_size(max: usize) -> usize {
    match max {
        0..=0xff => 1,
        0x100..=0xffff => 2,
        0x1_0000..=0xff_ffff => 3,
        _ => 4,
    }
}

fn write_index(out: &mut Vec<u8>, items: &[&[u8]]) -> Option<()> {
    out.extend_from_slice(&u16::try_from(items.len()).ok()?.to_be_bytes());
    if items.is_empty() {
        return Some(());
    }
    let data: usize = items.iter().map(|item| item.len()).sum();
    let size = offset_size(data + 1);
    out.push(size as u8);
    let mut offset = 1usize;
    for item in std::iter::once(&[][..]).chain(items.iter().copied()) {
        offset += item.len();
        out.extend_from_slice(&u32::try_from(offset).ok()?.to_be_bytes()[4 - size..]);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    Some(())
}

/// A DICT whose offset operators get new values, always as 5-byte integers so sizes do not depend on values.
struct DictWriter<'e, 'a> {
    entries: &'e [Entry<'a>],
    /// Operators to rewrite (values filled in later, in the same order as the operands).
    rewrite: &'e [u16],
}

impl DictWriter<'_, '_> {
    fn len(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| {
                let operands =
                    if self.rewrite.contains(&entry.op) { 5 * entry.operands.len() } else { entry.raw.len() };
                operands + if entry.op > 0xff { 2 } else { 1 }
            })
            .sum()
    }

    fn write(&self, out: &mut Vec<u8>, value: impl Fn(u16) -> Option<Vec<usize>>) -> Option<()> {
        for entry in self.entries {
            if self.rewrite.contains(&entry.op) {
                let values = value(entry.op)?;
                if values.len() != entry.operands.len() {
                    return None;
                }
                for value in values {
                    out.push(29);
                    out.extend_from_slice(&i32::try_from(value).ok()?.to_be_bytes());
                }
            } else {
                out.extend_from_slice(entry.raw);
            }
            if entry.op > 0xff {
                out.extend_from_slice(&entry.op.to_be_bytes());
            } else {
                out.push(entry.op as u8);
            }
        }
        Some(())
    }
}

const TOP_OFFSETS: &[u16] = &[OP_CHARSET, OP_ENCODING, OP_CHARSTRINGS, OP_PRIVATE, OP_FD_ARRAY, OP_FD_SELECT];

fn write(font: &Font<'_>, global: &[&[u8]], locals: &[Vec<&[u8]>]) -> Option<Vec<u8>> {
    let data_op = |section: &Section<'_>| match section {
        Section::Data { op, .. } => Some(*op),
        Section::Predefined => None,
    };
    let section_bytes = |section: &Section<'_>| match section {
        Section::Data { bytes, .. } => bytes.len(),
        Section::Predefined => 0,
    };
    // Predefined charset/Encoding ids stay as they are; only data sections move.
    let rewrite: Vec<u16> = TOP_OFFSETS
        .iter()
        .copied()
        .filter(|op| match *op {
            OP_CHARSET => data_op(&font.charset).is_some(),
            OP_ENCODING => data_op(&font.encoding).is_some(),
            _ => true,
        })
        .collect();
    let top = DictWriter { entries: &font.top, rewrite: &rewrite };
    let top_len = top.len();
    let privates: Vec<DictWriter<'_, '_>> =
        font.fds.iter().map(|fd| DictWriter { entries: &fd.private, rewrite: &[OP_SUBRS] }).collect();
    let fd_dicts: Vec<DictWriter<'_, '_>> =
        font.fds.iter().map(|fd| DictWriter { entries: &fd.entries, rewrite: &[OP_PRIVATE] }).collect();
    let cid = font.fd_select.is_some();

    // Layout: everything after the Top DICT has a size known in advance.
    let top_index_len = 3 + 2 * offset_size(top_len + 1) + top_len;
    let mut at = font.prefix.len() + top_index_len + font.strings.len() + index_len(global);
    let charset_at = at;
    at += section_bytes(&font.charset);
    let encoding_at = at;
    at += section_bytes(&font.encoding);
    let charstrings_at = at;
    at += font.charstrings_raw.len();
    let fd_select_at = at;
    at += font.fd_select.map_or(0, <[u8]>::len);
    let fd_array_at = at;
    if cid {
        let dict_lens: usize = fd_dicts.iter().map(DictWriter::len).sum();
        at += 3 + (fd_dicts.len() + 1) * offset_size(dict_lens + 1) + dict_lens;
    }
    let mut private_at = Vec::with_capacity(privates.len());
    let has_subrs = |private: &DictWriter<'_, '_>| find(private.entries, OP_SUBRS).is_some();
    for (private, subrs) in privates.iter().zip(locals) {
        private_at.push(at);
        at += private.len() + if has_subrs(private) { index_len(subrs) } else { 0 };
    }

    let mut out = Vec::with_capacity(at);
    out.extend_from_slice(font.prefix);
    out.extend_from_slice(&1u16.to_be_bytes());
    let size = offset_size(top_len + 1);
    out.push(size as u8);
    for offset in [1, top_len + 1] {
        out.extend_from_slice(&(offset as u32).to_be_bytes()[4 - size..]);
    }
    top.write(&mut out, |op| match op {
        OP_CHARSET => Some(vec![charset_at]),
        OP_ENCODING => Some(vec![encoding_at]),
        OP_CHARSTRINGS => Some(vec![charstrings_at]),
        OP_PRIVATE => Some(vec![privates.first()?.len(), *private_at.first()?]),
        OP_FD_ARRAY => Some(vec![fd_array_at]),
        OP_FD_SELECT => Some(vec![fd_select_at]),
        _ => None,
    })?;
    out.extend_from_slice(font.strings);
    write_index(&mut out, global)?;
    for section in [&font.charset, &font.encoding] {
        if let Section::Data { bytes, .. } = section {
            out.extend_from_slice(bytes);
        }
    }
    out.extend_from_slice(font.charstrings_raw);
    if cid {
        out.extend_from_slice(font.fd_select?);
        let mut dicts = Vec::with_capacity(fd_dicts.len());
        for (fd, dict) in fd_dicts.iter().enumerate() {
            let mut bytes = Vec::with_capacity(dict.len());
            dict.write(&mut bytes, |_| Some(vec![privates.get(fd)?.len(), *private_at.get(fd)?]))?;
            dicts.push(bytes);
        }
        write_index(&mut out, &dicts.iter().map(Vec::as_slice).collect::<Vec<_>>())?;
    }
    for (private, subrs) in privates.iter().zip(locals) {
        let len = private.len();
        private.write(&mut out, |_| Some(vec![len]))?;
        if has_subrs(private) {
            write_index(&mut out, subrs)?;
        }
    }
    (out.len() == at).then_some(out)
}

// ---------------------------------------------------------------------------------------------------------------
// Self-check

/// The rewritten font must execute every glyph exactly as the original: same charstrings, charset, encoding, FDSelect
/// and non-offset DICT entries, same subroutine counts, and byte-identical subroutines wherever a glyph can go.
fn verify(original: &Font<'_>, reach: &Reach, out: &[u8]) -> bool {
    let Some(font) = Font::parse(out) else { return false };
    let same_entries = |a: &[Entry<'_>], b: &[Entry<'_>], offsets: &[u16]| {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.op == b.op && (offsets.contains(&a.op) || a.raw == b.raw))
    };
    let structure = font.prefix == original.prefix
        && font.strings == original.strings
        && font.charstrings == original.charstrings
        && font.charset == original.charset
        && font.encoding == original.encoding
        && font.fd_select == original.fd_select
        && font.fd_of_glyph == original.fd_of_glyph
        && font.global.len() == original.global.len()
        && font.fds.len() == original.fds.len()
        && same_entries(&font.top, &original.top, TOP_OFFSETS)
        && font.fds.iter().zip(&original.fds).all(|(new, old)| {
            new.subrs.len() == old.subrs.len()
                && same_entries(&new.entries, &old.entries, &[OP_PRIVATE])
                && same_entries(&new.private, &old.private, &[OP_SUBRS])
        });
    let reached_same = reach.global.iter().all(|i| font.global.get(*i) == original.global.get(*i))
        && reach.local.iter().enumerate().all(|(fd, subrs)| {
            subrs.iter().all(|i| {
                font.fds.get(fd).and_then(|d| d.subrs.get(*i)) == original.fds.get(fd).and_then(|d| d.subrs.get(*i))
            })
        });
    structure && reached_same && reachable(&font).as_ref() == Some(reach)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Type 2 charstring operands for `numbers`, using the one-byte form where it fits.
    fn push(numbers: &[i32]) -> Vec<u8> {
        let mut out = Vec::new();
        for &n in numbers {
            if (-107..=107).contains(&n) {
                out.push((n + 139) as u8);
            } else {
                out.push(28);
                out.extend_from_slice(&(n as i16).to_be_bytes());
            }
        }
        out
    }

    fn dict_int(out: &mut Vec<u8>, value: usize) {
        out.push(29);
        out.extend_from_slice(&(value as i32).to_be_bytes());
    }

    fn index(items: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        write_index(&mut out, &items.iter().map(Vec::as_slice).collect::<Vec<_>>()).expect("small index");
        out
    }

    /// A name-keyed CFF font laid out by hand (independently of `write`): header, Name, Top DICT (CharStrings,
    /// Private), String, global subrs, CharStrings, Private DICT with Subrs, local subrs.
    fn name_keyed(charstrings: &[Vec<u8>], global: &[Vec<u8>], local: &[Vec<u8>]) -> Vec<u8> {
        let name = index(&[b"Test".to_vec()]);
        let strings = index(&[]);
        let global = index(global);
        let charstrings = index(charstrings);
        let local = index(local);
        // Top DICT: 5-byte CharStrings offset + op, Private (2 × 5 bytes) + op.
        let top_len = 6 + 11;
        let top_index_len = 3 + 2 + top_len;
        let charstrings_at = 4 + name.len() + top_index_len + strings.len() + global.len();
        let private_at = charstrings_at + charstrings.len();
        let private_len = 6;
        let mut top = Vec::new();
        dict_int(&mut top, charstrings_at);
        top.push(17);
        dict_int(&mut top, private_len);
        dict_int(&mut top, private_at);
        top.push(18);
        let mut out = vec![1, 0, 4, 4];
        out.extend_from_slice(&name);
        out.extend_from_slice(&index(&[top]));
        out.extend_from_slice(&strings);
        out.extend_from_slice(&global);
        out.extend_from_slice(&charstrings);
        dict_int(&mut out, private_len);
        out.push(19);
        out.extend_from_slice(&local);
        out
    }

    /// A subroutine drawing `lines` zero-length segments: each `rlineto` clears the operand stack.
    fn subr(lines: usize) -> Vec<u8> {
        let mut out = LINE.repeat(lines);
        out.push(RETURN);
        out
    }

    fn glyph(body: &[u8]) -> Vec<u8> {
        let mut out = body.to_vec();
        out.push(14);
        out
    }

    /// A simple path segment the scanner treats like an outline: `0 0 rlineto` (operands 139 139, op 5).
    const LINE: [u8; 3] = [139, 139, 5];

    #[test]
    fn unreachable_subroutines_become_returns_and_glyphs_stay_identical() {
        // Glyph 1 calls global subr 2 (biased -105) and local subr 0 (biased -107).
        let charstrings = vec![glyph(&[]), glyph(&[push(&[-105]), vec![29], push(&[-107]), vec![10]].concat())];
        let global: Vec<Vec<u8>> = (0..4).map(|i| subr(10 + i)).collect();
        let local: Vec<Vec<u8>> = (0..3).map(|i| subr(5 + i)).collect();
        let input = name_keyed(&charstrings, &global, &local);

        let output = prune_subroutines(&input).expect("prunes");

        assert!(output.len() < input.len(), "{} → {}", input.len(), output.len());
        let font = Font::parse(&output).expect("output parses");
        assert_eq!(font.charstrings, charstrings.iter().map(Vec::as_slice).collect::<Vec<_>>());
        assert_eq!(font.global.len(), 4);
        assert_eq!(font.global[2], global[2].as_slice());
        for unused in [0, 1, 3] {
            assert_eq!(font.global[unused], [RETURN]);
        }
        assert_eq!(font.fds[0].subrs[0], local[0].as_slice());
        assert_eq!((font.fds[0].subrs[1], font.fds[0].subrs[2]), (&[RETURN][..], &[RETURN][..]));
    }

    /// A CID-keyed font laid out by hand: ROS, charset format 2, FDSelect format 3 (glyphs 0–1 → FD 0, glyph 2 → FD 1)
    /// and two Font DICTs, each with its own Private DICT and local subrs.
    fn cid_keyed(charstrings: &[Vec<u8>], global: &[Vec<u8>], locals: [&[Vec<u8>]; 2]) -> Vec<u8> {
        let name = index(&[b"CIDTest".to_vec()]);
        let strings = index(&[]);
        let global = index(global);
        let charstrings = index(charstrings);
        let locals = locals.map(index);
        let top_len = 17 + 6 + 6 + 7 + 7;
        let top_index_len = 3 + 2 + top_len;
        let charset_at = 4 + name.len() + top_index_len + strings.len() + global.len();
        let charset = [2, 0, 1, 0, 1];
        let charstrings_at = charset_at + charset.len();
        let fd_select_at = charstrings_at + charstrings.len();
        let fd_select = [3, 0, 2, 0, 0, 0, 0, 2, 1, 0, 3];
        let fd_array_at = fd_select_at + fd_select.len();
        let fd_array_len = 3 + 3 + 2 * 11;
        let private_len = 6;
        let private0_at = fd_array_at + fd_array_len;
        let private1_at = private0_at + private_len + locals[0].len();
        let mut top = Vec::new();
        for value in [391, 392, 0] {
            dict_int(&mut top, value);
        }
        top.extend_from_slice(&[12, 30]);
        dict_int(&mut top, charset_at);
        top.push(15);
        dict_int(&mut top, charstrings_at);
        top.push(17);
        dict_int(&mut top, fd_array_at);
        top.extend_from_slice(&[12, 36]);
        dict_int(&mut top, fd_select_at);
        top.extend_from_slice(&[12, 37]);
        let font_dict = |at: usize| {
            let mut dict = Vec::new();
            dict_int(&mut dict, private_len);
            dict_int(&mut dict, at);
            dict.push(18);
            dict
        };
        let mut out = vec![1, 0, 4, 4];
        out.extend_from_slice(&name);
        out.extend_from_slice(&index(&[top]));
        out.extend_from_slice(&strings);
        out.extend_from_slice(&global);
        out.extend_from_slice(&charset);
        out.extend_from_slice(&charstrings);
        out.extend_from_slice(&fd_select);
        out.extend_from_slice(&index(&[font_dict(private0_at), font_dict(private1_at)]));
        for local in &locals {
            dict_int(&mut out, private_len);
            out.push(19);
            out.extend_from_slice(local);
        }
        out
    }

    #[test]
    fn cid_fonts_prune_the_local_subroutines_of_each_font_dict() {
        // Glyph 1 (FD 0) calls local subr 1; glyph 2 (FD 1) calls local subr 0 and global subr 0.
        let charstrings = vec![
            glyph(&[]),
            glyph(&[push(&[-106]), vec![10]].concat()),
            glyph(&[push(&[-107]), vec![10], push(&[-107]), vec![29]].concat()),
        ];
        let global = vec![subr(9), subr(11)];
        let fd0 = vec![subr(10), subr(12)];
        let fd1 = vec![subr(13), subr(14)];
        let input = cid_keyed(&charstrings, &global, [&fd0, &fd1]);

        let output = prune_subroutines(&input).expect("prunes");

        let font = Font::parse(&output).expect("parses");
        assert_eq!(font.fd_of_glyph, [0, 0, 1]);
        assert_eq!(font.charstrings, charstrings.iter().map(Vec::as_slice).collect::<Vec<_>>());
        assert_eq!((font.global[0], font.global[1]), (global[0].as_slice(), &[RETURN][..]));
        assert_eq!((font.fds[0].subrs[0], font.fds[0].subrs[1]), (&[RETURN][..], fd0[1].as_slice()));
        assert_eq!((font.fds[1].subrs[0], font.fds[1].subrs[1]), (fd1[0].as_slice(), &[RETURN][..]));
        assert!(matches!(font.charset, Section::Data { bytes: [2, 0, 1, 0, 1], .. }));
    }

    #[test]
    fn nested_calls_share_the_operand_stack() {
        // The glyph pushes the global subr number; local subr 0 consumes it with callgsubr.
        let charstrings = vec![glyph(&[push(&[-106, -107]), vec![10]].concat())];
        let global: Vec<Vec<u8>> = (0..3).map(|i| subr(12 + i)).collect();
        let local = vec![[vec![29], vec![RETURN]].concat(), subr(20)];
        let input = name_keyed(&charstrings, &global, &local);

        let output = prune_subroutines(&input).expect("prunes");
        let font = Font::parse(&output).expect("parses");

        assert_eq!(font.global[1], global[1].as_slice(), "reached through the caller's operand");
        assert_eq!((font.global[0], font.global[2]), (&[RETURN][..], &[RETURN][..]));
        assert_eq!(font.fds[0].subrs[1], [RETURN]);
    }

    #[test]
    fn hint_mask_bytes_are_skipped_not_executed() {
        // 9 stem pairs need a 2-byte mask; the mask bytes equal callsubr (10) and callgsubr (29) and must be ignored.
        let stems = push(&[0; 18]);
        let charstrings = vec![glyph(&[stems, vec![18, 19, 10, 29]].concat())];
        let input = name_keyed(&charstrings, &[subr(25)], &[subr(25)]);

        let output = prune_subroutines(&input).expect("prunes");
        let font = Font::parse(&output).expect("parses");

        assert_eq!((font.global[0], font.fds[0].subrs[0]), (&[RETURN][..], &[RETURN][..]));
    }

    #[test]
    fn large_subroutine_sets_use_the_larger_bias() {
        // 1,300 subrs: bias 1131, so operand -1131 calls subr 0 and operand 168 calls subr 1299.
        let global: Vec<Vec<u8>> = (0..1300).map(|_| subr(2)).collect();
        let charstrings = vec![glyph(&[push(&[-1131]), vec![29], push(&[168]), vec![29]].concat())];
        let input = name_keyed(&charstrings, &global, &[]);

        let output = prune_subroutines(&input).expect("prunes");
        let font = Font::parse(&output).expect("parses");

        assert_eq!((font.global[0].len(), font.global[1299].len(), font.global[1].len()), (7, 7, 1));
    }

    #[test]
    fn fonts_the_scan_cannot_model_are_kept() {
        let global = vec![subr(10), subr(10)];
        let cases: [(&str, Vec<u8>); 6] = [
            ("arithmetic (add)", glyph(&[push(&[1, 2]), vec![12, 10], vec![29]].concat())),
            ("index out of range", glyph(&[push(&[-100]), vec![29]].concat())),
            ("fixed-point subr number", glyph(&[vec![255, 0xff, 0x97, 0x80, 0x00], vec![29]].concat())),
            ("empty stack", glyph(&[29])),
            ("reserved operator", glyph(&[0])),
            ("truncated hint mask", [push(&[0, 0]), vec![18, 19]].concat()),
        ];
        for (case, glyph) in cases {
            let input = name_keyed(&[glyph], &global, &[]);
            assert_eq!(prune_subroutines(&input), None, "{case}");
        }
    }

    #[test]
    fn runaway_recursion_is_refused() {
        // Subr 0 calls itself: nesting passes the Type 2 limit.
        let global = vec![[push(&[-107]), vec![29, RETURN]].concat(), subr(10)];
        let input = name_keyed(&[glyph(&[push(&[-107]), vec![29]].concat())], &global, &[]);
        assert_eq!(prune_subroutines(&input), None);
    }

    #[test]
    fn fan_out_bombs_stop_at_the_step_budget() {
        // Each level calls the next 200 times, 8 levels deep: 200^8 calls if run to the end.
        let call = |n: i32| [push(&[n]), vec![29]].concat();
        let mut global: Vec<Vec<u8>> =
            (0..8).map(|level| [call(level - 106).repeat(200), vec![RETURN]].concat()).collect();
        global.push(subr(3));
        let input = name_keyed(&[glyph(&call(-107))], &global, &[]);
        let started = std::time::Instant::now();
        assert_eq!(prune_subroutines(&input), None);
        assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
    }

    #[test]
    fn the_self_check_rejects_a_font_whose_reachable_subroutines_changed() {
        let charstrings = vec![glyph(&[push(&[-107]), vec![29]].concat())];
        let input = name_keyed(&charstrings, &[subr(10), subr(10)], &[subr(4)]);
        let font = Font::parse(&input).expect("parses");
        let reach = reachable(&font).expect("scans");
        let local = vec![vec![&[RETURN][..]]];

        let correct = write(&font, &[font.global[0], &[RETURN][..]], &local).expect("writes");
        let broken = write(&font, &[&[RETURN][..], font.global[1]], &local).expect("writes");

        assert!(verify(&font, &reach, &correct));
        assert!(!verify(&font, &reach, &broken), "subr 0 is called but was emptied");
    }

    #[test]
    fn nothing_to_prune_keeps_the_font() {
        let input = name_keyed(&[glyph(&[push(&[-107]), vec![29]].concat())], &[subr(10)], &[]);
        assert_eq!(prune_subroutines(&input), None);
    }

    #[test]
    fn unusual_layouts_are_refused() {
        let font = name_keyed(&[glyph(&[])], &[subr(10)], &[]);
        let mut version_two = font.clone();
        version_two[0] = 2;
        assert_eq!(prune_subroutines(&version_two), None, "CFF2");
        let mut header_past_the_end = font.clone();
        header_past_the_end[2] = 200;
        assert_eq!(prune_subroutines(&header_past_the_end), None, "header size beyond the data");
    }

    #[test]
    fn corrupted_fonts_never_panic() {
        let charstrings = vec![glyph(&[]), glyph(&[push(&[-105]), vec![29], push(&[-107]), vec![10]].concat())];
        let global: Vec<Vec<u8>> = (0..4).map(|i| subr(10 + i)).collect();
        let local: Vec<Vec<u8>> = (0..3).map(|i| subr(5 + i)).collect();
        let valid = name_keyed(&charstrings, &global, &local);
        for cut in 0..valid.len() {
            let _ = prune_subroutines(&valid[..cut]);
        }
        let mut seed: u32 = 0x9e37_79b9;
        for _ in 0..5000 {
            let mut corrupted = valid.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let position = seed as usize % corrupted.len();
                corrupted[position] = (seed >> 8) as u8;
            }
            if let Some(output) = prune_subroutines(&corrupted) {
                assert!(output.len() < corrupted.len());
            }
        }
    }
}
