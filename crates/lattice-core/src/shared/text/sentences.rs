//! Markdown read as plain text, and a claim's sentence in its paragraph: what
//! the claim review shows of a claim and what the checker confines a repair
//! to.

use once_cell::sync::Lazy;
use regex::Regex;

use super::normalize_whitespace;
use super::segmentation::sentences;

// The patterns are constants and compile; `None` would only leave the
// syntax in the text, which is the safe direction.
static LINE_MARKER: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"^[ \t]*(?:#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+|\d+[.)][ \t]+)").ok());
static LINK: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r#"!?\[([^\]\n]*)\]\([^)\s]*(?:\s+"[^"]*")?\)"#).ok());
static CITATION_MARKER: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"[ \t]*\[\d+(?:[ \t]*[,–-][ \t]*\d+)*\]").ok());

/// `markdown` as plain text on one line: headings, list and quote markers,
/// emphasis, inline code ticks, link syntax (its text is kept) and `[n]`
/// citation markers removed, whitespace collapsed.
pub fn plain_text(markdown: &str) -> String {
    normalize_whitespace(&plain_lines(markdown))
}

/// The sentence of `field_text` that holds `quote`, with the rest of its
/// paragraph before and after it, all as plain text: `(before, sentence,
/// after)`. A quote that runs over several sentences takes them all. When
/// the quote is not in the text, the sentence is the quote itself, plain,
/// with no context.
pub fn plain_sentence_context(field_text: &str, quote: &str) -> (String, String, String) {
    let quote = plain_text(quote);
    if !quote.is_empty() {
        for paragraph in paragraphs(field_text) {
            if let Some(found) = table_row_context(&paragraph, &quote) {
                return found;
            }
            if let Some(found) = split_at_quote(&plain_lines(&paragraph), &quote) {
                return found;
            }
        }
    }
    (String::new(), quote, String::new())
}

/// Whether `line` is a Markdown table alignment separator row (`| :--- | --- |`).
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('|')
        && trimmed.contains('-')
        && trimmed
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// Cleaned cells of a Markdown pipe table row (`| a | b |`), or `None` when
/// `line` is not a table row.
fn table_cells(line: &str, require_outer_pipes: bool) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if is_table_separator(trimmed) || !trimmed.contains('|') {
        return None;
    }
    let has_outer = trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2;
    if require_outer_pipes && !has_outer {
        return None;
    }
    if !has_outer && trimmed.matches('|').count() < 2 {
        return None;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let cells: Vec<String> = inner
        .split('|')
        .map(|cell| normalize_whitespace(&clean_inline(cell)))
        .collect();
    (!cells.is_empty()).then_some(cells)
}

/// When `paragraph` holds a Markdown table and `plain_quote` matches one of its
/// data rows, return `(lead_before_table, header_paired_row, "")` so the review
/// UI shows a readable labeled row instead of flattened `| :--- |` pipe syntax.
fn table_row_context(paragraph: &str, plain_quote: &str) -> Option<(String, String, String)> {
    let lines: Vec<&str> = paragraph.lines().collect();
    let sep = lines.iter().position(|line| is_table_separator(line))?;
    let header_idx = sep.checked_sub(1)?;
    let headers = table_cells(lines.get(header_idx)?, false)?;
    for &line in lines.get(sep + 1..)? {
        let Some(cells) = table_cells(line, false) else {
            continue;
        };
        let joined = cells
            .iter()
            .filter(|cell| !cell.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        if joined.is_empty() || !joined.contains(plain_quote) {
            continue;
        }
        let paired: Vec<String> = cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| !cell.is_empty())
            .map(
                |(col, cell)| match headers.get(col).filter(|h| !h.is_empty()) {
                    Some(header) => format!("{header}: {cell}"),
                    None => cell.clone(),
                },
            )
            .collect();
        let before = lines
            .get(..header_idx)
            .unwrap_or_default()
            .iter()
            .map(|line| plain_text(line))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        return Some((before, paired.join(" · "), String::new()));
    }
    None
}

/// Inline Markdown (`[text](url)`, `[1]` citations, emphasis, backticks)
/// stripped from a single line or table cell.
fn clean_inline(text: &str) -> String {
    let text = replace(&LINK, text, "$1");
    let text = replace(&CITATION_MARKER, &text, "");
    without_emphasis(&text)
}

/// The paragraph's sentences before, holding, and after `quote`.
fn split_at_quote(plain: &str, quote: &str) -> Option<(String, String, String)> {
    let pieces: Vec<String> = sentences(plain)
        .into_iter()
        .map(|(start, end)| normalize_whitespace(&plain[start..end]))
        .collect();
    let mut flat = String::new();
    let mut offsets = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        if !flat.is_empty() {
            flat.push(' ');
        }
        let start = flat.len();
        flat.push_str(piece);
        offsets.push((start, flat.len()));
    }
    let at = flat.find(quote)?;
    let end = at + quote.len();
    let first = offsets.iter().position(|&(_, stop)| stop > at)?;
    let last = offsets.iter().rposition(|&(start, _)| start < end)?;
    if first > last {
        return None;
    }
    Some((
        pieces.get(..first)?.join(" "),
        pieces.get(first..=last)?.join(" "),
        pieces.get(last + 1..)?.join(" "),
    ))
}

