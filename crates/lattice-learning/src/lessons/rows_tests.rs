use super::copied_wording::{Wording, SAY_IT_YOURSELF};
use super::rows::{Edits, Held, Left, Quotes, Rows, Took};
use lattice_core::application::services::model_contract::shape::{
    Ended, Settled, Single, Value as Whole, Written,
};
use lattice_core::application::services::model_contract::shown::{Kind, Scope};
use lattice_core::application::services::model_contract::units::{Grain, Units};
use serde_json::{json, Value};

const TITLE: &str = "Reading a tide table";
const FIRST: &str = "A tide table lists the hours of high water.";
const SECOND: &str = "Pilots read it before sailing.";
const THIRD: &str = "The kiln at the quay is fired at dawn.";
const OPTION: &str = "The hours of high water";
const REASON: &str = "Cargo weight is on the manifest.";
const QUOTE_A: &str = "A tide table lists the hours of high water at the harbour.";
const QUOTE_B: &str = "Harbour pilots use it to plan each sailing safely.";
const QUOTE_KILN: &str = "The quay kiln is lit when the first boats leave.";

fn lesson() -> Value {
    json!({
        "blocks": [{
            "kind": "teaching",
            "title": TITLE,
            "body": format!("{FIRST} {SECOND}\n\n{THIRD}"),
            "sourceIndex": 0,
            "quote": QUOTE_A,
            "citations": [
                {"sentence": FIRST, "sourceIndex": 0, "quote": QUOTE_A},
                {"sentence": THIRD, "sourceIndex": 1, "quote": QUOTE_KILN}
            ]
        }],
        "questions": [{
            "kind": "multipleChoice",
            "prompt": "What does a tide table list?",
            "options": [OPTION, "The weight of cargo"],
            "correctIndex": 0,
            "rationales": ["It lists the hours of high water.", REASON],
            "explanation": QUOTE_A,
            "sourceIndex": 0,
            "quote": QUOTE_A,
            "citations": []
        }]
    })
}

fn sources() -> Vec<Value> {
    vec![
        json!({
            "sourceIndex": 0,
            "excerpt": format!("{QUOTE_A} Short one. {QUOTE_B} {QUOTE_A}")
        }),
        json!({"sourceIndex": 1, "excerpt": QUOTE_KILN}),
        json!({"sourceIndex": 2}),
    ]
}

fn quotes() -> Quotes {
    let sources = sources();
    Quotes::of(&sources)
}

/// The handle of a quotable sentence, which `Quotes` shows in the order it holds them.
fn q(quotes: &Quotes, sentence: &str) -> String {
    let at = quotes
        .all()
        .iter()
        .position(|quotable| quotable.text() == sentence)
        .expect("the sentence is quotable");
    format!("q{}", at + 1)
}

fn render(rows: &Rows) -> Value {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = rows.shown(&mut showing).expect("shown");
    rows.rendered(&shown)
}

/// The handle `rendered` shows before `sentence`.
fn h(rendered: &Value, sentence: &str) -> String {
    let text = rendered.to_string();
    let needle = format!("] {sentence}");
    let at = text.find(&needle).expect("the sentence is shown");
    let open = text[..at].rfind('[').expect("a handle opens before it");
    text[open + 1..at].to_owned()
}

fn list(given: &Value, key: &str) -> Value {
    given.get(key).cloned().unwrap_or_else(|| json!([]))
}

/// What a model answers for one entry, read against what `rows` and `quotes` show.
fn entry(rows: &Rows, quotes: &Quotes, given: Value) -> Edits {
    let answer = json!({
        "sentences": {
            "replace": list(&given, "replace"),
            "insert": list(&given, "insert"),
            "remove": list(&given, "remove"),
            "move": list(&given, "move"),
        },
        "cite": list(&given, "cite"),
        "recite": list(&given, "recite"),
        "uncite": list(&given, "uncite"),
        "keys": list(&given, "keys"),
    })
    .to_string();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let _shown = rows.shown(&mut showing).expect("shown");
    let _quoted = showing.show(quotes.all()).expect("shown");
    let written = Written {
        text: &answer,
        ended: Ended::Complete,
    };
    match <Whole<Edits> as Single>::read(&written, &showing) {
        Settled::Answered(edits) => edits,
        other => panic!("the entry is not read: {other:?}"),
    }
}

fn take_with(
    rows: &mut Rows,
    quotes: &Quotes,
    wording: &Wording,
    given: Value,
) -> Result<Took, Left> {
    let edits = entry(rows, quotes, given);
    rows.take(&edits, quotes, &Held { wording })
}

fn take(rows: &mut Rows, quotes: &Quotes, given: Value) -> Result<Took, Left> {
    take_with(rows, quotes, &Wording::default(), given)
}

