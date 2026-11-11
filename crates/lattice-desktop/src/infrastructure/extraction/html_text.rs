//! HTML to readable text, for every caller that has a page or a fragment of
//! one: indexing, tool output, image captions.

use super::markup::{decode_entities, tidy_lines, Markup, MarkupCursor};

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
pub fn html_to_text(html: &str) -> String {
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

    #[test]
    fn tags_and_script_bodies_are_dropped() {
        let html = "<html><body><h1>Title</h1><p>Content</p><script>ignore</script></body></html>";
        let text = html_to_text(html);

        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
        assert!(!text.contains("<h1>"));
        assert!(!text.contains("ignore"));
    }

    /// A multi-byte character before a tag neither shifts the lookahead nor
    /// slices a byte that is not a character boundary.
    #[test]
    fn non_ascii_content_is_stripped_without_panicking() {
        let html = "<p>日本語のテキスト🧪です</p><script>ignore()</script><p>Ωmega</p>";
        let text = html_to_text(html);
        assert!(text.contains("日本語のテキスト🧪です"));
        assert!(text.contains("Ωmega"));
        assert!(!text.contains("ignore"));
        assert!(!text.contains('<'));

        // Uppercase tags after multi-byte text still open and close the
        // script block: ASCII lowercasing keeps the offsets aligned.
        let shouted = "Ωμέγα<SCRIPT>ignore()</SCRIPT>tail";
        assert_eq!(html_to_text(shouted), "Ωμέγαtail");

        // 'İ' lowercases to two chars under full Unicode rules; the tag after
        // it must still be found at the right offset.
        assert_eq!(html_to_text("İ<b>x</b>"), "İx");
    }

    #[test]
    fn blocks_become_lines_and_headings_become_markdown() {
        assert_eq!(html_to_text("<p>one</p><p>two</p>"), "one\ntwo");
        assert_eq!(html_to_text("<h2>Title</h2><p>Body</p>"), "## Title\nBody");
        assert_eq!(
            html_to_text(
                "<ul><li>a</li><li>b</li></ul><table><tr><td>x</td><td>y</td></tr></table>"
            ),
            "a\nb\nx\ty"
        );
        assert_eq!(
            html_to_text("W<b>or</b>d<br>next  \n line"),
            "Word\nnext line"
        );
        assert_eq!(
            html_to_text(
                "<pre>keep\n  indent</pre><!-- gone --><p>it&#8217;s &#x2019; &amp;lt;</p>"
            ),
            "keep\n  indent\nit\u{2019}s \u{2019} &lt;"
        );
    }

    #[test]
    fn character_references_are_decoded_once() {
        assert_eq!(
            html_to_text("AT&amp;amp;T &amp;lt;b&amp;gt;"),
            "AT&amp;T &lt;b&gt;"
        );
    }
}
