//! The readers over scripted models: each reading's outcomes and findings from
//! the judges' own fixtures, the receipts they restore and record, and an
//! outage as an error.
use super::super::readings::{Read, ReadContext, Reader, ReaderSet, Readers, Statement};
use super::bridge_tests::{
    citing, half_stated, parts_reply, sources, Reading as ScriptedBridge, STATED, STATEMENT,
};
use super::class_checks_tests::Classes;
use super::evidence_paging_tests::{
    marker_collection, reference, PagingModel, CLAIM, CONTRARY, MARKER,
};
use super::*;
use crate::features::learning::lesson_drafts;
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::learning::acceptance::{Outcome, Reading};
use std::collections::VecDeque;

const EVERY_READING: [Reading; 6] = [
    Reading::Parts,
    Reading::Ground,
    Reading::Page,
    Reading::Consistency,
    Reading::Knowledge,
    Reading::Veto,
];

/// A model that cannot be reached.
struct Down;

#[async_trait::async_trait]
impl LLMPort for Down {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Err(AppError::ServiceNotAvailable("the model is down".into()))
    }
    fn model_name(&self) -> &str {
        "down-fixture"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(false)
    }
}

/// What the readers read: a lesson, the course it cites, its examples' runs
/// and the claims the user settled with evidence of their own.
struct World<'w> {
    lesson: Value,
    references: &'w ReferenceCollection<'w>,
    citations: Citations,
    runs: Vec<execution::Observation>,
    settled: HashMap<(usize, String), Finding>,
}

/// A lesson whose one section is an explanation of `body`.
fn explanation(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Quillet","body":body,"rubric":[]}],"questions":[]})
}

fn world_of<'w>(references: &'w ReferenceCollection<'w>, citations: Citations) -> World<'w> {
    World {
        lesson: explanation(STATEMENT),
        references,
        citations,
        runs: Vec::new(),
        settled: HashMap::new(),
    }
}

/// The run of section 0's example, which passed and printed `stdout`.
fn run_printing(stdout: &str) -> execution::Observation {
    serde_json::from_value(json!({
        "id": "example-0", "unit": 0, "runtime": "python", "code_sha256": "none",
        "code": "print(len('abc'))",
        "result": {
            "status": "passed", "exitCode": 0, "stdout": stdout, "stderr": "",
            "outputTruncated": false, "durationMs": 1
        }
    }))
    .unwrap()
}

fn claim(text: &str) -> Claim {
    Claim {
        quote: text.into(),
        statement: text.into(),
    }
}

fn classed() -> Classed {
    Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    }
}

/// What the reader of `reading` makes of `claims`, each with its section.
async fn read_all(
    reading: Reading,
    model: &dyn LLMPort,
    world: &World<'_>,
    claims: &[(usize, Claim)],
) -> Result<Vec<Read>> {
    let retained = ClaimChecks {
        settled: world.settled.clone(),
        ..ClaimChecks::default()
    };
    let at = ReadContext::new(
        model,
        &world.lesson,
        world.references,
        &world.citations,
        &world.runs,
        &retained,
    )
    .await?;
    let statements: Vec<Statement<'_>> = claims
        .iter()
        .map(|(unit, claim)| Statement {
            unit: *unit,
            claim,
            classed: classed(),
        })
        .collect();
    let readers = Readers::standard();
    let reader: &dyn Reader = readers.get(reading);
    reader.read(&at, &statements).await
}

/// What the reader of `reading` makes of `text` in section 0.
async fn read_one(
    reading: Reading,
    model: &dyn LLMPort,
    world: &World<'_>,
    text: &str,
) -> Result<Read> {
    let mut reads = read_all(reading, model, world, &[(0, claim(text))]).await?;
    assert_eq!(reads.len(), 1);
    Ok(reads.remove(0))
}

fn kinds(parts: &[CheckedPart]) -> Vec<PartKind> {
    parts.iter().map(|part| part.kind).collect()
}

fn sources_of(passages: &[EvidencePassage]) -> Vec<&str> {
    passages
        .iter()
        .map(|passage| passage.source_id.as_str())
        .collect()
}

#[test]
fn every_reading_has_its_reader() {
    let readers = Readers::standard();
    for reading in EVERY_READING {
        assert_eq!(readers.get(reading).reading(), reading);
    }
}

