//! The reading and the schema of every shape, over generated scopes and answers.

use proptest::prelude::*;
use serde_json::{json, Map, Value};

use super::super::edit::{Change, Consistent, Made};
use super::super::shown::{Drawn, Kind, Picked, Scope, Showing};
use super::document;
use super::Value as Whole;
use super::{
    Authored, Count, Each, Ended, Fields, Flagged, Form, Found, Held, Keeps, Lead, Led, Listed,
    New, PerItem, Pick, PickSome, Place, Settled, Shape, Single, Union, Vocabulary, Why, Written,
    STATEMENTS,
};

// ---- kinds and statements ----

#[derive(Debug, Clone, PartialEq)]
struct Sheet(u32);

impl Kind for Sheet {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Opt(u32);

impl Kind for Opt {
    const PREFIX: &'static str = "o";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    Yes,
    No,
}

impl Vocabulary for Verdict {
    const WORDS: &'static [(&'static str, Self)] = &[("yes", Verdict::Yes), ("no", Verdict::No)];
}

#[derive(Debug, PartialEq)]
struct Note {
    verdict: Verdict,
    text: String,
    cites: Vec<Picked<Sheet>>,
}

impl Authored for Note {
    fn form() -> Form {
        Form::new()
            .word::<Verdict>("verdict")
            .text("text")
            .cite::<Sheet>("cites")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Note {
            verdict: fields.word("verdict")?,
            text: fields.text("text")?,
            cites: fields.cite("cites"),
        })
    }
}

#[derive(Debug, PartialEq)]
struct Inner {
    label: String,
}

impl Authored for Inner {
    fn form() -> Form {
        Form::new().text("label")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Inner {
            label: fields.text("label")?,
        })
    }
}

#[derive(Debug, PartialEq)]
enum Choice {
    Plain { note: String },
    Pointing { at: Picked<Sheet> },
}

impl Union for Choice {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            ("plain", Form::new().text("note")),
            ("pointing", Form::new().point::<Sheet>("at")),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "plain" => Some(Choice::Plain {
                note: fields.text("note")?,
            }),
            "pointing" => Some(Choice::Pointing {
                at: fields.point("at")?,
            }),
            _ => None,
        }
    }
}

/// A statement with a field of every kind.
#[derive(Debug, PartialEq)]
struct Rich {
    text: String,
    verdict: Verdict,
    amount: u32,
    sheet: Picked<Sheet>,
    maybe: Option<Picked<Sheet>>,
    opt: Picked<Opt>,
    maybe_opt: Option<Picked<Opt>>,
    cites: Vec<Picked<Sheet>>,
    own_cites: Vec<Picked<Opt>>,
    inner: Inner,
    inners: Vec<Inner>,
    per_opt: Vec<(Picked<Opt>, Inner)>,
    edit: Consistent<Sheet, String>,
    choice: Choice,
}

impl Authored for Rich {
    fn form() -> Form {
        Form::new()
            .text("text")
            .word::<Verdict>("verdict")
            .number("amount")
            .point::<Sheet>("sheet")
            .point_or_none::<Sheet>("maybe")
            .own::<Opt>("opt")
            .own_or_none::<Opt>("maybe_opt")
            .cite::<Sheet>("cites")
            .cite_own::<Opt>("own_cites")
            .group::<Inner>("inner")
            .list::<Inner>("inners")
            .each_own::<Opt, Inner>("per_opt")
            .change::<Sheet, String>("edit")
            .one_of::<Choice>("choice")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Rich {
            text: fields.text("text")?,
            verdict: fields.word("verdict")?,
            amount: fields.number("amount")?,
            sheet: fields.point("sheet")?,
            maybe: fields.point_or_none("maybe")?,
            opt: fields.own("opt")?,
            maybe_opt: fields.own_or_none("maybe_opt")?,
            cites: fields.cite("cites"),
            own_cites: fields.cite_own("own_cites"),
            inner: fields.group("inner")?,
            inners: fields.list("inners")?,
            per_opt: fields.each_own("per_opt")?,
            edit: fields.change("edit")?,
            choice: fields.one_of("choice")?,
        })
    }
}

/// A statement of one whole number.
#[derive(Debug, PartialEq)]
struct Amount {
    amount: u32,
}

impl Authored for Amount {
    fn form() -> Form {
        Form::new().number("amount")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Amount {
            amount: fields.number("amount")?,
        })
    }
}

/// A statement of a reason and an order of every shown sheet.
#[derive(Debug, PartialEq)]
struct Ranked {
    reason: String,
    order: Vec<Picked<Sheet>>,
}

impl Authored for Ranked {
    fn form() -> Form {
        Form::new().text("reason").order::<Sheet>("order")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Ranked {
            reason: fields.text("reason")?,
            order: fields.order("order")?,
        })
    }
}

/// A statement whose evidence must name a shown sheet.
#[derive(Debug, PartialEq)]
struct Backed {
    cites: Vec<Picked<Sheet>>,
}

impl Authored for Backed {
    fn form() -> Form {
        Form::new().cite_some::<Sheet>("cites")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Backed {
            cites: fields.cite_some("cites")?,
        })
    }
}

/// A statement that holds where the model wrote it.
#[derive(Debug, PartialEq)]
struct Placed {
    place: Place,
}

impl Authored for Placed {
    fn form() -> Form {
        Form::new().text("note")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        fields.text("note")?;
        Some(Placed {
            place: fields.place(),
        })
    }
}

// ---- helpers ----

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// A request that shows `sheets` sheets, each with `own` options of its own.
fn with_showing<R>(sheets: usize, own: usize, body: impl FnOnce(&Showing<'_>) -> R) -> R {
    let sheet_things: Vec<Sheet> = (0..sheets as u32).map(Sheet).collect();
    let option_things: Vec<Opt> = (0..own as u32).map(Opt).collect();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(&sheet_things).unwrap();
    for (handle, _) in shown.iter() {
        showing.under(handle, &option_things).unwrap();
    }
    body(&showing)
}

fn written(text: &str, ended: Ended) -> Written<'_> {
    Written { text, ended }
}

/// Every strict prefix of `text`, cut at a character boundary, with its length.
fn cuts(text: &str) -> impl Iterator<Item = (usize, &str)> + '_ {
    text.char_indices().map(move |(at, _)| (at, &text[..at]))
}

fn handle_of(number: usize) -> String {
    format!("s{number}")
}

fn verdict_of(number: usize) -> Verdict {
    if number.is_multiple_of(2) {
        Verdict::Yes
    } else {
        Verdict::No
    }
}

fn verdict_word(number: usize) -> &'static str {
    if number.is_multiple_of(2) {
        "yes"
    } else {
        "no"
    }
}

fn random_cites(rng: &mut Rng, shown: usize) -> Vec<String> {
    (1..=shown)
        .filter(|_| rng.chance(40))
        .map(handle_of)
        .collect()
}

/// The fields of a `Note`, as the model writes them; `lead` goes first.
fn note_object(lead: Option<(&str, &str)>, number: usize, cites: &[String]) -> String {
    let mut object = Map::new();
    if let Some((key, handle)) = lead {
        object.insert(key.to_owned(), json!(handle));
    }
    object.insert("verdict".to_owned(), json!(verdict_word(number)));
    object.insert(
        "text".to_owned(),
        json!(format!("t{number} \u{e9}\u{65e5}")),
    );
    object.insert("cites".to_owned(), json!(cites));
    Value::Object(object).to_string()
}

fn expected_note(number: usize, cites: &[String], showing: &Showing<'_>) -> Note {
    Note {
        verdict: verdict_of(number),
        text: format!("t{number} \u{e9}\u{65e5}"),
        cites: cites
            .iter()
            .map(|cite| showing.resolve::<Sheet>(cite).unwrap())
            .collect(),
    }
}

/// An answer that is a list under `key`, with where each element closes.
struct Built {
    text: String,
    ends: Vec<usize>,
}

fn list_doc(key: &str, elements: &[String]) -> Built {
    let mut text = format!("{{\"{key}\":[");
    let mut ends = Vec::new();
    for (at, element) in elements.iter().enumerate() {
        if at > 0 {
            text.push(',');
        }
        text.push_str(element);
        ends.push(text.len());
    }
    text.push_str("]}");
    Built { text, ends }
}

fn schema_of<S: Shape>(showing: &Showing<'_>, pointers: Held) -> Value {
    S::schema(showing, pointers).unwrap().as_json().clone()
}

fn assert_in_order<K: Kind, T>(showing: &Showing<'_>, out: &[(Picked<K>, T)]) {
    let got: Vec<String> = out.iter().map(|(item, _)| item.handle.written()).collect();
    assert_eq!(got, showing.written(K::PREFIX));
}

fn via_value(text: &str, ended: Ended, showing: &Showing<'_>) -> Settled<Note> {
    <Whole<Note> as Single>::read(&written(text, ended), showing)
}

fn via_pick(text: &str, ended: Ended, showing: &Showing<'_>) -> Settled<Option<Picked<Sheet>>> {
    <Pick<Sheet> as Single>::read(&written(text, ended), showing)
}

fn via_pick_some(text: &str, ended: Ended, showing: &Showing<'_>) -> Settled<Vec<Picked<Sheet>>> {
    <PickSome<Sheet> as Single>::read(&written(text, ended), showing)
}

fn via_order(text: &str, ended: Ended, showing: &Showing<'_>) -> Settled<Vec<Picked<Sheet>>> {
    match <Whole<Ranked> as Single>::read(&written(text, ended), showing) {
        Settled::Answered(ranked) => Settled::Answered(ranked.order),
        Settled::Unanswered(why) => Settled::Unanswered(why),
    }
}

fn via_new(text: &str, ended: Ended, showing: &Showing<'_>) -> Found<Note> {
    <New<Note> as Listed>::read(&written(text, ended), showing)
}

fn via_each(
    text: &str,
    ended: Ended,
    showing: &Showing<'_>,
) -> Vec<(Picked<Sheet>, Settled<Note>)> {
    <Each<Sheet, Note> as PerItem>::read(&written(text, ended), showing)
}

fn via_flagged(
    text: &str,
    ended: Ended,
    showing: &Showing<'_>,
) -> Vec<(Picked<Sheet>, Settled<Option<Note>>)> {
    <Flagged<Sheet, Note> as PerItem>::read(&written(text, ended), showing)
}

type KeptRow = (Picked<Sheet>, Settled<Vec<Picked<Opt>>>);

