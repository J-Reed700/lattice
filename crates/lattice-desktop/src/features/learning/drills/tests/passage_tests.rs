//! An explanation of a passage as a drill's material: the passage it is taken from, the explanation and its review
//! against the source, what the planter and the reviewers are told, the proof of a planted error by the source, the
//! check of a learner's fix, the adapter over the claim checker, the rows a passage drill is stored in and the bank's
//! shelf for each course. The model, the source check and the grader are scripted.

use super::support::{
    contradicted, course, explanation, explanation_answer, explanation_files, figure_mutation,
    fix_answer, idle_grader, last, passage_baseline, passage_build, passage_code,
    passage_plant_answer, passage_spec, quiet, reversed_mutation, spacing_check, spacing_passage,
    supported, system, unsupported, unsure, FakeGrader, ScriptedCheck, ScriptedModel, CHECK_MODEL,
    CONTRADICTION, FIGURE, FIGURE_QUOTE, FIRST, FIRST_PLANTED, FOURTH, MODEL, SECOND, SLEEP,
    SMALLER_QUOTE, SPACING, THIRD, THIRD_PLANTED,
};
use crate::features::learning::drills::bank::{self, take, take_from, Shelf};
use crate::features::learning::drills::build::{
    CleanCode, CleanReview, Evidence, Mutation, Planted, Proved, ProvenMutation, SourcePassage,
    Trail, Verification,
};
use crate::features::learning::drills::fix_grading::{grade_fix, FixCase};
use crate::features::learning::drills::jobs::spec_of;
use crate::features::learning::drills::material::passage::{SourceFinding, SourceSays};
use crate::features::learning::drills::material::{
    ClaimSourceCheck, DrillMaterial, JudgeWording, PassageMaterial, SourceCheck,
};
use crate::features::learning::drills::proof::Tools;
use crate::features::learning::drills::repository::{self, NewDrill};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::persistence::{db, encode};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use crate::features::learning::tests::pool;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, SamplingOverride,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    apply, check_key, targets, Assessment, DecidedBy, Drill, DrillFile, Edit, FindingOutcome,
    FixOutcome, IssueFix, IssueOutcome, KeyIssue, LineSpan, Material, Origin, Pattern, Proof,
    Severity, SourceVerdict,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use serde_json::json;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

/// The course's sources as stored: each version's id, title and whole text.
async fn stored_sources(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<(String, String, String)>> {
    Ok(LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(program_id)
        .await?
        .into_iter()
        .map(|source| (source.id, source.title, source.excerpt))
        .collect())
}

/// The version id and the text of the source of the course with `title`.
async fn source_titled(
    pool: &SqlitePool,
    program_id: &str,
    title: &str,
) -> Result<(String, String)> {
    stored_sources(pool, program_id)
        .await?
        .into_iter()
        .find(|(_, found, _)| found == title)
        .map(|(id, _, text)| (id, text))
        .ok_or_else(|| AppError::InternalError(format!("No source is titled {title}.")))
}

fn targets_of(spec: &DrillSpec) -> Vec<&'static Pattern> {
    spec.targets().expect("the patterns are in the catalogue")
}

/// A planted error proven by the source, at line 1.
fn sourced_issue() -> KeyIssue {
    let figure = figure_mutation();
    KeyIssue {
        id: "planted-1".into(),
        pattern: FIGURE.into(),
        severity: Severity::Major,
        span: LineSpan {
            path: "explanation.md".into(),
            start: 1,
            end: 1,
        },
        consequence: figure.consequence.clone(),
        fix: figure.fix(),
        explanation: figure.explanation,
        proof: Proof::Sourced {
            quote: FIGURE_QUOTE.into(),
            planted: SourceVerdict::Contradicted,
            reason: "The source gives 14%, not 40%.".into(),
        },
        origin: Origin::Planted,
    }
}

/// The explanation with the figure planted, and its key.
fn planted_drill(id: &str) -> Drill {
    Drill {
        id: id.into(),
        material: Material::Passage,
        title: "Why spacing study sessions helps".into(),
        files: vec![DrillFile {
            path: "explanation.md".into(),
            text: explanation().replace(FIRST, FIRST_PLANTED),
        }],
        key: vec![sourced_issue()],
    }
}

fn no_proofs() -> Proved {
    Proved {
        proven: Vec::new(),
        dropped: Vec::new(),
        files: explanation_files(),
        spans: Vec::new(),
    }
}

fn clean_review() -> CleanReview {
    CleanReview {
        files: explanation_files(),
        repairs: Vec::new(),
        trail: Trail::default(),
    }
}

