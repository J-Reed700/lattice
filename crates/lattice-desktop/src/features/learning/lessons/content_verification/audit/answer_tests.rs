//! What the audit does with the Verifier's answers: an unreadable answer
//! asked for once more, an answer cut off at its limit sent again in halves,
//! web pages saved before they may be quoted, and the audit receipt.

use super::*;

#[tokio::test]
async fn an_unreadable_answer_is_asked_for_once_more() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    let llm = ScriptedVerifier::new(BIG, |request, call| {
        Ok(usage(CompletionResponse::from_text(if call == 1 {
            "Sure! Here is my audit.".to_owned()
        } else {
            answer_each(request, &supports)
        })))
    });

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 2);
    assert!(outcome.publishable());
    assert_eq!(
        harness.runs.only().input_tokens,
        2_000,
        "both attempts are counted"
    );
}

#[tokio::test]
async fn twice_unreadable_answers_leave_each_claim_could_not_verify_with_the_cause() {
    let harness = Harness::new(vec![verified("a", BOILS), verified("b", SALT)]);
    let llm = ScriptedVerifier::new(BIG, |_, _| {
        Ok(CompletionResponse::from_text("{\"verdicts\": \"none\"}"))
    });

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 2, "one repeat, no more");
    assert_eq!(outcome.summary().disputes, 2);
    for id in ["a", "b"] {
        let check = harness.store.verifier_checks(id).pop().unwrap();
        assert_eq!(
            check.result,
            CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge
            }
        );
        assert!(
            check.reason.contains("could not be used twice"),
            "{}",
            check.reason
        );
    }
}

#[tokio::test]
async fn an_answer_cut_off_at_its_limit_is_sent_again_in_halves() {
    let harness = Harness::new(vec![verified("a", BOILS), verified("b", SALT)]);
    let llm = ScriptedVerifier::new(BIG, |request, call| {
        if call == 1 {
            let mut cut = CompletionResponse::from_text("{\"verdicts\": [{\"claim\": \"C1\"");
            cut.finish_reason = "length".into();
            return Ok(cut);
        }
        Ok(CompletionResponse::from_text(answer_each(
            request, &supports,
        )))
    });

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 3);
    assert!(outcome.publishable());
    let texts = llm.user_texts();
    assert_eq!(sent_claims(&texts[0]), [BOILS, SALT]);
    assert_eq!(sent_claims(&texts[1]), [BOILS]);
    assert_eq!(sent_claims(&texts[2]), [SALT]);
}

/// The backend's refusal of a batch its window cannot hold, counted exactly.
fn refused() -> Result<CompletionResponse> {
    Err(AppError::PromptExceedsWindow {
        prompt_tokens: 251_588,
        window: 131_072,
        counted_exactly: true,
        message: "This request's prompt is 251588 tokens (counted exactly).".into(),
    })
}

/// A batch the estimate sized under the window can measure over it by the
/// backend's exact count; the refusal halves the batch.
#[tokio::test]
async fn a_batch_the_backend_refuses_for_its_size_is_sent_again_in_halves() {
    for with_web in [false, true] {
        let harness = Harness::new(vec![verified("a", BOILS), verified("b", SALT)]);
        let web = SavingWeb {
            search_fails: false,
            saved: Mutex::new(Vec::new()),
        };
        let llm = ScriptedVerifier::new(BIG, |request, call| {
            if call == 1 {
                return refused();
            }
            Ok(CompletionResponse::from_text(answer_each(
                request, &supports,
            )))
        });

        let outcome = harness
            .audit(&llm, with_web.then_some(&web as &dyn VerifierWeb))
            .await
            .unwrap();

        assert_eq!(llm.calls(), 3, "with_web={with_web}");
        assert!(outcome.publishable(), "with_web={with_web}");
        let texts = llm.user_texts();
        assert_eq!(sent_claims(&texts[1]), [BOILS]);
        assert_eq!(sent_claims(&texts[2]), [SALT]);
    }
}

/// Pages fetched by the Verifier, saved before it may read them.
pub(super) struct SavingWeb {
    pub search_fails: bool,
    pub saved: Mutex<Vec<String>>,
}

