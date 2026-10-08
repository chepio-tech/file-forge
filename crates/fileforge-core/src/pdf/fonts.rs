//! Embedded font programs: CFF fonts lose the subroutines no glyph calls, in every preset (ADR-0018).

// Core
use std::panic::{AssertUnwindSafe, catch_unwind};

use lopdf::{Document, Object, ObjectId};
// Domain
use super::cff;
use super::objects::dict_name;
use crate::control::Control;

/// Rewrites every bare CFF font program (`FontFile3` with `Subtype` `Type1C` or `CIDFontType0C`) that can be pruned
/// and verified; returns how many. Stops early when cancelled; the caller then discards the document.
pub(crate) fn prune_font_programs(doc: &mut Document, max_stream_bytes: usize, control: &dyn Control) -> u32 {
    let programs: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| match object {
            // Predictors are never used for font programs; a stream with `DecodeParms` is left alone rather than
            // risk decoding it differently from a reader.
            Object::Stream(stream)
                if matches!(dict_name(doc, &stream.dict, b"Subtype"), Some(b"Type1C" | b"CIDFontType0C"))
                    && !stream.dict.has(b"DecodeParms") =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect();

    let mut pruned = 0;
    for id in programs {
        if control.is_cancelled() {
            break;
        }
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else { continue };
        let Ok(plain) = stream.get_plain_content_with_limit(max_stream_bytes) else { continue };
        // A mistake in the CFF code must cost this font its savings, never the document.
        let Some(smaller) = catch_unwind(AssertUnwindSafe(|| cff::prune_subroutines(&plain))).ok().flatten() else {
            continue;
        };
        let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) else { continue };
        // Stored uncompressed here; the stream pass deflates it like every other stream.
        stream.dict.remove(b"Filter");
        stream.set_content(smaller);
        pruned += 1;
    }
    pruned
}
