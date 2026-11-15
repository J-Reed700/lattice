//! The readers over scripted models: each reading's outcomes and findings from
//! the judges' own fixtures, the receipts they restore and record, and an
//! outage as an error.
use super::super::readings::driver::{read_until_remedy, ReadSteps};
use super::super::readings::{Observer, Read, ReadContext, Reader, ReaderSet, Readers, Statement};
use super::bridge_tests::{
    citing, half_stated, parts_reply, sources, Reading as ScriptedBridge, STATED, STATEMENT,
};
use super::class_checks_tests::Classes;
use super::evidence_paging_tests::{
    marker_collection, reference, related, PagingModel, CLAIM, CONTRARY, MARKER,
};
use super::*;
use crate::lesson_drafts;
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::learning::acceptance::{
    start, Outcome, Position, Reading, StandardPolicy,
};
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
pub(in crate::lessons::content_verification) struct World<'w> {
    pub(super) lesson: Value,
    pub(super) references: &'w ReferenceCollection<'w>,
    pub(super) citations: Citations,
    pub(super) runs: Vec<execution::Observation>,
    pub(super) settled: HashMap<(usize, String), Finding>,
}

/// A lesson whose one section is an explanation of `body`.
fn explanation(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Quillet","body":body,"rubric":[]}],"questions":[]})
}

pub(in crate::lessons::content_verification) fn world_of<'w>(
    references: &'w ReferenceCollection<'w>,
    citations: Citations,
) -> World<'w> {
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

pub(super) fn claim(text: &str) -> Claim {
    Claim {
        quote: text.into(),
        statement: text.into(),
    }
}

pub(super) fn classed() -> Classed {
    Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    }
}

/// What the reader of `reading` makes of `claims`, each with its section.
pub(super) async fn read_all(
    reading: Reading,
    model: &dyn LLMPort,
    world: &World<'_>,
    claims: &[(usize, Claim)],
) -> Result<Vec<Read>> {
    let retained = ClaimChecks {
        settled: world.settled.clone(),
        ..ClaimChecks::default()
    };
    read_with(reading, model, world, &retained, claims).await
}