#[tokio::test]
async fn a_statement_its_citation_states_in_part_stands_with_the_rest_as_common_knowledge(
) -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated()]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    let finding = &read.finding;
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert!(finding
        .supporting_quote
        .as_deref()
        .is_some_and(|quote| quote.contains(STATED)));
    assert_eq!(
        kinds(&finding.two_pass.parts),
        vec![PartKind::Stated, PartKind::Common]
    );
    assert_eq!(finding.class, Some(classed()));
    assert_eq!(model.requests(), (1, 0));
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_every_part_is_common_reads_all_common() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![parts_reply(
        json!([
            {"part": "a lexer turns characters into tokens", "kind": "common", "passage": null},
            {"part": "a token is the smallest unit a parser reads", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    )]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::AllCommon);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unsupported);
    assert!(read
        .finding
        .reason
        .contains("No passage states any part of it."));
    Ok(())
}

#[tokio::test]
async fn a_specific_the_citation_lacks_leaves_the_statement_unproven() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "it was written in 2004", "kind": "specific_missing", "passage": null},
        ]),
        "supported",
    )]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Unproven);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unsupported);
    assert!(read
        .finding
        .reason
        .contains("Not given by the passages: it was written in 2004."));
    Ok(())
}

#[tokio::test]
async fn a_part_a_passage_contradicts_refutes_the_statement() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![parts_reply(
        json!([{"part": "Quillet is a parser", "kind": "contradicted", "passage": "passage-0"}]),
        "contradicted",
    )]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Refuted);
    assert_eq!(read.finding.verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn a_statement_with_no_passage_is_unproven_and_nothing_is_asked() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, Citations::default());
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Unproven);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unsupported);
    assert_eq!(read.finding.reason, evidence_checks::NO_PASSAGES);
    assert_eq!(read.finding.class, Some(classed()));
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_section_cites_nothing_is_read_by_parts_on_the_courses_page_and_stands_where_the_page_states_it(
) -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(32_000);

    let read = read_one(
        Reading::Parts,
        &model,
        &world_of(&references, Citations::default()),
        CLAIM,
    )
    .await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding.two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(read.finding.class, Some(classed()));
    assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn a_quotation_found_in_its_source_stands_in_code_without_a_call() -> Result<()> {
    let sources = vec![LearningSourceDto {
        id: "book".into(),
        title: "The Quillet Book".into(),
        url: None,
        excerpt: "Quillet ships with a toolchain. The lexer reads characters and emits tokens one at a time."
            .into(),
        acquired_at: 1,
    }];
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = Citations::new(
        HashMap::from([(
            0,
            references.cited_passages(&["Quillet ships with a toolchain."])?,
        )]),
        HashMap::new(),
    );
    let world = world_of(&references, citations);
    let model = ScriptedBridge::new(Vec::new());
    let quoted =
        "The Quillet Book states: \"The lexer reads characters and emits tokens one at a time.\"";

    let read = read_one(Reading::Parts, &model, &world, quoted).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding.label, Some(Label::Attributed));
    assert_eq!(read.finding.class, Some(classed()));
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn an_unreadable_answer_twice_leaves_the_statement_unjudged() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec!["not json".into(), "still not json".into()]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Unjudged);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unverified);
    assert!(read.finding.two_pass.parts.is_empty());
    assert_eq!(model.requests().0, 2, "asked once more, then left");
    Ok(())
}

#[tokio::test]
async fn the_sections_passed_example_runs_are_read_beside_its_citation() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut world = world_of(&references, citing(&references)?);
    world.runs = vec![run_printing("3\n")];
    // The run is the second of two passages: only a judge shown it can say so.
    let model = ScriptedBridge::new(vec![parts_reply(
        json!([{"part": "len counts the characters", "kind": "stated", "passage": "passage-1"}]),
        "supported",
    )]);

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    let shown: Vec<&str> = read
        .finding
        .evidence
        .iter()
        .map(|passage| passage.retrieval_kind.as_str())
        .collect();
    assert_eq!(shown, vec!["cited", "execution"]);
    Ok(())
}

