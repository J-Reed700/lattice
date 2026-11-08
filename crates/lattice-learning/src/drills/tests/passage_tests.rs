//! An explanation of a passage as a drill's material: the passage it is taken from, the explanation and its review
//! against the source, what the planter and the reviewers are told, the proof of a planted error by the source, the
//! check of a learner's fix, the adapter over the claim checker, the rows a passage drill is stored in and the bank's
//! shelf for each course. The model, the source check and the grader are scripted.

use super::support::{
    contradicted, course, course_of, explanation, explanation_answer, explanation_files,
    figure_mutation, fix_answer, held_back, idle_grader, last, lesson_titled, lessons_of,
    no_verdict, passage_baseline, passage_build, passage_code, passage_lesson_spec,
    passage_plant_answer, passage_spec, prepare_lesson, quiet, raised_all, reversed_mutation,
    spacing_check, spacing_passage, supported, system, teaches_answer, unchecked, unsupported,
    unsure, FakeGrader, ScriptedCheck, ScriptedModel, CHECK_MODEL, CONTRADICTION, FIGURE,
    FIGURE_QUOTE, FIRST, FIRST_PLANTED, FOURTH, MODEL, NOT_TEACHES_REASON, REVERSED, SECOND, SLEEP,
    SLEEP_LESSON, SMALLER_QUOTE, SPACING, SPACING_LESSON, TEACHES_REASON, THIRD, THIRD_PLANTED,
};
use crate::drills::bank::{self, take, Shelf};
use crate::drills::build::{
    Baseline, CleanCode, CleanReview, Evidence, Mutation, Planted, Proved, ProvenMutation,
    SourcePassage, Trail, Verification,
};
use crate::drills::fix_grading::{grade_fix, FixCase};
use crate::drills::jobs::spec_of;
use crate::drills::material::passage::{finding_of, SourceFinding, SourceSays};
use crate::drills::material::{
    ClaimSourceCheck, DrillMaterial, JudgeWording, PassageMaterial, SourceCheck,
};
use crate::drills::proof::Tools;
use crate::drills::repository::{self, NewDrill};
use crate::drills::spec::DrillSpec;
use crate::persistence::{db, encode};
use crate::source_library::LearningSourceLibraryRepository;
use crate::tests::pool;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, SamplingOverride,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::{
    ClaimJudgment, ClaimVerdict, JudgeOutcome,
};
use lattice_core::domain::learning::drills::{
    apply, check_key, targets, Assessment, DecidedBy, Drill, DrillFile, Edit, FindingOutcome,
    FixOutcome, IssueFix, IssueOutcome, KeyIssue, LineSpan, Material, Origin, Pattern, Proof,
    Severity, SourceVerdict,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use lattice_core::shared::text::segmentation::sentences;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::mpsc;
use std::time::{Duration, Instant};
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
        lesson: None,
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

/// Puts a ready drill on `source`, built from the lesson `lesson_id` of the course `program_id`, into the bank.
pub(super) async fn drilled(
    pool: &SqlitePool,
    id: &str,
    program_id: &str,
    lesson_id: &str,
    source: &SourcePassage,
) -> Result<()> {
    let drill = planted_drill(id);
    let spec = DrillSpec {
        lesson_id: Some(lesson_id.to_owned()),
        ..passage_spec(program_id)
    };
    let planted = Planted {
        mutations: Vec::new(),
        trail: Trail::default(),
    };
    let evidence = Evidence::new(
        &passage_code(),
        &clean_review(),
        &planted,
        &no_proofs(),
        &unchecked(),
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
    let spec = DrillSpec {
        lesson_id: Some("lesson".into()),
        ..passage_spec("course")
    };
    assert!(spec.validate().is_ok());
    for course in [None, Some(" ".to_owned())] {
        let without = DrillSpec {
            program_id: course,
            ..spec.clone()
        };
        assert!(
            matches!(without.validate(), Err(AppError::InvalidInput(_))),
            "{without:?}"
        );
    }
    let code_pattern = DrillSpec {
        patterns: vec!["correctness.wrong_operator".into()],
        ..spec.clone()
    };
    assert!(code_pattern.validate().is_err());
    let empty = DrillSpec {
        lines: 0,
        ..spec.clone()
    };
    assert!(
        matches!(empty.validate(), Err(AppError::InvalidInput(message)) if message.contains("sentence"))
    );
}

// The passage.

#[tokio::test]
async fn a_drill_explains_the_passage_of_the_lessons_best_source_sized_to_its_sentences(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (version, text) = source_titled(&pool, &program, "Spacing and memory").await?;
    let model = ScriptedModel::new([teaches_answer(true), explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
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
    assert_eq!(code.domain, SPACING_LESSON);
    assert_eq!(code.brief, None, "a passage is not written from a brief");
    assert!(code.basis.is_some());
    assert_eq!(code.files, explanation_files(), "one sentence to a line");
    assert_contiguous(&code.files[0].text);
    assert_eq!(code.suite, None);
    assert!(code.tests.is_empty());
    assert_eq!(model.purposes(), ["passage check", "clean explanation"]);
    let request = &model.requests()[1];
    let asked = last(request);
    assert!(asked.contains(&source.text), "{asked}");
    assert!(asked.contains("about 4 sentences"), "{asked}");
    assert!(
        asked.contains("Topic: Spacing study over sessions"),
        "{asked}"
    );
    assert!(check.asked().is_empty(), "nothing is checked while writing");
    Ok(())
}

/// The clean stage of a drill built to `spec` with the model answering `answers`, and the model.
async fn clean_with(
    pool: &SqlitePool,
    spec: &DrillSpec,
    answers: Vec<String>,
) -> (ScriptedModel, Result<CleanCode>) {
    let model = ScriptedModel::new(answers);
    let check = spacing_check();
    let grader = idle_grader();
    let built = passage_build(pool, &model, &check, &grader)
        .clean_code(spec)
        .await;
    (model, built)
}

/// The passage the clean stage of a drill built to `spec` takes when the model answers `answers`.
async fn passage_with(
    pool: &SqlitePool,
    spec: &DrillSpec,
    answers: Vec<String>,
) -> Result<SourcePassage> {
    clean_with(pool, spec, answers)
        .await
        .1?
        .source
        .ok_or_else(|| AppError::InternalError("The clean stage took no passage.".into()))
}

/// The passage the clean stage of a drill built to `spec` takes when the passage check says yes to the first place.
async fn next_passage(pool: &SqlitePool, spec: &DrillSpec) -> Result<SourcePassage> {
    passage_with(pool, spec, vec![teaches_answer(true), explanation_answer()]).await
}

#[tokio::test]
async fn the_next_drill_of_a_lesson_takes_the_passage_drilled_longest_ago_and_never_the_same_twice_running(
) -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool, &[("Spacing and memory", SPACING)]).await?;
    let (_, text) = source_titled(&pool, &program, "Spacing and memory").await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let opening = text
        .find("\n\nSpacing works best")
        .and_then(|end| text.get(..end))
        .expect("the third paragraph starts with the words");
    let later = text
        .find("Spacing works best")
        .zip(text.find("\n\nGains were"))
        .and_then(|(start, end)| text.get(start..end))
        .expect("the third and fourth paragraphs");

    let first = next_passage(&pool, &spec).await?;
    assert_eq!(
        first.text, opening,
        "the first two paragraphs: four sentences"
    );
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    drilled(&pool, "first", &program, &lesson, &first).await?;

    let second = next_passage(&pool, &spec).await?;
    assert_eq!(
        second.text, later,
        "the paragraphs no drill touched: the third and the fourth"
    );
    assert!(second.start_byte >= first.end_byte, "{second:?}");
    assert_eq!(
        text.get(second.start_byte..second.end_byte),
        Some(second.text.as_str())
    );
    drilled(&pool, "second", &program, &lesson, &second).await?;

    let third = passage_with(&pool, &spec, vec![explanation_answer()]).await?;
    assert_eq!(
        third.text, opening,
        "every place is used, so the one used longest ago, and it was judged to teach the lesson before"
    );
    Ok(())
}

#[tokio::test]
async fn another_lesson_of_the_course_takes_the_passage_its_own_words_find() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let (sleep, sleep_text) = source_titled(&pool, &program, "Sleep and memory").await?;
    let model = ScriptedModel::new([teaches_answer(true), explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let spec = passage_lesson_spec(&pool, &program, SLEEP_LESSON).await?;
    let code = passage_build(&pool, &model, &check, &grader)
        .clean_code(&spec)
        .await?;
    let source = code.source.expect("the passage");
    assert_eq!(source.version_id, sleep);
    assert_eq!(source.text, sleep_text.trim());
    assert_eq!(code.domain, SLEEP_LESSON);
    assert_eq!(model.purposes(), ["passage check", "clean explanation"]);
    assert!(last(&model.requests()[1]).contains("Topic: Sleep after learning"));
    Ok(())
}

#[tokio::test]
async fn a_passage_drill_takes_the_paragraph_of_a_hit_its_lessons_words_are_in() -> Result<()> {
    let pool = pool().await?;
    let opening = "The committee met on a Tuesday to discuss a study. Members reviewed the agenda with care. Minutes were circulated to everyone afterwards. Nothing was decided about the catering.";
    let closing = "Spacing sessions apart helps recall. Learners who study across several sessions improve their recall. Spreading the work out lets them explain the concept. Short gaps between sessions help much less.";
    let text = [opening, closing].join("\n\n");
    assert!(text.len() < 1200, "{}", text.len());
    let program = course_of(&pool, &[("Meeting notes", text.as_str())]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let (version, _) = source_titled(&pool, &program, "Meeting notes").await?;

    let picked = next_passage(&pool, &spec).await?;
    assert_eq!(picked.text, closing);
    assert!(
        !picked.text.contains("Nothing was decided"),
        "{}",
        picked.text
    );
    assert_eq!(picked.version_id, version);
    Ok(())
}

#[tokio::test]
async fn a_hits_paragraph_is_taken_by_the_weight_of_the_words_it_holds_and_not_their_count(
) -> Result<()> {
    let pool = pool().await?;
    let common = "The spacing study sessions began in March. Staff booked the hall for each date. Tea was served at the break. Attendance was recorded by the clerk.";
    let rare = "Every recall of a stored concept left a trace. Researchers logged each trace with a timestamp. The logs were archived in the basement. Nobody opened the archive for years.";
    let text = [common, rare].join("\n\n");
    let others: Vec<(String, String)> = (0..5)
        .map(|number| {
            (
                format!("Site {number}"),
                format!("The spacing study sessions of site {number} were short. Volunteers wore blue badges. Coffee was free. Parking was limited."),
            )
        })
        .collect();
    let mut sources: Vec<(&str, &str)> = vec![("Meeting notes", text.as_str())];
    sources.extend(
        others
            .iter()
            .map(|(title, body)| (title.as_str(), body.as_str())),
    );
    let program = course_of(&pool, &sources).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let (version, _) = source_titled(&pool, &program, "Meeting notes").await?;

    let picked = next_passage(&pool, &spec).await?;
    assert_eq!(picked.version_id, version);
    assert_eq!(picked.text, rare);
    Ok(())
}

/// `count` distinct sentences of filler about a harbour, none holding a word of the query of [`SPACING_LESSON`] or
/// another word that begins with one.
fn filler_sentences(count: usize) -> Vec<String> {
    const TRADES: [&str; 10] = [
        "baker", "weaver", "cooper", "mason", "tanner", "potter", "sailor", "miller", "carter",
        "smith",
    ];
    const CARGO: [&str; 3] = ["salt", "barley", "timber"];
    (0..count)
        .map(|number| {
            format!(
                "At the harbour the {} number {number} loaded crates of {} onto the grey cart.",
                TRADES[number % TRADES.len()],
                CARGO[number % CARGO.len()],
            )
        })
        .collect()
}

/// A sentence holding the words of the query of [`SPACING_LESSON`]: `number` makes it distinct.
fn lesson_sentence(number: usize) -> String {
    format!("Spacing study over sessions helps recall of item {number}, and learners improve and explain several.")
}

/// A sentence of filler with one word of the query of [`SPACING_LESSON`] in it.
fn touched_sentence(number: usize) -> String {
    format!("At the harbour the baker number {number} told of recall onto the grey cart.")
}

/// `count` sentences on one line, those of `lesson` the lesson's own and those of `touched` holding one word of its
/// query, and the byte span of each in the line.
fn one_line_of(count: usize, lesson: &[usize], touched: &[usize]) -> (String, Vec<(usize, usize)>) {
    let mut line = String::new();
    let mut spans = Vec::new();
    for (number, filler) in filler_sentences(count).into_iter().enumerate() {
        let sentence = if lesson.contains(&number) {
            lesson_sentence(number)
        } else if touched.contains(&number) {
            touched_sentence(number)
        } else {
            filler
        };
        if !line.is_empty() {
            line.push(' ');
        }
        spans.push((line.len(), line.len() + sentence.len()));
        line.push_str(&sentence);
    }
    (line, spans)
}

/// The text from the start of sentence `first` to the end of sentence `last` of `spans`.
fn run_of<'a>(line: &'a str, spans: &[(usize, usize)], first: usize, last: usize) -> &'a str {
    let (start, _) = spans[first];
    let (_, end) = spans[last];
    &line[start..end]
}

/// A paragraph of `total` sentences: `lead` if there is one, then sentences taken from `filler`.
fn paragraph_of(
    lead: Option<String>,
    filler: &mut std::vec::IntoIter<String>,
    total: usize,
) -> String {
    let mut held: Vec<String> = lead.into_iter().collect();
    while held.len() < total {
        match filler.next() {
            Some(sentence) => held.push(sentence),
            None => break,
        }
    }
    held.join(" ")
}

#[tokio::test]
async fn the_unrelated_paragraph_beside_a_drilled_one_gives_way_to_a_lower_hit_about_the_lesson(
) -> Result<()> {
    let pool = pool().await?;
    let mut fill = filler_sentences(60).into_iter();
    let own = (0..4).map(lesson_sentence).collect::<Vec<_>>().join(" ");
    let unrelated = paragraph_of(None, &mut fill, 4);
    let about = paragraph_of(Some(touched_sentence(9)), &mut fill, 4);
    let tail = paragraph_of(None, &mut fill, 4);
    for paragraph in [&own, &unrelated, &about] {
        assert_eq!(sentences(paragraph).len(), 4, "{paragraph}");
    }
    let first_source = [own.as_str(), unrelated.as_str()].join("\n\n");
    let second_source = [about.as_str(), tail.as_str()].join("\n\n");
    assert!(
        first_source.len() < 1200 && second_source.len() < 1200,
        "one window each"
    );
    let plain: Vec<(String, String)> = (0..6)
        .map(|number| (format!("Plain {number}"), paragraph_of(None, &mut fill, 4)))
        .collect();
    let mut sources: Vec<(&str, &str)> = vec![
        ("Lesson notes", first_source.as_str()),
        ("Side notes", second_source.as_str()),
    ];
    sources.extend(
        plain
            .iter()
            .map(|(title, body)| (title.as_str(), body.as_str())),
    );
    let program = course_of(&pool, &sources).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let (side, _) = source_titled(&pool, &program, "Side notes").await?;

    let first = next_passage(&pool, &spec).await?;
    assert_eq!(first.text, own);
    drilled(&pool, "first", &program, &lesson, &first).await?;

    let second = next_passage(&pool, &spec).await?;
    assert_eq!(second.version_id, side);
    assert_eq!(second.text, about);
    assert!(!second.text.contains(&unrelated), "{}", second.text);
    Ok(())
}

#[tokio::test]
async fn of_the_places_the_hits_overlap_the_heaviest_comes_first_and_not_the_best_ranked_hits(
) -> Result<()> {
    let pool = pool().await?;
    let mut fill = filler_sentences(120).into_iter();
    let repeated = "The spacing log said spacing was kept, spacing was logged, spacing was seen and spacing was shown.";
    let two_terms =
        "At the harbour the baker number 1 told of spacing and recall onto the grey cart.";
    let rank_first = paragraph_of(Some(repeated.to_owned()), &mut fill, 4);
    let heavier = paragraph_of(Some(two_terms.to_owned()), &mut fill, 4);
    for paragraph in [&rank_first, &heavier] {
        assert_eq!(sentences(paragraph).len(), 4, "{paragraph}");
        assert!(paragraph.len() < 1200);
    }
    let common: Vec<(String, String)> = (0..5)
        .map(|number| {
            (
                format!("Common {number}"),
                paragraph_of(Some(touched_sentence(number)), &mut fill, 4),
            )
        })
        .collect();
    let plain: Vec<(String, String)> = (0..15)
        .map(|number| (format!("Plain {number}"), paragraph_of(None, &mut fill, 4)))
        .collect();
    let mut sources: Vec<(&str, &str)> = vec![
        ("Notes X", rank_first.as_str()),
        ("Notes Y", heavier.as_str()),
    ];
    sources.extend(
        common
            .iter()
            .chain(plain.iter())
            .map(|(title, body)| (title.as_str(), body.as_str())),
    );
    let program = course_of(&pool, &sources).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let (version, _) = source_titled(&pool, &program, "Notes Y").await?;

    let picked = next_passage(&pool, &spec).await?;
    assert_eq!(picked.version_id, version);
    assert_eq!(picked.text, heavier);
    assert!(!picked.text.contains("spacing log"), "{}", picked.text);
    Ok(())
}

#[tokio::test]
async fn a_long_paragraph_gives_a_run_of_as_many_sentences_as_the_explanation() -> Result<()> {
    let pool = pool().await?;
    let (line, spans) = one_line_of(30, &[12, 13, 14, 15], &[]);
    assert!(!line.contains('\n') && line.len() > 2000, "{}", line.len());
    assert_eq!(sentences(&line).len(), 30);
    let program = course_of(&pool, &[("Harbour log", line.as_str())]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;

    let picked = next_passage(&pool, &spec).await?;
    assert_eq!(picked.text, run_of(&line, &spans, 12, 15));
    assert_eq!(sentences(&picked.text).len(), 4);
    assert_eq!(
        line.get(picked.start_byte..picked.end_byte),
        Some(picked.text.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn the_last_run_of_a_long_paragraph_takes_the_sentences_left_over() -> Result<()> {
    let pool = pool().await?;
    let (line, spans) = one_line_of(30, &[26, 27, 28], &[]);
    assert!(!line.contains('\n') && line.len() > 2000, "{}", line.len());
    let program = course_of(&pool, &[("Harbour log", line.as_str())]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;

    let picked = next_passage(&pool, &spec).await?;
    assert_eq!(picked.text, run_of(&line, &spans, 24, 29));
    assert_eq!(sentences(&picked.text).len(), 6);
    assert_eq!(
        line.get(picked.start_byte..picked.end_byte),
        Some(picked.text.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn a_second_drill_of_a_long_paragraph_takes_another_run_of_it() -> Result<()> {
    let pool = pool().await?;
    let (line, spans) = one_line_of(30, &[12, 13, 14, 15], &[21]);
    assert!(!line.contains('\n') && line.len() > 2000, "{}", line.len());
    let program = course_of(&pool, &[("Harbour log", line.as_str())]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;

    let first = next_passage(&pool, &spec).await?;
    assert_eq!(first.text, run_of(&line, &spans, 12, 15));
    drilled(&pool, "first", &program, &lesson, &first).await?;

    let second = next_passage(&pool, &spec).await?;
    let held = sentences(&second.text).len();
    assert!((4..=7).contains(&held), "{held}: {second:?}");
    assert_eq!(
        line.get(second.start_byte..second.end_byte),
        Some(second.text.as_str())
    );
    assert!(
        second.end_byte <= first.start_byte || first.end_byte <= second.start_byte,
        "{first:?} {second:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_hard_wrapped_paragraph_is_cut_at_sentence_ends_into_runs_as_long_as_the_explanation(
) -> Result<()> {
    let pool = pool().await?;
    let mut words = String::new();
    for (number, filler) in filler_sentences(16).into_iter().enumerate() {
        let sentence = if (6..=9).contains(&number) {
            lesson_sentence(number)
        } else {
            filler
        };
        words.push_str(&sentence);
        words.push(' ');
    }
    let mut text = String::new();
    let mut width = 0;
    for word in words.split_whitespace() {
        if width + word.len() + 1 > 60 && width > 0 {
            text.push('\n');
            width = 0;
        } else if width > 0 {
            text.push(' ');
            width += 1;
        }
        text.push_str(word);
        width += word.len();
    }
    assert!(
        !text.contains("\n\n") && text.len() > 1200,
        "{}",
        text.len()
    );
    assert_eq!(text.matches('.').count(), 16);
    let program = course_of(&pool, &[("Harbour log", text.as_str())]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;

    let picked = next_passage(&pool, &spec).await?;
    assert!(picked.text.len() < text.trim().len(), "{picked:?}");
    let ends = picked.text.matches('.').count();
    assert!((4..=7).contains(&ends), "{ends} sentences: {}", picked.text);
    assert!(picked.text.ends_with('.'), "{}", picked.text);
    let before = text.get(..picked.start_byte).unwrap_or("x").trim();
    assert!(before.is_empty() || before.ends_with('.'), "{before}");
    assert_eq!(
        text.get(picked.start_byte..picked.end_byte),
        Some(picked.text.as_str())
    );
    Ok(())
}

/// The id of the active version of the source of the course `program_id` titled `title`.
async fn version_of(pool: &SqlitePool, program_id: &str, title: &str) -> Result<String> {
    Ok(source_titled(pool, program_id, title).await?.0)
}

#[tokio::test]
async fn a_prepared_lessons_passage_comes_only_from_the_sources_its_blocks_cite() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let spacing = version_of(&pool, &program, "Spacing and memory").await?;
    let sleep = version_of(&pool, &program, "Sleep and memory").await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    prepare_lesson(
        &pool,
        &lesson,
        &[("explanation", "Rest helps.", &[sleep.as_str()])],
    )
    .await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let source = next_passage(&pool, &spec).await?;
    assert_eq!(
        source.version_id, sleep,
        "the query ranks the spacing source first ({spacing}), the block cites the other"
    );
    Ok(())
}

#[tokio::test]
async fn a_block_citing_an_older_version_of_a_source_takes_the_passage_from_its_active_version(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let older = version_of(&pool, &program, "Sleep and memory").await?;
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) SELECT 'newer-sleep',program_id,source_id,2,title,publisher,requested_url,resolved_url,full_text,excerpt,'newer-sha',word_count,truncated,extraction_version,acquired_at+1 FROM learning_source_versions WHERE id=?")
        .bind(&older)
        .execute(&pool)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_source_library SET active_version_id='newer-sleep' WHERE active_version_id=?")
        .bind(&older)
        .execute(&pool)
        .await
        .map_err(db)?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    prepare_lesson(
        &pool,
        &lesson,
        &[("explanation", "Gaps help.", &[older.as_str()])],
    )
    .await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let source = next_passage(&pool, &spec).await?;
    assert_eq!(
        source.version_id, "newer-sleep",
        "the spacing source ranks first; the block cites an older version of the sleep source"
    );
    Ok(())
}

#[tokio::test]
async fn a_prepared_lesson_whose_blocks_cite_no_source_gives_no_drill_and_is_recorded_as_not_fitting(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    prepare_lesson(&pool, &lesson, &[("explanation", "Gaps help.", &[])]).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let error = passage_build(&pool, &model, &check, &grader)
        .clean_code(&spec)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if *message == no_teaching_passage(SPACING_LESSON)),
        "{error:?}"
    );
    assert!(model.requests().is_empty());
    assert_not_fitting(&pool, &program, &lesson).await
}

/// What the clean stage fails with when the course's sources hold no passage that teaches the lesson titled `lesson`.
fn no_teaching_passage(lesson: &str) -> String {
    format!("The course's sources hold no passage that teaches the lesson \"{lesson}\".")
}

/// The lesson is recorded as not fitting a passage drill, on the basis it has now.
async fn assert_not_fitting(pool: &SqlitePool, program: &str, lesson: &str) -> Result<()> {
    let recorded: Vec<(i64, String)> = sqlx::query_as(
        "SELECT fits,basis FROM learning_drill_lesson_fits WHERE program_id=? AND lesson_id=? AND material='passage'",
    )
    .bind(program)
    .bind(lesson)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    let mut connection = pool.acquire().await.map_err(db)?;
    let basis = crate::drills::schedules::fit_basis_in(&mut connection, program, lesson).await?;
    assert_eq!(recorded, [(0, basis)]);
    Ok(())
}

#[tokio::test]
async fn a_course_without_sources_gives_no_drill_and_records_the_lesson_as_not_fitting(
) -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool, &[]).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let model = ScriptedModel::new([explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let error = passage_build(&pool, &model, &check, &grader)
        .clean_code(&spec)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if *message == no_teaching_passage(SPACING_LESSON)),
        "{error:?}"
    );
    assert!(model.requests().is_empty());
    assert_not_fitting(&pool, &program, &lesson).await
}

#[tokio::test]
async fn a_passage_spec_without_a_lesson_or_with_one_of_another_course_fails_before_any_model_call(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let foreign = lesson_titled(&pool, &other, SPACING_LESSON).await?;
    let without = passage_spec(&program);
    let elsewhere = DrillSpec {
        lesson_id: Some(foreign),
        ..passage_spec(&program)
    };
    for spec in [without, elsewhere] {
        let model = ScriptedModel::new([explanation_answer()]);
        let check = spacing_check();
        let grader = idle_grader();
        let error = passage_build(&pool, &model, &check, &grader)
            .clean_code(&spec)
            .await
            .unwrap_err();
        assert!(matches!(&error, AppError::InvalidInput(_)), "{error:?}");
        assert!(model.requests().is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn an_explanation_too_short_for_its_patterns_is_asked_for_again() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let short = json!({"title": "Spacing", "paragraphs": [FIRST]}).to_string();
    let model = ScriptedModel::new([teaches_answer(true), short, explanation_answer()]);
    let check = spacing_check();
    let grader = idle_grader();
    let code = passage_build(&pool, &model, &check, &grader)
        .clean_code(&passage_lesson_spec(&pool, &program, SPACING_LESSON).await?)
        .await?;
    assert_eq!(code.files, explanation_files());
    assert_eq!(
        model.purposes(),
        ["passage check", "clean explanation", "clean explanation"]
    );
    let again = last(&model.requests()[2]);
    assert!(
        again.contains("The explanation needs at least 3 sentences, and it has 1."),
        "{again}"
    );
    Ok(())
}

/// What a lesson's passage check is told of it: its module's title, its title and its objective.
async fn lesson_facts(pool: &SqlitePool, lesson_id: &str) -> Result<(String, String, String)> {
    sqlx::query_as(
        "SELECT m.title,l.title,l.objective FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.id=?",
    )
    .bind(lesson_id)
    .fetch_one(pool)
    .await
    .map_err(db)
}

/// The message the passage check is sent for `passage` of the source titled `source_title`.
fn check_message(facts: &(String, String, String), source_title: &str, passage: &str) -> String {
    format!(
        "Module: {}\nLesson: {}\nObjective: {}\n\nSource: {}\nThe passage:\n<passage>\n{}\n</passage>",
        facts.0, facts.1, facts.2, source_title, passage
    )
}

/// The schema of the passage check's answer.
fn check_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["reason", "passageTeachesTheLesson"],
        "properties": {
            "reason": {"type": "string", "minLength": 1},
            "passageTeachesTheLesson": {"type": "boolean"}
        }
    })
}

/// The start and end byte of `part` in `text`.
fn span_in(text: &str, part: &str) -> (i64, i64) {
    let start = text.find(part).expect("the part is in the text");
    (
        i64::try_from(start).expect("a start"),
        i64::try_from(start + part.len()).expect("an end"),
    )
}

/// The verdicts stored for the lesson `lesson_id`, in text order: the start, the end and `teaches` of each.
async fn verdicts(pool: &SqlitePool, lesson_id: &str) -> Result<Vec<(i64, i64, i64)>> {
    sqlx::query_as(
        "SELECT start_byte,end_byte,teaches FROM learning_drill_passage_checks WHERE lesson_id=? ORDER BY start_byte",
    )
    .bind(lesson_id)
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// A course of one source, "Harbour notes", whose two paragraphs of four sentences both hold words of the query of
/// [`SPACING_LESSON`]: the lighter holds one of them and comes first in the text, the heavier holds all of them. The
/// pick takes the heavier first and the lighter second. They are the only two places of the course.
struct TwoPlaces {
    program: String,
    lesson: String,
    spec: DrillSpec,
    lighter: String,
    heavier: String,
    text: String,
}

async fn two_places(pool: &SqlitePool) -> Result<TwoPlaces> {
    let mut fill = filler_sentences(20).into_iter();
    let lighter = paragraph_of(Some(touched_sentence(9)), &mut fill, 4);
    let heavier = (0..4).map(lesson_sentence).collect::<Vec<_>>().join(" ");
    for paragraph in [&lighter, &heavier] {
        assert_eq!(sentences(paragraph).len(), 4, "{paragraph}");
    }
    let text = [lighter.as_str(), heavier.as_str()].join("\n\n");
    assert!(text.len() < 1200, "{}", text.len());
    let program = course_of(pool, &[("Harbour notes", text.as_str())]).await?;
    Ok(TwoPlaces {
        lesson: lesson_titled(pool, &program, SPACING_LESSON).await?,
        spec: passage_lesson_spec(pool, &program, SPACING_LESSON).await?,
        program,
        lighter,
        heavier,
        text,
    })
}

#[tokio::test]
async fn before_the_explanation_the_passage_is_asked_whether_it_teaches_the_lesson() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let facts = lesson_facts(&pool, &lesson).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let (model, built) = clean_with(
        &pool,
        &spec,
        vec![teaches_answer(true), explanation_answer()],
    )
    .await;
    let source = built?.source.expect("the passage");

    assert_eq!(model.purposes(), ["passage check", "clean explanation"]);
    let request = &model.requests()[0];
    assert_eq!(
        last(request),
        check_message(&facts, "Spacing and memory", &source.text)
    );
    assert_eq!(request.json_schema, Some(check_schema()));
    assert!(
        system(request).starts_with("You decide whether a passage from a course's sources"),
        "{}",
        system(request)
    );
    let rows: Vec<(i64, i64, String, i64, String, String)> = sqlx::query_as(
        "SELECT start_byte,end_byte,basis,teaches,reason,model FROM learning_drill_passage_checks WHERE program_id=? AND lesson_id=?",
    )
    .bind(&program)
    .bind(&lesson)
    .fetch_all(&pool)
    .await
    .map_err(db)?;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (start, end, basis, teaches, reason, stored_model) = &rows[0];
    assert_eq!(
        (*start, *end),
        (
            i64::try_from(source.start_byte).expect("a start"),
            i64::try_from(source.end_byte).expect("an end")
        )
    );
    assert_eq!(*teaches, 1);
    assert!(!basis.trim().is_empty());
    assert_eq!(reason, TEACHES_REASON);
    assert_eq!(stored_model, MODEL);
    Ok(())
}

#[tokio::test]
async fn a_place_that_does_not_teach_the_lesson_gives_way_to_the_next_place_in_the_picks_order(
) -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let facts = lesson_facts(&pool, &places.lesson).await?;
    let (model, built) = clean_with(
        &pool,
        &places.spec,
        vec![
            teaches_answer(false),
            teaches_answer(true),
            explanation_answer(),
        ],
    )
    .await;
    let code = built?;

    assert_eq!(
        code.source.clone().expect("the passage").text,
        places.lighter
    );
    assert_eq!(
        model.purposes(),
        ["passage check", "passage check", "clean explanation"]
    );
    let requests = model.requests();
    assert_eq!(
        last(&requests[0]),
        check_message(&facts, "Harbour notes", &places.heavier),
        "the heavier place comes first, though it is later in the text"
    );
    assert_eq!(
        last(&requests[1]),
        check_message(&facts, "Harbour notes", &places.lighter)
    );
    let calls: Vec<(&str, &str)> = code
        .trail
        .calls
        .iter()
        .map(|call| (call.purpose.as_str(), call.model.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("passage check", MODEL),
            ("passage check", MODEL),
            ("clean explanation", MODEL)
        ]
    );
    let (lighter, heavier) = (
        span_in(&places.text, &places.lighter),
        span_in(&places.text, &places.heavier),
    );
    assert_eq!(
        verdicts(&pool, &places.lesson).await?,
        [(lighter.0, lighter.1, 1), (heavier.0, heavier.1, 0)]
    );
    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT reason FROM learning_drill_passage_checks WHERE lesson_id=? ORDER BY teaches",
    )
    .bind(&places.lesson)
    .fetch_all(&pool)
    .await
    .map_err(db)?;
    assert_eq!(reasons, [NOT_TEACHES_REASON, TEACHES_REASON]);
    Ok(())
}

#[tokio::test]
async fn a_place_judged_before_for_the_lesson_is_not_asked_about_again() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (_, first) = clean_with(
        &pool,
        &places.spec,
        vec![
            teaches_answer(false),
            teaches_answer(true),
            explanation_answer(),
        ],
    )
    .await;
    first?;

    let (model, second) = clean_with(&pool, &places.spec, vec![explanation_answer()]).await;
    let source = second?.source.expect("the passage");
    assert_eq!(source.text, places.lighter);
    assert_eq!(model.purposes(), ["clean explanation"]);
    assert_eq!(verdicts(&pool, &places.lesson).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_verdict_made_on_another_basis_is_asked_again() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let facts = lesson_facts(&pool, &places.lesson).await?;
    let (_, first) = clean_with(
        &pool,
        &places.spec,
        vec![
            teaches_answer(false),
            teaches_answer(true),
            explanation_answer(),
        ],
    )
    .await;
    first?;
    sqlx::query("UPDATE learning_drill_passage_checks SET basis='stale'")
        .execute(&pool)
        .await
        .map_err(db)?;

    let (model, second) = clean_with(
        &pool,
        &places.spec,
        vec![teaches_answer(true), explanation_answer()],
    )
    .await;
    let source = second?.source.expect("the passage");
    assert_eq!(source.text, places.heavier);
    assert_eq!(model.purposes(), ["passage check", "clean explanation"]);
    assert_eq!(
        last(&model.requests()[0]),
        check_message(&facts, "Harbour notes", &places.heavier)
    );
    let (start, end) = span_in(&places.text, &places.heavier);
    let row: (i64, String) = sqlx::query_as(
        "SELECT teaches,basis FROM learning_drill_passage_checks WHERE lesson_id=? AND start_byte=? AND end_byte=?",
    )
    .bind(&places.lesson)
    .bind(start)
    .bind(end)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(row.0, 1, "the old row was replaced");
    assert_ne!(row.1, "stale");
    assert_eq!(verdicts(&pool, &places.lesson).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn changing_the_lessons_objective_asks_again() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (_, first) = clean_with(
        &pool,
        &places.spec,
        vec![
            teaches_answer(false),
            teaches_answer(true),
            explanation_answer(),
        ],
    )
    .await;
    first?;
    let (module, title, objective) = lesson_facts(&pool, &places.lesson).await?;
    let changed = format!("{objective}. Gaps matter most.");
    sqlx::query("UPDATE learning_lessons SET objective=? WHERE id=?")
        .bind(&changed)
        .bind(&places.lesson)
        .execute(&pool)
        .await
        .map_err(db)?;

    let (model, second) = clean_with(
        &pool,
        &places.spec,
        vec![teaches_answer(true), explanation_answer()],
    )
    .await;
    second?;
    assert_eq!(model.purposes(), ["passage check", "clean explanation"]);
    assert_eq!(
        last(&model.requests()[0]),
        check_message(&(module, title, changed), "Harbour notes", &places.heavier)
    );
    Ok(())
}

#[tokio::test]
async fn when_no_place_teaches_the_lesson_no_drill_is_built_and_the_lesson_is_recorded_as_not_fitting(
) -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (model, built) = clean_with(
        &pool,
        &places.spec,
        vec![teaches_answer(false), teaches_answer(false)],
    )
    .await;
    let error = built.unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(message) if *message == no_teaching_passage(SPACING_LESSON)),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["passage check", "passage check"]);
    assert_eq!(model.unused(), 0);
    let (lighter, heavier) = (
        span_in(&places.text, &places.lighter),
        span_in(&places.text, &places.heavier),
    );
    assert_eq!(
        verdicts(&pool, &places.lesson).await?,
        [(lighter.0, lighter.1, 0), (heavier.0, heavier.1, 0)]
    );
    assert_not_fitting(&pool, &places.program, &places.lesson).await
}

fn blank_reason_answer() -> String {
    json!({"reason": " ", "passageTeachesTheLesson": true}).to_string()
}

#[tokio::test]
async fn a_blank_reason_is_asked_for_again_once() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (model, built) = clean_with(
        &pool,
        &places.spec,
        vec![
            blank_reason_answer(),
            teaches_answer(true),
            explanation_answer(),
        ],
    )
    .await;
    let source = built?.source.expect("the passage");

    assert_eq!(source.text, places.heavier);
    assert_eq!(
        model.purposes(),
        ["passage check", "passage check", "clean explanation"]
    );
    let again = last(&model.requests()[1]);
    assert!(
        again.contains("The reason is empty. Give your reason first."),
        "{again}"
    );
    let (start, end) = span_in(&places.text, &places.heavier);
    assert_eq!(verdicts(&pool, &places.lesson).await?, [(start, end, 1)]);
    Ok(())
}

#[tokio::test]
async fn an_answer_refused_twice_fails_the_clean_stage_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (model, built) = clean_with(
        &pool,
        &places.spec,
        vec![blank_reason_answer(), blank_reason_answer()],
    )
    .await;
    let error = built.unwrap_err();

    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["passage check", "passage check"]);
    assert!(verdicts(&pool, &places.lesson).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_failing_passage_check_call_fails_the_clean_stage_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let places = two_places(&pool).await?;
    let (model, built) = clean_with(&pool, &places.spec, Vec::new()).await;
    let error = built.unwrap_err();

    assert!(
        matches!(&error, AppError::InternalError(message) if message.contains("Unexpected extra model call")),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["passage check"]);
    assert!(verdicts(&pool, &places.lesson).await?.is_empty());
    Ok(())
}

