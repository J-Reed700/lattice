use super::*;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::Plain;
use lattice_core::application::services::model_contract::shape::Found;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;
use lattice_core::domain::learning::drills::Material;
use serde_json::{json, Value as Json};

const TIDES: &str = "def depth(chart, mark):\n    low = chart[mark]\n    return low - 1\n\ndef berth(chart, mark):\n    low = chart[mark]\n    return low + 1\n";

fn files() -> Vec<CodeFile> {
    vec![
        CodeFile {
            path: Told("tide/harbour.py".into()),
            text: Told(TIDES.into()),
        },
        CodeFile {
            path: Told("kiln/stack.py".into()),
            text: Told("heat = 3\n".into()),
        },
    ]
}

fn review_input() -> ReviewInput {
    ReviewInput {
        wording: Told("Read the tide tables as a harbour pilot would.".into()),
        subject: Told("A harbour office keeps these files.".into()),
        files: files(),
        output_tokens: Internal::of(400),
    }
}

fn entry_input() -> EntryInput {
    EntryInput {
        wording: Told("Describe the finding as an entry.".into()),
        subject: Told("A harbour office keeps these files.".into()),
        heading: Told("The files:".into()),
        files: files(),
        path: Told("tide/harbour.py".into()),
        quote: Told("    return low - 1".into()),
        comment: Told("The depth is one short.".into()),
        reason: Told("It is a real problem.".into()),
        patterns: patterns_for(&Material::code("python"))
            .map(Candidate::of)
            .collect(),
        output_tokens: Internal::of(300),
    }
}

/// A model that answers each call of `C` with the text at that call's place in `texts`.
fn script<C: Contract>(texts: Vec<String>) -> ScriptedModel {
    ScriptedModel::new().on_request::<C>(move |_, nth| {
        let text = texts.get(nth).cloned().unwrap_or_default();
        Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
    })
}

fn remark(from: &str, to: &str) -> Json {
    json!({"from": from, "to": to, "comment": "The tide is read wrongly.", "severity": "minor"})
}

fn review(items: Vec<Json>) -> String {
    json!({ "items": items }).to_string()
}

async fn reviewed(texts: Vec<String>) -> Found<Remark> {
    let model = script::<BlindReviewAsk>(texts);
    ask_new::<BlindReviewAsk>(
        &model,
        &Plain,
        &mut Scope::for_question(),
        review_input(),
        |_| None,
        &CallOptions::default(),
    )
    .await
    .expect("the review is asked")
}

fn replacing(at: &str, with: &str) -> Json {
    json!({"replace": [{"at": at, "with": with}], "insert": [], "remove": [], "move": []})
}

fn issue(fix: Json) -> String {
    json!({"entry": {
        "kind": "issue",
        "pattern": "p1",
        "severity": "minor",
        "consequence": "The depth is one short.",
        "fix": fix,
        "explanation": "Read the chart as it is."
    }})
    .to_string()
}

async fn described(model: &ScriptedModel) -> Settled<Entry> {
    ask::<KeyEntryAsk>(
        model,
        &Plain,
        &mut Scope::for_question(),
        &entry_input(),
        &CallOptions::default(),
    )
    .await
    .expect("the entry is asked")
}

fn last_said(model: &ScriptedModel, call: usize) -> String {
    model
        .sent()
        .get(call)
        .and_then(|sent| {
            sent.request
                .input()
                .iter()
                .rev()
                .find_map(|input| match input {
                    CompletionInput::Message { content, .. } => Some(content.clone()),
                    _ => None,
                })
        })
        .unwrap_or_default()
}

#[test]
fn the_review_request_shows_what_it_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = ReviewInput {
        wording: marks.told("Read the tide tables as a harbour pilot would.".to_owned()),
        subject: marks.told("A harbour office keeps these files.".to_owned()),
        files: vec![CodeFile {
            path: marks.told("tide/harbour.py".to_owned()),
            text: marks.told("depth = chart[mark]".to_owned()),
        }],
        output_tokens: marks.internal(123_456_usize),
    };
    holds::<BlindReviewAsk>(&input, &marks);
}

#[test]
fn the_entry_request_shows_what_it_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = EntryInput {
        wording: marks.told("Describe the finding as an entry.".to_owned()),
        subject: marks.told("A harbour office keeps these files.".to_owned()),
        heading: marks.told("The files of the harbour office:".to_owned()),
        files: vec![CodeFile {
            path: marks.told("tide/harbour.py".to_owned()),
            text: marks.told("depth = chart[mark]".to_owned()),
        }],
        path: marks.told("tide/harbour.py".to_owned()),
        quote: marks.told("depth = chart[mark]".to_owned()),
        comment: marks.told("The depth is read wrongly.".to_owned()),
        reason: marks.told("It is a real problem.".to_owned()),
        patterns: vec![Candidate {
            id: marks.internal("harbour.tide_table".to_owned()),
            name: marks.told("Tide table misread".to_owned()),
            definition: marks.told("A tide table is read at the wrong mark.".to_owned()),
        }],
        output_tokens: marks.internal(654_321_usize),
    };
    holds::<KeyEntryAsk>(&input, &marks);
}