/// Puts a ready drill on `source` of the course `program_id` into the bank.
async fn drilled(
    pool: &SqlitePool,
    id: &str,
    program_id: &str,
    source: &SourcePassage,
) -> Result<()> {
    let drill = planted_drill(id);
    let spec = passage_spec(program_id);
    let planted = Planted {
        mutations: Vec::new(),
        trail: Trail::default(),
    };
    let evidence = Evidence::new(
        &passage_code(),
        &clean_review(),
        &planted,
        &no_proofs(),
        &quiet(),
    );
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::insert_in(
        &mut connection,
        &NewDrill {
            drill: &drill,
            clean_files: &explanation_files(),
            spec: &spec,
            domain: &source.title,
            suite: None,
            source: Some(source),
            runner: None,
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await
}

async fn at(pool: &SqlitePool, id: &str, created_at: i64) -> Result<()> {
    sqlx::query("UPDATE learning_drills SET created_at=? WHERE id=?")
        .bind(created_at)
        .bind(id)
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(())
}

// The spec.

#[test]
fn a_passage_spec_names_its_course() {
    let spec = passage_spec("course");
    assert!(spec.validate().is_ok());
    for course in [None, Some(" ".to_owned())] {
        let without = DrillSpec {
            program_id: course,
            ..passage_spec("course")
        };
        assert!(
            matches!(without.validate(), Err(AppError::InvalidInput(_))),
            "{without:?}"
        );
    }
    let code_pattern = DrillSpec {
        patterns: vec!["correctness.wrong_operator".into()],
        ..passage_spec("course")
    };
    assert!(code_pattern.validate().is_err());
    let empty = DrillSpec {
        lines: 0,
        ..passage_spec("course")
    };
    assert!(
        matches!(empty.validate(), Err(AppError::InvalidInput(message)) if message.contains("sentence"))
    );
}

// The passage.

#[tokio::test]
async fn a_drill_explains_a_passage_of_the_course_s_first_source_sized_to_its_sentences(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (version, text) = source_titled(&pool, &program, "Spacing and memory").await?;
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let spec = passage_spec(&program);
    let code = passage_build(&pool, &model, &check, &grader)
        .clean_code(&spec)
        .await?;

    let source = code.source.expect("the passage");
    assert_eq!(source.version_id, version);
    assert_eq!(source.title, "Spacing and memory");
    assert_eq!(source.start_byte, 0);
    assert_eq!(
        text.get(source.start_byte..source.end_byte),
        Some(source.text.as_str()),
        "the passage is the exact bytes of the version"
    );
    assert!(source.text.contains(FIGURE_QUOTE));
    assert!(
        source.text.ends_with(SMALLER_QUOTE),
        "whole paragraphs: {}",
        source.text
    );
    assert!(
        !source.text.contains("Spacing works best"),
        "{}",
        source.text
    );

    assert_eq!(code.title, "Why spacing study sessions helps");
    assert_eq!(code.domain, "Spacing and memory");
    assert_eq!(code.files, explanation_files(), "one sentence to a line");
    assert_eq!(code.suite, None);
    assert!(code.tests.is_empty());
    assert_eq!(model.purposes(), ["clean explanation"]);
    let request = &model.requests()[0];
    let asked = last(request);
    assert!(asked.contains(&source.text), "{asked}");
    assert!(asked.contains("about 4 sentences"), "{asked}");
    assert!(check.asked().is_empty(), "nothing is checked while writing");
    Ok(())
}

/// The passage the clean stage of a drill on the course `program_id` takes.
async fn next_passage(pool: &SqlitePool, program_id: &str) -> Result<SourcePassage> {
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let code = passage_build(pool, &model, &check, &grader)
        .clean_code(&passage_spec(program_id))
        .await?;
    code.source
        .ok_or_else(|| AppError::InternalError("The clean stage took no passage.".into()))
}

#[tokio::test]
async fn the_next_drill_takes_the_least_drilled_source_and_never_a_drilled_passage() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (spacing, spacing_text) = source_titled(&pool, &program, "Spacing and memory").await?;
    let (sleep, sleep_text) = source_titled(&pool, &program, "Sleep and memory").await?;
    let first = next_passage(&pool, &program).await?;
    assert_eq!(first.version_id, spacing);
    drilled(&pool, "first", &program, &first).await?;

    let second = next_passage(&pool, &program).await?;
    assert_eq!(second.version_id, sleep, "the source drilled least");
    assert_eq!(
        second.text,
        sleep_text.trim(),
        "the whole of a short source"
    );
    drilled(&pool, "second", &program, &second).await?;

    let third = next_passage(&pool, &program).await?;
    assert_eq!(third.version_id, spacing);
    assert!(third.start_byte >= first.end_byte, "{third:?}");
    assert_eq!(
        spacing_text.get(third.start_byte..third.end_byte),
        Some(third.text.as_str())
    );
    assert!(
        third.text.starts_with("Spacing works best"),
        "{}",
        third.text
    );
    Ok(())
}

#[tokio::test]
async fn a_topic_narrows_the_passage_to_what_the_sources_say_about_it() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (sleep, _) = source_titled(&pool, &program, "Sleep and memory").await?;
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let spec = DrillSpec {
        domain: Some("sleep deprivation".into()),
        ..passage_spec(&program)
    };
    let code = passage_build(&pool, &model, &check, &grader)
        .clean_code(&spec)
        .await?;
    assert_eq!(code.source.map(|source| source.version_id), Some(sleep));
    assert_eq!(code.domain, "sleep deprivation");
    assert!(last(&model.requests()[0]).contains("Topic: sleep deprivation"));
    Ok(())
}

#[tokio::test]
async fn a_course_without_sources_gives_no_drill() -> Result<()> {
    let pool = pool().await?;
    let mut program = crate::features::learning::tests::fixture();
    program.sources = Vec::new();
    crate::features::learning::repository::LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let error = passage_build(&pool, &model, &check, &grader)
        .clean_code(&passage_spec(&program.summary.id))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("no source")),
        "{error:?}"
    );
    assert!(model.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn an_explanation_too_short_for_its_patterns_is_asked_for_again() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let short = json!({"title": "Spacing", "paragraphs": [FIRST]}).to_string();
    let model = ScriptedModel::new([short, explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let code = passage_build(&pool, &model, &check, &grader)
        .clean_code(&passage_spec(&program))
        .await?;
    assert_eq!(code.files, explanation_files());
    let again = last(&model.requests()[1]);
    assert!(
        again.contains("The explanation needs at least 3 sentences, and it has 1."),
        "{again}"
    );
    Ok(())
}

// The clean review.

#[tokio::test]
async fn every_sentence_the_source_supports_stays_and_nothing_is_asked_of_the_model() -> Result<()>
{
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = spacing_check();
    let grader = idle_grader();
    let reviewed = passage_build(&pool, &model, &check, &grader)
        .clean_review(&passage_spec("course"), &passage_code())
        .await?;
    assert_eq!(reviewed.files, explanation_files());
    assert!(reviewed.repairs.is_empty());
    assert_eq!(check.sentences(), [FIRST, SECOND, THIRD, FOURTH]);
    assert!(check
        .asked()
        .iter()
        .all(|(_, passage)| *passage == spacing_passage().text));
    assert_eq!(
        reviewed
            .trail
            .calls
            .iter()
            .map(|call| call.model.as_str())
            .collect::<Vec<_>>(),
        [CHECK_MODEL; 4]
    );
    assert!(model.requests().is_empty());
    Ok(())
}

const REWRITTEN: &str = "The benefit held for adults in laboratory settings.";
const STILL_WRONG: &str = "Each session should end with a test, the source says.";

fn rewriting_check() -> ScriptedCheck {
    ScriptedCheck::new(|sentence| match sentence {
        SECOND => unsupported("The source does not say the benefit was found, only that it held."),
        FOURTH => contradicted(
            "Spacing works best when each session ends with a test.",
            "Not in the passage.",
        ),
        STILL_WRONG => unsupported("The passage does not say this."),
        _ => supported(FIGURE_QUOTE),
    })
}

#[tokio::test]
async fn a_sentence_the_source_does_not_support_is_rewritten_once_and_dropped_when_still_unsupported(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        json!({"sentence": REWRITTEN}).to_string(),
        json!({"sentence": STILL_WRONG}).to_string(),
    ]);
    let check = rewriting_check();
    let grader = idle_grader();
    let spec = DrillSpec {
        patterns: vec![FIGURE.into()],
        ..passage_spec("course")
    };
    let reviewed = passage_build(&pool, &model, &check, &grader)
        .clean_review(&spec, &passage_code())
        .await?;
    assert_eq!(
        reviewed.files,
        vec![DrillFile {
            path: "explanation.md".into(),
            text: format!("{FIRST}\n{REWRITTEN}\n\n{THIRD}\n"),
        }]
    );
    assert_eq!(model.purposes(), ["rewrite", "rewrite"]);
    assert_eq!(
        check.sentences(),
        [FIRST, SECOND, REWRITTEN, THIRD, FOURTH, STILL_WRONG],
        "each rewrite is checked once"
    );
    let shown = last(&model.requests()[0]);
    assert!(shown.contains(&spacing_passage().text), "{shown}");
    assert!(shown.contains(SECOND), "{shown}");
    assert!(
        shown.contains("only that it held"),
        "the checker's reason: {shown}"
    );

    assert_eq!(reviewed.repairs.len(), 2);
    let kept = &reviewed.repairs[0];
    assert_eq!(kept.severity, Severity::Minor);
    assert_eq!(kept.edit.before, format!("{SECOND}\n"));
    assert_eq!(kept.edit.after, format!("{REWRITTEN}\n"));
    assert!(
        kept.description.starts_with("Sentence 2:"),
        "{}",
        kept.description
    );
    let dropped = &reviewed.repairs[1];
    assert_eq!(
        dropped.severity,
        Severity::Major,
        "the source contradicts it"
    );
    assert_eq!(dropped.edit.before, format!("{FOURTH}\n"));
    assert_eq!(dropped.edit.after, "");
    assert!(
        dropped.description.contains("dropped"),
        "{}",
        dropped.description
    );
    Ok(())
}

