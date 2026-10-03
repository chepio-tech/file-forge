//! Optional removal of metadata, thumbnails and editing data (ADR-0012): only on request, never what a page shows,
//! never what a declared standard or a screen reader needs.

mod support;

// Core
use fileforge_core::pdf::{PdfOptions, compress};
use lopdf::{Document, Object, dictionary};
// Utils
use support::{PdfBuilder, jpeg};

const STRIP_METADATA: PdfOptions = PdfOptions { strip_metadata: true, ..PdfOptions::LOSSLESS };
const STRIP_EDITING_DATA: PdfOptions = PdfOptions { strip_editing_data: true, ..PdfOptions::LOSSLESS };
const STRIP_ALL: PdfOptions = PdfOptions { strip_metadata: true, strip_editing_data: true, ..PdfOptions::LOSSLESS };

const PLAIN_XMP: &str = r#"xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:CreatorTool="Adobe Illustrator 29.0""#;
/// Differs from the catalog packet so the two are not merged as duplicates.
const IMAGE_XMP: &str = r#"xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:CreatorTool="Adobe Photoshop 26.0""#;
/// Size of the private data each `/PieceInfo` carries in the fixture.
const PRIVATE_BYTES: u32 = 90_000;

/// A page as design tools export it: Info, catalog XMP, an image with its own XMP, a page thumbnail, and
/// Illustrator private data on the page and on a form XObject.
fn exported(catalog_xmp: Option<&str>, configure: impl FnOnce(&mut PdfBuilder)) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let photo = pdf.image(320, 240, "DeviceRGB", Some("DCTDecode"), jpeg(320, 240, 90, false));
    pdf.object_xmp(photo, IMAGE_XMP);
    let form = pdf.form([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "q 160 0 0 120 0 0 cm /P Do Q", ("P", photo));
    pdf.piece_info(form, PRIVATE_BYTES);
    let page = pdf.page("BT /F1 12 Tf 72 720 Td (Poster) Tj ET q 1 0 0 1 72 400 cm /Fm Do Q", &[("Fm", form)]);
    pdf.thumbnail(page);
    pdf.piece_info(page, PRIVATE_BYTES);
    pdf.info(dictionary! {
        "Author" => Object::string_literal("Jane Designer"),
        "Creator" => Object::string_literal("Adobe Illustrator 29.0"),
    });
    if let Some(properties) = catalog_xmp {
        pdf.xmp(properties);
    }
    configure(&mut pdf);
    pdf.bytes()
}

fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("output must load")
}

fn dicts(doc: &Document) -> impl Iterator<Item = &lopdf::Dictionary> {
    doc.objects.values().filter_map(|object| match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    })
}

fn count_key(doc: &Document, key: &[u8]) -> usize {
    dicts(doc).filter(|dict| dict.has(key)).count()
}

fn metadata_streams(doc: &Document) -> Vec<Vec<u8>> {
    doc.objects
        .values()
        .filter_map(|object| match object {
            Object::Stream(stream)
                if stream.dict.get(b"Type").and_then(Object::as_name).is_ok_and(|name| name == b"Metadata") =>
            {
                Some(stream.content.clone())
            }
            _ => None,
        })
        .collect()
}

fn catalog_xmp(doc: &Document) -> Option<Vec<u8>> {
    let reference = doc.catalog().ok()?.get(b"Metadata").ok()?.as_reference().ok()?;
    let Object::Stream(stream) = doc.get_object(reference).ok()? else { return None };
    stream.get_plain_content().ok()
}

fn page_contents(doc: &Document) -> Vec<Vec<u8>> {
    doc.get_pages().into_values().map(|page| doc.get_page_content(page)).collect()
}

fn has_private_data(doc: &Document) -> bool {
    dicts(doc).any(|dict| dict.has(b"AIPrivateData1"))
}

#[test]
fn nothing_is_removed_unless_asked() {
    let input = exported(Some(PLAIN_XMP), |_| {});

    let output = compress(&input, &PdfOptions::LOSSLESS).expect("compresses");

    let result = load(&output.bytes);
    assert!(result.trailer.has(b"Info"));
    assert_eq!(metadata_streams(&result).len(), 2, "catalog and image XMP");
    assert_eq!((count_key(&result, b"Thumb"), count_key(&result, b"PieceInfo")), (1, 2));
    let report = &output.report;
    assert!(!report.metadata_removed && !report.metadata_kept_for_standard);
    assert_eq!((report.thumbnails_removed, report.editing_data_removed), (0, 0));
}

