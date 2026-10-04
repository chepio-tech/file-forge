# ADR-0018: Unreachable CFF subroutines are removed in every preset

## Status
Accepted

## Date
2026-10-04

## Context
ADR-0002 named font subsetting the biggest gap against Ghostscript. A survey of 420 non-private PDFs (76.1 MB) found
no fully embedded fonts: every one of 1,827 font programs was already a subset. CFF subsets, however, often keep the
complete subroutine sets of the original font. Subroutines are shared outline fragments that charstrings call by
number. No glyph in the subset calls most of them. Unreachable subroutines made up about 4.0 MB, 27% of all font
data and 5.3% of the corpus. In some files they were almost all of it: a 344 KB macOS icon PDF carries a two-glyph
SF Pro subset with 391 KB of subroutines. macOS 26.6.2 Quartz still writes CFF subsets this way (a 35-glyph STIX
subset: 1.5 KB of charstrings, 23.8 KB of subroutines).

General subsetters do not fit: `subsetter` (typst) removes all subroutines and turns name-keyed CFF into CID-keyed.
That breaks simple fonts, whose glyphs are found by name through `Encoding`/`Differences`.

## Decision
- In every preset, including Lossless, `FontFile3` programs with `Subtype` `Type1C` or `CIDFontType0C` (bare CFF
  version 1, one font) are checked for unreachable subroutines.
- A Type 2 scan follows every charstring with the operand stack shared across calls, counts stem hints for
  `hintmask`/`cntrmask` and resolves biased `callsubr`/`callgsubr`. The font is left as it is when the scan meets an
  arithmetic or storage operator, a non-integer subroutine number, an invalid index, nesting deeper than 10 or
  truncated data.
- Each unreachable subroutine becomes a single `return` byte. Subroutine counts stay the same, so the bias does not
  change and no charstring is touched. The font is written again with 5-byte offsets.
- Before the result is used, it is parsed again. Charstrings, charset, encoding and FDSelect must be byte-identical,
  the subroutine counts unchanged, and every reachable subroutine byte-identical to the original. The new program is
  kept only if it is smaller. Otherwise the original stream stays byte for byte.
- **Lossless, for fonts, means:** every glyph executes the same instructions as before, so outlines and metrics are
  identical; the font file's bytes may differ.

## Rationale
The transform never changes what a renderer executes for any glyph. The self-check proves this for every font it
rewrites, so a scanner mistake leads to "kept as is" rather than a missing glyph. No glyph-usage analysis of page
content is needed, which avoids the hardest and riskiest part of real subsetting.

## Alternatives considered
- **Subsetting with `subsetter`:** changes how glyphs are addressed (cmap removed, name-keyed → CID-keyed); every
  font would need its PDF dictionaries and possibly content streams rewritten.
- **Desubroutinizing** (inlining every call): grows fonts with many glyphs, and rewrites every charstring.
- **Truncating trailing unused subroutines:** changing the count can change the bias, which renumbers every call.
- **Only in lossy presets:** Lossless users would keep removable data although glyphs stay identical.

## Consequences
- macOS-made PDFs with CFF fonts shrink in Lossless; other PDFs are unaffected.
- The scan costs one pass over each CFF font's charstrings.
- Fully embedded fonts, OpenType-wrapped CFF, Type 1 programs and several subsets of one font are not reduced yet.

## Validation / fitness criteria
- `pdf::cff` unit tests: name-keyed and CID-keyed fonts built in code are pruned with identical charstrings; fonts
  with arithmetic operators, bad indices or truncation are refused; corrupted fonts never panic.
- `tests/pdf_fonts.rs`: pruning happens in Lossless, the output is never larger, unusual fonts stay byte-identical.
- Measured on 277 distinct non-private PDFs (`docs/architecture/runtime.md`): every page rendered by Quartz
  (`CGPDFDocument`, 2×) before and after is pixel-identical; disabling the reachability check makes the same
  comparison fail.

## Reconsider when
- A real-world corpus shows fully embedded fonts or many subset copies of one font: plan glyph-usage analysis.
- A renderer is found that rejects one-byte `return` subroutines.
- OpenType-wrapped CFF or Type 1 programs turn out to be common.
