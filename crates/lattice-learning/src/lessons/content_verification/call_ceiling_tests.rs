//! The ceiling on a lesson's calls over whole rounds (acceptance design §2.4):
//! the plan holds each call its checks make before it is made, so the ceiling
//! the panel shows rises only for a cause the log names.
use super::class_checks_tests::Classes;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Practice that states nothing to check and shares no word with the
/// statements below.
const PRACTICE: &str =
    " Try it yourself: open the sample file in a text editor and look over its layout before you go on.";

/// The classes fixture, whose repairs write each `from` they find as its `to`,
/// and whose first teaching review finds a defect in a sentence no claim
/// stands on and repairs it by writing a statement there, as the review after
/// a repair did in the Haiku run of 2026-11-04 (lesson `9ad43b26`, the claim
/// on "In this example, a missed referral note is treated as the expensive
/// error.").
struct Rounds {
    classes: Classes,
    rewrites: Vec<(&'static str, &'static str)>,
    /// The section, the sentence the first teaching review finds at fault, and
    /// what its repair writes in that sentence's place.
    teaching: Option<(usize, &'static str, &'static str)>,
    reviews: AtomicUsize,
    repairs: AtomicUsize,
    /// What the knowledge judge was asked about, once for each sample.
    known: Mutex<Vec<String>>,
}

impl Rounds {
    fn new(classes: Classes) -> Self {
        Self {
            classes,
            rewrites: Vec::new(),
            teaching: None,
            reviews: AtomicUsize::new(0),
            repairs: AtomicUsize::new(0),
            known: Mutex::new(Vec::new()),
        }
    }

    fn rewriting(mut self, rewrites: &[(&'static str, &'static str)]) -> Self {
        self.rewrites = rewrites.to_vec();
        self
    }

    fn teaching(mut self, section: usize, sentence: &'static str, written: &'static str) -> Self {
        self.teaching = Some((section, sentence, written));
        self
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if let Some((section, sentence, written)) = self.teaching {
            if prompt.starts_with("Review instructional quality.")
                && self.reviews.fetch_add(1, Ordering::Relaxed) == 0
            {
                return Ok(fault(prompt, section, sentence));
            }
            if prompt.starts_with("Check whether proposed lesson-review findings are justified") {
                return Ok(json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":[format!("blocks-{section}")],"partId":format!("blocks-{section}"),"requirementId":null,"reason":"The step asks for something the explanation beside it never set up."}]}).to_string());
            }
            if prompt.starts_with("You are editing a rejected draft.") {
                return Ok(json!({"edits":[{"path":format!("/blocks/{section}/body"),"before":sentence,"after":written}]}).to_string());
            }
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            self.repairs.fetch_add(1, Ordering::Relaxed);
            let data = fixture_task(prompt).unwrap();
            let body = data["candidate"]["blocks"][0]["body"].as_str().unwrap();
            let (from, to) = self
                .rewrites
                .iter()
                .find(|(from, _)| body.contains(*from))
                .unwrap_or_else(|| panic!("no rewrite is scripted for {body}"));
            return Ok(json!({"edits":[{"path":"/blocks/0/body","before":body,"after":body.replace(from, to)}]}).to_string());
        }
        if prompt.starts_with("You decide whether one statement is standard knowledge") {
            let statement = prompt.rsplit("\n\nStatement: ").next().unwrap_or_default();
            self.known.lock().unwrap().push(statement.to_owned());
        }
        self.classes.respond(prompt).await
    }
}

/// A teaching review that finds a defect in the passage of section `section`
/// holding `sentence`, and none elsewhere.
fn fault(prompt: &str, section: usize, sentence: &str) -> String {
    let data = fixture_task(prompt).unwrap();
    let checks: Vec<Value> = data["candidate"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let passages = block["bodyPassages"].as_array().unwrap();
            if index == section {
                let passage = passages
                    .iter()
                    .find(|passage| {
                        passage["text"]
                            .as_str()
                            .is_some_and(|text| text.contains(sentence))
                    })
                    .unwrap();
                json!({"index":index,"passageId":passage["id"],"finding":"This step asks the learner for something the explanation beside it never set up.","hasDefect":true})
            } else {
                json!({"index":index,"passageId":passages[0]["id"],"finding":"The section explains its point and asks nothing it has not set up.","hasDefect":false})
            }
        })
        .collect();
    json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)}).to_string()
}

