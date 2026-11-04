//! What a repair request tells the model (acceptance design §2.4, §2.6): the
//! goal of each correction, why a retry's first correction could not be
//! applied, and only the sentences an earlier correction replaced or removed.
use super::acceptance_switch_tests::{claim, lesson_of, open, walk, RESEARCHING, SOURCED};
use super::claim_ledger_tests::{prepare, section};
use super::class_checks_tests::Classes;
use super::*;
use crate::features::learning::lesson_drafts;
use lattice_core::domain::learning::acceptance::Step;

/// The draft's input hash in these fixtures, as `prepare` resumes it.
const INPUT: &str = "ledger-fixture";
const REWRITING_SENTENCE: &str = "The reader hands over one row at a time.";
const WAITING_SENTENCE: &str = "A header row names the columns of the file.";
/// A second sentence the fixture sources contradict, beside `BAD`.
const SECOND: &str = "DictReader removes all whitespace from CSV values by default.";
/// What a Sourced statement's rewrite is asked: its node's goal.
const WHAT_THE_PASSAGES_STATE: &str = "Say only what the claim's evidence passages state. Where no passage states a detail, keep the sentence's point and leave that detail out.";
/// Why a rewrite that left the sentence as it was could not be applied.
const LEFT_AS_IT_WAS: &str = "The repair left the sentence as it was.";

/// The steps that leave a Sourced statement at `s.rewrite`.
const AT_REWRITE: [(Step, Outcome); 3] = [
    (Step::Read(Reading::Parts), Outcome::Unproven),
    (Step::Research, Outcome::FoundNothing),
    (Step::Read(Reading::Page), Outcome::Unproven),
];

/// A model that extracts the statements it is given, as `Classes` does, and
/// keeps the context of each repair request, which it answers with an outage:
/// the run ends there.
struct Stopping {
    inner: Classes,
    requests: Mutex<Vec<Value>>,
}

impl Stopping {
    fn new(statements: &[&'static str]) -> Self {
        Self {
            inner: Classes::new(statements),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Repair verified lesson defects.") {
            self.requests.lock().unwrap().push(super::context(prompt));
            return Err(AppError::Network(
                "the run ends at the first repair request".into(),
            ));
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Stopping {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "stopping-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A model that keeps the context of each repair request. Its first correction
/// takes `BAD` out and leaves `SECOND` as it was, so `SECOND` comes back to be
/// asked again; the second request ends the run.
struct Corrections {
    inner: Model,
    requests: Mutex<Vec<Value>>,
}

impl Corrections {
    fn new() -> Self {
        Self {
            inner: Model::new(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = super::context(prompt);
            let passage = data["units"][0]["passages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["field"] == "/body")
                .unwrap();
            let text = passage["text"].as_str().unwrap();
            let claims: Vec<_> = [BAD, SECOND, GOOD]
                .into_iter()
                .filter(|s| text.contains(s))
                .map(|statement| json!({"passageId":passage["id"],"statement":statement}))
                .collect();
            return Ok(inventory::fixture_response(vec![
                json!({"index":0,"claims":claims,"nonFactualReason":""}),
            ])
            .to_string());
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let mut asked = self.requests.lock().unwrap();
            asked.push(super::context(prompt));
            if asked.len() > 1 {
                return Err(AppError::Network(
                    "the run ends at the second repair request".into(),
                ));
            }
            let body = super::context(prompt)["candidate"]["blocks"][0]["body"]
                .as_str()
                .unwrap()
                .replace(&format!("{BAD} "), "");
            return Ok(repair_body(prompt, json!(body)));
        }
        if prompt.starts_with("Source passages:")
            && prompt
                .split("\n\nClaim: ")
                .nth(1)
                .unwrap()
                .starts_with(SECOND)
        {
            return Ok("contradicted\nReason: The fixture reference preserves whitespace rather than removing it.\nSource passage: passage-0".into());
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Corrections {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "corrections-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The repair requests a lesson with two sentences its sources contradict is
/// sent: one for both, and one for the sentence the first correction left.
async fn requests_for_two_contradicted_sentences() -> Result<Vec<Value>> {
    let model = Corrections::new();
    let body = format!("{}{BAD} {SECOND}", format!("{GOOD} ").repeat(12));
    let lesson = json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":body,"rubric":[]}],"questions":[]});

    // The second request ends the run.
    let _ = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson.to_string(),
        &[source()],
        3000,
    )
    .await;

    let requests = model.requests();
    assert_eq!(
        requests.len(),
        2,
        "a request for both sentences, then one for what the first left"
    );
    Ok(requests)
}

#[tokio::test]
async fn a_sourced_statement_at_a_rewrite_is_sent_with_the_goal_of_its_node() -> Result<()> {
    let requests = requests_for_two_contradicted_sentences().await?;

    let failed = requests[0]["failedClaims"].as_array().unwrap();
    assert!(!failed.is_empty());
    for statement in failed {
        assert_eq!(statement["goal"], WHAT_THE_PASSAGES_STATE, "{statement}");
    }
    Ok(())
}

#[tokio::test]
async fn a_statement_at_the_retry_of_a_rewrite_is_told_why_its_first_correction_was_not_applied(
) -> Result<()> {
    let requests = requests_for_two_contradicted_sentences().await?;

    let retried = &requests[1]["failedClaims"][0];
    assert_eq!(retried["statement"], SECOND);
    assert_eq!(retried["notApplied"], LEFT_AS_IT_WAS, "{retried}");
    Ok(())
}

#[tokio::test]
async fn the_repair_history_does_not_carry_a_statement_that_waits_at_a_node_that_is_no_repair(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let body = section(&format!("{REWRITING_SENTENCE} {WAITING_SENTENCE}"));
    let raw = lesson_of(&body).to_string();
    // One statement stands at `s.rewrite`; the other failed and waits at
    // `s.research`.
    lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&raw).await?;
        let mut ledger = open().await?;
        let rewriting = (REWRITING_SENTENCE, REWRITING_SENTENCE);
        let id = claim(&mut ledger, "rewriting", rewriting, &body, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &AT_REWRITE).await?;
        let waiting = (WAITING_SENTENCE, WAITING_SENTENCE);
        let id = claim(&mut ledger, "waiting", waiting, &body, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &RESEARCHING).await?;
        Ok(())
    })
    .await?;
    let program_id = repo.job(&job).await?.program_id;
    let model = Stopping::new(&[REWRITING_SENTENCE, WAITING_SENTENCE]);

    // Research reaches nothing, so the second statement stays where it is,
    // and the first is repaired.
    let ended = research::run(
        pool.clone(),
        program_id,
        "Research methods".into(),
        Some(research::tests::web(false)),
        prepare(&repo, &job, &lesson_id, &model, Some(raw), &[source()]),
    )
    .await
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();

    assert!(ended.contains("first repair request"), "{ended}");
    let requests = model.requests();
    let [request] = requests.as_slice() else {
        panic!("one repair request: {requests:?}");
    };
    let history = request["repairHistory"].to_string();
    assert!(!history.contains(WAITING_SENTENCE), "{history}");
    Ok(())
}
