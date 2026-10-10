//! Errors returned to the webview. The UI maps `code` to a message (`errors` in `src/messages/messages.ts`);
//! `detail` is for logs and bug reports and is never shown verbatim.

// Core
use fileforge_core::pdf::PdfError;
use fileforge_core::raster::RasterError;
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
    /// A save destination is an input file, including a file removed from the current list.
    #[error("the save destination is an original input file")]
    OriginalTarget,
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
    /// The image file or its pixel count exceeds the engine's limits; `detail` names which.
    #[error("the image is too large: {0}")]
    ImageTooLarge(String),
    /// Not a JPEG, PNG or WebP by content; `detail` names the format when known (e.g. "GIF").
    #[error("unsupported image format: {0}")]
    ImageUnsupported(String),
    #[error("not a readable image: {0}")]
    ImageMalformed(String),
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    /// The user cancelled the compression; nothing was stored.
    #[error("the compression was cancelled")]
    Cancelled,
    /// A compression, save or removal holds the work slot, so the app cannot restart for an update now.
    #[error("busy with another operation")]
    Busy,
    /// Installing an update restarts the app, which discards results that were never saved.
    #[error("unsaved results would be discarded")]
    UnsavedResults,
    /// Checking, downloading, verifying or installing an update failed; nothing was changed.
    #[error("update failed: {0}")]
    Update(String),
    #[error("image exceeds background removal limits")]
    BackgroundTooLarge,
    #[error("unsupported background removal input: {0}")]
    BackgroundUnsupported(String),
    #[error("no subject found")]
    NoSubject,
    #[error("background model is not installed")]
    ModelMissing,
    #[error("background model download or verification failed: {0}")]
    ModelDownload(String),
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
            PdfError::Cancelled => Self::Cancelled,
            PdfError::Internal(detail) => Self::Internal(detail),
        }
    }
}

impl From<RasterError> for AppError {
    fn from(error: RasterError) -> Self {
        match error {
            RasterError::TooLarge { limit } => Self::ImageTooLarge(format!("{limit} bytes")),
            RasterError::TooManyPixels { limit } => Self::ImageTooLarge(format!("{limit} pixels")),
            RasterError::Unsupported(format) => Self::ImageUnsupported(format),
            RasterError::Malformed(detail) => Self::ImageMalformed(detail),
            RasterError::NoSubject => Self::NoSubject,
            RasterError::InvalidOptions(detail) => Self::InvalidOptions(detail),
            RasterError::Cancelled => Self::Cancelled,
            RasterError::Internal(detail) => Self::Internal(detail),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<tauri_plugin_updater::Error> for AppError {
    fn from(error: tauri_plugin_updater::Error) -> Self {
        Self::Update(error.to_string())
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
        let json = serde_json::to_value(AppError::OriginalTarget).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "originalTarget" })));
        let json = serde_json::to_value(AppError::Cancelled).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "cancelled" })));
        let json = serde_json::to_value(AppError::Busy).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "busy" })));
        let json = serde_json::to_value(AppError::UnsavedResults).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "unsavedResults" })));
        let json = serde_json::to_value(AppError::Update("offline".into())).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "update", "detail": "offline" })));
    }

    #[test]
    fn engine_errors_keep_their_meaning() {
        assert!(matches!(AppError::from(PdfError::Encrypted), AppError::PdfEncrypted));
        assert!(matches!(AppError::from(PdfError::TooLarge { limit: 5 }), AppError::PdfTooLarge(5)));
        assert!(matches!(AppError::from(PdfError::Malformed("x".into())), AppError::PdfMalformed(d) if d == "x"));
        assert!(matches!(AppError::from(PdfError::Cancelled), AppError::Cancelled));
        assert!(
            matches!(AppError::from(RasterError::TooManyPixels { limit: 9 }), AppError::ImageTooLarge(d) if d == "9 pixels")
        );
        assert!(
            matches!(AppError::from(RasterError::Unsupported("GIF".into())), AppError::ImageUnsupported(d) if d == "GIF")
        );
        assert!(matches!(AppError::from(RasterError::Cancelled), AppError::Cancelled));
        let json = serde_json::to_value(AppError::ImageMalformed("x".into())).ok();
        assert_eq!(json, Some(serde_json::json!({ "code": "imageMalformed", "detail": "x" })));
    }
}
