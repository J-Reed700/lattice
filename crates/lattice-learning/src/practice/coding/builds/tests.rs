#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::jobs::EASY_PATTERNS;
use crate::practice::coding::problem::ProblemDifficulty;
use crate::practice::coding::proof::fakes::{sample_problem, Behaviour, FakeRunner};
use crate::practice::coding::source::{ProgramFacts, ProgramOptions};
use crate::practice::kinds::program::shape::exercise_of;
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::kinds::program::KIND;
use crate::practice::practical_repository::LearningPracticalRepository;
use crate::practice::test_support::fixture;
use lattice_core::domain::learning::practice::problem_patterns::problem_pattern;
use uuid::Uuid;

fn builds(database: Database) -> CodingBuilds {
    CodingBuilds {
        database,
        tools: Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
            Behaviour::Passes { ms: 1 }
        }))),
    }
}

fn options(language: &str, difficulty: ProblemDifficulty) -> Value {
    serde_json::to_value(ProgramOptions {
        language: language.to_owned(),
        difficulty,
        focus: Some("arrays".to_owned()),
    })
    .unwrap()
}

async fn facts_of(
    builds: &CodingBuilds,
    program_id: &str,
    options: &Value,
) -> Result<ProgramFacts> {
    let facts = builds
        .facts(&KindId::new(KIND), program_id, Form::Build, options)
        .await?;
    serde_json::from_value(facts).map_err(|error| AppError::Serialization(error.to_string()))
}

#[tokio::test]
async fn an_easy_problem_calls_for_one_pattern_in_the_language_asked_for() {
    let (_, db, program_id, _, _) = fixture().await.unwrap();
    let builds = builds(db);
    let facts = facts_of(
        &builds,
        &program_id,
        &options(" python ", ProblemDifficulty::Easy),
    )
    .await
    .unwrap();
    assert_eq!(facts.language, "python");
    assert_eq!(facts.patterns.len(), EASY_PATTERNS);
    assert!(facts
        .patterns
        .iter()
        .all(|id| problem_pattern(id).is_some()));
    assert_eq!(facts.details, None);
}

#[tokio::test]
async fn a_harder_problem_calls_for_two_patterns_of_different_families() {
    let (_, db, program_id, _, _) = fixture().await.unwrap();
    let builds = builds(db);
    let facts = facts_of(
        &builds,
        &program_id,
        &options("python", ProblemDifficulty::Medium),
    )
    .await
    .unwrap();
    assert_eq!(facts.patterns.len(), 2);
    let families: Vec<_> = facts
        .patterns
        .iter()
        .map(|id| problem_pattern(id).unwrap().family)
        .collect();
    assert_ne!(families[0], families[1]);
}

#[tokio::test]
async fn a_build_naming_no_language_when_the_profile_names_none_is_refused() {
    let (_, db, program_id, _, _) = fixture().await.unwrap();
    let builds = builds(db);
    let refused = facts_of(
        &builds,
        &program_id,
        &options("  ", ProblemDifficulty::Easy),
    )
    .await;
    assert!(
        matches!(refused, Err(AppError::InvalidInput(_))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_proven_exercise_is_saved_as_the_activity_its_problem_was() {
    let (_, db, program_id, lesson_id, _) = fixture().await.unwrap();
    let builds = builds(db.clone());
    let job_id = Uuid::new_v4().to_string();
    let exercise = exercise_of(&sample_problem()).unwrap();
    let activity_id = builds
        .save(
            &Saving {
                job_id: &job_id,
                program_id: &program_id,
                lesson_id: &lesson_id,
                options: &options("python", ProblemDifficulty::Easy),
                model: "scripted-model",
            },
            &exercise,
        )
        .await
        .unwrap();
    let stored = LearningPracticalRepository::new(db)
        .stored_problem(&program_id, &activity_id)
        .await
        .unwrap();
    assert_eq!(stored.problem, sample_problem());
    assert_eq!(stored.lesson_id, lesson_id);
}

#[tokio::test]
async fn a_repair_is_saved_with_the_planted_code_as_the_starter_file() {
    let (_, db, program_id, lesson_id, _) = fixture().await.unwrap();
    let builds = builds(db.clone());
    let job_id = Uuid::new_v4().to_string();
    let mut repair = sample_problem();
    repair.bug = Some("misreading-2".into());
    let exercise = exercise_of(&repair).unwrap();
    let activity_id = builds
        .save(
            &Saving {
                job_id: &job_id,
                program_id: &program_id,
                lesson_id: &lesson_id,
                options: &options("python", ProblemDifficulty::Easy),
                model: "scripted-model",
            },
            &exercise,
        )
        .await
        .unwrap();
    let planted = repair.misreadings[1].code.clone();
    let repository = LearningPracticalRepository::new(db);
    let stored = repository
        .stored_problem(&program_id, &activity_id)
        .await
        .unwrap();
    assert_eq!(stored.problem.spec.starter, planted);
    let workspace = repository.workspace(&program_id).await.unwrap().workspace;
    let activity = workspace
        .activities
        .iter()
        .find(|activity| activity.id == activity_id)
        .unwrap();
    let starter = activity
        .files
        .iter()
        .find(|file| file.path == crate::practice::coding::problem::SOLUTION_PATH)
        .unwrap();
    assert_eq!(starter.content, planted);
}

#[tokio::test]
async fn a_program_remembers_no_lesson_with_nothing_to_make_because_it_always_has_something() {
    let (_, db, program_id, lesson_id, _) = fixture().await.unwrap();
    let builds = builds(db);
    let kind = KindId::new(KIND);
    let target = Target {
        program_id: &program_id,
        lesson_id: &lesson_id,
        kind: &kind,
        form: Form::Build,
    };
    assert_eq!(builds.nothing_known(&target).await.unwrap(), None);
    assert!(builds.record_nothing(&target, "Nothing.").await.is_err());
}