#[tokio::test]
async fn a_claim_the_user_resolved_with_evidence_of_their_own_stands_without_a_call() -> Result<()>
{
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut world = world_of(&references, citing(&references)?);
    let attached = EvidencePassage {
        source_id: "attached".into(),
        text: STATED.into(),
        start_byte: 0,
        end_byte: STATED.len(),
        retrieval_kind: "user_attached".into(),
        score: 0.0,
    };
    world.settled = HashMap::from([(
        (0, STATEMENT.to_owned()),
        Finding {
            unit: 0,
            quote: STATEMENT.into(),
            statement: STATEMENT.into(),
            verdict: ClaimVerdict::Supported,
            reason: "The source the user attached states it.".into(),
            evidence: vec![attached],
            supporting_quote: Some(STATED.into()),
            resolution: None,
            trust: None,
            label: None,
            class: None,
            two_pass: Default::default(),
        },
    )]);
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(
        read.finding.reason,
        "The source the user attached states it."
    );
    assert_eq!(read.finding.evidence[0].retrieval_kind, "user_attached");
    assert_eq!(read.finding.class, Some(classed()));
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn a_page_of_example_runs_alone_is_asked_of_the_strict_judge_as_pass_a_asks_it() -> Result<()>
{
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    // Nothing is cited, so the page is the section's passed run, and a run is
    // no source for the part-wise judge to read.
    let mut world = world_of(&references, Citations::default());

    world.runs = vec![run_printing("3\n")];
    let model = ScriptedBridge::new(Vec::new());
    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;
    assert_eq!(read.outcome, Outcome::Unproven);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unsupported);
    assert!(read.finding.two_pass.parts.is_empty());
    assert_eq!(
        model.requests(),
        (0, 1),
        "one strict request, none part-wise"
    );

    // A run that states the claim stands it, by the strict judge's verdict.
    world.runs = vec![run_printing(&format!("{MARKER}\n"))];
    let model = PagingModel::new(32_000);
    let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;
    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding.evidence[0].retrieval_kind, "execution");
    assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn the_guard_asks_the_rule_layers_test_with_the_sections_givens() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let passages = references.cited_passages(&[STATED])?;
    let citations = Citations::new(
        HashMap::from([(0, passages.clone()), (1, passages.clone()), (2, passages)]),
        HashMap::new(),
    );
    // Section 0 states what is so; section 1 works a hypothetical case;
    // section 2 is the answer to a question that gives the figure 8.
    let mut world = world_of(&references, citations);
    world.lesson = json!({
        "blocks": [
            {"kind": "explanation", "title": "Rows", "body": "x", "rubric": []},
            {"kind": "worked_example", "title": "Rows", "body": "x", "rubric": []},
        ],
        "questions": [
            {"prompt": "A batch holds 8 rows.", "options": ["8 rows", "9 rows"], "correctIndex": 0, "explanation": "x"},
        ],
    });
    let reply = parts_reply(
        json!([
            {"part": "Quillet reads rows in batches", "kind": "stated", "passage": "passage-0"},
            {"part": "a batch of 8 rows is small enough to hold in memory", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    );
    let model = ScriptedBridge::new(vec![reply.clone(), reply.clone(), reply]);

    let reads = read_all(
        Reading::Parts,
        &model,
        &world,
        &[
            (0, claim(STATEMENT)),
            (1, claim(STATEMENT)),
            (2, claim(STATEMENT)),
        ],
    )
    .await?;

    let outcomes: Vec<Outcome> = reads.iter().map(|read| read.outcome).collect();
    assert_eq!(
        outcomes,
        vec![Outcome::Unproven, Outcome::Stands, Outcome::Stands]
    );
    assert!(reads[0]
        .finding
        .reason
        .contains("Taken as specific, not as common knowledge: a batch of 8 rows"));
    Ok(())
}

#[tokio::test]
async fn a_second_reading_restores_its_own_receipt_and_never_one_pass_a_wrote() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let model = ScriptedBridge::new(vec![half_stated()]);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let mut page = citations.of(0).to_vec();
        canonicalize_evidence(&mut page);
        // Pass A asked the specifics guard with no givens, the reader asks it
        // with the section's: the reader's key carries the guard.
        let compared = ClaimChecks::key(0, &claim(STATEMENT), &page);
        let pass_a_key = claim_receipt_key(&model, &compared);
        let key = claim_receipt_key(&model, &format!("{compared}|guard:rule-layer"));
        assert!(lesson_drafts::checkpoint(&key).await?.is_none());
        // A part-wise receipt pass A wrote for the same statement and page.
        let written = serde_json::to_value(ClaimReceipt {
            verdict: ClaimVerdict::Supported,
            reason: "Pass A's.".into(),
            supporting_quote: Some(STATED.into()),
            interpretation_policy: None,
            shown: None,
        })?;
        lesson_drafts::record_checkpoint(&pass_a_key, written).await?;
        let world = world_of(&references, citing(&references)?);

        let first = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

        assert_eq!(
            model.requests().0,
            1,
            "pass A's receipt was not restored: the statement was asked"
        );
        assert!(
            lesson_drafts::checkpoint(&key).await?.is_some(),
            "recorded under the key that carries the guard"
        );
        let again = read_one(Reading::Parts, &model, &world, STATEMENT).await?;
        assert_eq!(model.requests().0, 1, "restored, not asked");
        assert_eq!(again.outcome, first.outcome);
        assert_eq!(again.finding.two_pass.parts, first.finding.two_pass.parts);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_sections_passages_ground_a_statement_that_a_passage_states() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated()]);

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding.class, Some(classed()));
    Ok(())
}

