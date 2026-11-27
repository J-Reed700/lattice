//! The schema and the reading of every shape. The reading rules are the same
//! for all of them: a statement is used only if it closed; what is missing is
//! `Missing` only in an answer read to its end.

use super::super::shown::{Kind, Picked, Showing};
use super::form::{Among, Field, Form};
use super::{Rows, Settled, Why};

mod keeps;
mod reader;
mod satisfiable;
mod schema;

pub(super) use keeps::read_keeps;
pub(super) use reader::{
    read_change, read_each, read_flagged, read_lead, read_new, read_pick, read_pick_some,
    read_value,
};
pub(super) use schema::{
    change_schema, each_schema, flagged_schema, new_schema, pick_schema, pick_some_schema,
    value_schema,
};

/// What the entries of one item say together: equal entries count once,
/// differing ones contradict. `None` when there is no entry.
fn join<K: Kind, V: PartialEq>(item: &Picked<K>, entries: Vec<Settled<V>>) -> Option<Settled<V>> {
    let mut answered: Option<V> = None;
    let mut unread: Option<Why> = None;
    for entry in entries {
        match entry {
            Settled::Answered(value) => match answered.as_ref() {
                Some(first) if *first != value => {
                    return Some(Settled::Unanswered(Why::Contradicted(format!(
                        "{} has two entries that differ.",
                        item.handle
                    ))))
                }
                Some(_) => {}
                None => answered = Some(value),
            },
            Settled::Unanswered(why) => {
                if unread.is_none() {
                    unread = Some(why);
                }
            }
        }
    }
    match (answered, unread) {
        (Some(value), _) => Some(Settled::Answered(value)),
        (None, Some(why)) => Some(Settled::Unanswered(why)),
        (None, None) => None,
    }
}

/// Every shown item, once, in shown order, each with the entries that name it.
fn per_item<K: Kind, V: PartialEq, T>(
    shown: Vec<Picked<K>>,
    mut entries: Rows<K, V>,
    mut settle: impl FnMut(Option<Settled<V>>) -> Settled<T>,
) -> Rows<K, T> {
    shown
        .into_iter()
        .map(|item| {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut entries)
                .into_iter()
                .partition(|(entry, _)| *entry == item);
            entries = rest;
            let mine = mine.into_iter().map(|(_, settled)| settled).collect();
            let settled = settle(join(&item, mine));
            (item, settled)
        })
        .collect()
}

fn all_unanswered<K: Kind, T>(shown: Vec<Picked<K>>, why: &Why) -> Rows<K, T> {
    shown
        .into_iter()
        .map(|item| (item, Settled::Unanswered(why.clone())))
        .collect()
}

/// Whether a line is the word for absence, in any case and with whatever
/// punctuation or markup surrounds it.
fn says_none(line: &str) -> bool {
    line.trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .eq_ignore_ascii_case("none")
}