/// What the reader of `reading` makes of `claims`, each with its section, with
/// the choices `retained` holds.
pub(super) async fn read_with(
    reading: Reading,
    model: &dyn LLMPort,
    world: &World<'_>,
    retained: &ClaimChecks,
    claims: &[(usize, Claim)],
) -> Result<Vec<Read>> {
    let at = ReadContext::new(
        model,
        &world.lesson,
        world.references,
        &world.citations,
        &world.runs,
        retained,
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

/// What the reader of `reading` makes of `claims`, each with its section, with
/// `observer` told of each reading as the reader makes it.
pub(super) async fn read_observed(
    reading: Reading,
    model: &dyn LLMPort,
    world: &World<'_>,
    claims: &[(usize, Claim)],
    observer: &Observer<'_>,
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
    let at = at.observed(observer);
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
pub(in crate::lessons::content_verification) async fn read_one(
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

/// Unit 0 cites the passage holding `MARKER`.
fn citing_marker(references: &ReferenceCollection<'_>) -> Result<Citations> {
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    ))
}

/// Whether a comparison read the marker's passage.
fn read_marker(check: &[String]) -> bool {
    check.iter().any(|text| text.contains(MARKER))
}

/// The claims of `inventory`, each with its section.
fn claims_of(inventory: &Inventory) -> Vec<(usize, Claim)> {
    inventory
        .units
        .iter()
        .flat_map(|unit| {
            unit.claims
                .iter()
                .map(move |claim| (unit.index, claim.clone()))
        })
        .collect()
}

/// What the page reader makes of each claim of `inventory` in the course
/// `references`, with the choices `retained` holds: the first page of the
/// course for each, as a pass reads it.
pub(super) async fn paged(
    model: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    retained: &ClaimChecks,
) -> Result<Vec<Read>> {
    let world = world_of(references, Citations::default());
    read_with(
        Reading::Page,
        model,
        &world,
        retained,
        &claims_of(inventory),
    )
    .await
}

/// What the reading driver makes of each claim of `inventory` in the course
/// `references`, with the choices `retained` holds: the standard readers along
/// the standard policy, each statement at the entry of its class's graph.
pub(super) async fn driven(
    model: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    retained: &ClaimChecks,
) -> Result<Vec<ReadSteps>> {
    let world = world_of(references, Citations::default());
    let at = ReadContext::new(
        model,
        &world.lesson,
        world.references,
        &world.citations,
        &world.runs,
        retained,
    )
    .await?;
    let policy = StandardPolicy::new();
    let sorted = classed();
    let entry = start(&policy, sorted.class);
    let claims = claims_of(inventory);
    let statements: Vec<(Statement<'_>, Position)> = claims
        .iter()
        .map(|(unit, claim)| {
            let statement = Statement {
                unit: *unit,
                claim,
                classed: sorted,
            };
            (statement, entry)
        })
        .collect();
    let readers = Readers::standard();
    read_until_remedy(&policy, &readers, &at, &statements).await
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

    assert_eq!(read.outcome(), Outcome::Stands);
    let finding = &read.finding();
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Attribution));
    assert!(finding
        .supporting_quote
        .as_deref()
        .is_some_and(|quote| quote.contains(STATED)));
    assert_eq!(
        kinds(&finding.two_pass.parts),
        vec![PartKind::Stated, PartKind::Common]
    );
    assert_eq!(
        finding.two_pass.parts[0].source_id.as_deref(),
        Some("quillet")
    );
    assert_eq!(finding.two_pass.parts[1].source_id, None);
    assert!(finding
        .reason
        .contains("Common knowledge, not in the passages: a lexer turns characters into tokens."));
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

    assert_eq!(read.outcome(), Outcome::AllCommon);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    assert!(read
        .finding()
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

    assert_eq!(read.outcome(), Outcome::Unproven);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    assert!(read
        .finding()
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

    assert_eq!(read.outcome(), Outcome::Refuted);
    assert_eq!(read.finding().verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn a_statement_with_no_passage_of_its_own_that_the_courses_page_does_not_state_is_unproven(
) -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, Citations::default());
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome(), Outcome::Unproven);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    assert_ne!(read.finding().reason, evidence_checks::NO_PASSAGES);
    assert_eq!(read.finding().class, Some(classed()));
    assert_eq!(
        model.requests(),
        (0, 1),
        "one strict request, none part-wise"
    );
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

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding().two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(read.finding().class, Some(classed()));
    assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn evidence_the_user_brought_settles_a_statement_whose_section_cites_nothing_with_no_page_read(
) -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let mut world = world_of(&references, Citations::default());
    world.settled = HashMap::from([(
        (0, CLAIM.to_owned()),
        Finding {
            unit: 0,
            quote: CLAIM.into(),
            statement: CLAIM.into(),
            verdict: ClaimVerdict::Supported,
            reason: "The source the user attached states it.".into(),
            evidence: Vec::new(),
            supporting_quote: None,
            resolution: None,
            trust: None,
            label: None,
            class: None,
            claim_id: None,
            two_pass: Default::default(),
        },
    )]);
    let model = PagingModel::new(32_000);

    let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(
        read.finding().reason,
        "The source the user attached states it."
    );
    assert!(read.reused());
    assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 0);
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

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding().label, Some(Label::Attributed));
    assert_eq!(read.finding().class, Some(classed()));
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

    assert_eq!(read.outcome(), Outcome::Unjudged);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unverified);
    assert!(read.finding().two_pass.parts.is_empty());
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

    assert_eq!(read.outcome(), Outcome::Stands);
    let shown: Vec<&str> = read
        .finding()
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
            claim_id: None,
            two_pass: Default::default(),
        },
    )]);
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(
        read.finding().reason,
        "The source the user attached states it."
    );
    assert_eq!(read.finding().evidence[0].retrieval_kind, "user_attached");
    assert_eq!(read.finding().class, Some(classed()));
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn a_page_of_example_runs_alone_is_asked_of_the_strict_judge() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    // Nothing is cited, so the page is the section's passed run, and a run is
    // no source for the part-wise judge to read.
    let mut world = world_of(&references, Citations::default());

    world.runs = vec![run_printing("3\n")];
    let model = ScriptedBridge::new(Vec::new());
    let read = read_one(Reading::Parts, &model, &world, STATEMENT).await?;
    assert_eq!(read.outcome(), Outcome::Unproven);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    assert!(read.finding().two_pass.parts.is_empty());
    assert_eq!(
        model.requests(),
        (0, 1),
        "one strict request, none part-wise"
    );

    // A run that states the claim stands it, by the strict judge's verdict.
    world.runs = vec![run_printing(&format!("{MARKER}\n"))];
    let model = PagingModel::new(32_000);
    let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;
    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding().evidence[0].retrieval_kind, "execution");
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

    let outcomes: Vec<Outcome> = reads.iter().map(|read| read.outcome()).collect();
    assert_eq!(
        outcomes,
        vec![Outcome::Unproven, Outcome::Stands, Outcome::Stands]
    );
    assert!(reads[0]
        .finding()
        .reason
        .contains("Taken as specific, not as common knowledge: a batch of 8 rows"));
    Ok(())
}