fn refused(taken: Result<Took, Left>) -> Vec<String> {
    match taken {
        Err(Left::Refused(problems)) => problems,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn body(value: &Value) -> String {
    value["blocks"][0]["body"]
        .as_str()
        .expect("a body")
        .to_owned()
}

fn citations(value: &Value) -> Vec<Value> {
    value["blocks"][0]["citations"]
        .as_array()
        .expect("citations")
        .clone()
}

fn replacing(handle: &str, text: &str) -> Value {
    json!({"replace": [{"at": handle, "with": {"text": text, "from": []}}]})
}

// ---- the material as rows ----

#[test]
fn rows_hold_the_material_exactly_when_nothing_is_taken() {
    assert_eq!(Rows::of(&lesson()).value(), lesson());
    let odd = json!({
        "blocks": [
            {
                "kind": "teaching",
                "title": "  Odd   spacing  ",
                "body": "  First one here.   Second one!\n\n\n   Third?  \r\n\r\nFourth.\t\n",
                "sourceIndex": null,
                "quote": "",
                "citations": [{"sentence": "  First one here. ", "sourceIndex": 0, "quote": " q "}]
            },
            {"body": "", "citations": null},
            {"body": "No stop at all", "sourceIndex": 3, "quote": "   "}
        ],
        "questions": [{
            "prompt": "p",
            "options": ["", "", " a b "],
            "correctIndex": 1,
            "rationales": [],
            "explanation": "E. F.\n",
            "quote": "x"
        }],
        "extra": {"note": "Free text. More text."}
    });
    assert_eq!(Rows::of(&odd).value(), odd);
    assert_eq!(Rows::of(&json!({})).value(), json!({}));
}

#[test]
fn two_fields_with_equal_text_get_different_handles() {
    let material = json!({"blocks": [{"title": "Same words here.", "body": "Same words here."}]});
    let rendered = render(&Rows::of(&material));
    assert_eq!(
        rendered["blocks"][0]["title"],
        json!("[u1] Same words here.")
    );
    assert_eq!(
        rendered["blocks"][0]["body"],
        json!("[u2] Same words here.")
    );
}

#[test]
fn a_small_lesson_is_rendered_with_handles_and_citations_by_handle() {
    let material = json!({"blocks": [{
        "kind": "teaching",
        "body": "Tides rise twice a day. Pilots read the table.",
        "sourceIndex": 0,
        "quote": "Tides rise twice a day in this harbour.",
        "citations": [{
            "sentence": "Tides rise twice a day.",
            "sourceIndex": 0,
            "quote": "Tides rise twice a day in this harbour."
        }]
    }]});
    let expected = json!({"blocks": [{
        "part": "/blocks/0",
        "kind": "teaching",
        "body": "[u1] Tides rise twice a day. [u2] Pilots read the table.",
        "citation": {"at": "c1", "sourceIndex": 0, "quote": "Tides rise twice a day in this harbour."},
        "citations": [{
            "at": "c2",
            "sentences": ["u1"],
            "sourceIndex": 0,
            "quote": "Tides rise twice a day in this harbour."
        }]
    }]});
    assert_eq!(
        render(&Rows::of(&material)).to_string(),
        expected.to_string()
    );
}

#[test]
fn a_part_without_a_citation_of_its_own_shows_null() {
    let material =
        json!({"blocks": [{"body": "Tides rise twice a day.", "sourceIndex": null, "quote": ""}]});
    let rendered = render(&Rows::of(&material));
    assert_eq!(rendered["blocks"][0]["citation"], Value::Null);
    assert_eq!(rendered["blocks"][0]["part"], json!("/blocks/0"));
}

#[test]
fn the_citations_are_shown_in_order_after_the_units() {
    let rows = Rows::of(&lesson());
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = rows.shown(&mut showing).expect("shown");
    let seen: Vec<(String, Option<u64>, String)> = shown
        .citations()
        .map(|(handle, citation)| {
            (
                handle.written(),
                citation.source_index(),
                citation.quote().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            ("c1".to_owned(), Some(0), QUOTE_A.to_owned()),
            ("c2".to_owned(), Some(0), QUOTE_A.to_owned()),
            ("c3".to_owned(), Some(1), QUOTE_KILN.to_owned()),
            ("c4".to_owned(), Some(0), QUOTE_A.to_owned()),
        ]
    );
    let at: Vec<Option<usize>> = shown
        .citations()
        .map(|(_, citation)| citation.at().index)
        .collect();
    assert_eq!(at, vec![None, Some(0), Some(1), None]);
}

// ---- quotes ----

#[test]
fn quotes_are_the_long_enough_sentences_of_each_excerpt_each_once() {
    let quotes = quotes();
    let seen: Vec<(u64, &str)> = quotes
        .all()
        .iter()
        .filter_map(|quotable| quotes.get(quotable.reference()))
        .collect();
    assert_eq!(
        seen,
        vec![(0, QUOTE_A), (0, QUOTE_B), (0, QUOTE_A), (1, QUOTE_KILN)]
    );
}

#[test]
fn a_sentence_under_the_quote_floor_has_no_handle_and_no_cite_can_name_it() {
    let quotes = quotes();
    let excerpt = format!("{QUOTE_A} Short one. {QUOTE_B} {QUOTE_A}");
    let cut = Units::of_at(&excerpt, Grain::Sentence, "0");
    let short = cut
        .units()
        .iter()
        .find(|unit| unit.text() == "Short one.")
        .expect("the short sentence is a unit");
    assert_eq!(quotes.get(short.reference()), None);
    assert!(quotes
        .all()
        .iter()
        .all(|quotable| quotable.text() != "Short one."));
}

#[test]
fn two_excerpts_of_one_source_are_two_texts_and_an_excerpt_added_twice_is_one() {
    let other = format!("{QUOTE_B} {QUOTE_A}");
    let mut quotes = Quotes::default();
    quotes.push(0, QUOTE_A);
    quotes.push(0, &other);
    quotes.push(0, QUOTE_A);
    quotes.push(0, &other);
    let seen: Vec<(u64, &str)> = quotes
        .all()
        .iter()
        .filter_map(|quotable| quotes.get(quotable.reference()))
        .collect();
    assert_eq!(seen, vec![(0, QUOTE_A), (0, QUOTE_B), (0, QUOTE_A)]);
    let refs: Vec<_> = quotes
        .all()
        .iter()
        .map(|quotable| quotable.reference())
        .collect();
    assert_ne!(refs.first(), refs.get(2));
}

#[test]
fn an_excerpt_is_marked_with_the_handle_of_each_quotable_sentence() {
    let quotes = quotes();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(quotes.all()).expect("shown");
    let excerpt = format!("{QUOTE_A} Short one. {QUOTE_B} {QUOTE_A}");
    assert_eq!(
        quotes.marked(0, &excerpt, &shown),
        format!("[q1] {QUOTE_A} Short one. [q2] {QUOTE_B} [q3] {QUOTE_A}")
    );
    assert_eq!(
        quotes.marked(1, QUOTE_KILN, &shown),
        format!("[q4] {QUOTE_KILN}")
    );
    assert_eq!(quotes.marked(5, QUOTE_KILN, &shown), QUOTE_KILN);
}

#[test]
fn marking_leaves_the_excerpt_as_it_is_apart_from_the_handles() {
    let quotes = quotes();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(quotes.all()).expect("shown");
    let excerpt = format!(
        "  {QUOTE_A}

 Short one.  {QUOTE_B}	{QUOTE_A}
"
    );
    let marked = quotes.marked(0, &excerpt, &shown);
    let mut bare = marked;
    for handle in ["[q1] ", "[q2] ", "[q3] "] {
        bare = bare.replacen(handle, "", 1);
    }
    assert_eq!(bare, excerpt);
}

// ---- citations follow their sentences ----

#[test]
fn a_replaced_sentence_keeps_its_citation_with_the_new_sentence_and_the_same_quote() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let first = h(&render(&rows), FIRST);
    let new = "A tide table gives the hours of high water.";
    take(&mut rows, &quotes, replacing(&first, new)).expect("taken");
    let value = rows.value();
    assert_eq!(
        citations(&value)[0],
        json!({"sentence": new, "sourceIndex": 0, "quote": QUOTE_A})
    );
    assert_eq!(citations(&value)[1], lesson()["blocks"][0]["citations"][1]);
    assert!(body(&value).starts_with(new));
}

#[test]
fn a_removed_sentence_loses_its_citation() {
    let mut rows = Rows::of(&lesson());
    let first = h(&render(&rows), FIRST);
    take(&mut rows, &quotes(), json!({"remove": [first]})).expect("taken");
    let value = rows.value();
    assert_eq!(
        citations(&value),
        vec![lesson()["blocks"][0]["citations"][1].clone()]
    );
    assert!(!body(&value).contains(FIRST));
}

#[test]
fn a_citation_over_two_sentences_holds_both_as_they_now_stand() {
    let mut material = lesson();
    material["blocks"][0]["citations"][0]["sentence"] = json!(format!("{FIRST} {SECOND}"));
    let mut rows = Rows::of(&material);
    let rendered = render(&rows);
    assert_eq!(
        rendered["blocks"][0]["citations"][0]["sentences"],
        json!([h(&rendered, FIRST), h(&rendered, SECOND)])
    );
    let new = "A tide table gives the hours of high water.";
    take(&mut rows, &quotes(), replacing(&h(&rendered, FIRST), new)).expect("taken");
    assert_eq!(
        citations(&rows.value())[0]["sentence"],
        json!(format!("{new} {SECOND}"))
    );
}

#[test]
fn an_untouched_citation_keeps_its_sentence_byte_for_byte() {
    let mut material = lesson();
    material["blocks"][0]["citations"][0]["sentence"] =
        json!("A  tide table lists the\thours of high water.");
    let mut rows = Rows::of(&material);
    let third = h(&render(&rows), THIRD);
    take(
        &mut rows,
        &quotes(),
        replacing(&third, "The kiln by the quay is lit at dawn each day."),
    )
    .expect("taken");
    let value = rows.value();
    assert_eq!(
        citations(&value)[0]["sentence"],
        json!("A  tide table lists the\thours of high water.")
    );
    assert_eq!(
        citations(&value)[1]["sentence"],
        json!("The kiln by the quay is lit at dawn each day.")
    );
}

#[test]
fn a_citation_follows_its_sentence_when_the_sentence_is_moved() {
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let (first, third) = (h(&rendered, FIRST), h(&rendered, THIRD));
    take(
        &mut rows,
        &quotes(),
        json!({"move": [{"at": first, "after": third}]}),
    )
    .expect("taken");
    let value = rows.value();
    assert!(body(&value).ends_with(FIRST));
    assert_eq!(citations(&value)[0]["sentence"], json!(FIRST));
    assert_eq!(citations(&value).len(), 2);
}

// ---- what an entry cites ----

#[test]
fn text_with_a_source_adds_a_citation_for_that_text() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let second = h(&render(&rows), SECOND);
    let new = "Boats leave when the tide turns.";
    let given = json!({"insert": [{
        "after": second,
        "with": {"text": new, "from": [q(&quotes, QUOTE_KILN)]}
    }]});
    let took = take(&mut rows, &quotes, given).expect("taken");
    assert_eq!(took.written, vec![new.to_owned()]);
    let value = rows.value();
    assert!(body(&value).contains(&format!("{SECOND} {new}")));
    assert_eq!(
        citations(&value)[2],
        json!({"sentence": new, "sourceIndex": 1, "quote": QUOTE_KILN})
    );
    assert_eq!(citations(&value).len(), 3);
}

#[test]
fn a_cite_of_an_existing_sentence_adds_one_and_a_duplicate_is_not_listed_twice() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let rendered = render(&rows);
    let (first, second) = (h(&rendered, FIRST), h(&rendered, SECOND));
    let given = json!({"cite": [
        {"sentence": second, "quote": q(&quotes, QUOTE_B)},
        {"sentence": first, "quote": q(&quotes, QUOTE_A)}
    ]});
    take(&mut rows, &quotes, given).expect("taken");
    let value = rows.value();
    assert_eq!(citations(&value).len(), 3);
    assert_eq!(
        citations(&value)[2],
        json!({"sentence": SECOND, "sourceIndex": 0, "quote": QUOTE_B})
    );
}

