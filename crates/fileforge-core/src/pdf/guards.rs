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

/// PDF/A-1 (ISO 19005-1) forbids object and cross-reference streams; such files keep a classic layout.
pub(crate) fn is_pdfa1(doc: &Document, max_metadata_bytes: usize) -> bool {
    let Ok(catalog) = doc.catalog() else { return false };
    let Some(Object::Stream(metadata)) = catalog.get(b"Metadata").ok().and_then(|m| resolve(doc, m)) else {
        return false;
    };
    let Ok(xmp) = metadata.get_plain_content_with_limit(max_metadata_bytes) else { return false };
    pdfa_part(&xmp) == Some(1)
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
