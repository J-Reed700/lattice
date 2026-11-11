//! # File Type Catalogue
//!
//! The one table that says what an extension is: its MIME type, its category
//! and, when the indexer can read it, the route its text is extracted by.
//! A format is indexable exactly when it has a route.

use serde::Serialize;
use std::path::Path;

/// MIME type for a file the catalogue does not know.
pub const UNKNOWN_MIME_TYPE: &str = "application/octet-stream";

/// Broad kind of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
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

/// How the indexer turns a file into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRoute {
    PlainText,
    Pdf,
    Docx,
    Rtf,
    Odt,
    Xlsx,
    Pptx,
    Csv,
    Html,
    Code,
    /// Transcribed on-device rather than parsed.
    Audio,
}

/// One extension of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileFormat {
    pub extension: &'static str,
    pub mime_type: &'static str,
    pub category: FileCategory,
    pub route: Option<ExtractionRoute>,
}

impl FileFormat {
    const fn new(
        extension: &'static str,
        mime_type: &'static str,
        category: FileCategory,
        route: Option<ExtractionRoute>,
    ) -> Self {
        Self {
            extension,
            mime_type,
            category,
            route,
        }
    }

    /// Every extension the catalogue knows.
    pub fn all() -> &'static [FileFormat] {
        CATALOGUE
    }

    /// The format of `extension` (no dot, any case), if the catalogue knows it.
    pub fn from_extension(extension: &str) -> Option<&'static FileFormat> {
        CATALOGUE
            .iter()
            .find(|format| format.extension.eq_ignore_ascii_case(extension))
    }

    /// The format of a path's final extension.
    pub fn from_path(path: &Path) -> Option<&'static FileFormat> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(Self::from_extension)
    }

    /// The MIME type of a path, [`UNKNOWN_MIME_TYPE`] when the extension is unknown.
    pub fn mime_type_of(path: &Path) -> &'static str {
        Self::from_path(path).map_or(UNKNOWN_MIME_TYPE, |format| format.mime_type)
    }

    /// Whether a path names an audio file, which is transcribed rather than parsed.
    pub fn is_audio_path(path: &Path) -> bool {
        Self::from_path(path).is_some_and(|format| format.category == FileCategory::Audio)
    }

    /// Whether the indexer can extract text from this format.
    pub fn is_indexable(&self) -> bool {
        self.route.is_some()
    }
}

