//! JSON schema builders for each [`Shape`](super::super::Shape).

use serde_json::{json, Map, Value};

use super::super::super::edit::Content;
use super::super::super::shown::Kind;
use super::super::{Authored, Count, Held, Schema, STATEMENTS};
use super::satisfiable::{field_satisfiable, form_satisfiable, shown_written};
use super::{Among, Field, Form, Showing};

pub(super) fn value_schema<V: Authored>(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
    Some(made(form_object(&V::form(), showing, pointers)))
}

pub(super) fn pick_schema<K: Kind>(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
    let pick = point_schema(K::PREFIX, Among::Shown, &["none"], showing, pointers);
    Some(made(closed(vec![("pick".to_owned(), pick)])))
}

pub(super) fn pick_some_schema<K: Kind>(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
    Some(made(closed(vec![(
        "picks".to_owned(),
        handles_schema(K::PREFIX, showing, pointers),
    )])))
}

pub(super) fn new_schema<V: Authored>(showing: &Showing<'_>, pointers: Held) -> Option<Schema> {
    let items = array(form_object(&V::form(), showing, pointers));
    let items = asked_length(items, &STATEMENTS, showing, pointers);
    Some(made(closed(vec![("items".to_owned(), items)])))
}

/// `list` with the length the request asks of `count`, for a backend that holds lists to it.
fn asked_length(mut list: Value, count: &Count, showing: &Showing<'_>, held: Held) -> Value {
    if held != Held::Counts {
        return list;
    }
    let Some((least, most)) = showing.asked(count) else {
        return list;
    };
    if let Some(fields) = list.as_object_mut() {
        if least > 0 {
            fields.insert("minItems".to_owned(), json!(least));
        }
        if let Some(most) = most {
            fields.insert("maxItems".to_owned(), json!(most));
        }
    }
    list
}

pub(super) fn each_schema<K: Kind, V: Authored>(
    showing: &Showing<'_>,
    pointers: Held,
) -> Option<Schema> {
    Some(made(closed(vec![(
        "entries".to_owned(),
        keyed_entries::<K, V>("of", showing, pointers),
    )])))
}

pub(super) fn flagged_schema<K: Kind, V: Authored>(
    showing: &Showing<'_>,
    pointers: Held,
) -> Option<Schema> {
    Some(made(closed(vec![(
        "marks".to_owned(),
        keyed_entries::<K, V>("at", showing, pointers),
    )])))
}

pub(super) fn change_schema<K: Kind, W: Content>(
    showing: &Showing<'_>,
    pointers: Held,
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

fn form_properties(form: &Form, showing: &Showing<'_>, pointers: Held) -> Vec<(String, Value)> {
    form.fields
        .iter()
        .map(|(name, field)| ((*name).to_owned(), field_schema(field, showing, pointers)))
        .collect()
}

fn form_object(form: &Form, showing: &Showing<'_>, pointers: Held) -> Value {
    closed(form_properties(form, showing, pointers))
}

/// A list of closed objects: a handle under `key` for a shown `K`, then `V`'s fields.
fn keyed_entries<K: Kind, V: Authored>(key: &str, showing: &Showing<'_>, pointers: Held) -> Value {
    let mut properties = vec![(
        key.to_owned(),
        point_schema(K::PREFIX, Among::Shown, &[], showing, pointers),
    )];
    properties.extend(form_properties(&V::form(), showing, pointers));
    array(closed(properties))
}

fn handles_schema(prefix: &str, showing: &Showing<'_>, pointers: Held) -> Value {
    array(point_schema(prefix, Among::Shown, &[], showing, pointers))
}

fn branch_object(tag: &str, form: &Form, showing: &Showing<'_>, pointers: Held) -> Value {
    let mut properties = vec![("kind".to_owned(), words(&[tag]))];
    properties.extend(form_properties(form, showing, pointers));
    closed(properties)
}

/// A pointer: the `enum` of the handles it may hold and the extra words, or a
/// plain string where nothing is shown to enumerate or the backend takes none.
fn point_schema(
    prefix: &str,
    among: Among,
    extra: &[&str],
    showing: &Showing<'_>,
    pointers: Held,
) -> Value {
    let shown = shown_written(prefix, among, showing);
    if pointers == Held::Plain {
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

fn array_or_empty(items: Value, allowed: bool, pointers: Held) -> Value {
    if !allowed && pointers == Held::Counts {
        json!({ "type": "array", "items": items, "maxItems": 0 })
    } else {
        array(items)
    }
}

fn field_schema(field: &Field, showing: &Showing<'_>, pointers: Held) -> Value {
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
            if shown.is_empty() && !*some && pointers == Held::Counts {
                json!({ "type": "array", "items": items, "maxItems": 0 })
            } else if *some && !shown.is_empty() {
                json!({ "type": "array", "items": items, "minItems": 1 })
            } else {
                array(items)
            }
        }
        Field::Order { prefix } => handles_schema(prefix, showing, pointers),
        Field::Group(form) => form_object(form, showing, pointers),
        Field::List { form, count } => {
            let item_schema = form_object(form, showing, pointers);
            match count {
                Some(count) => asked_length(array(item_schema), count, showing, pointers),
                None => array_or_empty(item_schema, form_satisfiable(form, showing), pointers),
            }
        }
        Field::EachOwn { prefix, form } => {
            let mut properties = vec![(
                "of".to_owned(),
                point_schema(prefix, Among::Own, &[], showing, pointers),
            )];
            properties.extend(form_properties(form, showing, pointers));
            let allowed =
                !showing.written_under(prefix).is_empty() && form_satisfiable(form, showing);
            array_or_empty(closed(properties), allowed, pointers)
        }
        Field::Change {
            prefix,
            with,
            start,
        } => {
            let has_shown = !showing.written(prefix).is_empty();
            let has_anchor = has_shown || *start;
            let with_ok = field_satisfiable(with, showing);
            let at = point_schema(prefix, Among::Shown, &[], showing, pointers);
            let begin: &[&str] = if *start { &["start"] } else { &[] };
            let anchor = point_schema(prefix, Among::Shown, begin, showing, pointers);
            let with = field_schema(with, showing, pointers);
            closed(vec![
                (
                    "replace".to_owned(),
                    array_or_empty(
                        closed(vec![
                            ("at".to_owned(), at.clone()),
                            ("with".to_owned(), with.clone()),
                        ]),
                        has_shown && with_ok,
                        pointers,
                    ),
                ),
                (
                    "insert".to_owned(),
                    array_or_empty(
                        closed(vec![
                            ("after".to_owned(), anchor.clone()),
                            ("with".to_owned(), with),
                        ]),
                        has_anchor && with_ok,
                        pointers,
                    ),
                ),
                (
                    "remove".to_owned(),
                    array_or_empty(at.clone(), has_shown, pointers),
                ),
                (
                    "move".to_owned(),
                    array_or_empty(
                        closed(vec![("at".to_owned(), at), ("after".to_owned(), anchor)]),
                        has_shown && has_anchor,
                        pointers,
                    ),
                ),
            ])
        }
        Field::OneOf(forms) => {
            let mut branches: Vec<Value> = forms
                .iter()
                .filter(|(_, form)| form_satisfiable(form, showing))
                .map(|(tag, form)| branch_object(tag, form, showing, pointers))
                .collect();
            if branches.is_empty() {
                branches = forms
                    .iter()
                    .map(|(tag, form)| branch_object(tag, form, showing, pointers))
                    .collect();
            }
            if branches.len() == 1 {
                if let Some(only) = branches.pop() {
                    return only;
                }
            }
            json!({ "anyOf": branches })
        }
    }
}
