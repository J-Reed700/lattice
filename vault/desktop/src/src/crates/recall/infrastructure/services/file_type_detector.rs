use crate::shared::error::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct FileTypeInfo {
    pub mime_type: String,
    pub extension: String,
    pub is_indexable: bool,
    pub category: FileCategory,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileCategory {
    Document,
    Code,
    Image,
    Video,
    Audio,
    Archive,
    Data,
    Binary,
}

pub struct FileTypeDetector;

impl FileTypeDetector {
    pub fn detect(path: &Path) -> Result<FileTypeInfo> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let category = Self::get_category(&extension);
        let is_indexable = Self::is_indexable(&extension);
        let mime_type = Self::get_mime_type_from_extension(&extension);

        Ok(FileTypeInfo {
            mime_type,
            extension,
            is_indexable,
            category,
        })
    }

    pub fn is_indexable(extension: &str) -> bool {
        let ext = extension.to_lowercase();
        matches!(
            ext.as_str(),
            // Documents
            "txt" | "md" | "pdf" | "docx" | "doc" | "rtf" | "odt" |
            // Code
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "css" | "scss" |
            "json" | "xml" | "yaml" | "yml" | "toml" | "sql" | "sh" | "bash" |
            "c" | "cpp" | "h" | "hpp" | "java" | "go" | "rb" | "php" | "swift" |
            "kt" | "scala" | "r" | "m" | "mm" | "cs" | "vb" | "fs" | "clj" |
            // Data
            "csv" | "tsv"
        )
    }

    pub fn get_category(extension: &str) -> FileCategory {
        let ext = extension.to_lowercase();
        match ext.as_str() {
            // Documents
            "txt" | "md" | "pdf" | "docx" | "doc" | "rtf" | "odt" => FileCategory::Document,

            // Code
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "css" | "scss" | "json"
            | "xml" | "yaml" | "yml" | "toml" | "sql" | "sh" | "bash" | "c" | "cpp" | "h"
            | "hpp" | "java" | "go" | "rb" | "php" | "swift" | "kt" | "scala" | "r" | "m"
            | "mm" | "cs" | "vb" | "fs" | "clj" => FileCategory::Code,

            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tiff" | "tif" => {
                FileCategory::Image
            }

            // Video
            "mp4" | "mov" | "avi" | "mkv" | "webm" | "flv" | "wmv" | "m4v" => FileCategory::Video,

            // Audio
            "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" => FileCategory::Audio,

            // Archives
            "zip" | "tar" | "gz" | "bz2" | "7z" | "rar" | "xz" | "tgz" => FileCategory::Archive,

            // Data
            "csv" | "tsv" | "parquet" | "arrow" => FileCategory::Data,

            // Binary/Unknown
            _ => FileCategory::Binary,
        }
    }

    pub fn get_mime_type(path: &Path) -> Result<String> {
        // First try using infer library for magic number detection
        if let Ok(bytes) = std::fs::read(path) {
            if let Some(kind) = infer::get(&bytes) {
                return Ok(kind.mime_type().to_string());
            }
        }

        // Fallback to extension-based detection
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Ok(Self::get_mime_type_from_extension(&extension))
    }

    fn get_mime_type_from_extension(extension: &str) -> String {
        match extension {
            // Documents
            "txt" => "text/plain",
            "md" => "text/markdown",
            "pdf" => "application/pdf",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "doc" => "application/msword",
            "rtf" => "application/rtf",
            "odt" => "application/vnd.oasis.opendocument.text",

            // Code
            "rs" => "text/x-rust",
            "py" => "text/x-python",
            "js" => "text/javascript",
            "ts" => "text/typescript",
            "tsx" => "text/typescript",
            "jsx" => "text/javascript",
            "html" => "text/html",
            "css" => "text/css",
            "scss" => "text/x-scss",
            "json" => "application/json",
            "xml" => "application/xml",
            "yaml" | "yml" => "application/x-yaml",
            "toml" => "application/toml",
            "sql" => "text/x-sql",
            "sh" | "bash" => "text/x-shellscript",
            "c" => "text/x-c",
            "cpp" => "text/x-c++",
            "h" => "text/x-c",
            "hpp" => "text/x-c++",
            "java" => "text/x-java",
            "go" => "text/x-go",
            "rb" => "text/x-ruby",
            "php" => "text/x-php",
            "swift" => "text/x-swift",
            "kt" => "text/x-kotlin",
            "scala" => "text/x-scala",
            "r" => "text/x-r",
            "m" | "mm" => "text/x-objective-c",
            "cs" => "text/x-csharp",
            "vb" => "text/x-vb",
            "fs" => "text/x-fsharp",
            "clj" => "text/x-clojure",

            // Images
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "svg" => "image/svg+xml",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            "tiff" | "tif" => "image/tiff",

            // Video
            "mp4" => "video/mp4",
            "mov" => "video/quicktime",
            "avi" => "video/x-msvideo",
            "mkv" => "video/x-matroska",
            "webm" => "video/webm",
            "flv" => "video/x-flv",
            "wmv" => "video/x-ms-wmv",
            "m4v" => "video/x-m4v",

            // Audio
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "flac" => "audio/flac",
            "aac" => "audio/aac",
            "m4a" => "audio/mp4",
            "wma" => "audio/x-ms-wma",

            // Archives
            "zip" => "application/zip",
            "tar" => "application/x-tar",
            "gz" => "application/gzip",
            "bz2" => "application/x-bzip2",
            "7z" => "application/x-7z-compressed",
            "rar" => "application/vnd.rar",
            "xz" => "application/x-xz",
            "tgz" => "application/gzip",

            // Data
            "csv" => "text/csv",
            "tsv" => "text/tab-separated-values",

            // Default
            _ => "application/octet-stream",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_indexable() {
        assert!(FileTypeDetector::is_indexable("txt"));
        assert!(FileTypeDetector::is_indexable("rs"));
        assert!(FileTypeDetector::is_indexable("pdf"));
        assert!(FileTypeDetector::is_indexable("md"));
        assert!(FileTypeDetector::is_indexable("json"));

        assert!(!FileTypeDetector::is_indexable("jpg"));
        assert!(!FileTypeDetector::is_indexable("mp4"));
        assert!(!FileTypeDetector::is_indexable("zip"));
    }

    #[test]
    fn test_get_category() {
        assert_eq!(
            FileTypeDetector::get_category("txt"),
            FileCategory::Document
        );
        assert_eq!(FileTypeDetector::get_category("rs"), FileCategory::Code);
        assert_eq!(FileTypeDetector::get_category("png"), FileCategory::Image);
        assert_eq!(FileTypeDetector::get_category("mp4"), FileCategory::Video);
        assert_eq!(FileTypeDetector::get_category("mp3"), FileCategory::Audio);
        assert_eq!(FileTypeDetector::get_category("zip"), FileCategory::Archive);
        assert_eq!(FileTypeDetector::get_category("csv"), FileCategory::Data);
        assert_eq!(FileTypeDetector::get_category("xyz"), FileCategory::Binary);
    }

    #[test]
    fn test_get_mime_type_from_extension() {
        assert_eq!(
            FileTypeDetector::get_mime_type_from_extension("txt"),
            "text/plain"
        );
        assert_eq!(
            FileTypeDetector::get_mime_type_from_extension("pdf"),
            "application/pdf"
        );
        assert_eq!(
            FileTypeDetector::get_mime_type_from_extension("rs"),
            "text/x-rust"
        );
        assert_eq!(
            FileTypeDetector::get_mime_type_from_extension("json"),
            "application/json"
        );
    }

    #[test]
    fn test_detect() {
        let path = Path::new("test.txt");
        let info = FileTypeDetector::detect(path).unwrap();

        assert_eq!(info.extension, "txt");
        assert_eq!(info.mime_type, "text/plain");
        assert!(info.is_indexable);
        assert_eq!(info.category, FileCategory::Document);
    }

    #[test]
    fn test_detect_code_file() {
        let path = Path::new("main.rs");
        let info = FileTypeDetector::detect(path).unwrap();

        assert_eq!(info.extension, "rs");
        assert_eq!(info.mime_type, "text/x-rust");
        assert!(info.is_indexable);
        assert_eq!(info.category, FileCategory::Code);
    }

    #[test]
    fn test_detect_binary_file() {
        let path = Path::new("image.png");
        let info = FileTypeDetector::detect(path).unwrap();

        assert_eq!(info.extension, "png");
        assert_eq!(info.mime_type, "image/png");
        assert!(!info.is_indexable);
        assert_eq!(info.category, FileCategory::Image);
    }
}
