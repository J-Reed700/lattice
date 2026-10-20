mod line_refs;
mod salience;

pub use line_refs::{contains_line_reference, is_line_reference};
pub use salience::{is_stopword, select_informative_terms};

pub fn safe_truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    input.chars().take(max_chars).collect()
}

/// `text` with the Latin combining diacritics (U+0300 to U+036F) taken off
/// its letters, so `café` reads `cafe`. This is what SQLite FTS5's
/// `unicode61` tokenizer does by default when it indexes a word, so a word
/// looked up in such an index's vocabulary has to be folded the same way.
/// Other scripts recompose as they were: only the Latin marks go.
pub fn fold_diacritics(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .nfc()
        .collect()
}

/// Normalize whitespace by collapsing all runs of whitespace into single spaces.
pub fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract highlight terms from a query string.
///
/// - Lowercases and de-duplicates terms
/// - Drops short tokens (<3 chars)
/// - Limits total terms to `max_terms`
pub fn extract_highlight_terms(query: &str, max_terms: usize) -> Vec<String> {
    if max_terms == 0 {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            tokens.push(current);
            current = String::new();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for token in tokens {
        if token.len() < 3 || !token.chars().any(|ch| ch.is_ascii_alphabetic()) {
            continue;
        }
        if seen.insert(token.clone()) {
            deduped.push(token);
        }
    }

    if deduped.is_empty() {
        return Vec::new();
    }

    let mut ranked: Vec<(String, f32)> = deduped
        .into_iter()
        .map(|term| {
            let score = term_salience(&term);
            (term, score)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let best_score = ranked.first().map(|(_, score)| *score).unwrap_or(0.0);
    if best_score <= f32::EPSILON {
        return ranked
            .into_iter()
            .take(max_terms)
            .map(|(term, _)| term)
            .collect();
    }

    let mut selected: Vec<String> = ranked
        .iter()
        .filter_map(|(term, score)| (*score >= best_score * 0.45).then_some(term.clone()))
        .take(max_terms)
        .collect();

    if selected.is_empty() {
        selected = ranked
            .into_iter()
            .take(max_terms)
            .map(|(term, _)| term)
            .collect();
    }

    selected
}

fn term_entropy(term: &str) -> f32 {
    use std::collections::HashMap;

    let len = term.len();
    if len == 0 {
        return 0.0;
    }

    let mut counts: HashMap<char, usize> = HashMap::new();
    for ch in term.chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }

    let denom = len as f32;
    counts.values().fold(0.0_f32, |acc, count| {
        let p = (*count as f32) / denom;
        if p <= f32::EPSILON {
            acc
        } else {
            acc - p * p.log2()
        }
    })
}

fn term_salience(term: &str) -> f32 {
    let entropy = term_entropy(term);
    let length_factor = ((term.len() as f32) + 1.0).ln();
    entropy * (0.65 + 0.35 * length_factor)
}

/// Build a query-aware excerpt from content.
///
/// - Normalizes whitespace
/// - Centers excerpt around the first matching highlight term
/// - Expands to sentence boundaries when possible
/// - Adds ellipses when truncated
pub fn build_excerpt(content: &str, highlight_terms: &[String], max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let normalized = normalize_whitespace(content);
    if normalized.is_empty() {
        return String::new();
    }

    let chars: Vec<char> = normalized.chars().collect();
    let total_chars = chars.len();

    if total_chars <= max_chars {
        return normalized;
    }

    let mut best_char_index: Option<usize> = None;

    for term in highlight_terms {
        if term.is_empty() {
            continue;
        }

        let term_index = find_term_char_index(&normalized, term);
        if let Some(char_idx) = term_index {
            best_char_index = Some(match best_char_index {
                Some(best) => best.min(char_idx),
                None => char_idx,
            });
        }
    }

    let center = best_char_index.unwrap_or(max_chars / 2);
    let half = max_chars / 2;
    let mut start = center.saturating_sub(half);
    let mut end = (start + max_chars).min(total_chars);

    // Adjust start to a sentence boundary within 80 chars, if available.
    let start_scan = start.saturating_sub(80);
    for i in (start_scan..start).rev() {
        if chars
            .get(i)
            .is_some_and(|ch| matches!(ch, '.' | '?' | '!' | '\n'))
        {
            start = (i + 1).min(total_chars.saturating_sub(1));
            break;
        }
    }

    // Adjust end forward to sentence boundary within 80 chars, if available.
    let end_scan = (end + 80).min(total_chars);
    for (i, ch) in chars.iter().enumerate().take(end_scan).skip(end) {
        if matches!(ch, '.' | '?' | '!' | '\n') {
            end = (i + 1).min(total_chars);
            break;
        }
    }

    if start >= end || end > total_chars {
        return safe_truncate(&normalized, max_chars);
    }

    let Some(excerpt_chars) = chars.get(start..end) else {
        return safe_truncate(&normalized, max_chars);
    };
    let excerpt: String = excerpt_chars.iter().collect();
    let trimmed = excerpt.trim();

    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < total_chars { "…" } else { "" };

    format!("{prefix}{trimmed}{suffix}")
}

fn find_term_char_index(haystack: &str, term: &str) -> Option<usize> {
    if term.is_empty() {
        return None;
    }

    // Fast path: direct match (case-sensitive)
    if let Some(byte_idx) = haystack.find(term) {
        return Some(haystack[..byte_idx].chars().count());
    }

    // Unicode-safe fallback: case-insensitive search without relying on byte offsets
    let needle = term.to_lowercase();
    for (byte_idx, _) in haystack.char_indices() {
        let slice = &haystack[byte_idx..];
        if slice.to_lowercase().starts_with(&needle) {
            return Some(haystack[..byte_idx].chars().count());
        }
    }

    None
}

pub fn redact_log(input: &str) -> String {
    const MAX_CHARS: usize = 20;
    if input.chars().count() <= MAX_CHARS {
        input.to_string()
    } else {
        format!("{}...", safe_truncate(input, MAX_CHARS))
    }
}

/// Largest byte index `<= index` that sits on a char boundary of `text`, so a
/// byte-budgeted prefix can be sliced without panicking inside a multibyte
/// character. `std::str::floor_char_boundary` is still unstable.
pub fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    (0..=index)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0)
}

