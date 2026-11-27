//! Reading JSON and lead-verdict shape answers.

use serde_json::Value;

use super::super::super::edit::{Consistent, Content};
use super::super::super::shown::{Kind, Picked, Showing};
use super::super::document::{self, Document};
use super::super::fields::trace;
use super::super::{Authored, Ended, Fields, Found, Led, Rows, Settled, Vocabulary, Why, Written};
use super::{all_unanswered, per_item, says_none};

/// Why an answer that cannot be used as a document leaves a statement unanswered.
fn unfinished(written: &Written<'_>) -> Why {
    match written.ended {
        Ended::Declined => Why::Declined,
        Ended::CutOff => Why::CutOff,
        Ended::Complete => Why::NotRead,
    }
}

/// The document an answer holds, as far as it was written.
fn open(written: &Written<'_>) -> Result<Document, Why> {
    if written.ended == Ended::Declined {
        return Err(Why::Declined);
    }
    document::read(written.text).ok_or_else(|| unfinished(written))
}

/// The document of an answer that is one statement: it must be read to its end.
fn whole(written: &Written<'_>) -> Result<Document, Why> {
    let document = open(written)?;
    if document.closed {
        Ok(document)
    } else {
        Err(unfinished(written))
    }
}

/// Why a statement the document does not hold is unanswered.
fn absent(written: &Written<'_>, document: &Document) -> Why {
    if document.closed {
        Why::Missing
    } else {
        unfinished(written)
    }
}

/// Why a field the dialect requires is not usable: a complete document that
/// lacks it broke the dialect.
fn lacking(written: &Written<'_>, document: &Document) -> Why {
    if document.closed {
        Why::NotRead
    } else {
        unfinished(written)
    }
}

/// The elements of the list field `name` that closed, also when it was the one still open.
fn listed<'d>(document: &'d Document, name: &str) -> Option<&'d [Value]> {
    match (document.fields.get(name), &document.open_list) {
        (Some(Value::Array(elements)), _) => Some(elements),
        (None, Some((open, elements))) if open == name => Some(elements),
        _ => None,
    }
}

pub(in crate::application::services::model_contract::shape) fn read_value<V: Authored>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Settled<V> {
    let document = match whole(written) {
        Ok(document) => document,
        Err(why) => return Settled::Unanswered(why),
    };
    let mut fields = Fields::of(&document.fields, showing, None);
    match V::read(&mut fields) {
        Some(value) => Settled::Answered(value),
        None => Settled::Unanswered(fields.why()),
    }
}

pub(in crate::application::services::model_contract::shape) fn read_pick<K: Kind>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Settled<Option<Picked<K>>> {
    let document = match open(written) {
        Ok(document) => document,
        Err(why) => return Settled::Unanswered(why),
    };
    let Some(value) = document.fields.get("pick") else {
        return Settled::Unanswered(lacking(written, &document));
    };
    let Some(handle) = value.as_str() else {
        return Settled::Unanswered(Why::NotRead);
    };
    if handle == "none" {
        return Settled::Answered(None);
    }
    match showing.resolve::<K>(handle) {
        Some(picked) => Settled::Answered(Some(picked)),
        None => Settled::Unanswered(Why::Unplaced),
    }
}

/// The handles of the list field `name` of a statement read whole, each
/// resolved. A handle that does not resolve leaves the statement `Unplaced`.
fn read_handles<K: Kind>(
    written: &Written<'_>,
    showing: &Showing<'_>,
    name: &str,
) -> Result<Vec<Picked<K>>, Why> {
    let document = whole(written)?;
    let Some(Value::Array(elements)) = document.fields.get(name) else {
        return Err(Why::NotRead);
    };
    let strings: Option<Vec<&str>> = elements.iter().map(Value::as_str).collect();
    let strings = strings.ok_or(Why::NotRead)?;
    strings
        .into_iter()
        .map(|handle| showing.resolve::<K>(handle).ok_or(Why::Unplaced))
        .collect()
}

pub(in crate::application::services::model_contract::shape) fn read_pick_some<K: Kind>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Settled<Vec<Picked<K>>> {
    match read_handles::<K>(written, showing, "picks") {
        Ok(handles) => {
            let mut picked: Vec<Picked<K>> = Vec::with_capacity(handles.len());
            for handle in handles {
                if !picked.contains(&handle) {
                    picked.push(handle);
                }
            }
            Settled::Answered(picked)
        }
        Err(why) => Settled::Unanswered(why),
    }
}

pub(in crate::application::services::model_contract::shape) fn read_new<V: Authored>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Found<V> {
    let document = match open(written) {
        Ok(document) => document,
        Err(why) => {
            return Found {
                items: Vec::new(),
                rest: Some(why),
                left_out: 0,
                unread_why: None,
            }
        }
    };
    let Some(elements) = listed(&document, "items") else {
        return Found {
            items: Vec::new(),
            rest: Some(lacking(written, &document)),
            left_out: 0,
            unread_why: None,
        };
    };
    let mut items = Vec::with_capacity(elements.len());
    let mut unread = None;
    for (place, element) in elements.iter().enumerate() {
        let item = element.as_object().map(|object| {
            let mut fields = Fields::of(object, showing, None).placed(place);
            V::read(&mut fields).ok_or_else(|| fields.why())
        });
        match item {
            Some(Ok(item)) => items.push(item),
            other => {
                trace("unread-item", None);
                if unread.is_none() {
                    unread = Some(other.and_then(Result::err).unwrap_or(Why::NotRead));
                }
            }
        }
    }
    let left_out = elements.len().saturating_sub(items.len());
    Found {
        items,
        rest: (!document.closed).then(|| unfinished(written)),
        left_out,
        unread_why: unread,
    }
}

