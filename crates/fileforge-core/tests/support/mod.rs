//! Builds small PDFs in code so tests never depend on binary sample files.
#![allow(dead_code)]

// Core
use jpeg_encoder::{ColorType, Encoder};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

/// Photo-like pixels: smooth gradients plus deterministic noise, so JPEG and Deflate behave as on real photos.
pub fn photo(width: u32, height: u32, gray: bool) -> Vec<u8> {
    let channels = if gray { 1 } else { 3 };
    let mut seed: u32 = 0x2545_f491;
    let mut pixels = Vec::with_capacity((width * height) as usize * channels);
    for y in 0..height {
        for x in 0..width {
            for channel in 0..channels as u32 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let base = (x * 255 / width + y * 128 / height + channel * 40) % 256;
                let noise = (seed % 24) as i32 - 12;
                pixels.push((base as i32 + noise).clamp(0, 255) as u8);
            }
        }
    }
    pixels
}

pub fn jpeg(width: u32, height: u32, quality: u8, gray: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let color = if gray { ColorType::Luma } else { ColorType::Rgb };
    Encoder::new(&mut out, quality)
        .encode(&photo(width, height, gray), width as u16, height as u16, color)
        .expect("test JPEG encodes");
    out
}

pub struct PdfBuilder {
    pub doc: Document,
    pages_id: ObjectId,
    kids: Vec<ObjectId>,
    catalog_extra: Vec<(&'static str, Object)>,
}

impl Default for PdfBuilder {
    fn default() -> Self {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        Self { doc, pages_id, kids: Vec::new(), catalog_extra: Vec::new() }
    }
}

impl PdfBuilder {
    pub fn jpeg_image(&mut self, width: u32, height: u32, quality: u8, gray: bool) -> ObjectId {
        let color_space = if gray { "DeviceGray" } else { "DeviceRGB" };
        self.image(width, height, color_space, Some("DCTDecode"), jpeg(width, height, quality, gray))
    }

    /// Uncompressed 8-bit RGB pixels.
    pub fn raw_image(&mut self, width: u32, height: u32) -> ObjectId {
        self.image(width, height, "DeviceRGB", None, photo(width, height, false))
    }

    pub fn image(
        &mut self,
        width: u32,
        height: u32,
        color_space: &str,
        filter: Option<&str>,
        content: Vec<u8>,
    ) -> ObjectId {
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(width),
            "Height" => i64::from(height),
            "ColorSpace" => color_space,
            "BitsPerComponent" => 8,
        };
        if let Some(filter) = filter {
            dict.set("Filter", Object::Name(filter.as_bytes().to_vec()));
        }
        self.doc.add_object(Stream::new(dict, content).with_compression(false))
    }

    /// A form XObject that draws `image` through `matrix`.
    pub fn form(&mut self, matrix: [f32; 6], content: &str, image: (&str, ObjectId)) -> ObjectId {
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
            "Matrix" => matrix.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
            "Resources" => dictionary! { "XObject" => dictionary! { image.0 => image.1 } },
        };
        self.doc.add_object(Stream::new(dict, content.as_bytes().to_vec()).with_compression(false))
    }

    /// Adds a page with an uncompressed content stream and the given XObjects in its resources.
    pub fn page(&mut self, content: &str, xobjects: &[(&str, ObjectId)]) -> ObjectId {
        let content_id =
            self.doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()).with_compression(false));
        let mut xobject_dict = Dictionary::new();
        for (name, id) in xobjects {
            xobject_dict.set(*name, *id);
        }
        let page_id = self.doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => xobject_dict },
        });
        self.kids.push(page_id);
        page_id
    }

    /// An object nothing refers to, like the leftovers of incremental saves.
    pub fn unused_object(&mut self) {
        self.doc.add_object(Stream::new(Dictionary::new(), vec![b'x'; 4096]).with_compression(false));
    }

    pub fn signature(&mut self) {
        let signature = self.doc.add_object(dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "ByteRange" => vec![0.into(), 100.into(), 200.into(), 300.into()],
            "Contents" => Object::string_literal(vec![0u8; 64]),
        });
        let field =
            self.doc.add_object(dictionary! { "FT" => "Sig", "T" => Object::string_literal("Sig1"), "V" => signature });
        self.catalog_extra
            .push(("AcroForm", Object::Dictionary(dictionary! { "Fields" => vec![field.into()], "SigFlags" => 3 })));
    }

    pub fn pdfa1_metadata(&mut self) {
        let xmp = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description pdfaid:part="1" pdfaid:conformance="B"/></rdf:RDF></x:xmpmeta>"#;
        let metadata =
            self.doc.add_object(Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp.to_vec()));
        self.catalog_extra.push(("Metadata", metadata.into()));
    }

    pub fn build(mut self) -> Document {
        let count = self.kids.len() as i64;
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => self.kids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => count,
        };
        self.doc.objects.insert(self.pages_id, Object::Dictionary(pages));
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => self.pages_id };
        for (key, value) in std::mem::take(&mut self.catalog_extra) {
            catalog.set(key, value);
        }
        let catalog_id = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", catalog_id);
        self.doc
    }

    /// Classic cross-reference table, nothing compressed: the kind of file scanners and old tools produce.
    pub fn bytes(self) -> Vec<u8> {
        let mut doc = self.build();
        doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("fixture saves");
        out
    }
}
