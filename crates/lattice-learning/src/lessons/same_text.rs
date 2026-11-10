//! When two pieces of text are the same words. Text is folded before it is
//! compared: Markdown emphasis and code markers dropped, typographic quotes
//! and dashes made plain, whitespace runs one space, case ignored, and no
//! space at either end.

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

#[cfg(test)]
#[path = "same_text_tests.rs"]
mod tests;