#[tokio::test]
async fn an_explanation_left_with_fewer_sentences_than_its_patterns_need_fails_the_build(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        json!({"sentence": STILL_WRONG}).to_string(),
        json!({"sentence": STILL_WRONG}).to_string(),
    ]);
    let check = rewriting_check();
    let grader = idle_grader();
    let error = passage_build(&pool, &model, &check, &grader)
        .clean_review(&passage_spec("course"), &passage_code())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidData(message) if message.contains("Only 2 sentences") && message.contains("at least 3")),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_source_check_that_fails_fails_the_review() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check =
        ScriptedCheck::new(|_| Err(AppError::ServiceNotAvailable("The checker is down.".into())));
    let grader = idle_grader();
    let error = passage_build(&pool, &model, &check, &grader)
        .clean_review(&passage_spec("course"), &passage_code())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ServiceNotAvailable(_)),
        "{error:?}"
    );
    Ok(())
}

// The plant.

#[test]
fn the_planter_is_shown_the_source_and_the_numbered_sentences() {
    let check = spacing_check();
    let spec = passage_spec("course");
    let targets = targets_of(&spec);
    let wording = PassageMaterial::new(&check).plant_wording(&spec, &passage_baseline(), &targets);
    assert!(!wording.asks_verification);
    assert!(wording
        .system
        .starts_with("You prepare a review exercise on an explanation of a source passage."));
    assert!(wording.system.contains("whole sentence lines"));
    let prompt = &wording.prompt;
    let passage = spacing_passage();
    for shown in [
        format!(
            "The source passage, from Spacing and memory:\n<passage>\n{}\n</passage>",
            passage.text
        ),
        format!("1 | {FIRST}\n2 | {SECOND}\n3 | \n4 | {THIRD}\n5 | {FOURTH}"),
        "The patterns, one change for each:\n".to_owned(),
    ] {
        assert!(prompt.contains(&shown), "{shown:?} in {prompt}");
    }
    for target in &targets {
        assert!(prompt.contains(&format!(
            "- {} ({}, family {}): {}\n",
            target.id,
            target.name,
            target.family.slug(),
            target.definition
        )));
    }
}

// The proof.

fn planted_of(mutations: Vec<Mutation>) -> Planted {
    Planted {
        mutations,
        trail: Trail::default(),
    }
}

#[tokio::test]
async fn a_planted_error_the_source_contradicts_is_proven_with_the_quote_and_raised_to_its_floor(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = spacing_check();
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation(), reversed_mutation()]),
        )
        .await?;
    assert!(proved.dropped.is_empty(), "{:?}", proved.dropped);
    assert_eq!(proved.proven.len(), 2);
    let figure = &proved.proven[0];
    assert_eq!(
        figure.proof,
        Proof::Sourced {
            quote: FIGURE_QUOTE.into(),
            planted: SourceVerdict::Contradicted,
            reason: "The source gives 14%, not 40%.".into(),
        }
    );
    assert_eq!(
        figure.mutation.severity,
        Severity::Major,
        "a contradicted figure is at least major"
    );
    assert_eq!(figure.mutation.edit, figure_mutation().edit);
    assert_eq!(proved.proven[1].mutation.severity, Severity::Major);
    assert_eq!(
        check.sentences(),
        [FIRST, FIRST_PLANTED, THIRD, THIRD_PLANTED],
        "the clean sentence, then the planted one"
    );
    assert_eq!(
        figure
            .trail
            .calls
            .iter()
            .map(|call| call.purpose.as_str())
            .collect::<Vec<_>>(),
        [
            "source check of the clean sentences",
            "source check of the planted sentences"
        ]
    );
    assert!(model.requests().is_empty(), "no judge is asked");
    Ok(())
}

#[tokio::test]
async fn a_mutation_whose_clean_sentence_the_source_does_not_support_is_dropped() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|sentence| {
        if sentence == FIRST {
            unsupported("The passage does not give the year of the study.")
        } else {
            contradicted(FIGURE_QUOTE, "The source gives 14%.")
        }
    });
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await?;
    assert!(proved.proven.is_empty());
    assert_eq!(proved.dropped.len(), 1);
    assert!(
        proved.dropped[0]
            .why
            .contains("does not support the sentences the change replaces"),
        "{}",
        proved.dropped[0].why
    );
    assert_eq!(
        check.sentences(),
        [FIRST],
        "the planted sentence is not asked about"
    );
    assert_eq!(proved.files, explanation_files());
    Ok(())
}

#[tokio::test]
async fn a_mutation_whose_planted_sentence_the_source_supports_is_dropped() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|_| supported(FIGURE_QUOTE));
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await?;
    assert!(proved.proven.is_empty());
    assert!(
        proved.dropped[0]
            .why
            .contains("supports the changed sentences too"),
        "{}",
        proved.dropped[0].why
    );
    assert_eq!(check.sentences(), [FIRST, FIRST_PLANTED]);
    Ok(())
}

