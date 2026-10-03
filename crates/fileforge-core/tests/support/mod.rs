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

/// Width, height and interleaved samples of a JPEG, decoded as PDF viewers do (libjpeg-compatible).
pub fn decode_jpeg(bytes: &[u8]) -> (u16, u16, Vec<u8>) {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let pixels = decoder.decode().expect("valid JPEG");
    let info = decoder.info().expect("decoded JPEG has info");
    (info.width, info.height, pixels)
}

/// Mean absolute difference per sample.
pub fn mean_error(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "sample counts differ");
    a.iter().zip(b).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / a.len() as f64
}

pub fn jpeg(width: u32, height: u32, quality: u8, gray: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let color = if gray { ColorType::Luma } else { ColorType::Rgb };
    Encoder::new(&mut out, quality)
        .encode(&photo(width, height, gray), width as u16, height as u16, color)
        .expect("test JPEG encodes");
    out
}

/// `photo(16, 12, false)` as a lossless (5/3) JPEG 2000 codestream, 789 bytes. No Rust JPEG 2000 encoder exists, so it
/// was generated once with OpenJPEG 2.5.4: Pillow `save(..., "JPEG2000", no_jp2=True)`.
const JPX_RGB_16X12: &str = "\
    ff4fff51002f0000000000100000000c0000000000000000000000100000000c00000000000000000003070101070101070101ff52000c00\
    000001000304040001ff5c000d4040484850484850484850ff640025000143726561746564206279204f70656e4a5045472076657273696f\
    6e20322e352e34ff90000a00000000029c0001ff93df802807b4f7ef85cfb41805ac5eb5ff7fcfb418097f50d8c87fcfc01607c80a1f6818\
    0cacf35a170a9d0b3e3fcfc0161f501c7e00601079da954709be3f0bca60cfc01a07c80e1f68180a64523b115f0a427f0bfa17cfc0321f50\
    5cfcc3801f446c2818fde58f9e96e6f11e998a790718aae89a40b41b28abcf020caf7f32edaca666dfdf9874fc0363f00e1d47214822813a\
    bc962b771b32d71b344883cff3b8371a9ad3dc5e192b31492cb4f655ed9ba7ff5f43cfc03a3ed0d8fc038014fc93dcc78d505e032d46537d\
    9f14bd52b03ca1d9ae67a834ff7f11eb3164a2faa882b0292b2abc00cfc0b23ed2a8fc0b4033df8a710b48af95508c7d0d67dfb3c96cc010\
    8050ca6c157fa4ef9942b9659d35a26cb1de99cec1214ed5ef33de69a12e8b20dd6adf1c25769ecdef3bdb2fd3d9213c7292cc1050012c60\
    8d1409cf927049626bdd172e565ff1ba77f18b3a5707fd7609eed0d27c1dbb0dc2c2ec4b99c65a13971683051726b79d86944927c965fea7\
    cfc0b23ed2a8fc0b002f9eb83dd77445966b3a0228c546d69375701b2ad303b8acae5580e0f1254cf2bb0321573ca2caaf5f3e8e592dda52\
    41a02d647e426329bc8a5bd4c918b4dc64eacd1a3dd4252a325b780ed9666cdb8234532d5ad37f2b253dc8e41b635e3dc3647692a7a7e4db\
    8d309efcd4945a159b7a571b20721d75c7622a79c8256e010135bfcfc0b67e0551f8180026099af0b746be9e76ea8bbd36338ab97dcd9074\
    147542856845c0a829f679120c2a6ed678c5bb4a86fae0b9ef56f9cfc5eb5d332905947194608269138e0e25d850afe67648f83965912f3c\
    64fe2b46440f06981acb3f247b5732eb0d3343591946ff80f28764baf1a1617ecf6e054ba90eabb242190d33012ab3f8c53a11e75ee5920a\
    3e33bfffd9";

pub fn jpx_rgb_16x12() -> Vec<u8> {
    let digits: Vec<u8> = JPX_RGB_16X12.bytes().filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16).expect("hex digit"))
        .collect()
}

pub struct PdfBuilder {
    pub doc: Document,
    pages_id: ObjectId,
    kids: Vec<ObjectId>,
    catalog_extra: Vec<(&'static str, Object)>,
    info: Option<ObjectId>,
}

impl Default for PdfBuilder {
    fn default() -> Self {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        Self { doc, pages_id, kids: Vec::new(), catalog_extra: Vec::new(), info: None }
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

    /// Document Info dictionary, as authoring tools write it.
    pub fn info(&mut self, entries: Dictionary) {
        self.info = Some(self.doc.add_object(entries));
    }

    /// Catalog XMP packet whose `rdf:Description` carries `properties`.
    pub fn xmp(&mut self, properties: &str) {
        let metadata = self.metadata_stream(properties);
        self.catalog_extra.push(("Metadata", metadata.into()));
    }

    /// XMP attached to any object, as Photoshop does for placed images.
    pub fn object_xmp(&mut self, target: ObjectId, properties: &str) {
        let metadata = self.metadata_stream(properties);
        self.dict_mut(target).set("Metadata", metadata);
    }

    pub fn thumbnail(&mut self, page: ObjectId) {
        let thumbnail = self.raw_image(76, 99);
        self.dict_mut(page).set("Thumb", thumbnail);
    }

    /// Illustrator-style private data: `/PieceInfo` → `/Illustrator` → `/Private` → a data stream of about `bytes`.
    pub fn piece_info(&mut self, target: ObjectId, bytes: u32) {
        let data =
            self.doc.add_object(Stream::new(Dictionary::new(), photo(bytes / 3, 1, false)).with_compression(false));
        let private = self.doc.add_object(dictionary! { "AIPrivateData1" => data });
        let illustrator = self.doc.add_object(
            dictionary! { "LastModified" => Object::string_literal("D:20261003120000Z"), "Private" => private },
        );
        self.dict_mut(target).set("PieceInfo", dictionary! { "Illustrator" => illustrator });
    }

    /// Tagged-PDF markers used by screen readers.
    pub fn structure_tree(&mut self) {
        let root = self.doc.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => Vec::<Object>::new() });
        self.catalog_extra.push(("StructTreeRoot", root.into()));
        self.catalog_extra.push(("MarkInfo", Object::Dictionary(dictionary! { "Marked" => true })));
    }

    fn metadata_stream(&mut self, properties: &str) -> ObjectId {
        let xmp = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description {properties}/></rdf:RDF></x:xmpmeta>"#
        );
        let dict = dictionary! { "Type" => "Metadata", "Subtype" => "XML" };
        self.doc.add_object(Stream::new(dict, xmp.into_bytes()).with_compression(false))
    }

    pub fn dict_mut(&mut self, id: ObjectId) -> &mut Dictionary {
        match self.doc.objects.get_mut(&id).expect("fixture object exists") {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &mut stream.dict,
            other => panic!("fixture object {id:?} has no dictionary: {other:?}"),
        }
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
        if let Some(info) = self.info {
            self.doc.trailer.set("Info", info);
        }
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
