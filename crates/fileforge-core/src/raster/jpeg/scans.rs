//! Sequential Huffman-coded scans (T.81 Annex F, 8-bit): decoded to quantized DCT coefficients and encoded again
//! with other Huffman tables. Pixels are never computed, so the coefficients, and the decoded image, stay the same.
//! Decoding is strict: anything a decoder would have to guess about (data running out, misplaced restart markers)
//! fails, and the caller keeps the original file.

// Domain
use super::huffman::{DecodeTable, EncodeTable, Spec};
use super::segments::Frame;
use crate::control::Control;
use crate::raster::error::{RasterError, malformed};

/// Check for cancellation after this many MCUs.
const CANCEL_INTERVAL: usize = 4096;

/// Block geometry of a frame (T.81 A.2).
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    mcus_wide: usize,
    mcus_high: usize,
    components: Vec<PlaneLayout>,
}

#[derive(Debug, Clone, Copy)]
struct PlaneLayout {
    id: u8,
    h: usize,
    v: usize,
    /// Blocks per row and rows as stored: whole MCUs, as interleaved scans code them.
    padded_wide: usize,
    padded_high: usize,
    /// Blocks covering the component's samples, as non-interleaved scans code them.
    wide: usize,
    high: usize,
}

impl Layout {
    pub(crate) fn new(frame: &Frame) -> Result<Self, RasterError> {
        if frame.height == 0 {
            return Err(malformed("height defined by a DNL marker"));
        }
        let max_h = frame.components.iter().map(|c| usize::from(c.h)).max().unwrap_or(1);
        let max_v = frame.components.iter().map(|c| usize::from(c.v)).max().unwrap_or(1);
        let (width, height) = (usize::from(frame.width), usize::from(frame.height));
        let mcus_wide = width.div_ceil(8 * max_h);
        let mcus_high = height.div_ceil(8 * max_v);
        let components = frame
            .components
            .iter()
            .map(|c| {
                let (h, v) = (usize::from(c.h), usize::from(c.v));
                PlaneLayout {
                    id: c.id,
                    h,
                    v,
                    padded_wide: mcus_wide * h,
                    padded_high: mcus_high * v,
                    wide: (width * h).div_ceil(max_h).div_ceil(8),
                    high: (height * v).div_ceil(max_v).div_ceil(8),
                }
            })
            .collect();
        Ok(Self { mcus_wide, mcus_high, components })
    }

    /// Bytes the coefficients of every component take.
    pub(crate) fn coefficient_bytes(&self) -> u64 {
        self.components.iter().map(|c| (c.padded_wide * c.padded_high) as u64 * 128).sum()
    }
}

/// Quantized coefficients of every component, in zigzag order, block rows of whole MCUs.
pub(crate) struct Coefficients {
    planes: Vec<Vec<[i16; 64]>>,
}

impl Coefficients {
    pub(crate) fn new(layout: &Layout) -> Self {
        Self { planes: layout.components.iter().map(|c| vec![[0; 64]; c.padded_wide * c.padded_high]).collect() }
    }
}

/// A parsed SOS header of a sequential scan.
#[derive(Debug, Clone)]
pub(crate) struct Scan {
    /// Frame component index and DC/AC table ids of each component in the scan.
    components: Vec<(usize, usize, usize)>,
    pub restart_interval: usize,
}