#[tokio::test]
async fn a_second_reading_restores_its_own_receipt_and_never_one_an_earlier_comparison_wrote(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let model = ScriptedBridge::new(vec![half_stated()]);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let mut page = citations.of(0).to_vec();
        canonicalize_evidence(&mut page);
        // The earlier part-wise comparison asked the specifics guard with no
        // givens, the reader asks it with the section's: the reader's key
        // carries the guard.
        let compared = ClaimChecks::key(0, &claim(STATEMENT), &page);
        let earlier_key = claim_receipt_key(&model, &compared);
        let key = claim_receipt_key(&model, &format!("{compared}|guard:rule-layer"));
        assert!(lesson_drafts::checkpoint(&key).await?.is_none());
        // A part-wise receipt the earlier comparison wrote for the same
        // statement and page.
        let written = serde_json::to_value(ClaimReceipt {
            verdict: ClaimVerdict::Supported,
            reason: "The earlier comparison's.".into(),
            supporting_quote: Some(STATED.into()),
            parts: Vec::new(),
            shown: None,
        })?;
        lesson_drafts::record_checkpoint(&earlier_key, written).await?;
        let world = world_of(&references, citing(&references)?);

        let first = read_one(Reading::Parts, &model, &world, STATEMENT).await?;

        assert_eq!(
            model.requests().0,
            1,
            "the earlier comparison's receipt was not restored: the statement was asked"
        );
        assert!(
            lesson_drafts::checkpoint(&key).await?.is_some(),
            "recorded under the key that carries the guard"
        );
        let again = read_one(Reading::Parts, &model, &world, STATEMENT).await?;
        assert_eq!(model.requests().0, 1, "restored, not asked");
        assert_eq!(again.outcome(), first.outcome());
        assert_eq!(
            again.finding().two_pass.parts,
            first.finding().two_pass.parts
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_sections_passages_ground_a_statement_that_a_passage_states() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let stated = parts_reply(
        json!([{"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"}]),
        "supported",
    );
    let model = ScriptedBridge::new(vec![stated]);

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert_eq!(read.finding().class, Some(classed()));
    Ok(())
}

#[tokio::test]
async fn a_common_part_beside_a_stated_one_does_not_ground_a_statement() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated()]);

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome(), Outcome::Refuted);
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

    assert_eq!(ground.outcome(), Outcome::Refuted);
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

    assert_eq!(read.outcome(), Outcome::Refuted);
    assert_eq!(read.finding().verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn no_passage_of_the_section_leaves_the_veto_standing_and_nothing_is_asked() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, Citations::default());
    let model = ScriptedBridge::new(Vec::new());

    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;

    assert_eq!(read.outcome(), Outcome::Refuted);
    assert_eq!(read.finding().reason, evidence_checks::NO_PASSAGES);
    assert_eq!(model.requests(), (0, 0));
    Ok(())
}

#[tokio::test]
async fn ground_reads_the_passages_of_the_sentence_its_claim_stands_on() -> Result<()> {
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

    let model = ScriptedBridge::new(vec![stated_by("passage-0")]);
    let ground = read_one(Reading::Ground, &model, &world, SENTENCE).await?;
    assert_eq!(ground.outcome(), Outcome::Stands);
    assert_eq!(sources_of(&ground.finding().evidence), vec!["beta"]);

    let model = ScriptedBridge::new(vec![stated_by("passage-0")]);
    let parts = read_one(Reading::Parts, &model, &world, SENTENCE).await?;
    assert_eq!(parts.outcome(), Outcome::Stands);
    assert_eq!(sources_of(&parts.finding().evidence), vec!["beta"]);
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

    let read = read_one(Reading::Ground, &model, &world, FEEDS).await?;

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(
        sources_of(&read.finding().evidence),
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

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(
        sources_of(&read.finding().evidence),
        vec!["source-0", "source-1"]
    );
    Ok(())
}

#[test]
fn a_citations_page_is_still_fitted_to_the_strict_judge() {
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
pub(super) fn citing_the_wrong_passage(references: &ReferenceCollection<'_>) -> Result<Citations> {
    let quote = "Mineral alpha notes 3: property beta was measured, pressure varied.";
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[quote])?)]),
        HashMap::from([(0, "related-3".to_owned())]),
    ))
}

