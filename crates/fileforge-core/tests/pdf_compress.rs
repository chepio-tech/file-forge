//! Engine guarantees from ADR-0003 and the rules in `docs/domain/invariants.md`, checked on fixtures built in code.

mod support;

// Core
use fileforge_core::pdf::{ImageOptions, PdfError, PdfOptions, compress};
use lopdf::{Dictionary, Document, Object, ObjectId};
// Utils
use support::{PdfBuilder, decode_jpeg, jpeg, jpx_rgb_16x12, mean_error, photo};

const BALANCED: PdfOptions = PdfOptions {
    images: Some(ImageOptions { jpeg_quality: 85, max_dpi: Some(200), compress_flate_photos: false }),
    ..PdfOptions::LOSSLESS
};
const MAXIMUM: PdfOptions = PdfOptions {
    images: Some(ImageOptions { jpeg_quality: 70, max_dpi: Some(150), compress_flate_photos: true }),
    ..PdfOptions::LOSSLESS
};

/// Text-heavy content stream, the bulk of most office documents.
fn text_content(page: usize) -> String {
    (0..200)
        .map(|line| format!("BT /F1 10 Tf 72 {} Td (Page {page} line {line} of the report) Tj ET\n", 700 - line * 3))
        .collect()
}

fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("output must load")
}

fn image_streams(doc: &Document) -> Vec<(ObjectId, &lopdf::Stream)> {
    doc.objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Stream(stream)
                if stream.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image") =>
            {
                Some((*id, stream))
            }
            _ => None,
        })
        .collect()
}

fn dimensions(stream: &lopdf::Stream) -> (i64, i64) {
    let get = |key: &[u8]| stream.dict.get(key).and_then(Object::as_i64).expect("dimension");
    (get(b"Width"), get(b"Height"))
}

fn filter(stream: &lopdf::Stream) -> Option<&[u8]> {
    stream.dict.get(b"Filter").and_then(Object::as_name).ok()
}

/// A change to an image dictionary.
type Edit = fn(&mut Dictionary);

/// A page showing the 16×12 px JPEG 2000 fixture `edit`ed at `width_pt`×`height_pt`.
fn jpx_page(content: Vec<u8>, width_pt: f64, height_pt: f64, edit: Edit) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    let image = pdf.image(16, 12, "DeviceRGB", Some("JPXDecode"), content);
    edit(pdf.dict_mut(image));
    pdf.page(&format!("q {width_pt} 0 0 {height_pt} 72 72 cm /J Do Q"), &[("J", image)]);
    pdf.bytes()
}

#[test]
fn lossless_shrinks_structure_and_keeps_every_byte_of_content() {
    let mut pdf = PdfBuilder::default();
    let logo = jpeg(320, 240, 90, false);
    let first = pdf.image(320, 240, "DeviceRGB", Some("DCTDecode"), logo.clone());
    let duplicate = pdf.image(320, 240, "DeviceRGB", Some("DCTDecode"), logo.clone());
    for page in 0..3 {
        let content = format!("{}q 160 0 0 120 72 600 cm /Im1 Do Q", text_content(page));
        pdf.page(&content, &[("Im1", if page == 0 { first } else { duplicate })]);
    }
    pdf.unused_object();
    let input = pdf.bytes();
    let original = load(&input);

    let output = compress(&input, &PdfOptions::LOSSLESS).expect("compresses");

    // The JPEG (kept as is) dominates; structure and text shrink around it.
    assert!(output.bytes.len() < input.len() / 2, "{} → {}", input.len(), output.bytes.len());
    let report = &output.report;
    assert_eq!((report.pages, report.duplicates_merged, report.images_recompressed), (3, 1, 0));
    assert!(report.unused_objects_removed >= 1);
    assert!(!report.kept_original);
    assert_eq!(report.output_size, output.bytes.len() as u64);

    let result = load(&output.bytes);
    for (page_number, page_id) in original.get_pages() {
        let before = original.get_page_content(page_id);
        let after = result.get_page_content(result.get_pages()[&page_number]);
        assert_eq!(before, after, "page {page_number} content changed");
    }
    let images = image_streams(&result);
    assert_eq!(images.len(), 1, "duplicate image merged");
    assert_eq!(images[0].1.content, logo, "lossless must not touch image bytes");
}

