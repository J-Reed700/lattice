//! Streaming filter stripping unmatched `</think>` tokens and hybrid replay artifacts.

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

/// Strips unmatched `</think>` wrappers without dropping real text or explicit
/// `<think>...</think>` blocks:
/// - Leading `</think>` tokens (before any `<think>` or public text) are stripped.
/// - Mid-stream unmatched `</think>` outside `<think>...</think>` is stripped
///   along with its suffix ONLY when that suffix duplicates the preceding
///   public output (a hybrid-model replay artifact when `enable_thinking: false`
///   closed `<think></think>` inside the prompt).
///
/// Returns `None` when `content` needs no repair.
pub(crate) fn strip_unmatched_think_close(content: &str) -> Option<String> {
    if !content.contains(THINK_CLOSE) {
        return None;
    }
    let mut filter = ContentFilter::default();
    let mut out = filter.push(content);
    out.push_str(&filter.flush());
    if filter.closed_unmatched_think() {
        let trimmed_len = out.trim_end().len();
        out.truncate(trimmed_len);
    }
    (out != content).then_some(out)
}

fn trailing_prefix_len(haystack: &str, needle: &str) -> usize {
    let max_len = haystack.len().min(needle.len().saturating_sub(1));
    for len in (1..=max_len).rev() {
        let start = haystack.len() - len;
        if haystack.is_char_boundary(start) && needle.starts_with(&haystack[start..]) {
            return len;
        }
    }
    0
}

fn trailing_tag_prefix_len(haystack: &str) -> usize {
    trailing_prefix_len(haystack, THINK_OPEN).max(trailing_prefix_len(haystack, THINK_CLOSE))
}

fn normalize_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DuplicateMatch {
    Pending,
    Confirmed,
    Diverged,
}

/// Compares text emitted after an unmatched `</think>` against the normalized
/// public text before it (`before_norm`).
fn compare_duplicate(before_norm: &str, after: &str, at_eof: bool) -> DuplicateMatch {
    if before_norm.is_empty() {
        return DuplicateMatch::Diverged;
    }
    let after_norm = normalize_ws(after);
    let mut cursor = after_norm.as_str();
    let mut matched_copies = 0usize;
    loop {
        cursor = cursor.trim_start();
        while let Some(next) = cursor.strip_prefix(THINK_CLOSE) {
            cursor = next.trim_start();
        }
        if cursor.is_empty() {
            if !at_eof {
                return DuplicateMatch::Pending;
            }
            return if matched_copies >= 1 {
                DuplicateMatch::Confirmed
            } else {
                DuplicateMatch::Diverged
            };
        }
        if let Some(tail) = cursor.strip_prefix(before_norm) {
            if !tail.is_empty()
                && !tail.starts_with(char::is_whitespace)
                && !tail.starts_with(THINK_CLOSE)
            {
                return DuplicateMatch::Diverged;
            }
            matched_copies += 1;
            cursor = tail;
            continue;
        }
        if before_norm.starts_with(cursor) {
            if !at_eof {
                return DuplicateMatch::Pending;
            }
            let truncated_replay = matched_copies >= 1
                || (cursor.len() >= 16 && cursor.split_whitespace().count() >= 3);
            return if truncated_replay {
                DuplicateMatch::Confirmed
            } else {
                DuplicateMatch::Diverged
            };
        }
        return DuplicateMatch::Diverged;
    }
}

#[derive(Default)]
enum ContentMode {
    #[default]
    Leading,
    Filtering,
    AfterClose(AfterCloseState),
    Closed,
}

struct AfterCloseState {
    before_norm: String,
    after: String,
}

#[derive(Default)]
pub(super) struct ContentFilter {
    mode: ContentMode,
    pending: String,
    stripped_leading_close: bool,
    in_think: bool,
    public_before: String,
}

impl ContentFilter {
    pub(super) fn closed_unmatched_think(&self) -> bool {
        matches!(self.mode, ContentMode::Closed)
    }

    pub(super) fn push(&mut self, chunk: &str) -> String {
        if chunk.is_empty() {
            return String::new();
        }
        match self.mode {
            ContentMode::Closed => String::new(),
            ContentMode::Filtering => self.filter_body(chunk),
            ContentMode::AfterClose(_) => self.push_after_close(chunk),
            ContentMode::Leading => {
                self.pending.push_str(chunk);
                loop {
                    if self.stripped_leading_close {
                        let trimmed_breaks = self.pending.trim_start_matches(['\r', '\n']);
                        if trimmed_breaks.is_empty() {
                            self.pending.clear();
                            return String::new();
                        }
                        if trimmed_breaks.len() != self.pending.len() {
                            self.pending = trimmed_breaks.to_owned();
                        }
                        self.stripped_leading_close = false;
                    }
                    let trimmed = self.pending.trim_start();
                    if trimmed.is_empty() {
                        return String::new();
                    }
                    if trimmed.starts_with(THINK_OPEN) {
                        self.mode = ContentMode::Filtering;
                        let buffered = std::mem::take(&mut self.pending);
                        return self.filter_body(&buffered);
                    }
                    if let Some(after) = trimmed.strip_prefix(THINK_CLOSE) {
                        self.pending = after.trim_start_matches(['\r', '\n']).to_owned();
                        self.stripped_leading_close = self.pending.is_empty();
                        continue;
                    }
                    if THINK_OPEN.starts_with(trimmed) || THINK_CLOSE.starts_with(trimmed) {
                        return String::new();
                    }
                    self.mode = ContentMode::Filtering;
                    let buffered = std::mem::take(&mut self.pending);
                    return self.filter_body(&buffered);
                }
            }
        }
    }

