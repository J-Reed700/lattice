//! The numbered form (`[n]`, `[n, m]`) of messages saved before handles existed,
//! read where it stands outside code and formulas, and nothing else: each turn
//! numbered its passages afresh, so a number means something only with the
//! sources saved beside its message.

use std::ops::Range;

use super::markdown_math::split_at_formulas;
use super::text::segmentation::{code_fences, CodeFence};

/// One `[n]` or `[n, m]` marker of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OldMarker<'a> {
    /// The marker's bytes in the text, brackets included.
    pub range: Range<usize>,
    /// Where the spaces and tabs just before the marker begin (the marker's own
    /// start when there are none), never earlier than the text since the last
    /// marker or code span.
    pub spaces_from: usize,
    /// The labels between the brackets, each digits only.
    pub labels: Vec<&'a str>,
}

/// Every marker of `text`, in order. Code is left as written, a formula's
/// brackets are its own (`$a[1]$`), and `[1](url)` is a link.
pub fn old_markers(text: &str) -> Vec<OldMarker<'_>> {
    let fences: Vec<_> = code_fences(text).iter().map(CodeFence::whole).collect();
    let mut found = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let at = start;
        start += line.len();
        if fences.iter().any(|whole| whole.contains(&at)) {
            continue;
        }
        let mut offset = at;
        for (piece, formula) in split_at_formulas(line) {
            if !formula {
                markers_outside_inline_code(piece, offset, &mut found);
            }
            offset += piece.len();
        }
    }
    found
}

/// `text` without its markers and the spaces before them.
pub fn without_citation_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for marker in old_markers(text) {
        out.push_str(text.get(at..marker.spaces_from).unwrap_or_default());
        at = marker.range.end;
    }
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

fn markers_outside_inline_code<'a>(
    piece: &'a str,
    offset: usize,
    found: &mut Vec<OldMarker<'a>>,
) {
    let mut at = offset;
    for (index, part) in piece.split('`').enumerate() {
        if index % 2 == 0 {
            markers_in(part, at, found);
        }
        at += part.len() + 1;
    }
}

fn markers_in<'a>(part: &'a str, at: usize, found: &mut Vec<OldMarker<'a>>) {
    let mut pos = 0;
    let mut floor = 0;
    while let Some(relative) = part.get(pos..).and_then(|rest| rest.find('[')) {
        let open = pos + relative;
        match part.get(open..).and_then(marker) {
            Some((labels, length)) => {
                let before = part.get(floor..open).unwrap_or_default();
                let spaces = before.len() - before.trim_end_matches([' ', '\t']).len();
                found.push(OldMarker {
                    range: at + open..at + open + length,
                    spaces_from: at + open - spaces,
                    labels: labels.split(',').map(str::trim).collect(),
                });
                pos = open + length;
                floor = pos;
            }
            None => pos = open + 1,
        }
    }
}

/// The labels of the marker `text` opens with and its length, if it opens with one.
fn marker(text: &str) -> Option<(&str, usize)> {
    let (labels, after) = text.strip_prefix('[')?.split_once(']')?;
    let numbered = labels
        .split(',')
        .map(str::trim)
        .all(|label| !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit()));
    (numbered && !after.starts_with('(')).then_some((labels, labels.len() + 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_goes_with_the_space_before_it() {
        assert_eq!(
            without_citation_markers("It lasts 74 hours [1]. Ask first [2][3], or [4, 5]."),
            "It lasts 74 hours. Ask first, or."
        );
    }

    #[test]
    fn lines_and_their_breaks_are_kept() {
        assert_eq!(
            without_citation_markers("First [1]\n\n- Second [2]\n"),
            "First\n\n- Second\n"
        );
    }

    #[test]
    fn code_is_left_as_written() {
        let answer = "Use `values[1]` here.\n```python\nprint(values[1])\n```\nDone [2].";
        assert_eq!(
            without_citation_markers(answer),
            "Use `values[1]` here.\n```python\nprint(values[1])\n```\nDone."
        );
    }

    #[test]
    fn a_shorter_run_inside_a_fence_does_not_close_it() {
        assert_eq!(
            without_citation_markers("````\n```\nx [1]\n````\nText [2].\n"),
            "````\n```\nx [1]\n````\nText.\n"
        );
    }

    #[test]
    fn a_formula_keeps_its_brackets() {
        assert_eq!(
            without_citation_markers("The term $a[1]$ grows [2]."),
            "The term $a[1]$ grows."
        );
    }

    #[test]
    fn a_link_and_other_brackets_are_not_markers() {
        let answer = "See [1](https://example.com), [note], [] and [1a].";
        assert_eq!(without_citation_markers(answer), answer);
    }

    #[test]
    fn each_marker_is_found_with_its_place_and_labels() {
        let text = "A [2] and [1, 3]. `x[4]` $a[5]$ [6](u)";
        let found = old_markers(text);
        let places: Vec<(&str, Vec<&str>)> = found
            .iter()
            .map(|marker| (&text[marker.range.clone()], marker.labels.clone()))
            .collect();
        assert_eq!(
            places,
            vec![("[2]", vec!["2"]), ("[1, 3]", vec!["1", "3"])]
        );
        assert_eq!(&text[found[0].spaces_from..found[0].range.end], " [2]");
    }
}