#[test]
fn a_cite_follows_its_sentence_when_the_same_entry_replaces_it() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let second = h(&render(&rows), SECOND);
    let new = "Pilots check the table well before every sailing.";
    let given = json!({
        "replace": [{"at": second, "with": {"text": new, "from": []}}],
        "cite": [{"sentence": second, "quote": q(&quotes, QUOTE_B)}]
    });
    take(&mut rows, &quotes, given).expect("taken");
    assert_eq!(
        citations(&rows.value())[2],
        json!({"sentence": new, "sourceIndex": 0, "quote": QUOTE_B})
    );
}

#[test]
fn a_listed_citation_is_given_another_quote_or_taken_back() {
    let quotes = quotes();
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let c2 = rendered["blocks"][0]["citations"][0]["at"]
        .as_str()
        .expect("a handle")
        .to_owned();
    take(
        &mut rows,
        &quotes,
        json!({"recite": [{"citation": c2, "quote": q(&quotes, QUOTE_B)}]}),
    )
    .expect("taken");
    assert_eq!(
        citations(&rows.value())[0],
        json!({"sentence": FIRST, "sourceIndex": 0, "quote": QUOTE_B})
    );

    let mut rows = Rows::of(&lesson());
    take(&mut rows, &quotes, json!({"uncite": [c2]})).expect("taken");
    assert_eq!(
        citations(&rows.value()),
        vec![lesson()["blocks"][0]["citations"][1].clone()]
    );
}

