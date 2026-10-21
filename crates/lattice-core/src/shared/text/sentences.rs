//! Sentences of written text, and markdown read as plain text: what the
//! claim review shows of a claim (its sentence in its paragraph) and what
//! the checker confines a repair to.

use once_cell::sync::Lazy;
use regex::Regex;

use super::normalize_whitespace;

// The patterns are constants and compile; `None` would only leave the
// syntax in the text, which is the safe direction.
static LINE_MARKER: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"^[ \t]*(?:#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+|\d+[.)][ \t]+)").ok());
static LINK: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r#"!?\[([^\]\n]*)\]\([^)\s]*(?:\s+"[^"]*")?\)"#).ok());
static CITATION_MARKER: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"[ \t]*\[\d+(?:[ \t]*[,–-][ \t]*\d+)*\]").ok());

/// The byte spans of `text`'s sentences, trimmed: a sentence ends at a
/// line break, or at '.', '!' or '?' before whitespace or the end.
pub fn sentences(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut push = |from: usize, to: usize| {
        let Some(piece) = text.get(from..to) else {
            return;
        };
        let lead = piece.len() - piece.trim_start().len();
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            spans.push((from + lead, from + lead + trimmed.len()));
        }
    };
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let before_space = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        if ch == '\n' || (matches!(ch, '.' | '!' | '?') && before_space) {
            let end = at + ch.len_utf8();
            push(start, end);
            start = end;
        }
    }
    push(start, text.len());
    spans
}

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
            if let Some(found) = split_at_quote(&plain_lines(&paragraph), &quote) {
                return found;
            }
        }
    }
    (String::new(), quote, String::new())
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
    (first <= last).then(|| {
        (
            pieces[..first].join(" "),
            pieces[first..=last].join(" "),
            pieces[last + 1..].join(" "),
        )
    })
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
/// them.
fn plain_lines(markdown: &str) -> String {
    markdown
        .split('\n')
        .map(|line| {
            let line = replace(&LINE_MARKER, line, "");
            let line = replace(&LINK, &line, "$1");
            let line = replace(&CITATION_MARKER, &line, "");
            without_emphasis(&line)
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
                    !(word(before) && word(after)) && !(ch == '*' && space(before) && space(after))
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
}