// The clean review.

/// One sentence to a line and no empty line, so that the sentences are numbered 1 to n.
fn assert_contiguous(text: &str) {
    assert!(text.ends_with('\n'), "{text:?}");
    assert!(
        text.lines().all(|line| !line.trim().is_empty()),
        "an empty line in {text:?}"
    );
}

#[tokio::test]
async fn a_blank_line_a_stored_explanation_has_is_not_kept_by_the_review() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = spacing_check();
    let grader = idle_grader();
    let mut code = passage_code();
    code.files[0].text = format!("{FIRST}\n{SECOND}\n\n{THIRD}\n{FOURTH}\n");
    let reviewed = passage_build(&pool, &model, &check, &grader)
        .clean_review(&passage_spec("course"), &code)
        .await?;
    assert_eq!(reviewed.files, explanation_files());
    assert_contiguous(&reviewed.files[0].text);
    Ok(())
}

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
            text: format!("{FIRST}\n{REWRITTEN}\n{THIRD}\n"),
        }]
    );
    assert_contiguous(&reviewed.files[0].text);
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
        format!("1 | {FIRST}\n2 | {SECOND}\n3 | {THIRD}\n4 | {FOURTH}"),
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
async fn a_source_check_whose_call_fails_is_an_error_and_never_a_proof() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = idle_grader();
    let check =
        ScriptedCheck::new(|_| Err(AppError::ServiceNotAvailable("The checker is down.".into())));
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation()]),
        )
        .await;
    assert!(
        matches!(proved, Err(AppError::ServiceNotAvailable(_))),
        "{proved:?}"
    );
    assert_eq!(
        check.sentences(),
        [FIRST],
        "nothing more is asked once a call fails"
    );
    Ok(())
}

