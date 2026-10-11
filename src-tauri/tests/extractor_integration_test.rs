//! Exercise the public extraction boundary against actual files and streams.
//! No model, external endpoint, or application library is required.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lattice::application::ports::content_extraction_port::ContentExtractionPort;
use lattice::features::indexing::engine::extraction::ContentExtractor;
use lattice::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
use lattice::shared::error::AppError;
use std::path::Path;

#[tokio::test]
async fn extracts_unicode_text_from_a_path_with_spaces_without_mutating_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Notes café 日本語.txt");
    let text = "Café 日本語 🦀\nSecond line\r\n";
    std::fs::write(&path, text).unwrap();
    let extracted = ContentExtractionAdapter::new()
        .extract_content(&path)
        .await
        .unwrap();
    assert_eq!(extracted.text, text);
    assert_eq!(extracted.word_count, 5);
    assert_eq!(extracted.char_count, text.chars().count());
    assert_eq!(extracted.mime_type, "text/plain");
    assert_eq!(extracted.page_count, None);
    assert!(extracted.page_ranges.is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}

#[tokio::test]
async fn empty_files_are_valid_content_with_zero_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, b"").unwrap();
    let extracted = ContentExtractor::with_max_size(0)
        .extract_from_file(&path)
        .await
        .unwrap();
    assert!(extracted.text.is_empty());
    assert_eq!(extracted.metadata.word_count, 0);
    assert_eq!(extracted.metadata.char_count, 0);
}

#[tokio::test]
async fn markdown_aliases_and_case_keep_their_mime_type() {
    let dir = tempfile::tempdir().unwrap();
    for extension in ["md", "MD", "markdown", "MARKDOWN"] {
        let path = dir.path().join(format!("note.{extension}"));
        std::fs::write(&path, "# Heading\n\nA **fact**.").unwrap();
        let content = ContentExtractor::new()
            .extract_from_file(&path)
            .await
            .unwrap();
        assert_eq!(content.mime_type, "text/markdown", "{extension}");
        assert!(content.text.contains("**fact**"));
    }
}

#[tokio::test]
async fn file_size_limit_is_inclusive_and_counts_bytes_not_characters() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("boundary.txt");
    std::fs::write(&path, "🦀🦀").unwrap();
    let extractor = ContentExtractor::with_max_size(8);
    assert_eq!(
        extractor
            .extract_from_file(&path)
            .await
            .unwrap()
            .metadata
            .char_count,
        2
    );
    std::fs::write(&path, "🦀🦀x").unwrap();
    assert!(matches!(
        extractor.extract_from_file(&path).await,
        Err(AppError::FileTooLarge {
            size_bytes: 9,
            max_size_bytes: 8,
            ..
        })
    ));
    std::fs::write(&path, "ok").unwrap();
    assert_eq!(extractor.extract_from_file(&path).await.unwrap().text, "ok");
}

#[tokio::test]
async fn rejects_executables_archives_unknown_types_and_missing_extensions() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "program.exe",
        "library.dll",
        "archive.zip",
        "archive.tar.gz",
        "unknown.xyz",
        "README",
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"not supported").unwrap();
        assert!(
            matches!(
                ContentExtractor::new().extract_from_file(&path).await,
                Err(AppError::UnsupportedFileType { .. })
            ),
            "{name}"
        );
    }
}

#[tokio::test]
async fn missing_files_and_directories_return_errors_instead_of_empty_successes() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.txt");
    assert!(matches!(
        ContentExtractor::new().extract_from_file(&missing).await,
        Err(AppError::Io { .. })
    ));
    let directory = dir.path().join("directory.txt");
    std::fs::create_dir(&directory).unwrap();
    assert!(ContentExtractor::new()
        .extract_from_file(&directory)
        .await
        .is_err());
}

#[tokio::test]
async fn corrupt_document_archives_and_pdf_fail_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "broken.pdf",
        "broken.docx",
        "broken.xlsx",
        "broken.pptx",
        "broken.odt",
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"this is not a document archive").unwrap();
        assert!(
            ContentExtractor::new()
                .extract_from_file(&path)
                .await
                .is_err(),
            "{name}"
        );
    }
}

#[test]
fn supported_extensions_match_the_public_predicate_in_both_cases() {
    let extractor = ContentExtractor::default();
    for extension in ContentExtractor::supported_extensions() {
        assert!(extractor.is_supported(Path::new(&format!("document.{extension}"))));
        assert!(
            extractor.is_supported(Path::new(&format!("document.{}", extension.to_uppercase())))
        );
    }
    assert!(!extractor.is_supported(Path::new("document.exe")));
    assert!(!extractor.is_supported(Path::new("README")));
}

#[tokio::test]
async fn streaming_preserves_unicode_blank_lines_crlf_and_an_unterminated_final_line() {
    for mime in ["text/plain", "text/markdown"] {
        let input = "first\r\n\r\ncafé 🦀\nlast".as_bytes();
        let lines = ContentExtractor::extract_text_streaming(input, mime)
            .await
            .unwrap();
        assert_eq!(lines, ["first", "", "café 🦀", "last"]);
    }
    assert!(
        ContentExtractor::extract_text_streaming(&b""[..], "text/plain")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn streaming_rejects_unsupported_mime_and_invalid_utf8() {
    assert!(matches!(
        ContentExtractor::extract_text_streaming(&b"text"[..], "application/pdf").await,
        Err(AppError::UnsupportedFileType { .. })
    ));
    assert!(matches!(
        ContentExtractor::extract_text_streaming(&b"valid\n\xff"[..], "text/plain").await,
        Err(AppError::FileRead { .. })
    ));
}
