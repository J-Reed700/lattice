//! HTML and code file extraction.

use super::markup::{decode_entities, decode_text_bytes, tidy_lines, Markup, MarkupCursor};
use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::path::Path;

/// Extract content from code files (including HTML).
pub async fn extract_code_file(
    path: &Path,
    mime_type: &str,
    max_file_size: u64,
) -> Result<ExtractedContent> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let file_size = metadata.len();

    if file_size > max_file_size {
        return Err(IndexingError::FileTooLarge {
            path: path.display().to_string(),
            size_bytes: file_size,
            max_size_bytes: max_file_size,
        });
    }

    let bytes = tokio::fs::read(path).await.map_err(|e| IndexingError::Io {
        message: e.to_string(),
        kind: format!("{:?}", e.kind()),
    })?;
    let text = decode_text_bytes(bytes, path);

    let processed_text = if mime_type == "text/html" {
        strip_html_tags(&text)
    } else {
        text
    };

    let metadata = ContentMetadata {
        page_count: None,
        word_count: processed_text.split_whitespace().count(),
        char_count: processed_text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text: processed_text,
        mime_type: mime_type.to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

/// Elements that start and end a line of their own.
const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "caption",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "head",
    "header",
    "hr",
    "html",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "section",
    "summary",
    "table",
    "tbody",
    "tfoot",
    "thead",
    "title",
    "tr",
    "ul",
];

/// Phrasing elements that sit inside a word as often as between words
/// (`<b>W</b>ord`), so they add no separator.
const INLINE_TAGS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "font", "i", "ins",
    "kbd", "label", "mark", "q", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u",
    "var", "wbr",
];

/// Strip HTML to readable text: block elements become lines, table cells are
/// tab-separated, `h1`–`h6` become markdown `#` headings so a saved page forms
/// sections, whitespace collapses outside `<pre>`, and named and numeric
/// character references are decoded.
pub fn strip_html_tags(html: &str) -> String {
    let mut out = HtmlText::default();
    let mut cursor = MarkupCursor::new(html);
    while let Some(event) = cursor.next() {
        match event {
            Markup::Open { name, empty, .. } => {
                let tag = name.to_ascii_lowercase();
                match tag.as_str() {
                    "script" | "style" | "noscript" | "template" | "svg" => {
                        if !empty {
                            cursor.skip_raw_text(&tag);
                        }
                    }
                    "pre" => {
                        out.block();
                        out.pre += usize::from(!empty);
                    }
                    "br" => out.line_break(),
                    "td" | "th" => out.cell(),
                    t => match heading_level(t) {
                        Some(level) => {
                            out.block();
                            out.raw(&"#".repeat(level));
                            out.raw(" ");
                        }
                        None if BLOCK_TAGS.contains(&t) => out.block(),
                        None if INLINE_TAGS.contains(&t) => {}
                        None => out.space(),
                    },
                }
            }
            Markup::Close { name } => {
                let tag = name.to_ascii_lowercase();
                match tag.as_str() {
                    "pre" => {
                        out.pre = out.pre.saturating_sub(1);
                        out.block();
                    }
                    "td" | "th" => out.space(),
                    t if heading_level(t).is_some() || BLOCK_TAGS.contains(&t) => out.block(),
                    t if INLINE_TAGS.contains(&t) => {}
                    _ => out.space(),
                }
            }
            Markup::Text(text) => out.text(&decode_entities(text)),
        }
    }
    tidy_lines(&out.text)
}

fn heading_level(tag: &str) -> Option<usize> {
    let level = tag.strip_prefix('h')?.parse::<usize>().ok()?;
    (1..=6).contains(&level).then_some(level)
}

#[derive(Default)]
struct HtmlText {
    text: String,
    pending_space: bool,
    pre: usize,
}

impl HtmlText {
    fn text(&mut self, s: &str) {
        if self.pre > 0 {
            self.pending_space = false;
            self.text.push_str(s);
            return;
        }
        for c in s.chars() {
            if c.is_whitespace() {
                self.pending_space = true;
                continue;
            }
            if std::mem::take(&mut self.pending_space)
                && !self.text.is_empty()
                && !self.text.ends_with(['\n', '\t', ' '])
            {
                self.text.push(' ');
            }
            self.text.push(c);
        }
    }

    fn raw(&mut self, s: &str) {
        self.pending_space = false;
        self.text.push_str(s);
    }

