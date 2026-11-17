//! The type bucket a library document counts in, for the Library's facets and the corpus shape.

/// `web` is true for a saved web article; `file_type` is the bare extension, any case.
pub fn type_bucket(web: bool, file_type: &str) -> String {
    if web {
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
mod tests {
    use super::*;

    #[test]
    fn maps_the_known_extensions() {
        assert_eq!(type_bucket(false, "pdf"), "PDF");
        assert_eq!(type_bucket(false, "PDF"), "PDF");
        assert_eq!(type_bucket(false, "md"), "Markdown");
        assert_eq!(type_bucket(false, "markdown"), "Markdown");
        assert_eq!(type_bucket(false, "txt"), "Text");
        assert_eq!(type_bucket(false, "rtf"), "Text");
        assert_eq!(type_bucket(false, "doc"), "Word");
        assert_eq!(type_bucket(false, "docx"), "Word");
        assert_eq!(type_bucket(false, "xls"), "Spreadsheet");
        assert_eq!(type_bucket(false, "xlsx"), "Spreadsheet");
        assert_eq!(type_bucket(false, "csv"), "Spreadsheet");
        assert_eq!(type_bucket(false, "html"), "HTML");
        assert_eq!(type_bucket(false, "htm"), "HTML");
        for ext in ["json", "js", "jsx", "ts", "tsx", "py", "rs", "go", "java"] {
            assert_eq!(type_bucket(false, ext), "Code", "extension {ext}");
        }
    }

    #[test]
    fn web_wins_over_any_file_type() {
        assert_eq!(type_bucket(true, "pdf"), "Web");
        assert_eq!(type_bucket(true, "md"), "Web");
        assert_eq!(type_bucket(true, ""), "Web");
    }

    #[test]
    fn unknown_extension_uppercases() {
        assert_eq!(type_bucket(false, "epub"), "EPUB");
        assert_eq!(type_bucket(false, "  epub  "), "EPUB");
    }

    #[test]
    fn empty_extension_is_other() {
        assert_eq!(type_bucket(false, ""), "Other");
        assert_eq!(type_bucket(false, "   "), "Other");
    }
}