/// A lesson section citing the related passage, as source 0.
pub(super) fn wrongly_cited() -> Value {
    json!({"blocks":[{
        "kind":"explanation","title":"Alpha","body":CLAIM,"rubric":[],
        "quote":"Mineral alpha notes 3: property beta was measured, pressure varied.",
        "sourceIndex":0
    }],"questions":[]})
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

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert!(read.finding().two_pass.supported_by.is_none());
    assert_eq!(read.finding().two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(read.finding().two_pass.page, Some(1));
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

    assert_eq!(read.outcome(), Outcome::StandsElsewhere);
    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    let found = read.finding().two_pass.supported_by.clone().unwrap();
    assert_eq!(found.source_id, "marker");
    assert_eq!(found.cited, vec!["related-3".to_owned()]);
    assert_eq!(read.finding().class, Some(classed()));
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

    assert_eq!(read.outcome(), Outcome::Unproven);
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    assert!(read.finding().wants_research());
    assert_eq!(
        model.checks.lock().unwrap().len(),
        1,
        "the first page only, though the support is on a later one"
    );
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

    assert_eq!(read.outcome(), Outcome::Refuted);
    assert_eq!(read.finding().verdict, ClaimVerdict::Contradicted);
    Ok(())
}

#[tokio::test]
async fn a_page_read_twice_is_asked_once_and_its_receipt_is_keyed_by_the_claim_and_its_page(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(32_000);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let world = world_of(&references, citing_the_wrong_passage(&references)?);

        let first = read_one(Reading::Page, &model, &world, CLAIM).await?;

        assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        let key = claim_receipt_key(
            &model,
            &ClaimChecks::key(0, &claim(CLAIM), &first.finding().evidence),
        );
        assert!(lesson_drafts::checkpoint(&key).await?.is_some());
        let again = read_one(Reading::Page, &model, &world, CLAIM).await?;
        assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(again.outcome(), first.outcome());
        assert_eq!(
            again.finding().two_pass.supported_by,
            first.finding().two_pass.supported_by
        );
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
        .map(|read| (read.outcome(), read.finding().verdict))
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
        reads[0].finding().reason,
        "The passages say nothing against it."
    );
    assert_eq!(reads[1].finding().reason, "The passages say otherwise.");
    assert!(reads
        .iter()
        .all(|read| sources_of(&read.finding().evidence) == vec!["quillet"]));
    assert!(reads
        .iter()
        .all(|read| read.finding().class == Some(classed())));
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
        (FALSE, ["false"; 3]),
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
async fn the_knowledge_reading_passes_on_unanimity_and_refutes_only_when_every_sample_calls_it_false(
) -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let model = closed_book();

    let reads = read_all(Reading::Knowledge, &model, &world, &samples()).await?;

    let seen: Vec<(Outcome, ClaimVerdict)> = reads
        .iter()
        .map(|read| (read.outcome(), read.finding().verdict))
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
    assert!(reads[0].finding().reason.starts_with("Standard knowledge"));
    assert!(reads[2].finding().reason.contains("(3 of 3)"));
    assert!(reads
        .iter()
        .all(|read| read.finding().evidence.is_empty() && read.finding().class == Some(classed())));
    // Three samples each, the unreadable one stopping at its first.
    assert_eq!(model.calls("knowledge"), 3 + 3 + 3 + 2);
    assert_eq!(model.calls("consistency"), 0);
    Ok(())
}

#[tokio::test]
async fn the_veto_reading_fails_only_when_every_sample_calls_it_false() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let model = closed_book();

    let reads = read_all(Reading::Veto, &model, &world, &samples()).await?;

    let seen: Vec<(Outcome, ClaimVerdict)> = reads
        .iter()
        .map(|read| (read.outcome(), read.finding().verdict))
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
        reads[1].finding().reason.contains("(0 of 3)"),
        "a request for a source is no falsity: {}",
        reads[1].finding().reason
    );
    assert!(reads[2].finding().reason.contains("(3 of 3)"));
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

