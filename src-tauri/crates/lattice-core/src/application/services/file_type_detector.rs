use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported file types for internal viewing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    WebArticleHtml,
    Pdf,
    Image,
    Text,
    Unknown,
}

impl FileType {
    /// Detect file type from file path extension
    pub fn from_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "pdf" => FileType::Pdf,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => FileType::Image,
            "txt" | "md" | "markdown" | "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "json"
            | "toml" | "yaml" | "yml" | "xml" | "html" | "css" | "sh" | "bash" => FileType::Text,
            _ => FileType::Unknown,
        }
    }

    /// Check if this file type can be rendered internally
    pub fn is_renderable_internally(&self) -> bool {
        matches!(
            self,
            FileType::WebArticleHtml | FileType::Pdf | FileType::Image | FileType::Text
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_pdf_detection() {
        let path = PathBuf::from("/test/document.pdf");
        assert_eq!(FileType::from_path(&path), FileType::Pdf);
    }

    #[test]
    fn test_image_detection() {
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/image.png")),
            FileType::Image
        );
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/photo.jpg")),
            FileType::Image
        );
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/icon.svg")),
            FileType::Image
        );
    }

    #[test]
    fn test_text_detection() {
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/file.txt")),
            FileType::Text
        );
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/README.md")),
            FileType::Text
        );
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/main.rs")),
            FileType::Text
        );
    }

    #[test]
    fn test_unknown_detection() {
        assert_eq!(
            FileType::from_path(&PathBuf::from("/test/file.xyz")),
            FileType::Unknown
        );
    }

    #[test]
    fn test_is_renderable() {
        assert!(FileType::Pdf.is_renderable_internally());
        assert!(FileType::Image.is_renderable_internally());
        assert!(FileType::Text.is_renderable_internally());
        assert!(!FileType::Unknown.is_renderable_internally());
    }
}