#[tokio::test]
async fn common_knowledge_alone_does_not_ground_a_statement_the_veto_stands() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let all_common = parts_reply(
        json!([
            {"part": "a lexer turns characters into tokens", "kind": "common", "passage": null},
            {"part": "a token is the smallest unit a parser reads", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    );
    let model = ScriptedBridge::new(vec![all_common]);

    let ground = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(ground.outcome, Outcome::Refuted);
    Ok(())
}

#[tokio::test]
async fn a_contradicting_passage_does_not_ground_a_statement() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "it ships in 2004", "kind": "contradicted", "passage": "passage-0"},
        ]),
        "contradicted",
    )]);

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Refuted);
    assert_eq!(read.finding.verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn no_passage_of_the_section_leaves_the_veto_standing_and_nothing_is_asked() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, Citations::default());
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome, Outcome::Refuted);
    assert_eq!(read.finding.reason, evidence_checks::NO_PASSAGES);
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn ground_reads_the_sections_passages_and_parts_the_sentences() -> Result<()> {
    const SECOND: &str = "A parser consumes the tokens a lexer emits.";
    const SENTENCE: &str = "Quillet feeds a parser the tokens it lexes.";
    let sources = vec![
        reference("alpha".into(), STATED.into()),
        reference("beta".into(), SECOND.into()),
    ];
    let references = ReferenceCollection::lexical(&sources)?;
    // The section cites alpha; its sentence cites beta.
    let lesson = json!({"blocks": [{
        "kind": "explanation", "title": "Quillet", "body": SENTENCE, "rubric": [],
        "quote": STATED, "sourceIndex": 0,
        "citations": [{"sentence": SENTENCE, "quote": SECOND, "sourceIndex": 1}],
    }], "questions": []});
    let mut world = world_of(&references, section_citations(&lesson, &references)?);
    world.lesson = lesson;
    let stated_by = |passage: &str| {
        parts_reply(
            json!([{"part": SENTENCE, "kind": "stated", "passage": passage}]),
            "supported",
        )
    };

    let model = ScriptedBridge::new(vec![stated_by("passage-1")]);
    let ground = read_one(Reading::Ground, &model, &world, SENTENCE).await?;
    assert_eq!(ground.outcome, Outcome::Stands);
    assert_eq!(sources_of(&ground.finding.evidence), vec!["alpha", "beta"]);

    let model = ScriptedBridge::new(vec![stated_by("passage-0")]);
    let parts = read_one(Reading::Parts, &model, &world, SENTENCE).await?;
    assert_eq!(parts.outcome, Outcome::Stands);
    assert_eq!(sources_of(&parts.finding.evidence), vec!["beta"]);
    Ok(())
}

/// A model with a window of its own, which answers every request from a queue.
struct Windowed {
    window: usize,
    replies: Mutex<VecDeque<String>>,
}