#[tokio::test]
async fn a_source_check_with_no_verdict_is_an_error_and_never_a_proof() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = idle_grader();
    let failures: [fn() -> AppError; 2] = [
        || {
            AppError::ModelAnswerUnusable(
                "The source check gave no verdict that could be used.".into(),
            )
        },
        || AppError::ServiceNotAvailable("The checker is down.".into()),
    ];
    for failure in failures {
        let check = ScriptedCheck::new(move |_| Err(failure()));
        let proved = passage_build(&pool, &model, &check, &grader)
            .prove(
                &passage_spec("course"),
                &passage_baseline(),
                &planted_of(vec![figure_mutation()]),
            )
            .await;
        assert!(proved.is_err(), "{:?}: {proved:?}", failure());
        assert_eq!(
            check.sentences(),
            [FIRST],
            "nothing more is asked once a check fails"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_contradiction_within_the_explanation_is_judged_on_the_explanation() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([json!({
        "reason": "Sentence 4 now says the opposite of sentence 2.",
        "editIntroducesIt": true,
        "cleanLacksIt": true
    })
    .to_string()]);
    let check = spacing_check();
    let grader = idle_grader();
    let spec = DrillSpec {
        patterns: vec![CONTRADICTION.into()],
        ..passage_spec("course")
    };
    let contradiction = Mutation {
        pattern: CONTRADICTION.into(),
        edit: Edit {
            path: "explanation.md".into(),
            before: format!("{THIRD}\n"),
            after: "The benefit was found in children in classrooms.\n".into(),
        },
        severity: Severity::Minor,
        consequence: "A reader cannot tell who the benefit was found in.".into(),
        explanation: "Sentence 4 contradicts sentence 2 about who was studied.".into(),
        verification: Verification::NotAsked,
    };
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(&spec, &passage_baseline(), &planted_of(vec![contradiction]))
        .await?;
    assert_eq!(
        proved.proven[0].proof,
        Proof::Judged {
            reason: "Sentence 4 now says the opposite of sentence 2.".into()
        }
    );
    assert_eq!(
        proved.proven[0].mutation.severity,
        Severity::Minor,
        "no floor for a judged contradiction"
    );
    assert!(check.asked().is_empty(), "the source is not asked");
    assert_eq!(model.purposes(), ["judge"]);
    let shown = last(&model.requests()[0]);
    assert!(shown.contains("The original explanation:"), "{shown}");
    assert!(
        shown.contains("The explanation with the edit applied:"),
        "{shown}"
    );
    assert!(!shown.contains("Language:"), "{shown}");
    Ok(())
}

// The blind review.

fn proved_figure() -> Proved {
    let (files, spans) =
        apply(&explanation_files(), &[figure_mutation().edit]).expect("the edit applies");
    let mut mutation = figure_mutation();
    mutation.severity = Severity::Major;
    Proved {
        proven: vec![ProvenMutation {
            mutation,
            proof: sourced_issue().proof,
            trail: Trail::default(),
        }],
        dropped: Vec::new(),
        files,
        spans,
    }
}

#[tokio::test]
async fn the_blind_reviewer_reads_the_explanation_with_its_source_and_an_extra_it_finds_joins_the_key(
) -> Result<()> {
    let pool = pool().await?;
    let blind = json!({"findings": [
        {"path": "explanation.md", "quote": "improved recall by 40%", "comment": "The study found 14%, not 40%.", "severity": "major"},
        {"path": "explanation.md", "quote": SECOND, "comment": "The source says the benefit held for adults in the laboratory only.", "severity": "minor"}
    ]})
    .to_string();
    let entry = json!({
        "pattern": "scope.shifted_population",
        "severity": "minor",
        "consequence": "A reader would expect the same benefit outside the laboratory.",
        "fix": {
            "path": "explanation.md",
            "before": format!("{SECOND}\n"),
            "after": "The benefit held for adults in laboratory settings.\n"
        },
        "explanation": "The source found the benefit in laboratory settings only."
    })
    .to_string();
    let model = ScriptedModel::new([blind, entry]);
    let check = spacing_check();
    let grader = FakeGrader::new(Assessment {
        issues: vec![(
            IssueOutcome::Found { finding: 0 },
            "It names the figure.".into(),
        )],
        findings: vec![
            (
                FindingOutcome::Matches { issues: vec![0] },
                "It is the figure.".into(),
            ),
            (
                FindingOutcome::ValidExtra,
                "The source studied adults in the laboratory.".into(),
            ),
        ],
    });
    let proved = proved_figure();
    let reviewed = passage_build(&pool, &model, &check, &grader)
        .blind_review(&passage_spec("course"), &passage_baseline(), &proved)
        .await?;
    assert_eq!(model.purposes(), ["blind review", "key entry"]);

    let requests = model.requests();
    let reviewer = system(&requests[0]).to_lowercase();
    for word in ["plant", "mutat", "exercise", "key"] {
        assert!(!reviewer.contains(word), "{word}");
    }
    let source = format!(
        "The source passage, from Spacing and memory:\n<passage>\n{}\n</passage>",
        spacing_passage().text
    );
    let shown = last(&requests[0]);
    assert!(shown.contains(&source), "{shown}");
    assert!(
        shown.contains(FIRST_PLANTED),
        "the drill's explanation: {shown}"
    );
    let described = last(&requests[1]);
    assert!(described.contains(&source), "{described}");
    assert!(
        described.contains("The explanation:\n\n<file path=\"explanation.md\">"),
        "{described}"
    );
    assert!(
        described.contains("scope.shifted_population"),
        "the passage patterns: {described}"
    );
    assert!(
        !described.contains("correctness.wrong_operator"),
        "no code pattern: {described}"
    );

    assert_eq!(reviewed.extras.len(), 1);
    let extra = &reviewed.extras[0];
    assert_eq!(extra.origin, Origin::FoundInReview);
    assert_eq!(
        extra.proof,
        Proof::Judged {
            reason: "The source studied adults in the laboratory.".into()
        }
    );
    assert_eq!(extra.span.start, 2);
    Ok(())
}

// A whole build.

#[tokio::test]
async fn a_passage_drill_builds_end_to_end_and_is_stored_with_its_source() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (_, text) = source_titled(&pool, &program, "Spacing and memory").await?;
    let spec = passage_spec(&program);
    let job = bank::queue_build(&pool, &spec, "passage").await?;
    let store = JobStore::new(pool.clone());
    store
        .claim(&job)
        .await?
        .ok_or_else(|| AppError::InternalError("The job was not claimed.".into()))?;
    assert_eq!(
        store.get(&job).await?.subject_id,
        Some(format!("passage:{program}"))
    );
    let context = JobContext::detached(&store, &job).await?;
    let blind = json!({"findings": [
        {"path": "explanation.md", "quote": "improved recall by 40%", "comment": "The study found 14%, not 40%.", "severity": "major"}
    ]})
    .to_string();
    let model = ScriptedModel::new([explanation_answer(), passage_plant_answer(), blind]);
    let check = spacing_check();
    let grader = FakeGrader::new(Assessment {
        issues: vec![
            (
                IssueOutcome::Found { finding: 0 },
                "It names the figure.".into(),
            ),
            (IssueOutcome::Missed, "Nothing names the direction.".into()),
        ],
        findings: vec![(
            FindingOutcome::Matches { issues: vec![0] },
            "It is the figure.".into(),
        )],
    });
    let outcome = passage_build(&pool, &model, &check, &grader)
        .run(&context, &spec)
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    assert_eq!(
        model.purposes(),
        ["clean explanation", "plant", "blind review"]
    );
    assert_eq!(
        check.asked().len(),
        8,
        "four clean sentences, two for each proof"
    );

    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::oldest_ready_of_course_in(&mut connection, &program)
        .await?
        .expect("the drill is in the course's bank");
    drop(connection);
    assert_eq!(stored.drill.material, Material::Passage);
    assert_eq!(stored.spec, spec);
    assert_eq!(stored.program_id.as_deref(), Some(program.as_str()));
    assert_eq!(stored.suite, None);
    assert_eq!(stored.runner, None);
    assert_eq!(stored.clean_files, explanation_files());
    let source = stored.source.clone().expect("the source passage");
    assert_eq!(
        text.get(source.start_byte..source.end_byte),
        Some(source.text.as_str())
    );
    assert_eq!(
        stored.drill.files[0].text,
        explanation()
            .replace(FIRST, FIRST_PLANTED)
            .replace(THIRD, THIRD_PLANTED)
    );
    assert_eq!(stored.drill.key.len(), 2);
    let figure = &stored.drill.key[0];
    assert_eq!(figure.pattern, FIGURE);
    assert_eq!(
        figure.severity,
        Severity::Major,
        "raised from minor to its floor"
    );
    assert_eq!(figure.span.start, 1);
    assert!(matches!(
        &figure.proof,
        Proof::Sourced { quote, planted: SourceVerdict::Contradicted, .. } if source.text.contains(quote.as_str())
    ));
    assert_eq!(stored.drill.key[1].span.start, 4);
    assert!(check_key(&stored.clean_files, &stored.drill).is_ok());
    assert_eq!(stored.evidence.proofs.len(), 2);
    Ok(())
}