#[test]
fn metadata_and_thumbnails_are_removed_on_request_and_pages_stay_identical() {
    let input = exported(Some(PLAIN_XMP), |_| {});
    let lossless = compress(&input, &PdfOptions::LOSSLESS).expect("compresses");

    let output = compress(&input, &STRIP_METADATA).expect("compresses");

    let result = load(&output.bytes);
    assert!(!result.trailer.has(b"Info"));
    assert!(metadata_streams(&result).is_empty(), "every XMP packet is gone, not just unlinked");
    assert_eq!(count_key(&result, b"Metadata") + count_key(&result, b"Thumb"), 0);
    assert_eq!(count_key(&result, b"PieceInfo"), 2, "editing data is a separate option");
    assert_eq!(page_contents(&result), page_contents(&load(&input)));
    let report = &output.report;
    assert!(report.metadata_removed && !report.metadata_kept_for_standard);
    assert_eq!((report.thumbnails_removed, report.editing_data_removed), (1, 0));
    assert!(output.bytes.len() < lossless.bytes.len(), "{} vs {}", output.bytes.len(), lossless.bytes.len());
}

#[test]
fn editing_data_is_removed_with_the_private_streams_it_pointed_to() {
    let input = exported(Some(PLAIN_XMP), |_| {});
    let lossless = compress(&input, &PdfOptions::LOSSLESS).expect("compresses");

    let output = compress(&input, &STRIP_EDITING_DATA).expect("compresses");

    let result = load(&output.bytes);
    assert_eq!(count_key(&result, b"PieceInfo"), 0);
    assert!(!has_private_data(&result));
    assert!(result.trailer.has(b"Info"), "metadata is a separate option");
    assert_eq!(metadata_streams(&result).len(), 2);
    assert_eq!(page_contents(&result), page_contents(&load(&input)));
    assert_eq!(output.report.editing_data_removed, 2);
    // The private data dominates design exports; here it is more than half of the file.
    assert!(output.bytes.len() * 2 < lossless.bytes.len(), "{} vs {}", output.bytes.len(), lossless.bytes.len());
}

#[test]
fn declared_standards_keep_their_document_metadata() {
    let standards = [
        r#"xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" pdfaid:part="2" pdfaid:conformance="B""#,
        r#"xmlns:pdfuaid="http://www.aiim.org/pdfua/ns/id/" pdfuaid:part="1""#,
        r#"xmlns:pdfxid="http://www.npes.org/pdfx/ns/id/" pdfxid:GTS_PDFXVersion="PDF/X-4""#,
    ];
    for properties in standards {
        let input = exported(Some(properties), |_| {});
        let before = catalog_xmp(&load(&input));

        let output = compress(&input, &STRIP_ALL).expect("compresses");

        let result = load(&output.bytes);
        assert_eq!(catalog_xmp(&result), before, "{properties}");
        assert!(result.trailer.has(b"Info"), "{properties}");
        assert_eq!(metadata_streams(&result).len(), 1, "image XMP is not required: {properties}");
        assert_eq!(count_key(&result, b"Thumb") + count_key(&result, b"PieceInfo"), 0, "{properties}");
        assert!(output.report.metadata_kept_for_standard, "{properties}");
    }
}

#[test]
fn pdfx_declared_in_the_info_dictionary_keeps_it() {
    let input = exported(None, |pdf| {
        pdf.info(dictionary! {
            "GTS_PDFXVersion" => Object::string_literal("PDF/X-1:2001"),
            "Title" => Object::string_literal("Poster"),
        });
    });

    let output = compress(&input, &STRIP_METADATA).expect("compresses");

    let result = load(&output.bytes);
    let info = result.trailer.get(b"Info").and_then(Object::as_reference).expect("Info kept");
    assert!(result.get_dictionary(info).is_ok_and(|info| info.has(b"GTS_PDFXVersion")));
    assert!(output.report.metadata_kept_for_standard);
}

#[test]
fn accessibility_structure_survives_every_option() {
    let input = exported(Some(PLAIN_XMP), PdfBuilder::structure_tree);

    let output = compress(&input, &STRIP_ALL).expect("compresses");

    let result = load(&output.bytes);
    let catalog = result.catalog().expect("catalog");
    assert!(catalog.has(b"StructTreeRoot") && catalog.has(b"MarkInfo"));
    assert_eq!(count_key(&result, b"PieceInfo") + metadata_streams(&result).len(), 0);
}

#[test]
fn stripping_never_makes_a_file_larger() {
    let once = compress(&exported(Some(PLAIN_XMP), |_| {}), &STRIP_ALL).expect("first pass");

    let twice = compress(&once.bytes, &STRIP_ALL).expect("second pass");

    assert!(twice.bytes.len() <= once.bytes.len());
    if twice.report.kept_original {
        assert_eq!(twice.bytes, once.bytes);
        assert!(!twice.report.metadata_removed, "a kept original reports no removal");
    }
}
