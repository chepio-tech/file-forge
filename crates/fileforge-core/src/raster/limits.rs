// Core
use std::time::Duration;

/// Largest image file the engine accepts; the whole file is held in memory.
pub const MAX_INPUT_BYTES: u64 = 256 << 20;

/// Resource bounds for one compression. Inputs are untrusted: every allocation driven by file content is capped.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub max_input_bytes: u64,
    /// Larger images are refused. Covers 100-megapixel cameras; a 4:4:4 JPEG at the limit needs about 0.7 GB of
    /// coefficients, the same again for its decoded pixels.
    pub max_pixels: u64,
    /// Decoded PNG rows (all channels and bit depths) above this are refused: oxipng keeps several filtered copies.
    pub max_png_bytes: usize,
    /// Zopfli only runs on PNGs whose decoded rows fit this. Measured 2026-10-08: about 2.5 s for a 256×256 RGBA
    /// icon, but 40 s for 1–2-megapixel screenshots, which it made only 1–2.6% smaller than level 6 alone.
    pub max_zopfli_bytes: usize,
    /// oxipng stops trying further filters after this long and keeps its best result so far.
    pub png_timeout: Duration,
    /// Lossless WebPs up to this many pixels get libwebp's slowest level (up to about 12 s measured at 3.7 MP);
    /// larger ones a level that costs seconds, not minutes.
    pub max_webp_effort_pixels: u64,
}

impl Limits {
    pub const DEFAULT: Self = Self {
        max_input_bytes: MAX_INPUT_BYTES,
        max_pixels: 120_000_000,
        max_png_bytes: 512 << 20,
        max_zopfli_bytes: 512 << 10,
        png_timeout: Duration::from_secs(60),
        // 2048 × 2048: covers 2560 × 1600 screens.
        max_webp_effort_pixels: 2048 * 2048,
    };
}