#[tokio::test]
async fn parts_reads_only_what_its_section_cites_and_keys_its_receipt_by_the_claim_and_those_passages(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let world = world_of(&references, citing_marker(&references)?);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;

        assert_eq!(read.outcome(), Outcome::Stands);
        assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
        assert_eq!(read.finding().two_pass.pass, Some(Pass::Attribution));
        assert!(read.finding().two_pass.supported_by.is_none());
        // It compared the cited passage and nothing else, in one request.
        let checks = model.checks.lock().unwrap().clone();
        assert_eq!(checks.len(), 1);
        assert!(read_marker(&checks[0]));
        assert_eq!(checks[0].len(), world.citations.of(0).len());
        assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Its receipt is keyed by the claim and those passages alone, beside
        // the guard it asks with: no selection, no ranking.
        let mut page = world.citations.of(0).to_vec();
        canonicalize_evidence(&mut page);
        let compared = ClaimChecks::key(0, &claim(CLAIM), &page);
        let key = claim_receipt_key(&model, &format!("{compared}|guard:rule-layer"));
        assert!(lesson_drafts::checkpoint(&key).await?.is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_page_never_shows_what_the_citation_shows() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // A window large enough for the whole course on one page.
    let model = PagingModel::new(32_000);
    let world = world_of(&references, citing_marker(&references)?);

    read_one(Reading::Page, &model, &world, CLAIM).await?;

    let checks = model.checks.lock().unwrap();
    assert_eq!(
        checks.len(),
        1,
        "the page alone: the citation is read at its own node"
    );
    assert!(!read_marker(&checks[0]));
    assert_eq!(
        checks[0].len(),
        12,
        "every related source, and not the cited one"
    );
    Ok(())
}

#[tokio::test]
async fn the_first_page_of_a_job_is_bounded_by_the_window_alone_and_the_pages_after_it_by_its_answers(
) -> Result<()> {
    use lattice_core::application::ports::llm_port::Pace;
    let sources: Vec<_> = (0..12).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let pace = Pace {
        prompt_tokens_per_second: 1_000.0,
        output_tokens_per_second: 100.0,
    };
    let model = PagingModel::paced(32_000, pace, 130);
    let claims = [
        (0, claim(CLAIM)),
        (0, claim("Mineral alpha shows property beta under heat.")),
        (0, claim("Mineral alpha shows property beta when cold.")),
    ];
    // No section cites anything: no answer is measured before the first page.
    let world = world_of(&references, Citations::default());

    read_all(Reading::Page, &model, &world, &claims).await?;

    let sizes = model.request_sizes.lock().unwrap().clone();
    let checks = model.checks.lock().unwrap().clone();
    assert_eq!(sizes[0], 1, "the first claim alone");
    assert_eq!(checks[0].len(), 12, "its page bounded by the window alone");
    let budget = page_budget(Some(pace), Some(130.0)).unwrap();
    for check in &checks[1..] {
        let tokens: usize = check.iter().map(|text| bank_tokens(&model, text)).sum();
        assert!(tokens <= budget, "{tokens} > {budget}");
    }
    assert!(checks.len() > 1);
    Ok(())
}

#[tokio::test]
async fn the_answers_the_parts_reading_measured_size_the_pages_the_page_reading_reads() -> Result<()>
{
    use lattice_core::application::ports::llm_port::Pace;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // 130 output tokens an answer, reading ten times faster than writing: a
    // page reads 1,300 tokens, two related passages.
    let pace = Pace {
        prompt_tokens_per_second: 1_000.0,
        output_tokens_per_second: 100.0,
    };
    let model = PagingModel::paced(32_000, pace, 130);
    let world = world_of(&references, citing_marker(&references)?);
    let claims = [
        (0, claim(CLAIM)),
        (0, claim(&format!("{CLAIM} It holds in the lab."))),
    ];
    let retained = ClaimChecks::default();

    read_with(Reading::Parts, &model, &world, &retained, &claims).await?;

    let sizes = model.request_sizes.lock().unwrap().clone();
    assert_eq!(
        sizes,
        [1, 1],
        "each citation is read part by part in a call of its own"
    );
    assert!(model
        .checks
        .lock()
        .unwrap()
        .iter()
        .all(|check| read_marker(check)));

    read_with(Reading::Page, &model, &world, &retained, &claims).await?;

    // Every page is sized by the measured answers, not by the most an answer
    // could be.
    let budget = page_budget(Some(pace), Some(130.0)).unwrap();
    assert_eq!(budget, 1_300);
    let checks = model.checks.lock().unwrap().clone();
    assert!(checks.len() > 2);
    for check in &checks[2..] {
        assert!(!read_marker(check));
        let tokens: usize = check.iter().map(|text| bank_tokens(&model, text)).sum();
        assert!(tokens <= budget, "{tokens} > {budget}");
        assert!(!check.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn a_parts_receipt_survives_a_change_in_the_course_ranking_and_the_page_is_compared_again(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let mut sources = marker_collection();
    let model = PagingModel::new(32_000);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let references = ReferenceCollection::lexical(&sources)?;
        let world = world_of(&references, citing_marker(&references)?);
        read_one(Reading::Parts, &model, &world, CLAIM).await?;
        read_one(Reading::Page, &model, &world, CLAIM).await?;
        assert_eq!(model.checks.lock().unwrap().len(), 2);
        // A new source joins the course and ranks into the page's list.
        sources.push(related(50));
        let references = ReferenceCollection::lexical(&sources)?;
        let world = world_of(&references, citing_marker(&references)?);

        let parts = read_one(Reading::Parts, &model, &world, CLAIM).await?;
        let page = read_one(Reading::Page, &model, &world, CLAIM).await?;

        assert_eq!(parts.outcome(), Outcome::Stands);
        assert!(parts.reused(), "the citation's comparison was restored");
        // The page leaves out the passage the citation shows, and nothing else
        // on it states the claim.
        assert_eq!(page.outcome(), Outcome::Unproven);
        assert_eq!(page.finding().verdict, ClaimVerdict::Unsupported);
        let checks = model.checks.lock().unwrap().clone();
        assert_eq!(checks.len(), 3, "the page alone is compared again");
        assert!(!read_marker(&checks[2]));
        assert!(checks[2].iter().any(|text| text.contains("notes 50")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn another_sections_citations_are_not_the_statements_own() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    // Section 1 cites the support; the claim belongs to section 0.
    let citations = Citations::new(
        HashMap::from([(1, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );
    let world = world_of(&references, citations);

    let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;

    let checks = model.checks.lock().unwrap();
    assert_eq!(
        checks.len(),
        1,
        "no citation of its own: section 0 cites nothing"
    );
    assert!(!read_marker(&checks[0]));
    assert_eq!(read.finding().verdict, ClaimVerdict::Unsupported);
    Ok(())
}

/// The section cites the wrong passage; the sentence of the claim cites the
/// marker's, as source 1.
fn sentence_cited() -> Value {
    let mut lesson = wrongly_cited();
    lesson["blocks"][0]["citations"] = json!([{"sentence":CLAIM,"sourceIndex":1,"quote":MARKER}]);
    lesson
}

#[tokio::test]
async fn a_statement_is_read_against_the_passage_its_own_sentence_cites() -> Result<()> {
    const OTHER: &str = "Tides rise twice a day on most coasts.";
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let lesson = sentence_cited();
    let citations = section_citations(&lesson, &references)?;

    let own = citations.of_claim(0, CLAIM);
    assert!(!own.is_empty() && own.iter().all(|passage| passage.source_id == "marker"));
    // A statement of no cited sentence reads the section's: the passage its
    // own quote cites, and the sentence's.
    let section = citations.of_claim(0, OTHER);
    assert!(section
        .iter()
        .any(|passage| passage.source_id == "related-3"));
    assert!(section.iter().any(|passage| passage.source_id == "marker"));
    assert!(citations.is_cited(0, CLAIM) && !citations.is_cited(0, OTHER));
    assert_eq!(citations.source_at(1), Some("marker"));
    let mut world = world_of(&references, citations);
    world.lesson = lesson;

    let read = read_one(Reading::Parts, &model, &world, CLAIM).await?;

    assert_eq!(read.finding().verdict, ClaimVerdict::Supported);
    assert_eq!(
        read.finding().two_pass.pass,
        Some(Pass::Attribution),
        "the sentence's passage was read, not the section's"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_naming_context_its_sentence_leaves_implicit_is_read_against_its_sentences_passage(
) -> Result<()> {
    const STATEMENT: &str =
        "Acme's analysis log for the sample lists the pressure at which the beta reading was taken.";
    assert!(
        lattice_core::shared::text::claim_sentence(CLAIM, (0, CLAIM.len()), STATEMENT).is_none()
    );
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let lesson = sentence_cited();
    let citations = section_citations(&lesson, &references)?;
    let mut world = world_of(&references, citations);
    world.lesson = lesson;
    let claim = Claim {
        quote: CLAIM.into(),
        statement: STATEMENT.into(),
    };

    let reads = read_all(Reading::Parts, &model, &world, &[(0, claim)]).await?;

    assert_eq!(reads.len(), 1);
    let checks = model.checks.lock().unwrap();
    assert_eq!(checks.len(), 1);
    assert!(read_marker(&checks[0]), "the sentence's passage was read");
    assert!(
        checks[0].iter().all(|text| !text.contains("notes 3:")),
        "not the section's other passage"
    );
    Ok(())
}

#[test]
fn a_claim_is_read_against_the_entries_that_hold_its_sentence_and_no_other() -> Result<()> {
    const SECOND: &str = "Tides rise twice a day on most coasts.";
    const THIRD: &str = "Granite is an igneous rock.";
    const ELSEWHERE: &str = "Something else entirely.";
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let lesson = json!({"blocks":[{
        "kind":"explanation","title":"Alpha","body":format!("{CLAIM} {SECOND} {THIRD}"),"rubric":[],
        "quote":"Mineral alpha notes 3: property beta was measured, pressure varied.",
        "sourceIndex":0,
        "citations":[
            {"sentence":CLAIM,"sourceIndex":1,"quote":MARKER},
            {"sentence":format!("{SECOND} {THIRD}"),"sourceIndex":2,
             "quote":"Mineral alpha notes 5: property beta was measured, pressure varied."},
        ]
    }],"questions":[]});
    let citations = section_citations(&lesson, &references)?;
    let sources = |unit: usize, sentence: &str| -> Vec<String> {
        let mut ids: Vec<String> = citations
            .of_claim(unit, sentence)
            .into_iter()
            .map(|passage| passage.source_id)
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    assert_eq!(sources(0, CLAIM), vec!["marker"]);
    assert_eq!(sources(0, SECOND), vec!["related-5"]);
    assert_eq!(sources(0, THIRD), vec!["related-5"]);
    assert_eq!(
        sources(0, ELSEWHERE),
        vec!["marker", "related-3", "related-5"],
        "a sentence with no entry reads the section's passages"
    );
    assert!(
        sources(1, CLAIM).is_empty(),
        "another section's entries are not its own"
    );
    for sentence in [CLAIM, SECOND, THIRD] {
        assert!(citations.is_cited(0, sentence));
        assert!(!citations.is_cited(1, sentence));
    }
    assert!(!citations.is_cited(0, ELSEWHERE));
    Ok(())
}

/// Reads `asked` and the two `saved` statements of section 0 by `reading`, the
/// two having been read before and their receipts saved, and tells, for each
/// reading the reader reports, its place in the batch, whether a saved check
/// gave it, and how many requests `requests` counts the model was sent since.
async fn told_after(
    reading: Reading,
    model: &dyn LLMPort,
    requests: &(dyn Fn() -> usize + Sync),
    world: &World<'_>,
    (asked, saved): (&str, [&str; 2]),
) -> Result<Vec<(usize, bool, usize)>> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let saved = [(0, claim(saved[0])), (0, claim(saved[1]))];
        read_all(reading, model, world, &saved).await?;
        let before = requests();
        let told = Mutex::new(Vec::new());
        let record = |ordinal: usize, read: &Read| {
            told.lock()
                .unwrap()
                .push((ordinal, read.reused(), requests() - before));
        };
        let claims = [(0, claim(asked)), saved[0].clone(), saved[1].clone()];
        read_observed(reading, model, world, &claims, &record).await?;
        Ok(told.into_inner().unwrap())
    })
    .await
}

#[tokio::test]
async fn consistency_tells_the_receipts_of_its_batch_before_its_first_request() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, citing(&references)?);
    let model = Classes::new(&[]);

    let told = told_after(
        Reading::Consistency,
        &model,
        &|| model.calls("consistency"),
        &world,
        (UNFOUNDED, [CONSISTENT, CONTRADICTED]),
    )
    .await?;

    assert_eq!(told, vec![(1, true, 0), (2, true, 0), (0, false, 1)]);
    Ok(())
}

#[tokio::test]
async fn knowledge_tells_the_receipts_of_its_batch_before_its_first_request() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let model = Classes::new(&[]);

    let told = told_after(
        Reading::Knowledge,
        &model,
        &|| model.calls("knowledge"),
        &world,
        (UNSURE, [TRUE_COMMON, WANTS_A_SOURCE]),
    )
    .await?;

    // The three samples of the one statement asked.
    assert_eq!(told, vec![(1, true, 0), (2, true, 0), (0, false, 3)]);
    Ok(())
}

#[tokio::test]
async fn ground_tells_the_receipts_of_its_batch_before_its_first_request() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated(), half_stated(), half_stated()]);

    let told = told_after(
        Reading::Ground,
        &model,
        &|| model.requests().0,
        &world,
        (UNREADABLE, [CONSISTENT, CONTRADICTED]),
    )
    .await?;

    assert_eq!(told, vec![(1, true, 0), (2, true, 0), (0, false, 1)]);
    Ok(())
}

#[tokio::test]
async fn parts_tells_of_each_statement_of_its_batch_once_at_its_own_place_whoever_reads_it(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let references = ReferenceCollection::lexical(&sources())?;
    // Section 0 cites a passage that Parts reads; section 1 cites nothing, so
    // Parts hands its statements to the page.
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated(), half_stated()]);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let before = [
            (0, claim("Quillet is a lexer that reads characters.")),
            (1, claim("Quillet is a lexer that emits tokens.")),
        ];
        read_all(Reading::Parts, &model, &world, &before).await?;
        let told = Mutex::new(Vec::new());
        let record = |ordinal: usize, read: &Read| {
            told.lock().unwrap().push((ordinal, read.reused()));
        };
        let claims = [
            before[0].clone(),
            before[1].clone(),
            (0, claim("Quillet is a lexer that skips blanks.")),
            (1, claim("Quillet is a lexer that tracks lines.")),
        ];

        read_observed(Reading::Parts, &model, &world, &claims, &record).await?;

        assert_eq!(
            *told.lock().unwrap(),
            vec![(0, true), (1, true), (2, false), (3, false)]
        );
        Ok(())
    })
    .await
}