/// The entries of the list field `name` that name a shown `K` under `key`,
/// each read under that item, and whether every element of the list was placed.
fn read_entries<K: Kind, V: Authored>(
    elements: &[Value],
    key: &str,
    dropped: &'static str,
    showing: &Showing<'_>,
) -> (Rows<K, V>, bool) {
    let mut entries = Vec::with_capacity(elements.len());
    let mut all_placed = true;
    for (place, element) in elements.iter().enumerate() {
        let Some(object) = element.as_object() else {
            trace("unread-item", None);
            all_placed = false;
            continue;
        };
        let Some(handle) = object.get(key).and_then(Value::as_str) else {
            trace("unread-item", None);
            all_placed = false;
            continue;
        };
        let Some(item) = showing.resolve::<K>(handle) else {
            trace(dropped, Some(handle));
            all_placed = false;
            continue;
        };
        let settled = {
            let mut fields = Fields::of(object, showing, Some(item.handle.name())).placed(place);
            match V::read(&mut fields) {
                Some(value) => Settled::Answered(value),
                None => Settled::Unanswered(fields.why()),
            }
        };
        entries.push((item, settled));
    }
    (entries, all_placed)
}

pub(in crate::application::services::model_contract::shape) fn read_each<
    K: Kind,
    V: Authored + PartialEq,
>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Rows<K, V> {
    let shown = showing.shown::<K>();
    let document = match open(written) {
        Ok(document) => document,
        Err(why) => return all_unanswered(shown, &why),
    };
    let Some(elements) = listed(&document, "entries") else {
        return all_unanswered(shown, &lacking(written, &document));
    };
    let (entries, _) = read_entries::<K, V>(elements, "of", "unplaced-entry", showing);
    let none = absent(written, &document);
    per_item(shown, entries, |joined| {
        joined.unwrap_or_else(|| Settled::Unanswered(none.clone()))
    })
}

pub(in crate::application::services::model_contract::shape) fn read_flagged<
    K: Kind,
    V: Authored + PartialEq,
>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Rows<K, Option<V>> {
    let shown = showing.shown::<K>();
    let document = match open(written) {
        Ok(document) => document,
        Err(why) => return all_unanswered(shown, &why),
    };
    let Some(elements) = listed(&document, "marks") else {
        return all_unanswered(shown, &lacking(written, &document));
    };
    let (marks, all_placed) = read_entries::<K, V>(elements, "at", "dropped-mark", showing);
    let unmarked = if !document.closed {
        Some(unfinished(written))
    } else if !all_placed {
        Some(Why::Unplaced)
    } else {
        None
    };
    per_item(shown, marks, |joined| match joined {
        Some(Settled::Answered(value)) => Settled::Answered(Some(value)),
        Some(Settled::Unanswered(why)) => Settled::Unanswered(why),
        None => match &unmarked {
            Some(why) => Settled::Unanswered(why.clone()),
            None => Settled::Answered(None),
        },
    })
}

pub(in crate::application::services::model_contract::shape) fn read_change<K: Kind, W: Content>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Settled<Consistent<K, W>> {
    let document = match whole(written) {
        Ok(document) => document,
        Err(why) => return Settled::Unanswered(why),
    };
    let mut fields = Fields::of(&document.fields, showing, None);
    match fields.change_object::<K, W>(&document.fields, true) {
        Some(change) => Settled::Answered(change),
        None => Settled::Unanswered(fields.why()),
    }
}

pub(in crate::application::services::model_contract::shape) fn read_lead<W: Vocabulary, K: Kind>(
    written: &Written<'_>,
    showing: &Showing<'_>,
) -> Settled<Led<W, K>> {
    match written.ended {
        Ended::Declined => return Settled::Unanswered(Why::Declined),
        Ended::CutOff => return Settled::Unanswered(Why::CutOff),
        Ended::Complete => {}
    }
    let text = written.text;
    let lead = text.trim_start();
    let word_end = lead.find(char::is_whitespace).unwrap_or(lead.len());
    let word = lead
        .get(..word_end)
        .unwrap_or_default()
        .trim_end_matches(['.', ':', ',']);
    let Some((_, verdict)) = W::WORDS.iter().find(|(candidate, _)| *candidate == word) else {
        return Settled::Unanswered(Why::NotRead);
    };
    let after_word = text
        .len()
        .saturating_sub(lead.len())
        .saturating_add(word_end);
    let mut offset = 0usize;
    let mut last: Option<(usize, &str)> = None;
    for line in text.split_inclusive('\n') {
        if !line.trim().is_empty() {
            last = Some((offset, line));
        }
        offset = offset.saturating_add(line.len());
    }
    let Some((line_start, line)) = last.filter(|(start, _)| *start >= after_word) else {
        return Settled::Unanswered(Why::Unplaced);
    };
    let pointer = if says_none(line) {
        None
    } else {
        match showing.resolve::<K>(line.trim()) {
            Some(picked) => Some(picked),
            None => return Settled::Unanswered(Why::Unplaced),
        }
    };
    let between = text
        .get(after_word..line_start)
        .unwrap_or_default()
        .trim()
        .to_owned();
    Settled::Answered(Led {
        verdict: *verdict,
        text: between,
        pointer,
    })
}