#[tokio::test]
async fn a_planted_sentence_the_checker_gave_no_verdict_on_drops_only_that_mutation() -> Result<()>
{
    let pool = pool().await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = idle_grader();
    let check = ScriptedCheck::new(|sentence| {
        if sentence == FIRST_PLANTED {
            no_verdict()
        } else if sentence == THIRD_PLANTED {
            contradicted(SMALLER_QUOTE, "The source says smaller.")
        } else {
            supported(FIGURE_QUOTE)
        }
    });
    let proved = passage_build(&pool, &model, &check, &grader)
        .prove(
            &passage_spec("course"),
            &passage_baseline(),
            &planted_of(vec![figure_mutation(), reversed_mutation()]),
        )
        .await?;
    assert_eq!(proved.proven.len(), 1);
    assert_eq!(proved.proven[0].mutation.pattern, REVERSED);
    assert_eq!(proved.dropped.len(), 1);
    assert_eq!(proved.dropped[0].mutation.pattern, FIGURE);
    assert!(
        proved.dropped[0].why.contains("no usable verdict"),
        "{}",
        proved.dropped[0].why
    );
    assert!(
        proved.dropped[0]
            .trail
            .calls
            .iter()
            .any(|call| call.purpose.contains("no usable verdict")),
        "{:?}",
        proved.dropped[0].trail
    );
    Ok(())
}