fn via_keeps(text: &str, ended: Ended, showing: &Showing<'_>) -> Vec<KeptRow> {
    <Keeps<Sheet, Opt> as PerItem>::read(&written(text, ended), showing)
}

/// What each sheet kept, as handles, or why it is unanswered.
fn kept(rows: &[KeptRow]) -> Vec<Result<Vec<String>, Why>> {
    rows.iter()
        .map(|(_, settled)| match settled {
            Settled::Answered(picked) => {
                Ok(picked.iter().map(|own| own.handle.written()).collect())
            }
            Settled::Unanswered(why) => Err(why.clone()),
        })
        .collect()
}

fn handles(words: &[&str]) -> Result<Vec<String>, Why> {
    Ok(words.iter().map(|word| (*word).to_owned()).collect())
}

fn via_change(
    text: &str,
    ended: Ended,
    showing: &Showing<'_>,
) -> Settled<Consistent<Sheet, String>> {
    <Change<Sheet, String> as Single>::read(&written(text, ended), showing)
}

fn via_lead(text: &str, ended: Ended, showing: &Showing<'_>) -> Settled<Led<Verdict, Sheet>> {
    <Lead<Verdict, Sheet> as Single>::read(&written(text, ended), showing)
}

// ---- the dialect and a structural validator for its subset ----

const ALLOWED: [&str; 9] = [
    "type",
    "minItems",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "anyOf",
    "description",
];

/// Every object closed, every property required, nothing outside the dialect.
fn assert_dialect(schema: &Value) {
    let object = schema.as_object().unwrap();
    for key in object.keys() {
        assert!(
            ALLOWED.contains(&key.as_str()),
            "keyword {key} is outside the dialect"
        );
    }
    if let Some(branches) = object.get("anyOf") {
        assert_eq!(object.len(), 1);
        for branch in branches.as_array().unwrap() {
            assert_eq!(branch["type"], json!("object"));
            assert_dialect(branch);
        }
        return;
    }
    match object["type"].as_str().unwrap() {
        "string" => {
            assert!(object
                .keys()
                .all(|key| key == "type" || key == "enum" || key == "description"));
            if let Some(members) = object.get("enum") {
                let members = members.as_array().unwrap();
                assert!(!members.is_empty());
                assert!(members.iter().all(Value::is_string));
            }
        }
        "integer" => assert_eq!(object.len(), 1),
        "array" => {
            let counted = usize::from(object.contains_key("minItems"));
            assert_eq!(object.len(), 2 + counted);
            if counted == 1 {
                assert_eq!(object["minItems"], json!(1));
            }
            assert_dialect(&object["items"]);
        }
        "object" => {
            assert_eq!(object["additionalProperties"], json!(false));
            let properties = object["properties"].as_object().unwrap();
            let required: Vec<&str> = object["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect();
            let names: Vec<&str> = properties.keys().map(String::as_str).collect();
            assert_eq!(required, names);
            for property in properties.values() {
                assert_dialect(property);
            }
        }
        other => panic!("type {other} is outside the dialect"),
    }
}

/// Whether `value` has the structure `schema` describes.
fn validate(schema: &Value, value: &Value) -> bool {
    if let Some(branches) = schema.get("anyOf") {
        return branches
            .as_array()
            .unwrap()
            .iter()
            .any(|branch| validate(branch, value));
    }
    match schema["type"].as_str().unwrap() {
        "string" => value.as_str().is_some_and(|text| {
            schema.get("enum").is_none_or(|members| {
                members
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|member| member.as_str() == Some(text))
            })
        }),
        "integer" => value.as_u64().is_some(),
        "array" => value.as_array().is_some_and(|elements| {
            let least = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            elements.len() as u64 >= least
                && elements
                    .iter()
                    .all(|element| validate(&schema["items"], element))
        }),
        "object" => value.as_object().is_some_and(|object| {
            let properties = schema["properties"].as_object().unwrap();
            object.len() == properties.len()
                && properties
                    .iter()
                    .all(|(name, inner)| object.get(name).is_some_and(|held| validate(inner, held)))
        }),
        _ => false,
    }
}

fn assert_valid<S: Shape>(showing: &Showing<'_>, answer: &str) {
    let value: Value = serde_json::from_str(answer).unwrap();
    for pointers in [Held::Enums, Held::Plain] {
        let schema = schema_of::<S>(showing, pointers);
        assert!(validate(&schema, &value), "{answer} does not fit {schema}");
    }
}

// ---- document ----

#[test]
fn document_skips_prose_and_fences() {
    let text = "Sure:\n```json\n{\"a\": 1, \"b\": [1, 2]}\n```\nand {not json}";
    let read = document::read(text).unwrap();
    assert!(read.closed);
    assert_eq!(read.fields["a"], json!(1));
    assert_eq!(read.fields["b"], json!([1, 2]));
    assert!(read.open_list.is_none());
}

#[test]
fn document_starts_at_the_first_brace_that_begins_an_object() {
    let read = document::read("a { b } c {\"k\": \"v\"}").unwrap();
    assert_eq!(read.fields["k"], json!("v"));
}

#[test]
fn document_is_none_when_no_object_begins() {
    for text in ["", "no braces", "{", "{ nope", "[1, 2]", "} {"] {
        assert!(document::read(text).is_none(), "{text:?}");
    }
}

#[test]
fn document_keeps_closed_fields_and_the_open_list() {
    let read = document::read("{\"a\": 1, \"items\": [{\"x\": 1}, {\"x\": 2}, {\"x\"").unwrap();
    assert!(!read.closed);
    assert_eq!(read.fields.len(), 1);
    let (name, elements) = read.open_list.unwrap();
    assert_eq!(name, "items");
    assert_eq!(elements, vec![json!({"x": 1}), json!({"x": 2})]);
}

#[test]
fn document_never_completes_a_value_that_did_not_close() {
    for (text, absent) in [
        ("{\"a\": 12", "a"),
        ("{\"a\": tru", "a"),
        ("{\"a\": \"abc", "a"),
        ("{\"a\": {\"b\": 1", "a"),
        ("{\"a\": [1, 2", "a"),
        ("{\"a\": \"x\\", "a"),
    ] {
        let read = document::read(text).unwrap();
        assert!(!read.fields.contains_key(absent), "{text:?}");
        assert!(!read.closed);
    }
    let read = document::read("{\"a\": 12, \"b\": 3").unwrap();
    assert_eq!(read.fields["a"], json!(12));
    assert!(!read.fields.contains_key("b"));
}

#[test]
fn document_handles_nesting_and_escapes() {
    let read = document::read(r#"{"a": "q\"}\\", "b": {"c": ["]", "}"]}, "d": []}"#).unwrap();
    assert!(read.closed);
    assert_eq!(read.fields["a"], json!("q\"}\\"));
    assert_eq!(read.fields["b"], json!({"c": ["]", "}"]}));
    assert_eq!(read.fields["d"], json!([]));
}

#[test]
fn document_survives_deep_nesting() {
    let deep = format!("{{\"a\": {}", "[".repeat(200_000));
    let read = document::read(&deep).unwrap();
    assert!(read.fields.is_empty());
    let braces = "{".repeat(200_000);
    assert!(document::read(&braces).is_none());
    let keyed = format!("{{\"a\": {}{}}}", "[".repeat(5_000), "]".repeat(5_000));
    let _ = document::read(&keyed);
}

proptest! {
    #[test]
    fn document_never_panics(text in any::<String>(), tail in "\\PC{0,40}") {
        let _ = document::read(&text);
        let braced = format!("{{\"k\": [{{\"a\": {text}");
        for (_, cut) in cuts(&braced) {
            let _ = document::read(cut);
        }
        let _ = document::read(&format!("{braced}{tail}"));
    }
}

// ---- schemas ----

#[test]
fn every_schema_obeys_the_dialect() {
    for (sheets, own) in [(0, 0), (1, 0), (3, 2), (12, 4)] {
        with_showing(sheets, own, |showing| {
            for pointers in [Held::Enums, Held::Plain] {
                let schemas = [
                    schema_of::<Whole<Rich>>(showing, pointers),
                    schema_of::<Pick<Sheet>>(showing, pointers),
                    schema_of::<PickSome<Sheet>>(showing, pointers),
                    schema_of::<Whole<Ranked>>(showing, pointers),
                    schema_of::<Whole<Backed>>(showing, pointers),
                    schema_of::<New<Rich>>(showing, pointers),
                    schema_of::<Each<Sheet, Rich>>(showing, pointers),
                    schema_of::<Flagged<Sheet, Rich>>(showing, pointers),
                    schema_of::<Change<Sheet, String>>(showing, pointers),
                    schema_of::<Change<Sheet, Made<Inner>>>(showing, pointers),
                ];
                for schema in &schemas {
                    assert_dialect(schema);
                }
            }
        });
    }
}

#[test]
fn a_lead_has_no_schema() {
    with_showing(2, 0, |showing| {
        assert!(<Lead<Verdict, Sheet> as Shape>::schema(showing, Held::Enums).is_none());
    });
}

#[test]
fn keeps_has_no_schema() {
    with_showing(2, 2, |showing| {
        assert!(<Keeps<Sheet, Opt> as Shape>::schema(showing, Held::Enums).is_none());
        assert!(<Keeps<Sheet, Opt> as Shape>::schema(showing, Held::Plain).is_none());
    });
}

#[test]
fn pointer_enums_hold_the_shown_handles_and_none() {
    with_showing(3, 2, |showing| {
        let schema = schema_of::<Pick<Sheet>>(showing, Held::Enums);
        assert_eq!(
            schema["properties"]["pick"]["enum"],
            json!(["s1", "s2", "s3", "none"])
        );
        let schema = schema_of::<Pick<Sheet>>(showing, Held::Plain);
        assert_eq!(
            schema["properties"]["pick"],
            json!({"type": "string", "description": PLAIN_NOTE_OR_NONE})
        );
        let schema = schema_of::<Each<Sheet, Rich>>(showing, Held::Enums);
        let entry = &schema["properties"]["entries"]["items"]["properties"];
        assert_eq!(entry["of"]["enum"], json!(["s1", "s2", "s3"]));
        assert_eq!(entry["opt"]["enum"], json!(["o1", "o2"]));
        assert_eq!(entry["maybe_opt"]["enum"], json!(["o1", "o2", "none"]));
        let anchor = &entry["edit"]["properties"]["insert"]["items"]["properties"]["after"];
        assert_eq!(anchor["enum"], json!(["s1", "s2", "s3", "start"]));
    });
}

const PLAIN_NOTE: &str = "One of the names the request shows, such as s1.";
const PLAIN_NOTE_OR_NONE: &str =
    "One of the names the request shows, such as s1, or the word none.";

#[test]
fn a_plain_pointer_says_how_a_handle_is_written() {
    with_showing(3, 2, |showing| {
        let schema = schema_of::<Each<Sheet, Rich>>(showing, Held::Plain);
        let entry = &schema["properties"]["entries"]["items"]["properties"];
        assert_eq!(
            entry["of"],
            json!({"type": "string", "description": PLAIN_NOTE})
        );
        let schema = schema_of::<Each<Sheet, Rich>>(showing, Held::Enums);
        let entry = &schema["properties"]["entries"]["items"]["properties"];
        assert_eq!(
            entry["of"],
            json!({"type": "string", "enum": ["s1", "s2", "s3"]})
        );
    });
}

#[test]
fn a_plain_pointer_that_may_be_none_names_the_word_too() {
    with_showing(3, 0, |showing| {
        let schema = schema_of::<Pick<Sheet>>(showing, Held::Plain);
        assert_eq!(
            schema["properties"]["pick"],
            json!({"type": "string", "description": PLAIN_NOTE_OR_NONE})
        );
    });
}

#[test]
fn a_plain_pointer_with_nothing_shown_holds_only_the_extra_word() {
    with_showing(0, 0, |showing| {
        let schema = schema_of::<Pick<Sheet>>(showing, Held::Plain);
        assert_eq!(
            schema["properties"]["pick"],
            json!({"type": "string", "description": "The word none."})
        );
        let schema = schema_of::<PickSome<Sheet>>(showing, Held::Plain);
        assert_eq!(
            schema["properties"]["picks"]["items"],
            json!({"type": "string"})
        );
    });
}

#[test]
fn nothing_shown_renders_a_plain_pointer() {
    with_showing(0, 0, |showing| {
        let schema = schema_of::<PickSome<Sheet>>(showing, Held::Enums);
        assert_eq!(
            schema["properties"]["picks"]["items"],
            json!({"type": "string"})
        );
        let schema = schema_of::<Pick<Sheet>>(showing, Held::Enums);
        assert_eq!(schema["properties"]["pick"]["enum"], json!(["none"]));
    });
}

#[test]
fn cite_some_holds_one_item_at_least_when_things_are_shown() {
    with_showing(3, 0, |showing| {
        for pointers in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Whole<Backed>>(showing, pointers);
            assert_eq!(schema["properties"]["cites"]["minItems"], json!(1));
        }
    });
}

