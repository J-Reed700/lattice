//! The schema and the reading of every shape. The reading rules are the same
//! for all of them: a statement is used only if it closed; what is missing is
//! `Missing` only in an answer read to its end.

use serde_json::{json, Map, Value};

use super::super::edit::{Consistent, Content};
use super::super::shown::{Kind, Picked, Showing};
use super::document::{self, Document};
use super::fields::trace;
use super::form::{Among, Field, Form};
use super::{
    Authored, Ended, Fields, Found, Led, Pointers, Rows, Schema, Settled, Vocabulary, Why, Written,
};

// ---- schema ----

pub(super) fn value_schema<V: Authored>(
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Option<Schema> {
    Some(made(form_object(&V::form(), showing, pointers)))
}

pub(super) fn pick_schema<K: Kind>(showing: &Showing<'_>, pointers: Pointers) -> Option<Schema> {
    let pick = point_schema(K::PREFIX, Among::Shown, &["none"], showing, pointers);
    Some(made(closed(vec![("pick".to_owned(), pick)])))
}

pub(super) fn pick_some_schema<K: Kind>(
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Option<Schema> {
    Some(made(closed(vec![(
        "picks".to_owned(),
        handles_schema(K::PREFIX, showing, pointers),
    )])))
}

pub(super) fn new_schema<V: Authored>(showing: &Showing<'_>, pointers: Pointers) -> Option<Schema> {
    let items = array(form_object(&V::form(), showing, pointers));
    Some(made(closed(vec![("items".to_owned(), items)])))
}

pub(super) fn each_schema<K: Kind, V: Authored>(
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Option<Schema> {
    Some(made(closed(vec![(
        "entries".to_owned(),
        keyed_entries::<K, V>("of", showing, pointers),
    )])))
}

pub(super) fn flagged_schema<K: Kind, V: Authored>(
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Option<Schema> {
    Some(made(closed(vec![(
        "marks".to_owned(),
        keyed_entries::<K, V>("at", showing, pointers),
    )])))
}

pub(super) fn change_schema<K: Kind, W: Content>(
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Option<Schema> {
    let field = Field::Change {
        prefix: K::PREFIX,
        with: Box::new(W::field()),
        start: true,
    };
    Some(made(field_schema(&field, showing, pointers)))
}

fn made(json: Value) -> Schema {
    Schema::made(json)
}

fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn closed(properties: Vec<(String, Value)>) -> Value {
    let required: Vec<Value> = properties
        .iter()
        .map(|(name, _)| Value::String(name.clone()))
        .collect();
    let properties: Map<String, Value> = properties.into_iter().collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn words(words: &[&str]) -> Value {
    json!({ "type": "string", "enum": words })
}

fn form_properties(form: &Form, showing: &Showing<'_>, pointers: Pointers) -> Vec<(String, Value)> {
    form.fields
        .iter()
        .map(|(name, field)| ((*name).to_owned(), field_schema(field, showing, pointers)))
        .collect()
}

fn form_object(form: &Form, showing: &Showing<'_>, pointers: Pointers) -> Value {
    closed(form_properties(form, showing, pointers))
}

/// A list of closed objects: a handle under `key` for a shown `K`, then `V`'s fields.
fn keyed_entries<K: Kind, V: Authored>(
    key: &str,
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Value {
    let mut properties = vec![(
        key.to_owned(),
        point_schema(K::PREFIX, Among::Shown, &[], showing, pointers),
    )];
    properties.extend(form_properties(&V::form(), showing, pointers));
    array(closed(properties))
}

fn handles_schema(prefix: &str, showing: &Showing<'_>, pointers: Pointers) -> Value {
    array(point_schema(prefix, Among::Shown, &[], showing, pointers))
}

/// A pointer: the `enum` of the handles it may hold and the extra words, or a
/// plain string where nothing is shown to enumerate or the backend takes none.
fn point_schema(
    prefix: &str,
    among: Among,
    extra: &[&str],
    showing: &Showing<'_>,
    pointers: Pointers,
) -> Value {
    let shown = match among {
        Among::Shown => showing.written(prefix),
        Among::Own => showing.written_under(prefix),
    };
    if pointers == Pointers::Plain {
        let words = extra.join(" or ");
        let description = match (shown.first(), words.is_empty()) {
            (Some(first), true) => {
                format!("One of the names the request shows, such as {first}.")
            }
            (Some(first), false) => {
                format!("One of the names the request shows, such as {first}, or the word {words}.")
            }
            (None, false) => format!("The word {words}."),
            (None, true) => return json!({ "type": "string" }),
        };
        return json!({ "type": "string", "description": description });
    }
    let mut members = shown;
    members.extend(extra.iter().map(|word| (*word).to_owned()));
    if members.is_empty() {
        return json!({ "type": "string" });
    }
    json!({ "type": "string", "enum": members })
}

fn field_schema(field: &Field, showing: &Showing<'_>, pointers: Pointers) -> Value {
    match field {
        Field::Text => json!({ "type": "string" }),
        Field::Word(list) => words(list),
        Field::Number => json!({ "type": "integer" }),
        Field::Point {
            prefix,
            among,
            none,
        } => {
            let extra: &[&str] = if *none { &["none"] } else { &[] };
            point_schema(prefix, *among, extra, showing, pointers)
        }
        Field::Cite {
            prefix,
            among,
            some,
        } => {
            let shown = match among {
                Among::Shown => showing.written(prefix),
                Among::Own => showing.written_under(prefix),
            };
            let items = point_schema(prefix, *among, &[], showing, pointers);
            if *some && !shown.is_empty() {
                json!({ "type": "array", "items": items, "minItems": 1 })
            } else {
                array(items)
            }
        }
        Field::Order { prefix } => handles_schema(prefix, showing, pointers),
        Field::Group(form) => form_object(form, showing, pointers),
        Field::List(form) => array(form_object(form, showing, pointers)),
        Field::EachOwn { prefix, form } => {
            let mut properties = vec![(
                "of".to_owned(),
                point_schema(prefix, Among::Own, &[], showing, pointers),
            )];
            properties.extend(form_properties(form, showing, pointers));
            array(closed(properties))
        }
        Field::Change {
            prefix,
            with,
            start,
        } => {
            let at = point_schema(prefix, Among::Shown, &[], showing, pointers);
            let begin: &[&str] = if *start { &["start"] } else { &[] };
            let anchor = point_schema(prefix, Among::Shown, begin, showing, pointers);
            let with = field_schema(with, showing, pointers);
            closed(vec![
                (
                    "replace".to_owned(),
                    array(closed(vec![
                        ("at".to_owned(), at.clone()),
                        ("with".to_owned(), with.clone()),
                    ])),
                ),
                (
                    "insert".to_owned(),
                    array(closed(vec![
                        ("after".to_owned(), anchor.clone()),
                        ("with".to_owned(), with),
                    ])),
                ),
                ("remove".to_owned(), array(at.clone())),
                (
                    "move".to_owned(),
                    array(closed(vec![
                        ("at".to_owned(), at),
                        ("after".to_owned(), anchor),
                    ])),
                ),
            ])
        }
        Field::OneOf(forms) => {
            let branches: Vec<Value> = forms
                .iter()
                .map(|(tag, form)| {
                    let mut properties = vec![("kind".to_owned(), words(&[*tag]))];
                    properties.extend(form_properties(form, showing, pointers));
                    closed(properties)
                })
                .collect();
            json!({ "anyOf": branches })
        }
    }
}

// ---- reading ----

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

pub(super) fn read_value<V: Authored>(written: &Written<'_>, showing: &Showing<'_>) -> Settled<V> {
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

pub(super) fn read_pick<K: Kind>(
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

pub(super) fn read_pick_some<K: Kind>(
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

pub(super) fn read_new<V: Authored>(written: &Written<'_>, showing: &Showing<'_>) -> Found<V> {
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

pub(super) fn read_each<K: Kind, V: Authored + PartialEq>(
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

pub(super) fn read_flagged<K: Kind, V: Authored + PartialEq>(
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

pub(super) fn read_change<K: Kind, W: Content>(
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

pub(super) fn read_lead<W: Vocabulary, K: Kind>(
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
    let pointer = match line.trim() {
        "none" => None,
        handle => match showing.resolve::<K>(handle) {
            Some(picked) => Some(picked),
            None => return Settled::Unanswered(Why::Unplaced),
        },
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
