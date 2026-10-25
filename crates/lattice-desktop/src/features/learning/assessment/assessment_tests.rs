//! Quizzes and tests: normalisation, numeric tolerance, partial credit,
//! option order, rationale checks, and one quiz through the repository.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use crate::features::learning::{
    assessment_engine::{
        self as engine, KeyedOutcome, LearningAssessmentCandidate, LearningAssessmentKey,
        LearningItemFormat, LearningNumericKey, LearningRubricCriterion, LearningTextKey,
    },
    assessment_generation::StoredItem,
    assessment_repository::{
        insert_bank_items_in, BankItem, ItemGrade, LearningAssessmentRepository, Regrade,
    },
    dto::*,
    grade_rationale::{self, Checked, GraderCriterion, GraderItem, GraderVerdict},
    persistence::db,
    quiz_blueprints,
    repository::LearningRepository,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn candidate(
    format: LearningItemFormat,
    options: &[&str],
    key: LearningAssessmentKey,
) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        id: id(),
        outcome_ids: vec![id()],
        format,
        difficulty: 3,
        prompt: "Which of these holds for the scenario?".into(),
        options: options.iter().map(|o| o.to_string()).collect(),
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key,
        explanation: "Because the lesson says so.".into(),
        distractor_rationales: vec![],
    }
}

fn response(item: &LearningAssessmentCandidate) -> engine::LearningAssessmentResponse {
    engine::LearningAssessmentResponse::empty(item.id.clone())
}

fn graded(outcome: KeyedOutcome) -> engine::KeyedGrade {
    match outcome {
        KeyedOutcome::Graded(grade) => grade,
        KeyedOutcome::NeedsJudge => panic!("expected a code grade"),
    }
}

fn text_key(variants: &[&str], numeric: Option<(f64, f64, &[&str])>) -> LearningTextKey {
    LearningTextKey {
        variants: variants.iter().map(|v| v.to_string()).collect(),
        numeric: numeric.map(|(value, tolerance, units)| LearningNumericKey {
            value,
            tolerance,
            unit_variants: units.iter().map(|u| u.to_string()).collect(),
        }),
    }
}

#[test]
fn typed_answers_ignore_case_accents_punctuation_articles_and_separators() {
    assert_eq!(engine::normalize_answer("  The Café, Noir! "), "cafe noir");
    assert_eq!(engine::normalize_answer("1,000,000"), "1000000");
    assert_eq!(engine::normalize_answer("3.14"), "3.14");
    assert_eq!(engine::normalize_answer("An"), "an");
    let item = candidate(
        LearningItemFormat::ShortAnswer,
        &[],
        LearningAssessmentKey::Text(text_key(&["Mitochondrion"], None)),
    );
    let mut answer = response(&item);
    answer.text = Some("the mitochondrion.".into());
    assert!(graded(engine::grade_keyed(&item, &answer).unwrap()).correct);
    answer.text = Some("the powerhouse of the cell".into());
    assert_eq!(
        engine::grade_keyed(&item, &answer).unwrap(),
        KeyedOutcome::NeedsJudge
    );
}

#[test]
fn numbers_are_graded_in_code_within_an_absolute_tolerance() {
    let item = candidate(
        LearningItemFormat::ShortAnswer,
        &[],
        LearningAssessmentKey::Text(text_key(
            &["7 cm"],
            Some((7.0, 0.5, &["cm", "centimetres"])),
        )),
    );
    let mut answer = response(&item);
    for (given, right) in [
        ("7.4", true),
        ("6.5 cm", true),
        ("7,000", false),
        ("8", false),
        ("7.6 centimetres", false),
    ] {
        answer.text = Some(given.into());
        let grade = graded(engine::grade_keyed(&item, &answer).unwrap());
        assert_eq!(grade.correct, right, "{given}");
    }
    answer.text = Some("8".into());
    let grade = graded(engine::grade_keyed(&item, &answer).unwrap());
    assert_eq!(grade.detail, "You gave 8; the answer is 7 ± 0.5 cm.");
    // A number never goes to the model, even when the key has no number.
    let plain = candidate(
        LearningItemFormat::ShortAnswer,
        &[],
        LearningAssessmentKey::Text(text_key(&["twelve", "12"], None)),
    );
    let mut answer = response(&plain);
    answer.text = Some("13".into());
    assert!(
        matches!(engine::grade_keyed(&plain, &answer).unwrap(), KeyedOutcome::Graded(g) if !g.correct)
    );
}