#[tokio::test]
async fn a_clean_sentence_the_checker_gave_no_verdict_on_is_rewritten_and_the_build_goes_on(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        json!({"sentence": REWRITTEN}).to_string(),
        json!({"sentence": STILL_WRONG}).to_string(),
    ]);
    let check = ScriptedCheck::new(|sentence| match sentence {
        SECOND | FOURTH | STILL_WRONG => no_verdict(),
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
        explanation_text_of(&[FIRST, REWRITTEN, THIRD]),
        "rewritten when the rewrite is supported, dropped when it gets no verdict either"
    );
    assert_eq!(model.purposes(), ["rewrite", "rewrite"]);
    assert_eq!(reviewed.repairs.len(), 2);
    assert!(
        reviewed.repairs[0]
            .description
            .contains("no usable verdict"),
        "{}",
        reviewed.repairs[0].description
    );
    let unusable = reviewed
        .trail
        .calls
        .iter()
        .filter(|call| call.purpose.contains("no usable verdict"))
        .count();
    assert_eq!(unusable, 3, "{:?}", reviewed.trail);
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
        lesson_sentences: vec![],
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
        .blind_review(
            &passage_spec("course"),
            &passage_baseline(),
            &proved,
            &unchecked(),
        )
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
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let job = bank::queue_build(&pool, &spec, "passage").await?;
    let store = JobStore::new(pool.clone());
    store
        .claim(&job)
        .await?
        .ok_or_else(|| AppError::InternalError("The job was not claimed.".into()))?;
    assert_eq!(
        store.get(&job).await?.subject_id,
        Some(format!("drill:{program}:passage"))
    );
    let context = JobContext::detached(&store, &job).await?;
    let blind = json!({"findings": [
        {"path": "explanation.md", "quote": "improved recall by 40%", "comment": "The study found 14%, not 40%.", "severity": "major"}
    ]})
    .to_string();
    let model = ScriptedModel::new([
        teaches_answer(true),
        explanation_answer(),
        passage_plant_answer(),
        blind,
    ]);
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
        [
            "passage check",
            "clean explanation",
            "plant",
            "blind review"
        ]
    );
    assert_eq!(
        check.asked().len(),
        8,
        "four clean sentences, two for each proof"
    );

    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::oldest_ready_on(
        &mut connection,
        Shelf::of(&spec)?,
        spec.lesson_id.as_deref(),
    )
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
    assert_eq!(stored.drill.key[1].span.start, 3);
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
            assessment: &raised_all(drill),
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
async fn the_claim_checker_is_shown_the_passage_s_sentences_and_quotes_the_one_it_cites(
) -> Result<()> {
    let model = ScriptedModel::new([
        "Reason: The passage gives 14% a week later.\nSource passage: passage-0\nVerdict: supported"
            .to_owned(),
    ]);
    let passage = spacing_passage();
    let found = ClaimSourceCheck::new(&model)
        .check(FIRST, &passage.text)
        .await?;
    let first = "In a 2006 study by Cepeda and colleagues, learners who spread their study over several sessions recalled 14% more a week later than learners who studied in one session.";
    assert_eq!(
        found,
        SourceFinding {
            says: SourceSays::Supported,
            quote: Some(first.into()),
            reason: "The passage gives 14% a week later.".into(),
            confident: true,
            unusable: false,
        }
    );
    assert!(passage.text.contains(first));
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].sampling,
        Some(SamplingOverride::deterministic())
    );
    let asked = last(&requests[0]);
    for shown in [
        format!("[passage-0]\n{first}"),
        "[passage-1]\nAll of the learners were adults.".to_owned(),
        "[passage-2]\nThe benefit held in laboratory settings.".to_owned(),
        format!("[passage-3]\n{SMALLER_QUOTE}"),
    ] {
        assert!(asked.contains(&shown), "{shown:?} in {asked}");
    }
    assert!(!asked.contains("[passage-4]"), "{asked}");
    assert!(asked.contains(FIRST), "{asked}");
    assert_eq!(ClaimSourceCheck::new(&model).model_name(), MODEL);
    Ok(())
}