// The judge's wording.

#[test]
fn the_judge_of_a_passage_reads_an_explanation_in_sentences_with_its_source() -> Result<()> {
    let wording = JudgeWording::of(&Material::Passage)?;
    assert!(wording
        .system
        .starts_with("You grade a review of an explanation against an answer key."));
    assert!(wording
        .system
        .contains("Issues and findings are numbered from 1."));
    assert!(wording.system.contains("valid_extra"));
    assert_eq!(wording.line, "sentence");
    assert_eq!(wording.lines, "sentences");
    assert!(wording.source.is_some());
    assert!(!wording.system.contains("code"), "{}", wording.system);
    assert!(!wording.brief.contains("code"), "{}", wording.brief);
    Ok(())
}

// The fix.

/// What the learner's text makes of the planted figure.
async fn fix_of(
    model: &ScriptedModel,
    check: &ScriptedCheck,
    drill: &Drill,
    learner: &str,
) -> Result<IssueFix> {
    let cancel = CancellationToken::new();
    let tools = Tools {
        llm: model,
        runner: None,
        cancel: &cancel,
    };
    let learner = vec![DrillFile {
        path: "explanation.md".into(),
        text: learner.into(),
    }];
    let source = spacing_passage();
    let material = PassageMaterial::new(check);
    let mut graded = grade_fix(
        &material,
        tools,
        &FixCase {
            drill,
            learner: &learner,
            suite: None,
            source: Some(&source),
        },
    )
    .await?;
    Ok(graded.remove(0))
}

const ABOUT_TWENTY: &str = "Spreading study over several sessions improved recall by about 20% a week later in a 2006 study by Cepeda and colleagues.";
const ROUGHLY_FORTY: &str =
    "Spreading study over several sessions improved recall by roughly 40% a week later.";

fn fixing_check() -> ScriptedCheck {
    ScriptedCheck::new(|sentence| {
        if sentence.contains("20%") || sentence.contains("40%") {
            contradicted(FIGURE_QUOTE, "The source gives 14%.")
        } else {
            supported(FIGURE_QUOTE)
        }
    })
}

