//! The one rule that maps a document to a display type.
//!
//! Mirrors `websrc/components/FileBrowser/docMeta.ts::typeBucket` — same words,
//! same precedence. If you change one, change the other and both tests.

/// `source_type` is `"web"` or `"local"`; `file_type` is the bare extension, any case.
pub fn type_label(source_type: &str, file_type: &str) -> String {
    if source_type.eq_ignore_ascii_case("web") {
        return "Web".to_string();
    }
    match file_type.trim().to_ascii_lowercase().as_str() {
        "pdf" => "PDF".into(),
        "md" | "markdown" => "Markdown".into(),
        "txt" | "rtf" => "Text".into(),
        "doc" | "docx" => "Word".into(),
        "xls" | "xlsx" | "csv" => "Spreadsheet".into(),
        "html" | "htm" => "HTML".into(),
        "json" | "js" | "jsx" | "ts" | "tsx" | "py" | "rs" | "go" | "java" => "Code".into(),
        "" => "Other".into(),
        other => other.to_ascii_uppercase(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Must stay identical to `TYPE_BUCKETS` in
    /// `websrc/components/FileBrowser/docMeta.ts`. A divergence fails here.
    const TYPE_BUCKETS: [&str; 9] = [
        "PDF",
        "Markdown",
        "Text",
        "Word",
        "Spreadsheet",
        "HTML",
        "Code",
        "Web",
        "Other",
    ];

    #[test]
    fn maps_the_known_extensions() {
        assert_eq!(type_label("local", "pdf"), "PDF");
        assert_eq!(type_label("local", "PDF"), "PDF");
        assert_eq!(type_label("local", "md"), "Markdown");
        assert_eq!(type_label("local", "markdown"), "Markdown");
        assert_eq!(type_label("local", "txt"), "Text");
        assert_eq!(type_label("local", "rtf"), "Text");
        assert_eq!(type_label("local", "doc"), "Word");
        assert_eq!(type_label("local", "docx"), "Word");
        assert_eq!(type_label("local", "xls"), "Spreadsheet");
        assert_eq!(type_label("local", "xlsx"), "Spreadsheet");
        assert_eq!(type_label("local", "csv"), "Spreadsheet");
        assert_eq!(type_label("local", "html"), "HTML");
        assert_eq!(type_label("local", "htm"), "HTML");
        for ext in ["json", "js", "jsx", "ts", "tsx", "py", "rs", "go", "java"] {
            assert_eq!(type_label("local", ext), "Code", "extension {ext}");
        }
    }

    #[test]
    fn web_source_wins_over_any_extension() {
        assert_eq!(type_label("web", "pdf"), "Web");
        assert_eq!(type_label("WEB", "md"), "Web");
        assert_eq!(type_label("web", ""), "Web");
    }

    #[test]
    fn unknown_extension_uppercases() {
        assert_eq!(type_label("local", "epub"), "EPUB");
        assert_eq!(type_label("local", "  epub  "), "EPUB");
    }

    #[test]
    fn empty_extension_is_other() {
        assert_eq!(type_label("local", ""), "Other");
        assert_eq!(type_label("local", "   "), "Other");
    }

    #[test]
    fn every_known_label_is_a_declared_bucket() {
        let known = [
            "pdf", "md", "markdown", "txt", "rtf", "doc", "docx", "xls", "xlsx", "csv", "html",
            "htm", "json", "js", "jsx", "ts", "tsx", "py", "rs", "go", "java", "",
        ];
        for ext in known {
            let label = type_label("local", ext);
            assert!(
                TYPE_BUCKETS.contains(&label.as_str()),
                "label {label} (from {ext}) is not in TYPE_BUCKETS"
            );
        }
        assert!(TYPE_BUCKETS.contains(&type_label("web", "pdf").as_str()));
    }
}