#[test]
fn cite_some_holds_no_count_when_nothing_is_shown() {
    with_showing(0, 0, |showing| {
        let schema = schema_of::<Whole<Backed>>(showing, Held::Enums);
        assert_eq!(
            schema["properties"]["cites"],
            json!({"type": "array", "items": {"type": "string"}})
        );
    });
}

#[test]
fn a_plain_cite_holds_no_count() {
    with_showing(3, 0, |showing| {
        let schema = schema_of::<Whole<Note>>(showing, Held::Enums);
        assert_eq!(
            schema["properties"]["cites"],
            json!({"type": "array", "items": {"type": "string", "enum": ["s1", "s2", "s3"]}})
        );
    });
}

#[derive(Debug, PartialEq)]
struct Chosen {
    choice: Choice,
}

impl Authored for Chosen {
    fn form() -> Form {
        Form::new().one_of::<Choice>("choice")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            choice: fields.one_of("choice")?,
        })
    }
}

#[test]
fn one_of_omits_branches_whose_required_pointer_has_nothing_shown() {
    with_showing(0, 0, |showing| {
        for pointers in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Whole<Chosen>>(showing, pointers);
            assert_dialect(&schema);
            let choice = &schema["properties"]["choice"];
            assert!(choice.get("anyOf").is_none());
            assert_eq!(choice["properties"]["kind"]["enum"], json!(["plain"]));
            assert!(validate(
                &schema,
                &json!({"choice": {"kind": "plain", "note": "ok"}})
            ));
            assert!(!validate(
                &schema,
                &json!({"choice": {"kind": "pointing", "at": "s1"}})
            ));
        }
    });
    with_showing(2, 0, |showing| {
        for pointers in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Whole<Chosen>>(showing, pointers);
            assert_dialect(&schema);
            let branches = schema["properties"]["choice"]["anyOf"].as_array().unwrap();
            assert_eq!(branches.len(), 2);
        }
    });
}

const STEPS: Count = Count("steps");

/// A statement with a list whose length the request sets.
#[derive(Debug, PartialEq)]
struct Stepped {
    steps: Vec<Inner>,
}

impl Authored for Stepped {
    fn form() -> Form {
        Form::new().counted::<Inner>("steps", &STEPS)
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Stepped {
            steps: fields.list("steps")?,
        })
    }
}

fn asking(ask: impl FnOnce(&mut Showing<'_>), body: impl FnOnce(&Showing<'_>)) {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    ask(&mut showing);
    body(&showing);
}

#[test]
fn a_counted_list_carries_the_length_asked_only_where_lists_are_held_to_it() {
    asking(
        |showing| showing.asks(&STEPS, 2..=4),
        |showing| {
            let counted = schema_of::<Whole<Stepped>>(showing, Held::Counts);
            assert_eq!(counted["properties"]["steps"]["minItems"], json!(2));
            assert_eq!(counted["properties"]["steps"]["maxItems"], json!(4));
            for held in [Held::Enums, Held::Plain] {
                let schema = schema_of::<Whole<Stepped>>(showing, held);
                let steps = schema["properties"]["steps"].as_object().unwrap();
                assert!(!steps.contains_key("minItems") && !steps.contains_key("maxItems"));
            }
        },
    );
}

#[test]
fn an_open_ended_ask_is_a_minimum_only() {
    asking(
        |showing| showing.asks(&STEPS, 1..),
        |showing| {
            let counted = schema_of::<Whole<Stepped>>(showing, Held::Counts);
            assert_eq!(counted["properties"]["steps"]["minItems"], json!(1));
            assert!(counted["properties"]["steps"]
                .as_object()
                .unwrap()
                .get("maxItems")
                .is_none());
        },
    );
}

#[test]
fn with_nothing_asked_the_counts_schema_is_the_enums_schema() {
    asking(
        |_| {},
        |showing| {
            assert_eq!(
                schema_of::<Whole<Stepped>>(showing, Held::Counts),
                schema_of::<Whole<Stepped>>(showing, Held::Enums)
            );
            assert_eq!(
                schema_of::<New<Inner>>(showing, Held::Counts),
                schema_of::<New<Inner>>(showing, Held::Enums)
            );
        },
    );
}

#[test]
fn the_statements_of_a_new_answer_carry_the_length_asked_under_counts_only() {
    asking(
        |showing| showing.asks(&STATEMENTS, 1..=3),
        |showing| {
            let counted = schema_of::<New<Inner>>(showing, Held::Counts);
            assert_eq!(counted["properties"]["items"]["minItems"], json!(1));
            assert_eq!(counted["properties"]["items"]["maxItems"], json!(3));
            for held in [Held::Enums, Held::Plain] {
                let schema = schema_of::<New<Inner>>(showing, held);
                let items = schema["properties"]["items"].as_object().unwrap();
                assert!(!items.contains_key("minItems") && !items.contains_key("maxItems"));
            }
        },
    );
}

#[test]
fn a_list_that_names_no_count_never_carries_one() {
    asking(
        |showing| {
            showing.asks(&STEPS, 2..=4);
            showing.asks(&STATEMENTS, 1..=3);
        },
        |showing| {
            let schema = schema_of::<Whole<Rich>>(showing, Held::Counts);
            let inners = schema["properties"]["inners"].as_object().unwrap();
            assert!(!inners.contains_key("minItems") && !inners.contains_key("maxItems"));
        },
    );
}

// ---- Value ----

proptest! {
    #[test]
    fn value_is_unanswered_unless_whole(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let cites = random_cites(&mut rng, n);
            let text = note_object(None, 3, &cites);
            assert_valid::<Whole<Note>>(showing, &text);
            assert_eq!(
                via_value(&text, Ended::Complete, showing),
                Settled::Answered(expected_note(3, &cites, showing))
            );
            for (_, cut) in cuts(&text) {
                assert_eq!(
                    via_value(cut, Ended::CutOff, showing),
                    Settled::Unanswered(Why::CutOff)
                );
                assert_eq!(
                    via_value(cut, Ended::Complete, showing),
                    Settled::Unanswered(Why::NotRead)
                );
            }
        });
    }

    #[test]
    fn a_cite_that_does_not_resolve_never_unsettles_its_statement(n in 1usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut cites = random_cites(&mut rng, n);
            let kept = cites.clone();
            cites.insert(0, "s99".to_owned());
            cites.push("zz".to_owned());
            cites.push("[[s1]]".to_owned());
            if let Some(first) = kept.first() {
                cites.push(first.clone());
            }
            let text = note_object(None, 2, &cites);
            assert_eq!(
                via_value(&text, Ended::Complete, showing),
                Settled::Answered(expected_note(2, &kept, showing))
            );
        });
    }
}

#[test]
fn value_reasons() {
    with_showing(2, 0, |showing| {
        let ok = note_object(None, 1, &[]);
        assert_eq!(
            via_value(&ok, Ended::Declined, showing),
            Settled::Unanswered(Why::Declined)
        );
        assert_eq!(
            via_value("no json here", Ended::Complete, showing),
            Settled::Unanswered(Why::NotRead)
        );
        assert_eq!(
            via_value("no json here", Ended::CutOff, showing),
            Settled::Unanswered(Why::CutOff)
        );
        for broken in [
            r#"{"verdict":"maybe","text":"x","cites":[]}"#,
            r#"{"verdict":"yes","text":3,"cites":[]}"#,
            r#"{"verdict":"yes","cites":[]}"#,
            r#"{"Verdict":"yes","text":"x","cites":[]}"#,
            r#"{"verdict":"Yes","text":"x","cites":[]}"#,
        ] {
            assert_eq!(
                via_value(broken, Ended::Complete, showing),
                Settled::Unanswered(Why::NotRead),
                "{broken}"
            );
        }
        let extra = r#"{"verdict":"yes","text":"x","cites":[],"more":1}"#;
        assert!(matches!(
            via_value(extra, Ended::Complete, showing),
            Settled::Answered(_)
        ));
    });
}