impl Windowed {
    fn new(window: usize, replies: Vec<String>) -> Self {
        Self {
            window,
            replies: Mutex::new(replies.into()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Windowed {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        let reply = self.replies.lock().unwrap().pop_front().unwrap_or_default();
        Ok(CompletionResponse::from_text(reply))
    }
    fn model_name(&self) -> &str {
        "windowed-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

const FEEDS: &str = "Quillet feeds a parser the tokens it lexes.";

/// Three passages of a thousand characters, from three sources.
fn three_passages() -> Vec<EvidencePassage> {
    (0..3)
        .map(|index| EvidencePassage {
            source_id: format!("source-{index}"),
            text: format!("{index}{}", "x".repeat(999)),
            start_byte: 0,
            end_byte: 1_000,
            retrieval_kind: "cited".into(),
            score: 0.0,
        })
        .collect()
}

fn texts_of(passages: &[EvidencePassage]) -> Vec<String> {
    passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect()
}

/// The smallest window in which the part-wise judge holds `passages` beside
/// `FEEDS`.
fn window_for_part_wise(passages: &[EvidencePassage]) -> usize {
    let texts = texts_of(passages);
    (1..)
        .find(|&window| {
            evidence_checks::bridge_checker(&Windowed::new(window, Vec::new()))
                .bridge_fits(FEEDS, &texts)
        })
        .unwrap_or_default()
}

/// The window in which the part-wise judge holds two of the three passages and
/// not the third.
fn two_of_three() -> usize {
    let three = three_passages();
    let window = window_for_part_wise(&three) - 1;
    assert!(window_for_part_wise(&three[..2]) <= window);
    window
}

fn stated_by_the_second() -> String {
    parts_reply(
        json!([{"part": FEEDS, "kind": "stated", "passage": "passage-1"}]),
        "supported",
    )
}

#[tokio::test]
async fn ground_takes_the_passages_the_part_wise_judge_holds_not_the_consistency_judges(
) -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let citations = Citations::new(HashMap::from([(0, three_passages())]), HashMap::new());
    let world = world_of(&references, citations);
    let model = Windowed::new(two_of_three(), vec![stated_by_the_second()]);
    // The consistency judge's room takes all three, which overflow this
    // judge's window.
    assert_eq!(
        class_checks::fitted(&model, FEEDS, &three_passages()).len(),
        3
    );

    let read = read_one(Reading::Ground, &model, &world, FEEDS).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(
        sources_of(&read.finding.evidence),
        vec!["source-0", "source-1"]
    );
    Ok(())
}

#[tokio::test]
async fn parts_takes_the_cited_passages_the_part_wise_judge_holds() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let citations = Citations::new(HashMap::from([(0, three_passages())]), HashMap::new());
    let world = world_of(&references, citations);
    let model = Windowed::new(two_of_three(), vec![stated_by_the_second()]);

    let read = read_one(Reading::Parts, &model, &world, FEEDS).await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(
        sources_of(&read.finding.evidence),
        vec!["source-0", "source-1"]
    );
    Ok(())
}

#[test]
fn pass_as_page_is_still_fitted_to_the_strict_judge() {
    let three = three_passages();
    let citations = Citations::new(HashMap::from([(0, three.clone())]), HashMap::new());
    // The window in which the strict check holds two of the three passages.
    let window = (1..)
        .find(|&window| fits_window(&Windowed::new(window, Vec::new()), FEEDS, &three[..2]))
        .unwrap_or_default();
    let model = Windowed::new(window, Vec::new());

    let (page, shown) =
        evidence_checks::attribution_page(&[], &citations, (0, FEEDS), FEEDS, |text, passages| {
            fits_window(&model, text, passages)
        });

    assert_eq!(sources_of(&page), vec!["source-0", "source-1"]);
    assert_eq!(sources_of(&shown), vec!["source-0", "source-1"]);
    // The part-wise judge holds all three in that window: its room is not the
    // strict judge's, which is why a reader fits its page to its own.
    assert!(evidence_checks::bridge_checker(&model).bridge_fits(FEEDS, &texts_of(&three)));
}

/// Unit 0 cites a related passage that does not state the claim, as source 0.
fn citing_the_wrong_passage(references: &ReferenceCollection<'_>) -> Result<Citations> {
    let quote = "Mineral alpha notes 3: property beta was measured, pressure varied.";
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[quote])?)]),
        HashMap::from([(0, "related-3".to_owned())]),
    ))
}