#[test]
fn a_part_s_own_citation_is_given_another_quote_or_taken_back() {
    let quotes = quotes();
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let own = rendered["blocks"][0]["citation"]["at"]
        .as_str()
        .expect("a handle")
        .to_owned();
    take(
        &mut rows,
        &quotes,
        json!({"recite": [{"citation": own, "quote": q(&quotes, QUOTE_KILN)}]}),
    )
    .expect("taken");
    let value = rows.value();
    assert_eq!(value["blocks"][0]["sourceIndex"], json!(1));
    assert_eq!(value["blocks"][0]["quote"], json!(QUOTE_KILN));
    assert_eq!(value["questions"][0]["quote"], json!(QUOTE_A));

    let mut rows = Rows::of(&lesson());
    take(&mut rows, &quotes, json!({"uncite": [own]})).expect("taken");
    let value = rows.value();
    assert_eq!(value["blocks"][0]["sourceIndex"], Value::Null);
    assert_eq!(value["blocks"][0]["quote"], json!(""));
    assert_eq!(citations(&value).len(), 2);
}

// ---- what an entry may not do ----

#[test]
fn new_text_that_is_blank_is_refused_and_the_text_applied_is_trimmed() {
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let second = h(&rendered, SECOND);
    assert_eq!(
        refused(take(&mut rows, &quotes(), replacing(&second, " \t "))),
        vec![format!(
            "The new text for {second} is empty; list a sentence in remove to take it out."
        )]
    );
    assert_eq!(
        refused(take(
            &mut rows,
            &quotes(),
            json!({"insert": [{"after": second, "with": {"text": "", "from": []}}]})
        )),
        vec![format!("The new text after {second} is empty.")]
    );
    assert_eq!(rows.value(), lesson());

    take(
        &mut rows,
        &quotes(),
        replacing(
            &second,
            "  Pilots check the table well before every sailing. \t",
        ),
    )
    .expect("taken");
    assert!(
        body(&rows.value()).contains(&format!(
            "{FIRST} Pilots check the table well before every sailing. \n\n{THIRD}"
        )) || body(&rows.value()).contains("sailing.\n\n")
    );
    assert!(!body(&rows.value()).contains("  Pilots"));
}

