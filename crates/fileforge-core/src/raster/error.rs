// Core
use thiserror::Error;

/// Why an image could not be compressed. Each variant is a distinct message in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RasterError {
    #[error("the file is larger than the {limit}-byte limit")]
    TooLarge { limit: u64 },
    #[error("the image has more than {limit} pixels")]
    TooManyPixels { limit: u64 },
    /// Not a JPEG or PNG file (by content, whatever the extension says).
    #[error("unsupported image format: {0}")]
    Unsupported(String),
    #[error("the file is not a readable image: {0}")]
    Malformed(String),
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    /// The caller's [`crate::control::Control`] asked to stop; nothing was produced.
    #[error("the compression was cancelled")]
    Cancelled,
    /// An encoder failed on data that decoded correctly; the original is untouched.
    #[error("internal error: {0}")]
    Internal(String),
}

pub(crate) fn malformed(detail: &str) -> RasterError {
    RasterError::Malformed(detail.into())
}