#[tokio::test]
async fn a_page_supported_on_a_source_the_sentence_cites_stands() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // The sentence cites another passage of the marker's source, far from the
    // one that states the claim: the page may show the one it does not cite.
    let elsewhere_in_the_marker = EvidencePassage {
        source_id: "marker".into(),
        text: "Mineral alpha notes 99: property was logged.".into(),
        start_byte: 100_000,
        end_byte: 100_044,
        retrieval_kind: "cited".into(),
        score: 0.0,
    };
    let citations = Citations::new(
        HashMap::from([(0, vec![elsewhere_in_the_marker])]),
        HashMap::new(),
    );
    let model = PagingModel::new(32_000);

    let read = read_one(
        Reading::Page,
        &model,
        &world_of(&references, citations),
        CLAIM,
    )
    .await?;

    assert_eq!(read.outcome, Outcome::Stands);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    assert!(read.finding.two_pass.supported_by.is_none());
    assert_eq!(read.finding.two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(read.finding.two_pass.page, Some(1));
    Ok(())
}

#[tokio::test]
async fn a_page_supported_on_another_source_stands_elsewhere_and_names_it() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(32_000);

    let read = read_one(
        Reading::Page,
        &model,
        &world_of(&references, citing_the_wrong_passage(&references)?),
        CLAIM,
    )
    .await?;

    assert_eq!(read.outcome, Outcome::StandsElsewhere);
    assert_eq!(read.finding.verdict, ClaimVerdict::Supported);
    let found = read.finding.two_pass.supported_by.clone().unwrap();
    assert_eq!(found.source_id, "marker");
    assert_eq!(found.cited, vec!["related-3".to_owned()]);
    assert_eq!(read.finding.class, Some(classed()));
    Ok(())
}

#[tokio::test]
async fn a_page_that_does_not_state_the_claim_leaves_it_unproven() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // A window that holds a few passages: the support is on a later page.
    let model = PagingModel::new(6_000);

    let read = read_one(
        Reading::Page,
        &model,
        &world_of(&references, Citations::default()),
        CLAIM,
    )
    .await?;

    assert_eq!(read.outcome, Outcome::Unproven);
    assert_eq!(read.finding.verdict, ClaimVerdict::Unsupported);
    assert_eq!(model.checks.lock().unwrap().len(), 1, "the first page only");
    Ok(())
}

#[tokio::test]
async fn a_page_that_contradicts_the_claim_refutes_it() -> Result<()> {
    let mut sources = marker_collection();
    sources.push(reference(
        "contrary".into(),
        format!("Mineral alpha notes 98: property beta was measured, pressure varied. {CONTRARY}"),
    ));
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );
    let model = PagingModel::new(32_000);

    let read = read_one(
        Reading::Page,
        &model,
        &world_of(&references, citations),
        CLAIM,
    )
    .await?;

    assert_eq!(read.outcome, Outcome::Refuted);
    assert_eq!(read.finding.verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn a_page_read_twice_is_asked_once_and_its_receipt_is_the_pass_b_key() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(32_000);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let world = world_of(&references, citing_the_wrong_passage(&references)?);

        let first = read_one(Reading::Page, &model, &world, CLAIM).await?;

        assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        let key = claim_receipt_key(
            &model,
            &ClaimChecks::key(0, &claim(CLAIM), &first.finding.evidence),
        );
        assert!(lesson_drafts::checkpoint(&key).await?.is_some());
        let again = read_one(Reading::Page, &model, &world, CLAIM).await?;
        assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(again.outcome, first.outcome);
        Ok(())
    })
    .await
}

const CONSISTENT: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const CONTRADICTED: &str = "Every row is held in memory until the file ends.";
const UNFOUNDED: &str = "The reader keeps its buffer in a ring that wraps after a fixed size.";
const UNREADABLE: &str = "A row ends where the next one starts.";

