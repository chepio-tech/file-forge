//! Merges byte-identical streams (fonts, images, ICC profiles repeated by merging tools) into one object.

// Core
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use lopdf::{Document, Object, ObjectId};
// Domain
use super::objects::rewrite_references;

/// Returns how many duplicate streams were removed.
pub(crate) fn merge_identical_streams(doc: &mut Document) -> u32 {
    let mut buckets: HashMap<u64, Vec<ObjectId>> = HashMap::new();
    for (id, object) in &doc.objects {
        if let Object::Stream(stream) = object {
            let mut hasher = DefaultHasher::new();
            // Dictionary has no `Hash`; its Debug form is a stable proxy. Equality is re-checked exactly below.
            format!("{:?}", stream.dict).hash(&mut hasher);
            stream.content.hash(&mut hasher);
            buckets.entry(hasher.finish()).or_default().push(*id);
        }
    }

    let mut replacements = HashMap::new();
    for ids in buckets.into_values().filter(|ids| ids.len() > 1) {
        // BTreeMap iteration gave ascending ids; the first of each equal group survives.
        for (index, &candidate) in ids.iter().enumerate() {
            if replacements.contains_key(&candidate) {
                continue;
            }
            for &other in &ids[index + 1..] {
                if !replacements.contains_key(&other) && doc.objects.get(&candidate) == doc.objects.get(&other) {
                    replacements.insert(other, candidate);
                }
            }
        }
    }

    if replacements.is_empty() {
        return 0;
    }
    rewrite_references(doc, &replacements);
    for duplicate in replacements.keys() {
        doc.objects.remove(duplicate);
    }
    u32::try_from(replacements.len()).unwrap_or(u32::MAX)
}