#[tokio::test]
async fn a_review_is_read_into_remarks_whose_lines_the_app_places() {
    let found = reviewed(vec![review(vec![remark("u2", "u3"), remark("u8", "u8")])]).await;
    assert!(found.rest.is_none());
    let findings: Vec<_> = found
        .items
        .iter()
        .filter_map(|remark| reviewed_of(&files(), remark))
        .collect();
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].path, "tide/harbour.py");
    assert_eq!(
        findings[0].quote,
        "    low = chart[mark]\n    return low - 1"
    );
    let span = findings[0].span.clone().expect("a span");
    assert_eq!((span.start, span.end), (2, 3));
    assert_eq!(findings[1].path, "kiln/stack.py");
    assert_eq!(findings[1].quote, "heat = 3");
}

#[tokio::test]
async fn a_remark_over_two_files_or_backwards_is_not_a_finding() {
    let found = reviewed(vec![review(vec![
        remark("u3", "u8"),
        remark("u3", "u2"),
        remark("u3", "u3"),
    ])])
    .await;
    assert_eq!(found.items.len(), 1);
}

#[tokio::test]
async fn a_remark_at_a_line_that_was_not_shown_is_left_out() {
    let found = reviewed(vec![review(vec![remark("u99", "u99"), remark("u3", "u3")])]).await;
    assert_eq!(found.items.len(), 1);
}

#[tokio::test]
async fn a_review_that_was_cut_off_says_it_is_incomplete() {
    let model = ScriptedModel::new().on_request::<BlindReviewAsk>(|_, _| {
        let cut = CompletionResponse {
            finish_reason: "length".into(),
            ..CompletionResponse::from_text(
                review(vec![remark("u3", "u3")])
                    .trim_end_matches("]}")
                    .to_owned(),
            )
        };
        Box::pin(std::future::ready(Ok(cut)))
    });
    let found = ask_new::<BlindReviewAsk>(
        &model,
        &Plain,
        &mut Scope::for_question(),
        review_input(),
        |_| None,
        &CallOptions::default(),
    )
    .await
    .expect("the review is asked");
    assert!(found.rest.is_some());
}

#[tokio::test]
async fn a_finding_of_no_pattern_adds_nothing() {
    let model = script::<KeyEntryAsk>(vec![json!({"entry": {"kind": "none"}}).to_string()]);
    let Settled::Answered(entry) = described(&model).await else {
        panic!("the entry is answered");
    };
    assert!(matches!(entry.0, Described::Nothing));
}

#[tokio::test]
async fn a_fix_that_changes_one_line_becomes_the_edit_of_that_line() {
    let model = script::<KeyEntryAsk>(vec![issue(replacing("u3", "    return low - 2"))]);
    let Settled::Answered(Entry(Described::Issue { fix, .. })) = described(&model).await else {
        panic!("an issue is answered");
    };
    let edit = fix_of(&files(), &fix).expect("the fix applies");
    assert_eq!(edit.path, "tide/harbour.py");
    assert_eq!(edit.before, "    return low - 1\n");
    assert_eq!(edit.after, "    return low - 2\n");
}

#[tokio::test]
async fn a_fix_of_a_line_the_file_has_twice_takes_in_the_lines_around_it_without_a_correction() {
    let model = script::<KeyEntryAsk>(vec![issue(replacing("u2", "    low = chart[mark] - 1"))]);
    let Settled::Answered(Entry(Described::Issue { fix, .. })) = described(&model).await else {
        panic!("an issue is answered");
    };
    let edit = fix_of(&files(), &fix).expect("the fix applies");
    assert_eq!(
        edit.before,
        "def depth(chart, mark):\n    low = chart[mark]\n    return low - 1\n"
    );
    assert_eq!(model.count::<KeyEntryAsk>(), 1);
}

#[tokio::test]
async fn a_fix_that_changes_nothing_stays_unanswered() {
    let same = issue(replacing("u3", "    return low - 1"));
    let model = script::<KeyEntryAsk>(vec![same.clone(), same]);
    assert!(matches!(described(&model).await, Settled::Unanswered(_)));
    assert_eq!(model.count::<KeyEntryAsk>(), 2);
}

#[tokio::test]
async fn a_fix_across_two_files_stays_unanswered() {
    let across = issue(json!({
        "replace": [{"at": "u3", "with": "    return low - 2"}, {"at": "u8", "with": "heat = 4"}],
        "insert": [], "remove": [], "move": []
    }));
    let model = script::<KeyEntryAsk>(vec![across.clone(), across]);
    assert!(matches!(described(&model).await, Settled::Unanswered(_)));
}

#[tokio::test]
async fn an_entry_without_its_consequence_is_corrected() {
    let bare = json!({"entry": {
        "kind": "issue", "pattern": "p1", "severity": "minor", "consequence": " ",
        "fix": replacing("u3", "    return low - 2"), "explanation": "Read the chart as it is."
    }})
    .to_string();
    let model = script::<KeyEntryAsk>(vec![bare, issue(replacing("u3", "    return low - 2"))]);
    assert!(matches!(
        described(&model).await,
        Settled::Answered(Entry(Described::Issue { .. }))
    ));
    assert!(last_said(&model, 1).contains("Give the consequence and the explanation."));
}
