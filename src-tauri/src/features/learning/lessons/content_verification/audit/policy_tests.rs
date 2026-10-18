//! The policy: what a request says, the answer's schema, grounding, and
//! consent.

use super::*;
use crate::features::learning::lessons::verifier_audit::tests::ScriptedVerifier;
use lattice_core::domain::learning::claim_review::ClaimId;

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

#[test]
fn a_request_sends_each_section_and_passage_once_and_labels_claims_in_order() {
    let claims = vec![
        claim("Water boils at 100 °C at sea level.", vec![shared()], false),
        claim(
            "Salt raises the boiling point.",
            vec![shared(), passage("v2", "Salt water boils later.")],
            false,
        ),
    ];
    let rendered = render(
        &claims,
        &units(),
        &[
            item(0, vec![0], Context::Unit),
            item(1, vec![0, 1], Context::Unit),
        ],
    );

    assert_eq!(rendered.claim_labels, ["C1", "C2"]);
    let labels: Vec<&str> = rendered.passages.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["P1", "P2"], "the shared passage is sent once");
    assert_eq!(rendered.task.matches("[S0] Boiling").count(), 1);
    assert_eq!(rendered.task.matches("Pure water boils").count(), 1);
    assert!(rendered
        .task
        .contains("[C1] Water boils at 100 °C at sea level.\nLesson text:"));
    assert!(rendered.task.contains("Passages: P1, P2"));
    assert!(rendered.task.contains("(source version v2)"));
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
    let rendered = render(&claims, &units(), &[part]);

    assert!(!rendered.task.contains("[S0]"), "no whole section");
    assert!(rendered
        .task
        .contains("Context: \"Water boils at 100 °C at sea level.\""));
    assert!(!rendered.task.contains("Water is everywhere."));
    assert!(rendered
        .task
        .contains("Part 2 of 3 of this claim's passages."));
}

#[test]
fn general_knowledge_is_marked_for_contradiction_only() {
    let claims = vec![claim("Salt raises the boiling point.", vec![], true)];
    let rendered = render(&claims, &units(), &[item(0, vec![], Context::Unit)]);
    assert!(rendered.task.contains(
        "Passages: none (accepted as general knowledge: check it for contradiction only)"
    ));
}

#[test]
fn the_schema_names_the_labels_and_bounds_every_string() {
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let rendered = render(&claims, &units(), &[item(0, vec![0], Context::Unit)]);

    let schema = schema(&rendered, false);
    let verdict = &schema["properties"]["verdicts"]["items"]["properties"];
    assert_eq!(verdict["claim"]["enum"], json!(["C1"]));
    assert_eq!(
        verdict["passagesUsed"]["items"]["properties"]["passage"]["enum"],
        json!(["P1"])
    );
    assert_eq!(
        verdict["verdict"]["enum"],
        json!([
            "supported",
            "contradicted",
            "unsupported",
            "could_not_verify"
        ])
    );
    assert_eq!(verdict["reason"]["maxLength"], json!(REASON_MAX_CHARS));
    assert_eq!(
        verdict["passagesUsed"]["maxItems"],
        json!(QUOTES_PER_VERDICT)
    );

    let with_web = schema(&rendered, true);
    assert!(
        with_web["properties"]["verdicts"]["items"]["properties"]["passagesUsed"]["items"]
            ["properties"]["passage"]
            .get("enum")
            .is_none(),
        "a fetched page's label is not known in advance"
    );
}

fn read_one(answer: Value) -> Judged {
    let claims = vec![claim(
        "Water boils at 100 °C at sea level.",
        vec![shared()],
        false,
    )];
    let rendered = render(&claims, &units(), &[item(0, vec![0], Context::Unit)]);
    let mut answer = answer;
    answer["claim"] = json!("C1");
    read(&json!({"verdicts": [answer]}).to_string(), &rendered, &[])
        .unwrap()
        .remove(0)
        .unwrap()
}

