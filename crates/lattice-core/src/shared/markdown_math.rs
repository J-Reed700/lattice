//! TeX math in answer markdown, found the way the window draws it.
//!
//! The viewer draws `$…$`, `\(…\)` and `$$…$$` inside a line as inline
//! formulas, and `$$…$$` or `\[…\]` starting a line of its own (on that line
//! or across several, with no blank line between) as a displayed one. The
//! dollar rules are Pandoc's, so prose keeps its prices: an opening `$` has a
//! non-space after it, the closing `$` has a non-space before it and no digit
//! after it (`$5 and $10` stays text), `\$` is a literal dollar, and a `$`
//! straight after another never opens one. A formula never holds a backtick,
//! so a code span keeps its dollars, and an unclosed opener is text.
//!
//! The checks read answers by the same rules: a displayed formula is set
//! aside like a diagram's fence (`markdown_media`), and an inline one is part
//! of its sentence whose brackets are never a citation and whose TeX control
//! words are never the sentence's words.
//! Viewer: src/components/TiptapEditor/extensions/mathSyntax.ts.
//! Design: docs/design/2026-10-19-math-rendering.md.

use std::ops::Range;

/// Standing guidance for every prompt whose answer the viewer draws: chat
/// turns and lessons. Constant, so a cached prompt prefix stays the same.
pub const MATH_GUIDANCE: &str =
    "Write mathematical notation as LaTeX: inline as $…$, and a displayed formula as $$…$$ on \
     lines of its own. Never use $ math for plain numbers or money, and keep citation markers \
     outside the math.";

/// Byte ranges of the inline formulas in one line or sentence of markdown,
/// delimiters included, in order. Code spans and escaped characters are
/// stepped over, as the viewer's parser steps over them.
pub fn inline_math_spans(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'`' => at = past_code_span(bytes, at),
            b'\\' if bytes.get(at + 1) != Some(&b'(') => at += 2,
            b'\\' | b'$' => match inline_math_at(text, at) {
                Some(end) => {
                    spans.push(at..end);
                    at = end;
                }
                // An unmatched `$$` is two literal dollars.
                None if bytes.get(at..at + 2) == Some(b"$$") => at += 2,
                None => at += 1,
            },
            _ => at += 1,
        }
    }
    spans
}

/// `text` with each inline formula replaced by the names written in it,
/// without TeX's control words, braces or symbols: `$\frac{TP}{TP+FP}$`
/// reads as `TP TP FP`. Lexical matching then counts a sentence's prose and
/// a formula's names, never `\frac`.
pub fn formulas_as_words(text: &str) -> String {
    split_at_formulas(text)
        .into_iter()
        .map(|(piece, formula)| {
            if formula {
                tex_words(piece)
            } else {
                piece.to_string()
            }
        })
        .collect()
}

/// `text` with `prose` applied to the stretches between its inline
/// formulas; the formulas themselves are copied as written.
pub fn map_prose(text: &str, prose: impl Fn(&str) -> String) -> String {
    split_at_formulas(text)
        .into_iter()
        .map(|(piece, formula)| {
            if formula {
                piece.to_string()
            } else {
                prose(piece)
            }
        })
        .collect()
}

/// `text` cut at its inline formulas, in order: each piece with whether it
/// is a formula (delimiters included).
pub fn split_at_formulas(text: &str) -> Vec<(&str, bool)> {
    let mut pieces = Vec::new();
    let mut last = 0;
    for span in inline_math_spans(text) {
        pieces.push((text.get(last..span.start).unwrap_or_default(), false));
        pieces.push((text.get(span.clone()).unwrap_or_default(), true));
        last = span.end;
    }
    pieces.push((text.get(last..).unwrap_or_default(), false));
    pieces
}

/// How many of `lines` a displayed formula opening the first one takes, or
/// `None` when the first opens none. Each line is its block content, with
/// indentation, quote and list markers removed. A blank line or the end of
/// the text before the closer leaves the opener as text, as does text after
/// the closer on its line (that is a formula inside a paragraph).
pub fn display_math_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let mut lines = lines.into_iter();
    let first = lines.next()?.trim_end();
    let (open, close) = if first.starts_with("$$") {
        ("$$", "$$")
    } else if first.starts_with("\\[") {
        ("\\[", "\\]")
    } else {
        return None;
    };
    let mut line = first.get(open.len()..)?;
    let mut count = 1;
    let mut empty = true;
    loop {
        if let Some(end) = closer_at(line, 0, close) {
            if !line.get(end + close.len()..)?.trim().is_empty() {
                return None;
            }
            empty &= line.get(..end)?.trim().is_empty();
            break;
        }
        empty &= line.trim().is_empty();
        line = lines.next()?.trim_end();
        if line.trim().is_empty() {
            return None;
        }
        count += 1;
    }
    (!empty).then_some(count)
}