#[tokio::test]
async fn a_rewrite_the_source_supports_is_fixed() -> Result<()> {
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = fixing_check();
    let drill = planted_drill("drill");
    let fixed = fix_of(&model, &check, &drill, &explanation()).await?;
    assert_eq!(fixed.outcome, FixOutcome::Fixed);
    assert_eq!(fixed.decided_by, DecidedBy::Source);
    assert!(fixed.reason.contains(FIGURE_QUOTE), "{}", fixed.reason);
    assert_eq!(check.sentences(), [FIRST]);
    assert!(model.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn sentences_left_as_they_were_are_not_fixed_and_nothing_is_asked() -> Result<()> {
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = fixing_check();
    let drill = planted_drill("drill");
    let learner = drill.files[0]
        .text
        .replace(THIRD, "The benefit was smaller for what was known.");
    let fixed = fix_of(&model, &check, &drill, &learner).await?;
    assert_eq!(fixed.outcome, FixOutcome::NotFixed);
    assert_eq!(fixed.decided_by, DecidedBy::Source);
    assert!(check.asked().is_empty());
    assert!(model.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn removed_sentences_are_partly_a_fix() -> Result<()> {
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = fixing_check();
    let drill = planted_drill("drill");
    let learner = drill.files[0]
        .text
        .replace(&format!("{FIRST_PLANTED}\n"), "");
    let fixed = fix_of(&model, &check, &drill, &learner).await?;
    assert_eq!(fixed.outcome, FixOutcome::Partly);
    assert_eq!(fixed.decided_by, DecidedBy::Source);
    assert!(check.asked().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_rewrite_without_the_error_that_the_source_does_not_support_is_partly_a_fix() -> Result<()>
{
    let model = ScriptedModel::new([
        json!({"reason": "The 40% is gone; 20% is a new figure.", "errorGone": true}).to_string(),
    ]);
    let check = fixing_check();
    let drill = planted_drill("drill");
    let learner = drill.files[0].text.replace(FIRST_PLANTED, ABOUT_TWENTY);
    let fixed = fix_of(&model, &check, &drill, &learner).await?;
    assert_eq!(fixed.outcome, FixOutcome::Partly);
    assert_eq!(fixed.decided_by, DecidedBy::Judge);
    assert!(
        fixed.reason.contains("The source gives 14%."),
        "{}",
        fixed.reason
    );
    assert!(fixed.reason.contains("The 40% is gone"), "{}", fixed.reason);
    assert_eq!(check.sentences(), [ABOUT_TWENTY]);
    assert_eq!(model.purposes(), ["error gone"]);
    let shown = last(&model.requests()[0]);
    for part in [
        FIRST_PLANTED,
        FIRST,
        ABOUT_TWENTY,
        spacing_passage().text.as_str(),
    ] {
        assert!(shown.contains(part), "{part:?} in {shown}");
    }
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_still_states_the_error_is_not_fixed() -> Result<()> {
    let model = ScriptedModel::new([
        json!({"reason": "It still says 40%.", "errorGone": false}).to_string()
    ]);
    let check = fixing_check();
    let drill = planted_drill("drill");
    let learner = drill.files[0].text.replace(FIRST_PLANTED, ROUGHLY_FORTY);
    let fixed = fix_of(&model, &check, &drill, &learner).await?;
    assert_eq!(fixed.outcome, FixOutcome::NotFixed);
    assert_eq!(fixed.decided_by, DecidedBy::Judge);
    Ok(())
}

#[tokio::test]
async fn a_judged_issue_of_a_passage_is_judged_with_the_source_shown() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("fixed", "The contradiction is gone.")]);
    let check = fixing_check();
    let mut drill = planted_drill("drill");
    drill.key[0].proof = Proof::Judged {
        reason: "It contradicts sentence 2.".into(),
    };
    let fixed = fix_of(&model, &check, &drill, &explanation()).await?;
    assert_eq!(fixed.outcome, FixOutcome::Fixed);
    assert!(check.asked().is_empty());
    assert_eq!(model.purposes(), ["fix judge"]);
    let shown = last(&model.requests()[0]);
    assert!(shown.contains("The source passage:\n<passage>"), "{shown}");
    assert!(
        shown.contains("The explanation the learner reviewed:"),
        "{shown}"
    );
    Ok(())
}

// The adapter over the claim checker.

#[tokio::test]
async fn the_claim_checker_finds_a_sentence_supported_with_an_exact_quote_of_the_passage(
) -> Result<()> {
    let model = ScriptedModel::new([format!(
        "supported\nReason: The passage gives 14% a week later.\nSource quote: {FIGURE_QUOTE}"
    )]);
    let passage = spacing_passage();
    let found = ClaimSourceCheck::new(&model)
        .check(FIRST, &passage.text)
        .await?;
    assert_eq!(
        found,
        SourceFinding {
            says: SourceSays::Supported,
            quote: Some(FIGURE_QUOTE.into()),
            reason: "The passage gives 14% a week later.".into(),
            confident: true,
        }
    );
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].sampling,
        Some(SamplingOverride::deterministic())
    );
    let asked = last(&requests[0]);
    assert!(asked.contains(&passage.text), "{asked}");
    assert!(asked.contains(FIRST), "{asked}");
    assert_eq!(ClaimSourceCheck::new(&model).model_name(), MODEL);
    Ok(())
}

#[tokio::test]
async fn a_verdict_without_an_exact_quote_of_the_passage_is_an_error() {
    let model = ScriptedModel::new([
        "contradicted\nReason: The source gives 14%.\nSource quote: recalled 40% more".to_owned(),
    ]);
    let error = ClaimSourceCheck::new(&model)
        .check(FIRST_PLANTED, &spacing_passage().text)
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_check_with_no_verdict_is_an_error() {
    let model = ScriptedModel::new(["I cannot tell.".to_owned(), "Still not sure.".to_owned()]);
    let error = ClaimSourceCheck::new(&model)
        .check(FIRST, &spacing_passage().text)
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(
        model.requests().len(),
        2,
        "the checker asks once more for its format"
    );
}

/// A model that is not there.
struct Down;

#[async_trait]
impl LLMPort for Down {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err(AppError::ServiceNotAvailable("The model is down.".into()))
    }
    fn model_name(&self) -> &str {
        "down"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(false)
    }
}

#[tokio::test]
async fn a_check_whose_model_is_not_there_is_an_outage() {
    let error = ClaimSourceCheck::new(&Down)
        .check(FIRST, &spacing_passage().text)
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ServiceNotAvailable(_)),
        "{error:?}"
    );
}

// The rows.

#[tokio::test]
async fn a_passage_drill_comes_back_as_it_went_in() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let source = spacing_passage();
    drilled(&pool, "passage", &program, &source).await?;
    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::find_in(&mut connection, "passage")
        .await?
        .expect("the drill");
    drop(connection);
    assert_eq!(stored.drill, planted_drill("passage"));
    assert_eq!(stored.source, Some(source.clone()));
    assert_eq!(stored.spec, passage_spec(&program));
    assert_eq!(stored.program_id.as_deref(), Some(program.as_str()));
    let (material, language): (String, Option<String>) =
        sqlx::query_as("SELECT material,language FROM learning_drills WHERE id='passage'")
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(material, "passage");
    assert_eq!(language, None);
    assert_eq!(
        repository::drilled_passages(&pool, &program).await?,
        [source]
    );
    Ok(())
}

#[tokio::test]
async fn a_passage_drill_without_its_source_is_not_stored() -> Result<()> {
    let pool = pool().await?;
    let drill = planted_drill("passage");
    let spec = passage_spec("course");
    let planted = Planted {
        mutations: Vec::new(),
        trail: Trail::default(),
    };
    let evidence = Evidence::new(
        &passage_code(),
        &clean_review(),
        &planted,
        &no_proofs(),
        &quiet(),
    );
    let mut connection = pool.acquire().await.map_err(db)?;
    let error = repository::insert_in(
        &mut connection,
        &NewDrill {
            drill: &drill,
            clean_files: &explanation_files(),
            spec: &spec,
            domain: "Spacing and memory",
            suite: None,
            source: None,
            runner: None,
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::InvalidData(_)), "{error:?}");
    Ok(())
}

