//! Conversation messages the last run left mid-flight: unfinished turns and grounding checks.

use crate::features::conversation::chat::resumable_research_questions;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::DatabaseHandle;
use lattice_core::shared::runtime::jobs::JobStore;

/// Close out grounding checks the last run never finished.
///
/// An answer is persisted with `verification.pending = true` and checked in a
/// background task; if the app quits under that task the marker stays, and the
/// message would show "Checking…" forever. Nothing re-runs the check (its
/// evidence lived in the turn), so the honest state is "not checked".
pub async fn reconcile_interrupted_verifications(
    pool: &impl DatabaseHandle,
) -> Result<u64, AppError> {
    super::repository::mark_interrupted_verifications(pool).await
}

/// Close out turns the last run never finished.
///
/// A question is written `pending` before generation and settled when the
/// answer commits or the turn fails. A crash or force-quit in between leaves it
/// `pending`, which renders as still in flight. Only a deep research turn
/// survives a restart, and only while what it saved can be read: its job is
/// delivered again and answers the question its `assembled` checkpoint names,
/// so that row stays `pending`. Every other such row has nothing left to
/// finish it, and is failed so it can be retried.
pub async fn reconcile_stuck_turns(
    pool: &impl DatabaseHandle,
    jobs: &JobStore,
) -> Result<u64, AppError> {
    let resuming = resumable_research_questions(jobs).await?;
    let resuming = serde_json::to_string(&resuming)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    super::repository::fail_unfinished_turns(pool, resuming).await
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::features::conversation::chat::{saved_by_a_turn, DEEP_RESEARCH};
    use crate::features::conversation::repository::ConversationRepository;
    use crate::features::conversation::service::ConversationService;
    use crate::features::conversation::ConversationServiceTrait;
    use lattice_core::domain::conversation::MessageRole;
    use lattice_core::shared::runtime::jobs::{JobStatus, NewJob};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn a_check_the_last_run_never_finished_is_marked_not_checked() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, metadata) in [
            (
                "waiting",
                r#"{"verification":{"enabled":true,"pending":true},"turn":{"x":1}}"#,
            ),
            (
                "checked",
                r#"{"verification":{"enabled":true,"pending":false,"claimsEvaluated":3}}"#,
            ),
            ("off", r#"{"verification":{"enabled":false}}"#),
        ] {
            sqlx::query(
                "INSERT INTO conversation_messages (id, conversation_id, role, content, metadata) \
                 VALUES (?, 'c', 'assistant', 'a', ?)",
            )
            .bind(id)
            .bind(metadata)
            .execute(&pool)
            .await
            .unwrap();
        }

        let marked = reconcile_interrupted_verifications(&pool).await.unwrap();
        assert_eq!(marked, 1);

        let waiting: String =
            sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id = 'waiting'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let waiting: serde_json::Value = serde_json::from_str(&waiting).unwrap();
        assert_eq!(waiting["verification"]["pending"], false);
        assert_eq!(waiting["verification"]["interrupted"], true);
        // The rest of the metadata is untouched.
        assert_eq!(waiting["turn"]["x"], 1);

        let checked: String =
            sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id = 'checked'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!checked.contains("interrupted"));
    }

    #[tokio::test]
    async fn a_turn_the_last_run_never_finished_is_marked_failed() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, role, status) in [
            ("asked", "user", "pending"),
            ("half-written", "assistant", "streaming"),
            ("answered", "user", "completed"),
            ("already-failed", "user", "failed"),
        ] {
            sqlx::query(
                "INSERT INTO conversation_messages (id, conversation_id, role, content, status) \
                 VALUES (?, 'c', ?, 'text', ?)",
            )
            .bind(id)
            .bind(role)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        }

        let jobs = JobStore::new(pool.clone());
        assert_eq!(reconcile_stuck_turns(&pool, &jobs).await.unwrap(), 2);

        let statuses: Vec<(String, String)> =
            sqlx::query_as("SELECT id, status FROM conversation_messages ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let status = |id: &str| {
            statuses
                .iter()
                .find(|(row, _)| row == id)
                .map(|(_, status)| status.as_str())
                .unwrap()
        };
        assert_eq!(status("asked"), "failed");
        assert_eq!(status("half-written"), "failed");
        assert_eq!(status("answered"), "completed");
        assert_eq!(status("already-failed"), "failed");
    }

    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// The pass startup makes over the turns the last run left.
    async fn reconcile(pool: &sqlx::SqlitePool) {
        reconcile_stuck_turns(pool, &JobStore::new(pool.clone()))
            .await
            .unwrap();
    }

    async fn status_of(pool: &sqlx::SqlitePool, message_id: &str) -> String {
        sqlx::query_scalar("SELECT status FROM conversation_messages WHERE id = ?")
            .bind(message_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A question of conversation `c`, saved pending.
    async fn ask(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO conversation_messages (id, conversation_id, role, content, status) \
             VALUES (?, 'c', 'user', 'text', 'pending')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// A research job of `chat` as startup finds it, in `status`, with the
    /// request it was submitted with and, if its turn had planned, the
    /// `assembled` checkpoint it saved.
    async fn save_research_job(
        jobs: &JobStore,
        chat: &str,
        request: &str,
        requested: serde_json::Value,
        assembled: Option<serde_json::Value>,
        status: JobStatus,
    ) {
        let job = jobs
            .submit(&NewJob {
                kind: DEEP_RESEARCH.into(),
                subject_id: Some(chat.into()),
                operation_id: format!("{DEEP_RESEARCH}:{request}"),
                payload_hash: request.into(),
                requested,
                progress_total: 1,
                message: "Waiting to research".into(),
            })
            .await
            .unwrap()
            .id;
        // A job saves checkpoints only while it runs.
        assert!(jobs.claim(&job).await.unwrap().is_some());
        if let Some(assembled) = assembled {
            assert!(jobs
                .put_checkpoint(&job, "assembled", &assembled)
                .await
                .unwrap());
        }
        let settled = match status {
            JobStatus::Running => true,
            JobStatus::Pending => jobs.suspend(&job).await.unwrap(),
            JobStatus::Completed => jobs.complete(&job, "answer", "Answered").await.unwrap(),
            JobStatus::Failed => jobs.fail(&job, "failed", "It failed").await.unwrap(),
            JobStatus::Interrupted => jobs.interrupt(&job).await.unwrap(),
            JobStatus::Cancelled => jobs.cancel(&job).await.is_ok(),
        };
        assert!(settled);
    }

    /// A research turn whose job is still pending or running is delivered
    /// again after a restart and answers its question, so that question stays
    /// pending; a finished job, or no job, leaves its question to be failed.
    #[tokio::test]
    async fn only_the_question_of_a_research_job_still_live_survives_the_restart() {
        let pool = migrated_pool().await;
        let jobs = JobStore::new(pool.clone());
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        // Each question, the state of the research job that saved it, and what
        // startup leaves the question as.
        let cases = [
            ("of-a-waiting-job", Some(JobStatus::Pending), "pending"),
            ("of-a-running-job", Some(JobStatus::Running), "pending"),
            ("of-a-completed-job", Some(JobStatus::Completed), "failed"),
            ("of-a-failed-job", Some(JobStatus::Failed), "failed"),
            ("of-a-cancelled-job", Some(JobStatus::Cancelled), "failed"),
            (
                "of-an-interrupted-job",
                Some(JobStatus::Interrupted),
                "failed",
            ),
            ("of-no-job", None, "failed"),
        ];
        for (question, job, _) in cases {
            ask(&pool, question).await;
            if let Some(status) = job {
                let (requested, assembled) = saved_by_a_turn("c", question, question);
                save_research_job(&jobs, "c", question, requested, Some(assembled), status).await;
            }
        }
        // A turn still planning has saved no question.
        let (requested, _) = saved_by_a_turn("c", "still-planning", "no-question");
        save_research_job(
            &jobs,
            "c",
            "still-planning",
            requested,
            None,
            JobStatus::Pending,
        )
        .await;

        reconcile(&pool).await;

        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, status FROM conversation_messages")
                .fetch_all(&pool)
                .await
                .unwrap();
        let left: BTreeMap<&str, &str> = rows
            .iter()
            .map(|(id, status)| (id.as_str(), status.as_str()))
            .collect();
        let expected: BTreeMap<&str, &str> = cases
            .iter()
            .map(|(question, _, status)| (*question, *status))
            .collect();
        assert_eq!(left, expected);
    }

    /// The resumed turn commits its answer against the question its checkpoint
    /// names, as `finalize_successful_turn` does: that works only if the
    /// startup pass left the question pending.
    #[tokio::test]
    async fn a_resumed_research_turn_saves_its_answer_after_the_startup_sweep() {
        let pool = migrated_pool().await;
        let jobs = JobStore::new(pool.clone());
        let service: Arc<dyn ConversationServiceTrait> = Arc::new(ConversationService::new(
            Arc::new(ConversationRepository::new(pool.clone())),
        ));
        let chat = service
            .create_conversation("Research".into(), None)
            .await
            .unwrap()
            .id
            .to_string();
        let question = service
            .add_message_with_status(
                &chat,
                MessageRole::User,
                "How are lithium batteries recycled?".into(),
                8,
                "pending".into(),
            )
            .await
            .unwrap()
            .id;
        // The app closed with the research turn saved on its job.
        let (requested, assembled) = saved_by_a_turn(&chat, "request", &question);
        save_research_job(
            &jobs,
            &chat,
            "request",
            requested,
            Some(assembled),
            JobStatus::Pending,
        )
        .await;

        // The app opens again and the job is delivered again.
        reconcile(&pool).await;
        let answered = service
            .complete_turn(
                &chat,
                &question,
                "Shredding, then leaching recovers most of the lithium.".into(),
                12,
                None,
            )
            .await;

        assert!(
            answered.is_ok(),
            "the resumed turn could not save its answer: {:?}",
            answered.as_ref().err()
        );
        assert_eq!(status_of(&pool, &question).await, "completed");
        let answers: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM conversation_messages \
             WHERE role = 'assistant' AND status = 'completed'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            answers,
            ["Shredding, then leaching recovers most of the lithium."]
        );
    }

    /// A job whose saved turn cannot be read fails when it is delivered again,
    /// so it never answers its question: startup fails it, live job or not.
    #[tokio::test]
    async fn the_question_of_a_research_job_whose_saved_turn_cannot_be_read_is_failed() {
        let pool = migrated_pool().await;
        let jobs = JobStore::new(pool.clone());
        sqlx::query("INSERT INTO conversations (id, title) VALUES ('c', 'c')")
            .execute(&pool)
            .await
            .unwrap();
        ask(&pool, "of-an-older-turn").await;
        let (requested, mut assembled) = saved_by_a_turn("c", "older", "of-an-older-turn");
        // Saved by an older build: the checkpoint lacks a field this one saves.
        assembled.as_object_mut().unwrap().remove("pages_read");
        save_research_job(
            &jobs,
            "c",
            "older",
            requested,
            Some(assembled),
            JobStatus::Pending,
        )
        .await;

        reconcile(&pool).await;

        assert_eq!(status_of(&pool, "of-an-older-turn").await, "failed");
    }
}
