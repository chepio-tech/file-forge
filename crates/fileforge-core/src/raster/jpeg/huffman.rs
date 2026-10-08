//! Huffman tables: reading DHT definitions (T.81 Annex C, F.2.2.3) and building optimal ones from symbol counts
//! (Annex K.2, as libjpeg's `jpeg_gen_optimal_table`).

// Domain
use crate::raster::error::{RasterError, malformed};

/// A table as DHT stores it: how many codes have each length 1–16, then the symbols in code order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub counts: [u8; 16],
    pub symbols: Vec<u8>,
}

/// Canonical decoding tables per code length (F.2.2.3 `DECODE`).
#[derive(Debug, Clone)]
pub(crate) struct DecodeTable {
    /// Largest code of each length, or -1 when the length has no codes.
    max_code: [i32; 17],
    /// Index into `symbols` of the first code of each length, minus that code.
    offset: [i32; 17],
    symbols: Vec<u8>,
}

/// Code and length for each symbol.
#[derive(Debug, Clone)]
pub(crate) struct EncodeTable {
    codes: [(u16, u8); 256],
}

impl Spec {
    /// Every table in one DHT segment payload: `(class, id, spec)`, class 0 for DC and 1 for AC.
    pub(crate) fn parse_segment(payload: &[u8]) -> Result<Vec<(usize, usize, Self)>, RasterError> {
        let mut tables = Vec::new();
        let mut rest = payload;
        while let [class_id, tail @ ..] = rest {
            let (class, id) = (usize::from(class_id >> 4), usize::from(class_id & 0x0F));
            if class > 1 || id > 3 {
                return Err(malformed("bad Huffman table class or id"));
            }
            let counts: [u8; 16] =
                tail.get(..16).and_then(|c| c.try_into().ok()).ok_or_else(|| malformed("truncated Huffman table"))?;
            let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
            if total > 256 {
                return Err(malformed("more than 256 Huffman codes"));
            }
            let symbols = tail.get(16..16 + total).ok_or_else(|| malformed("truncated Huffman table"))?.to_vec();
            tables.push((class, id, Self { counts, symbols }));
            rest = &tail[16 + total..];
        }
        Ok(tables)
    }

    pub(crate) fn decoder(&self) -> Result<DecodeTable, RasterError> {
        let mut max_code = [-1; 17];
        let mut offset = [0; 17];
        let mut code: i32 = 0;
        let mut index: i32 = 0;
        for length in 1..=16 {
            let count = i32::from(self.counts[length - 1]);
            if count > 0 {
                offset[length] = index - code;
                code += count;
                index += count;
                max_code[length] = code - 1;
            }
            // Codes of this length must fit in `length` bits.
            if code > 1 << length {
                return Err(malformed("Huffman table overflows its code space"));
            }
            code <<= 1;
        }
        Ok(DecodeTable { max_code, offset, symbols: self.symbols.clone() })
    }

    pub(crate) fn encoder(&self) -> EncodeTable {
        let mut codes = [(0, 0); 256];
        let mut code: u16 = 0;
        let mut symbols = self.symbols.iter();
        for (length, &count) in (1u8..).zip(&self.counts) {
            for _ in 0..count {
                if let Some(&symbol) = symbols.next() {
                    codes[usize::from(symbol)] = (code, length);
                }
                code = code.wrapping_add(1);
            }
            code = code.wrapping_shl(1);
        }
        EncodeTable { codes }
    }

    /// DHT bytes for this table: class/id, counts, symbols.
    pub(crate) fn write(&self, class: usize, id: usize, out: &mut Vec<u8>) {
        // Class is 0 or 1 and id 0–3, so the byte cannot overflow.
        out.push(((class << 4) | id) as u8);
        out.extend_from_slice(&self.counts);
        out.extend_from_slice(&self.symbols);
    }

    /// The optimal table for these symbol counts, with no code longer than 16 bits and without the all-ones code
    /// (Annex K.2, libjpeg's tie-breaking). `counts` must include at least one used symbol.
    pub(crate) fn optimal(counts: &[u64; 256]) -> Result<Self, RasterError> {
        const MAX_LENGTH: usize = 64;
        let mut freq = [0u64; 257];
        freq[..256].copy_from_slice(counts);
        // A reserved symbol guarantees no real symbol gets the all-ones code.
        freq[256] = 1;
        let mut code_size = [0usize; 257];
        let mut others = [usize::MAX; 257];
        // The least frequent symbol, preferring the highest value on ties; then the next least frequent.
        while let Some(c1) = smallest(&freq, None) {
            let Some(c2) = smallest(&freq, Some(c1)) else { break };
            freq[c1] += freq[c2];
            freq[c2] = 0;
            for start in [c1, c2] {
                let mut symbol = start;
                code_size[symbol] += 1;
                while others[symbol] != usize::MAX {
                    symbol = others[symbol];
                    code_size[symbol] += 1;
                }
                if start == c1 {
                    others[symbol] = c2;
                }
            }
        }
        let mut lengths = [0u32; MAX_LENGTH + 1];
        for &size in &code_size {
            if size > MAX_LENGTH {
                return Err(RasterError::Internal("Huffman code longer than 64 bits".into()));
            }
            if size > 0 {
                lengths[size] += 1;
            }
        }
        // Shorten codes longer than 16 bits (Annex K.3).
        for length in (17..=MAX_LENGTH).rev() {
            while lengths[length] > 0 {
                let shorter = (1..length - 1)
                    .rev()
                    .find(|&j| lengths[j] > 0)
                    .ok_or_else(|| RasterError::Internal("cannot limit Huffman code lengths".into()))?;
                lengths[length] -= 2;
                lengths[length - 1] += 1;
                lengths[shorter + 1] += 2;
                lengths[shorter] -= 1;
            }
        }
        // Drop the reserved symbol's code: one code of the longest remaining length.
        let longest = (1..=16).rev().find(|&length| lengths[length] > 0).unwrap_or(0);
        if longest > 0 {
            lengths[longest] -= 1;
        }
        let mut spec = Self { counts: [0; 16], symbols: Vec::new() };
        for (slot, &count) in spec.counts.iter_mut().zip(&lengths[1..=16]) {
            *slot = u8::try_from(count).map_err(|_| RasterError::Internal("too many Huffman codes".into()))?;
        }
        for size in 1..=MAX_LENGTH {
            spec.symbols.extend((0..=255u8).filter(|&symbol| code_size[usize::from(symbol)] == size));
        }
        Ok(spec)
    }
}

