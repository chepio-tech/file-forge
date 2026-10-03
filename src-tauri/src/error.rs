//! Errors returned to the webview. The UI maps `code` to a message (`errors` in `src/messages/messages.ts`);
//! `detail` is for logs and bug reports and is never shown verbatim.

// Core
use fileforge_core::pdf::PdfError;
use serde::Serialize;
// Types
use crate::file_registry::FileId;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "camelCase")]
pub enum AppError {
    /// The id is not (or no longer) registered, e.g. the file was removed from the list.
    #[error("unknown file id {0}")]
    UnknownFile(FileId),
    /// Saving was requested for a file that has not been compressed yet.
    #[error("no result for file id {0}")]
    NoResult(FileId),
    /// The path is a directory or another non-regular file.
    #[error("not a regular file: {0}")]
    NotAFile(String),
    #[error("the file exceeds the {0}-byte limit")]
    PdfTooLarge(u64),
    #[error("the PDF is password-protected")]
    PdfEncrypted,
    #[error("the PDF is digitally signed")]
    PdfSigned,
    #[error("not a readable PDF: {0}")]
    PdfMalformed(String),
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    #[error("i/o error: {0}")]
    Io(String),
    /// A background task panicked or was cancelled, or a result failed verification; the app keeps running.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<PdfError> for AppError {
    fn from(error: PdfError) -> Self {
        match error {
            PdfError::TooLarge { limit } => Self::PdfTooLarge(limit),
            PdfError::Encrypted => Self::PdfEncrypted,
            PdfError::Signed => Self::PdfSigned,
            PdfError::Malformed(detail) => Self::PdfMalformed(detail),
            PdfError::InvalidOptions(detail) => Self::InvalidOptions(detail),
            PdfError::Internal(detail) => Self::Internal(detail),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(error: tauri::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_detail() {
        let json = serde_json::to_value(AppError::NotAFile("Scans".into())).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "notAFile", "detail": "Scans" })));
        let json = serde_json::to_value(AppError::PdfSigned).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "pdfSigned" })));
    }

    #[test]
    fn engine_errors_keep_their_meaning() {
        assert!(matches!(AppError::from(PdfError::Encrypted), AppError::PdfEncrypted));
        assert!(matches!(AppError::from(PdfError::TooLarge { limit: 5 }), AppError::PdfTooLarge(5)));
        assert!(matches!(AppError::from(PdfError::Malformed("x".into())), AppError::PdfMalformed(d) if d == "x"));
    }
}
