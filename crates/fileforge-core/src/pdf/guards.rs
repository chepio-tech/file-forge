//! Properties of the input that change what the engine is allowed to do.

// Core
use lopdf::{Dictionary, Document, Object};
// Domain
use super::objects::resolve;

/// A signature dictionary (`/ByteRange` + `/Contents`) means the bytes are signed; rewriting would break it.
pub(crate) fn is_signed(doc: &Document) -> bool {
    doc.objects.values().any(contains_signature)
}

fn contains_signature(object: &Object) -> bool {
    match object {
        Object::Dictionary(dict) => is_signature(dict) || dict.iter().any(|(_, value)| contains_signature(value)),
        Object::Stream(stream) => is_signature(&stream.dict),
        Object::Array(items) => items.iter().any(contains_signature),
        _ => false,
    }
}

fn is_signature(dict: &Dictionary) -> bool {
    dict.has(b"ByteRange") && dict.has(b"Contents")
}

/// XMP properties by which ISO standards declare conformance (PDF/A, PDF/UA, PDF/X, PDF/E, PDF/VT).
const CONFORMANCE_KEYS: [&[u8]; 5] =
    [b"pdfaid:part", b"pdfuaid:part", b"pdfxid:GTS_PDFXVersion", b"pdfe:ISO_PDFEVersion", b"pdfvtid:GTS_PDFVTVersion"];

/// PDF/A-1 (ISO 19005-1) forbids object and cross-reference streams; such files keep a classic layout.
pub(crate) fn is_pdfa1(doc: &Document, max_metadata_bytes: usize) -> bool {
    catalog_xmp(doc, max_metadata_bytes).is_some_and(|xmp| pdfa_part(&xmp) == Some(1))
}

/// The file claims conformance to a standard that requires its document metadata. PDF/X-1a and PDF/X-3 declare it
/// in the Info dictionary instead of XMP.
pub(crate) fn declares_standard(doc: &Document, max_metadata_bytes: usize) -> bool {
    let in_xmp = catalog_xmp(doc, max_metadata_bytes)
        .is_some_and(|xmp| CONFORMANCE_KEYS.iter().any(|key| xmp.windows(key.len()).any(|window| window == *key)));
    let in_info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|info| resolve(doc, info))
        .and_then(|info| info.as_dict().ok())
        .is_some_and(|info| info.has(b"GTS_PDFXVersion"));
    in_xmp || in_info
}

fn catalog_xmp(doc: &Document, max_metadata_bytes: usize) -> Option<Vec<u8>> {
    let catalog = doc.catalog().ok()?;
    let Some(Object::Stream(metadata)) = catalog.get(b"Metadata").ok().and_then(|m| resolve(doc, m)) else {
        return None;
    };
    metadata.get_plain_content_with_limit(max_metadata_bytes).ok()
}

/// Reads `pdfaid:part` from XMP, written either as an attribute (`pdfaid:part="1"`) or an element
/// (`<pdfaid:part>1</pdfaid:part>`).
fn pdfa_part(xmp: &[u8]) -> Option<u8> {
    const KEY: &[u8] = b"pdfaid:part";
    let start = xmp.windows(KEY.len()).position(|window| window == KEY)? + KEY.len();
    xmp[start..]
        .iter()
        .take(16)
        .find(|byte| !matches!(byte, b'=' | b'"' | b'\'' | b'>' | b' ' | b'\n' | b'\r' | b'\t'))
        .filter(|byte| byte.is_ascii_digit())
        .map(|digit| digit - b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pdfa_part_in_both_xmp_forms() {
        assert_eq!(pdfa_part(br#"<rdf:Description pdfaid:part="1" pdfaid:conformance="B"/>"#), Some(1));
        assert_eq!(pdfa_part(b"<pdfaid:part>2</pdfaid:part>"), Some(2));
        assert_eq!(pdfa_part(b"<pdfaid:part>\n  3\n</pdfaid:part>"), Some(3));
        assert_eq!(pdfa_part(b"<x:xmpmeta>no conformance</x:xmpmeta>"), None);
        assert_eq!(pdfa_part(b"pdfaid:part"), None);
    }
}