#[async_trait::async_trait]
impl LLMPort for Rounds {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "rounds-fixture"
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

fn block(index: usize, body: &str) -> Value {
    json!({"kind":"explanation","title":format!("Section {index}"),"body":body,"rubric":[],"quote":""})
}

/// The lines of the call plan's log.
fn plan_lines(logs: &lattice_core::shared::testing::logs::CapturedLogs) -> Vec<String> {
    logs.text()
        .lines()
        .filter(|line| line.contains("claim.plan"))
        .map(str::to_owned)
        .collect()
}

const ROWS: &str = "Parsing the header row turns each later record into a list of positions.";
const MAPPED: &str =
    "Parsing the header row turns each later record into a mapping from names to values.";
const DELIMITER: &str = "A delimiter separates the fields within one record.";
const ALOUD: &str = "Say each column name aloud before you parse the first record.";
const HEADER_LAST: &str = "A CSV file keeps its header row after its last record.";
const HEADER_FIRST: &str = "A CSV file keeps its header row before its first record.";

#[tokio::test]
async fn a_statement_the_teaching_review_writes_after_the_sort_is_held_by_the_plan_and_every_rise_of_the_ceiling_names_its_cause(
) -> Result<()> {
    let logs = lattice_core::shared::testing::logs::capture_logs(tracing::Level::TRACE);
    // The first section's statement is contradicted and rewritten; the
    // teaching review after that repair writes a statement into the second
    // section, in a sentence no claim stood on. It is new to the ledger, sorted
    // after the plan was made, refuted by every sample and rewritten itself.
    let model = Rounds::new(
        Classes::new(&[ROWS, MAPPED, DELIMITER, HEADER_LAST, HEADER_FIRST])
            .classed(&[
                (ROWS, "reasoning"),
                (DELIMITER, "common"),
                (HEADER_LAST, "common"),
            ])
            .consistent(&[(ROWS, "contradicted")])
            .knows(&[(HEADER_LAST, ["false"; 3])]),
    )
    .rewriting(&[(ROWS, MAPPED), (HEADER_LAST, HEADER_FIRST)])
    .teaching(1, ALOUD, HEADER_LAST);
    // The blank line makes the review read the second section as two
    // paragraphs, so its finding names the one no claim stands on.
    let lesson = json!({"blocks":[
        block(0, &format!("{ROWS}{}", PRACTICE.repeat(3))),
        block(1, &format!("{DELIMITER}{}\n\n{ALOUD}{PRACTICE}{PRACTICE}", PRACTICE.repeat(3))),
    ],"questions":[]});

    let (finished, trace) = crate::lesson_progress::traced(verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson.to_string(),
        &[source()],
        3000,
    ))
    .await;

    let (raw, _) = finished?;
    // The statement the review wrote was checked, rewritten and checked
    // again: its calls are the checks', and counted.
    assert!(raw.contains(HEADER_FIRST), "{raw}");
    let known = model.known.lock().unwrap().clone();
    assert!(
        known.iter().any(|asked| asked.contains(HEADER_LAST))
            && known.iter().any(|asked| asked.contains(HEADER_FIRST)),
        "{known:?}"
    );
    assert!(
        trace.within_the_ceiling(),
        "{} calls, ceilings {:?}",
        trace.calls,
        trace.ceilings
    );
    // The ceiling may rise where the plan takes in a statement it was not
    // made for, and the log says so; it never rises for calls no slot held,
    // which is how the run rose twice with no line of the plan's log.
    let lines = plan_lines(&logs);
    let rises = trace
        .ceilings
        .windows(2)
        .filter(|pair| pair[1] > pair[0])
        .count();
    let named = lines
        .iter()
        .filter(|line| line.contains("claim.plan: the ceiling rose"))
        .count();
    assert_eq!(named, rises, "ceilings {:?}\n{lines:#?}", trace.ceilings);
    assert!(
        !lines.iter().any(|line| line.contains("calls no slot held")),
        "ceilings {:?}\n{lines:#?}",
        trace.ceilings
    );
    Ok(())
}

const ALPHA: &str = "Alpha reads a header row";
const BETA: &str = "beta keeps memory flat";
const GAMMA: &str = "gamma skips blank lines";
const SENTENCE: &str =
    "Alpha reads a header row, beta keeps memory flat and gamma skips blank lines.";
const ALPHA_AGAIN: &str = "Alpha reads a header line";
const BETA_AGAIN: &str = "beta holds memory steady";
const GAMMA_AGAIN: &str = "gamma passes blank lines";
const REWRITTEN: &str =
    "Alpha reads a header line, beta holds memory steady and gamma passes blank lines.";
const NARROWED: &str = "Then beta holds memory steady and gamma passes blank lines.";

#[tokio::test]
async fn the_statements_beside_a_rewritten_one_are_read_again_and_its_narrowing_is_made_from_what_the_plan_holds_and_the_ceiling_never_rises(
) -> Result<()> {
    // Three Reasoning statements stand on one sentence. The first is
    // contradicted and rewritten, which gives the other two successors read
    // from the post-rewrite node in the same pass; its rewrite is refuted by
    // every sample and grounded by no passage, so its cut narrows the sentence,
    // and the other two are read again after that.
    let model = Rounds::new(
        Classes::new(&[ALPHA, BETA, GAMMA, ALPHA_AGAIN, BETA_AGAIN, GAMMA_AGAIN])
            .classed(&[
                (ALPHA, "reasoning"),
                (BETA, "reasoning"),
                (GAMMA, "reasoning"),
            ])
            .consistent(&[(ALPHA, "contradicted")])
            .knows(&[(ALPHA_AGAIN, ["false"; 3])]),
    )
    .rewriting(&[(SENTENCE, REWRITTEN), (REWRITTEN, NARROWED)]);
    let lesson =
        json!({"blocks":[block(0, &format!("{SENTENCE}{}", PRACTICE.repeat(5)))],"questions":[]});

    let (finished, trace) = crate::lesson_progress::traced(verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson.to_string(),
        &[source()],
        3000,
    ))
    .await;

    let (raw, _) = finished?;
    assert!(raw.contains(NARROWED), "{raw}");
    assert_eq!(model.repairs.load(Ordering::Relaxed), 2);
    // The successors' readings come from the reserves held for their
    // statements, and the narrowing's call from its statement's share.
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    assert!(
        trace.within_the_ceiling(),
        "{} calls, ceilings {:?}",
        trace.calls,
        trace.ceilings
    );
    Ok(())
}