#[test]
fn output_is_never_larger_and_recompressing_is_stable() {
    let mut pdf = PdfBuilder::default();
    pdf.page(&text_content(0), &[]);
    let once = compress(&pdf.bytes(), &PdfOptions::LOSSLESS).expect("first pass");

    let twice = compress(&once.bytes, &PdfOptions::LOSSLESS).expect("second pass");

    assert!(twice.bytes.len() <= once.bytes.len());
    if twice.report.kept_original {
        assert_eq!(twice.bytes, once.bytes, "kept original means identical bytes");
        assert_eq!(twice.report.output_size, twice.report.original_size);
    }
}

#[test]
fn balanced_downsamples_photos_shown_above_the_target_dpi() {
    let mut pdf = PdfBuilder::default();
    let photo = pdf.jpeg_image(2400, 1800, 95, false);
    // 288×216 pt = 4×3 in → 600 DPI; Balanced targets 200 DPI → 800×600 px.
    pdf.page("q 288 0 0 216 72 500 cm /Photo Do Q", &[("Photo", photo)]);
    let input = pdf.bytes();

    let output = compress(&input, &BALANCED).expect("compresses");

    assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 1));
    let result = load(&output.bytes);
    let images = image_streams(&result);
    assert_eq!(dimensions(images[0].1), (800, 600));
    let (width, height, _) = decode_jpeg(&images[0].1.content);
    assert_eq!((width, height), (800, 600));
    assert!(output.bytes.len() < input.len() / 4, "{} → {}", input.len(), output.bytes.len());
}

#[test]
fn compressing_our_own_output_again_keeps_the_pixels() {
    let mut pdf = PdfBuilder::default();
    let photo_id = pdf.jpeg_image(320, 240, 95, false);
    pdf.page("q 320 0 0 240 72 72 cm /P Do Q", &[("P", photo_id)]);
    let once = compress(&pdf.bytes(), &BALANCED).expect("first pass");

    let twice = compress(&once.bytes, &MAXIMUM).expect("second pass");

    // zune-jpeg 0.5.15 misread the engine's own optimized-Huffman JPEGs; the second pass then saved garbage (error ~58).
    assert_eq!(twice.report.images_recompressed, 1);
    let result = load(&twice.bytes);
    let error = mean_error(&decode_jpeg(&image_streams(&result)[0].1.content).2, &photo(320, 240, false));
    assert!(error < 10.0, "mean error {error:.1} per sample after two passes");
}

#[test]
fn placement_inside_form_xobjects_is_followed() {
    let mut pdf = PdfBuilder::default();
    let photo = pdf.jpeg_image(1600, 1600, 95, true);
    // Form scales by 0.5, then the page scales by 288: the photo is drawn 2×2 in → 800 DPI. Maximum: 150 DPI → 300 px.
    let form = pdf.form([0.5, 0.0, 0.0, 0.5, 0.0, 0.0], "q 288 0 0 288 0 0 cm /P Do Q", ("P", photo));
    pdf.page("q 1 0 0 1 100 100 cm /Fm Do Q", &[("Fm", form)]);

    let output = compress(&pdf.bytes(), &MAXIMUM).expect("compresses");

    let result = load(&output.bytes);
    assert_eq!(dimensions(image_streams(&result)[0].1), (300, 300));
    assert_eq!(output.report.images_downsampled, 1);
}

#[test]
fn images_drawn_nowhere_are_reencoded_but_keep_their_size() {
    let mut pdf = PdfBuilder::default();
    let photo = pdf.jpeg_image(1200, 900, 95, false);
    pdf.page(&text_content(0), &[("Unused", photo)]);

    let output = compress(&pdf.bytes(), &MAXIMUM).expect("compresses");

    assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 0));
    assert_eq!(dimensions(image_streams(&load(&output.bytes))[0].1), (1200, 900));
}