#[test]
fn a_number_is_read_as_written() {
    with_showing(1, 0, |showing| {
        let read =
            <Whole<Amount> as Single>::read(&written(r#"{"amount":30}"#, Ended::Complete), showing);
        assert_eq!(read, Settled::Answered(Amount { amount: 30 }));
    });
}

#[test]
fn a_number_written_as_text_a_fraction_or_a_negative_is_not_read() {
    with_showing(1, 0, |showing| {
        for broken in [
            r#"{"amount":"30"}"#,
            r#"{"amount":1.5}"#,
            r#"{"amount":-1}"#,
        ] {
            let read = <Whole<Amount> as Single>::read(&written(broken, Ended::Complete), showing);
            assert_eq!(read, Settled::Unanswered(Why::NotRead), "{broken}");
        }
    });
}

#[test]
fn a_number_field_is_an_integer_schema_without_a_range() {
    with_showing(1, 0, |showing| {
        let schema = schema_of::<Whole<Amount>>(showing, Held::Enums);
        assert_eq!(schema["properties"]["amount"], json!({"type": "integer"}));
        assert_dialect(&schema);
    });
}

// ---- Pick ----

proptest! {
    #[test]
    fn pick_is_never_a_different_answer_in_a_cut(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let handle = if n == 0 || rng.chance(20) {
                "none".to_owned()
            } else {
                handle_of(1 + rng.below(n))
            };
            let text = json!({ "pick": handle }).to_string();
            assert_valid::<Pick<Sheet>>(showing, &text);
            let full = via_pick(&text, Ended::Complete, showing);
            match &full {
                Settled::Answered(None) => assert_eq!(handle, "none"),
                Settled::Answered(Some(picked)) => assert_eq!(picked.handle.written(), handle),
                other => panic!("{other:?}"),
            }
            for (_, cut) in cuts(&text) {
                let read = via_pick(cut, Ended::CutOff, showing);
                assert!(read == full || read == Settled::Unanswered(Why::CutOff), "{cut}");
            }
        });
    }
}

