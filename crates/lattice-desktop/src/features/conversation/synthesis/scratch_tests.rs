//! A scratch conversation is never left behind by a part that could not
//! record it.
use super::job_tests::{finished, pool};
use super::*;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::conversation::chat::test_runtime::FakeRuntime;
use std::time::Duration;

#[tokio::test]
async fn a_part_whose_scratch_cannot_be_recorded_leaves_no_scratch_conversation() {
    let pool = pool().await;
    let runtime = Arc::new(FakeRuntime::default());
    let service = runtime.conversation_service();
    let mut ids = Vec::new();
    for title in ["Garden notes", "Kitchen notes"] {
        let created = service
            .create_conversation(title.to_string(), None)
            .await
            .unwrap();
        service
            .add_user_message(&created.id.to_string(), format!("Thoughts on {title}"), 4)
            .await
            .unwrap();
        ids.push(created.id.to_string());
    }
    sqlx::query(
        "CREATE TRIGGER refuse_scratch BEFORE INSERT ON job_checkpoints \
         WHEN NEW.key = 'scratch' BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let jobs = JobRuntime::with_poll_interval(pool.clone(), Duration::from_millis(50));
    jobs.register(
        JOURNAL_SYNTHESIS,
        Arc::new(JournalSynthesisJob::new(runtime.clone())),
        synthesis_job_config(),
    )
    .await
    .unwrap();

    let started = synthesize_journal_entries_impl(
        StartJournalSynthesisRequestDto {
            request: SynthesizeJournalEntriesRequestDto {
                conversation_ids: ids.clone(),
                scope: Some("conversation".into()),
            },
            destination: SynthesisDestinationDto::Note {
                note_id: "note-1".into(),
            },
            title: "Notes".into(),
            heading: "Notes".into(),
        },
        &jobs,
    )
    .await
    .unwrap();
    let job = finished(&jobs, &started.job.id).await;

    assert_ne!(job.status, JobStatus::Completed);
    let left = service.list_conversations(None, None).await.unwrap();
    assert_eq!(
        left.len(),
        ids.len(),
        "only the two conversations that were read remain"
    );
    jobs.close();
    jobs.drain().await;
}