/// Answers every part-wise check, holding the one about `held` until `gate`
/// is opened.
struct HeldUntil {
    held: &'static str,
    gate: std::sync::Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl LLMPort for HeldUntil {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompt: String = request
            .input
            .iter()
            .filter_map(|input| match input {
                lattice_core::application::ports::llm_port::CompletionInput::Message {
                    content,
                    ..
                } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        if prompt.contains(self.held) {
            self.gate.notified().await;
        }
        Ok(CompletionResponse::from_text(half_stated()))
    }
    fn model_name(&self) -> &str {
        "held-fixture"
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

#[tokio::test]
async fn parts_tells_of_a_statement_while_another_comparison_is_still_in_flight() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, citing(&references)?);
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let model = HeldUntil {
        held: "skips blanks",
        gate: gate.clone(),
    };
    let claims = [
        (0, claim("Quillet is a lexer that reads characters.")),
        (0, claim("Quillet is a lexer that skips blanks.")),
    ];
    let told = Mutex::new(Vec::new());
    let record = |ordinal: usize, _: &Read| {
        told.lock().unwrap().push(ordinal);
        if ordinal == 0 {
            gate.notify_one();
        }
    };

    let reads = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read_observed(Reading::Parts, &model, &world, &claims, &record),
    )
    .await
    .expect("the first statement's reading was told while the second was still held")?;

    assert_eq!(reads.len(), 2);
    assert_eq!(*told.lock().unwrap(), vec![0, 1]);
    Ok(())
}

/// Wraps a model and records the most of its calls that were in flight
/// together; each call is held until `overlap` are in flight, at most 10 s.
pub(super) struct Overlap<'m> {
    inner: &'m dyn LLMPort,
    overlap: usize,
    peak: tokio::sync::watch::Sender<usize>,
    now: std::sync::atomic::AtomicUsize,
    most: std::sync::atomic::AtomicUsize,
}