    fn space(&mut self) {
        self.pending_space = true;
    }

    fn trim_trailing_spaces(&mut self) {
        let kept = self.text.trim_end_matches([' ', '\t']).len();
        self.text.truncate(kept);
        self.pending_space = false;
    }

    fn block(&mut self) {
        self.trim_trailing_spaces();
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text.push('\n');
        }
    }

    fn line_break(&mut self) {
        self.trim_trailing_spaces();
        self.text.push('\n');
    }

    fn cell(&mut self) {
        self.trim_trailing_spaces();
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text.push('\t');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_code_file_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "fn main() {{").unwrap();
        writeln!(temp_file, "    println!(\"Hello, world!\");").unwrap();
        writeln!(temp_file, "}}").unwrap();

        let path = temp_file.path().with_extension("rs");
        std::fs::copy(temp_file.path(), &path).unwrap();

        let result = extract_code_file(&path, "text/x-rust", 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("fn main"));
        assert!(content.text.contains("println"));
        assert_eq!(content.mime_type, "text/x-rust");

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_html_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "<html><head><title>Test</title></head>").unwrap();
        writeln!(temp_file, "<body><h1>Hello World</h1>").unwrap();
        writeln!(temp_file, "<p>This is a test.</p>").unwrap();
        writeln!(temp_file, "<script>console.log('ignore');</script>").unwrap();
        writeln!(temp_file, "</body></html>").unwrap();

        let path = temp_file.path().with_extension("html");
        std::fs::copy(temp_file.path(), &path).unwrap();

        let result = extract_code_file(&path, "text/html", 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("Test"));
        assert!(content.text.contains("Hello World"));
        assert!(content.text.contains("This is a test"));
        assert!(!content.text.contains("<html>"));
        assert!(!content.text.contains("console.log"));
        assert_eq!(content.mime_type, "text/html");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_html_tag_stripping() {
        let html = "<html><body><h1>Title</h1><p>Content</p><script>ignore</script></body></html>";
        let text = strip_html_tags(html);

        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
        assert!(!text.contains("<h1>"));
        assert!(!text.contains("ignore"));
    }

    /// The scan used to index a lowercased copy with a *char* counter, so any
    /// multi-byte character before a tag either shifted the lookahead or
    /// panicked on a byte that is not a character boundary.
    #[test]
    fn non_ascii_content_is_stripped_without_panicking() {
        let html = "<p>日本語のテキスト🧪です</p><script>ignore()</script><p>Ωmega</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("日本語のテキスト🧪です"));
        assert!(text.contains("Ωmega"));
        assert!(!text.contains("ignore"));
        assert!(!text.contains('<'));

        // Uppercase tags after multi-byte text still open and close the
        // script block: ASCII lowercasing keeps the offsets aligned.
        let shouted = "Ωμέγα<SCRIPT>ignore()</SCRIPT>tail";
        assert_eq!(strip_html_tags(shouted), "Ωμέγαtail");

        // 'İ' lowercases to two chars under full Unicode rules; the tag after
        // it must still be found at the right offset.
        assert_eq!(strip_html_tags("İ<b>x</b>"), "İx");
    }

    #[test]
    fn blocks_become_lines_and_headings_become_markdown() {
        assert_eq!(strip_html_tags("<p>one</p><p>two</p>"), "one\ntwo");
        assert_eq!(
            strip_html_tags("<h2>Title</h2><p>Body</p>"),
            "## Title\nBody"
        );
        assert_eq!(
            strip_html_tags(
                "<ul><li>a</li><li>b</li></ul><table><tr><td>x</td><td>y</td></tr></table>"
            ),
            "a\nb\nx\ty"
        );
        assert_eq!(
            strip_html_tags("W<b>or</b>d<br>next  \n line"),
            "Word\nnext line"
        );
        assert_eq!(
            strip_html_tags(
                "<pre>keep\n  indent</pre><!-- gone --><p>it&#8217;s &#x2019; &amp;lt;</p>"
            ),
            "keep\n  indent\nit\u{2019}s \u{2019} &lt;"
        );
    }

    #[tokio::test]
    async fn a_windows_1252_file_is_read_instead_of_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.html");
        std::fs::write(&path, b"<p>caf\xE9</p>").unwrap();
        let content = extract_code_file(&path, "text/html", 1 << 20)
            .await
            .unwrap();
        assert_eq!(content.text, "caf\u{e9}");
    }
}