/// The end of the inline formula opening at `at`, past its closer.
fn inline_math_at(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?;
    let (open, close) = if rest.starts_with("\\(") {
        ("\\(", "\\)")
    } else if rest.starts_with("$$") {
        ("$$", "$$")
    } else if rest.starts_with('$') && !text.get(..at)?.ends_with('$') {
        ("$", "$")
    } else {
        return None;
    };
    let from = at + open.len();
    let end = closer_at(text, from, close)?;
    let latex = text.get(from..end)?;
    if latex.trim().is_empty() || latex.contains('`') {
        return None;
    }
    if open == "$" {
        let after = text.get(end + 1..).and_then(|rest| rest.chars().next());
        if latex.starts_with(char::is_whitespace)
            || latex.ends_with(char::is_whitespace)
            || after.is_some_and(|next| next.is_ascii_digit())
        {
            return None;
        }
    }
    Some(end + close.len())
}

/// Where `closer` first stands in `text` from `from`, stepping over `\x`
/// escapes, so `\$` inside a formula never closes it.
fn closer_at(text: &str, from: usize, closer: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = from;
    while at + closer.len() <= bytes.len() {
        if bytes.get(at..at + closer.len()) == Some(closer.as_bytes()) {
            return Some(at);
        }
        at += if bytes.get(at) == Some(&b'\\') { 2 } else { 1 };
    }
    None
}

/// Past the code span opening at `at`, or past its backtick run when nothing
/// closes it (the run is then literal text).
fn past_code_span(bytes: &[u8], at: usize) -> usize {
    let run = |from: usize| {
        bytes
            .get(from..)
            .map_or(0, |rest| rest.iter().take_while(|&&b| b == b'`').count())
    };
    let open = run(at);
    let mut next = at + open;
    while next < bytes.len() {
        let length = run(next);
        if length == open {
            return next + length;
        }
        next += length.max(1);
    }
    at + open
}

/// The names written in a formula, one space between: control words,
/// braces, operators and delimiters are TeX's, not the sentence's.
fn tex_words(formula: &str) -> String {
    let mut out = String::from(" ");
    let mut chars = formula.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            while chars.next_if(char::is_ascii_alphabetic).is_some() {}
            out.push(' ');
        } else if ch.is_alphanumeric() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out.push(' ');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formulas(text: &str) -> Vec<&str> {
        inline_math_spans(text)
            .into_iter()
            .filter_map(|span| text.get(span))
            .collect()
    }

    #[test]
    fn finds_inline_formulas_in_each_delimiter() {
        assert_eq!(
            formulas(r"Area $\pi r^2$, \(x+1\) and $$k$$ here."),
            vec![r"$\pi r^2$", r"\(x+1\)", "$$k$$"]
        );
    }

    #[test]
    fn prose_dollars_escapes_and_code_stay_text() {
        for prose in [
            "It costs $5 and $10 today.",
            "Pay $ 5$ now.",
            "A $x $ gap.",
            "Then $x$5 follows.",
            r"A literal \$x$ sign.",
            "Use `$HOME` and `$x$` in code.",
            r"Still arriving: $\frac{a}{b",
            "Pay $$5 now.",
            "Café \\é then $5",
        ] {
            assert!(formulas(prose).is_empty(), "{prose}");
        }
    }

    #[test]
    fn an_escaped_dollar_inside_a_formula_does_not_close_it() {
        assert_eq!(formulas(r"Cost $\$5 + x$ total."), vec![r"$\$5 + x$"]);
    }

    #[test]
    fn formulas_read_as_their_names_without_tex() {
        let words = formulas_as_words(r"Precision is $\frac{TP}{TP+FP}$ here [1].");
        assert_eq!(
            words.split_whitespace().collect::<Vec<_>>(),
            vec!["Precision", "is", "TP", "TP", "FP", "here", "[1]."]
        );
    }

    #[test]
    fn display_formulas_span_their_lines_and_need_a_closer() {
        assert_eq!(display_math_lines(["$$x$$\n"]), Some(1));
        assert_eq!(
            display_math_lines(["$$\n", "E = mc^2\n", "$$\n", "after"]),
            Some(3)
        );
        assert_eq!(display_math_lines([r"\[ a \]"]), Some(1));
        assert_eq!(display_math_lines([r"\[", "a", r"b \]"]), Some(3));
        // Text after the closer: a formula inside a paragraph.
        assert_eq!(display_math_lines(["$$a$$ is the area"]), None);
        assert_eq!(display_math_lines([r"\[Note\] to self."]), None);
        // Unclosed, closed only past a blank line, or empty.
        assert_eq!(display_math_lines(["$$\n", "x\n"]), None);
        assert_eq!(display_math_lines(["$$\n", "\n", "x $$"]), None);
        assert_eq!(display_math_lines(["$$ $$"]), None);
        assert_eq!(display_math_lines(["Area $$x$$"]), None);
    }
}