#[rustfmt::skip]
static CATALOGUE: &[FileFormat] = &[
    FileFormat::new("txt", "text/plain", FileCategory::Document, Some(ExtractionRoute::PlainText)),
    FileFormat::new("md", "text/markdown", FileCategory::Document, Some(ExtractionRoute::PlainText)),
    FileFormat::new("markdown", "text/markdown", FileCategory::Document, Some(ExtractionRoute::PlainText)),
    FileFormat::new("pdf", "application/pdf", FileCategory::Document, Some(ExtractionRoute::Pdf)),
    FileFormat::new("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", FileCategory::Document, Some(ExtractionRoute::Docx)),
    FileFormat::new("doc", "application/msword", FileCategory::Document, None),
    FileFormat::new("rtf", "application/rtf", FileCategory::Document, Some(ExtractionRoute::Rtf)),
    FileFormat::new("odt", "application/vnd.oasis.opendocument.text", FileCategory::Document, Some(ExtractionRoute::Odt)),
    FileFormat::new("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", FileCategory::Document, Some(ExtractionRoute::Xlsx)),
    FileFormat::new("xls", "application/vnd.ms-excel", FileCategory::Document, None),
    FileFormat::new("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation", FileCategory::Document, Some(ExtractionRoute::Pptx)),
    FileFormat::new("ppt", "application/vnd.ms-powerpoint", FileCategory::Document, None),
    FileFormat::new("rs", "text/x-rust", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("toml", "text/x-toml", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("js", "text/javascript", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("mjs", "text/javascript", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cjs", "text/javascript", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("ts", "text/typescript", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("tsx", "text/typescript-jsx", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("jsx", "text/javascript-jsx", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("html", "text/html", FileCategory::Document, Some(ExtractionRoute::Html)),
    FileFormat::new("htm", "text/html", FileCategory::Document, Some(ExtractionRoute::Html)),
    FileFormat::new("css", "text/css", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("scss", "text/x-scss", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("sass", "text/x-scss", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("less", "text/x-less", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("py", "text/x-python", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("pyw", "text/x-python", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("pyi", "text/x-python", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("c", "text/x-c", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("h", "text/x-c-header", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cpp", "text/x-c++", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cxx", "text/x-c++", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cc", "text/x-c++", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("hpp", "text/x-c++-header", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("hxx", "text/x-c++-header", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("hh", "text/x-c++-header", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("go", "text/x-go", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("java", "text/x-java", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("kt", "text/x-kotlin", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("kts", "text/x-kotlin", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("scala", "text/x-scala", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("clj", "text/x-clojure", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cljs", "text/x-clojure", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cljc", "text/x-clojure", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("rb", "text/x-ruby", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("rake", "text/x-ruby", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("php", "text/x-php", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("swift", "text/x-swift", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("r", "text/x-r", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("m", "text/x-matlab", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("mm", "text/x-objective-c", FileCategory::Code, None),
    FileFormat::new("pl", "text/x-perl", FileCategory::Code, None),
    FileFormat::new("pm", "text/x-perl", FileCategory::Code, None),
    FileFormat::new("lua", "text/x-lua", FileCategory::Code, None),
    FileFormat::new("hs", "text/x-haskell", FileCategory::Code, None),
    FileFormat::new("lhs", "text/x-haskell", FileCategory::Code, None),
    FileFormat::new("cs", "text/x-csharp", FileCategory::Code, None),
    FileFormat::new("vb", "text/x-vb", FileCategory::Code, None),
    FileFormat::new("fs", "text/x-fsharp", FileCategory::Code, None),
    FileFormat::new("ex", "text/x-elixir", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("exs", "text/x-elixir", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("erl", "text/x-erlang", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("hrl", "text/x-erlang", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("sh", "text/x-shellscript", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("bash", "text/x-bash", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("zsh", "text/x-zsh", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("fish", "text/x-fish", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("ps1", "text/x-powershell", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("psm1", "text/x-powershell", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("bat", "text/x-batch", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("cmd", "text/x-batch", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("json", "application/json", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("xml", "application/xml", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("yaml", "text/x-yaml", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("yml", "text/x-yaml", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("ini", "text/x-ini", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("conf", "text/plain", FileCategory::Code, Some(ExtractionRoute::PlainText)),
    FileFormat::new("config", "text/plain", FileCategory::Code, Some(ExtractionRoute::PlainText)),
    FileFormat::new("sql", "text/x-sql", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("graphql", "application/graphql", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("gql", "application/graphql", FileCategory::Code, Some(ExtractionRoute::Code)),
    FileFormat::new("csv", "text/csv", FileCategory::Data, Some(ExtractionRoute::Csv)),
    FileFormat::new("tsv", "text/tab-separated-values", FileCategory::Data, Some(ExtractionRoute::Csv)),
    FileFormat::new("parquet", "application/octet-stream", FileCategory::Data, None),
    FileFormat::new("arrow", "application/octet-stream", FileCategory::Data, None),
    FileFormat::new("jpg", "image/jpeg", FileCategory::Image, None),
    FileFormat::new("jpeg", "image/jpeg", FileCategory::Image, None),
    FileFormat::new("png", "image/png", FileCategory::Image, None),
    FileFormat::new("gif", "image/gif", FileCategory::Image, None),
    FileFormat::new("bmp", "image/bmp", FileCategory::Image, None),
    FileFormat::new("svg", "image/svg+xml", FileCategory::Image, None),
    FileFormat::new("webp", "image/webp", FileCategory::Image, None),
    FileFormat::new("ico", "image/x-icon", FileCategory::Image, None),
    FileFormat::new("tiff", "image/tiff", FileCategory::Image, None),
    FileFormat::new("tif", "image/tiff", FileCategory::Image, None),
    FileFormat::new("heic", "image/heic", FileCategory::Image, None),
    FileFormat::new("heif", "image/heif", FileCategory::Image, None),
    FileFormat::new("mp4", "video/mp4", FileCategory::Video, None),
    FileFormat::new("mov", "video/quicktime", FileCategory::Video, None),
    FileFormat::new("avi", "video/x-msvideo", FileCategory::Video, None),
    FileFormat::new("mkv", "video/x-matroska", FileCategory::Video, None),
    FileFormat::new("webm", "video/webm", FileCategory::Video, None),
    FileFormat::new("flv", "video/x-flv", FileCategory::Video, None),
    FileFormat::new("wmv", "video/x-ms-wmv", FileCategory::Video, None),
    FileFormat::new("m4v", "video/x-m4v", FileCategory::Video, None),
    FileFormat::new("mp3", "audio/mpeg", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("wav", "audio/wav", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("ogg", "audio/ogg", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("flac", "audio/flac", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("aac", "audio/aac", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("m4a", "audio/mp4", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("wma", "audio/x-ms-wma", FileCategory::Audio, Some(ExtractionRoute::Audio)),
    FileFormat::new("zip", "application/zip", FileCategory::Archive, None),
    FileFormat::new("tar", "application/x-tar", FileCategory::Archive, None),
    FileFormat::new("gz", "application/gzip", FileCategory::Archive, None),
    FileFormat::new("tgz", "application/gzip", FileCategory::Archive, None),
    FileFormat::new("bz2", "application/x-bzip2", FileCategory::Archive, None),
    FileFormat::new("7z", "application/x-7z-compressed", FileCategory::Archive, None),
    FileFormat::new("rar", "application/vnd.rar", FileCategory::Archive, None),
    FileFormat::new("xz", "application/x-xz", FileCategory::Archive, None),
    FileFormat::new("exe", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("dll", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("dylib", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("so", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("bin", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("msi", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("app", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("apk", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("deb", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("rpm", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("pkg", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("dmg", "application/octet-stream", FileCategory::Binary, None),
    FileFormat::new("iso", "application/octet-stream", FileCategory::Binary, None),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn each_extension_appears_once_in_lower_case() {
        let mut seen = HashSet::new();
        for format in FileFormat::all() {
            assert_eq!(format.extension, format.extension.to_ascii_lowercase());
            assert!(seen.insert(format.extension), "{} twice", format.extension);
        }
    }

    #[test]
    fn lookup_ignores_case_and_uses_the_last_extension() {
        let format = FileFormat::from_path(Path::new("/a/b/Report.PDF"));
        assert_eq!(format.map(|f| f.mime_type), Some("application/pdf"));
        assert_eq!(
            FileFormat::from_path(Path::new("archive.tar.gz")).map(|f| f.extension),
            Some("gz")
        );
    }

    #[test]
    fn an_unknown_or_missing_extension_has_no_format() {
        assert!(FileFormat::from_path(Path::new("notes.unknown")).is_none());
        assert!(FileFormat::from_path(Path::new("README")).is_none());
        assert_eq!(
            FileFormat::mime_type_of(Path::new("README")),
            UNKNOWN_MIME_TYPE
        );
    }

    #[test]
    fn a_format_is_indexable_exactly_when_it_has_a_route() {
        let lua = FileFormat::from_extension("lua");
        assert_eq!(lua.map(|f| f.mime_type), Some("text/x-lua"));
        assert_eq!(lua.map(FileFormat::is_indexable), Some(false));
        assert_eq!(
            FileFormat::from_extension("rs").map(FileFormat::is_indexable),
            Some(true)
        );
    }

    #[test]
    fn every_audio_extension_is_routed_to_transcription() {
        for format in FileFormat::all() {
            assert_eq!(
                format.category == FileCategory::Audio,
                format.route == Some(ExtractionRoute::Audio),
                "{}",
                format.extension
            );
        }
    }
}