fn smallest(freq: &[u64; 257], except: Option<usize>) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (symbol, &count) in freq.iter().enumerate() {
        if count > 0 && Some(symbol) != except && best.is_none_or(|b| count <= freq[b]) {
            best = Some(symbol);
        }
    }
    best
}

impl DecodeTable {
    /// Decodes one symbol from the next 16 bits (`peek`, most significant first); returns it and its code length.
    pub(crate) fn decode(&self, peek: u32) -> Option<(u8, u32)> {
        (1..=16).find_map(|length| {
            let code = (peek >> (16 - length)) as i32;
            (code <= self.max_code[length as usize]).then(|| {
                let index = usize::try_from(self.offset[length as usize] + code).ok()?;
                Some((*self.symbols.get(index)?, length))
            })?
        })
    }
}

impl EncodeTable {
    /// Code and its length in bits; length 0 means the symbol has no code.
    pub(crate) fn code(&self, symbol: u8) -> (u16, u8) {
        self.codes[usize::from(symbol)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(spec: &Spec) {
        let decoder = spec.decoder();
        assert!(decoder.is_ok(), "{spec:?}");
        let Ok(decoder) = decoder else { return };
        let encoder = spec.encoder();
        for &symbol in &spec.symbols {
            let (code, length) = encoder.code(symbol);
            assert!(length > 0 && length <= 16);
            let peek = u32::from(code) << (16 - length);
            assert_eq!(decoder.decode(peek), Some((symbol, u32::from(length))));
            assert_ne!(u32::from(code), (1 << length) - 1, "all-ones code is reserved");
        }
    }

    #[test]
    fn optimal_tables_decode_every_symbol_and_favor_frequent_ones() {
        let mut counts = [0u64; 256];
        counts[0x00] = 1000;
        counts[0x01] = 500;
        counts[0x11] = 20;
        counts[0xF0] = 1;
        let spec = Spec::optimal(&counts);
        assert!(spec.is_ok());
        let Ok(spec) = spec else { return };
        assert_eq!(spec.symbols[0], 0x00, "most frequent symbol first");
        round_trip(&spec);
        let lengths: Vec<u8> = [0x00, 0x01, 0x11, 0xF0].iter().map(|&s| spec.encoder().code(s).1).collect();
        assert!(lengths.windows(2).all(|pair| pair[0] <= pair[1]), "{lengths:?}");
    }

    #[test]
    fn a_single_symbol_still_gets_a_code() {
        let mut counts = [0u64; 256];
        counts[0x00] = 7;
        let spec = Spec::optimal(&counts);
        assert_eq!(spec.as_ref().map(|s| (s.counts[0], s.symbols.clone())), Ok((1, vec![0x00])));
        if let Ok(spec) = spec {
            round_trip(&spec);
        }
    }

    #[test]
    fn skewed_counts_are_limited_to_sixteen_bits() {
        // Fibonacci counts would need codes far longer than 16 bits.
        let mut counts = [0u64; 256];
        let (mut a, mut b) = (1u64, 1u64);
        for count in counts.iter_mut().take(40) {
            *count = a;
            (a, b) = (b, a + b);
        }
        let spec = Spec::optimal(&counts);
        assert!(spec.is_ok());
        let Ok(spec) = spec else { return };
        assert_eq!(spec.symbols.len(), 40);
        round_trip(&spec);
    }

    #[test]
    fn overfull_and_truncated_tables_are_refused() {
        let overfull = Spec { counts: [3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], symbols: vec![1, 2, 3] };
        assert!(overfull.decoder().is_err());
        assert!(Spec::parse_segment(&[0x00, 1]).is_err());
        assert!(Spec::parse_segment(&[0x20; 17]).is_err(), "class 2");
        let mut one = vec![0x13, 1];
        one.extend([0; 15]);
        one.push(0x42);
        let parsed = Spec::parse_segment(&one);
        assert_eq!(
            parsed.map(|t| t.into_iter().map(|(c, i, s)| (c, i, s.symbols)).collect::<Vec<_>>()),
            Ok(vec![(1, 3, vec![0x42])])
        );
    }
}