/// Inserts a row as the columns `material`, `language` and `source_json` give it, the rest as a code drill's.
async fn raw(
    pool: &SqlitePool,
    id: &str,
    material: &str,
    language: Option<&str>,
    source: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO learning_drills(id,material,language,source_json,title,domain,program_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES(?,?,?,?,'t','d',NULL,'{}','[]','[]','[]','{}',NULL,NULL,'m','ready',1,NULL)")
        .bind(id)
        .bind(material)
        .bind(language)
        .bind(source)
        .execute(pool)
        .await
        .map(drop)
}

#[tokio::test]
async fn the_table_holds_a_language_for_code_and_a_source_for_a_passage() -> Result<()> {
    let pool = pool().await?;
    let null: i64 = sqlx::query_scalar(
        "SELECT \"notnull\" FROM pragma_table_info('learning_drills') WHERE name='language'",
    )
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(null, 0, "a passage has no language");
    let index: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='learning_drills_course_bank'",
    )
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(index, 1);
    assert!(raw(&pool, "code", "code", Some("python"), None)
        .await
        .is_ok());
    assert!(raw(&pool, "passage", "passage", None, Some("{}"))
        .await
        .is_ok());
    for (id, material, language, source) in [
        ("code without a language", "code", None, None),
        ("code with a source", "code", Some("python"), Some("{}")),
        (
            "a passage with a language",
            "passage",
            Some("python"),
            Some("{}"),
        ),
        ("a passage without its source", "passage", None, None),
        (
            "a passage with a source that is not JSON",
            "passage",
            None,
            Some("not json"),
        ),
        ("something else", "poem", None, None),
    ] {
        assert!(
            raw(&pool, id, material, language, source).await.is_err(),
            "{id}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_code_drill_stored_before_passages_still_reads() -> Result<()> {
    use super::support::{code, files, planted, proved, review, OPERATOR};
    let pool = pool().await?;
    let spec = json!({"language": "python", "domain": "rate limiting", "lines": 40, "patterns": [OPERATOR], "programId": null});
    let evidence = Evidence::new(&code(), &review(), &planted(), &proved(), &quiet());
    sqlx::query("INSERT INTO learning_drills(id,language,title,domain,program_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES('old','python','A limiter','rate limiting',NULL,?,?,?,?,?,NULL,NULL,'m','ready',1,NULL)")
        .bind(spec.to_string())
        .bind(encode(&files())?)
        .bind(encode(&proved().files)?)
        .bind(encode(&proved().planted_key())?)
        .bind(encode(&evidence)?)
        .execute(&pool)
        .await
        .map_err(db)?;
    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::find_in(&mut connection, "old")
        .await?
        .expect("the old drill");
    assert_eq!(stored.drill.material, Material::code("python"));
    assert_eq!(stored.spec.material, Material::code("python"));
    assert_eq!(stored.source, None);
    let ready = repository::oldest_ready_in(&mut connection, "python").await?;
    assert_eq!(ready.map(|ready| ready.drill.id), Some("old".to_owned()));
    Ok(())
}

// The checkpoints.

#[test]
fn a_clean_stage_saved_before_passages_still_loads() {
    use super::support::code;
    let mut saved = serde_json::to_value(code()).unwrap();
    let removed = saved.as_object_mut().unwrap().remove("source");
    assert_eq!(removed, Some(serde_json::Value::Null));
    let read: CleanCode = serde_json::from_value(saved).unwrap();
    assert_eq!(read, code());

    let passage = serde_json::to_value(passage_code()).unwrap();
    assert_eq!(passage["source"]["versionId"], "spacing-v1");
    let read: CleanCode = serde_json::from_value(passage).unwrap();
    assert_eq!(read, passage_code());
}

// The bank.

#[tokio::test]
async fn the_bank_keeps_passage_drills_on_a_shelf_for_each_course() -> Result<()> {
    let pool = pool().await?;
    let first = course(&pool).await?;
    let second = course(&pool).await?;
    let source = spacing_passage();
    drilled(&pool, "first-new", &first, &source).await?;
    drilled(&pool, "first-old", &first, &source).await?;
    drilled(&pool, "second", &second, &source).await?;
    at(&pool, "first-new", 20).await?;
    at(&pool, "first-old", 10).await?;

    assert_eq!(
        take(&pool, "python").await?,
        None,
        "no passage is on a language's shelf"
    );
    let taken = take_from(&pool, Shelf::Course(&first))
        .await?
        .expect("a ready drill of the first course");
    assert_eq!(taken.drill.drill.id, "first-old");
    let job = JobStore::new(pool.clone()).get(&taken.next_build).await?;
    assert_eq!(job.subject_id, Some(format!("passage:{first}")));
    let queued = spec_of(&job)?;
    let expected: Vec<String> = targets(&Material::Passage, &[], 0, 2)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert_eq!(
        queued,
        DrillSpec {
            material: Material::Passage,
            domain: None,
            lines: 4,
            patterns: expected,
            program_id: Some(first.clone()),
        }
    );

    let taken = take_from(&pool, Shelf::Course(&second))
        .await?
        .expect("a ready drill of the second course");
    assert_eq!(taken.drill.drill.id, "second");
    assert_eq!(take_from(&pool, Shelf::Course(&second)).await?, None);
    let again = take_from(&pool, Shelf::Course(&first))
        .await?
        .expect("the other drill of the first course");
    assert_eq!(again.drill.drill.id, "first-new");
    Ok(())
}

#[tokio::test]
async fn a_build_under_way_for_a_course_is_found_again_and_another_course_gets_its_own(
) -> Result<()> {
    let pool = pool().await?;
    let first = bank::ensure_build(&pool, &passage_spec("one")).await?;
    assert_eq!(
        bank::ensure_build(&pool, &passage_spec("one")).await?,
        first
    );
    let other = bank::ensure_build(&pool, &passage_spec("two")).await?;
    assert_ne!(other, first);
    assert_eq!(Shelf::of(&passage_spec("one"))?, Shelf::Course("one"));
    assert_eq!(Shelf::Course("one").subject(), "passage:one");
    assert_eq!(Shelf::of(&super::support::spec())?.subject(), "python");
    let lost = DrillSpec {
        program_id: None,
        ..passage_spec("one")
    };
    assert!(Shelf::of(&lost).is_err());
    Ok(())
}

#[test]
fn the_explanation_texts_of_the_fixtures_agree() {
    assert_eq!(passage_code().files, explanation_files());
    assert!(SPACING.contains(FIGURE_QUOTE));
    assert!(SPACING.contains(SMALLER_QUOTE));
    assert!(spacing_passage().text.contains(SMALLER_QUOTE));
    assert!(!SLEEP.contains("spacing"));
}

// The checker's certainty: a finding the checker was not sure of proves nothing.

const UNSURE: &str =
    "The checker could not confidently establish a factual finding from these passages.";

#[tokio::test]
async fn a_planted_sentence_the_checker_is_unsure_of_drops_the_mutation() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|sentence| {
        if sentence == FIRST {
            supported(FIGURE_QUOTE)
        } else {
            unsure(SourceSays::Unsupported, UNSURE)
        }
    });
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await?;
    assert!(proved.proven.is_empty(), "{:?}", proved.proven);
    assert_eq!(proved.dropped.len(), 1);
    assert!(
        proved.dropped[0].why.contains("not sure"),
        "{}",
        proved.dropped[0].why
    );
    assert_eq!(check.sentences(), [FIRST, FIRST_PLANTED]);
    Ok(())
}

