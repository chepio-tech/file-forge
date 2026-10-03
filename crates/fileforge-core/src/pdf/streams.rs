//! Lossless re-encoding of streams with the strongest Deflate level.

// Core
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use lopdf::{Document, Object, Stream};
// Domain
use super::control::{Control, Counter, Stage};
use super::objects::filters;

/// Compresses unfiltered streams and re-deflates Flate streams (without predictors) when that makes them smaller.
/// Decoded content never changes. XMP metadata stays uncompressed so other tools (and PDF/A) can read it.
/// Stops early when cancelled; the caller then discards the document.
pub(crate) fn recompress_streams(doc: &mut Document, max_stream_bytes: usize, control: &dyn Control) {
    let total = doc.objects.values().filter(|object| matches!(object, Object::Stream(_))).count();
    let counter = Counter::start(control, Stage::Streams, total);
    for object in doc.objects.values_mut() {
        let Object::Stream(stream) = object else { continue };
        if control.is_cancelled() {
            return;
        }
        recompress(stream, max_stream_bytes);
        counter.advance();
    }
}

fn recompress(stream: &mut Stream, max_stream_bytes: usize) {
    if !stream.allows_compression || is_metadata(stream) {
        return;
    }
    let Some(chain) = filters(stream) else { return };
    match chain.as_slice() {
        [] => {
            if let Some(packed) = deflate(&stream.content)
                && packed.len() < stream.content.len()
            {
                stream.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
                stream.set_content(packed);
            }
        }
        [b"FlateDecode"] if !stream.dict.has(b"DecodeParms") => {
            let Ok(plain) = stream.decompressed_content_with_limit(max_stream_bytes) else { return };
            if let Some(packed) = deflate(&plain)
                && packed.len() < stream.content.len()
            {
                stream.set_content(packed);
            }
        }
        _ => {}
    }
}

fn is_metadata(stream: &Stream) -> bool {
    stream.dict.get(b"Type").and_then(Object::as_name).is_ok_and(|name| name == b"Metadata")
}

pub(crate) fn deflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::with_capacity(data.len() / 2), Compression::best());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}
