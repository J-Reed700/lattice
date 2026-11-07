//! What became of a text after an edit: the sentences of two versions
//! matched, and the fate of any span of the old one (design §2f).

use std::ops::Range;

use super::{sentences, Span};

/// The longest common subsequence of two lists, as matched index pairs in
/// order.
pub fn common_subsequence<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let width = b.len() + 1;
    let mut table = vec![0usize; (a.len() + 1) * width];
    let cell = |table: &[usize], i: usize, j: usize| table.get(i * width + j).copied().unwrap_or(0);
    for (i, x) in a.iter().enumerate().rev() {
        for (j, y) in b.iter().enumerate().rev() {
            let value = if x == y {
                cell(&table, i + 1, j + 1) + 1
            } else {
                cell(&table, i + 1, j).max(cell(&table, i, j + 1))
            };
            if let Some(slot) = table.get_mut(i * width + j) {
                *slot = value;
            }
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        if x == y {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if cell(&table, i + 1, j) >= cell(&table, i, j + 1) {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// What became of a span of the old text, as a span of the new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fate {
    /// The same text, at this place.
    Kept(Span),
    /// Changed: what stands in its place.
    Rewritten(Span),
    /// Gone.
    Removed,
}

/// Two versions of a text with their sentences matched.
#[derive(Clone, Debug)]
pub struct Alignment {
    old: String,
    new: String,
    /// The old sentences, trimmed, with what became of each.
    entries: Vec<(Span, Fate)>,
    hunks: Vec<(Span, Span)>,
}

/// `old` and `new` with their sentences matched by `common_subsequence`. A
/// matched sentence is `Kept` at its new place. The unmatched sentences
/// between two matches (or before the first, or after the last) form a hunk
/// with the unmatched new ones between the same matches, or, when both sides
/// hold as many, one hunk per pair. Old sentences in a hunk with no new side
/// are `Removed`, the others `Rewritten` into the hunk's new span.
pub fn align(old: &str, new: &str) -> Alignment {
    let (old_spans, new_spans) = (sentences(old), sentences(new));
    let matches = common_subsequence(&slices(old, &old_spans), &slices(new, &new_spans));
    let mut fates = vec![Fate::Removed; old_spans.len()];
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    let ends = (old_spans.len(), new_spans.len());
    for (next_i, next_j) in matches.iter().copied().chain(std::iter::once(ends)) {
        let (old_count, new_count) = (next_i.saturating_sub(i), next_j.saturating_sub(j));
        let gaps: Vec<(Range<usize>, Range<usize>)> = if old_count == new_count {
            (0..old_count)
                .map(|k| (i + k..i + k + 1, j + k..j + k + 1))
                .collect()
        } else {
            vec![(i..next_i, j..next_j)]
        };
        for (olds, news) in gaps {
            let written = region(&new_spans, &news, new.len());
            for fate in fates.get_mut(olds.clone()).into_iter().flatten() {
                *fate = if news.is_empty() {
                    Fate::Removed
                } else {
                    Fate::Rewritten(written.clone())
                };
            }
            hunks.push((region(&old_spans, &olds, old.len()), written));
        }
        if let (Some(fate), Some(&(start, end))) = (fates.get_mut(next_i), new_spans.get(next_j)) {
            *fate = Fate::Kept(start..end);
        }
        (i, j) = (next_i + 1, next_j + 1);
    }
    Alignment {
        old: old.to_owned(),
        new: new.to_owned(),
        entries: old_spans
            .into_iter()
            .zip(fates)
            .map(|((start, end), fate)| (start..end, fate))
            .collect(),
        hunks,
    }
}

impl Alignment {
    /// The changes, in order: old text and the new text that took its place.
    /// A removal's new span and an insertion's old span are empty, before the
    /// next sentence of that text (at its end when none follows).
    pub fn hunks(&self) -> &[(Span, Span)] {
        &self.hunks
    }

    /// What became of `old`, a span of the old text. It counts only the old
    /// sentences it overlaps, so the whitespace around them is nothing. It is
    /// `Removed` when none of them survived; else its new span runs from the
    /// first surviving image to the last, keeping the offset where it starts
    /// or ends inside a kept sentence, so a fragment maps to the same
    /// fragment. It is `Kept` when every sentence it overlaps was kept and
    /// the new text is the old text, which an insertion between two kept
    /// sentences breaks; else `Rewritten`.
    pub fn fate(&self, old: Span) -> Fate {
        let stop = old.end.max(old.start);
        let first = self
            .entries
            .partition_point(|(span, _)| span.end <= old.start);
        let after = self.entries.partition_point(|(span, _)| span.start < stop);
        let Some(touched) = self.entries.get(first..after) else {
            return Fate::Removed;
        };
        let alive = |entry: &&(Span, Fate)| !matches!(entry.1, Fate::Removed);
        let (Some((head, _)), Some((tail, _))) = (touched.first(), touched.last()) else {
            return Fate::Removed;
        };
        let (Some((front, front_fate)), Some((back, back_fate))) =
            (touched.iter().find(alive), touched.iter().rev().find(alive))
        else {
            return Fate::Removed;
        };
        let (from, to) = (old.start.max(head.start), stop.min(tail.end));
        let image = |fate: &Fate| match fate {
            Fate::Kept(span) | Fate::Rewritten(span) => span.clone(),
            Fate::Removed => 0..0,
        };
        let mut start = image(front_fate).start;
        if front == head && matches!(front_fate, Fate::Kept(_)) {
            start += from.saturating_sub(head.start);
        }
        let mut end = image(back_fate).end;
        if back == tail && matches!(back_fate, Fate::Kept(_)) {
            end = end.saturating_sub(tail.end.saturating_sub(to));
        }
        let span = start..end;
        let unchanged = touched
            .iter()
            .all(|(_, fate)| matches!(fate, Fate::Kept(_)))
            && self.new.get(span.clone()) == self.old.get(from..to);
        if unchanged {
            Fate::Kept(span)
        } else {
            Fate::Rewritten(span)
        }
    }
}

/// The sentences of `text` at `spans`.
fn slices<'t>(text: &'t str, spans: &[(usize, usize)]) -> Vec<&'t str> {
    spans
        .iter()
        .map(|&(start, end)| text.get(start..end).unwrap_or_default())
        .collect()
}

/// The text of sentences `range` of `spans`; when `range` is empty, the empty
/// span where they would stand: before the next sentence, or at `len`.
fn region(spans: &[(usize, usize)], range: &Range<usize>, len: usize) -> Span {
    let first = spans.get(range.start);
    let last = range.end.checked_sub(1).and_then(|last| spans.get(last));
    match (first, last) {
        (Some(&(start, _)), Some(&(_, end))) if !range.is_empty() => start..end,
        _ => {
            let at = first.map_or(len, |&(start, _)| start);
            at..at
        }
    }
}