#[test]
fn raw_images_are_downsampled_and_stored_with_deflate() {
    let mut pdf = PdfBuilder::default();
    let scan = pdf.raw_image(1600, 1200);
    // 144×108 pt = 2×1.5 in → 800 DPI; Maximum: 150 DPI → 300×225 px.
    pdf.page("q 144 0 0 108 72 72 cm /Scan Do Q", &[("Scan", scan)]);

    let output = compress(&pdf.bytes(), &MAXIMUM).expect("compresses");

    let result = load(&output.bytes);
    let (_, stream) = image_streams(&result)[0];
    assert_eq!(dimensions(stream), (300, 225));
    assert_eq!(stream.dict.get(b"Filter").and_then(Object::as_name).ok(), Some(&b"FlateDecode"[..]));
    assert_eq!(stream.decompressed_content().expect("inflates").len(), 300 * 225 * 3);
}

#[test]
fn unsupported_color_spaces_are_left_byte_identical() {
    let mut pdf = PdfBuilder::default();
    let bytes = jpeg(1200, 900, 95, false);
    let cmyk = pdf.image(1200, 900, "DeviceCMYK", Some("DCTDecode"), bytes.clone());
    pdf.page("q 72 0 0 54 72 72 cm /C Do Q", &[("C", cmyk)]);

    let output = compress(&pdf.bytes(), &MAXIMUM).expect("compresses");

    assert_eq!(output.report.images_recompressed, 0);
    assert_eq!(image_streams(&load(&output.bytes))[0].1.content, bytes);
}

#[test]
fn jpeg2000_images_become_jpeg_when_clearly_smaller() {
    let jpx = jpx_rgb_16x12();
    // 7.68×5.76 pt shows 16×12 px at exactly 150 DPI: Maximum keeps the size. The bit depth is optional for JPEG 2000.
    let input = jpx_page(jpx.clone(), 7.68, 5.76, |dict| {
        dict.remove(b"BitsPerComponent");
    });

    let output = compress(&input, &MAXIMUM).expect("compresses");

    assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 0));
    let result = load(&output.bytes);
    let (_, stream) = image_streams(&result)[0];
    assert_eq!(filter(stream), Some(&b"DCTDecode"[..]));
    assert_eq!(stream.dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok(), Some(8));
    assert_eq!(dimensions(stream), (16, 12));
    assert!(stream.content.len() < jpx.len(), "{} → {}", jpx.len(), stream.content.len());
    // The fixture is lossless, so the result must equal encoding the source pixels directly at the same quality.
    assert_eq!(decode_jpeg(&stream.content).2, decode_jpeg(&jpeg(16, 12, 70, false)).2);
}

#[test]
fn jpeg2000_images_are_downsampled_to_the_target_dpi() {
    // 3.84×2.88 pt shows 16×12 px at 300 DPI; Maximum targets 150 DPI → 8×6 px.
    let input = jpx_page(jpx_rgb_16x12(), 3.84, 2.88, |_| {});

    let output = compress(&input, &MAXIMUM).expect("compresses");

    assert_eq!((output.report.images_recompressed, output.report.images_downsampled), (1, 1));
    let result = load(&output.bytes);
    let (_, stream) = image_streams(&result)[0];
    assert_eq!((filter(stream), dimensions(stream)), (Some(&b"DCTDecode"[..]), (8, 6)));
}

#[test]
fn unusual_jpeg2000_images_are_left_byte_identical() {
    let jpx = jpx_rgb_16x12();
    let cases: [(&str, Vec<u8>, Edit); 7] = [
        ("alpha used as soft mask", jpx.clone(), |dict| dict.set("SMaskInData", 1)),
        ("Decode array", jpx.clone(), |dict| {
            dict.set("Decode", [1, 0, 1, 0, 1, 0].map(Object::from).to_vec());
        }),
        ("CMYK dictionary", jpx.clone(), |dict| dict.set("ColorSpace", "DeviceCMYK")),
        ("gray dictionary, RGB codestream", jpx.clone(), |dict| dict.set("ColorSpace", "DeviceGray")),
        ("16-bit dictionary", jpx.clone(), |dict| dict.set("BitsPerComponent", 16)),
        ("size differs from the codestream", jpx.clone(), |dict| dict.set("Width", 15)),
        ("not a codestream", b"not a JPEG 2000 codestream".to_vec(), |_| {}),
    ];
    for (case, content, edit) in cases {
        // Shown at 300 DPI, so Maximum would downsample any image it accepts.
        let input = jpx_page(content.clone(), 3.84, 2.88, edit);

        let output = compress(&input, &MAXIMUM).expect("compresses");

        assert_eq!(output.report.images_recompressed, 0, "{case}");
        let result = load(&output.bytes);
        let (_, stream) = image_streams(&result)[0];
        assert_eq!((filter(stream), &stream.content), (Some(&b"JPXDecode"[..]), &content), "{case}");
    }
}