#[tokio::test]
async fn a_clean_sentence_the_checker_is_unsure_of_drops_the_mutation_before_the_planted_one_is_asked(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|sentence| {
        if sentence == FIRST {
            unsure(SourceSays::Supported, UNSURE)
        } else {
            contradicted(FIGURE_QUOTE, "The source gives 14%.")
        }
    });
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await?;
    assert!(proved.proven.is_empty());
    assert_eq!(check.sentences(), [FIRST]);
    Ok(())
}

#[tokio::test]
async fn a_planted_sentence_the_source_confidently_does_not_support_proves_the_error() -> Result<()>
{
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|sentence| {
        if sentence == FIRST {
            supported(FIGURE_QUOTE)
        } else {
            unsupported("The passage gives no figure of 40%.")
        }
    });
    let grader = idle_grader();
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await?;
    assert!(proved.dropped.is_empty(), "{:?}", proved.dropped);
    assert_eq!(
        proved.proven[0].proof,
        Proof::Sourced {
            quote: FIGURE_QUOTE.into(),
            planted: SourceVerdict::Unsupported,
            reason: "The passage gives no figure of 40%.".into(),
        }
    );
    assert_eq!(
        proved.proven[0].mutation.severity,
        Severity::Minor,
        "an unsupported figure is at least minor"
    );
    Ok(())
}

#[tokio::test]
async fn a_clean_sentence_the_checker_is_unsure_of_is_rewritten_as_an_unsupported_one_is(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        json!({"sentence": REWRITTEN}).to_string(),
        json!({"sentence": STILL_WRONG}).to_string(),
    ]);
    let check = ScriptedCheck::new(|sentence| match sentence {
        SECOND => unsure(SourceSays::Unsupported, UNSURE),
        FOURTH => unsure(SourceSays::Supported, UNSURE),
        STILL_WRONG => unsure(SourceSays::Supported, UNSURE),
        _ => supported(FIGURE_QUOTE),
    });
    let grader = idle_grader();
    let spec = DrillSpec {
        patterns: vec![FIGURE.into()],
        ..passage_spec("course")
    };
    let reviewed = passage_build(&pool, &model, &check, &grader)
        .clean_review(&spec, &passage_code())
        .await?;
    assert_eq!(
        reviewed.files[0].text,
        format!("{FIRST}\n{REWRITTEN}\n\n{THIRD}\n"),
        "an unsure verdict, even of support, keeps nothing as it is"
    );
    assert_eq!(model.purposes(), ["rewrite", "rewrite"]);
    assert_eq!(reviewed.repairs.len(), 2);
    assert_eq!(
        reviewed.repairs[1].edit.after, "",
        "the unsure rewrite is dropped"
    );
    Ok(())
}

#[tokio::test]
async fn a_rewrite_the_checker_is_unsure_of_goes_to_the_judge_and_is_never_fixed_on_its_say_so(
) -> Result<()> {
    let drill = planted_drill("drill");
    for says in [SourceSays::Supported, SourceSays::Unsupported] {
        let check = ScriptedCheck::new(move |_| unsure(says, UNSURE));
        let model = ScriptedModel::new([
            json!({"reason": "The 40% is gone.", "errorGone": true}).to_string()
        ]);
        let fixed = fix_of(&model, &check, &drill, &explanation()).await?;
        assert_eq!(fixed.outcome, FixOutcome::Partly, "{says:?}");
        assert!(fixed.reason.contains("not sure"), "{}", fixed.reason);
        assert_eq!(model.purposes(), ["error gone"], "{says:?}");

        let check = ScriptedCheck::new(move |_| unsure(says, UNSURE));
        let model = ScriptedModel::new([
            json!({"reason": "It still says 40%.", "errorGone": false}).to_string(),
        ]);
        let fixed = fix_of(&model, &check, &drill, &explanation()).await?;
        assert_eq!(fixed.outcome, FixOutcome::NotFixed, "{says:?}");
        assert_eq!(model.purposes(), ["error gone"], "{says:?}");
    }
    Ok(())
}

/// A model that answers with `text` and reports `logprobs` for its first token.
struct Weighed {
    text: String,
    logprobs: Vec<(String, f32)>,
}

impl Weighed {
    fn new(text: &str, probabilities: &[(&str, f32)]) -> Self {
        Self {
            text: text.to_owned(),
            logprobs: probabilities
                .iter()
                .map(|(token, probability)| ((*token).to_owned(), probability.ln()))
                .collect(),
        }
    }
}

#[async_trait]
impl LLMPort for Weighed {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut response = CompletionResponse::from_text(self.text.clone());
        response.first_token_logprobs = Some(self.logprobs.clone());
        Ok(response)
    }
    fn model_name(&self) -> &str {
        "weighed"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn the_claim_checker_says_when_it_was_unsure() -> Result<()> {
    let passage = spacing_passage();
    let answer = "unsupported\nReason: The passage gives no figure of 40%.\nSource quote: none";
    let low = Weighed::new(answer, &[("unsupported", 0.6), ("supported", 0.4)]);
    let found = ClaimSourceCheck::new(&low)
        .check(FIRST_PLANTED, &passage.text)
        .await?;
    assert_eq!(found.says, SourceSays::Unsupported);
    assert!(!found.confident, "a low probability: {found:?}");

    let disagreeing = Weighed::new(answer, &[("supported", 0.95), ("unsupported", 0.05)]);
    let found = ClaimSourceCheck::new(&disagreeing)
        .check(FIRST_PLANTED, &passage.text)
        .await?;
    assert_eq!(found.says, SourceSays::Unsupported);
    assert!(
        !found.confident,
        "the first token and the text disagree, though the first token was likely: {found:?}"
    );

    let sure = Weighed::new(answer, &[("unsupported", 0.95), ("supported", 0.05)]);
    let found = ClaimSourceCheck::new(&sure)
        .check(FIRST_PLANTED, &passage.text)
        .await?;
    assert_eq!(found.says, SourceSays::Unsupported);
    assert!(found.confident, "{found:?}");
    assert_eq!(found.reason, "The passage gives no figure of 40%.");
    Ok(())
}
