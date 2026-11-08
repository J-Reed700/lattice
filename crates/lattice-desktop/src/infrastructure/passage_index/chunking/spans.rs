//! The span chunker: prose cut into overlapping windows of whole sentences,
//! addressed by byte.
//!
//! A window holds up to `chunk_chars` characters and the next one starts
//! about `step_chars` further on, the widths the reference collection has
//! always ranked by. Both cuts move to sentence edges: a passage ends after
//! the last sentence that fits in its window, and the next passage starts at
//! the start of the sentence holding the step point. So a passage the claim
//! checker reads is whole sentences, never half of one at either end, and two
//! neighbours overlap by whole sentences. A window with no sentence end in it
//! (one run-on longer than the window, a table, code) is cut at its edge, as
//! a fixed window would be.
//!
//! Offsets are bytes into the text, always on character boundaries, so a
//! passage maps onto the source's bytes with no relocation.

use super::NewPassage;

/// Characters that close a sentence when whitespace or the end of the text
/// follows them (a full stop inside `3.14` or `file.rs` closes nothing).
const TERMINATORS: [char; 4] = ['.', '!', '?', '…'];
/// Full-width terminators close a sentence with nothing after them: the
/// scripts that use them put no space between sentences.
const FULL_WIDTH_TERMINATORS: [char; 3] = ['。', '！', '？'];
/// Closing quotes and brackets that belong to the sentence they follow.
const CLOSERS: [char; 8] = ['"', '\'', '”', '’', '»', ')', ']', '」'];

/// The passages of `text`: windows of at most `chunk_chars` characters,
/// starting about every `step_chars`, cut at sentence edges. Whitespace
/// between passages is left out; nothing else is.
pub fn span_chunks(text: &str, chunk_chars: usize, step_chars: usize) -> Vec<NewPassage> {
    let chunk_chars = chunk_chars.max(1);
    let step_chars = step_chars.clamp(1, chunk_chars);
    let mut passages = Vec::new();
    let mut start = skip_space(text, 0);
    while start < text.len() {
        let window = Window::scan(text, start, chunk_chars, step_chars);
        let Some(edge) = window.edge else {
            push(&mut passages, text, start, text.len());
            break;
        };
        let end = window.sentence_ends.last().copied().unwrap_or(edge);
        push(&mut passages, text, start, end);
        start = skip_space(text, window.next_start(text, start));
    }
    passages
}

/// What one scan from a passage's start found.
struct Window {
    /// Where the window's characters run out; `None` when the text ends
    /// inside it.
    edge: Option<usize>,
    /// The step point: where the next window would start were there no
    /// sentences to respect.
    step: Option<usize>,
    /// Ends of the sentences that finish inside the window, in order.
    sentence_ends: Vec<usize>,
}

impl Window {
    fn scan(text: &str, start: usize, chunk_chars: usize, step_chars: usize) -> Self {
        let mut window = Self {
            edge: None,
            step: None,
            sentence_ends: Vec::new(),
        };
        for (ordinal, (offset, c)) in text[start..].char_indices().enumerate() {
            let at = start + offset;
            if ordinal == step_chars {
                window.step = Some(at);
            }
            if ordinal == chunk_chars {
                window.edge = Some(at);
                break;
            }
            if let Some(end) = sentence_end(text, at, c).filter(|end| *end > start) {
                window.sentence_ends.push(end);
            }
        }
        if let Some(edge) = window.edge {
            window.sentence_ends.retain(|end| *end <= edge);
        }
        window
    }

    /// Where the passage after one starting at `start` begins: the start of
    /// the sentence holding the step point, or of the first sentence after
    /// it when that sentence began at `start`; the step point itself when
    /// the window has no sentence end. Always past `start`, and never past
    /// the end of this passage but for whitespace, so nothing is skipped.
    fn next_start(&self, text: &str, start: usize) -> usize {
        let step = self.step.or(self.edge).unwrap_or(text.len());
        let starts = self
            .sentence_ends
            .iter()
            .map(|end| skip_space(text, *end))
            .filter(|next| *next > start && *next < text.len());
        let mut before = None;
        let mut after = None;
        for next in starts {
            if next <= step {
                before = Some(next);
            } else if after.is_none() {
                after = Some(next);
            }
        }
        before.or(after).unwrap_or(step)
    }
}

/// The end of a sentence closed by `c` at byte `at`, or `None` when `c`
/// closes nothing there. A line break ends a sentence before it, so a
/// heading or a list item is a sentence of its own.
fn sentence_end(text: &str, at: usize, c: char) -> Option<usize> {
    if c == '\n' {
        return Some(trim_end(text, 0, at));
    }
    let full_width = FULL_WIDTH_TERMINATORS.contains(&c);
    if !full_width && !TERMINATORS.contains(&c) {
        return None;
    }
    let mut end = at + c.len_utf8();
    for next in text[end..].chars() {
        if !CLOSERS.contains(&next) {
            break;
        }
        end += next.len_utf8();
    }
    let followed_by_space = text[end..].chars().next().is_none_or(char::is_whitespace);
    (full_width || followed_by_space).then_some(end)
}

/// The first byte at or after `at` that is not whitespace.
fn skip_space(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(text.len(), |(offset, _)| at + offset)
}

/// `end` moved back over trailing whitespace, not before `start`.
fn trim_end(text: &str, start: usize, end: usize) -> usize {
    start + text[start..end].trim_end().len()
}

fn push(passages: &mut Vec<NewPassage>, text: &str, start: usize, end: usize) {
    let end = trim_end(text, start, end);
    if end > start {
        passages.push(NewPassage {
            start,
            end,
            text: text[start..end].to_string(),
        });
    }
}
