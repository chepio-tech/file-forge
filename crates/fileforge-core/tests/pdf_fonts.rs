//! Font programs: CFF subroutines no glyph calls are removed in every preset, glyph programs stay identical, and
//! anything unusual stays as it was (ADR-0018).

mod support;

// Core
use fileforge_core::pdf::{ImageOptions, PdfOptions, compress};
use lopdf::{Document, Object, Stream, dictionary};
// Utils
use support::PdfBuilder;

/// A change to a font program stream.
type Edit = fn(&mut Stream);

const MAXIMUM: PdfOptions = PdfOptions {
    images: Some(ImageOptions { jpeg_quality: 70, max_dpi: Some(150), compress_flate_photos: true }),
    ..PdfOptions::LOSSLESS
};

fn index(items: &[Vec<u8>]) -> Vec<u8> {
    if items.is_empty() {
        return vec![0, 0];
    }
    let mut out = (items.len() as u16).to_be_bytes().to_vec();
    out.push(4);
    let mut offset = 1u32;
    out.extend_from_slice(&offset.to_be_bytes());
    for item in items {
        offset += item.len() as u32;
        out.extend_from_slice(&offset.to_be_bytes());
    }
    items.iter().for_each(|item| out.extend_from_slice(item));
    out
}

fn int(out: &mut Vec<u8>, value: usize) {
    out.push(29);
    out.extend_from_slice(&(value as i32).to_be_bytes());
}

/// `count` subroutines of zero-length line segments (`0 0 rlineto` ×20, `return`).
fn subrs(count: usize) -> Vec<Vec<u8>> {
    (0..count).map(|i| [[139, 139, 5].repeat(20 + i % 3), vec![11]].concat()).collect()
}

/// A name-keyed CFF font: `.notdef` and one glyph calling global subr 0 and local subr 1; 40 global and 30 local
/// subrs, so most are unreachable, as in Quartz-made subsets.
struct Cff {
    bytes: Vec<u8>,
    charstrings: Vec<u8>,
    reachable: [Vec<u8>; 2],
}

fn cff(glyph: &[u8]) -> Cff {
    let charstrings = index(&[vec![14], glyph.to_vec()]);
    let (global, local) = (subrs(40), subrs(30));
    let reachable = [global[0].clone(), local[1].clone()];
    let (name, strings, global, local) = (index(&[b"Test".to_vec()]), index(&[]), index(&global), index(&local));
    let top_len = 6 + 11;
    let charstrings_at = 4 + name.len() + (3 + 8 + top_len) + strings.len() + global.len();
    let private_at = charstrings_at + charstrings.len();
    let mut top = Vec::new();
    int(&mut top, charstrings_at);
    top.push(17);
    int(&mut top, 6);
    int(&mut top, private_at);
    top.push(18);
    let mut bytes = vec![1, 0, 4, 4];
    for part in [&name, &index(&[top]), &strings, &global, &charstrings] {
        bytes.extend_from_slice(part);
    }
    int(&mut bytes, 6);
    bytes.push(19);
    bytes.extend_from_slice(&local);
    Cff { bytes, charstrings, reachable }
}

/// Glyph 1: callgsubr 0 (operand -107), callsubr 1 (operand -106), endchar.
const GLYPH: [u8; 5] = [32, 29, 33, 10, 14];

/// A page showing text in a Type 1 font whose `FontFile3` holds `program` (compressed), with `subtype`.
fn pdf_with_font(program: Vec<u8>, subtype: &str, edit: Edit) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let mut stream = Stream::new(dictionary! { "Subtype" => subtype }, program);
    stream.compress().expect("compresses");
    edit(&mut stream);
    let file = pdf.doc.add_object(stream);
    let descriptor = pdf.doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AAAAAB+Test",
        "Flags" => 4,
        "FontBBox" => vec![0.into(), 0.into(), 500.into(), 700.into()],
        "ItalicAngle" => 0,
        "Ascent" => 700,
        "Descent" => 0,
        "CapHeight" => 700,
        "StemV" => 80,
        "FontFile3" => file,
    });
    let font = pdf.doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AAAAAB+Test",
        "FontDescriptor" => descriptor,
    });
    let page = pdf.page("BT /F1 24 Tf 72 700 Td (A) Tj ET", &[]);
    pdf.dict_mut(page)
        .get_mut(b"Resources")
        .and_then(Object::as_dict_mut)
        .expect("page resources")
        .set("Font", dictionary! { "F1" => font });
    pdf.bytes()
}

fn font_program(bytes: &[u8]) -> Vec<u8> {
    let doc = Document::load_mem(bytes).expect("output loads");
    let streams: Vec<Vec<u8>> = doc
        .objects
        .values()
        .filter_map(|object| match object {
            Object::Stream(stream) if stream.dict.has(b"Subtype") && !stream.dict.has(b"Type") => {
                Some(stream.decompressed_content().unwrap_or_else(|_| stream.content.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(streams.len(), 1, "one font program");
    streams.into_iter().next().unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn lossless_removes_subroutines_no_glyph_calls_and_keeps_glyph_programs() {
    let font = cff(&GLYPH);
    for (preset, options) in [("lossless", PdfOptions::LOSSLESS), ("maximum", MAXIMUM)] {
        let input = pdf_with_font(font.bytes.clone(), "Type1C", |_| {});

        let output = compress(&input, &options).expect("compresses");

        let program = font_program(&output.bytes);
        assert!(program.len() < font.bytes.len() / 3, "{preset}: {} → {}", font.bytes.len(), program.len());
        assert!(contains(&program, &font.charstrings), "{preset}: charstrings copied verbatim");
        for subr in &font.reachable {
            assert!(contains(&program, subr), "{preset}: reachable subroutine kept");
        }
        assert!(!output.report.kept_original);
    }
}

#[test]
fn a_second_pass_leaves_pruned_fonts_unchanged() {
    let input = pdf_with_font(cff(&GLYPH).bytes, "Type1C", |_| {});
    let once = compress(&input, &PdfOptions::LOSSLESS).expect("first pass");

    let twice = compress(&once.bytes, &PdfOptions::LOSSLESS).expect("second pass");

    assert_eq!(font_program(&twice.bytes), font_program(&once.bytes));
}

#[test]
fn unusual_font_programs_are_left_as_they_are() {
    // Glyph 1 adds two numbers (12 10) before calling: subroutine numbers would depend on arithmetic.
    let arithmetic = cff(&[139, 140, 12, 10, 29, 14]).bytes;
    let plain = cff(&GLYPH).bytes;
    let cases: [(&str, Vec<u8>, &str, Edit); 4] = [
        ("arithmetic operators", arithmetic, "Type1C", |_| {}),
        ("OpenType wrapper (not handled yet)", plain.clone(), "OpenType", |_| {}),
        ("decode parameters", plain.clone(), "Type1C", |stream| {
            stream.dict.set("DecodeParms", dictionary! { "Predictor" => 1 });
        }),
        ("not a CFF font", b"%!PS-AdobeFont-1.0: Test".repeat(40), "Type1C", |_| {}),
    ];
    for (case, program, subtype, edit) in cases {
        let input = pdf_with_font(program.clone(), subtype, edit);

        let output = compress(&input, &PdfOptions::LOSSLESS).expect("compresses");

        assert_eq!(font_program(&output.bytes), program, "{case}");
    }
}
