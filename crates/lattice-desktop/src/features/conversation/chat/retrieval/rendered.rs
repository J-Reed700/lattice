use std::ops::Range;

/// A tool result as the model reads it, and the parts of it that are one labelled source's text.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Rendered {
    pub text: String,
    pub passages: Vec<Passage>,
    pub legend: Vec<Legend>,
}

/// One labelled source's text inside a result: the app's heading, the line it
/// stands on, and where its text is. Ranges are bytes of `Rendered::text`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Passage {
    pub heading: String,
    pub line: Range<usize>,
    pub body: Range<usize>,
}

/// A line the app wrote to label text shown elsewhere in the result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Legend {
    pub line: String,
    pub at: Range<usize>,
}

impl From<String> for Rendered {
    fn from(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
}

fn moved(range: Range<usize>, by: usize) -> Range<usize> {
    range.start + by..range.end + by
}

fn moved_legend(legend: Legend, by: usize) -> Legend {
    Legend {
        line: legend.line,
        at: moved(legend.at, by),
    }
}

impl Rendered {
    /// `other` after this text, its ranges moved to where it now stands.
    pub(crate) fn append(&mut self, other: Rendered) {
        let by = self.text.len();
        self.text.push_str(&other.text);
        self.passages
            .extend(other.passages.into_iter().map(|passage| Passage {
                heading: passage.heading,
                line: moved(passage.line, by),
                body: moved(passage.body, by),
            }));
        self.legend.extend(
            other
                .legend
                .into_iter()
                .map(|legend| moved_legend(legend, by)),
        );
    }

    /// This result between `prefix` and `suffix`.
    pub(crate) fn wrapped(self, prefix: &str, suffix: &str) -> Self {
        let mut wrapped = Self::from(prefix.to_owned());
        wrapped.append(self);
        wrapped.text.push_str(suffix);
        wrapped
    }

    /// Only the first `at` bytes: a passage whose heading line starts at or past
    /// them goes, one whose heading starts before them stays with its ranges
    /// clipped, a legend line that does not end within goes.
    pub(crate) fn truncate_bytes(&mut self, at: usize) {
        if self.text.get(..at).is_none() {
            return;
        }
        self.text.truncate(at);
        self.passages.retain(|passage| passage.line.start < at);
        for passage in &mut self.passages {
            passage.line.end = passage.line.end.min(at);
            passage.body.start = passage.body.start.min(at);
            passage.body.end = passage.body.end.min(at);
        }
        self.legend.retain(|legend| legend.at.end <= at);
    }

    /// Only the first `max_chars` characters, as `safe_truncate` cuts.
    pub(crate) fn cut(mut self, max_chars: usize) -> Self {
        if let Some((at, _)) = self.text.char_indices().nth(max_chars) {
            self.truncate_bytes(at);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(heading: &str, body: &str) -> Rendered {
        let text = format!("{heading}\n{body}");
        let line = 0..heading.len();
        let start = heading.len() + 1;
        Rendered {
            passages: vec![Passage {
                heading: heading.to_owned(),
                line,
                body: start..text.len(),
            }],
            text,
            legend: Vec::new(),
        }
    }

    fn body_of<'a>(rendered: &'a Rendered, at: usize) -> Option<&'a str> {
        rendered
            .passages
            .get(at)
            .and_then(|passage| rendered.text.get(passage.body.clone()))
    }

    #[test]
    fn appended_and_wrapped_ranges_follow_their_text() {
        let mut both = Rendered::from("Intro\n\n".to_owned());
        both.append(labelled("[s1] Page A", "Tides rise twice a day."));
        both.append("\n\n".to_owned().into());
        both.append(labelled("[s2] Page B", "Kilns fire at night."));
        let both = both.wrapped("Results:\n", "\nDone.");
        assert_eq!(body_of(&both, 0), Some("Tides rise twice a day."));
        assert_eq!(body_of(&both, 1), Some("Kilns fire at night."));
        assert_eq!(
            both.text.get(both.passages[1].line.clone()),
            Some("[s2] Page B")
        );
    }

    #[test]
    fn a_cut_clips_the_passage_across_it_and_drops_the_one_past_it() {
        let mut both = labelled("[s1] Page A", "Tides rise twice a day.");
        both.append("\n\n".to_owned().into());
        both.append(labelled("[s2] Page B", "Kilns fire at night."));
        let cut = both.clone().cut(30);
        assert_eq!(cut.text.chars().count(), 30);
        assert_eq!(cut.passages.len(), 1);
        assert_eq!(body_of(&cut, 0), Some("Tides rise twice a"));
        let whole = both.clone().cut(1_000);
        assert_eq!(whole, both);
        let before_body = both.clone().cut(12);
        assert_eq!(before_body.passages.len(), 1);
        assert_eq!(body_of(&before_body, 0), Some(""));
        let heading_cut = both.clone().cut(40);
        assert_eq!(heading_cut.passages.len(), 2);
        assert_eq!(heading_cut.passages[1].line, 37..40);
        assert_eq!(heading_cut.passages[1].body, 40..40);
        let at_heading = both.cut(37);
        assert_eq!(at_heading.passages.len(), 1);
    }

    #[test]
    fn a_cut_lands_on_a_character_boundary() {
        let cut = labelled("[s1] Café", "crème brûlée").cut(15);
        assert_eq!(cut.text.chars().count(), 15);
        assert_eq!(body_of(&cut, 0), Some("crème"));
    }

    #[test]
    fn a_legend_line_that_is_cut_goes() {
        let mut rendered = Rendered::from("Text.".to_owned());
        let at = rendered.text.len() + 2..rendered.text.len() + 2 + 10;
        rendered.text.push_str("\n\n0123456789");
        rendered.legend.push(Legend {
            line: "0123456789".to_owned(),
            at,
        });
        assert_eq!(rendered.clone().cut(100).legend.len(), 1);
        assert!(rendered.cut(10).legend.is_empty());
    }
}
