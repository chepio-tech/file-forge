// Core
use thiserror::Error;

/// Why a PDF could not be compressed. Each variant is a distinct message in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PdfError {
    #[error("the file is larger than the {limit}-byte limit")]
    TooLarge { limit: u64 },
    /// Rewriting would silently remove the password protection, so encrypted files are refused.
    #[error("the PDF is password-protected")]
    Encrypted,
    /// Any rewrite invalidates digital signatures, so signed files are refused.
    #[error("the PDF is digitally signed")]
    Signed,
    #[error("the file is not a readable PDF: {0}")]
    Malformed(String),
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    /// The caller's [`super::Control`] asked to stop; nothing was produced.
    #[error("the compression was cancelled")]
    Cancelled,
    /// The rewritten document failed verification; the original is untouched.
    #[error("internal error: {0}")]
    Internal(String),
}