impl Scan {
    pub(crate) fn parse(header: &[u8], layout: &Layout, restart_interval: usize) -> Result<Self, RasterError> {
        let [count, rest @ ..] = header else { return Err(malformed("empty scan header")) };
        let count = usize::from(*count);
        if count == 0 || count > 4 || rest.len() != count * 2 + 3 {
            return Err(malformed("bad scan header"));
        }
        if rest[count * 2..] != [0, 63, 0] {
            return Err(malformed("not a sequential scan"));
        }
        let mut components = Vec::with_capacity(count);
        for pair in rest[..count * 2].chunks_exact(2) {
            let index = layout
                .components
                .iter()
                .position(|c| c.id == pair[0])
                .ok_or_else(|| malformed("scan names an unknown component"))?;
            // T.81 B.2.3: components appear in frame order, each once.
            if components.last().is_some_and(|&(previous, _, _)| previous >= index) {
                return Err(malformed("scan components out of order"));
            }
            let (dc, ac) = (usize::from(pair[1] >> 4), usize::from(pair[1] & 0x0F));
            if dc > 3 || ac > 3 {
                return Err(malformed("bad Huffman table id"));
            }
            components.push((index, dc, ac));
        }
        if count > 1 {
            let blocks: usize =
                components.iter().map(|&(i, _, _)| layout.components[i].h * layout.components[i].v).sum();
            if blocks > 10 {
                return Err(malformed("more than 10 blocks per MCU"));
            }
        }
        Ok(Self { components, restart_interval })
    }

    /// Frame component indices this scan codes.
    pub(crate) fn component_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.components.iter().map(|&(index, _, _)| index)
    }

    fn mcus(&self, layout: &Layout) -> usize {
        match self.components.as_slice() {
            [(index, _, _)] => layout.components[*index].wide * layout.components[*index].high,
            _ => layout.mcus_wide * layout.mcus_high,
        }
    }

    /// `(position in scan, block index)` of every block of MCU `mcu`, in coding order.
    fn mcu_blocks(&self, layout: &Layout, mcu: usize, blocks: &mut Vec<(usize, usize)>) {
        blocks.clear();
        if let [(index, _, _)] = self.components.as_slice() {
            let plane = &layout.components[*index];
            blocks.push((0, (mcu / plane.wide) * plane.padded_wide + mcu % plane.wide));
            return;
        }
        let (row, column) = (mcu / layout.mcus_wide, mcu % layout.mcus_wide);
        for (position, &(index, _, _)) in self.components.iter().enumerate() {
            let plane = &layout.components[index];
            for v in 0..plane.v {
                for h in 0..plane.h {
                    blocks.push((position, (row * plane.v + v) * plane.padded_wide + column * plane.h + h));
                }
            }
        }
    }

    /// Restart marker number expected before MCU `mcu`, if one is due there.
    fn restart_before(&self, mcu: usize) -> Option<u8> {
        (self.restart_interval > 0 && mcu > 0 && mcu.is_multiple_of(self.restart_interval))
            .then(|| ((mcu / self.restart_interval - 1) % 8) as u8)
    }
}

/// Huffman tables currently defined, as DHT segments set them.
#[derive(Default)]
pub(crate) struct Tables {
    decoders: [[Option<DecodeTable>; 4]; 2],
}

impl Tables {
    pub(crate) fn define(&mut self, payload: &[u8]) -> Result<(), RasterError> {
        for (class, id, spec) in Spec::parse_segment(payload)? {
            self.decoders[class][id] = Some(spec.decoder()?);
        }
        Ok(())
    }
}

/// Decodes one scan's data into `coefficients`.
pub(crate) fn decode(
    scan: &Scan,
    data: &[u8],
    tables: &Tables,
    layout: &Layout,
    coefficients: &mut Coefficients,
    control: &dyn Control,
) -> Result<(), RasterError> {
    let mut decoders = Vec::with_capacity(scan.components.len());
    for &(_, dc, ac) in &scan.components {
        let dc = tables.decoders[0][dc].as_ref().ok_or_else(|| malformed("undefined DC table"))?;
        let ac = tables.decoders[1][ac].as_ref().ok_or_else(|| malformed("undefined AC table"))?;
        decoders.push((dc, ac));
    }
    let mut reader = BitReader::new(data);
    let mut predictors = [0i32; 4];
    let mut blocks = Vec::with_capacity(10);
    for mcu in 0..scan.mcus(layout) {
        if mcu % CANCEL_INTERVAL == 0 && control.is_cancelled() {
            return Err(RasterError::Cancelled);
        }
        if let Some(number) = scan.restart_before(mcu) {
            reader.restart(number)?;
            predictors = [0; 4];
        }
        scan.mcu_blocks(layout, mcu, &mut blocks);
        for &(position, block) in &blocks {
            let (dc, ac) = decoders[position];
            let plane = scan.components[position].0;
            decode_block(&mut reader, dc, ac, &mut predictors[position], &mut coefficients.planes[plane][block])?;
        }
    }
    reader.finish()
}