#[tokio::test]
async fn the_consistency_judges_each_verdict_is_an_outcome_over_the_sections_passages() -> Result<()>
{
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = Classes::new(&[]).consistent(&[
        (CONTRADICTED, "contradicted"),
        (UNFOUNDED, "unfounded_specific"),
        (UNREADABLE, "no such verdict"),
    ]);
    let claims: Vec<(usize, Claim)> = [CONSISTENT, CONTRADICTED, UNFOUNDED, UNREADABLE]
        .into_iter()
        .map(|text| (0, claim(text)))
        .collect();

    let reads = read_all(Reading::Consistency, &model, &world, &claims).await?;

    let seen: Vec<(Outcome, ClaimVerdict)> = reads
        .iter()
        .map(|read| (read.outcome, read.finding.verdict))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Outcome::Stands, ClaimVerdict::Supported),
            (Outcome::Refuted, ClaimVerdict::Contradicted),
            (Outcome::NeedsSource, ClaimVerdict::Unsupported),
            (Outcome::Unjudged, ClaimVerdict::Unverified),
        ]
    );
    assert_eq!(
        reads[0].finding.reason,
        "The passages say nothing against it."
    );
    assert_eq!(reads[1].finding.reason, "The passages say otherwise.");
    assert!(reads
        .iter()
        .all(|read| sources_of(&read.finding.evidence) == vec!["quillet"]));
    assert!(reads
        .iter()
        .all(|read| read.finding.class == Some(classed())));
    // One call each, and the unreadable one asked once more.
    assert_eq!(model.calls("consistency"), 5);
    assert_eq!(model.calls("knowledge"), 0);
    Ok(())
}

const TRUE_COMMON: &str =
    "A page number identifies the page of a document that a value appears on.";
const WANTS_A_SOURCE: &str = "A lone page can hold several values from one table.";
const FALSE: &str = "A header row names the columns of a CSV file after its data.";
const UNSURE: &str = "A cell is the smallest unit of a table.";

fn closed_book() -> Classes {
    Classes::new(&[]).knows(&[
        (
            WANTS_A_SOURCE,
            ["true_common", "true_common", "needs_source"],
        ),
        (FALSE, ["false", "false", "true_common"]),
        (UNSURE, ["maybe", "maybe", "maybe"]),
    ])
}

fn samples() -> Vec<(usize, Claim)> {
    [TRUE_COMMON, WANTS_A_SOURCE, FALSE, UNSURE]
        .into_iter()
        .map(|text| (0, claim(text)))
        .collect()
}

#[tokio::test]
async fn the_knowledge_reading_passes_on_unanimity_and_refutes_on_a_majority() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let model = closed_book();

    let reads = read_all(Reading::Knowledge, &model, &world, &samples()).await?;

    let seen: Vec<(Outcome, ClaimVerdict)> = reads
        .iter()
        .map(|read| (read.outcome, read.finding.verdict))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Outcome::Stands, ClaimVerdict::Supported),
            (Outcome::NeedsSource, ClaimVerdict::Unsupported),
            (Outcome::Refuted, ClaimVerdict::Contradicted),
            (Outcome::Unjudged, ClaimVerdict::Unverified),
        ]
    );
    assert!(reads[0].finding.reason.starts_with("Standard knowledge"));
    assert!(reads[2].finding.reason.contains("(2 of 3)"));
    assert!(reads
        .iter()
        .all(|read| read.finding.evidence.is_empty() && read.finding.class == Some(classed())));
    // Three samples each, the unreadable one stopping at its first.
    assert_eq!(model.calls("knowledge"), 3 + 3 + 3 + 2);
    assert_eq!(model.calls("consistency"), 0);
    Ok(())
}

#[tokio::test]
async fn the_veto_reading_fails_only_on_a_majority_that_calls_it_false() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let model = closed_book();

    let reads = read_all(Reading::Veto, &model, &world, &samples()).await?;

    let seen: Vec<(Outcome, ClaimVerdict)> = reads
        .iter()
        .map(|read| (read.outcome, read.finding.verdict))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Outcome::Stands, ClaimVerdict::Supported),
            (Outcome::Stands, ClaimVerdict::Supported),
            (Outcome::Refuted, ClaimVerdict::Contradicted),
            (Outcome::Unjudged, ClaimVerdict::Unverified),
        ]
    );
    assert!(
        reads[1].finding.reason.contains("(0 of 3)"),
        "a request for a source is no falsity: {}",
        reads[1].finding.reason
    );
    assert!(reads[2].finding.reason.contains("(2 of 3)"));
    Ok(())
}

#[tokio::test]
async fn an_outage_is_an_error_from_every_reader_and_never_an_outcome() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let citations = Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );
    let world = world_of(&references, citations);
    for reading in EVERY_READING {
        let reads = read_all(reading, &Down, &world, &[(0, claim(CLAIM))]).await;
        assert!(
            matches!(reads, Err(AppError::ServiceNotAvailable(_))),
            "{reading:?}: {reads:?}"
        );
    }
    Ok(())
}
