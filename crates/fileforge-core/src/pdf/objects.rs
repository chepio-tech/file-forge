//! Object-graph helpers. lopdf's own traversal tracks visited ids in a `Vec` (quadratic), so these use hash sets.

// Core
use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// Ids of every object reachable from the trailer.
pub(crate) fn reachable(doc: &Document) -> HashSet<ObjectId> {
    let mut seen = HashSet::new();
    let mut pending = Vec::new();
    for (_, value) in doc.trailer.iter() {
        collect_references(value, &mut pending);
    }
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(object) = doc.objects.get(&id) {
            collect_references(object, &mut pending);
        }
    }
    seen
}

fn collect_references(object: &Object, out: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items.iter().for_each(|item| collect_references(item, out)),
        Object::Dictionary(dict) => dict.iter().for_each(|(_, value)| collect_references(value, out)),
        Object::Stream(stream) => stream.dict.iter().for_each(|(_, value)| collect_references(value, out)),
        _ => {}
    }
}

/// Removes objects nothing points to (left over from incremental saves and editors). Returns how many.
pub(crate) fn prune_unreachable(doc: &mut Document) -> u32 {
    let keep = reachable(doc);
    let before = doc.objects.len();
    doc.objects.retain(|id, _| keep.contains(id));
    u32::try_from(before - doc.objects.len()).unwrap_or(u32::MAX)
}

/// Points every reference found in `replacements` at its replacement, in all objects and the trailer.
pub(crate) fn rewrite_references(doc: &mut Document, replacements: &HashMap<ObjectId, ObjectId>) {
    for (_, value) in doc.trailer.iter_mut() {
        rewrite(value, replacements);
    }
    for object in doc.objects.values_mut() {
        rewrite(object, replacements);
    }
}

fn rewrite(object: &mut Object, replacements: &HashMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(target) = replacements.get(id) {
                *id = *target;
            }
        }
        Object::Array(items) => items.iter_mut().for_each(|item| rewrite(item, replacements)),
        Object::Dictionary(dict) => dict.iter_mut().for_each(|(_, value)| rewrite(value, replacements)),
        Object::Stream(stream) => stream.dict.iter_mut().for_each(|(_, value)| rewrite(value, replacements)),
        _ => {}
    }
}

/// Follows indirect references (bounded, cycles end as `None`).
pub(crate) fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    let mut current = object;
    for _ in 0..16 {
        match current {
            Object::Reference(id) => current = doc.objects.get(id)?,
            other => return Some(other),
        }
    }
    None
}

pub(crate) fn resolve_dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match resolve(doc, object)? {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub(crate) fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

pub(crate) fn dict_integer(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<i64> {
    match resolve(doc, dict.get(key).ok()?)? {
        Object::Integer(value) => Some(*value),
        _ => None,
    }
}

pub(crate) fn dict_name<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    match resolve(doc, dict.get(key).ok()?)? {
        Object::Name(name) => Some(name),
        _ => None,
    }
}

/// The stream's filter chain, normalized: `[]` for none, one entry per filter otherwise. `None` when malformed.
pub(crate) fn filters(stream: &Stream) -> Option<Vec<&[u8]>> {
    match stream.dict.get(b"Filter") {
        Err(_) => Some(Vec::new()),
        Ok(Object::Name(name)) => Some(vec![name.as_slice()]),
        Ok(Object::Array(items)) => items.iter().map(|item| item.as_name().ok()).collect(),
        Ok(_) => None,
    }
}
