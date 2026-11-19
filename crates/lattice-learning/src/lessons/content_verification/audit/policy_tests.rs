//! The policy: combining the verdicts of a split claim, consent, and the
//! answer's reservation.

use super::*;
use crate::lessons::verifier_audit::tests::ScriptedVerifier;

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
fn the_answer_reservation_is_measured_from_the_planned_room() {
    let llm = ScriptedVerifier::answering(200_000, |_, _| json!({}));
    let reserve = verdict_reserve(&llm);
    let planned_text = REASON_ROOM_CHARS;
    assert!(
        reserve >= planned_text / 4,
        "{reserve} tokens cannot hold {planned_text} characters"
    );
    assert!(
        reserve > envelope_reserve(&llm),
        "a verdict holds more than the envelope"
    );
    assert!(envelope_reserve(&llm) > 0);
}