#[tokio::test]
async fn a_hard_wrapped_source_with_irregular_space_still_gives_exact_quotes() -> Result<()> {
    let passage = "Spacing study sessions\nhelped learners recall  14% more\r\na week later.   It was\n   smaller for material\nalready well known.\n\nA new paragraph.";
    let model = ScriptedModel::new([
        "Reason: The source says smaller.\nSource passage: passage-1\nVerdict: contradicted"
            .to_owned(),
    ]);
    let found = ClaimSourceCheck::new(&model)
        .check(THIRD_PLANTED, passage)
        .await?;
    let cited = "It was\n   smaller for material\nalready well known.";
    assert_eq!(found.says, SourceSays::Contradicted);
    assert_eq!(found.quote.as_deref(), Some(cited));
    assert!(
        passage.contains(cited),
        "the quote is the source's own bytes"
    );
    let asked = last(&model.requests()[0]);
    assert!(
        asked.contains("[passage-0]\nSpacing study sessions\nhelped learners recall  14% more\r\na week later."),
        "a wrapped line does not end a sentence: {asked}"
    );
    assert!(asked.contains("[passage-2]\nA new paragraph."), "{asked}");
    Ok(())
}

#[tokio::test]
async fn the_items_of_a_list_in_a_passage_go_to_the_checker_apart() -> Result<()> {
    let passage = "Study habits that help:\n- Space sessions apart\n- Test yourself often\n- Sleep after studying\n\nTogether they improve recall.";
    let model = ScriptedModel::new([
        "Reason: The list names it.\nSource passage: passage-2\nVerdict: supported".to_owned(),
    ]);
    let found = ClaimSourceCheck::new(&model)
        .check("Testing oneself often helps.", passage)
        .await?;
    assert_eq!(found.says, SourceSays::Supported);
    assert_eq!(found.quote.as_deref(), Some("- Test yourself often"));
    let asked = last(&model.requests()[0]);
    for shown in [
        "[passage-0]\nStudy habits that help:",
        "[passage-1]\n- Space sessions apart",
        "[passage-2]\n- Test yourself often",
        "[passage-3]\n- Sleep after studying",
        "[passage-4]\nTogether they improve recall.",
    ] {
        assert!(asked.contains(shown), "{shown:?} in {asked}");
    }
    assert!(!asked.contains("[passage-5]"), "{asked}");
    Ok(())
}

