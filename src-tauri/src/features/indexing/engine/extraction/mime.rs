//! MIME type detection for file types.

use crate::features::indexing::engine::error::{IndexingError, Result};
use std::path::Path;

/// Detect MIME type from file extension.
pub fn detect_mime_type(path: &Path) -> Result<String> {
    let extension = path.extension().and_then(|s| s.to_str()).ok_or_else(|| {
        IndexingError::UnsupportedFileType {
            path: path.display().to_string(),
            detected_type: "unknown".to_string(),
        }
    })?;

    let mime_type = match extension.to_lowercase().as_str() {
        // Documents
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "rtf" => "application/rtf",
        "odt" => "application/vnd.oasis.opendocument.text",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",

        // Code - Rust
        "rs" => "text/x-rust",
        "toml" => "text/x-toml",

        // Code - Web
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" => "text/typescript",
        "tsx" => "text/typescript-jsx",
        "jsx" => "text/javascript-jsx",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "scss" | "sass" => "text/x-scss",
        "less" => "text/x-less",

        // Code - Python
        "py" | "pyw" | "pyi" => "text/x-python",

        // Code - Systems
        "c" => "text/x-c",
        "h" => "text/x-c-header",
        "cpp" | "cxx" | "cc" => "text/x-c++",
        "hpp" | "hxx" | "hh" => "text/x-c++-header",
        "go" => "text/x-go",

        // Code - JVM
        "java" => "text/x-java",
        "kt" | "kts" => "text/x-kotlin",
        "scala" => "text/x-scala",
        "clj" | "cljs" | "cljc" => "text/x-clojure",

        // Code - Other languages
        "rb" | "rake" => "text/x-ruby",
        "php" => "text/x-php",
        "swift" => "text/x-swift",
        "r" => "text/x-r",
        "m" => "text/x-matlab",
        "ex" | "exs" => "text/x-elixir",
        "erl" | "hrl" => "text/x-erlang",

        // Shell scripts
        "sh" => "text/x-shellscript",
        "bash" => "text/x-bash",
        "zsh" => "text/x-zsh",
        "fish" => "text/x-fish",
        "ps1" | "psm1" => "text/x-powershell",
        "bat" | "cmd" => "text/x-batch",

        // Config/Data formats
        "json" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "text/x-yaml",
        "ini" => "text/x-ini",
        "conf" | "config" => "text/plain",

        // Database
        "sql" => "text/x-sql",
        "graphql" | "gql" => "application/graphql",

        // Data files
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",

        _ => {
            return Err(IndexingError::UnsupportedFileType {
                path: path.display().to_string(),
                detected_type: format!("unknown/{}", extension),
            })
        }
    };

    Ok(mime_type.to_string())
}

/// List of all supported file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        // Documents
        "txt", "md", "markdown", "pdf", "docx", "rtf", "odt", "xlsx", "pptx", // Code - Rust
        "rs", "toml", // Code - Web
        "js", "mjs", "cjs", "ts", "tsx", "jsx", "html", "htm", "css", "scss", "sass", "less",
        // Code - Python
        "py", "pyw", "pyi", // Code - Systems
        "c", "h", "cpp", "cxx", "cc", "hpp", "hxx", "hh", "go", // Code - JVM
        "java", "kt", "kts", "scala", "clj", "cljs", "cljc", // Code - Other
        "rb", "rake", "php", "swift", "r", "m", "ex", "exs", "erl", "hrl", // Shell
        "sh", "bash", "zsh", "fish", "ps1", "psm1", "bat", "cmd", // Config/Data
        "json", "xml", "yaml", "yml", "ini", "conf", "config", // Database
        "sql", "graphql", "gql", // Data files
        "csv", "tsv", // Audio — transcribed on-device by the transcription slice.
        // Listed here because `BatchFileImportService` calls `ContentExtractor::is_supported`
        // directly rather than through `ContentExtractionPort`, and would otherwise
        // fail the whole batch fast. Extraction itself happens in
        // `ContentExtractionAdapter`, not in `ContentExtractor`.
        "mp3", "wav", "ogg", "flac", "aac", "m4a", "wma",
    ]
}

/// Check if a file extension is supported.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| supported_extensions().contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_detection() {
        assert_eq!(
            detect_mime_type(Path::new("test.txt")).unwrap(),
            "text/plain"
        );
        assert_eq!(
            detect_mime_type(Path::new("test.md")).unwrap(),
            "text/markdown"
        );
        assert_eq!(
            detect_mime_type(Path::new("test.pdf")).unwrap(),
            "application/pdf"
        );
    }

    #[test]
    fn test_is_supported() {
        assert!(is_supported(Path::new("test.txt")));
        assert!(is_supported(Path::new("test.md")));
        assert!(is_supported(Path::new("test.pdf")));
        assert!(is_supported(Path::new("test.docx")));

        assert!(!is_supported(Path::new("test.exe")));
        assert!(!is_supported(Path::new("test.jpg")));
    }
}
