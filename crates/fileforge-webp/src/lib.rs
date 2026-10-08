//! WebP encoding with libwebp behind one safe function (ADR-0020).
//!
//! This is the only crate in the workspace allowed to use `unsafe`. It checks the pixels in Rust, hands them to
//! libwebp's encoder and copies the result out; it never decodes. Guards free libwebp's memory on every path, and
//! the progress callback never lets a panic unwind into C.

// Core
use std::ffi::{c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

use libwebp_sys::{self as sys, WebPEncodingError};
use thiserror::Error;

/// Largest width or height of a WebP image (`WEBP_MAX_DIMENSION`).
pub const MAX_DIMENSION: u32 = 16383;

/// How the pixel buffer is laid out: rows of 8-bit samples, top to bottom, without padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Rgb,
    Rgba,
}

impl Layout {
    pub const fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

/// What kind of WebP to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Exact pixels, including the colors of fully transparent ones. `level` 0–9 trades time for size, as
    /// `cwebp -z` does.
    Lossless { level: u8 },
    /// `quality` 0–100; `method` 0–6 trades time for size. Alpha is always stored losslessly.
    Lossy { quality: u8, method: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EncodeError {
    /// The size, buffer or settings are out of range. libwebp was not called.
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    /// The `stop` callback asked to stop, or panicked, while libwebp was encoding.
    #[error("the encoding was stopped")]
    Stopped,
    #[error("libwebp failed: {0}")]
    Encoder(&'static str),
}

/// Encodes `width × height` pixels to a WebP file. libwebp calls `stop` from its encoding threads several times per
/// image; once it returns `true`, the encoding ends with [`EncodeError::Stopped`].
pub fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    layout: Layout,
    mode: Mode,
    stop: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u8>, EncodeError> {
    let stride = validate(pixels, width, height, layout)?;
    let config = config(mode)?;
    let mut picture = Picture::new(width, height)?;
    picture.import(pixels, stride, layout)?;
    let mut writer = Writer::new();
    let hook = Hook { stop };
    picture.0.writer = Some(sys::WebPMemoryWrite);
    picture.0.custom_ptr = (&raw mut writer.0).cast::<c_void>();
    picture.0.progress_hook = Some(progress);
    picture.0.user_data = (&raw const hook).cast_mut().cast::<c_void>();
    // SAFETY: `config` passed WebPValidateConfig and `picture` holds the imported ARGB pixels. `custom_ptr` points to
    // `writer` and `user_data` to `hook`; both are locals that do not move and outlive this call, the only one that
    // uses the pointers.
    let encoded = unsafe { sys::WebPEncode(&config, &mut picture.0) };
    if encoded == 0 {
        return Err(match picture.0.error_code {
            WebPEncodingError::VP8_ENC_ERROR_USER_ABORT => EncodeError::Stopped,
            code => EncodeError::Encoder(describe(code)),
        });
    }
    Ok(writer.bytes())
}

/// Checks the size and buffer length and returns the row stride in bytes.
fn validate(pixels: &[u8], width: u32, height: u32, layout: Layout) -> Result<c_int, EncodeError> {
    let sides = 1..=MAX_DIMENSION;
    if !sides.contains(&width) || !sides.contains(&height) {
        return Err(EncodeError::InvalidInput("width and height must be 1–16383 pixels"));
    }
    let stride = usize::try_from(width).map_err(|_| EncodeError::InvalidInput("width"))? * layout.channels();
    let rows = usize::try_from(height).map_err(|_| EncodeError::InvalidInput("height"))?;
    if stride.checked_mul(rows) != Some(pixels.len()) {
        return Err(EncodeError::InvalidInput("the pixel buffer does not match the size and layout"));
    }
    c_int::try_from(stride).map_err(|_| EncodeError::InvalidInput("row too long"))
}

fn config(mode: Mode) -> Result<sys::WebPConfig, EncodeError> {
    let mut config = sys::WebPConfig::new().map_err(|()| EncodeError::Encoder("libwebp version mismatch"))?;
    match mode {
        Mode::Lossless { level } => {
            if level > 9 {
                return Err(EncodeError::InvalidInput("lossless level must be 0–9"));
            }
            // SAFETY: `config` is an initialized WebPConfig owned by this frame; the call only writes its fields.
            if unsafe { sys::WebPConfigLosslessPreset(&mut config, c_int::from(level)) } == 0 {
                return Err(EncodeError::Encoder("lossless preset refused"));
            }
            config.exact = 1;
        }
        Mode::Lossy { quality, method } => {
            if quality > 100 || method > 6 {
                return Err(EncodeError::InvalidInput("quality must be 0–100 and method 0–6"));
            }
            config.quality = f32::from(quality);
            config.method = c_int::from(method);
            config.alpha_compression = 1;
            config.alpha_quality = 100;
        }
    }
    config.thread_level = 1;
    // SAFETY: `config` is an initialized WebPConfig; the call only reads it.
    if unsafe { sys::WebPValidateConfig(&config) } == 0 {
        return Err(EncodeError::Encoder("invalid configuration"));
    }
    Ok(config)
}

/// A WebPPicture that frees its pixel memory when dropped.
struct Picture(sys::WebPPicture);

impl Picture {
    fn new(width: u32, height: u32) -> Result<Self, EncodeError> {
        let mut picture = sys::WebPPicture::new().map_err(|()| EncodeError::Encoder("libwebp version mismatch"))?;
        // ARGB keeps lossless pixels exact; lossy encoding converts it to YUV itself.
        picture.use_argb = 1;
        picture.width = c_int::try_from(width).map_err(|_| EncodeError::InvalidInput("width"))?;
        picture.height = c_int::try_from(height).map_err(|_| EncodeError::InvalidInput("height"))?;
        Ok(Self(picture))
    }

    fn import(&mut self, pixels: &[u8], stride: c_int, layout: Layout) -> Result<(), EncodeError> {
        let import = match layout {
            Layout::Rgb => sys::WebPPictureImportRGB,
            Layout::Rgba => sys::WebPPictureImportRGBA,
        };
        // SAFETY: `validate` checked that `pixels` holds exactly `height` rows of `stride` bytes for this picture's
        // width and layout. libwebp copies them into memory it allocates and reads `pixels` only during the call.
        if unsafe { import(&mut self.0, pixels.as_ptr(), stride) } == 0 {
            return Err(EncodeError::Encoder(describe(self.0.error_code)));
        }
        Ok(())
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: the picture was initialized by WebPPictureInit; freeing is valid whether or not pixels were
        // allocated, and nothing uses the picture afterwards.
        unsafe { sys::WebPPictureFree(&mut self.0) }
    }
}

/// libwebp's in-memory output buffer, freed when dropped.
struct Writer(sys::WebPMemoryWriter);

impl Writer {
    fn new() -> Self {
        let mut writer = sys::WebPMemoryWriter { mem: std::ptr::null_mut(), size: 0, max_size: 0, pad: [0] };
        // SAFETY: `writer` is a valid WebPMemoryWriter owned by this frame; the call only resets its fields.
        unsafe { sys::WebPMemoryWriterInit(&mut writer) };
        Self(writer)
    }

    fn bytes(&self) -> Vec<u8> {
        if self.0.mem.is_null() {
            return Vec::new();
        }
        // SAFETY: WebPMemoryWrite keeps `mem` pointing to `size` written bytes until the writer is cleared in `drop`.
        unsafe { std::slice::from_raw_parts(self.0.mem, self.0.size) }.to_vec()
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // SAFETY: `mem` is null or was allocated by WebPMemoryWrite; nothing reads the writer afterwards.
        unsafe { sys::WebPMemoryWriterClear(&mut self.0) }
    }
}

/// What the progress callback receives through `user_data`.
struct Hook<'a> {
    stop: &'a (dyn Fn() -> bool + Sync),
}

/// libwebp's progress callback: returning 0 aborts the encoding with `VP8_ENC_ERROR_USER_ABORT`.
unsafe extern "C" fn progress(_percent: c_int, picture: *const sys::WebPPicture) -> c_int {
    // SAFETY: libwebp passes the picture given to WebPEncode, or a copy of it, both valid during the callback.
    let user_data = unsafe { (*picture).user_data };
    // SAFETY: `encode` set `user_data` to a `Hook` that lives until WebPEncode returns, and only reads happen here.
    let hook = unsafe { &*user_data.cast_const().cast::<Hook<'_>>() };
    // A panic must not unwind into C; it stops the encoding instead.
    match catch_unwind(AssertUnwindSafe(|| (hook.stop)())) {
        Ok(false) => 1,
        Ok(true) | Err(_) => 0,
    }
}

fn describe(code: WebPEncodingError) -> &'static str {
    match code {
        WebPEncodingError::VP8_ENC_ERROR_OUT_OF_MEMORY | WebPEncodingError::VP8_ENC_ERROR_BITSTREAM_OUT_OF_MEMORY => {
            "out of memory"
        }
        WebPEncodingError::VP8_ENC_ERROR_INVALID_CONFIGURATION => "invalid configuration",
        WebPEncodingError::VP8_ENC_ERROR_BAD_DIMENSION => "bad dimension",
        WebPEncodingError::VP8_ENC_ERROR_PARTITION0_OVERFLOW | WebPEncodingError::VP8_ENC_ERROR_PARTITION_OVERFLOW => {
            "partition overflow"
        }
        WebPEncodingError::VP8_ENC_ERROR_FILE_TOO_BIG => "file too big",
        _ => "encoder error",
    }
}
