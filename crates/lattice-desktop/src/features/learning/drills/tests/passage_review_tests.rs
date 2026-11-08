//! What the review view's commands do for a drill on a passage of a course: asking for one, the drill as the learner
//! may see it, which holds none of the source, the reveal, which holds the source and the quote behind each error, and
//! the courses a passage drill can draw on.

use super::passage_tests::drilled;
use super::support::{
    course, lesson_titled, lessons_of, spacing_passage, supported, FakeGrader, ScriptedCheck,
    ScriptedModel, FIGURE, FIGURE_QUOTE, FIRST, SPACING_LESSON,
};
use crate::features::learning::drills::attempt_grading::AttemptGrading;
use crate::features::learning::drills::attempts::{self, AttemptMode, AttemptStatus};
use crate::features::learning::drills::dto::{
    CodeReviewFindingDto, CodeReviewMaterialDto, CodeReviewSpanDto, IssueResultKind,
    ReviewDrillCourseDto, StartedCodeReviewDto,
};
use crate::features::learning::drills::jobs::{spec_of, DRILL_BUILD};
use crate::features::learning::drills::material::PassageMaterial;
use crate::features::learning::drills::review::{self, CourseDrill};
use crate::features::learning::tests::{fixture, pool};
use lattice_core::domain::learning::drills::{
    targets, Assessment, FindingOutcome, IssueOutcome, Material,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobStore};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

/// A request for a drill on a passage of the course `program_id`, from any of its lessons.
fn passage(program_id: &str) -> CourseDrill {
    CourseDrill {
        program_id: program_id.to_owned(),
        lesson_id: None,
        material: Material::Passage,
    }
}

/// A course with a drill on its first source, ready, and an attempt opened on it through the command's function.
async fn started() -> Result<(SqlitePool, String, String, Value)> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    drilled(&pool, "passage", &program, &lesson, &spacing_passage()).await?;
    let started = review::start(&pool, &passage(&program), AttemptMode::Practice, 4, 2).await?;
    let shown = serde_json::to_value(&started.dto).expect("a DTO is JSON");
    let StartedCodeReviewDto::Started { attempt_id, .. } = started.dto else {
        panic!("a drill was ready");
    };
    Ok((pool, program, attempt_id, shown))
}

fn escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).expect("text is JSON");
    quoted[1..quoted.len() - 1].to_owned()
}

fn assert_none_of_the_source(shown: &Value) {
    let json = serde_json::to_string(shown).expect("a DTO is JSON");
    for part in [
        spacing_passage().text.as_str(),
        FIRST,
        FIGURE_QUOTE,
        "spacing-v1",
    ] {
        assert!(!json.contains(&escaped(part)), "{part:?} in {json}");
    }
    for name in ["source", "sourceQuote", "versionId", "quote"] {
        assert!(!json.contains(&format!("\"{name}\"")), "{name} in {json}");
    }
}

#[tokio::test]
async fn asking_for_a_passage_with_none_ready_queues_a_build_on_the_course() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let started = review::start(&pool, &passage(&program), AttemptMode::Practice, 6, 2).await?;
    let StartedCodeReviewDto::Building { job_id } = &started.dto else {
        panic!("none was ready");
    };
    let job = JobStore::new(pool.clone()).get(job_id).await?;
    assert_eq!(job.kind, DRILL_BUILD);
    assert_eq!(job.subject_id, Some(format!("drill:{program}:passage")));
    let spec = spec_of(&job)?;
    assert_eq!(spec.material, Material::Passage);
    assert_eq!(spec.program_id.as_deref(), Some(program.as_str()));
    assert_eq!(spec.lesson_id.as_deref(), Some(lessons[0].as_str()));
    assert_eq!(spec.domain, None);
    assert_eq!(spec.lines, 6);
    let wanted: Vec<String> = targets(&Material::Passage, &[], 0, 2)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert_eq!(spec.patterns, wanted);
    Ok(())
}