#[test]
fn a_verdict_counts_only_when_it_quotes_a_passage_it_was_given() {
    let quoted = |quote: &str| json!([{"passage": "P1", "quote": quote}]);

    let supported = read_one(json!({"verdict": "supported", "reason": "says so",
        "passagesUsed": quoted("pure  WATER boils at 100 °C at sea level.")}));
    assert_eq!(
        supported.result,
        CheckResult::Supported {
            source_version_id: "v1".into(),
            supporting_quote: "pure  WATER boils at 100 °C at sea level.".into()
        }
    );

    let remembered = read_one(json!({"verdict": "supported", "reason": "I know it",
        "passagesUsed": quoted("Water boils at 100 °C everywhere.")}));
    assert_eq!(
        remembered.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
    assert!(remembered.reason.contains("I know it"));

    let wrong_label = read_one(json!({"verdict": "supported", "reason": "r",
        "passagesUsed": [{"passage": "P9", "quote": "Salt raises it."}]}));
    assert_eq!(
        wrong_label.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );

    let contradicted = read_one(
        json!({"verdict": "contradicted", "reason": "salt changes it",
        "passagesUsed": quoted("Salt raises it.")}),
    );
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

    let bare = read_one(json!({"verdict": "contradicted", "reason": "no", "passagesUsed": []}));
    assert_eq!(
        bare.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );

    let unsupported =
        read_one(json!({"verdict": "unsupported", "reason": "none says so", "passagesUsed": []}));
    assert_eq!(
        unsupported.result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    );

    let unsure =
        read_one(json!({"verdict": "could_not_verify", "reason": "unclear", "passagesUsed": []}));
    assert_eq!(
        unsure.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );

    let invented = read_one(json!({"verdict": "probably", "reason": "", "passagesUsed": []}));
    assert_eq!(
        invented.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
}

#[test]
fn a_page_fetched_and_saved_during_the_request_may_be_quoted() {
    let claims = vec![claim("Salt raises the boiling point.", vec![], true)];
    let rendered = render(&claims, &units(), &[item(0, vec![], Context::Unit)]);
    let fetched = vec![(
        "W1".to_owned(),
        passage("web-1", "Dissolved salt raises the boiling point."),
    )];
    let answer = json!({"verdicts": [{"claim": "C1", "verdict": "supported", "reason": "the page says so",
        "passagesUsed": [{"passage": "W1", "quote": "Dissolved salt raises the boiling point."}]}]});

    let judged = read(&answer.to_string(), &rendered, &fetched).unwrap();

    assert!(matches!(
        &judged[0],
        Some(Judged { result: CheckResult::Supported { source_version_id, .. }, .. }) if source_version_id == "web-1"
    ));
}

#[test]
fn a_claim_the_answer_leaves_out_has_no_verdict_and_prose_is_unreadable() {
    let claims = vec![
        claim("Water boils at 100 °C at sea level.", vec![shared()], false),
        claim("Salt raises the boiling point.", vec![shared()], false),
    ];
    let rendered = render(
        &claims,
        &units(),
        &[
            item(0, vec![0], Context::Unit),
            item(1, vec![0], Context::Unit),
        ],
    );
    let answer = json!({"verdicts": [
        {"claim": "C2", "verdict": "unsupported", "reason": "r", "passagesUsed": []},
        {"claim": "C2", "verdict": "supported", "reason": "a second answer is ignored", "passagesUsed": []},
        {"claim": "C7", "verdict": "unsupported", "reason": "no such claim", "passagesUsed": []}
    ]});

    let judged = read(&answer.to_string(), &rendered, &[]).unwrap();

    assert!(judged[0].is_none());
    assert_eq!(
        judged[1].as_ref().unwrap().result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    );
    assert!(read("I think they are fine.", &rendered, &[]).is_err());
}

#[test]
fn quotes_match_ignoring_case_spacing_and_typography_but_never_when_empty() {
    let text = "It is \u{201C}boiling\u{201D} \u{2014} at 100\n°C.";
    assert!(occurs(text, "it is \"BOILING\" - at 100 °C."));
    assert!(!occurs(text, "at 90 °C"));
    assert!(!occurs(text, "   "));
}

#[test]
fn parts_of_a_split_claim_combine_with_the_strongest_verdict() {
    let failed = |kind| Judged {
        result: CheckResult::Failed { kind },
        reason: String::new(),
    };
    let supported = Judged {
        result: CheckResult::Supported {
            source_version_id: "v2".into(),
            supporting_quote: "q".into(),
        },
        reason: String::new(),
    };
    assert_eq!(
        combine(vec![failed(VerdictKind::Unsupported), supported.clone()])
            .unwrap()
            .result,
        supported.result
    );
    assert_eq!(
        combine(vec![supported.clone(), failed(VerdictKind::Contradicted)])
            .unwrap()
            .result,
        CheckResult::Failed {
            kind: VerdictKind::Contradicted
        }
    );
    assert_eq!(
        combine(vec![
            failed(VerdictKind::CouldNotJudge),
            failed(VerdictKind::Unsupported)
        ])
        .unwrap()
        .result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    );
    assert!(combine(Vec::new()).is_none());
}

#[test]
fn consent_names_a_cloud_provider_and_nothing_else_needs_it() {
    let cloud = |provider| ModelRef::Cloud {
        provider,
        model: "m".into(),
    };
    assert_eq!(
        consent(
            &ModelRef::Local {
                model_id: "q".into()
            },
            None
        ),
        Consent::NotNeeded
    );
    assert_eq!(
        consent(&ModelRef::LlamaCpp { model: "q".into() }, None),
        Consent::NotNeeded
    );
    assert_eq!(
        consent(&cloud(CloudProvider::Anthropic), None),
        Consent::Missing {
            provider: CloudProvider::Anthropic
        }
    );
    assert_eq!(
        consent(&cloud(CloudProvider::Anthropic), Some("anthropic")),
        Consent::Given {
            provider: CloudProvider::Anthropic
        }
    );
    assert_eq!(
        consent(&cloud(CloudProvider::Openai), Some("anthropic")),
        Consent::OtherProvider {
            consented: "anthropic".into(),
            now: CloudProvider::Openai
        }
    );
}

#[test]
fn the_answer_reservation_is_measured_from_the_schema_bounds() {
    let llm = ScriptedVerifier::answering(200_000, |_, _| json!({}));
    let reserve = verdict_reserve(&llm);
    let bounded_text = REASON_MAX_CHARS + QUOTE_MAX_CHARS * QUOTES_PER_VERDICT;
    assert!(
        reserve >= bounded_text / 4,
        "{reserve} tokens cannot hold {bounded_text} characters"
    );
    assert!(envelope_reserve(&llm) > 0);
}
