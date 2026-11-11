//! When two pieces of text are the same words. Text is folded before it is
//! compared: Markdown emphasis and code markers dropped, typographic quotes
//! and dashes made plain, whitespace runs one space, case ignored, and no
//! space at either end.

use lattice_core::shared::text::segmentation::sentences;

/// One character of folded text with the bytes of the original it came from.
pub(crate) type Folded = Vec<(char, usize, usize)>;

fn plain(character: char) -> char {
    match character {
        '\u{2018}' | '\u{2019}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201F}' => '"',
        '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        other => other,
    }
}

/// `text` folded, each character with its byte span in `text`.
pub(crate) fn folded(text: &str) -> Folded {
    let mut out: Folded = Vec::new();
    let mut gap: Option<(usize, usize)> = None;
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        if matches!(character, '*' | '_' | '`') {
            continue;
        }
        if character.is_whitespace() {
            if gap.is_none() && !out.is_empty() {
                gap = Some((start, end));
            }
            continue;
        }
        if let Some((from, to)) = gap.take() {
            out.push((' ', from, to));
        }
        out.extend(
            plain(character)
                .to_lowercase()
                .map(|lower| (lower, start, end)),
        );
    }
    out
}

/// `text` folded.
pub(crate) fn fold(text: &str) -> String {
    folded(text).into_iter().map(|(c, _, _)| c).collect()
}

/// Whether `a` and `b` are the same words, or one holds the other. Neither
/// may be empty once folded.
pub(crate) fn overlap(a: &str, b: &str) -> bool {
    let (a, b) = (fold(a), fold(b));
    !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a))
}

/// Whether some sentence of `sentence` is, folded, a whole sentence of
/// `text`: a sentence inside a longer one does not count.
pub(crate) fn holds_sentence(text: &str, sentence: &str) -> bool {
    let folded_sentences = |text: &str| -> Vec<String> {
        sentences(text)
            .into_iter()
            .filter_map(|(from, to)| text.get(from..to))
            .map(fold)
            .filter(|folded| !folded.is_empty())
            .collect()
    };
    let held = folded_sentences(text);
    folded_sentences(sentence)
        .iter()
        .any(|folded| held.contains(folded))
}

/// The bytes of `text` that `needle` stands for, if it occurs in it (the
/// first occurrence) once both are folded: from where its first folded character began to where
/// its last one ended.
pub(crate) fn find(text: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = folded(needle).into_iter().map(|(c, _, _)| c).collect();
    if needle.is_empty() {
        return None;
    }
    let hay = folded(text);
    let chars: Vec<char> = hay.iter().map(|(c, _, _)| *c).collect();
    let at = chars
        .windows(needle.len())
        .position(|window| window == needle.as_slice())?;
    let first = hay.get(at)?;
    let last = hay.get(at + needle.len() - 1)?;
    Some((first.1, last.2))
}

/// The bytes of `text` that `needle` stands for at each place it occurs once
/// both are folded, in order. Occurrences may overlap.
pub(crate) fn find_all(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = folded(needle).into_iter().map(|(c, _, _)| c).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = folded(text);
    let chars: Vec<char> = hay.iter().map(|(c, _, _)| *c).collect();
    chars
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle.as_slice())
        .filter_map(|(at, _)| {
            let first = hay.get(at)?;
            let last = hay.get(at + needle.len() - 1)?;
            Some((first.1, last.2))
        })
        .collect()
}

/// The byte spans of the sentences of `text` that the span `(start, end)`
/// overlaps. A span that overlaps none (text the segmenter does not split)
/// stands for itself, so a caller always has the exact text.
pub(crate) fn sentences_over(text: &str, (start, end): (usize, usize)) -> Vec<(usize, usize)> {
    let lying: Vec<(usize, usize)> = sentences(text)
        .into_iter()
        .filter(|(from, to)| *from < end && start < *to)
        .collect();
    if lying.is_empty() {
        vec![(start, end)]
    } else {
        lying
    }
}

/// The text of `text` from the start of the first sentence the span lies in
/// to the end of the last ([`sentences_over`]).
pub(crate) fn exact_sentences(text: &str, span: (usize, usize)) -> Option<&str> {
    let lying = sentences_over(text, span);
    let start = lying.first()?.0;
    let end = lying.last()?.1;
    text.get(start..end)
}

#[cfg(test)]
#[path = "same_text_tests.rs"]
mod tests;