/// The text's paragraphs: runs of lines between blank lines.
fn paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join("\n"));
    }
    paragraphs
}

/// `markdown` as plain text, line breaks kept so sentences still end at
/// them. Markdown table alignment rows are dropped and pipe-delimited rows
/// become readable `·`-separated cells.
fn plain_lines(markdown: &str) -> String {
    markdown
        .split('\n')
        .filter_map(|line| {
            if is_table_separator(line) {
                return None;
            }
            if let Some(cells) = table_cells(line, true) {
                let row = cells
                    .into_iter()
                    .filter(|cell| !cell.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                return Some(row);
            }
            let line = replace(&LINE_MARKER, line, "");
            Some(clean_inline(&line))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace(pattern: &Lazy<Option<Regex>>, text: &str, with: &str) -> String {
    match pattern.as_ref() {
        Some(pattern) => pattern.replace_all(text, with).into_owned(),
        None => text.to_owned(),
    }
}

/// `line` without emphasis marks or inline code ticks. The text inside
/// backticks is kept as written; outside them a `*` or `_` is a mark unless
/// it joins two word characters (`snake_case`, `2*3`) or stands alone
/// between spaces, and `~~` is always one.
fn without_emphasis(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    for (index, segment) in line.split('`').enumerate() {
        if index % 2 == 1 {
            plain.push_str(segment);
            continue;
        }
        let chars: Vec<char> = segment.chars().collect();
        for (at, &ch) in chars.iter().enumerate() {
            let before = at.checked_sub(1).and_then(|prev| chars.get(prev));
            let after = chars.get(at + 1);
            let mark = match ch {
                '*' | '_' => {
                    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
                    let space = |c: Option<&char>| c.is_none_or(|c| c.is_whitespace());
                    let joins_words = word(before) && word(after);
                    let stands_alone = ch == '*' && space(before) && space(after);
                    !(joins_words || stands_alone)
                }
                '~' => before == Some(&'~') || after == Some(&'~'),
                _ => false,
            };
            if !mark {
                plain.push(ch);
            }
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::{plain_sentence_context, plain_text, sentences};

    fn pieces(text: &str) -> Vec<&str> {
        sentences(text)
            .into_iter()
            .map(|(start, end)| &text[start..end])
            .collect()
    }

    #[test]
    fn sentences_end_at_stops_before_whitespace_and_at_line_breaks() {
        assert_eq!(
            pieces("Cells divide. Version 2.5 ships!  Why?\nA list item"),
            ["Cells divide.", "Version 2.5 ships!", "Why?", "A list item"]
        );
        assert!(sentences("   ").is_empty());
    }

    #[test]
    fn plain_text_drops_markdown_and_citation_markers() {
        assert_eq!(
            plain_text("## The **Treaty** of _Westphalia_ [2] ended the war [3, 4]."),
            "The Treaty of Westphalia ended the war."
        );
        assert_eq!(
            plain_text("See [the atlas](https://example.org/atlas \"Atlas\") and ~~not~~ this."),
            "See the atlas and not this."
        );
        assert_eq!(
            plain_text("- Call `snake_case(2*3)` with my_value and 2*3 or 2 * 3."),
            "Call snake_case(2*3) with my_value and 2*3 or 2 * 3."
        );
    }

    #[test]
    fn the_quote_is_found_in_its_sentence_and_paragraph() {
        let field = "Monarchs breed in the north.\n\nIn autumn, **monarch butterflies** migrate up to 3,000 miles [1]. They winter in Mexico. Few survive the trip.";
        let (before, sentence, after) =
            plain_sentence_context(field, "monarch butterflies migrate up to 3,000 miles");
        assert_eq!(before, "");
        assert_eq!(
            sentence,
            "In autumn, monarch butterflies migrate up to 3,000 miles."
        );
        assert_eq!(after, "They winter in Mexico. Few survive the trip.");

        let (before, sentence, after) =
            plain_sentence_context(field, "They winter in Mexico. Few survive");
        assert_eq!(
            before,
            "In autumn, monarch butterflies migrate up to 3,000 miles."
        );
        assert_eq!(sentence, "They winter in Mexico. Few survive the trip.");
        assert_eq!(after, "");
    }

    #[test]
    fn a_quote_the_text_lacks_stands_alone() {
        assert_eq!(
            plain_sentence_context("Water boils at 100 °C.", "The *Nile* floods [1]"),
            (String::new(), "The Nile floods".to_owned(), String::new())
        );
    }

    #[test]
    fn markdown_table_rows_render_with_column_headers_and_without_separator_pipes() {
        let field = concat!(
            "Side-by-Side Comparison:\n",
            "| Feature | Patient | Encounter |\n",
            "| :--- | :--- | :--- |\n",
            "| **Role** | Persistent Identity | Temporary Context |\n",
            "| **Stability** | High (changes rarely) | Low (starts/ends with visits) |"
        );
        let (before, sentence, after) = plain_sentence_context(
            field,
            "| **Role** | Persistent Identity | Temporary Context |",
        );
        assert_eq!(before, "Side-by-Side Comparison:");
        assert_eq!(
            sentence,
            "Feature: Role · Patient: Persistent Identity · Encounter: Temporary Context"
        );
        assert_eq!(after, "");
        assert!(!plain_text(field).contains(":---"));
        assert!(!plain_text(field).contains('|'));
    }
}