fn decode_block(
    reader: &mut BitReader,
    dc: &DecodeTable,
    ac: &DecodeTable,
    predictor: &mut i32,
    block: &mut [i16; 64],
) -> Result<(), RasterError> {
    let size = reader.symbol(dc)?;
    if size > 11 {
        return Err(malformed("DC difference too large"));
    }
    *predictor += extend(reader.bits(u32::from(size))?, size);
    block[0] = i16::try_from(*predictor).map_err(|_| malformed("DC coefficient out of range"))?;
    let mut k = 1;
    while k < 64 {
        let symbol = reader.symbol(ac)?;
        let (run, size) = (usize::from(symbol >> 4), symbol & 0x0F);
        match (run, size) {
            (0, 0) => break,
            (15, 0) => k += 16,
            (_, 0) => return Err(malformed("end-of-band run in a sequential scan")),
            (_, 1..=10) => {
                k += run;
                if k > 63 {
                    return Err(malformed("coefficient index past 63"));
                }
                // Size ≤ 10, so the value fits in 11 bits.
                block[k] = extend(reader.bits(u32::from(size))?, size) as i16;
                k += 1;
            }
            _ => return Err(malformed("AC coefficient too large")),
        }
    }
    if k > 64 {
        return Err(malformed("zero run past the end of a block"));
    }
    Ok(())
}

/// T.81 F.2.2.1 `EXTEND`: the signed value of `size` received bits.
fn extend(bits: u32, size: u8) -> i32 {
    if size == 0 {
        return 0;
    }
    let value = bits as i32;
    if value < 1 << (size - 1) { value - (1 << size) + 1 } else { value }
}

/// A scan coded again: the DHT payload to put before it and its entropy-coded data.
pub(crate) struct EncodedScan {
    pub dht: Vec<u8>,
    pub data: Vec<u8>,
}

/// Encodes every scan again with optimal Huffman tables.
pub(crate) fn encode(
    scans: &[Scan],
    layout: &Layout,
    coefficients: &Coefficients,
    control: &dyn Control,
) -> Result<Vec<EncodedScan>, RasterError> {
    scans
        .iter()
        .map(|scan| {
            let mut counter = Counter { counts: Box::new([[[0; 256]; 4]; 2]) };
            walk(scan, layout, coefficients, &mut counter, control)?;
            let mut dht = Vec::new();
            let mut encoders: [[Option<EncodeTable>; 4]; 2] = Default::default();
            for (class, encoders) in encoders.iter_mut().enumerate() {
                for (id, encoder) in encoders.iter_mut().enumerate() {
                    if counter.counts[class][id].iter().any(|&count| count > 0) {
                        let spec = Spec::optimal(&counter.counts[class][id])?;
                        spec.write(class, id, &mut dht);
                        *encoder = Some(spec.encoder());
                    }
                }
            }
            let mut writer = BitWriter { out: Vec::new(), acc: 0, count: 0, encoders };
            walk(scan, layout, coefficients, &mut writer, control)?;
            writer.flush();
            Ok(EncodedScan { dht, data: writer.out })
        })
        .collect()
}

/// Receives the symbols of a scan in coding order.
trait Sink {
    fn symbol(&mut self, class: usize, table: usize, symbol: u8, bits: u32, size: u8) -> Result<(), RasterError>;
    fn restart(&mut self, number: u8);
}