impl<'m> Overlap<'m> {
    pub(super) fn new(inner: &'m dyn LLMPort, overlap: usize) -> Self {
        Self {
            inner,
            overlap,
            peak: tokio::sync::watch::channel(0).0,
            now: Default::default(),
            most: Default::default(),
        }
    }

    pub(super) fn most(&self) -> usize {
        self.most.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl LLMPort for Overlap<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        use std::sync::atomic::Ordering::SeqCst;
        let now = self.now.fetch_add(1, SeqCst) + 1;
        self.most.fetch_max(now, SeqCst);
        self.peak.send_modify(|peak| *peak = (*peak).max(now));
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.peak.subscribe().wait_for(|peak| *peak >= self.overlap),
        )
        .await;
        let answer = self.inner.complete(request).await;
        self.now.fetch_sub(1, SeqCst);
        answer
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

#[tokio::test]
async fn ground_asks_two_statements_that_both_need_a_call_at_once() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, citing(&references)?);
    let scripted = ScriptedBridge::new(vec![half_stated(), half_stated()]);
    let model = Overlap::new(&scripted, 2);
    let claims = [
        (0, claim("Quillet is a lexer that reads characters.")),
        (0, claim("Quillet is a lexer that skips blanks.")),
    ];

    let reads = read_all(Reading::Ground, &model, &world, &claims).await?;

    assert_eq!(reads.len(), 2);
    assert_eq!(model.most(), 2);
    Ok(())
}

