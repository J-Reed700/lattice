//! The audit's contract: what a request says, what the Verifier may cite,
//! and how what it states is grounded.

use serde_json::{json, Value};

use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::ask_each;
use lattice_core::application::services::model_contract::shape::{Held, Shape};
use lattice_core::application::services::model_contract::shown::Scope;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::domain::learning::claim_review::ClaimId;

use super::*;
use crate::lessons::verifier_audit::calls::Audit;
use crate::lessons::verifier_audit::tests::{
    answer_each, handle_of_sentence, sent_claims, usage, Labelled, ScriptedVerifier,
};
use lattice_core::application::ports::llm_port::CompletionResponse;

const UNIT: &str = "Water is everywhere.\n\nWater boils at 100 °C at sea level.\n\nThen it cools.";

fn passage(version: &str, text: &str) -> AuditPassage {
    AuditPassage {
        source_version_id: version.into(),
        title: format!("Handbook {version}"),
        start_byte: 0,
        end_byte: text.len(),
        text: text.into(),
    }
}

fn claim(statement: &str, passages: Vec<AuditPassage>, general_knowledge: bool) -> AuditClaim {
    AuditClaim {
        claim_id: ClaimId(statement.into()),
        unit_index: 0,
        statement: statement.into(),
        quote: statement.into(),
        passages,
        general_knowledge,
    }
}

fn item(claim: usize, passages: Vec<usize>, context: Context) -> Item {
    Item {
        claim,
        passages,
        context,
        part: None,
    }
}

fn units() -> Vec<AuditUnit> {
    vec![AuditUnit {
        index: 0,
        title: "Boiling".into(),
        text: UNIT.into(),
    }]
}

fn shared() -> AuditPassage {
    passage(
        "v1",
        "Pure water boils at 100 °C at sea level. Salt raises it.",
    )
}

fn input_of(claims: &[AuditClaim], items: &[Item], fetched: &[AuditPassage]) -> Input {
    let units = units();
    let asked = items
        .iter()
        .filter_map(|item| Claim::of(claims, &units, item))
        .collect();
    let pages = fetched.iter().map(Passage::of).collect();
    Input::of(asked, &units, Packed::new(pages, |_| 1), 1_000)
}

fn said(input: &Input) -> String {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    VerifierAudit::says(input, &mut showing).unwrap().task
}

#[test]
fn a_request_sends_each_section_and_passage_once_and_names_claims_by_handle() {
    let claims = vec![
        claim("Water boils at 100 °C at sea level.", vec![shared()], false),
        claim(
            "Salt raises the boiling point.",
            vec![shared(), passage("v2", "Salt water boils later.")],
            false,
        ),
    ];
    let task = said(&input_of(
        &claims,
        &[
            item(0, vec![0], Context::Unit),
            item(1, vec![0, 1], Context::Unit),
        ],
        &[],
    ));

    assert_eq!(task.matches("[S0] Boiling").count(), 1);
    assert_eq!(task.matches("Pure water boils").count(), 1);
    assert!(task.contains("[c1] Water boils at 100 °C at sea level.\nLesson text:"));
    assert!(task.contains("[c2] Salt raises the boiling point.\nLesson text:"));
    assert!(task.contains("Passages: [p1], [p2]"));
    assert!(task.contains("[p2] Handbook v2"));
    assert!(task.contains("] Salt water boils later."));
    assert!(!task.contains("source version"), "{task}");
}

#[test]
fn a_paragraph_context_carries_only_the_paragraph_holding_the_quote() {
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let mut part = item(0, vec![0], Context::Paragraph);
    part.part = Some((2, 3));
    let task = said(&input_of(&claims, &[part], &[]));

    assert!(!task.contains("[S0]"), "no whole section");
    assert!(task.contains("Context: \"Water boils at 100 °C at sea level.\""));
    assert!(!task.contains("Water is everywhere."));
    assert!(task.contains("Part 2 of 3 of this claim's passages."));
}

#[test]
fn general_knowledge_is_marked_for_contradiction_only() {
    let claims = vec![claim("Salt raises the boiling point.", vec![], true)];
    let task = said(&input_of(&claims, &[item(0, vec![], Context::Unit)], &[]));
    assert!(task.contains(
        "Passages: none (accepted as general knowledge: check it for contradiction only)"
    ));
}

#[test]
fn pages_the_verifier_fetched_are_shown_as_sentences_it_may_cite() {
    let claims = vec![claim("Salt raises the boiling point.", vec![], true)];
    let page = passage("web-1", "Dissolved salt raises the boiling point.");
    let task = said(&input_of(
        &claims,
        &[item(0, vec![], Context::Unit)],
        &[page],
    ));

    assert!(task.contains("Pages you fetched\n\n[p1] Handbook web-1\n[u1] Dissolved salt raises"));
}