fn walk(
    scan: &Scan,
    layout: &Layout,
    coefficients: &Coefficients,
    sink: &mut dyn Sink,
    control: &dyn Control,
) -> Result<(), RasterError> {
    let mut predictors = [0i32; 4];
    let mut blocks = Vec::with_capacity(10);
    for mcu in 0..scan.mcus(layout) {
        if mcu % CANCEL_INTERVAL == 0 && control.is_cancelled() {
            return Err(RasterError::Cancelled);
        }
        if let Some(number) = scan.restart_before(mcu) {
            sink.restart(number);
            predictors = [0; 4];
        }
        scan.mcu_blocks(layout, mcu, &mut blocks);
        for &(position, block) in &blocks {
            let (plane, dc, ac) = scan.components[position];
            let block = &coefficients.planes[plane][block];
            let diff = i32::from(block[0]) - predictors[position];
            predictors[position] = i32::from(block[0]);
            let (size, bits) = magnitude(diff);
            sink.symbol(0, dc, size, bits, size)?;
            let mut run = 0u8;
            for &value in &block[1..] {
                if value == 0 {
                    run += 1;
                    continue;
                }
                while run >= 16 {
                    sink.symbol(1, ac, 0xF0, 0, 0)?;
                    run -= 16;
                }
                let (size, bits) = magnitude(i32::from(value));
                sink.symbol(1, ac, (run << 4) | size, bits, size)?;
                run = 0;
            }
            if run > 0 {
                sink.symbol(1, ac, 0x00, 0, 0)?;
            }
        }
    }
    Ok(())
}

/// Bit length of `|value|` and the bits coding `value` (T.81 F.1.2.1).
fn magnitude(value: i32) -> (u8, u32) {
    let size = (32 - value.unsigned_abs().leading_zeros()) as u8;
    let bits = if value < 0 { (value - 1) as u32 & ((1 << size) - 1) } else { value as u32 };
    (size, bits)
}

struct Counter {
    counts: Box<[[[u64; 256]; 4]; 2]>,
}

impl Sink for Counter {
    fn symbol(&mut self, class: usize, table: usize, symbol: u8, _: u32, _: u8) -> Result<(), RasterError> {
        self.counts[class][table][usize::from(symbol)] += 1;
        Ok(())
    }

    fn restart(&mut self, _: u8) {}
}

/// Reads entropy-coded bits, removing stuffed zero bytes. At a marker or the end of the data it supplies zero bits
/// and counts them, so running out of data is detected instead of guessed around.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    count: u32,
    /// Zero bits appended after the data stopped; they sit at the end of `acc`.
    padding: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, acc: 0, count: 0, padding: 0 }
    }

    fn fill(&mut self) {
        while self.count <= 56 {
            let byte = match self.data.get(self.pos) {
                Some(0xFF) if self.data.get(self.pos + 1) == Some(&0x00) => {
                    self.pos += 2;
                    0xFF
                }
                Some(0xFF) | None => {
                    self.padding += 8;
                    0
                }
                Some(&byte) => {
                    self.pos += 1;
                    byte
                }
            };
            self.acc |= u64::from(byte) << (56 - self.count);
            self.count += 8;
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32, RasterError> {
        if n == 0 {
            return Ok(0);
        }
        if self.count < n {
            self.fill();
        }
        let value = (self.acc >> (64 - n)) as u32;
        self.acc <<= n;
        self.count -= n;
        self.check()?;
        Ok(value)
    }

    fn symbol(&mut self, table: &DecodeTable) -> Result<u8, RasterError> {
        if self.count < 16 {
            self.fill();
        }
        let (symbol, length) =
            table.decode((self.acc >> 48) as u32).ok_or_else(|| malformed("invalid Huffman code"))?;
        self.acc <<= length;
        self.count -= length;
        self.check()?;
        Ok(symbol)
    }

    /// Fails once a padding bit was consumed.
    fn check(&self) -> Result<(), RasterError> {
        if self.padding > self.count { Err(malformed("scan data ended early")) } else { Ok(()) }
    }

    /// Bits left that came from the data; only the fill bits of the current byte may remain.
    fn data_bits_left(&self) -> u32 {
        self.count - self.padding.min(self.count)
    }

    fn restart(&mut self, number: u8) -> Result<(), RasterError> {
        if self.data_bits_left() >= 8 {
            return Err(malformed("data before a restart marker"));
        }
        let mut pos = self.pos;
        while self.data.get(pos) == Some(&0xFF) && self.data.get(pos + 1) == Some(&0xFF) {
            pos += 1;
        }
        if self.data.get(pos..pos + 2) != Some(&[0xFF, 0xD0 + number]) {
            return Err(malformed("missing or misnumbered restart marker"));
        }
        *self = Self { pos: pos + 2, ..Self::new(self.data) };
        Ok(())
    }

    fn finish(&self) -> Result<(), RasterError> {
        if self.data_bits_left() >= 8 {
            return Err(malformed("extra data after the last block"));
        }
        if self.data[self.pos.min(self.data.len())..].iter().any(|&byte| byte != 0xFF) {
            return Err(malformed("extra data after the last block"));
        }
        Ok(())
    }
}

/// Writes Huffman codes and value bits with byte stuffing; pads with one bits before restart markers and at the end.
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    count: u32,
    encoders: [[Option<EncodeTable>; 4]; 2],
}

