//! How pieces are chosen under a budget: an exact tiling, or overlapping
//! windows.

use super::{Boundaries, Break, Measure, Span, Strength};

/// One step of a ladder: cut at the latest break of at least `at_least`
/// that leaves a piece of at least `floor` of the plan's `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rung {
    pub at_least: Strength,
    pub floor: f64,
}

/// What a piece does when no rung finds a break within the budget: `Cut` at
/// the budget, or `Extend` to the next break as strong as the last rung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    Cut,
    Extend,
}

/// A piece is at most `max` of the measure. The ladder is tried in order and
/// the first rung that finds a break decides the cut.
#[derive(Clone, Copy, Debug)]
pub struct Plan<'a> {
    pub max: usize,
    pub ladder: &'a [Rung],
    pub overflow: Overflow,
}

/// `text` as consecutive pieces that concatenate to it, each cut by `plan`.
/// A piece is never empty and, under `Cut`, never over budget unless one
/// char alone is.
pub fn partition<B: Boundaries + ?Sized, M: Measure + ?Sized>(
    text: &str,
    boundaries: &B,
    measure: &M,
    plan: &Plan,
) -> Vec<Span> {
    let cuts = Cuts::new(text, boundaries);
    let mut spans = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = cuts.end_of(measure, plan, start);
        spans.push(start..end);
        start = end;
    }
    spans
}

/// Overlapping windows of `text`, each cut as `partition` cuts and trimmed
/// of surrounding whitespace. The next one starts about `step` further on,
/// at the latest break of the ladder's weakest strength up to there, or at
/// the first one after it within the window. Starts increase, and every
/// non-whitespace byte is in some window.
pub fn windows<B: Boundaries + ?Sized, M: Measure + ?Sized>(
    text: &str,
    boundaries: &B,
    measure: &M,
    plan: &Plan,
    step: usize,
) -> Vec<Span> {
    let cuts = Cuts::new(text, boundaries);
    let weakest = plan.ladder.iter().map(|rung| rung.at_least).min();
    let mut spans = Vec::new();
    let mut start = skip_space(text, 0);
    while start < text.len() {
        let end = cuts.end_of(measure, plan, start);
        let trimmed = start + text.get(start..end).map_or(0, |win| win.trim_end().len());
        if trimmed > start {
            spans.push(start..trimmed);
        }
        if end >= text.len() {
            break;
        }
        let step_at = measure.reach(text, start, step).clamp(start, end);
        let next = weakest
            .and_then(|weakest| {
                cuts.latest(start, step_at, weakest)
                    .or_else(|| cuts.first(step_at, end, weakest))
            })
            .unwrap_or(step_at);
        start = skip_space(text, if next > start { next } else { end });
    }
    spans
}

/// The breaks of one text, searched by offset.
struct Cuts<'a> {
    text: &'a str,
    breaks: Vec<Break>,
}

impl<'a> Cuts<'a> {
    fn new<B: Boundaries + ?Sized>(text: &'a str, boundaries: &B) -> Self {
        Self {
            text,
            breaks: boundaries.breaks(text),
        }
    }

    /// Where the piece starting at `start` ends.
    fn end_of<M: Measure + ?Sized>(&self, measure: &M, plan: &Plan, start: usize) -> usize {
        let len = self.text.len();
        let reach = measure.reach(self.text, start, plan.max).max(start);
        if reach >= len {
            return len;
        }
        for rung in plan.ladder {
            let share = (rung.floor * plan.max as f64).ceil() as usize;
            let floor = match share.checked_sub(1) {
                Some(fits) => measure.reach(self.text, start, fits).max(start),
                None => start,
            };
            if let Some(at) = self.latest(floor, reach, rung.at_least) {
                return at;
            }
        }
        match (plan.overflow, plan.ladder.last()) {
            (Overflow::Extend, Some(last)) => self.first(reach, len, last.at_least).unwrap_or(len),
            _ if reach > start => reach,
            _ => self.char_after(start),
        }
    }

    /// The latest cut `at` with `after < at <= upto` of at least `at_least`.
    fn latest(&self, after: usize, upto: usize, at_least: Strength) -> Option<usize> {
        if upto <= after {
            return None;
        }
        if at_least <= Strength::Char {
            return Some(upto);
        }
        self.within(after, upto)
            .iter()
            .rev()
            .find(|cut| cut.strength >= at_least)
            .map(|cut| cut.at)
    }

    /// The first cut `at` with `after < at <= upto` of at least `at_least`.
    fn first(&self, after: usize, upto: usize, at_least: Strength) -> Option<usize> {
        if upto <= after {
            return None;
        }
        if at_least <= Strength::Char {
            return Some(self.char_after(after)).filter(|&at| at <= upto);
        }
        self.within(after, upto)
            .iter()
            .find(|cut| cut.strength >= at_least)
            .map(|cut| cut.at)
    }

    /// The breaks `at` with `after < at <= upto`.
    fn within(&self, after: usize, upto: usize) -> &[Break] {
        let from = self.breaks.partition_point(|cut| cut.at <= after);
        let to = self.breaks.partition_point(|cut| cut.at <= upto);
        self.breaks.get(from..to).unwrap_or_default()
    }

    /// The offset one char past `at`, or the end of the text.
    fn char_after(&self, at: usize) -> usize {
        self.text
            .get(at..)
            .and_then(|rest| rest.chars().next())
            .map_or(self.text.len(), |c| at + c.len_utf8())
    }
}

/// The first byte at or after `at` that is not whitespace.
fn skip_space(text: &str, at: usize) -> usize {
    text.get(at..)
        .map_or(text.len(), |rest| at + rest.len() - rest.trim_start().len())
}