#[test]
fn a_move_between_two_texts_is_refused() {
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let (first, option) = (h(&rendered, FIRST), h(&rendered, OPTION));
    assert_eq!(
        refused(take(
            &mut rows,
            &quotes(),
            json!({"move": [{"at": first, "after": option}]})
        )),
        vec![format!(
            "{first} cannot move after {option}: they are in different texts."
        )]
    );
    assert_eq!(rows.value(), lesson());
}

#[test]
fn only_a_taught_text_takes_a_citation() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let rendered = render(&rows);
    let (title, option) = (h(&rendered, TITLE), h(&rendered, OPTION));
    let told = |handle: &str| {
        format!("{handle} is not in a block's body or a question's explanation or reasons; only those sentences take a citation.")
    };
    assert_eq!(
        refused(take(
            &mut rows,
            &quotes,
            json!({"cite": [{"sentence": title, "quote": q(&quotes, QUOTE_A)}]})
        )),
        vec![told(&title)]
    );
    let given = json!({"replace": [{
        "at": option,
        "with": {"text": "The times of high water", "from": [q(&quotes, QUOTE_A)]}
    }]});
    assert_eq!(
        refused(take(&mut rows, &quotes, given)),
        vec![told(&option)]
    );
    assert_eq!(rows.value(), lesson());
}

#[test]
fn a_removed_sentence_cannot_be_cited_in_the_same_entry() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let second = h(&render(&rows), SECOND);
    let given = json!({
        "remove": [second],
        "cite": [{"sentence": second, "quote": q(&quotes, QUOTE_B)}]
    });
    assert_eq!(
        refused(take(&mut rows, &quotes, given)),
        vec![format!("{second} is removed and cannot be cited.")]
    );
}

#[test]
fn a_citation_named_twice_is_refused() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let rendered = render(&rows);
    let c2 = rendered["blocks"][0]["citations"][0]["at"]
        .as_str()
        .expect("a handle")
        .to_owned();
    let given = json!({
        "recite": [{"citation": c2, "quote": q(&quotes, QUOTE_B)}],
        "uncite": [c2]
    });
    assert_eq!(
        refused(take(&mut rows, &quotes, given)),
        vec![format!("{c2} is changed more than once.")]
    );
    assert_eq!(rows.value(), lesson());
}

#[test]
fn a_quote_the_excerpts_do_not_hold_is_refused() {
    let mut rows = Rows::of(&lesson());
    let all = quotes();
    let second = h(&render(&rows), SECOND);
    let kiln = q(&all, QUOTE_KILN);
    let given = json!({"cite": [{"sentence": second, "quote": kiln}]});
    let edits = entry(&rows, &all, given);
    let only_the_first = Quotes::of(&sources()[..1]);
    let taken = rows.take(
        &edits,
        &only_the_first,
        &Held {
            wording: &Wording::default(),
        },
    );
    assert_eq!(
        refused(taken),
        vec![format!("{kiln} is not a sentence of the excerpts shown.")]
    );
    assert_eq!(rows.value(), lesson());
}

#[test]
fn a_sentence_the_lesson_already_has_is_refused_in_a_body() {
    let mut rows = Rows::of(&lesson());
    let second = h(&render(&rows), SECOND);
    assert_eq!(
        refused(take(&mut rows, &quotes(), replacing(&second, THIRD))),
        vec![format!(
            "The new text at {second} adds a sentence the lesson already has: \"{THIRD}\". Reword it or leave it out."
        )]
    );
    assert_eq!(rows.value(), lesson());
}

#[test]
fn a_sentence_that_copies_the_source_is_refused_in_a_taught_text() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let wording = Wording::of_excerpts([format!("{QUOTE_A} Short one. {QUOTE_B}").as_str()]);
    let second = h(&render(&rows), SECOND);
    let problems = refused(take_with(
        &mut rows,
        &quotes,
        &wording,
        replacing(&second, QUOTE_B),
    ));
    let [problem] = problems.as_slice() else {
        panic!("{problems:?}");
    };
    assert!(
        problem.starts_with(&format!("The new text at {second} writes the source's wording as the lesson's own sentence: \"{QUOTE_B}\" (the source's words are \"Harbour pilots")),
        "{problem}"
    );
    assert!(problem.ends_with(SAY_IT_YOURSELF), "{problem}");
    assert_eq!(rows.value(), lesson());

    let own_words = "Pilots plan every sailing with the help of the table.";
    take_with(&mut rows, &quotes, &wording, replacing(&second, own_words)).expect("taken");
}

#[test]
fn the_quotes_an_entry_brings_count_as_source_wording_for_its_part() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let second = h(&render(&rows), SECOND);
    let copying = |from: Vec<String>| json!({"replace": [{"at": second, "with": {"text": QUOTE_B, "from": from}}]});
    let problems = refused(take(&mut rows, &quotes, copying(vec![q(&quotes, QUOTE_B)])));
    assert!(
        problems[0].contains("writes the source's wording as the lesson's own sentence"),
        "{problems:?}"
    );
    take(&mut rows, &quotes, copying(Vec::new())).expect("without the quote it stands");
}

