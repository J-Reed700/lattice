#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;
#[test]
fn a_section_patch_keeps_other_content_and_only_receives_its_evidence() -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"First","body":"Keep byte-for-byte"},{"kind":"explanation","title":"Second","body":"Fix this"}],"questions":[{"kind":"quiz","prompt":"Keep this question"}]});
    let context = json!({"candidate":original,"requirements":{},"failedClaims":[{"unit":1,"evidence":["evidence-1"],"repeatedDefect":true},{"unit":2,"evidence":["evidence-2"]}],"evidence":[{"id":"evidence-1","text":"Relevant"},{"id":"evidence-2","text":"Another section"}],"executions":[],"repairHistory":[{"unit":1,"statement":"Rejected here"},{"unit":2,"statement":"Rejected elsewhere"}]});
    let local = section_context(&context, 1, &section_failures(&context, 1))?;
    assert_eq!(local["candidate"]["blocks"].as_array().unwrap().len(), 1);
    assert_eq!(
        local["evidence"],
        json!([{"id":"evidence-1","text":"Relevant"}])
    );
    assert_eq!(
        local["repairHistory"],
        json!([{"unit":1,"statement":"Rejected here"}])
    );
    assert_eq!(local["repeatedDefects"], true);
    // The other section's claim is on its first attempt.
    let other = section_context(&context, 2, &section_failures(&context, 2))?;
    assert_eq!(other["repeatedDefects"], false);
    let patch = json!({"blocks":[{"kind":"explanation","title":"Second","body":"Corrected"}],"questions":[]});
    let mut result = original.clone();
    merge(&mut result, patch, 1)?;
    assert_eq!(result["blocks"][0], original["blocks"][0]);
    assert_eq!(result["questions"], original["questions"]);
    assert_eq!(result["blocks"][1]["body"], "Corrected");
    assert!(merge(&mut result, original.clone(), 1).is_err());
    assert!(merge(
        &mut result,
        json!({"blocks":[{"kind":"recap"}],"questions":[]}),
        1
    )
    .is_err());
    Ok(())
}

#[tokio::test]
async fn an_ambiguous_edit_falls_back_to_one_validated_section() -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Keep that."}],"questions":[]});
    let local =
        json!({"candidate":original,"failedClaims":[{"unit":0,"statement":"Correct one fact."}]});
    let replacement = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Correct that."}],"questions":[]});
    let model = ScriptedModel {
        outputs: Mutex::new(vec![replacement.to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let ambiguous =
        json!({"edits":[{"path":"/blocks/0/body","before":"Keep","after":"Correct"}]}).to_string();

    let repaired = apply_or_replace_section(
        &model, &local, &original, &schema, "blocks", 4_000, &ambiguous,
    )
    .await?;

    assert_eq!(repaired, replacement);
    let prompts = model
        .prompts
        .lock()
        .map_err(|_| AppError::InternalError("fixture lock".into()))?;
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("rejectedEditResponse"));
    Ok(())
}

/// A small window that records each request and edits the next marker.
struct SmallWindow {
    window: usize,
    requests: Mutex<Vec<(String, String)>>,
}

#[async_trait::async_trait]
impl LLMPort for SmallWindow {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let (system, user) =
            crate::features::learning::lessons::structured_call::task_messages(request);
        let mut requests = self.requests.lock().unwrap();
        let marker = format!("[{}]", requests.len());
        requests.push((system.to_owned(), user.to_owned()));
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                json!({"edits":[{"path":"/blocks/0/body","before":marker,"after":marker.replace('[', "(").replace(']', ")")}]})
                    .to_string(),
            ),
        )
    }
    fn model_name(&self) -> &str {
        "small-window"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_section_too_large_for_one_request_is_repaired_in_several_that_fit() -> Result<()> {
    let passages: Vec<_> = (0..12)
        .map(|i| json!({"id":format!("evidence-{i}"),"sourceId":"s","startByte":i * 2000,"endByte":i * 2000 + 1600,"text":format!("passage-{i} {}", "x".repeat(1600))}))
        .collect();
    // Each failed claim is a ledger claim located on its own marker: a
    // repair may change only the failed claims' sentences (claim sources
    // design §10.2), so the edits must land on sentences it targets.
    let body = "[0] [1] [2] [3] [4] [5]";
    let claims: Vec<_> = (0..6)
        .map(|k| json!({"unit":0,"claimId":format!("claim-{k}"),"statement":format!("Claim {k}"),"verdict":"unsupported","reason":"Unstated.","evidence":[format!("evidence-{}", 2 * k), format!("evidence-{}", 2 * k + 1)]}))
        .collect();
    let context = json!({"candidate":{"blocks":[{"kind":"explanation","title":"T","body":body}],"questions":[]},"requirements":{},"failedClaims":claims,"evidence":passages,"executions":[],"repairHistory":[]});
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let output_tokens = 500;
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let references = ReferenceCollection::lexical(&[])?;
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    // Open for repair, as a pass leaves its failed claims.
    for k in 0..6 {
        use lattice_core::domain::learning::claim_review::{
            Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLocation, EvidenceFingerprint,
            UnitKind, VerdictKind,
        };
        let id = ClaimId(format!("claim-{k}"));
        let location = ClaimLocation::locate(0, UnitKind::Block, "body", format!("[{k}]"), body);
        ledger
            .append(
                &id,
                ClaimEvent::Extracted {
                    location,
                    statement: format!("Claim {k}"),
                },
            )
            .await?;
        let check = Check {
            checker: Checker::Local {
                model: llm.model_name().into(),
            },
            result: CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            reason: "Unstated.".into(),
            fingerprint: EvidenceFingerprint::of("fixture-v1", llm.model_name(), &[]),
            evidence: Vec::new(),
            window_tokens: 4_000,
        };
        ledger.append(&id, ClaimEvent::Checked(check)).await?;
        assert!(ledger.may_repair(&id));
    }

    let repaired = repair_sections(
        &llm,
        "{}",
        &schema,
        context,
        output_tokens,
        &references,
        &mut ledger,
    )
    .await?;

    let requests = llm.requests.lock().unwrap();
    assert!(requests.len() >= 3, "{} requests", requests.len());
    for (system, user) in requests.iter() {
        assert!(llm.count_tokens(system) + llm.count_tokens(user) + output_tokens <= 4_000);
        // Each request carries exactly the evidence its own claims cite.
        let local: Value = serde_json::from_str(user)?;
        let cited: Vec<_> = local["failedClaims"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|claim| claim["evidence"].as_array().unwrap().clone())
            .collect();
        let carried: Vec<_> = local["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|passage| passage["id"].clone())
            .collect();
        assert_eq!(cited, carried);
    }
    for k in 0..6 {
        let statement = format!("\"Claim {k}\"");
        assert_eq!(
            requests
                .iter()
                .filter(|(_, user)| user.contains(&statement))
                .count(),
            1
        );
    }
    // Each request edited the section the previous one left.
    let body = repaired["blocks"][0]["body"].as_str().unwrap();
    for part in 0..requests.len() {
        assert!(body.contains(&format!("({part})")), "{body}");
    }
    Ok(())
}
