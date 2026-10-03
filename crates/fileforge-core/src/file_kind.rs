//! Classification of input files into the groups the UI offers tools for.

// Core
use serde::{Deserialize, Serialize};

/// How many leading bytes [`FileKind::detect`] needs to recognize a signature.
pub const SNIFF_LEN: usize = 1024;

/// The group of tools a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Pdf,
    Image,
    Video,
    Audio,
    Other,
}

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "tif", "tiff", "bmp"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg", "opus"];

impl FileKind {
    /// Classifies a file by its content and extension.
    ///
    /// `head` is the first [`SNIFF_LEN`] bytes of the file (or the whole file if shorter). A PDF is recognized only by
    /// its `%PDF-` header, never by extension alone, so a renamed non-PDF is not handed to the PDF engine. Other
    /// kinds currently rely on the extension; their engines validate content themselves.
    pub fn detect(extension: Option<&str>, head: &[u8]) -> Self {
        if is_pdf(head) {
            return Self::Pdf;
        }
        let Some(extension) = extension else {
            return Self::Other;
        };
        let extension = extension.to_ascii_lowercase();
        let extension = extension.as_str();
        if IMAGE_EXTENSIONS.contains(&extension) {
            Self::Image
        } else if VIDEO_EXTENSIONS.contains(&extension) {
            Self::Video
        } else if AUDIO_EXTENSIONS.contains(&extension) {
            Self::Audio
        } else {
            Self::Other
        }
    }

    /// File extensions offered in the "open file" dialog for this kind. Empty for [`FileKind::Other`].
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Pdf => &["pdf"],
            Self::Image => IMAGE_EXTENSIONS,
            Self::Video => VIDEO_EXTENSIONS,
            Self::Audio => AUDIO_EXTENSIONS,
            Self::Other => &[],
        }
    }
}

/// The PDF spec (ISO 32000-2, 7.5.2) puts `%PDF-` at offset 0, but readers accept up to 1024 bytes of leading
/// garbage, and so do we.
fn is_pdf(head: &[u8]) -> bool {
    let window = &head[..head.len().min(SNIFF_LEN)];
    window.windows(5).any(|w| w == b"%PDF-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_is_detected_by_header() {
        assert_eq!(FileKind::detect(Some("pdf"), b"%PDF-1.7\n..."), FileKind::Pdf);
    }

    #[test]
    fn pdf_header_wins_over_wrong_extension() {
        assert_eq!(FileKind::detect(Some("bin"), b"%PDF-2.0\n"), FileKind::Pdf);
        assert_eq!(FileKind::detect(None, b"%PDF-1.4"), FileKind::Pdf);
    }

    #[test]
    fn pdf_header_after_leading_garbage_is_accepted() {
        let mut head = vec![b' '; 600];
        head.extend_from_slice(b"%PDF-1.5");
        assert_eq!(FileKind::detect(Some("pdf"), &head), FileKind::Pdf);
    }

    #[test]
    fn pdf_extension_without_header_is_not_a_pdf() {
        assert_eq!(FileKind::detect(Some("pdf"), b"PK\x03\x04 zip in disguise"), FileKind::Other);
        assert_eq!(FileKind::detect(Some("pdf"), b""), FileKind::Other);
    }

    #[test]
    fn other_kinds_are_detected_by_extension_case_insensitively() {
        assert_eq!(FileKind::detect(Some("JPG"), b"\xFF\xD8\xFF"), FileKind::Image);
        assert_eq!(FileKind::detect(Some("heic"), b""), FileKind::Image);
        assert_eq!(FileKind::detect(Some("MoV"), b""), FileKind::Video);
        assert_eq!(FileKind::detect(Some("flac"), b""), FileKind::Audio);
        assert_eq!(FileKind::detect(Some("docx"), b""), FileKind::Other);
        assert_eq!(FileKind::detect(None, b"plain text"), FileKind::Other);
    }

    #[test]
    fn every_listed_extension_maps_back_to_its_kind() {
        for kind in [FileKind::Pdf, FileKind::Image, FileKind::Video, FileKind::Audio] {
            for extension in kind.extensions() {
                let head: &[u8] = if kind == FileKind::Pdf { b"%PDF-1.7" } else { b"" };
                assert_eq!(FileKind::detect(Some(extension), head), kind, "extension {extension}");
            }
        }
    }

    #[test]
    fn serializes_as_camel_case_for_the_ui() {
        assert_eq!(serde_json::to_string(&FileKind::Pdf).ok().as_deref(), Some("\"pdf\""));
        assert_eq!(serde_json::from_str::<FileKind>("\"video\"").ok(), Some(FileKind::Video));
    }
}