/// `text` in consecutive parts of at most `max_chars` characters that
/// together are exactly `text`: for material too long for one model call,
/// which is read in parts rather than cut. A part ends at a line break, or
/// failing that a space, in its last fifth where there is one, so a part
/// rarely ends mid-word.
pub fn split_into_parts(text: &str, max_chars: usize) -> Vec<&str> {
    let size = max_chars.max(1);
    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest
            .char_indices()
            .nth(size)
            .map_or(rest.len(), |(offset, _)| offset);
        let mut cut = end;
        if end < rest.len() {
            let floor = rest
                .char_indices()
                .nth(size - size / 5)
                .map_or(end, |(offset, _)| offset)
                .min(end);
            let window = &rest[floor..end];
            if let Some(found) = window.rfind('\n').or_else(|| window.rfind(' ')) {
                cut = floor + found + 1;
            }
        }
        parts.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    parts
}

#[cfg(test)]
mod tests {
    #[test]
    fn parts_are_bounded_lossless_and_break_between_words() {
        let text: String = (0..500).map(|index| format!("word{index} ")).collect();
        let parts = super::split_into_parts(&text, 100);
        assert!(parts.len() > 1);
        assert_eq!(parts.concat(), text);
        assert!(parts.iter().all(|part| part.chars().count() <= 100));
        assert!(parts[..parts.len() - 1]
            .iter()
            .all(|part| part.ends_with(' ')));
        assert_eq!(super::split_into_parts("short", 100), vec!["short"]);
        assert!(super::split_into_parts("", 100).is_empty());
        // No break to use: a hard cut on a character boundary.
        assert_eq!(super::split_into_parts("時時時時", 3), vec!["時時時", "時"]);
    }

    #[test]
    fn floor_char_boundary_never_splits_a_character() {
        let text = "ab時cd"; // '時' spans bytes 2..5
        assert_eq!(super::floor_char_boundary(text, 0), 0);
        assert_eq!(super::floor_char_boundary(text, 2), 2);
        assert_eq!(super::floor_char_boundary(text, 3), 2);
        assert_eq!(super::floor_char_boundary(text, 4), 2);
        assert_eq!(super::floor_char_boundary(text, 5), 5);
        assert_eq!(super::floor_char_boundary(text, 99), text.len());
        assert!(text.get(..super::floor_char_boundary(text, 4)).is_some());
    }

    use super::*;

    #[test]
    fn folding_takes_latin_marks_off_and_leaves_other_scripts_whole() {
        assert_eq!(fold_diacritics("Café Ñandú"), "Cafe Nandu");
        assert_eq!(fold_diacritics("한국어 日本語"), "한국어 日本語");
        assert_eq!(fold_diacritics("plain_ascii"), "plain_ascii");
    }

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_safe_truncate_unicode() {
        let input = "こんにちは世界";
        assert_eq!(safe_truncate(input, 4), "こんにち");
    }

    #[test]
    fn test_safe_truncate_exact_length() {
        let input = "abcd";
        assert_eq!(safe_truncate(input, 4), "abcd");
    }

    #[test]
    fn test_redact_log_short() {
        let input = "short message";
        assert_eq!(redact_log(input), "short message");
    }

    #[test]
    fn test_redact_log_long() {
        let input = "this is a very long message for redaction";
        assert_eq!(redact_log(input), "this is a very long ...");
    }

    #[test]
    fn test_normalize_whitespace() {
        let input = "Hello   world\nThis\tis  a test";
        assert_eq!(normalize_whitespace(input), "Hello world This is a test");
    }

    #[test]
    fn test_extract_highlight_terms() {
        let terms = extract_highlight_terms("The quick brown fox jumps over the lazy dog", 5);
        assert!(!terms.contains(&"the".to_string()));
        assert!(terms.contains(&"quick".to_string()));
        assert!(terms.contains(&"brown".to_string()));
        assert!(terms.len() <= 5);
    }

    #[test]
    fn test_build_excerpt_basic() {
        let content = "This is a long piece of text about climate change impacts on glaciers.";
        let terms = vec!["climate".to_string()];
        let excerpt = build_excerpt(content, &terms, 40);
        assert!(excerpt.to_lowercase().contains("climate"));
    }
}
