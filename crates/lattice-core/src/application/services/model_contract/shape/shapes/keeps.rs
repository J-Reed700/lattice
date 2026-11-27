//! Reading plain-text [`Keeps`](super::super::Keeps) answers line by line.

use super::super::super::shown::{Kind, Name, Picked, Showing};
use super::super::fields::trace;
use super::super::{Ended, Rows, Settled, Why, Written};
use super::{all_unanswered, per_item, says_none};

const RANGE_SEPARATORS: [&str; 7] = ["-", "--", "–", "—", "..", "...", "…"];

/// The runs of ASCII letters and digits in a line, with where each starts.
fn tokens_of(line: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut start = None;
    for (at, c) in line.char_indices() {
        match (c.is_ascii_alphanumeric(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                found.push((from, &line[from..at]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push((from, &line[from..]));
    }
    found
}

/// Whether what stands between token `at` and the next one is a range sign.
fn range_follows(line: &str, tokens: &[(usize, &str)], at: usize) -> bool {
    let (Some(&(start, token)), Some(&(next, _))) = (tokens.get(at), tokens.get(at + 1)) else {
        return false;
    };
    let Some(between) = line.get(start + token.len()..next) else {
        return false;
    };
    let between: String = between.chars().filter(|c| !c.is_whitespace()).collect();
    RANGE_SEPARATORS.contains(&between.as_str())
}

fn all_digits(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit())
}

/// An entry being read: the item and what has been said under it so far.
struct Entry<K: Kind, C: Kind> {
    item: Picked<K>,
    kept: Vec<Picked<C>>,
    unplaced: bool,
    names_own: bool,
    says_none: bool,
    other_words: bool,
}

impl<K: Kind, C: Kind> Entry<K, C> {
    fn new(item: Picked<K>) -> Self {
        Self {
            item,
            kept: Vec::new(),
            unplaced: false,
            names_own: false,
            says_none: false,
            other_words: false,
        }
    }

    fn keep(&mut self, showing: &Showing<'_>, first: u32, last: u32) {
        self.names_own = true;
        if self.unplaced {
            return;
        }
        if last < first {
            trace("unplaced-keep", Some("range runs backwards"));
            self.unplaced = true;
            return;
        }
        for ordinal in first..=last {
            let written = format!("{}{ordinal}", C::PREFIX);
            match showing.resolve_under::<C>(self.item.handle.name(), &written) {
                Some(own) => {
                    if !self.kept.contains(&own) {
                        self.kept.push(own);
                    }
                }
                None => {
                    trace("unplaced-keep", Some(written.as_str()));
                    self.unplaced = true;
                    return;
                }
            }
        }
    }

    fn into_row(mut self) -> (Picked<K>, Settled<Vec<Picked<C>>>) {
        let settled = self.settled();
        (self.item, settled)
    }

    fn settled(&mut self) -> Settled<Vec<Picked<C>>> {
        if self.unplaced {
            Settled::Unanswered(Why::Unplaced)
        } else if self.says_none && self.names_own {
            Settled::Unanswered(Why::Contradicted(format!(
                "{} both says none and names what it keeps.",
                self.item.handle
            )))
        } else if !self.names_own && self.other_words {
            Settled::Unanswered(Why::NotRead)
        } else {
            Settled::Answered(std::mem::take(&mut self.kept))
        }
    }
}

/// The lines of an answer read one by one. An item named on a line that holds
/// none of its own handles stays open: the lines after it that name no item are
/// its own.
struct KeepsReader<'a, 'b, K: Kind, C: Kind> {
    showing: &'a Showing<'b>,
    entries: Rows<K, Vec<Picked<C>>>,
    open: Option<Entry<K, C>>,
    stray: bool,
}

impl<K: Kind, C: Kind> KeepsReader<'_, '_, K, C> {
    fn close(&mut self, entry: Option<Entry<K, C>>) {
        if let Some(entry) = entry {
            self.entries.push(entry.into_row());
        }
    }

    fn unplace_item(&mut self, word: &str) {
        trace("unplaced-entry", Some(word));
        self.stray = true;
        if let Some(item) = self.showing.resolve::<K>(word) {
            self.entries
                .push((item, Settled::Unanswered(Why::Unplaced)));
        }
    }

    fn line(&mut self, line: &str) {
        let tokens = tokens_of(line);
        let words: Vec<String> = tokens
            .iter()
            .map(|(_, token)| token.to_ascii_lowercase())
            .collect();
        let prefix_of = |word: &str| Name::pointed(word).map(|name| name.prefix);
        let names_item = words
            .iter()
            .any(|word| prefix_of(word).as_deref() == Some(K::PREFIX));
        let mut current = if names_item {
            let open = self.open.take();
            self.close(open);
            None
        } else {
            self.open.take()
        };
        let mut at = 0;
        while let Some(word) = words.get(at).map(String::as_str) {
            let next = words.get(at + 1).map(String::as_str);
            let ranged = range_follows(line, &tokens, at);
            match prefix_of(word) {
                Some(prefix) if prefix == K::PREFIX => {
                    self.close(current.take());
                    let to_item = next.filter(|n| prefix_of(n).as_deref() == Some(K::PREFIX));
                    if ranged && (to_item.is_some() || next.is_some_and(all_digits)) {
                        self.unplace_item(word);
                        if let Some(second) = to_item {
                            self.unplace_item(second);
                        }
                        at += 2;
                        continue;
                    }
                    match self.showing.resolve::<K>(word) {
                        Some(item) => current = Some(Entry::new(item)),
                        None => {
                            trace("unplaced-entry", Some(word));
                            self.stray = true;
                        }
                    }
                }
                Some(prefix) if prefix == C::PREFIX => {
                    let first = Name::pointed(word).map_or(0, |name| name.ordinal);
                    let to_own = next
                        .filter(|n| prefix_of(n).as_deref() == Some(C::PREFIX))
                        .and_then(Name::pointed)
                        .map(|name| name.ordinal);
                    let to_number = next
                        .filter(|n| all_digits(n))
                        .map(|n| n.parse().unwrap_or(u32::MAX));
                    let last = if ranged { to_own.or(to_number) } else { None };
                    match current.as_mut() {
                        Some(entry) => entry.keep(self.showing, first, last.unwrap_or(first)),
                        None => {
                            trace("unplaced-keep", Some(word));
                            self.stray = true;
                        }
                    }
                    if last.is_some() {
                        at += 2;
                        continue;
                    }
                }
                _ => {
                    if let Some(entry) = current.as_mut() {
                        if word == "none" {
                            entry.says_none = true;
                        } else {
                            entry.other_words = true;
                        }
                    }
                }
            }
            at += 1;
        }
        match current {
            Some(entry) if !names_item || !entry.names_own => self.open = Some(entry),
            other => self.close(other),
        }
    }

    fn finish(&mut self, complete: bool) {
        let Some(entry) = self.open.take() else {
            return;
        };
        if complete {
            self.close(Some(entry));
        } else {
            self.entries
                .push((entry.item, Settled::Unanswered(Why::CutOff)));
        }
    }
}

pub(in crate::application::services::model_contract::shape) fn read_keeps<K: Kind, C: Kind>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Rows<K, Vec<Picked<C>>> {
    let shown = showing.shown::<K>();
    if written.ended == Ended::Declined {
        return all_unanswered(shown, &Why::Declined);
    }
    let complete = written.ended == Ended::Complete;
    let mut lines: Vec<&str> = written
        .text
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if !complete {
        lines.pop();
    }
    let mut reader = KeepsReader::<K, C> {
        showing,
        entries: Vec::new(),
        open: None,
        stray: false,
    };
    let mut none_said = false;
    for line in &lines {
        none_said |= says_none(line);
        reader.line(line);
    }
    reader.finish(complete);
    let KeepsReader { entries, stray, .. } = reader;
    let unnamed = if !complete {
        Some(Why::CutOff)
    } else if stray {
        Some(Why::Unplaced)
    } else if entries.is_empty() && !none_said {
        Some(Why::NotRead)
    } else {
        None
    };
    per_item(shown, entries, |joined| match joined {
        Some(settled) => settled,
        None => match &unnamed {
            Some(why) => Settled::Unanswered(why.clone()),
            None => Settled::Answered(Vec::new()),
        },
    })
}
