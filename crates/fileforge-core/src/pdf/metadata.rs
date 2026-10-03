//! Optional removal of document metadata and editor-private data (ADR-0012). Nothing removed here is drawn on a
//! page; the prune pass that follows deletes the objects these entries pointed to.

// Core
use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetadataStats {
    pub removed: bool,
    /// The catalog XMP and the Info dictionary stayed because the file declares a standard that requires them.
    pub kept_for_standard: bool,
    pub thumbnails_removed: u32,
}

/// Removes XMP metadata from every object, the Info dictionary and page thumbnails. With `keep_document_metadata`
/// the catalog's XMP and the Info dictionary stay.
pub(crate) fn strip_metadata(doc: &mut Document, keep_document_metadata: bool) -> MetadataStats {
    let mut stats = MetadataStats { kept_for_standard: keep_document_metadata, ..MetadataStats::default() };
    let catalog = doc.trailer.get(b"Root").and_then(Object::as_reference).ok();
    if !keep_document_metadata && doc.trailer.remove(b"Info").is_some() {
        stats.removed = true;
    }
    let pages: HashSet<ObjectId> = doc.get_pages().into_values().collect();
    for (id, object) in &mut doc.objects {
        let Some(dict) = dictionary_mut(object) else { continue };
        let keep_xmp = keep_document_metadata && Some(*id) == catalog;
        if !keep_xmp && dict.remove(b"Metadata").is_some() {
            stats.removed = true;
        }
        if pages.contains(id) && dict.remove(b"Thumb").is_some() {
            stats.thumbnails_removed += 1;
        }
    }
    stats
}

/// Removes `/PieceInfo` from every object. Returns how many objects carried it.
pub(crate) fn strip_editing_data(doc: &mut Document) -> u32 {
    let mut removed = 0;
    for object in doc.objects.values_mut() {
        if let Some(dict) = dictionary_mut(object)
            && dict.remove(b"PieceInfo").is_some()
        {
            removed += 1;
        }
    }
    removed
}

fn dictionary_mut(object: &mut Object) -> Option<&mut Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&mut stream.dict),
        _ => None,
    }
}
