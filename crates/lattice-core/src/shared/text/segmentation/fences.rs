//! Fenced code blocks of a Markdown text: the one recognizer.

use std::ops::Range;

/// One fenced code block of a Markdown text, by byte ranges of that text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFence {
    /// The opening line, its line break included.
    pub opening: Range<usize>,
    /// The info string's first word ("rust" in "```rust title"); empty when there is none.
    pub language: String,
    /// The lines between the opening and the closing line, line breaks included.
    pub text: Range<usize>,
    /// The closing line, its line break included; None when the text ends first.
    pub closing: Option<Range<usize>>,
}

impl CodeFence {
    /// From the opening line's start to the closing line's end (or the text's end).
    pub fn whole(&self) -> Range<usize> {
        let end = self
            .closing
            .as_ref()
            .map_or(self.text.end, |closing| closing.end);
        self.opening.start..end
    }
}

/// Every fenced code block of `markdown`, in order.
pub fn code_fences(markdown: &str) -> Vec<CodeFence> {
    let mut fences = Vec::new();
    let mut open: Option<(Fence, CodeFence)> = None;
    let mut start = 0;
    for line in markdown.split_inclusive('\n') {
        let end = start + line.len();
        open = match open.take() {
            Some((fence, mut found)) if fence.closed_by(line) => {
                found.text.end = start;
                found.closing = Some(start..end);
                fences.push(found);
                None
            }
            Some(pending) => Some(pending),
            None => Fence::opened_by(line).map(|fence| {
                let found = CodeFence {
                    opening: start..end,
                    language: fence.language.clone(),
                    text: end..end,
                    closing: None,
                };
                (fence, found)
            }),
        };
        start = end;
    }
    if let Some((_, mut found)) = open {
        found.text.end = markdown.len();
        fences.push(found);
    }
    fences
}

/// Whether byte `at` lies in a fence, its opening and closing lines included.
pub fn in_code_fence(markdown: &str, at: usize) -> bool {
    code_fences(markdown)
        .iter()
        .any(|fence| fence.whole().contains(&at))
}

/// An open code fence: its marker character, run length and language.
struct Fence {
    marker: char,
    len: usize,
    language: String,
}

impl Fence {
    fn opened_by(line: &str) -> Option<Self> {
        let content = block_content(line);
        let marker = content.chars().next()?;
        if marker != '`' && marker != '~' {
            return None;
        }
        let info = content.trim_start_matches(marker);
        let len = content.len() - info.len();
        if len < 3 {
            return None;
        }
        // A backtick fence's info string cannot itself hold a backtick.
        if marker == '`' && info.contains('`') {
            return None;
        }
        let language = info.split_whitespace().next().unwrap_or_default();
        Some(Self {
            marker,
            len,
            language: language.to_owned(),
        })
    }

    fn closed_by(&self, line: &str) -> bool {
        let content = block_content(line);
        let rest = content.trim_start_matches(self.marker);
        content.len() - rest.len() >= self.len && rest.trim().is_empty()
    }
}

/// The line with its indentation, blockquote markers and one list marker
/// removed: a fence inside a list item or a blockquote is still a fence.
pub(crate) fn block_content(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    if let Some(after) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        return after.trim_start();
    }
    let after = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if after.len() < rest.len() {
        if let Some(after) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return after.trim_start();
        }
    }
    rest
}