impl BitWriter {
    fn put(&mut self, bits: u32, size: u32) {
        if size == 0 {
            return;
        }
        self.acc = (self.acc << size) | u64::from(bits & ((1 << size) - 1));
        self.count += size;
        while self.count >= 8 {
            self.count -= 8;
            let byte = (self.acc >> self.count) as u8;
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0x00);
            }
        }
        self.acc &= (1 << self.count) - 1;
    }

    fn flush(&mut self) {
        let pad = (8 - self.count % 8) % 8;
        self.put((1 << pad) - 1, pad);
    }
}

impl Sink for BitWriter {
    fn symbol(&mut self, class: usize, table: usize, symbol: u8, bits: u32, size: u8) -> Result<(), RasterError> {
        let (code, length) = self.encoders[class][table]
            .as_ref()
            .map(|encoder| encoder.code(symbol))
            .filter(|&(_, length)| length > 0)
            .ok_or_else(|| RasterError::Internal("symbol without a Huffman code".into()))?;
        self.put(u32::from(code), u32::from(length));
        self.put(bits, u32::from(size));
        Ok(())
    }

    fn restart(&mut self, number: u8) {
        self.flush();
        self.out.extend_from_slice(&[0xFF, 0xD0 + number]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_and_extend_are_inverse() {
        for value in -2047..=2047 {
            let (size, bits) = magnitude(value);
            assert_eq!(extend(bits, size), value, "{value}");
        }
        assert_eq!(magnitude(0), (0, 0));
        assert_eq!(magnitude(-1), (1, 0));
        assert_eq!(magnitude(5), (3, 5));
    }

    #[test]
    fn written_bits_read_back_with_stuffing_and_restart_markers() {
        let mut writer = BitWriter { out: Vec::new(), acc: 0, count: 0, encoders: Default::default() };
        writer.put(0xFF, 8);
        writer.put(0b101, 3);
        writer.restart(0);
        writer.put(0x3FF, 10);
        writer.flush();
        assert_eq!(writer.out, [0xFF, 0x00, 0b1011_1111, 0xFF, 0xD0, 0xFF, 0x00, 0b1111_1111, 0x00]);
        let mut reader = BitReader::new(&writer.out);
        assert_eq!(reader.bits(8).ok(), Some(0xFF));
        assert_eq!(reader.bits(3).ok(), Some(0b101));
        assert!(reader.restart(0).is_ok());
        assert_eq!(reader.bits(10).ok(), Some(0x3FF));
        assert!(reader.finish().is_ok());
    }

    #[test]
    fn running_out_of_data_and_wrong_restart_markers_are_errors() {
        let mut reader = BitReader::new(&[0xAB]);
        assert_eq!(reader.bits(8).ok(), Some(0xAB));
        assert!(reader.bits(1).is_err());
        let mut reader = BitReader::new(&[0x80, 0xFF, 0xD3]);
        assert!(reader.bits(1).is_ok());
        assert!(reader.restart(0).is_err());
        let reader = BitReader::new(&[0x80, 0x12]);
        assert!(reader.finish().is_err());
    }
}