#[test]
fn an_entry_with_a_problem_in_its_second_text_changes_nothing() {
    let mut rows = Rows::of(&lesson());
    let rendered = render(&rows);
    let (second, reason) = (h(&rendered, SECOND), h(&rendered, REASON));
    let given = json!({"replace": [
        {"at": second, "with": {"text": "Pilots check the table well before every sailing.", "from": []}},
        {"at": reason, "with": {"text": "   ", "from": []}}
    ]});
    let problems = refused(take(&mut rows, &quotes(), given));
    assert_eq!(
        problems,
        vec![format!(
            "The new text for {reason} is empty; list a sentence in remove to take it out."
        )]
    );
    assert_eq!(rows.value(), lesson());
}

// ---- entries that meet ----

#[test]
fn an_entry_that_changes_what_an_earlier_one_changed_is_left_and_the_earlier_one_stands() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let second = h(&render(&rows), SECOND);
    let new = "Pilots check the table well before every sailing.";
    take(&mut rows, &quotes, replacing(&second, new)).expect("taken");
    let told = format!("{second} was already changed by an earlier edit.");
    assert_eq!(
        take(&mut rows, &quotes, json!({"remove": [second]})),
        Err(Left::Met(told.clone()))
    );
    assert_eq!(
        take(
            &mut rows,
            &quotes,
            json!({"insert": [{"after": second, "with": {"text": "Boats leave when the tide turns.", "from": []}}]})
        ),
        Err(Left::Met(told))
    );
    assert!(body(&rows.value()).contains(new));
    assert!(!body(&rows.value()).contains("Boats leave"));
}

#[test]
fn an_entry_that_changes_a_citation_an_earlier_one_changed_is_left() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let rendered = render(&rows);
    let c2 = rendered["blocks"][0]["citations"][0]["at"]
        .as_str()
        .expect("a handle")
        .to_owned();
    take(
        &mut rows,
        &quotes,
        json!({"recite": [{"citation": c2, "quote": q(&quotes, QUOTE_B)}]}),
    )
    .expect("taken");
    assert_eq!(
        take(&mut rows, &quotes, json!({"uncite": [c2]})),
        Err(Left::Met(format!(
            "{c2} was already changed by an earlier edit."
        )))
    );
    assert_eq!(citations(&rows.value())[0]["quote"], json!(QUOTE_B));
}

#[test]
fn an_entry_that_is_refused_is_never_met() {
    let mut rows = Rows::of(&lesson());
    let second = h(&render(&rows), SECOND);
    take(
        &mut rows,
        &quotes(),
        replacing(&second, "Pilots check the table well before every sailing."),
    )
    .expect("taken");
    let taken = take(&mut rows, &quotes(), replacing(&second, " "));
    assert!(matches!(taken, Err(Left::Refused(_))), "{taken:?}");
}

// ---- what an entry reports ----

#[test]
fn an_entry_reports_the_parts_it_changed_and_the_sentences_it_wrote() {
    let mut rows = Rows::of(&lesson());
    let quotes = quotes();
    let rendered = render(&rows);
    let second = h(&rendered, SECOND);
    let c4 = rendered["blocks"][0]["citation"]["at"]
        .as_str()
        .expect("a handle")
        .to_owned();
    let own_of_question = {
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        let shown = rows.shown(&mut showing).expect("shown");
        let own = shown
            .citations()
            .find(|(_, citation)| citation.at().part == "/questions/0")
            .map(|(handle, _)| handle.written())
            .expect("the question has its own citation");
        own
    };
    assert_ne!(own_of_question, c4);
    let new = "Pilots check the table well before every sailing.";
    let given = json!({
        "replace": [{"at": second, "with": {"text": new, "from": []}}],
        "recite": [{"citation": own_of_question, "quote": q(&quotes, QUOTE_KILN)}]
    });
    let took = take(&mut rows, &quotes, given).expect("taken");
    assert_eq!(
        took.parts.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["/blocks/0", "/questions/0"]
    );
    assert_eq!(took.written, vec![new.to_owned()]);
}

#[test]
fn a_text_outside_every_part_is_the_part_with_no_name() {
    let material = json!({
        "summary": "Tides rise twice a day. Boats leave with the tide.",
        "blocks": []
    });
    let mut rows = Rows::of(&material);
    let boats = h(&render(&rows), "Boats leave with the tide.");
    let took = take(
        &mut rows,
        &Quotes::default(),
        replacing(&boats, "Boats sail when the water is high."),
    )
    .expect("taken");
    assert_eq!(took.parts.len(), 1);
    assert!(took.parts.contains(""));
    assert_eq!(
        rows.value()["summary"],
        json!("Tides rise twice a day. Boats sail when the water is high.")
    );
}

#[test]
fn an_entry_that_changes_nothing_is_empty_and_taken() {
    let mut rows = Rows::of(&lesson());
    let edits = entry(&rows, &quotes(), json!({}));
    assert!(edits.is_empty());
    let took = rows
        .take(
            &edits,
            &quotes(),
            &Held {
                wording: &Wording::default(),
            },
        )
        .expect("taken");
    assert!(took.parts.is_empty() && took.written.is_empty());
    assert_eq!(rows.value(), lesson());
}

// ---- new paragraphs ----

