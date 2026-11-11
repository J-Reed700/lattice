//! The knowledge judge is given the case a statement comes from: the example
//! or question that sets the figures its calculation uses.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::class_checks::knowledge_key;
use super::super::classify::case_text;
use super::bridge_tests::sources;
use super::class_checks_tests::Classes;
use super::readings_tests::{claim, read_all, world_of};
use super::*;
use lattice_core::application::services::claim_verification::KNOWLEDGE_SYSTEM;

const ROPE: &str = "A rope is cut into a piece of 3 units and a piece of 4 units.";
const ROPE_TOTAL: &str = "Together the two pieces make 7 units.";
const BEADS_PROMPT: &str =
    "A string holds 2 beads of one colour and 5 of another. How many beads does it hold?";
const BEADS_EXPLANATION: &str = "The 2 and the 5 make 7.";
const BEADS_WRONG: &str = "Together the two colours make 9 beads.";
const CUT: &str = "A cut divides a rope into two parts.";
const CUT_TOTAL: &str = "The parts of a cut rope add up to its whole length.";

/// A model that records every request it is sent and answers as `inner` does.
struct Recording {
    inner: Classes,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl Recording {
    fn new(inner: Classes) -> Self {
        Self {
            inner,
            sent: Mutex::new(Vec::new()),
        }
    }

    /// The system and user message of each request sent.
    fn messages(&self) -> Vec<(String, String)> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|request| {
                let (system, user) = crate::lessons::structured_call::task_messages(request);
                (system.to_owned(), user.to_owned())
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl LLMPort for Recording {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
}

fn worked_example(body: &str) -> Value {
    json!({"blocks":[{"kind":"worked_example","title":"Rope","body":body,"rubric":[]}],"questions":[]})
}

fn explanation(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Rope","body":body,"rubric":[]}],"questions":[]})
}

/// What the knowledge reader asks the model about `statement` in `lesson`.
async fn asked(lesson: Value, statement: &str) -> Result<Vec<(String, String)>> {
    let references = ReferenceCollection::lexical(&sources())?;
    let mut world = world_of(&references, Citations::default());
    world.lesson = lesson;
    let model = Recording::new(Classes::new(&[]));
    read_all(Reading::Knowledge, &model, &world, &[(0, claim(statement))]).await?;
    Ok(model.messages())
}

#[tokio::test]
async fn the_knowledge_judge_is_sent_the_worked_example_a_statement_comes_from() -> Result<()> {
    let body = format!("{ROPE} {ROPE_TOTAL}");

    let sent = asked(worked_example(&body), ROPE_TOTAL).await?;

    assert_eq!(sent.len(), 3);
    for (_, user) in &sent {
        assert!(user.contains(&body), "{user}");
        assert!(
            user.ends_with(&format!("Statement: {ROPE_TOTAL}")),
            "{user}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_statement_of_an_explanation_is_sent_alone_under_the_judge_s_own_instructions(
) -> Result<()> {
    let body = format!("{CUT} {CUT_TOTAL}");

    let sent = asked(explanation(&body), CUT_TOTAL).await?;

    assert_eq!(sent.len(), 3);
    for (system, user) in &sent {
        assert_eq!(system, KNOWLEDGE_SYSTEM);
        assert_eq!(user, &format!("Statement: {CUT_TOTAL}"));
    }
    Ok(())
}

#[test]
fn the_case_of_a_part_is_its_hypothetical_text_or_its_question_without_the_answer() {
    let candidate = json!({
        "blocks": [
            {"kind":"worked_example","title":"Rope","body":ROPE,"rubric":[]},
            {"kind":"explanation","title":"Cuts","body":CUT,"rubric":[]},
        ],
        "questions": [{
            "prompt": BEADS_PROMPT,
            "options": ["6", "7", "8"],
            "explanation": BEADS_EXPLANATION,
            "rationales": ["No.", "Yes.", "No."],
        }],
    });

    assert_eq!(case_text(&candidate, 0), Some(format!("{ROPE}\n\nRope")));
    assert_eq!(case_text(&candidate, 1), None);
    assert_eq!(
        case_text(&candidate, 2),
        Some(format!("{BEADS_PROMPT}\n\n6\n\n7\n\n8"))
    );
    assert_eq!(case_text(&candidate, 3), None);
}

#[test]
fn a_knowledge_receipt_is_kept_apart_by_the_case_it_was_asked_with() {
    let model = Classes::new(&[]);

    let bare = knowledge_key(&model, ROPE_TOTAL, None);
    let rope = knowledge_key(&model, ROPE_TOTAL, Some(ROPE));
    let other = knowledge_key(&model, ROPE_TOTAL, Some(CUT));

    assert_ne!(bare, rope);
    assert_ne!(rope, other);
    assert_eq!(rope, knowledge_key(&model, ROPE_TOTAL, Some(ROPE)));
}

#[tokio::test]
async fn a_statement_the_model_calls_false_says_whether_it_saw_the_case() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    for reading in [Reading::Knowledge, Reading::Veto] {
        for (lesson, own_words) in [
            (
                worked_example(&format!("{ROPE} {BEADS_WRONG}")),
                "Asked with only the case it comes from, the model calls this false",
            ),
            (
                explanation(&format!("{CUT} {BEADS_WRONG}")),
                "Asked on its own, the model calls this false",
            ),
        ] {
            let mut world = world_of(&references, Citations::default());
            world.lesson = lesson;
            let model = Classes::new(&[]).knows(&[(BEADS_WRONG, ["false"; 3])]);

            let reads = read_all(reading, &model, &world, &[(0, claim(BEADS_WRONG))]).await?;

            assert!(
                reads[0].finding.reason.starts_with(own_words),
                "{reading:?}: {}",
                reads[0].finding.reason
            );
        }
    }
    Ok(())
}