#[tokio::test]
async fn a_verdict_citing_no_sentence_of_the_passage_is_a_finding_of_nothing() -> Result<()> {
    let cites_nothing =
        "Reason: The source gives 14%.\nSource passage: passage-9\nVerdict: contradicted"
            .to_owned();
    let model = ScriptedModel::new([cites_nothing.clone(), cites_nothing]);
    let found = ClaimSourceCheck::new(&model)
        .check(FIRST_PLANTED, &spacing_passage().text)
        .await?;
    assert!(found.unusable, "{found:?}");
    assert!(!found.confident);
    assert_eq!(found.quote, None);
    assert_eq!(
        model.requests().len(),
        2,
        "the checker asks once more for its format"
    );
    Ok(())
}

#[tokio::test]
async fn a_check_with_no_verdict_is_a_finding_of_nothing_and_not_an_error() -> Result<()> {
    let model = ScriptedModel::new(["I cannot tell.".to_owned(), "Still not sure.".to_owned()]);
    let found = ClaimSourceCheck::new(&model)
        .check(FIRST, &spacing_passage().text)
        .await?;
    assert_eq!(found, no_verdict()?);
    assert_eq!(model.requests().len(), 2);
    Ok(())
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
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    drilled(&pool, "passage", &program, &lesson, &source).await?;
    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::find_in(&mut connection, "passage")
        .await?
        .expect("the drill");
    drop(connection);
    assert_eq!(stored.drill, planted_drill("passage"));
    assert_eq!(stored.source, Some(source.clone()));
    assert_eq!(
        stored.spec,
        DrillSpec {
            lesson_id: Some(lesson),
            ..passage_spec(&program)
        }
    );
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
        &unchecked(),
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
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='learning_drills_shelf'",
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
    let evidence = Evidence::new(
        &code(),
        &review(),
        &planted(),
        &proved(),
        &unchecked(),
        &quiet(),
    );
    sqlx::query("INSERT INTO learning_drills(id,language,title,domain,program_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES('old','python','A limiter','rate limiting',NULL,?,?,?,?,?,NULL,NULL,'m','ready',1,NULL)")
        .bind(spec.to_string())
        .bind(encode(&files())?)
        .bind(encode(&proved().files)?)
        .bind(encode(&proved().planted_key(&unchecked()))?)
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

    let mut saved = serde_json::to_value(code()).unwrap();
    let object = saved.as_object_mut().unwrap();
    object.remove("source");
    assert!(object.remove("brief").is_some());
    assert!(object.remove("basis").is_some());
    let read: CleanCode = serde_json::from_value(saved).unwrap();
    assert_eq!((read.brief, read.basis), (None, None));

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
    let first_lessons = lessons_of(&pool, &first).await?;
    let second_lessons = lessons_of(&pool, &second).await?;
    let source = spacing_passage();
    drilled(&pool, "first-new", &first, &first_lessons[0], &source).await?;
    drilled(&pool, "first-old", &first, &first_lessons[0], &source).await?;
    drilled(&pool, "second", &second, &second_lessons[0], &source).await?;
    at(&pool, "first-new", 20).await?;
    at(&pool, "first-old", 10).await?;
    let python = Material::code("python");
    let passages = Material::Passage;

    assert_eq!(
        take(
            &pool,
            Shelf {
                program_id: &first,
                material: &python
            },
            None
        )
        .await?,
        None,
        "no passage is on a language's shelf"
    );
    let taken = take(
        &pool,
        Shelf {
            program_id: &first,
            material: &passages,
        },
        None,
    )
    .await?
    .expect("a ready drill of the first course");
    assert_eq!(taken.drill.drill.id, "first-old");
    assert_eq!(taken.next_build, None, "first-new is ready");

    let taken = take(
        &pool,
        Shelf {
            program_id: &second,
            material: &passages,
        },
        None,
    )
    .await?
    .expect("a ready drill of the second course");
    assert_eq!(taken.drill.drill.id, "second");
    let next = taken.next_build.as_deref().expect("a build was queued");
    let job = JobStore::new(pool.clone()).get(next).await?;
    assert_eq!(job.subject_id, Some(format!("drill:{second}:passage")));
    let expected: Vec<String> = targets(&Material::Passage, &held_back(&taken.drill.drill), 0, 2)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert_eq!(
        spec_of(&job)?,
        DrillSpec {
            material: Material::Passage,
            lines: 4,
            patterns: expected,
            program_id: Some(second.clone()),
            lesson_id: Some(second_lessons[1].clone()),
        },
        "the lesson the drill was built from is pending, so the next is built from the one after"
    );
    assert_eq!(
        take(
            &pool,
            Shelf {
                program_id: &second,
                material: &passages
            },
            None
        )
        .await?,
        None
    );
    let again = take(
        &pool,
        Shelf {
            program_id: &first,
            material: &passages,
        },
        None,
    )
    .await?
    .expect("the other drill of the first course");
    assert_eq!(again.drill.drill.id, "first-new");
    assert!(
        again.next_build.is_some(),
        "the first course's shelf is empty now"
    );
    Ok(())
}