#[test]
fn encrypted_pdfs_are_refused_even_without_a_user_password() {
    for user_password in ["", "user"] {
        let mut pdf = PdfBuilder::default();
        pdf.page(&text_content(0), &[]);
        let mut doc = pdf.build();
        let file_id = Object::string_literal(b"fileforge-test-id".to_vec());
        doc.trailer.set("ID", vec![file_id.clone(), file_id]);
        let version = lopdf::EncryptionVersion::V2 {
            document: &doc,
            owner_password: "owner",
            user_password,
            key_length: 128,
            permissions: lopdf::Permissions::all(),
        };
        let state = lopdf::EncryptionState::try_from(version).expect("encryption state");
        doc.encrypt(&state).expect("encrypts");
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("saves");

        assert_eq!(
            compress(&bytes, &PdfOptions::LOSSLESS),
            Err(PdfError::Encrypted),
            "user password {user_password:?}"
        );
    }
}

#[test]
fn signed_pdfs_are_refused() {
    let mut pdf = PdfBuilder::default();
    pdf.page(&text_content(0), &[]);
    pdf.signature();

    assert_eq!(compress(&pdf.bytes(), &PdfOptions::LOSSLESS), Err(PdfError::Signed));
}

#[test]
fn pdfa1_files_keep_a_classic_cross_reference_table() {
    let mut pdf = PdfBuilder::default();
    pdf.page(&text_content(0), &[]);
    pdf.pdfa1_metadata();

    let output = compress(&pdf.bytes(), &PdfOptions::LOSSLESS).expect("compresses");

    let contains = |needle: &[u8]| output.bytes.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(b"/ObjStm"), "PDF/A-1 forbids object streams");
    assert!(!contains(b"/Type/XRef"), "PDF/A-1 forbids cross-reference streams");
    assert!(contains(b"\nxref"));
    assert!(contains(b"pdfaid:part"), "XMP metadata stays readable (uncompressed)");
}

#[test]
fn invalid_options_are_rejected_before_reading_the_file() {
    let options = PdfOptions {
        images: Some(ImageOptions { jpeg_quality: 5, max_dpi: None, compress_flate_photos: false }),
        ..PdfOptions::LOSSLESS
    };
    assert!(matches!(compress(b"not even a pdf", &options), Err(PdfError::InvalidOptions(_))));
}

#[test]
fn malformed_input_is_an_error() {
    let mut pdf = PdfBuilder::default();
    pdf.page(&text_content(0), &[]);
    let valid = pdf.bytes();
    for input in [&b""[..], b"%PDF-1.7", b"%PDF-1.7\n%%EOF", &valid[..valid.len() / 2], b"\x89PNG\r\n\x1a\n"] {
        assert!(compress(input, &PdfOptions::LOSSLESS).is_err(), "{:?}", &input[..input.len().min(16)]);
    }
}

#[test]
fn corrupted_files_never_panic_and_never_grow() {
    let mut pdf = PdfBuilder::default();
    let photo = pdf.jpeg_image(200, 150, 90, false);
    pdf.page(&format!("{}q 100 0 0 75 72 72 cm /P Do Q", text_content(0)), &[("P", photo)]);
    let valid = pdf.bytes();
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for _ in 0..300 {
        let mut input = valid.clone();
        for _ in 0..(next() % 8 + 1) {
            let position = (next() % input.len() as u64) as usize;
            input[position] = next() as u8;
        }
        if next() % 4 == 0 {
            input.truncate((next() % input.len() as u64) as usize);
        }
        if let Ok(output) = compress(&input, &BALANCED) {
            assert!(output.bytes.len() <= input.len());
        }
    }
}