#[test]
fn new_text_that_begins_with_a_blank_line_starts_a_paragraph() {
    let mut rows = Rows::of(&lesson());
    let second = h(&render(&rows), SECOND);
    let new = "A new paragraph begins right here.";
    let given = json!({"insert": [{
        "after": second,
        "with": {"text": format!("\n\n{new}"), "from": []}
    }]});
    take(&mut rows, &quotes(), given).expect("taken");
    assert_eq!(
        body(&rows.value()),
        format!("{FIRST} {SECOND}\n\n{new}\n\n{THIRD}")
    );
}

// ---- the answer key ----

fn quiz() -> Value {
    json!({"blocks": [], "questions": [{
        "kind": "multipleChoice",
        "prompt": "Which tide comes first?",
        "options": ["Neap.", "Spring.", "Slack."],
        "correctIndex": 0,
        "rationales": ["Neap is not first.", "Spring is not first.", "Slack is not first."],
        "explanation": "The kiln fires at dawn.",
        "sourceIndex": null,
        "quote": "",
        "citations": []
    }]})
}

fn without_reasons() -> Value {
    let mut material = quiz();
    if let Some(question) = material["questions"][0].as_object_mut() {
        question.remove("rationales");
    }
    material
}

fn keyed(right: &str, was_wrong: &str) -> Value {
    json!({"keys": [{"right": right, "wasWrong": was_wrong}]})
}

#[test]
fn a_key_is_moved_and_the_reasons_follow() {
    let mut rows = Rows::of(&quiz());
    let spring = h(&render(&rows), "Spring.");
    let took = take(
        &mut rows,
        &Quotes::default(),
        keyed(&spring, "  Neap tides are not the first.  "),
    )
    .expect("taken");
    let value = rows.value();
    let question = &value["questions"][0];
    assert_eq!(question["correctIndex"], json!(1));
    assert_eq!(
        question["rationales"],
        json!(["Neap tides are not the first.", "", "Slack is not first."])
    );
    assert_eq!(question["options"], quiz()["questions"][0]["options"]);
    assert_eq!(took.parts.iter().collect::<Vec<_>>(), vec!["/questions/0"]);
    assert_eq!(
        took.written,
        vec!["Neap tides are not the first.".to_owned()]
    );
}

#[test]
fn a_key_moves_again_in_a_later_entry_from_where_it_now_stands() {
    let mut rows = Rows::of(&quiz());
    let rendered = render(&rows);
    let (spring, slack) = (h(&rendered, "Spring."), h(&rendered, "Slack."));
    take(
        &mut rows,
        &Quotes::default(),
        keyed(&spring, "Neap is wrong."),
    )
    .expect("taken");
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), keyed(&spring, "x"))),
        vec![format!(
            "The answer key already names the option {spring} is in."
        )]
    );
    take(
        &mut rows,
        &Quotes::default(),
        keyed(&slack, "Spring is wrong."),
    )
    .expect("taken");
    assert_eq!(rows.value()["questions"][0]["correctIndex"], json!(2));
}

#[test]
fn a_sentence_outside_an_option_of_a_keyed_question_is_refused() {
    let none = |label: &str| {
        format!("{label} is not a sentence of an option of a question that has an answer key.")
    };
    let mut rows = Rows::of(&quiz());
    let kiln = h(&render(&rows), "The kiln fires at dawn.");
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), keyed(&kiln, "x"))),
        vec![none(&kiln)]
    );
    assert_eq!(rows.value(), quiz());

    let mut unkeyed = quiz();
    unkeyed["questions"][0]["correctIndex"] = Value::Null;
    let mut rows = Rows::of(&unkeyed);
    let spring = h(&render(&rows), "Spring.");
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), keyed(&spring, "x"))),
        vec![none(&spring)]
    );
    assert_eq!(rows.value(), unkeyed);

    let mut rows = Rows::of(&lesson());
    let first = h(&render(&rows), FIRST);
    assert_eq!(
        refused(take(&mut rows, &quotes(), keyed(&first, "x"))),
        vec![none(&first)]
    );
}

#[test]
fn the_option_the_key_names_already_is_refused() {
    let mut rows = Rows::of(&quiz());
    let neap = h(&render(&rows), "Neap.");
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), keyed(&neap, "x"))),
        vec![format!(
            "The answer key already names the option {neap} is in."
        )]
    );
    assert_eq!(rows.value(), quiz());
}

#[test]
fn a_questions_key_is_moved_once_in_an_entry() {
    let mut rows = Rows::of(&quiz());
    let rendered = render(&rows);
    let (spring, slack) = (h(&rendered, "Spring."), h(&rendered, "Slack."));
    let given = json!({"keys": [
        {"right": spring, "wasWrong": "Neap is wrong."},
        {"right": slack, "wasWrong": "Neap is wrong."}
    ]});
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), given)),
        vec!["An entry moves a question's answer key once.".to_owned()]
    );
    assert_eq!(rows.value(), quiz());
}

#[test]
fn a_blank_reason_for_the_old_key_is_refused_when_the_options_have_reasons() {
    let mut rows = Rows::of(&quiz());
    let spring = h(&render(&rows), "Spring.");
    assert_eq!(
        refused(take(&mut rows, &Quotes::default(), keyed(&spring, " \t "))),
        vec!["Say in wasWrong why the option the answer key named until now is wrong.".to_owned()]
    );
    assert_eq!(rows.value(), quiz());
}

