//! A citation marker means something only in the turn that wrote it: each turn
//! numbers its passages afresh.

use super::markdown_math::map_prose;
use super::text::segmentation::{code_fences, CodeFence};

/// `text` without its `[n]` and `[n, m]` markers and the spaces before them.
/// Code is left as written, a formula's brackets are its own (`$a[1]$`), and
/// `[1](url)` is a link.
pub fn without_citation_markers(text: &str) -> String {
    let fences: Vec<_> = code_fences(text).iter().map(CodeFence::whole).collect();
    let mut start = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let at = start;
            start += line.len();
            if fences.iter().any(|whole| whole.contains(&at)) {
                line.to_owned()
            } else {
                map_prose(line, outside_inline_code)
            }
        })
        .collect()
}

fn outside_inline_code(text: &str) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, part)| {
            if index % 2 == 0 {
                stripped(part)
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

fn stripped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let before = rest.get(..open).unwrap_or_default();
        let from = rest.get(open..).unwrap_or_default();
        match marker_length(from) {
            Some(length) => {
                out.push_str(before.trim_end_matches([' ', '\t']));
                rest = from.get(length..).unwrap_or_default();
            }
            None => {
                out.push_str(before);
                out.push('[');
                rest = from.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

/// How long the marker `text` opens with is, if it opens with one.
fn marker_length(text: &str) -> Option<usize> {
    let (labels, after) = text.strip_prefix('[')?.split_once(']')?;
    let numbered = labels
        .split(',')
        .map(str::trim)
        .all(|label| !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit()));
    (numbered && !after.starts_with('(')).then_some(labels.len() + 2)
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
}