#[tokio::test]
async fn a_passage_request_that_cannot_make_a_build_is_refused() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    for (material, lines, count) in [
        (passage("  "), 4, 2),
        (passage(&program), 0, 2),
        (passage(&program), 4, 0),
    ] {
        let result = review::start(&pool, &material, AttemptMode::Practice, lines, count).await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{material:?} {lines} {count}: {result:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_passage_drill_is_started_from_its_course_s_shelf_and_the_next_is_built_for_the_course(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    drilled(&pool, "passage", &program, &lessons[0], &spacing_passage()).await?;
    let elsewhere = review::start(&pool, &passage(&other), AttemptMode::Practice, 4, 2).await?;
    assert!(matches!(
        elsewhere.dto,
        StartedCodeReviewDto::Building { .. }
    ));
    let started = review::start(&pool, &passage(&program), AttemptMode::Interview, 4, 2).await?;
    let StartedCodeReviewDto::Started { mode, drill, .. } = started.dto else {
        panic!("a drill was ready");
    };
    assert_eq!(mode, AttemptMode::Interview);
    assert_eq!(
        drill.material,
        CodeReviewMaterialDto::Passage {
            source_title: "Spacing and memory".into()
        }
    );
    assert_eq!(drill.title, "Why spacing study sessions helps");
    assert_eq!(drill.files.len(), 1);
    assert_eq!(drill.files[0].path, "explanation.md");
    let next = JobStore::new(pool.clone()).get(&started.jobs[0]).await?;
    let spec = spec_of(&next)?;
    assert_eq!(spec.material, Material::Passage);
    assert_eq!(spec.program_id.as_deref(), Some(program.as_str()));
    assert_eq!(
        spec.lesson_id.as_deref(),
        Some(lessons[1].as_str()),
        "the drill taken is of the first lesson, which is pending"
    );
    Ok(())
}

#[tokio::test]
async fn the_learners_view_of_a_passage_drill_holds_nothing_of_its_source() -> Result<()> {
    let (pool, _, attempt_id, started) = started().await?;
    assert_none_of_the_source(&started);
    assert_eq!(
        started["drill"]["material"],
        json!({"kind": "passage", "sourceTitle": "Spacing and memory"})
    );
    let open =
        serde_json::to_value(review::result(&pool, &attempt_id).await?).expect("a DTO is JSON");
    assert!(open["reveal"].is_null());
    assert_none_of_the_source(&open);
    let finding = CodeReviewFindingDto {
        span: Some(CodeReviewSpanDto {
            path: "explanation.md".into(),
            start: 1,
            end: 1,
        }),
        text: "The figure is wrong.".into(),
        severity: None,
    };
    let acted = review::submit(&pool, &attempt_id, vec![finding], None, 1_000).await?;
    assert_eq!(acted.dto.status, AttemptStatus::Submitted);
    assert_none_of_the_source(&serde_json::to_value(&acted.dto).expect("a DTO is JSON"));
    Ok(())
}

#[tokio::test]
async fn the_reveal_of_a_passage_drill_holds_the_source_and_the_quote_behind_each_error(
) -> Result<()> {
    let (pool, _, attempt_id, _) = started().await?;
    let acted = review::submit(
        &pool,
        &attempt_id,
        vec![CodeReviewFindingDto {
            span: Some(CodeReviewSpanDto {
                path: "explanation.md".into(),
                start: 1,
                end: 1,
            }),
            text: "The study found 14%, not 40%.".into(),
            severity: None,
        }],
        None,
        1_000,
    )
    .await?;
    let job = acted.job.expect("a grading job");
    let store = JobStore::new(pool.clone());
    store.claim(&job).await?.expect("the job is claimed");
    let (attempt, stored) = attempts::waiting(&pool, &attempt_id)
        .await?
        .expect("the attempt is waiting");
    let context = JobContext::detached(&store, &job).await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let check = ScriptedCheck::new(|_| supported(FIGURE_QUOTE));
    let grader = FakeGrader::new(Assessment {
        issues: vec![(
            IssueOutcome::Found { finding: 0 },
            "It names the figure.".into(),
        )],
        findings: vec![(
            FindingOutcome::Matches { issues: vec![0] },
            "It is the figure.".into(),
        )],
    });
    AttemptGrading::new(
        &model,
        Box::new(PassageMaterial::new(&check)),
        &grader,
        &pool,
        CancellationToken::new(),
    )
    .run(&context, &attempt, &stored)
    .await?;

    let result = review::result(&pool, &attempt_id).await?;
    assert_eq!(result.status, AttemptStatus::Graded);
    let reveal = result.reveal.expect("a graded review is revealed");
    let source = reveal
        .source
        .as_ref()
        .expect("a passage drill reveals its source");
    let expected = spacing_passage();
    assert_eq!(source.title, expected.title);
    assert_eq!(source.text, expected.text);
    assert_eq!(source.version_id, expected.version_id);
    assert_eq!(reveal.issues.len(), 1);
    assert_eq!(reveal.issues[0].outcome, IssueResultKind::Found);
    assert_eq!(reveal.issues[0].pattern, FIGURE);
    assert_eq!(reveal.issues[0].source_quote.as_deref(), Some(FIGURE_QUOTE));
    assert_eq!(reveal.clean_files[0].text.lines().next(), Some(FIRST));
    let shown = serde_json::to_value(&reveal).expect("a DTO is JSON");
    assert_eq!(shown["source"]["versionId"], expected.version_id);
    assert_eq!(shown["issues"][0]["sourceQuote"], FIGURE_QUOTE);
    Ok(())
}

#[tokio::test]
async fn the_courses_a_passage_drill_can_draw_on_are_the_ones_with_a_source() -> Result<()> {
    let pool = pool().await?;
    assert_eq!(review::courses(&pool).await?, []);
    let with_sources = course(&pool).await?;
    let mut bare = fixture();
    bare.sources = Vec::new();
    bare.summary.title = "A course with no source".into();
    crate::features::learning::repository::LearningRepository::new(pool.clone())
        .create(&bare)
        .await?;
    let listed = review::courses(&pool).await?;
    assert_eq!(
        listed,
        [ReviewDrillCourseDto {
            program_id: with_sources,
            title: "A generic learning program".into()
        }]
    );
    assert_eq!(
        serde_json::to_value(&listed).expect("a DTO is JSON")[0]
            .as_object()
            .map(|object| object.len()),
        Some(2)
    );
    Ok(())
}
