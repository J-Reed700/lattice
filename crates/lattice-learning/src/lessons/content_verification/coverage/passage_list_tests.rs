//! Direct passage-to-fidelity routing in `coverage::audit`: empty/reflection
//! units finish without LLM calls, verbatim passages are skipped deterministically,
//! and non-verbatim passages go straight to `FidelityAudit`.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::fixture_prompts;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::persistence::Database;

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn prompt_of(sent: &Sent) -> String {
    fixture_prompts(&sent.request)
        .into_iter()
        .next()
        .unwrap_or_default()
}

fn structured_sent(model: &ScriptedModel) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| sent.request.schema().is_some())
        .collect()
}

fn scripted_fidelity(
    decide: impl Fn(&Sent, Value) -> Value + Send + Sync + 'static,
) -> ScriptedModel {
    ScriptedModel::new().on_request::<FidelityAudit>(move |sent, _| {
        if sent.request.schema().is_none() {
            return reply("Draft reasoning.".into());
        }
        let task = crate::content_verification::tests::fixture_task(&prompt_of(sent)).unwrap();
        reply(decide(sent, task).to_string())
    })
}

#[tokio::test]
async fn empty_and_reflection_units_produce_complete_true_without_llm_calls() -> Result<()> {
    let content = vec![
        json!({"kind": "teaching", "body": ""}),
        json!({"kind": "teaching", "body": "   \n\t  "}),
        json!({
            "kind": "teaching",
            "role": "reflection",
            "body": "Reflect on how this rule applies to your own project."
        }),
    ];
    let inventory = Inventory {
        units: (0..content.len())
            .map(|index| UnitClaims {
                index,
                claims: vec![],
                non_factual_reason: "No factual assertions in this section.".into(),
            })
            .collect(),
    };
    let silent = ScriptedModel::new();
    let audited = audit(&silent, &content, &inventory, None).await?;
    assert_eq!(audited.units.len(), 3);
    for (index, unit) in audited.units.iter().enumerate() {
        assert_eq!(unit.index, index);
        assert!(unit.complete, "unit {index} should be complete");
        assert!(unit.missing_assertions.is_empty());
        assert!(!unit.reason.trim().is_empty());
    }
    assert!(silent.sent().is_empty(), "0 passages must make 0 LLM calls");
    Ok(())
}

#[tokio::test]
async fn non_empty_passages_route_directly_to_fidelity_audit_skipping_verbatim() -> Result<()> {
    const VERBATIM: &str = "The harbour gate opens at first light.";
    const PARAPHRASED: &str = "Tide tables are revised every spring.";
    let content = vec![
        json!({"kind": "teaching", "body": VERBATIM}),
        json!({"kind": "teaching", "body": PARAPHRASED}),
        json!({"kind": "teaching", "role": "reflection", "body": "Think about tides."}),
    ];
    let inventory = Inventory {
        units: vec![
            UnitClaims {
                index: 0,
                claims: vec![Claim {
                    quote: VERBATIM.into(),
                    statement: VERBATIM.into(),
                }],
                non_factual_reason: String::new(),
            },
            UnitClaims {
                index: 1,
                claims: vec![Claim {
                    quote: PARAPHRASED.into(),
                    statement: "Every spring, tide tables are updated.".into(),
                }],
                non_factual_reason: String::new(),
            },
            UnitClaims {
                index: 2,
                claims: vec![],
                non_factual_reason: "Reflection prompt only.".into(),
            },
        ],
    };

    let complete_model = scripted_fidelity(|_, task| {
        assert_eq!(task["targetPassage"]["id"], "unit-1-passage-0");
        json!({"missingAssertions": [], "reason": "Represented."})
    });
    let audited = audit(&complete_model, &content, &inventory, None).await?;
    assert!(audited.units.iter().all(|unit| unit.complete));
    assert_eq!(structured_sent(&complete_model).len(), 1);

    let gap_model = scripted_fidelity(|_, task| {
        let sentence_id = task["targetPassage"]["sentences"][0]["id"].clone();
        json!({
            "missingAssertions": [{"sentenceId": sentence_id, "statement": PARAPHRASED}],
            "reason": "The wording was weakened."
        })
    });
    let gapped = audit(&gap_model, &content, &inventory, None).await?;
    assert!(gapped.units[0].complete);
    assert!(!gapped.units[1].complete);
    assert_eq!(gapped.units[1].missing_assertions.len(), 1);
    assert_eq!(
        gapped.units[1].missing_assertions[0].passage_id,
        "unit-1-passage-0"
    );
    assert!(gapped.units[2].complete);
    assert_eq!(structured_sent(&gap_model).len(), 1);
    Ok(())
}

#[tokio::test]
async fn saved_unit_coverage_is_reused_and_invalidated_when_section_or_claims_change() -> Result<()>
{
    const BODY: &str = "The harbour gate opens at first light.";
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let content = vec![json!({"kind": "teaching", "body": BODY})];
    let inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![Claim {
                quote: BODY.into(),
                statement: "At dawn, the harbour gate opens.".into(),
            }],
            non_factual_reason: String::new(),
        }],
    };
    let asking =
        || scripted_fidelity(|_, _| json!({"missingAssertions": [], "reason": "Represented."}));

    let first = asking();
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let audited = audit(&first, &content, &inventory, None).await?;
        assert!(audited.units[0].complete);
        assert_eq!(structured_sent(&first).len(), 1);
        Ok(())
    })
    .await?;

    let silent = ScriptedModel::new();
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let reused = audit(&silent, &content, &inventory, None).await?;
        assert!(reused.units[0].complete);
        assert_eq!(structured_sent(&silent).len(), 0);

        let changed_content =
            vec![json!({"kind": "teaching", "body": "The harbour gate opens at noon."})];
        let after_content = asking();
        audit(&after_content, &changed_content, &inventory, None).await?;
        assert_eq!(structured_sent(&after_content).len(), 1);

        let mut changed_inventory = inventory.clone();
        changed_inventory.units[0].claims[0].statement =
            "The harbour gate opens at sunrise.".into();
        let after_claims = asking();
        audit(&after_claims, &content, &changed_inventory, None).await?;
        assert_eq!(structured_sent(&after_claims).len(), 1);
        Ok(())
    })
    .await
}