#[tokio::test]
async fn a_build_under_way_for_a_course_is_found_again_and_another_course_gets_its_own(
) -> Result<()> {
    let pool = pool().await?;
    let one = course(&pool).await?;
    let two = course(&pool).await?;
    let passages = Material::Passage;
    let on_one = Shelf {
        program_id: &one,
        material: &passages,
    };
    let on_two = Shelf {
        program_id: &two,
        material: &passages,
    };
    let first = bank::ensure_build(&pool, on_one, None, 4, 2).await?;
    assert_eq!(bank::ensure_build(&pool, on_one, None, 4, 2).await?, first);
    let other = bank::ensure_build(&pool, on_two, None, 4, 2).await?;
    assert_ne!(other, first);
    let store = JobStore::new(pool.clone());
    assert_eq!(
        store.get(&first).await?.subject_id,
        Some(format!("drill:{one}:passage"))
    );
    assert_eq!(
        store.get(&other).await?.subject_id,
        Some(format!("drill:{two}:passage"))
    );
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
        format!("{FIRST}\n{REWRITTEN}\n{THIRD}\n"),
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

fn outcome(verdict: ClaimVerdict, confidence: Option<f32>, uncertain: bool) -> ClaimJudgment {
    ClaimJudgment::Judged(JudgeOutcome {
        verdict,
        quote: None,
        reason: Some("The passage gives no figure of 40%.".into()),
        confidence,
        uncertain,
    })
}

#[test]
fn a_judgment_comes_to_a_finding_that_says_whether_the_checker_was_sure() {
    let sure = finding_of(outcome(ClaimVerdict::Unsupported, None, false)).unwrap();
    assert_eq!(sure.says, SourceSays::Unsupported);
    assert!(sure.confident && !sure.unusable, "{sure:?}");
    assert_eq!(sure.reason, "The passage gives no figure of 40%.");
    assert!(
        finding_of(outcome(ClaimVerdict::Unsupported, Some(0.95), false))
            .unwrap()
            .confident
    );

    let low = finding_of(outcome(ClaimVerdict::Unsupported, Some(0.6), false)).unwrap();
    assert!(!low.confident && !low.unusable, "{low:?}");
    let downgraded = finding_of(outcome(ClaimVerdict::Unsupported, Some(0.95), true)).unwrap();
    assert!(
        !downgraded.confident,
        "the checker recorded Unsupported for an answer it was unsure of: {downgraded:?}"
    );

    for nothing in [
        ClaimJudgment::Unusable,
        outcome(ClaimVerdict::Unverified, None, false),
    ] {
        let found = finding_of(nothing).unwrap();
        assert!(found.unusable && !found.confident, "{found:?}");
        assert_eq!(found.says, SourceSays::Unsupported);
    }
    let failed = finding_of(ClaimJudgment::Failed(AppError::ServiceNotAvailable(
        "down".into(),
    )));
    assert!(
        matches!(failed, Err(AppError::ServiceNotAvailable(_))),
        "{failed:?}"
    );
}

/// An explanation's text of these sentences, as the explanation is written.
fn explanation_text_of(sentences: &[&str]) -> String {
    let mut text = sentences.join("\n");
    text.push('\n');
    text
}

#[test]
fn a_baseline_and_the_evidence_keep_the_brief_the_clean_code_was_written_from() {
    use lattice_core::domain::learning::drills::LessonBrief;
    let brief = LessonBrief {
        lesson_id: "lesson-1".into(),
        course: "Memory science".into(),
        goal: String::new(),
        prior_knowledge: String::new(),
        module: "Spacing".into(),
        module_summary: String::new(),
        outcomes: Vec::new(),
        lesson: "Spacing study over sessions".into(),
        objective: "Explain why spreading study helps".into(),
        evidence: Vec::new(),
    };
    let code = CleanCode {
        brief: Some(brief.clone()),
        ..passage_code()
    };
    let baseline = Baseline::new(&code, &clean_review());
    assert_eq!(baseline.brief, Some(brief.clone()));
    let planted = Planted {
        mutations: Vec::new(),
        trail: Trail::default(),
    };
    let evidence = Evidence::new(
        &code,
        &clean_review(),
        &planted,
        &no_proofs(),
        &unchecked(),
        &quiet(),
    );
    assert_eq!(evidence.brief, Some(brief));
    let without = Evidence::new(
        &passage_code(),
        &clean_review(),
        &planted,
        &no_proofs(),
        &unchecked(),
        &quiet(),
    );
    assert_eq!(without.brief, None);
    assert!(serde_json::to_value(&without)
        .unwrap()
        .get("brief")
        .is_none());
}

// The lesson a stored passage drill fits.

#[tokio::test]
async fn a_stored_passage_drill_records_that_the_lesson_fits_passages() -> Result<()> {
    use super::support::Harness;
    use crate::drills::build::store::Built;
    let pool = pool().await?;
    let program = course(&pool).await?;
    let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
    let harness = Harness::on(pool, &spec).await?;
    let (model, grader) = (ScriptedModel::new([]), FakeGrader::failing("unused"));
    let code = CleanCode {
        basis: Some("basis-1".into()),
        ..passage_code()
    };
    let planted = Planted {
        mutations: vec![figure_mutation()],
        trail: Trail::default(),
    };
    let (proved, review, baseline, blind) =
        (proved_figure(), clean_review(), passage_baseline(), quiet());
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, None, &grader)
        .store(
            &context,
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                checked: &unchecked(),
                blind: &blind,
            },
        )
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    let fits: Vec<(i64, String)> = sqlx::query_as(
        "SELECT fits,basis FROM learning_drill_lesson_fits WHERE material='passage'",
    )
    .fetch_all(&harness.pool)
    .await
    .map_err(db)?;
    assert_eq!(fits, [(1, "basis-1".to_owned())]);
    Ok(())
}