#[tokio::test]
async fn consistency_asks_two_statements_that_both_need_a_call_at_once() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, citing(&references)?);
    let classes = Classes::new(&[]);
    let model = Overlap::new(&classes, 2);
    let claims = [(0, claim(CONSISTENT)), (0, claim(CONTRADICTED))];

    let reads = read_all(Reading::Consistency, &model, &world, &claims).await?;

    assert_eq!(reads.len(), 2);
    assert_eq!(model.most(), 2);
    Ok(())
}

#[tokio::test]
async fn knowledge_asks_two_statements_that_both_need_a_call_at_once() -> Result<()> {
    let references = ReferenceCollection::lexical(&sources())?;
    let world = world_of(&references, Citations::default());
    let classes = closed_book();
    let model = Overlap::new(&classes, 2);
    let claims = [(0, claim(TRUE_COMMON)), (0, claim(WANTS_A_SOURCE))];

    let reads = read_all(Reading::Knowledge, &model, &world, &claims).await?;

    assert_eq!(reads.len(), 2);
    assert_eq!(model.most(), 2);
    Ok(())
}

#[tokio::test]
async fn ground_reads_the_sections_passed_example_runs_as_parts_does() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut world = world_of(&references, Citations::default());
    world.runs = vec![run_printing(&format!(
        "{MARKER}
"
    ))];
    let model = PagingModel::new(32_000);

    let read = read_one(Reading::Ground, &model, &world, CLAIM).await?;

    assert_eq!(read.outcome(), Outcome::Stands);
    assert_eq!(read.finding().evidence[0].retrieval_kind, "execution");
    assert_eq!(model.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}

/// A model with a window of its own that finds every statement consistent
/// and keeps what each request asked.
struct Recording {
    window: usize,
    asked: Mutex<Vec<String>>,
}

impl Recording {
    fn new(window: usize) -> Self {
        Self {
            window,
            asked: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Recording {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = fixture_messages(request);
        self.asked.lock().unwrap().push(user.to_owned());
        Ok(CompletionResponse::from_text(
            json!({"verdict": "consistent", "reason": "Nothing says otherwise."}).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "recording-fixture"
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

#[tokio::test]
async fn consistency_reads_the_sections_passed_example_runs() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut world = world_of(&references, Citations::default());
    world.runs = vec![run_printing(&format!(
        "{MARKER}
"
    ))];
    let model = Recording::new(32_000);

    read_one(Reading::Consistency, &model, &world, CLAIM).await?;

    let asked = model.asked.lock().unwrap();
    assert!(asked.iter().any(|user| user.contains(MARKER)), "{asked:?}");
    Ok(())
}

#[tokio::test]
async fn consistency_shows_every_passage_its_judge_holds() -> Result<()> {
    let three = three_passages();
    let texts = texts_of(&three);
    let window = (1..)
        .find(|&window| {
            ClaimChecker::for_policy(
                &Windowed::new(window, Vec::new()),
                SamplingOverride::deterministic(),
                CheckPolicy::Consistency,
            )
            .consistency_fits(FEEDS, &texts)
        })
        .unwrap_or_default();
    let references = ReferenceCollection::lexical(&sources())?;
    let citations = Citations::new(HashMap::from([(0, three)]), HashMap::new());
    let world = world_of(&references, citations);
    let model = Recording::new(window);

    read_one(Reading::Consistency, &model, &world, FEEDS).await?;

    let asked = model.asked.lock().unwrap();
    let shown: usize = asked
        .iter()
        .map(|user| user.matches("[passage-").count())
        .sum();
    assert_eq!(shown, 3, "{asked:?}");
    Ok(())
}