#[test]
fn pick_reasons() {
    with_showing(3, 0, |showing| {
        for (text, want) in [
            (r#"{"pick":"s99"}"#, Why::Unplaced),
            (r#"{"pick":"[s1] text"}"#, Why::Unplaced),
            (r#"{"pick":"[[s1]]"}"#, Why::Unplaced),
            (r#"{"pick":"[s1"}"#, Why::Unplaced),
            (r#"{"pick":"None"}"#, Why::Unplaced),
            (r#"{"pick":1}"#, Why::NotRead),
            (r#"{"choice":"s1"}"#, Why::NotRead),
        ] {
            assert_eq!(
                via_pick(text, Ended::Complete, showing),
                Settled::Unanswered(want),
                "{text}"
            );
        }
        assert_eq!(
            via_pick("{", Ended::CutOff, showing),
            Settled::Unanswered(Why::CutOff)
        );
        assert_eq!(
            via_pick("{\"pick\":\"s1\"}", Ended::Declined, showing),
            Settled::Unanswered(Why::Declined)
        );
    });
}

#[test]
fn a_field_holding_the_prose_form_of_a_shown_handle_resolves() {
    with_showing(3, 0, |showing| {
        let Settled::Answered(Some(picked)) =
            via_pick(r#"{"pick":"[s2]"}"#, Ended::Complete, showing)
        else {
            panic!("the bracketed handle did not resolve");
        };
        assert_eq!(picked.handle.written(), "s2");
    });
}

// ---- PickSome ----

proptest! {
    #[test]
    fn pick_some_is_unanswered_unless_whole(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut handles: Vec<String> = Vec::new();
            if n > 0 {
                for _ in 0..rng.below(8) {
                    handles.push(handle_of(1 + rng.below(n)));
                }
            }
            let text = json!({ "picks": handles }).to_string();
            assert_valid::<PickSome<Sheet>>(showing, &text);
            let mut once: Vec<String> = Vec::new();
            for handle in &handles {
                if !once.contains(handle) {
                    once.push(handle.clone());
                }
            }
            let Settled::Answered(picked) = via_pick_some(&text, Ended::Complete, showing) else {
                panic!("whole picks were not read");
            };
            let got: Vec<String> = picked.iter().map(|p| p.handle.written()).collect();
            assert_eq!(got, once);
            for (_, cut) in cuts(&text) {
                assert_eq!(
                    via_pick_some(cut, Ended::CutOff, showing),
                    Settled::Unanswered(Why::CutOff)
                );
            }
        });
    }
}

#[test]
fn pick_some_reasons() {
    with_showing(3, 0, |showing| {
        assert_eq!(
            via_pick_some(r#"{"picks":[]}"#, Ended::Complete, showing),
            Settled::Answered(vec![])
        );
        assert_eq!(
            via_pick_some(r#"{"picks":["s1","s99"]}"#, Ended::Complete, showing),
            Settled::Unanswered(Why::Unplaced)
        );
        assert_eq!(
            via_pick_some(r#"{"picks":["s1",2]}"#, Ended::Complete, showing),
            Settled::Unanswered(Why::NotRead)
        );
        assert_eq!(
            via_pick_some(r#"{"picks":["s1","s2""#, Ended::CutOff, showing),
            Settled::Unanswered(Why::CutOff)
        );
        assert_eq!(
            via_pick_some(r#"{"picks":["s1"]}"#, Ended::Declined, showing),
            Settled::Unanswered(Why::Declined)
        );
    });
}

// ---- Order ----

proptest! {
    #[test]
    fn order_is_every_shown_thing_once_or_nothing(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut order: Vec<String> = (1..=n).map(handle_of).collect();
            for at in (1..order.len()).rev() {
                order.swap(at, rng.below(at + 1));
            }
            let text = json!({ "reason": "r", "order": order }).to_string();
            assert_valid::<Whole<Ranked>>(showing, &text);
            let Settled::Answered(read) = via_order(&text, Ended::Complete, showing) else {
                panic!("a whole order was not read");
            };
            let got: Vec<String> = read.iter().map(|p| p.handle.written()).collect();
            assert_eq!(got, order);
            for (_, cut) in cuts(&text) {
                assert_eq!(
                    via_order(cut, Ended::CutOff, showing),
                    Settled::Unanswered(Why::CutOff)
                );
            }
            if let Some(dropped) = order.pop() {
                let text = json!({ "reason": "r", "order": order }).to_string();
                let Settled::Unanswered(Why::Contradicted(told)) =
                    via_order(&text, Ended::Complete, showing)
                else {
                    panic!("an order lacking a thing was accepted");
                };
                assert_eq!(told, format!("The order lacks {dropped}."));
                order.push(dropped.clone());
                order.push(dropped.clone());
                let text = json!({ "reason": "r", "order": order }).to_string();
                let Settled::Unanswered(Why::Contradicted(told)) =
                    via_order(&text, Ended::Complete, showing)
                else {
                    panic!("an order repeating a thing was accepted");
                };
                assert_eq!(told, format!("The order holds {dropped} twice."));
            }
        });
    }
}

#[test]
fn order_reasons() {
    with_showing(2, 0, |showing| {
        assert_eq!(
            via_order(
                r#"{"reason":"r","order":["s1","s99"]}"#,
                Ended::Complete,
                showing
            ),
            Settled::Unanswered(Why::Unplaced)
        );
        assert_eq!(
            via_order(r#"{"reason":"r","order":"s1"}"#, Ended::Complete, showing),
            Settled::Unanswered(Why::NotRead)
        );
        assert_eq!(
            via_order(
                r#"{"reason":"r","order":["s2","s1"]}"#,
                Ended::Declined,
                showing
            ),
            Settled::Unanswered(Why::Declined)
        );
    });
}

#[test]
fn an_order_reads_with_the_fields_beside_it() {
    with_showing(2, 0, |showing| {
        let Settled::Answered(read) = <Whole<Ranked> as Single>::read(
            &written(r#"{"reason":"why","order":["s2","s1"]}"#, Ended::Complete),
            showing,
        ) else {
            panic!("an order was not read");
        };
        assert_eq!(read.reason, "why");
    });
}

// ---- cite_some ----

#[test]
fn an_empty_cite_some_is_unanswered_when_things_are_shown() {
    with_showing(2, 0, |showing| {
        let told = "cites names none of the things shown: name at least one.";
        for text in [r#"{"cites":[]}"#, r#"{"cites":["s9"]}"#] {
            assert_eq!(
                <Whole<Backed> as Single>::read(&written(text, Ended::Complete), showing),
                Settled::Unanswered(Why::Refused(told.to_owned()))
            );
        }
        assert_eq!(
            <Whole<Backed> as Single>::read(
                &written(r#"{"cites":["s1"]}"#, Ended::Complete),
                showing
            ),
            Settled::Answered(Backed {
                cites: vec![showing.resolve::<Sheet>("s1").unwrap()]
            })
        );
    });
}

#[test]
fn an_empty_cite_some_is_answered_when_nothing_is_shown() {
    with_showing(0, 0, |showing| {
        assert_eq!(
            <Whole<Backed> as Single>::read(&written(r#"{"cites":[]}"#, Ended::Complete), showing),
            Settled::Answered(Backed { cites: Vec::new() })
        );
    });
}

// ---- place ----

fn first_request(entry: usize) -> Place {
    Place { request: 0, entry }
}

#[test]
fn a_mark_holds_the_place_it_was_written_at() {
    with_showing(3, 0, |showing| {
        let text =
            r#"{"marks":[{"at":"s3","note":"a"},{"at":"s1","note":"b"},{"at":"s2","note":"c"}]}"#;
        let rows =
            <Flagged<Sheet, Placed> as PerItem>::read(&written(text, Ended::Complete), showing);
        let places: Vec<(String, Place)> = rows
            .into_iter()
            .map(|(item, settled)| match settled {
                Settled::Answered(Some(placed)) => (item.handle.written(), placed.place),
                other => panic!("a mark was not read: {other:?}"),
            })
            .collect();
        assert_eq!(
            places,
            vec![
                ("s1".to_owned(), first_request(1)),
                ("s2".to_owned(), first_request(2)),
                ("s3".to_owned(), first_request(0))
            ]
        );
    });
}

#[test]
fn a_place_counts_the_elements_that_were_skipped() {
    with_showing(2, 0, |showing| {
        let text = r#"{"marks":[{"at":"s99","note":"a"},{"at":"s2","note":"b"}]}"#;
        let rows =
            <Flagged<Sheet, Placed> as PerItem>::read(&written(text, Ended::Complete), showing);
        let second = rows.into_iter().nth(1).map(|(_, settled)| settled);
        assert_eq!(
            second,
            Some(Settled::Answered(Some(Placed {
                place: first_request(1)
            })))
        );
    });
}

#[test]
fn a_new_item_holds_the_place_it_was_written_at() {
    with_showing(1, 0, |showing| {
        let text = r#"{"items":[{"note":"a"},{"note":"b"}]}"#;
        let found = <New<Placed> as Listed>::read(&written(text, Ended::Complete), showing);
        assert_eq!(
            found.items,
            vec![
                Placed {
                    place: first_request(0)
                },
                Placed {
                    place: first_request(1)
                }
            ]
        );
    });
}

#[test]
fn a_mark_of_a_later_request_is_placed_after_every_mark_of_an_earlier_one() {
    let sheets: Vec<Sheet> = (0..3).map(Sheet).collect();
    let mut scope = Scope::for_question();
    let mut place_of = |text: &str, handle: &str| {
        let mut showing = scope.request();
        showing.show(&sheets).unwrap();
        <Flagged<Sheet, Placed> as PerItem>::read(&written(text, Ended::Complete), &showing)
            .into_iter()
            .find_map(|(item, settled)| match settled {
                Settled::Answered(Some(placed)) if item.handle.written() == handle => {
                    Some(placed.place)
                }
                _ => None,
            })
            .unwrap()
    };
    let last_of_the_first = place_of(
        r#"{"marks":[{"at":"s3","note":"a"},{"at":"s1","note":"b"}]}"#,
        "s1",
    );
    let first_of_the_second = place_of(r#"{"marks":[{"at":"s2","note":"c"}]}"#, "s2");
    assert!(last_of_the_first < first_of_the_second);
}

// ---- New ----

proptest! {
    #[test]
    fn new_keeps_exactly_the_elements_that_closed(count in 0usize..=6, bad in any::<bool>(), seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(4, 0, |showing| {
            let mut elements = Vec::new();
            let mut readable = Vec::new();
            for number in 1..=count {
                let cites = random_cites(&mut rng, 4);
                elements.push(note_object(None, number, &cites));
                readable.push(true);
                if bad && number == 1 {
                    elements.push(r#"{"verdict":"maybe","text":"x","cites":[]}"#.to_owned());
                    readable.push(false);
                }
            }
            let built = list_doc("items", &elements);
            let valid: Vec<&String> = elements
                .iter()
                .zip(&readable)
                .filter(|(_, ok)| **ok)
                .map(|(element, _)| element)
                .collect();
            if !bad {
                assert_valid::<New<Note>>(showing, &built.text);
            }
            let full = via_new(&built.text, Ended::Complete, showing);
            assert_eq!(full.items.len(), valid.len());
            assert_eq!(full.rest, None);
            for (cut_at, cut) in cuts(&built.text) {
                let found = via_new(cut, Ended::CutOff, showing);
                let closed: Vec<&String> = elements
                    .iter()
                    .zip(&readable)
                    .zip(&built.ends)
                    .filter(|((_, ok), end)| **ok && **end <= cut_at)
                    .map(|((element, _), _)| element)
                    .collect();
                assert_eq!(found.items.len(), closed.len(), "cut at {cut_at}");
                for (item, element) in found.items.iter().zip(&closed) {
                    let want: Value = serde_json::from_str(element).unwrap();
                    assert_eq!(Some(item.text.as_str()), want["text"].as_str());
                }
                assert_ne!(found.rest, None, "cut at {cut_at}");
            }
        });
    }
}

#[test]
fn new_reasons() {
    with_showing(2, 0, |showing| {
        let found = via_new("", Ended::Declined, showing);
        assert_eq!((found.items.len(), found.rest), (0, Some(Why::Declined)));
        let found = via_new("text only", Ended::Complete, showing);
        assert_eq!((found.items.len(), found.rest), (0, Some(Why::NotRead)));
        let found = via_new("{\"items\":[]}", Ended::Complete, showing);
        assert_eq!((found.items.len(), found.rest), (0, None));
        let found = via_new("{\"other\":[]}", Ended::Complete, showing);
        assert_eq!((found.items.len(), found.rest), (0, Some(Why::NotRead)));
        let found = via_new("{\"items\":[]", Ended::CutOff, showing);
        assert_eq!((found.items.len(), found.rest), (0, Some(Why::CutOff)));
    });
}

#[test]
fn new_counts_the_items_it_leaves_out_and_not_a_cut_off_tail() {
    let bad = r#"{"verdict":"maybe","text":"x","cites":[]}"#;
    with_showing(2, 0, |showing| {
        let found = via_new(&format!(r#"{{"items":[{bad}]}}"#), Ended::Complete, showing);
        assert_eq!(
            (found.items.len(), found.left_out, found.rest),
            (0, 1, None)
        );
        let found = via_new("{\"items\":[]}", Ended::Complete, showing);
        assert_eq!(found.left_out, 0);
        let found = via_new(
            &format!(r#"{{"items":[{bad},{{"verdict":"sup"#),
            Ended::CutOff,
            showing,
        );
        assert_eq!((found.left_out, found.rest), (1, Some(Why::CutOff)));
    });
}

// ---- Each ----

proptest! {
    #[test]
    fn each_answers_exactly_the_items_with_a_closed_entry(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut elements = Vec::new();
            let mut owners = Vec::new();
            let mut cites_of = vec![Vec::new(); n + 1];
            for (number, slot) in cites_of.iter_mut().enumerate().skip(1) {
                let cites = random_cites(&mut rng, n);
                for _ in 0..rng.below(3) {
                    elements.push(note_object(Some(("of", handle_of(number).as_str())), number, &cites));
                    owners.push(number);
                }
                *slot = cites;
                if rng.chance(15) {
                    elements.push(note_object(Some(("of", "s99")), 1, &[]));
                    owners.push(0);
                }
            }
            let built = list_doc("entries", &elements);
            if owners.iter().all(|owner| *owner != 0) {
                assert_valid::<Each<Sheet, Note>>(showing, &built.text);
            }
            let full = via_each(&built.text, Ended::Complete, showing);
            assert_in_order(showing, &full);
            for (at, (_, settled)) in full.iter().enumerate() {
                let number = at + 1;
                if owners.contains(&number) {
                    assert_eq!(
                        *settled,
                        Settled::Answered(expected_note(number, &cites_of[number], showing))
                    );
                } else {
                    assert_eq!(*settled, Settled::Unanswered(Why::Missing));
                }
            }
            for (cut_at, cut) in cuts(&built.text) {
                let read = via_each(cut, Ended::CutOff, showing);
                assert_in_order(showing, &read);
                for (at, (_, settled)) in read.iter().enumerate() {
                    let number = at + 1;
                    let closed = owners
                        .iter()
                        .zip(&built.ends)
                        .any(|(owner, end)| *owner == number && *end <= cut_at);
                    if closed {
                        assert_eq!(
                            *settled,
                            Settled::Answered(expected_note(number, &cites_of[number], showing))
                        );
                    } else {
                        assert_eq!(*settled, Settled::Unanswered(Why::CutOff));
                    }
                }
            }
        });
    }
}

#[test]
fn each_contradicted_and_equal_entries() {
    with_showing(2, 0, |showing| {
        let one = note_object(Some(("of", "s1")), 1, &[]);
        let other = note_object(Some(("of", "s1")), 2, &[]);
        let text = list_doc("entries", &[one.clone(), one.clone(), other]).text;
        let read = via_each(&text, Ended::Complete, showing);
        assert_eq!(
            read.first().map(|(_, settled)| settled.clone_why()),
            Some(Some(Why::Contradicted(
                "s1 has two entries that differ.".to_owned()
            )))
        );
        let text = list_doc("entries", &[one.clone(), one]).text;
        let read = via_each(&text, Ended::Complete, showing);
        assert!(matches!(read.first(), Some((_, Settled::Answered(_)))));
        assert!(matches!(
            read.get(1),
            Some((_, Settled::Unanswered(Why::Missing)))
        ));
    });
}

#[test]
fn each_reasons() {
    with_showing(2, 0, |showing| {
        for (text, ended, want) in [
            ("", Ended::Declined, Why::Declined),
            ("prose", Ended::Complete, Why::NotRead),
            ("prose", Ended::CutOff, Why::CutOff),
            ("{\"entries\":3}", Ended::Complete, Why::NotRead),
            ("{\"rows\":[]}", Ended::Complete, Why::NotRead),
            ("{\"entries\":[]}", Ended::Complete, Why::Missing),
            ("{\"entries\":[]", Ended::CutOff, Why::CutOff),
        ] {
            let read = via_each(text, ended, showing);
            assert_in_order(showing, &read);
            assert!(
                read.iter()
                    .all(|(_, settled)| *settled == Settled::Unanswered(want.clone())),
                "{text:?}"
            );
        }
    });
}

/// An entry with a field of every kind; `stray` adds pointers that do not resolve and repeats.
fn rich_entry(number: usize, stray: bool) -> String {
    let mut cites = vec!["s1"];
    let mut own_cites = vec!["o2"];
    let mut per_opt = vec![
        json!({ "of": "o2", "label": "y" }),
        json!({ "of": "o1", "label": "x" }),
    ];
    if stray {
        cites.extend(["s99", "s1"]);
        own_cites.push("o9");
        per_opt.push(json!({ "of": "o1", "label": "x" }));
        per_opt.push(json!({ "of": "o9", "label": "z" }));
    }
    json!({
        "of": handle_of(number),
        "text": "words",
        "verdict": "yes",
        "amount": 30,
        "sheet": "s2",
        "maybe": "none",
        "opt": "o1",
        "maybe_opt": "o2",
        "cites": cites,
        "own_cites": own_cites,
        "inner": { "label": "a" },
        "inners": [{ "label": "b" }, { "label": "c" }],
        "per_opt": per_opt,
        "edit": {
            "replace": [{ "at": "s1", "with": "new" }],
            "insert": [{ "after": "start", "with": "added" }],
            "remove": ["s2"],
            "move": []
        },
        "choice": { "kind": "pointing", "at": "s3" }
    })
    .to_string()
}

#[test]
fn each_reads_a_statement_with_a_field_of_every_kind() {
    with_showing(3, 2, |showing| {
        for stray in [false, true] {
            let elements: Vec<String> = (1..=3).map(|number| rich_entry(number, stray)).collect();
            let text = list_doc("entries", &elements).text;
            if !stray {
                assert_valid::<Each<Sheet, Rich>>(showing, &text);
            }
            let read =
                <Each<Sheet, Rich> as PerItem>::read(&written(&text, Ended::Complete), showing);
            assert_in_order(showing, &read);
            for (_, settled) in &read {
                let Settled::Answered(rich) = settled else {
                    panic!("{settled:?}");
                };
                assert_eq!(rich.amount, 30);
                assert_eq!(rich.cites.len(), 1);
                assert_eq!(rich.own_cites.len(), 1);
                assert_eq!(rich.maybe, None);
                assert_eq!(rich.opt.handle.written(), "o1");
                assert_eq!(
                    rich.maybe_opt.as_ref().map(|own| own.handle.written()),
                    Some("o2".to_owned())
                );
                let labels: Vec<(String, &str)> = rich
                    .per_opt
                    .iter()
                    .map(|(own, inner)| (own.handle.written(), inner.label.as_str()))
                    .collect();
                assert_eq!(labels, vec![("o1".to_owned(), "x"), ("o2".to_owned(), "y")]);
                assert_eq!(rich.edit.change().replace.len(), 1);
                assert!(matches!(rich.choice, Choice::Pointing { .. }));
            }
        }
    });
}

#[test]
fn an_inner_list_that_lacks_an_own_thing_leaves_the_entry_unanswered() {
    with_showing(1, 2, |showing| {
        let entry = json!({
            "of": "s1", "text": "w", "verdict": "no", "amount": 5, "sheet": "s1", "maybe": "none",
            "opt": "o1", "maybe_opt": "none", "cites": [], "own_cites": [],
            "inner": { "label": "a" }, "inners": [],
            "per_opt": [{ "of": "o1", "label": "x" }],
            "edit": { "replace": [], "insert": [], "remove": [], "move": [] },
            "choice": { "kind": "plain", "note": "n" }
        })
        .to_string();
        let text = list_doc("entries", &[entry]).text;
        let read = <Each<Sheet, Rich> as PerItem>::read(&written(&text, Ended::Complete), showing);
        assert!(matches!(
            read.first(),
            Some((_, Settled::Unanswered(Why::Missing)))
        ));
    });
}

// ---- Flagged ----

proptest! {
    #[test]
    fn flagged_is_unmarked_only_in_a_whole_answer(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut elements = Vec::new();
            let mut marked = Vec::new();
            for number in 1..=n {
                if rng.chance(40) {
                    elements.push(note_object(Some(("at", handle_of(number).as_str())), number, &[]));
                    marked.push(number);
                }
            }
            let built = list_doc("marks", &elements);
            assert_valid::<Flagged<Sheet, Note>>(showing, &built.text);
            let full = via_flagged(&built.text, Ended::Complete, showing);
            assert_in_order(showing, &full);
            for (at, (_, settled)) in full.iter().enumerate() {
                let number = at + 1;
                if marked.contains(&number) {
                    assert_eq!(
                        *settled,
                        Settled::Answered(Some(expected_note(number, &[], showing)))
                    );
                } else {
                    assert_eq!(*settled, Settled::Answered(None));
                }
            }
            for (cut_at, cut) in cuts(&built.text) {
                let read = via_flagged(cut, Ended::CutOff, showing);
                assert_in_order(showing, &read);
                for (at, (_, settled)) in read.iter().enumerate() {
                    let number = at + 1;
                    let closed = marked
                        .iter()
                        .zip(&built.ends)
                        .any(|(mark, end)| *mark == number && *end <= cut_at);
                    if closed {
                        assert_eq!(
                            *settled,
                            Settled::Answered(Some(expected_note(number, &[], showing)))
                        );
                    } else {
                        assert_eq!(*settled, Settled::Unanswered(Why::CutOff));
                    }
                }
            }
        });
    }
}

#[test]
fn flagged_with_a_mark_that_does_not_place_clears_nothing() {
    with_showing(3, 0, |showing| {
        let placed = note_object(Some(("at", "s2")), 2, &[]);
        let stray = note_object(Some(("at", "s99")), 1, &[]);
        let text = list_doc("marks", &[placed, stray]).text;
        let read = via_flagged(&text, Ended::Complete, showing);
        assert!(matches!(read.get(1), Some((_, Settled::Answered(Some(_))))));
        assert_eq!(
            read.first().map(|(_, settled)| settled.clone_why()),
            Some(Some(Why::Unplaced))
        );
        let differing = list_doc(
            "marks",
            &[
                note_object(Some(("at", "s1")), 1, &[]),
                note_object(Some(("at", "s1")), 2, &[]),
            ],
        )
        .text;
        let read = via_flagged(&differing, Ended::Complete, showing);
        assert_eq!(
            read.first().map(|(_, settled)| settled.clone_why()),
            Some(Some(Why::Contradicted(
                "s1 has two entries that differ.".to_owned()
            )))
        );
    });
}

#[test]
fn flagged_reasons() {
    with_showing(2, 0, |showing| {
        for (text, ended, want) in [
            ("", Ended::Declined, Why::Declined),
            ("prose", Ended::Complete, Why::NotRead),
            ("prose", Ended::CutOff, Why::CutOff),
            ("{\"marks\":", Ended::CutOff, Why::CutOff),
            ("{\"rows\":[]}", Ended::Complete, Why::NotRead),
        ] {
            let read = via_flagged(text, ended, showing);
            assert_in_order(showing, &read);
            assert!(
                read.iter()
                    .all(|(_, settled)| *settled == Settled::Unanswered(want.clone())),
                "{text:?}"
            );
        }
    });
}

/// The reason a settled statement is unanswered, for comparing without the value.
trait CloneWhy {
    fn clone_why(&self) -> Option<Why>;
}

impl<V> CloneWhy for Settled<V> {
    fn clone_why(&self) -> Option<Why> {
        match self {
            Settled::Answered(_) => None,
            Settled::Unanswered(why) => Some(why.clone()),
        }
    }
}

// ---- Change ----

proptest! {
    #[test]
    fn change_is_unanswered_unless_whole(n in 1usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let (mut replace, mut remove, mut insert) = (Vec::new(), Vec::new(), Vec::new());
            for number in 1..=n {
                match rng.below(3) {
                    0 => replace.push(json!({ "at": handle_of(number), "with": format!("r{number}") })),
                    1 => remove.push(handle_of(number)),
                    _ => {}
                }
                if rng.chance(30) {
                    let after = if rng.chance(50) { "start".to_owned() } else { handle_of(1 + rng.below(n)) };
                    insert.push(json!({ "after": after, "with": format!("i{number}") }));
                }
            }
            let text = json!({
                "replace": replace, "insert": insert, "remove": remove, "move": []
            })
            .to_string();
            assert_valid::<Change<Sheet, String>>(showing, &text);
            let Settled::Answered(change) = via_change(&text, Ended::Complete, showing) else {
                panic!("a consistent change was not read");
            };
            let change = change.change();
            assert_eq!(change.replace.len(), replace.len());
            assert_eq!(change.insert.len(), insert.len());
            assert_eq!(change.remove.len(), remove.len());
            for (_, cut) in cuts(&text) {
                assert_eq!(
                    via_change(cut, Ended::CutOff, showing),
                    Settled::Unanswered(Why::CutOff)
                );
            }
        });
    }
}

#[test]
fn change_reasons() {
    with_showing(3, 0, |showing| {
        let empty = r#"{"replace":[],"insert":[],"remove":[],"move":[]}"#;
        assert!(matches!(
            via_change(empty, Ended::Complete, showing),
            Settled::Answered(change) if change.change().is_empty()
        ));
        for (text, want) in [
            (
                r#"{"replace":[],"insert":[],"remove":["s99"],"move":[]}"#,
                Why::Unplaced,
            ),
            (
                r#"{"replace":[{"at":"s9","with":"x"}],"insert":[],"remove":[],"move":[]}"#,
                Why::Unplaced,
            ),
            (
                r#"{"replace":[],"insert":[{"after":"s9","with":"x"}],"remove":[],"move":[]}"#,
                Why::Unplaced,
            ),
            (
                r#"{"replace":[],"insert":[],"remove":[],"move":[{"at":"s1","after":"nowhere"}]}"#,
                Why::Unplaced,
            ),
            (r#"{"replace":[],"insert":[],"remove":[]}"#, Why::NotRead),
            (
                r#"{"replace":[{"at":"s1"}],"insert":[],"remove":[],"move":[]}"#,
                Why::NotRead,
            ),
        ] {
            assert_eq!(
                via_change(text, Ended::Complete, showing),
                Settled::Unanswered(want),
                "{text}"
            );
        }
        let twice = r#"{"replace":[{"at":"s1","with":"x"}],"insert":[],"remove":["s1"],"move":[]}"#;
        assert!(matches!(
            via_change(twice, Ended::Complete, showing),
            Settled::Unanswered(Why::Contradicted(_))
        ));
        let anchored = r#"{"replace":[],"insert":[{"after":"start","with":"x"}],"remove":[],"move":[{"at":"s2","after":"s1"}]}"#;
        assert!(matches!(
            via_change(anchored, Ended::Complete, showing),
            Settled::Answered(_)
        ));
        assert_eq!(
            via_change(empty, Ended::Declined, showing),
            Settled::Unanswered(Why::Declined)
        );
    });
}

// ---- Lead ----

#[test]
fn lead_reads_verdict_text_and_pointer() {
    with_showing(3, 0, |showing| {
        let read = via_lead(
            "yes\nThe sheet shows it plainly.\ns2",
            Ended::Complete,
            showing,
        );
        let Settled::Answered(led) = read else {
            panic!("{read:?}");
        };
        assert_eq!(led.verdict, Verdict::Yes);
        assert_eq!(led.text, "The sheet shows it plainly.");
        assert_eq!(
            led.pointer.map(|p| p.handle.written()),
            Some("s2".to_owned())
        );

        let read = via_lead(
            "  no.  Because.\n\nmore words\n none \n",
            Ended::Complete,
            showing,
        );
        let Settled::Answered(led) = read else {
            panic!("{read:?}");
        };
        assert_eq!(led.verdict, Verdict::No);
        assert_eq!(led.text, "Because.\n\nmore words");
        assert!(led.pointer.is_none());

        let read = via_lead("yes:\ns1", Ended::Complete, showing);
        assert!(matches!(read, Settled::Answered(led) if led.text.is_empty()));

        let read = via_lead("yes\nwords\n[s1]", Ended::Complete, showing);
        assert!(matches!(read, Settled::Answered(led) if led.pointer.is_some()));

        let read = via_lead("yes\nwords\nNone.", Ended::Complete, showing);
        assert!(matches!(read, Settled::Answered(led) if led.pointer.is_none()));
    });
}

#[test]
fn lead_reasons() {
    with_showing(3, 0, |showing| {
        for (text, ended, want) in [
            ("maybe\nwords\ns1", Ended::Complete, Why::NotRead),
            ("Yes\nwords\ns1", Ended::Complete, Why::NotRead),
            ("", Ended::Complete, Why::NotRead),
            ("yes\nwords\ns99", Ended::Complete, Why::Unplaced),
            ("yes\nwords\n[[s1]]", Ended::Complete, Why::Unplaced),
            (
                "yes\nwords\nThe answer is s1",
                Ended::Complete,
                Why::Unplaced,
            ),
            ("yes", Ended::Complete, Why::Unplaced),
            ("yes s1", Ended::Complete, Why::Unplaced),
            ("yes\nwords\ns1", Ended::CutOff, Why::CutOff),
            ("yes\nwords\ns1", Ended::Declined, Why::Declined),
        ] {
            assert_eq!(
                via_lead(text, ended, showing),
                Settled::Unanswered(want),
                "{text:?}"
            );
        }
    });
}

proptest! {
    #[test]
    fn lead_cut_off_is_always_cut_off(n in 0usize..=12, words in "[a-z ]{0,30}") {
        with_showing(n, 0, |showing| {
            let handle = if n == 0 { "none".to_owned() } else { handle_of(n) };
            let text = format!("yes\n{words}\n{handle}");
            assert!(matches!(via_lead(&text, Ended::Complete, showing), Settled::Answered(_)));
            for (_, cut) in cuts(&text) {
                assert_eq!(via_lead(cut, Ended::CutOff, showing), Settled::Unanswered(Why::CutOff));
            }
        });
    }
}

// ---- Keeps ----

#[test]
fn keeps_reads_every_spelling_of_a_line() {
    with_showing(4, 3, |showing| {
        let text = "s1: o1 o2\n[s2] [o1], [o3]\n  s3: o2,o3  \n\ns4\to3 o1\n";
        let read = via_keeps(text, Ended::Complete, showing);
        assert_in_order(showing, &read);
        assert_eq!(
            kept(&read),
            vec![
                handles(&["o1", "o2"]),
                handles(&["o1", "o3"]),
                handles(&["o2", "o3"]),
                handles(&["o3", "o1"]),
            ]
        );
    });
}

#[test]
fn keeps_counts_a_handle_once_in_written_order() {
    with_showing(2, 3, |showing| {
        let read = via_keeps("s1: o3 o1 o3 o2 o1", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o3", "o1", "o2"]), handles(&[])]
        );
    });
}

#[test]
fn an_item_line_with_no_own_handles_keeps_nothing() {
    with_showing(3, 2, |showing| {
        let read = via_keeps("s1\ns2: o1\ns3:", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&[]), handles(&["o1"]), handles(&[])]
        );
    });
}

#[test]
fn the_word_none_says_every_item_keeps_nothing() {
    with_showing(3, 2, |showing| {
        for text in ["none", "  none \n", "\n\nnone"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![handles(&[]), handles(&[]), handles(&[])],
                "{text:?}"
            );
        }
        assert_eq!(
            kept(&via_keeps("s1: o1\nnone", Ended::Complete, showing)),
            vec![handles(&["o1"]), handles(&[]), handles(&[])]
        );
        assert_eq!(
            kept(&via_keeps("none", Ended::CutOff, showing)),
            vec![Err(Why::CutOff), Err(Why::CutOff), Err(Why::CutOff)]
        );
    });
}

#[test]
fn an_empty_answer_is_not_read_and_a_declined_one_is_declined() {
    with_showing(2, 2, |showing| {
        for text in ["", " \n\n  "] {
            assert_eq!(
                kept(&via_keeps(text, Ended::Complete, showing)),
                vec![Err(Why::NotRead), Err(Why::NotRead)],
                "{text:?}"
            );
            assert_eq!(
                kept(&via_keeps(text, Ended::CutOff, showing)),
                vec![Err(Why::CutOff), Err(Why::CutOff)],
                "{text:?}"
            );
        }
        assert_eq!(
            kept(&via_keeps("s1: o1", Ended::Declined, showing)),
            vec![Err(Why::Declined), Err(Why::Declined)]
        );
    });
}

#[test]
fn a_cut_off_answer_drops_its_last_line_and_leaves_the_rest_cut_off() {
    with_showing(3, 2, |showing| {
        let read = via_keeps("s1: o1\ns2: o2", Ended::CutOff, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), Err(Why::CutOff), Err(Why::CutOff)]
        );
        let read = via_keeps("s1: o1\ns2: o2\n\n", Ended::CutOff, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), Err(Why::CutOff), Err(Why::CutOff)]
        );
        let read = via_keeps("s1: o1\ns2: o2\n", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), handles(&["o2"]), handles(&[])]
        );
    });
}

#[test]
fn an_own_handle_that_does_not_resolve_leaves_only_its_item_unplaced() {
    with_showing(3, 2, |showing| {
        for line in ["s1: o9", "s1: o1 o9"] {
            let text = format!("{line}\ns2: o1");
            let read = via_keeps(&text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![Err(Why::Unplaced), handles(&["o1"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn an_item_handle_that_is_not_shown_leaves_the_unnamed_items_unplaced() {
    with_showing(3, 2, |showing| {
        for line in ["s9: o1", "o1: o1", "[s9]: o1"] {
            let text = format!("s2: o1\n{line}");
            let read = via_keeps(&text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![Err(Why::Unplaced), handles(&["o1"]), Err(Why::Unplaced)],
                "{text:?}"
            );
        }
    });
}

#[test]
fn words_that_are_not_handles_are_passed_over() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("Here is my answer:\ns1: o1", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), handles(&[]), handles(&[])]
        );
        let read = via_keeps("- s1: o1\n* s2: o2\n1. s3: o3", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), handles(&["o2"]), handles(&["o3"])]
        );
        let read = via_keeps("**s1**: `o1`", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1"]), handles(&[]), handles(&[])]
        );
    });
}

#[test]
fn items_on_one_line_each_keep_their_own() {
    with_showing(3, 3, |showing| {
        for text in ["s1: o1 o2; s2: o3", "s1: o1, o2, s2: o3"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![handles(&["o1", "o2"]), handles(&["o3"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn handles_are_read_whatever_punctuation_surrounds_them() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("s1:o1,o2.\n[s2]: [o3].", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2"]), handles(&["o3"]), handles(&[])]
        );
    });
}

#[test]
fn an_answer_naming_no_item_is_not_read_unless_it_says_none() {
    with_showing(3, 2, |showing| {
        for text in ["None", "none.", "**NONE**"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![handles(&[]), handles(&[]), handles(&[])],
                "{text:?}"
            );
        }
        let read = via_keeps(
            "I found nothing that bears on it.",
            Ended::Complete,
            showing,
        );
        assert_eq!(
            kept(&read),
            vec![Err(Why::NotRead), Err(Why::NotRead), Err(Why::NotRead)]
        );
    });
}

#[test]
fn an_own_handle_before_any_item_leaves_the_unnamed_items_unplaced() {
    with_showing(3, 2, |showing| {
        let read = via_keeps("o1\ns2: o1", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![Err(Why::Unplaced), handles(&["o1"]), Err(Why::Unplaced)]
        );
    });
}

#[test]
fn an_own_handle_on_a_line_after_an_unknown_word_is_not_given_to_the_line_above() {
    with_showing(3, 2, |showing| {
        let read = via_keeps("s2: o1\nx1: o2", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![Err(Why::Unplaced), handles(&["o1"]), Err(Why::Unplaced)]
        );
    });
}

#[test]
fn equal_lines_for_one_item_count_once_and_differing_lines_contradict() {
    with_showing(3, 2, |showing| {
        let read = via_keeps("s1: o1 o2\ns2: o1\ns1: o1 o2", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2"]), handles(&["o1"]), handles(&[])]
        );
        let read = via_keeps("s1: o1\ns1: o2", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![
                Err(Why::Contradicted(
                    "s1 has two entries that differ.".to_owned()
                )),
                handles(&[]),
                handles(&[]),
            ]
        );
    });
}

#[test]
fn a_range_of_own_handles_keeps_every_handle_from_the_first_through_the_second() {
    with_showing(3, 4, |showing| {
        for text in [
            "s1: o1-o4",
            "s1: o1–o4",
            "s1: o1 - o4",
            "s1: o1..o4",
            "s1: o1...o4",
            "s1: o1…o4",
            "s1: o1—o4",
            "s1: o1-4",
            "s1: o1 – 4",
            "s1: o1 -- o4",
        ] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![
                    handles(&["o1", "o2", "o3", "o4"]),
                    handles(&[]),
                    handles(&[])
                ],
                "{text:?}"
            );
        }
    });
}

#[test]
fn a_range_counts_a_handle_already_kept_once() {
    with_showing(2, 4, |showing| {
        let read = via_keeps("s1: o3 o1-o3", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o3", "o1", "o2"]), handles(&[])]
        );
    });
}

#[test]
fn a_reversed_range_or_one_past_the_last_own_handle_leaves_its_item_unplaced() {
    with_showing(3, 4, |showing| {
        for line in ["s1: o3-o1", "s1: o2-o5", "s1: o2-5", "s1: o3-2"] {
            let text = format!("{line}\ns2: o1");
            let read = via_keeps(&text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![Err(Why::Unplaced), handles(&["o1"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn a_range_of_items_is_not_an_entry() {
    with_showing(3, 4, |showing| {
        for text in ["s1-s3: o1", "s1-3: o1", "s1 – s3: o1"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![Err(Why::Unplaced), Err(Why::Unplaced), Err(Why::Unplaced)],
                "{text:?}"
            );
        }
    });
}

#[test]
fn handles_are_read_in_any_case() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("S1: O1 O2\ns2: O3", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2"]), handles(&["o3"]), handles(&[])]
        );
    });
}

#[test]
fn an_item_named_above_its_sentences_keeps_the_sentences_listed_under_it() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("s1:\n- o1\n- o2\ns2:\n- o3", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2"]), handles(&["o3"]), handles(&[])]
        );
        let read = via_keeps("s1\no1 o2", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2"]), handles(&[]), handles(&[])]
        );
        let read = via_keeps("s1:\n- o1-o3", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&["o1", "o2", "o3"]), handles(&[]), handles(&[])]
        );
    });
}

#[test]
fn an_item_named_above_a_line_that_says_none_keeps_nothing() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("s1:\nnone\ns2: o1", Ended::Complete, showing);
        assert_eq!(
            kept(&read),
            vec![handles(&[]), handles(&["o1"]), handles(&[])]
        );
    });
}

#[test]
fn an_entry_holding_a_word_or_number_it_cannot_read_is_not_read() {
    with_showing(3, 3, |showing| {
        for text in ["s1: 2 3\ns2: o1", "s1: sentence two\ns2: o1"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![Err(Why::NotRead), handles(&["o1"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn an_entry_of_only_punctuation_or_none_keeps_nothing() {
    with_showing(3, 3, |showing| {
        for text in ["s1: -\ns2: o1", "s1: none\ns2: o1", "s1:\ns2: o1"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![handles(&[]), handles(&["o1"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn an_entry_that_says_none_and_names_what_it_keeps_contradicts_itself() {
    with_showing(3, 3, |showing| {
        let contradicted = Err(Why::Contradicted(
            "s1 both says none and names what it keeps.".to_owned(),
        ));
        for text in ["s1: none o1\ns2: o2", "s1:\nnone\no1\ns2: o2"] {
            let read = via_keeps(text, Ended::Complete, showing);
            assert_eq!(
                kept(&read),
                vec![contradicted.clone(), handles(&["o2"]), handles(&[])],
                "{text:?}"
            );
        }
    });
}

#[test]
fn a_cut_off_answer_with_an_item_still_open_leaves_every_item_cut_off() {
    with_showing(3, 3, |showing| {
        let read = via_keeps("s1:\n- o1\n- o2", Ended::CutOff, showing);
        assert_eq!(
            kept(&read),
            vec![Err(Why::CutOff), Err(Why::CutOff), Err(Why::CutOff)]
        );
    });
}

// ---- any answer ----

fn arbitrary_json() -> impl Strategy<Value = Value> {
    let key = prop_oneof![
        prop::sample::select(vec![
            "pick", "picks", "order", "items", "entries", "marks", "of", "at", "replace", "insert",
            "remove", "move", "verdict", "text", "cites", "with", "after",
        ])
        .prop_map(str::to_owned),
        "[a-z]{1,4}",
    ]
    .boxed();
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|number| json!(number)),
        ".{0,6}".prop_map(Value::String),
        "[a-z][0-9]{1,2}".prop_map(Value::String),
        Just(json!("none")),
        Just(json!("start")),
        Just(json!("yes")),
    ];
    leaf.prop_recursive(4, 40, 5, move |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
            prop::collection::vec((key.clone(), inner), 0..5)
                .prop_map(|pairs| Value::Object(pairs.into_iter().collect())),
        ]
    })
}

fn arbitrary_ended() -> impl Strategy<Value = Ended> {
    prop_oneof![
        Just(Ended::Complete),
        Just(Ended::CutOff),
        Just(Ended::Declined)
    ]
}

/// Reads `text` as every shape: none may panic, and a per-item shape answers every shown item once.
fn read_as_every_shape(text: &str, ended: Ended, showing: &Showing<'_>) {
    let _ = via_value(text, ended, showing);
    let _ = via_pick(text, ended, showing);
    let _ = via_pick_some(text, ended, showing);
    let _ = via_order(text, ended, showing);
    let _ = via_new(text, ended, showing);
    let _ = via_change(text, ended, showing);
    let _ = via_lead(text, ended, showing);
    assert_in_order(showing, &via_each(text, ended, showing));
    assert_in_order(showing, &via_flagged(text, ended, showing));
    assert_in_order(showing, &via_keeps(text, ended, showing));
    let rich = <Each<Sheet, Rich> as PerItem>::read(&written(text, ended), showing);
    assert_in_order(showing, &rich);
}

proptest! {
    #[test]
    fn any_json_is_read_without_panic(
        n in 0usize..=12,
        own in 0usize..=4,
        value in arbitrary_json(),
        ended in arbitrary_ended(),
    ) {
        with_showing(n, own, |showing| {
            let text = value.to_string();
            read_as_every_shape(&text, ended, showing);
            let wrapped = json!({ "entries": [value.clone()], "marks": [value.clone()], "items": [value] })
                .to_string();
            read_as_every_shape(&wrapped, ended, showing);
        });
    }

    #[test]
    fn any_text_is_read_without_panic(
        n in 0usize..=12,
        text in any::<String>(),
        ended in arbitrary_ended(),
    ) {
        with_showing(n, 2, |showing| read_as_every_shape(&text, ended, showing));
    }

    #[test]
    fn a_declined_answer_declines_every_statement(n in 0usize..=12, value in arbitrary_json()) {
        with_showing(n, 0, |showing| {
            let text = value.to_string();
            let declined = Settled::Unanswered(Why::Declined);
            assert_eq!(via_value(&text, Ended::Declined, showing), declined);
            assert_eq!(via_pick(&text, Ended::Declined, showing), Settled::Unanswered(Why::Declined));
            assert_eq!(via_order(&text, Ended::Declined, showing), Settled::Unanswered(Why::Declined));
            assert!(via_each(&text, Ended::Declined, showing)
                .iter()
                .all(|(_, settled)| *settled == Settled::Unanswered(Why::Declined)));
            assert!(via_flagged(&text, Ended::Declined, showing)
                .iter()
                .all(|(_, settled)| *settled == Settled::Unanswered(Why::Declined)));
            assert_eq!(via_new(&text, Ended::Declined, showing).rest, Some(Why::Declined));
        });
    }

    #[test]
    fn prose_and_fences_around_an_answer_change_nothing(n in 0usize..=12, seed in any::<u64>()) {
        let mut rng = Rng(seed);
        with_showing(n, 0, |showing| {
            let mut elements = Vec::new();
            for number in 1..=n {
                if rng.chance(60) {
                    elements.push(note_object(Some(("of", handle_of(number).as_str())), number, &[]));
                }
            }
            let bare = list_doc("entries", &elements).text;
            let fenced = format!("Here is my answer {{as asked}}:\n```json\n{bare}\n```\nAnything else {{?}}");
            assert_eq!(
                format!("{:?}", via_each(&fenced, Ended::Complete, showing)),
                format!("{:?}", via_each(&bare, Ended::Complete, showing))
            );
            let note = note_object(None, 1, &[]);
            let fenced = format!("```\n{note}\n```\n{{ trailing");
            assert_eq!(
                via_value(&fenced, Ended::Complete, showing),
                via_value(&note, Ended::Complete, showing)
            );
        });
    }
}

// ---- a change that never anchors at the start ----

/// A statement of one change whose places always follow a shown sheet.
#[derive(Debug, PartialEq)]
struct After {
    edit: Consistent<Sheet, String>,
}

impl Authored for After {
    fn form() -> Form {
        Form::new().change_after::<Sheet, String>("edit")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(After {
            edit: fields.change_after("edit")?,
        })
    }
}

fn via_after(text: &str, showing: &Showing<'_>) -> Settled<After> {
    <Whole<After> as Single>::read(&written(text, Ended::Complete), showing)
}

#[test]
fn a_change_after_has_no_start_in_either_anchor() {
    with_showing(3, 0, |showing| {
        for pointers in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Whole<After>>(showing, pointers);
            let edit = &schema["properties"]["edit"]["properties"];
            let anchors = [
                &edit["insert"]["items"]["properties"]["after"],
                &edit["move"]["items"]["properties"]["after"],
            ];
            for anchor in anchors {
                match pointers {
                    Held::Enums | Held::Counts => {
                        assert_eq!(anchor["enum"], json!(["s1", "s2", "s3"]))
                    }
                    Held::Plain => assert_eq!(
                        anchor,
                        &json!({"type": "string", "description": PLAIN_NOTE})
                    ),
                }
            }
            assert_dialect(&schema);
        }
    });
}

#[test]
fn a_change_still_offers_start() {
    with_showing(3, 0, |showing| {
        let schema = schema_of::<Change<Sheet, String>>(showing, Held::Enums);
        let edit = &schema["properties"];
        assert_eq!(
            edit["insert"]["items"]["properties"]["after"]["enum"],
            json!(["s1", "s2", "s3", "start"])
        );
        assert_eq!(
            edit["move"]["items"]["properties"]["after"]["enum"],
            json!(["s1", "s2", "s3", "start"])
        );
    });
}

#[test]
fn an_answer_anchoring_a_change_after_at_the_start_is_unanswered() {
    with_showing(3, 0, |showing| {
        for text in [
            r#"{"edit":{"replace":[],"insert":[{"after":"start","with":"x"}],"remove":[],"move":[]}}"#,
            r#"{"edit":{"replace":[],"insert":[],"remove":[],"move":[{"at":"s2","after":"start"}]}}"#,
        ] {
            assert_eq!(
                via_after(text, showing),
                Settled::Unanswered(Why::Unplaced),
                "{text}"
            );
        }
        let placed = r#"{"edit":{"replace":[],"insert":[{"after":"s1","with":"x"}],"remove":[],"move":[{"at":"s2","after":"s3"}]}}"#;
        let Settled::Answered(read) = via_after(placed, showing) else {
            panic!("an answer that follows shown sheets is read");
        };
        assert_eq!(read.edit.change().insert.len(), 1);
        assert_eq!(read.edit.change().moved.len(), 1);
    });
}

#[test]
fn empty_change_and_cite_constrain_to_empty_arrays_under_counts_only() {
    with_showing(0, 0, |showing| {
        let change_counts = schema_of::<Change<Sheet, String>>(showing, Held::Counts);
        let props = &change_counts["properties"];
        assert_eq!(props["replace"]["maxItems"], json!(0));
        assert_eq!(props["remove"]["maxItems"], json!(0));
        assert_eq!(props["move"]["maxItems"], json!(0));
        assert!(props["insert"].get("maxItems").is_none());

        for held in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Change<Sheet, String>>(showing, held);
            assert_dialect(&schema);
        }

        let cite_counts = schema_of::<Whole<Rich>>(showing, Held::Counts);
        assert_eq!(cite_counts["properties"]["cites"]["maxItems"], json!(0));
        for held in [Held::Enums, Held::Plain] {
            let schema = schema_of::<Whole<Rich>>(showing, held);
            assert_dialect(&schema);
        }
    });
}