/// A source of `count` short paragraphs of two sentences each, every one made of words the query of [`SPACING_LESSON`]
/// holds, so that every chunk of it matches.
fn large_source(count: usize) -> String {
    (0..count)
        .map(|number| {
            format!(
                "Spacing study over sessions helps recall of item {number}. Spread sessions strengthen recall after {number} days."
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Picks the passage for [`SPACING_LESSON`] from a course of one large source, in a thread of its own so that a
/// picker that never returns fails the test instead of hanging it, and checks that it came back within a minute and
/// is exact bytes of the source. With `already_drilled`, a drill covering the whole source was recorded first.
fn picks_within_the_deadline(already_drilled: bool) {
    const DEADLINE: Duration = Duration::from_secs(60);
    let text = large_source(6_000);
    assert!(text.len() >= 500_000, "{}", text.len());
    let size = text.len();
    let body = text.clone();
    let (ready_sender, ready_receiver) = mpsc::channel::<std::result::Result<String, String>>();
    let (done_sender, done_receiver) =
        mpsc::channel::<(std::result::Result<SourcePassage, String>, Duration)>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async move {
            let setup = async {
                let pool = pool().await?;
                let program = course_of(&pool, &[("Large source", body.as_str())]).await?;
                let (version, _) = source_titled(&pool, &program, "Large source").await?;
                let spec = passage_lesson_spec(&pool, &program, SPACING_LESSON).await?;
                if already_drilled {
                    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
                    let whole = SourcePassage {
                        version_id: version.clone(),
                        start_byte: 0,
                        end_byte: body.len(),
                        title: "Large source".to_owned(),
                        text: body.clone(),
                    };
                    drilled(&pool, "whole", &program, &lesson, &whole).await?;
                }
                Ok::<_, AppError>((pool, spec, version))
            };
            let (pool, spec, version) = match setup.await {
                Ok(done) => done,
                Err(error) => {
                    let _ = ready_sender.send(Err(format!("{error:?}")));
                    return;
                }
            };
            let _ = ready_sender.send(Ok(version));
            let started = Instant::now();
            let picked = next_passage(&pool, &spec).await;
            let _ = done_sender.send((
                picked.map_err(|error| format!("{error:?}")),
                started.elapsed(),
            ));
        });
    });
    let version = ready_receiver
        .recv_timeout(Duration::from_secs(600))
        .expect("the course was set up within ten minutes")
        .expect("the course was set up");
    let (picked, elapsed) = done_receiver.recv_timeout(DEADLINE).unwrap_or_else(|_| {
        panic!("no passage was picked within {DEADLINE:?} from a source of {size} bytes")
    });
    let passage = picked.expect("a passage was picked");
    assert!(elapsed < DEADLINE, "{elapsed:?}");
    assert_eq!(passage.version_id, version);
    assert_eq!(
        text.get(passage.start_byte..passage.end_byte),
        Some(passage.text.as_str())
    );
}

#[test]
fn a_passage_is_picked_from_a_large_fresh_course_within_the_deadline() {
    picks_within_the_deadline(false);
}

#[test]
fn a_passage_is_picked_from_a_large_course_whose_every_place_was_drilled_within_the_deadline() {
    picks_within_the_deadline(true);
}
