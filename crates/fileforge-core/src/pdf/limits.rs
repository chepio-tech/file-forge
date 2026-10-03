/// Largest input file the engine accepts. The whole document is held in memory (ADR-0002).
pub const MAX_INPUT_BYTES: u64 = 1 << 30;

/// Resource bounds for one compression. Inputs are untrusted: every allocation driven by file content is capped.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub max_input_bytes: u64,
    /// Decoded size of any single stream the engine reads (decompression-bomb guard).
    pub max_stream_bytes: usize,
    /// Images with more pixels than this are left untouched.
    pub max_image_pixels: u64,
}

impl Limits {
    pub const DEFAULT: Self =
        Self { max_input_bytes: MAX_INPUT_BYTES, max_stream_bytes: 512 << 20, max_image_pixels: 150_000_000 };
}
