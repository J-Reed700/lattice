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
    let sep = after_open.find("](")?;
    let label = &after_open[..sep];
    if label.contains('[') || label.contains(']') {
        return None;
    }
    let after_sep = &after_open[sep + 2..];
    let url = after_sep.strip_suffix(')')?;
    if url.trim().is_empty() {
        return None;
    }
    Some((label, url))
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
    while kept.last().is_some_and(|prev| prev.trim().is_empty()) {
        kept.pop();
    }
    let mut out = kept.concat();
    if text.ends_with('\n') && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
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
}