#[test]
fn the_answer_form_cites_sentences_and_has_no_quote_or_label_fields() {
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let input = input_of(&claims, &[item(0, vec![0], Context::Unit)], &[]);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    VerifierAudit::says(&input, &mut showing).unwrap();

    for pointers in [Held::Enums, Held::Plain] {
        let schema = <<VerifierAudit as Answered>::Answer as Shape>::schema(&showing, pointers)
            .unwrap()
            .as_json()
            .to_string();
        for word in [
            "sentencesUsed",
            "verdict",
            "reason",
            "could_not_verify",
            "entries",
        ] {
            assert!(schema.contains(word), "{word} missing: {schema}");
        }
        for gone in ["passagesUsed", "quote", "passage\"", "claim\""] {
            assert!(!schema.contains(gone), "{gone} still asked: {schema}");
        }
    }
}

#[test]
fn the_answer_schema_has_no_text_length_rule() {
    use crate::practice::practice_tests::text_length_rules;

    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let input = input_of(&claims, &[item(0, vec![0], Context::Unit)], &[]);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    VerifierAudit::says(&input, &mut showing).unwrap();
    let schema = <<VerifierAudit as Answered>::Answer as Shape>::schema(&showing, Held::Enums)
        .unwrap()
        .as_json()
        .clone();
    assert!(text_length_rules(&schema).is_empty(), "{schema}");
}

fn marked(marks: &mut Marks, version: &str, title: &str, text: &str, start: usize) -> Passage {
    Passage {
        version: marks.internal(version.to_owned()),
        title: marks.told(title.to_owned()),
        start: marks.internal(start),
        end: marks.internal(start + 200_000),
        text: marks.told(text.to_owned()),
    }
}

#[test]
fn what_is_internal_never_reaches_the_request_and_what_is_told_does() {
    let mut marks = Marks::new();
    let cited = marked(
        &mut marks,
        "version-of-the-tide-tables",
        "Harbour tide tables",
        "The harbour crane lifts crate one before the tide turns.",
        100_001,
    );
    let fetched = marked(
        &mut marks,
        "version-of-the-fetched-page",
        "Kiln firing notes",
        "The kiln fires every bowl once the damper is shut.",
        300_001,
    );
    let claim = Claim {
        at: marks.reference(Place {
            claim: 100_003,
            part: Some(100_004),
        }),
        statement: marks.told("The crane lifts crate one at high tide.".to_owned()),
        lesson_text: marks.told("The crane lifts crates at high tide.".to_owned()),
        setting: Setting::Paragraph(
            marks.told("Crates are lifted before the tide turns.".to_owned()),
        ),
        passages: vec![cited],
        general_knowledge: false,
        part: Some(Part {
            index: marks.told(1),
            of: marks.told(2),
        }),
    };
    let section = Section {
        index: marks.told(0),
        title: marks.told("Harbour practice".to_owned()),
        text: marks.told("Every crate is lifted before the tide turns.".to_owned()),
    };
    let input = Input {
        sections: vec![section],
        claims: vec![claim],
        fetched: Packed::new(vec![fetched], |_| 10),
        reserve: marks.internal(123_456),
    };

    holds::<VerifierAudit>(&input, &marks);
}

/// The Verifier's judgements of `items`, each answered by `verdict`, grounded as the audit grounds them.
async fn judged(
    llm: &ScriptedVerifier,
    claims: &[AuditClaim],
    items: &[Item],
    fetched: &[AuditPassage],
) -> Vec<Judged> {
    let units = units();
    let asked: Vec<Claim> = items
        .iter()
        .filter_map(|item| Claim::of(claims, &units, item))
        .collect();
    let pages: Vec<Passage> = fetched.iter().map(Passage::of).collect();
    let packed = Packed::new(pages.clone(), |_| 1);
    let settled = ask_each::<VerifierAudit>(
        llm,
        &Audit::default(),
        &mut Scope::for_question(),
        &asked,
        |open, _| {
            let open: Vec<Claim> = open.iter().map(|claim| (*claim).clone()).collect();
            Input::of(open, &units, packed.clone(), 1_000)
        },
        &CallOptions::default(),
    )
    .await
    .unwrap();
    let evidence = Evidence::of(&asked, &pages);
    settled
        .iter()
        .map(|settled| evidence.judged(settled))
        .collect()
}

fn answering(
    verdict: impl Fn(&str, &[Labelled]) -> Value + Send + Sync + 'static,
) -> ScriptedVerifier {
    ScriptedVerifier::answering(200_000, verdict)
}

async fn judged_one(verdict: Value) -> Judged {
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let llm = answering(move |_, sentences| {
        let mut verdict = verdict.clone();
        if verdict.get("cite").is_some() {
            let text = verdict["cite"].as_str().unwrap_or_default().to_owned();
            verdict["sentencesUsed"] = json!([handle_of_sentence(sentences, &text)]);
            verdict.as_object_mut().unwrap().remove("cite");
        }
        verdict
    });
    judged(&llm, &claims, &[item(0, vec![0], Context::Unit)], &[])
        .await
        .remove(0)
}

