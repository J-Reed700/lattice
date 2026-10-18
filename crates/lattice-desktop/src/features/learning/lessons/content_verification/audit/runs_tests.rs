//! The audit rows and the course switch against a migrated database.

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::*;
use crate::features::learning::curriculum_repository::ProgramVerifierAudit;
use lattice_core::domain::models::model_ref::CloudProvider;

async fn database() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for statement in [
        "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES ('p1', 'Thermodynamics', 'Understand phase changes', 'active', 1, 'none', 30, 'qwen3.5-27b', 1000)",
        "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json, prerequisite_ids_json, project_json) \
         VALUES ('m1', 'p1', 1, 'Phases', 'Summary', '[]', '[]', '{}')",
        "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed, curriculum_state) \
         VALUES ('l1', 'p1', 'm1', 1, 'Boiling', 'Explain boiling', 20, 'outline', 0, 'ready')",
        "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) \
         VALUES ('j1', 'lesson_preparation', 'l1', 'op-1', 'hash', 'pending', '{}', 1, 'Queued', 1000)",
        "INSERT INTO learning_lesson_reviews (lesson_id, program_id, job_id, state, draft_sha256, checker_model, stop_reason_json, opened_at, updated_at) \
         VALUES ('l1', 'p1', 'j1', 'verifying', 'draft-1', 'qwen3.5-27b', NULL, 2000, 2000)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
    pool
}

fn key(draft: &str) -> RunKey {
    RunKey {
        lesson_id: "l1".into(),
        draft_sha256: draft.into(),
        verifier_model: "cloud:anthropic/claude-x".into(),
    }
}

fn batch(index: usize, outcome: BatchOutcome, tokens: (u64, u64)) -> BatchRecord {
    BatchRecord {
        index,
        of: 2,
        claim_ids: vec![format!("c{index}")],
        outcome,
        input_tokens: tokens.0,
        output_tokens: tokens.1,
        error: (outcome == BatchOutcome::Failed)
            .then(|| "Anthropic API returned HTTP 503 Service Unavailable".into()),
        error_code: (outcome == BatchOutcome::Failed).then(|| "NETWORK_ERROR".into()),
        web_note: None,
        at: 3_000,
    }
}

#[tokio::test]
async fn a_run_keeps_each_batch_and_its_tokens_across_a_stop_and_a_resume() {
    let runs = SqliteAuditRuns::new(database().await);
    let key = key("draft-1");

    let opened = runs.begin(&key, "verifier-audit-v1", 2_500).await.unwrap();
    assert_eq!(opened.state, RunState::Running);
    assert!(opened.batches.is_empty());
    runs.record(&key, &batch(1, BatchOutcome::Done, (1_000, 50)))
        .await
        .unwrap();
    runs.record(&key, &batch(2, BatchOutcome::Failed, (0, 0)))
        .await
        .unwrap();
    runs.finish(&key, RunState::Stopped, 3_100).await.unwrap();

    let resumed = runs.begin(&key, "verifier-audit-v1", 9_000).await.unwrap();
    assert_eq!(resumed.state, RunState::Running);
    assert_eq!(resumed.started_at, 2_500, "the first start is kept");
    assert_eq!(resumed.finished_at, None);
    assert_eq!(resumed.batches.len(), 2);
    assert_eq!(resumed.last_failure(), Some("NETWORK_ERROR"));
    assert_eq!((resumed.input_tokens, resumed.output_tokens), (1_000, 50));

    runs.record(&key, &batch(2, BatchOutcome::Done, (900, 40)))
        .await
        .unwrap();
    runs.finish(&key, RunState::Completed, 9_500).await.unwrap();
    let latest = runs.latest("l1").await.unwrap().unwrap();
    assert_eq!(latest.state, RunState::Completed);
    assert_eq!(latest.last_failure(), None, "a batch completed since");
    assert_eq!((latest.input_tokens, latest.output_tokens), (1_900, 90));
}

#[tokio::test]
async fn the_course_total_sums_every_audit_of_its_lessons() {
    let runs = SqliteAuditRuns::new(database().await);
    for (draft, tokens) in [("draft-1", (1_000, 50)), ("draft-2", (2_000, 70))] {
        runs.begin(&key(draft), "verifier-audit-v1", 1)
            .await
            .unwrap();
        runs.record(&key(draft), &batch(1, BatchOutcome::Done, tokens))
            .await
            .unwrap();
    }

    let usage = runs.course_usage("p1").await.unwrap();

    assert_eq!(
        usage,
        CourseAuditUsage {
            audits: 2,
            input_tokens: 3_000,
            output_tokens: 120
        }
    );
    assert_eq!(
        runs.course_usage("other").await.unwrap(),
        CourseAuditUsage::default()
    );
}

#[tokio::test]
async fn a_batch_for_an_audit_that_is_gone_is_refused() {
    let runs = SqliteAuditRuns::new(database().await);
    let refused = runs
        .record(&key("never-begun"), &batch(1, BatchOutcome::Done, (1, 1)))
        .await;
    assert!(matches!(refused, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn the_course_switch_is_stored_and_turning_it_off_clears_the_consent() {
    let programs = LearningCurriculumRepository::new(database().await);
    assert_eq!(
        programs.verifier_audit("p1").await.unwrap(),
        ProgramVerifierAudit::default(),
        "off until the user opts in"
    );

    let on = ProgramVerifierAudit {
        on: true,
        web_search: true,
        consent_at: Some(5_000),
        consent_provider: Some("anthropic".into()),
    };
    programs.set_verifier_audit("p1", &on).await.unwrap();
    assert_eq!(programs.verifier_audit("p1").await.unwrap(), on);

    programs
        .set_verifier_audit(
            "p1",
            &ProgramVerifierAudit {
                on: false,
                ..on.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        programs.verifier_audit("p1").await.unwrap(),
        ProgramVerifierAudit::default()
    );
    assert!(matches!(
        programs.verifier_audit("missing").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn the_gate_stops_an_audit_switched_off_or_consented_for_another_provider() {
    let pool = database().await;
    let programs = LearningCurriculumRepository::new(pool.clone());
    let gate = |model: ModelRef| ProgramGate {
        programs: LearningCurriculumRepository::new(pool.clone()),
        program_id: "p1".into(),
        model,
    };
    let openai = ModelRef::Cloud {
        provider: CloudProvider::Openai,
        model: "gpt-x".into(),
    };
    let local = ModelRef::Local {
        model_id: "qwen3.5-27b".into(),
    };

    assert_eq!(
        gate(openai.clone()).check().await.unwrap(),
        Some(AuditStop::TurnedOff)
    );

    let consented = |provider: Option<&str>| ProgramVerifierAudit {
        on: true,
        web_search: false,
        consent_at: provider.map(|_| 5_000),
        consent_provider: provider.map(str::to_owned),
    };
    programs
        .set_verifier_audit("p1", &consented(Some("anthropic")))
        .await
        .unwrap();
    assert_eq!(
        gate(openai.clone()).check().await.unwrap(),
        Some(AuditStop::ConsentForOtherProvider {
            consented: "anthropic".into(),
            now: CloudProvider::Openai
        })
    );

    programs
        .set_verifier_audit("p1", &consented(Some("openai")))
        .await
        .unwrap();
    assert_eq!(gate(openai).check().await.unwrap(), None);

    programs
        .set_verifier_audit("p1", &consented(None))
        .await
        .unwrap();
    assert_eq!(
        gate(local).check().await.unwrap(),
        None,
        "a Verifier that is not a cloud model needs no consent"
    );
}
