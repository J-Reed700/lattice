//! Stripping scraped documentation page chrome before storage, chunking,
//! and repair prompts while keeping fenced code blocks and genuine prose intact.
use super::segmentation::code_fences;

const EXACT_CHROME_LABELS: &[&str] = &[
    "view a markdown version of this page",
    "view markdown version of this page",
    "view this page as markdown",
    "view this page in markdown",
    "view page source as markdown",
];

const SHORT_MARKDOWN_CHROME_LABELS: &[&str] = &[
    "view as markdown",
    "view markdown",
    "copy page as markdown",
    "copy as markdown",
    "open in markdown",
];

fn parse_standalone_markdown_link(trimmed: &str) -> Option<(&str, &str)> {
    let after_open = trimmed.strip_prefix('[')?;
    let (label, after_sep) = after_open.split_once("](")?;
    if label.contains('[') || label.contains(']') || !after_sep.ends_with(')') {
        return None;
    }
    let mut depth = 1usize;
    for (idx, ch) in after_sep.char_indices() {
        if ch.is_whitespace() {
            return None;
        }
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let is_last = idx + ch.len_utf8() == after_sep.len();
                    let url = after_sep.get(..idx)?;
                    return (is_last && !url.is_empty()).then_some((label, url));
                }
            }
            _ => {}
        }
    }
    None
}

fn is_markdown_url(url: &str) -> bool {
    let base = url.split('#').next().unwrap_or(url);
    let (path, query) = base.split_once('?').unwrap_or((base, ""));
    let path_lower = path.trim().to_ascii_lowercase();
    if path_lower.ends_with(".md") || path_lower.ends_with(".mdx") {
        return true;
    }
    let query_lower = query.to_ascii_lowercase();
    query_lower
        .split('&')
        .any(|param| matches!(param, "format=md" | "format=markdown" | "output=md"))
}

fn is_markdown_mirror_chrome_line(trimmed: &str) -> bool {
    let Some((label, url)) = parse_standalone_markdown_link(trimmed) else {
        return false;
    };
    let normalized = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    EXACT_CHROME_LABELS.contains(&normalized.as_str())
        || (SHORT_MARKDOWN_CHROME_LABELS.contains(&normalized.as_str()) && is_markdown_url(url))
}

/// Removes standalone scraped markdown-mirror chrome lines (for example
/// `[View a markdown version of this page](...)`) outside fenced code blocks,
/// collapsing blank separator lines created around removed boilerplate while
/// returning non-boilerplate prose and fenced code untouched.
pub fn strip_scraped_chrome(text: &str) -> String {
    let fences = code_fences(text);
    let in_fence = |offset: usize| fences.iter().any(|fence| fence.whole().contains(&offset));
    let mut kept: Vec<&str> = Vec::new();
    let mut offset = 0usize;
    let mut just_dropped = false;
    let mut stripped_any = false;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let line_offset = offset;
        offset += line.len();
        if !in_fence(line_offset) && is_markdown_mirror_chrome_line(trimmed) {
            stripped_any = true;
            just_dropped = true;
            continue;
        }
        if just_dropped && trimmed.is_empty() {
            let at_start = kept.is_empty();
            let after_blank = kept.last().is_some_and(|prev| prev.trim().is_empty());
            if at_start || after_blank {
                continue;
            }
        }
        just_dropped = false;
        kept.push(line);
    }

    if !stripped_any {
        return text.to_owned();
    }
    if just_dropped {
        while kept.last().is_some_and(|prev| prev.trim().is_empty()) {
            kept.pop();
        }
    }
    let mut out = kept.concat();
    if text.ends_with('\n') {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
    } else if just_dropped && out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_aws_markdown_mirror_chrome_and_leading_blank_separator() {
        let raw = "[View a markdown version of this page](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-dead-letter-queues.md)\n\n# Using dead-letter queues\n\nBody text stays.";
        assert_eq!(
            strip_scraped_chrome(raw),
            "# Using dead-letter queues\n\nBody text stays."
        );
        let with_parens =
            "[View a markdown version of this page](https://example.com/a_(1).md)\n\n# Title";
        assert_eq!(strip_scraped_chrome(with_parens), "# Title");
    }

    #[test]
    fn strips_short_mirror_chrome_links_only_for_markdown_urls() {
        let raw = "Intro\n\n[View as Markdown](https://example.com/guide.md)\n\nNext paragraph";
        assert_eq!(strip_scraped_chrome(raw), "Intro\n\nNext paragraph");

        let real = "[View as Markdown](https://example.com/rendering-tutorial)\n\nBody";
        assert_eq!(strip_scraped_chrome(real), real);
    }

    #[test]
    fn preserves_chrome_inside_code_fences_and_inline_links_in_prose() {
        let raw = concat!(
            "For details, see [View a markdown version of this page](https://docs.aws.amazon.com/foo.md) in the header.\n\n",
            "```markdown\n",
            "[View a markdown version of this page](https://docs.aws.amazon.com/foo.md)\n",
            "```\n\n",
            "~~~\n",
            "[View as Markdown](https://example.com/example.md)\n",
            "~~~\n\n",
            "[Read the AWS SQS guide](https://docs.aws.amazon.com/sqs.md)"
        );
        assert_eq!(strip_scraped_chrome(raw), raw);
    }

    #[test]
    fn preserves_multi_link_lines_trailing_commentary_and_intentional_trailing_newlines() {
        let multi = "[View as Markdown](https://a.md) | [Guide](https://b.md)\n\nBody";
        assert_eq!(strip_scraped_chrome(multi), multi);

        let exact_multi =
            "[View a markdown version of this page](https://a.md) | [Guide](https://b.md)";
        assert_eq!(strip_scraped_chrome(exact_multi), exact_multi);

        let annotated = "[View a markdown version of this page](https://a.md) (notes)\n\nBody";
        assert_eq!(strip_scraped_chrome(annotated), annotated);

        let top_with_eof_blanks =
            "[View a markdown version of this page](https://a.md)\n\nBody\n\n";
        assert_eq!(strip_scraped_chrome(top_with_eof_blanks), "Body\n\n");

        let trailing_without_eof_nl =
            "Body\n[View a markdown version of this page](https://a.md)";
        assert_eq!(strip_scraped_chrome(trailing_without_eof_nl), "Body");

        let trailing_sep_without_eof_nl =
            "Body\n\n[View a markdown version of this page](https://a.md)";
        assert_eq!(strip_scraped_chrome(trailing_sep_without_eof_nl), "Body");

        let trailing_crlf_without_eof_nl =
            "Body\r\n[View a markdown version of this page](https://a.md)";
        assert_eq!(strip_scraped_chrome(trailing_crlf_without_eof_nl), "Body");
    }
}