#[tokio::test]
async fn a_verdict_counts_only_when_it_cites_a_sentence_it_was_shown() {
    let supported = judged_one(json!({"verdict": "supported", "reason": "says so",
        "cite": "Pure water boils at 100 °C at sea level."}))
    .await;
    assert_eq!(
        supported.result,
        CheckResult::Supported {
            source_version_id: "v1".into(),
            supporting_quote: "Pure water boils at 100 °C at sea level.".into()
        }
    );

    let remembered = judged_one(json!({"verdict": "supported", "reason": "I know it",
        "sentencesUsed": ["u99"]}))
    .await;
    assert_eq!(
        remembered.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
    assert!(remembered.reason.contains("I know it"));
    assert!(remembered.reason.contains("quoted no passage it was given"));

    let contradicted = judged_one(
        json!({"verdict": "contradicted", "reason": "salt changes it",
        "cite": "Salt raises it."}),
    )
    .await;
    assert_eq!(
        contradicted.result,
        CheckResult::Failed {
            kind: VerdictKind::Contradicted
        }
    );
    assert_eq!(
        contradicted.reason,
        "salt changes it Handbook v1: \"Salt raises it.\""
    );

    let bare = judged_one(json!({"verdict": "contradicted", "reason": "no"})).await;
    assert_eq!(
        bare.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );

    let unsupported = judged_one(json!({"verdict": "unsupported", "reason": "none says so"})).await;
    assert_eq!(
        unsupported.result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    );

    let unsure = judged_one(json!({"verdict": "could_not_verify", "reason": "unclear"})).await;
    assert_eq!(
        unsure.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
}

#[tokio::test]
async fn a_verdict_word_outside_the_four_is_unread_and_means_could_not_verify() {
    let invented = judged_one(json!({"verdict": "probably", "reason": "r"})).await;

    assert_eq!(
        invented.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
    assert!(invented.reason.contains("could not be used"));
}

#[tokio::test]
async fn a_page_fetched_and_saved_during_the_request_may_be_cited() {
    let claims = vec![claim("Salt raises the boiling point.", vec![], true)];
    let page = passage("web-1", "Dissolved salt raises the boiling point.");
    let llm = answering(|_, sentences| {
        json!({"verdict": "supported", "reason": "the page says so",
            "sentencesUsed": [handle_of_sentence(sentences, "Dissolved salt raises the boiling point.")]})
    });

    let judged = judged(&llm, &claims, &[item(0, vec![], Context::Unit)], &[page]).await;

    assert!(matches!(
        &judged[0],
        Judged { result: CheckResult::Supported { source_version_id, .. }, .. } if source_version_id == "web-1"
    ));
}

#[tokio::test]
async fn a_claim_the_answer_leaves_out_is_asked_for_alone_and_the_others_stand() {
    let claims = vec![
        claim("Water boils at 100 °C at sea level.", vec![shared()], false),
        claim("Salt raises the boiling point.", vec![shared()], false),
    ];
    let llm = ScriptedVerifier::new(200_000, |request, call| {
        let text = if call == 1 {
            let mut answer: Value = serde_json::from_str(&answer_each(
                request,
                &|_, _| json!({"verdict": "unsupported"}),
            ))
            .unwrap();
            answer["entries"].as_array_mut().unwrap().truncate(1);
            answer.to_string()
        } else {
            answer_each(request, &|_, _| json!({"verdict": "unsupported"}))
        };
        Ok(usage(CompletionResponse::from_text(text)))
    });

    let judged = judged(
        &llm,
        &claims,
        &[
            item(0, vec![0], Context::Unit),
            item(1, vec![0], Context::Unit),
        ],
        &[],
    )
    .await;

    assert!(judged.iter().all(|judged| matches!(
        judged.result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    )));
    assert_eq!(llm.calls(), 2);
    let texts = llm.user_texts();
    assert_eq!(sent_claims(&texts[1]), ["Salt raises the boiling point."]);
}

#[tokio::test]
async fn a_reason_and_sentence_longer_than_the_planned_room_are_read_whole() {
    let sentence = format!(
        "Pure water boils at 100 °C at sea level {}.",
        "and more ".repeat(120)
    );
    let reason = "The passage says so at length. "
        .repeat(30)
        .trim()
        .to_string();
    assert!(reason.chars().count() > crate::lessons::verifier_audit::policy::REASON_ROOM_CHARS);
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![passage("v1", &sentence)],
        false,
    )];
    let cited = sentence.clone();
    let said = reason.clone();
    let llm = answering(move |_, sentences| {
        json!({"verdict": "supported", "reason": said,
            "sentencesUsed": [handle_of_sentence(sentences, &cited)]})
    });

    let judged = judged(&llm, &claims, &[item(0, vec![0], Context::Unit)], &[])
        .await
        .remove(0);

    assert_eq!(judged.reason, reason);
    assert_eq!(
        judged.result,
        CheckResult::Supported {
            source_version_id: "v1".into(),
            supporting_quote: sentence
        }
    );
}
