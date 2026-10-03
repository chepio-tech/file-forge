//! Errors returned to the webview. The UI maps `code` to a localized message; `detail` is for logs and bug reports.

// Core
use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "camelCase")]
pub enum AppError {
    /// The path is a directory or another non-regular file.
    #[error("not a regular file: {0}")]
    NotAFile(String),
    #[error("i/o error: {0}")]
    Io(String),
    /// A background task panicked or was cancelled; the app keeps running.
    #[error("internal error: {0}")]
    Internal(String),
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
    }
}