#[test]
fn an_empty_answer_scores_zero_with_a_reason_and_no_model() {
    let item = candidate(
        LearningItemFormat::ShortAnswer,
        &[],
        LearningAssessmentKey::Text(text_key(&["osmosis"], None)),
    );
    let grade = graded(engine::grade_keyed(&item, &response(&item)).unwrap());
    assert_eq!(grade.fraction, 0.0);
    assert_eq!(grade.detail, engine::NO_ANSWER);
}

#[test]
fn partial_credit_follows_the_section_three_rules() {
    // Multi-select: max(0, hits − wrong picks) / |key|.
    let multi = candidate(
        LearningItemFormat::MultiSelect,
        &["Alpha", "Bravo", "Charlie", "Delta", "Echo"],
        LearningAssessmentKey::Choices(BTreeSet::from([0, 1, 2])),
    );
    let mut answer = response(&multi);
    answer.selected_indices = vec![0, 1, 3];
    let grade = graded(engine::grade_keyed(&multi, &answer).unwrap());
    assert!((grade.fraction - 1.0 / 3.0).abs() < 1e-9);
    assert!(grade.detail.starts_with("2 of 3 right, 1 wrong pick."));
    answer.selected_indices = vec![3, 4];
    assert_eq!(
        graded(engine::grade_keyed(&multi, &answer).unwrap()).fraction,
        0.0
    );
    // Cloze: blanks right / blanks.
    let mut cloze = candidate(
        LearningItemFormat::Cloze,
        &[],
        LearningAssessmentKey::Cloze(vec![
            text_key(&["oxygen"], None),
            text_key(&["water"], None),
        ]),
    );
    cloze.prompt = "Plants release {{1}} and take up {{2}}.".into();
    let mut answer = response(&cloze);
    answer.blank_responses = vec!["Oxygen".into(), "light".into()];
    let grade = graded(engine::grade_keyed(&cloze, &answer).unwrap());
    assert_eq!(grade.fraction, 0.5);
    assert!(grade.detail.contains("Blank 2"));
    // Matching: pairs right / pairs.
    let mut matching = candidate(
        LearningItemFormat::Matching,
        &["energy currency", "catalyst", "genetic store", "membrane"],
        LearningAssessmentKey::Matching(vec![0, 1, 2]),
    );
    matching.left = vec!["ATP".into(), "Enzyme".into(), "DNA".into()];
    let mut answer = response(&matching);
    answer.matches = vec![Some(0), Some(2), None];
    let grade = graded(engine::grade_keyed(&matching, &answer).unwrap());
    assert!((grade.fraction - 1.0 / 3.0).abs() < 1e-9);
    // Ordering: correctly ordered pairs / all pairs (Kendall).
    let order: Vec<String> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let ordering = candidate(
        LearningItemFormat::Ordering,
        &["a", "b", "c", "d", "e"],
        LearningAssessmentKey::Ordering(order),
    );
    let mut answer = response(&ordering);
    answer.ordered_values = ["b", "a", "c", "e", "d"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let grade = graded(engine::grade_keyed(&ordering, &answer).unwrap());
    assert_eq!(grade.detail, "8 of 10 pairs in order.");
    assert!((grade.fraction - 0.8).abs() < 1e-9);
}

#[test]
fn shown_positions_are_inverted_before_grading_and_reveals_use_shown_order() {
    let item = candidate(
        LearningItemFormat::SingleChoice,
        &["Alpha", "Bravo", "Charlie", "Delta"],
        LearningAssessmentKey::Choice(2),
    );
    // Shown position -> authored index.
    let order = vec![3, 2, 0, 1];
    let mut shown = response(&item);
    shown.selected_indices = vec![1];
    let authored = engine::to_authored(&shown, &order);
    assert_eq!(authored.selected_indices, vec![2]);
    assert!(graded(engine::grade_keyed(&item, &authored).unwrap()).correct);
    assert_eq!(engine::key_reveal(&item, &order).correct_indices, vec![1]);
    // The same permutation for one seed; a different form reshuffles.
    assert_eq!(
        engine::option_order(LearningItemFormat::SingleChoice, 4, "form-a:item"),
        engine::option_order(LearningItemFormat::SingleChoice, 4, "form-a:item")
    );
}

#[test]
fn authoring_shuffles_the_key_with_its_options() {
    let mut item = candidate(
        LearningItemFormat::SingleChoice,
        &["Alpha", "Bravo", "Charlie", "Delta"],
        LearningAssessmentKey::Choice(1),
    );
    item.distractor_rationales = vec!["a".into(), String::new(), "c".into(), "d".into()];
    for _ in 0..20 {
        let mut shuffled = item.clone();
        engine::shuffle_authored(&mut shuffled);
        let LearningAssessmentKey::Choice(key) = shuffled.key else {
            unreachable!()
        };
        assert_eq!(shuffled.options[key], "Bravo");
        assert_eq!(shuffled.distractor_rationales[key], "");
    }
}

#[test]
fn candidate_checks_refuse_leaks_length_bias_and_bad_blanks() {
    let mut leak = candidate(
        LearningItemFormat::SingleChoice,
        &["osmosis", "diffusion", "filtration", "active transport"],
        LearningAssessmentKey::Choice(0),
    );
    leak.prompt = "Water crossing a membrane by osmosis is called what?".into();
    assert!(engine::validate_candidate(&leak).is_err());
    // Naming several options is a comparison, not a leak.
    leak.prompt =
        "Which differs from diffusion and filtration: osmosis or active transport?".into();
    assert!(engine::validate_candidate(&leak).is_ok());
    let biased = candidate(
        LearningItemFormat::SingleChoice,
        &[
            "Yes",
            "No",
            "Maybe",
            "Only when the membrane is selectively permeable to the solvent",
        ],
        LearningAssessmentKey::Choice(3),
    );
    assert!(engine::validate_candidate(&biased).is_err());
    let mut cloze = candidate(
        LearningItemFormat::Cloze,
        &[],
        LearningAssessmentKey::Cloze(vec![text_key(&["oxygen"], None)]),
    );
    cloze.prompt = "Plants release {{1}} and {{2}}.".into();
    assert!(engine::validate_candidate(&cloze).is_err());
}

fn rubric() -> Vec<LearningRubricCriterion> {
    vec![
        LearningRubricCriterion {
            id: id(),
            title: "Names the mechanism".into(),
            description: "Says which inhibition is at work.".into(),
            max_points: 4,
        },
        LearningRubricCriterion {
            id: id(),
            title: "Explains Vmax".into(),
            description: "Says why Vmax is unchanged.".into(),
            max_points: 4,
        },
    ]
}

fn criterion(
    id: &str,
    verdict: GraderVerdict,
    points: f64,
    missing: &str,
    words: &[&str],
) -> GraderCriterion {
    GraderCriterion {
        criterion_id: id.into(),
        your_words: words.iter().map(|w| w.to_string()).collect(),
        what_was_missing: missing.into(),
        full_credit_includes: "Names competitive inhibition and why Vmax holds.".into(),
        source_quote: None,
        verdict,
        points,
    }
}

const ANSWER: &str = "It is competitive   inhibition, because the inhibitor binds the active site.";

#[test]
fn rationale_checks_keep_verbatim_words_and_trust_the_verdict_over_points() {
    let rubric = rubric();
    let grader = grade_rationale::Grader {
        model: "model",
        graded_at: 1,
    };
    let raw = GraderItem {
        criteria: vec![
            criterion(
                &rubric[0].id,
                GraderVerdict::Met,
                1.0,
                "",
                &["competitive inhibition"],
            ),
            criterion(
                &rubric[1].id,
                GraderVerdict::PartlyMet,
                9.0,
                "Did not say why Vmax is unchanged.",
                &[],
            ),
        ],
        summary: "Named the mechanism but not the Vmax point.".into(),
    };
    let Checked::Graded {
        rationale,
        fraction,
        status,
    } = grade_rationale::check(&rubric, ANSWER, None, raw, &grader)
    else {
        panic!("expected a grade");
    };
    // Met is the maximum whatever was returned; a partly-met 9/4 becomes 2/4.
    assert_eq!(rationale.criteria[0].points, Some(4.0));
    assert_eq!(rationale.criteria[1].points, Some(2.0));
    assert!((fraction - 0.75).abs() < 1e-9);
    assert_eq!(status, LearningAssessmentGradeStatus::Provisional);
    assert_eq!(
        rationale.criteria[0].your_words,
        vec!["competitive inhibition"]
    );
}

#[test]
fn an_invented_quote_makes_the_criterion_uncertain() {
    let rubric = rubric();
    let grader = grade_rationale::Grader {
        model: "model",
        graded_at: 1,
    };
    let raw = GraderItem {
        criteria: vec![
            criterion(
                &rubric[0].id,
                GraderVerdict::Met,
                4.0,
                "",
                &["noncompetitive"],
            ),
            criterion(&rubric[1].id, GraderVerdict::Met, 4.0, "", &[]),
        ],
        summary: String::new(),
    };
    let Checked::Graded {
        rationale, status, ..
    } = grade_rationale::check(&rubric, ANSWER, None, raw, &grader)
    else {
        panic!("expected a grade");
    };
    assert_eq!(status, LearningAssessmentGradeStatus::Uncertain);
    assert_eq!(
        rationale.criteria[0].verdict,
        LearningCriterionVerdict::Uncertain
    );
    assert!(rationale.criteria[0].your_words.is_empty());
}

#[test]
fn no_reason_no_wrong() {
    let rubric = rubric();
    let grader = grade_rationale::Grader {
        model: "model",
        graded_at: 1,
    };
    let unexplained_loss = GraderItem {
        criteria: vec![
            criterion(&rubric[0].id, GraderVerdict::Missed, 0.0, "  ", &[]),
            criterion(&rubric[1].id, GraderVerdict::Met, 4.0, "", &[]),
        ],
        summary: "Partly right.".into(),
    };
    assert!(matches!(
        grade_rationale::check(&rubric, ANSWER, None, unexplained_loss, &grader),
        Checked::Unexplained(_)
    ));
    let no_summary = GraderItem {
        criteria: vec![
            criterion(
                &rubric[0].id,
                GraderVerdict::Missed,
                0.0,
                "Wrong mechanism.",
                &[],
            ),
            criterion(&rubric[1].id, GraderVerdict::Met, 4.0, "", &[]),
        ],
        summary: String::new(),
    };
    assert!(matches!(
        grade_rationale::check(&rubric, ANSWER, None, no_summary, &grader),
        Checked::Unexplained(_)
    ));
}

async fn migrated() -> Result<SqlitePool> {
    let pool = crate::features::learning::tests::pool().await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(pool)
}

const WORDS: [&str; 24] = [
    "membrane",
    "enzyme",
    "ribosome",
    "nucleus",
    "chloroplast",
    "vacuole",
    "cytoplasm",
    "lysosome",
    "golgi",
    "flagellum",
    "cilia",
    "centriole",
    "vesicle",
    "plasmid",
    "spore",
    "hypha",
    "xylem",
    "phloem",
    "stomata",
    "cambium",
    "pollen",
    "anther",
    "stigma",
    "ovule",
];

/// A valid bank item of `format` for `outcome`, distinct per `n`.
fn item(format: LearningItemFormat, outcome: &str, difficulty: u8, n: usize) -> StoredItem {
    let word = |k: usize| WORDS[(n * 3 + k) % WORDS.len()];
    let mut c = candidate(format, &[], LearningAssessmentKey::Rubric);
    c.outcome_ids = vec![outcome.into()];
    c.difficulty = difficulty;
    c.prompt = format!(
        "Case {n}: what follows for {} {} {}?",
        word(0),
        word(1),
        word(2)
    );
    match format {
        LearningItemFormat::SingleChoice => {
            c.options = vec![
                "Alpha".into(),
                "Bravo".into(),
                "Charlie".into(),
                "Delta".into(),
            ];
            c.key = LearningAssessmentKey::Choice(0);
        }
        LearningItemFormat::TrueFalse => {
            c.options = engine::TRUE_FALSE.map(String::from).to_vec();
            c.key = LearningAssessmentKey::Choice(0);
        }
        LearningItemFormat::MultiSelect => {
            c.options = vec![
                "Alpha".into(),
                "Bravo".into(),
                "Charlie".into(),
                "Delta".into(),
            ];
            c.key = LearningAssessmentKey::Choices(BTreeSet::from([0, 1]));
        }
        LearningItemFormat::Matching => {
            c.left = vec!["One".into(), "Two".into(), "Three".into()];
            c.options = vec!["Uno".into(), "Dos".into(), "Tres".into()];
            c.key = LearningAssessmentKey::Matching(vec![0, 1, 2]);
        }
        LearningItemFormat::Ordering => {
            c.options = vec!["First".into(), "Second".into(), "Third".into()];
            c.key = LearningAssessmentKey::Ordering(c.options.clone());
        }
        LearningItemFormat::ShortAnswer => {
            c.key = LearningAssessmentKey::Text(text_key(&["photosynthesis"], None));
        }
        LearningItemFormat::Cloze => {
            c.prompt = format!(
                "Case {n}: the {} makes {{{{1}}}} for {} {}.",
                word(0),
                word(1),
                word(2)
            );
            c.key = LearningAssessmentKey::Cloze(vec![text_key(&["energy"], None)]);
        }
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => {
            c.rubric = rubric();
        }
    }
    engine::validate_candidate(&c).unwrap();
    c.into()
}

static ITEMS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Fill a quiz's bank with one valid item per question its form asks.
pub(crate) async fn fill_bank(
    pool: &SqlitePool,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<usize> {
    let mut tx = pool.begin().await.map_err(db)?;
    let (blueprint, _) = quiz_blueprints::ensure_blueprint_in(&mut tx, program_id, scope)
        .await?
        .ok_or_else(|| AppError::InvalidState("this quiz has nothing to ask".into()))?;
    let requirements = quiz_blueprints::requirements_in(&mut tx, &blueprint).await?;
    let mut items = Vec::new();
    for requirement in &requirements {
        let lesson_id: Option<String> =
            sqlx::query_scalar("SELECT lesson_id FROM learning_outcome_definitions WHERE id=?")
                .bind(&requirement.outcome_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        for _ in 0..requirement.count {
            let n = ITEMS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            items.push(BankItem {
                item: item(
                    requirement.format,
                    &requirement.outcome_id,
                    requirement.difficulty_min,
                    n,
                ),
                source_quote: String::new(),
                lesson_id: lesson_id.clone(),
                origin: "bank",
            });
        }
    }
    insert_bank_items_in(&mut tx, program_id, &items, Some("test-model")).await?;
    tx.commit().await.map_err(db)?;
    Ok(requirements.iter().map(|r| r.count).sum())
}

/// Take a quiz with every answer left blank: graded at once, not passed.
pub(crate) async fn take_quiz(
    pool: &SqlitePool,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<String> {
    fill_bank(pool, program_id, scope).await?;
    let repo = LearningAssessmentRepository::new(pool.clone());
    let form_id = id();
    let form = repo
        .start_quiz(
            &StartLearningQuizRequestDto {
                operation_id: id(),
                form_id: form_id.clone(),
                program_id: program_id.into(),
                scope: scope.clone(),
                mode: LearningQuizMode::Graded,
                retake_of_form_id: None,
            },
            "start",
        )
        .await?;
    repo.submit(
        &MutateLearningAssessmentFormRequestDto {
            operation_id: id(),
            program_id: program_id.into(),
            form_id: form_id.clone(),
            expected_revision: form.revision,
        },
        "submit",
    )
    .await?;
    Ok(form_id)
}

#[tokio::test]
async fn a_lesson_quiz_grades_keyed_answers_at_once_and_waits_for_written_ones() -> Result<()> {
    let pool = migrated().await?;
    let program = crate::features::learning::tests::fixture();
    let programs = LearningRepository::new(pool.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    programs
        .prepare(
            &program_id,
            &lesson_id,
            1,
            &crate::features::learning::tests::prepared(&lesson_id),
        )
        .await?;
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.clone()),
    };
    let asked = fill_bank(&pool, &program_id, &scope).await?;
    let repo = LearningAssessmentRepository::new(pool.clone());
    let overview = repo.overview(&program_id).await?;
    let status = &overview.modules[0].lessons[0].quiz;
    assert_eq!(status.bank_status, LearningQuizBankStatus::Ready);
    assert_eq!(status.verdict, LearningQuizVerdict::NotTaken);
    let form_id = id();
    let mut form = repo
        .start_quiz(
            &StartLearningQuizRequestDto {
                operation_id: id(),
                form_id: form_id.clone(),
                program_id: program_id.clone(),
                scope: scope.clone(),
                mode: LearningQuizMode::Graded,
                retake_of_form_id: None,
            },
            "start",
        )
        .await?;
    assert_eq!(form.items.len(), asked);
    assert!(form.items.iter().all(|i| i.key_reveal.is_none()));
    let written = form
        .items
        .iter()
        .find(|i| i.format == LearningItemFormat::Explanation)
        .map(|i| i.id.clone())
        .unwrap();
    let mut answer = engine::LearningAssessmentResponse::empty(written.clone());
    answer.text = Some("Competitive inhibition: the inhibitor takes the active site.".into());
    form = repo
        .save_response(
            &SaveLearningAssessmentResponseRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                expected_revision: form.revision,
                response: answer,
                assistance: vec![],
            },
            "answer",
        )
        .await?;
    let (form, job) = repo
        .submit(
            &MutateLearningAssessmentFormRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                expected_revision: form.revision,
            },
            "submit",
        )
        .await?;
    let submission = form.submission.clone().unwrap();
    assert!(job.is_some());
    assert_eq!(submission.pending_items, 1);
    assert_eq!(submission.passed, None);
    assert_eq!(
        submission.grade_status,
        LearningAssessmentGradeStatus::Pending
    );
    assert!(form.items.iter().all(|i| i.key_reveal.is_some()));
    // Unanswered keyed items score zero with the reason, never a model call.
    assert!(submission
        .item_results
        .iter()
        .filter(|r| r.grade_status == LearningAssessmentGradeStatus::Deterministic)
        .all(|r| r.detail == engine::NO_ANSWER));
    let ordinal = form.items.iter().position(|i| i.id == written).unwrap() as i64;
    let rationale = LearningGradeRationaleDto {
        summary: "Named the mechanism.".into(),
        criteria: vec![],
        grader_model: "test-model".into(),
        rubric_version: "rubric:test".into(),
        graded_at: 1,
    };
    repo.apply_grades(
        &form_id,
        &[ItemGrade::Graded {
            ordinal,
            fraction: 0.5,
            correct: None,
            status: LearningAssessmentGradeStatus::Provisional,
            detail: String::new(),
            rationale: Some(rationale.clone()),
        }],
        "test-model",
    )
    .await?;
    let (form, reads) =
        crate::tests::common::query_counts::count_reads(repo.get_form(&form_id)).await;
    let form = form?;
    assert_eq!(
        reads, 9,
        "quiz candidates, outcomes and contests must be loaded per form"
    );
    assert!(form.items.len() > 1);
    for item in &form.items {
        let result = form
            .submission
            .as_ref()
            .unwrap()
            .item_results
            .iter()
            .find(|result| result.item_id == item.id)
            .unwrap();
        assert_eq!(result.outcome_ids, item.outcome_ids);
    }
    let submission = form.submission.unwrap();
    assert_eq!(submission.pending_items, 0);
    assert_eq!(submission.passed, Some(false));
    // Contest, an upheld regrade, then an override labelled as such.
    let (_, regrade_job) = repo
        .contest(
            &ContestLearningQuizGradeRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                item_id: written.clone(),
                note: "I named the mechanism.".into(),
            },
            "contest",
        )
        .await?;
    assert!(regrade_job.is_some());
    let contest_id = repo
        .get_form(&form_id)
        .await?
        .submission
        .unwrap()
        .item_results[ordinal as usize]
        .contest
        .clone()
        .unwrap()
        .id;
    repo.apply_regrade(
        &contest_id,
        Regrade::Graded {
            fraction: 0.5,
            correct: None,
            status: LearningAssessmentGradeStatus::Provisional,
            rationale,
            response_to_note: Some("The mechanism was named, but Vmax was not explained.".into()),
        },
        "test-model",
    )
    .await?;
    let form = repo
        .override_grade(
            &OverrideLearningQuizGradeRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                item_id: written.clone(),
            },
            "override",
        )
        .await?;
    let submission = form.submission.unwrap();
    let result = &submission.item_results[ordinal as usize];
    assert!(result.learner_override);
    assert_eq!(
        result.contest.as_ref().unwrap().status,
        LearningGradeContestStatus::Overridden
    );
    assert!(submission.score_without_overrides.unwrap() < submission.score);
    let overview = repo.overview(&program_id).await?;
    assert_eq!(overview.modules[0].lessons[0].quiz.attempts, 1);
    Ok(())
}