#[test]
fn a_question_without_reasons_moves_its_key_and_ignores_what_was_wrong() {
    let mut rows = Rows::of(&without_reasons());
    let spring = h(&render(&rows), "Spring.");
    let took = take(&mut rows, &Quotes::default(), keyed(&spring, "")).expect("taken");
    let mut expected = without_reasons();
    expected["questions"][0]["correctIndex"] = json!(1);
    assert_eq!(rows.value(), expected);
    assert!(took.written.is_empty());
    assert_eq!(took.parts.len(), 1);
}

#[test]
fn the_parts_of_a_key_are_the_questions() {
    let rows = Rows::of(&quiz());
    let spring = h(&render(&rows), "Spring.");
    let edits = entry(&rows, &Quotes::default(), keyed(&spring, "Neap is wrong."));
    assert!(!edits.is_empty());
    assert_eq!(
        rows.parts_of(&edits).into_iter().collect::<Vec<_>>(),
        vec!["/questions/0".to_owned()]
    );
}

#[test]
fn an_entry_that_changes_a_reason_an_earlier_key_rewrote_is_left() {
    let mut rows = Rows::of(&quiz());
    let rendered = render(&rows);
    let (spring, neap_reason) = (h(&rendered, "Spring."), h(&rendered, "Neap is not first."));
    take(
        &mut rows,
        &Quotes::default(),
        keyed(&spring, "Neap tides are not the first."),
    )
    .expect("taken");
    let taken = take(
        &mut rows,
        &Quotes::default(),
        replacing(&neap_reason, "Neap comes later in the month."),
    );
    assert!(matches!(taken, Err(Left::Met(_))), "{taken:?}");
    assert_eq!(
        rows.value()["questions"][0]["rationales"][0],
        json!("Neap tides are not the first.")
    );
}

fn two_blocks() -> Value {
    json!({
        "blocks": [
            {"kind": "teaching", "title": "First part", "body": "Tides rise twice a day. Pilots read the table."},
            {"kind": "teaching", "title": "Second part", "body": "Kilns are fired at dawn. Boats leave at noon.",
             "sourceIndex": 0, "quote": QUOTE_A,
             "citations": [{"sentence": "Kilns are fired at dawn.", "sourceIndex": 1, "quote": QUOTE_KILN}]}
        ],
        "questions": []
    })
}

#[test]
fn a_part_s_fields_have_their_real_pointers() {
    let material = two_blocks();
    let rows = Rows::of_part(&material, "/blocks/1");
    let rendered = render(&rows);
    assert_eq!(
        rendered["blocks"][1]["body"],
        json!("[u2] Kilns are fired at dawn. [u3] Boats leave at noon.")
    );
    assert_eq!(rendered["blocks"][1]["title"], json!("[u1] Second part"));
    assert_eq!(rows.value(), material);
    let units = Rows::of(&material);
    let whole = render(&units);
    assert_eq!(
        whole["blocks"][1]["body"],
        json!("[u5] Kilns are fired at dawn. [u6] Boats leave at noon.")
    );
}

#[test]
fn a_change_in_a_part_gives_the_whole_lesson_with_only_that_part_changed() {
    let material = two_blocks();
    let mut rows = Rows::of_part(&material, "/blocks/1");
    let rendered = render(&rows);
    let dawn = h(&rendered, "Kilns are fired at dawn.");
    take(
        &mut rows,
        &Quotes::default(),
        replacing(&dawn, "Kilns are lit at first light."),
    )
    .expect("taken");
    let mut expected = material.clone();
    expected["blocks"][1]["body"] = json!("Kilns are lit at first light. Boats leave at noon.");
    assert_eq!(rows.value()["blocks"][0], material["blocks"][0]);
    assert_eq!(
        rows.value()["blocks"][1]["body"],
        expected["blocks"][1]["body"]
    );
    assert_eq!(rows.value()["questions"], material["questions"]);
}

#[test]
fn a_label_of_another_part_is_not_shown() {
    let material = two_blocks();
    let rendered = render(&Rows::of_part(&material, "/blocks/1"));
    assert_eq!(rendered["blocks"][0], material["blocks"][0]);
    assert!(!rendered.to_string().contains("[u1] Tides"));
    assert_eq!(rendered["blocks"][1]["part"], json!("/blocks/1"));
    assert_eq!(rendered["blocks"][1]["citation"]["at"], json!("c1"));
}

#[test]
fn a_pointer_that_names_nothing_gives_rows_with_no_field() {
    let material = two_blocks();
    let rows = Rows::of_part(&material, "/blocks/9");
    assert_eq!(render(&rows), material);
    assert_eq!(rows.value(), material);
}

#[test]
fn the_edits_words_are_the_ones_the_teaching_repair_sends() {
    use super::rows::words::{citing, KEYS, NO_CITATIONS, SENTENCES};
    assert!(SENTENCES.starts_with("candidate shows the material with a label"));
    assert!(SENTENCES.contains("each of its options, each option's reason and its explanation"));
    assert!(SENTENCES.contains("A short text such as an option is one labelled sentence"));
    assert!(KEYS.starts_with("A question's answer key"));
    assert!(NO_CITATIONS.starts_with("This material takes no citations"));
    assert!(citing().starts_with("A sentence that states a figure"));
}