    fn filter_body(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        let mut out = String::new();
        loop {
            if self.pending.is_empty() {
                return out;
            }
            if self.in_think {
                if let Some(idx) = self.pending.find(THINK_CLOSE) {
                    let end = idx + THINK_CLOSE.len();
                    out.push_str(&self.pending[..end]);
                    self.pending.drain(..end);
                    self.in_think = false;
                    continue;
                }
                let hold = trailing_prefix_len(&self.pending, THINK_CLOSE);
                let emit_len = self.pending.len() - hold;
                if emit_len > 0 {
                    out.push_str(&self.pending[..emit_len]);
                    self.pending.drain(..emit_len);
                }
                return out;
            }
            let open_idx = self.pending.find(THINK_OPEN);
            let close_idx = self.pending.find(THINK_CLOSE);
            let next_tag = match (open_idx, close_idx) {
                (Some(open), Some(close)) => {
                    if open < close {
                        Some((true, open))
                    } else {
                        Some((false, close))
                    }
                }
                (Some(open), None) => Some((true, open)),
                (None, Some(close)) => Some((false, close)),
                (None, None) => None,
            };
            match next_tag {
                Some((true, open)) => {
                    let end = open + THINK_OPEN.len();
                    let before_tag = &self.pending[..open];
                    self.public_before.push_str(before_tag);
                    out.push_str(&self.pending[..end]);
                    self.pending.drain(..end);
                    self.in_think = true;
                }
                Some((false, close)) => {
                    let end = close + THINK_CLOSE.len();
                    let before_tag = &self.pending[..close];
                    self.public_before.push_str(before_tag);
                    out.push_str(before_tag);
                    self.pending.drain(..end);
                    let rest = std::mem::take(&mut self.pending);
                    self.mode = ContentMode::AfterClose(AfterCloseState {
                        before_norm: normalize_ws(&self.public_before),
                        after: String::new(),
                    });
                    out.push_str(&self.push_after_close(&rest));
                    return out;
                }
                None => {
                    let hold = trailing_tag_prefix_len(&self.pending);
                    let emit_len = self.pending.len() - hold;
                    if emit_len > 0 {
                        let piece = &self.pending[..emit_len];
                        self.public_before.push_str(piece);
                        out.push_str(piece);
                        self.pending.drain(..emit_len);
                    }
                    return out;
                }
            }
        }
    }

    fn push_after_close(&mut self, chunk: &str) -> String {
        let ContentMode::AfterClose(state) = &mut self.mode else {
            return String::new();
        };
        state.after.push_str(chunk);
        match compare_duplicate(&state.before_norm, &state.after, false) {
            DuplicateMatch::Pending => String::new(),
            DuplicateMatch::Confirmed => {
                self.mode = ContentMode::Closed;
                String::new()
            }
            DuplicateMatch::Diverged => self.resume_after_diverged(),
        }
    }

    fn resume_after_diverged(&mut self) -> String {
        let ContentMode::AfterClose(state) =
            std::mem::replace(&mut self.mode, ContentMode::Filtering)
        else {
            return String::new();
        };
        self.public_before.push_str(THINK_CLOSE);
        let mut out = String::from(THINK_CLOSE);
        out.push_str(&self.filter_body(&state.after));
        out
    }

    pub(super) fn flush(&mut self) -> String {
        match &self.mode {
            ContentMode::Closed => {
                self.pending.clear();
                String::new()
            }
            ContentMode::AfterClose(state) => {
                match compare_duplicate(&state.before_norm, &state.after, true) {
                    DuplicateMatch::Confirmed => {
                        self.pending.clear();
                        self.mode = ContentMode::Closed;
                        String::new()
                    }
                    DuplicateMatch::Pending | DuplicateMatch::Diverged => {
                        let mut out = self.resume_after_diverged();
                        out.push_str(&self.flush());
                        out
                    }
                }
            }
            ContentMode::Leading | ContentMode::Filtering => std::mem::take(&mut self.pending),
        }
    }
}