#[async_trait]
impl VerifierWeb for SavingWeb {
    async fn search(
        &self,
        need: &crate::features::learning::references::web_research::PlannedNeed,
        _room: crate::features::learning::references::web_research::Room<'_>,
    ) -> Result<Vec<tools::SearchHit>> {
        let query = &need.query;
        if self.search_fails {
            return Err(AppError::Network("search engine unreachable".into()));
        }
        Ok(vec![tools::SearchHit {
            title: format!("About {query}"),
            url: "https://example.org/salt".into(),
            snippet: "Salt does not change the boiling point.".into(),
        }])
    }
    async fn save_page(&self, url: &str) -> Result<AuditPassage> {
        self.saved.lock().unwrap().push(url.to_owned());
        Ok(passage(
            "saved-web-1",
            "Dissolved salt raises the boiling point of water slightly.",
        ))
    }
}

fn tool_call(name: &str, arguments: Value) -> CompletionResponse {
    let call = CompletionInput::ToolCall {
        id: format!("call-{name}"),
        name: name.into(),
        arguments,
    };
    CompletionResponse {
        tool_calls: vec![call.clone()],
        replay: vec![call],
        finish_reason: "tool_calls".into(),
        ..CompletionResponse::default()
    }
}

#[tokio::test]
async fn pages_the_verifier_fetches_are_saved_before_it_can_quote_them() {
    let mut harness = Harness::new(vec![general_knowledge("g", SALT)]);
    harness.passages.clear();
    let web = SavingWeb {
        search_fails: false,
        saved: Mutex::new(Vec::new()),
    };
    let llm = ScriptedVerifier::new(BIG, |request, call| {
        Ok(match call {
            1 => tool_call(
                "search_web",
                json!({"query": "salt boiling point", "kind": "web"}),
            ),
            2 => tool_call("fetch_page", json!({"url": "https://example.org/salt"})),
            _ => {
                let fetched = request.input.iter().any(|item| {
                    matches!(item, CompletionInput::ToolResult { output, .. } if output.starts_with("[W1]"))
                });
                assert!(fetched, "the saved page reaches the Verifier as W1");
                CompletionResponse::from_text(answer_each(request, &|_, _| {
                    json!({"verdict": "contradicted", "reason": "it raises it only slightly",
                           "passagesUsed": [{"passage": "W1",
                               "quote": "Dissolved salt raises the boiling point of water slightly."}]})
                }))
            }
        })
    });

    let outcome = harness.audit(&llm, Some(&web)).await.unwrap();

    assert_eq!(
        web.saved.lock().unwrap().as_slice(),
        ["https://example.org/salt"]
    );
    assert!(llm.sent.lock().unwrap()[0].tools.len() == 2);
    assert_eq!(
        outcome.summary().disputes,
        1,
        "a grounded contradiction of general knowledge"
    );
    assert!(matches!(
        harness.store.state("g"),
        ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            ..
        }
    ));
}

#[tokio::test]
async fn web_search_that_fails_is_noted_and_the_audit_runs_on() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    let web = SavingWeb {
        search_fails: true,
        saved: Mutex::new(Vec::new()),
    };
    let llm = ScriptedVerifier::new(BIG, |request, call| {
        Ok(if call == 1 {
            tool_call("search_web", json!({"query": "boiling", "kind": "web"}))
        } else {
            CompletionResponse::from_text(answer_each(request, &supports))
        })
    });

    let outcome = harness.audit(&llm, Some(&web)).await.unwrap();

    assert!(outcome.publishable());
    let note = outcome.summary().web_note.clone().unwrap();
    assert!(note.contains("Web search was unavailable"), "{note}");
    assert_eq!(
        harness.runs.only().batches[0].web_note.as_deref(),
        Some(note.as_str())
    );
}

#[test]
fn the_receipt_names_the_policy_the_wording_the_model_and_the_passages() {
    let claim = AuditClaim {
        claim_id: ClaimId("a".into()),
        unit_index: 0,
        statement: BOILS.into(),
        quote: BOILS.into(),
        passages: vec![passage("v1", "Water boils at 100 °C at sea level.")],
        general_knowledge: false,
    };
    let base = ledger::receipt(&claim, "cloud:anthropic/claude-x");
    assert_eq!(
        base,
        ledger::receipt(&claim.clone(), "cloud:anthropic/claude-x")
    );
    assert_ne!(base, ledger::receipt(&claim, "cloud:openai/gpt-x"));
    let mut reworded = claim.clone();
    reworded.statement = "Water boils at 100 °C.".into();
    assert_ne!(base, ledger::receipt(&reworded, "cloud:anthropic/claude-x"));
    let mut new_passage = claim.clone();
    new_passage.passages[0].text.push_str(" Revised.");
    assert_ne!(
        base,
        ledger::receipt(&new_passage, "cloud:anthropic/claude-x")
    );
}
