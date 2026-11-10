//! A scratch conversation is never left behind by a part that could not
//! record it.
use super::job_tests::{finished, pool};
use super::*;
use std::sync::Mutex;
use std::time::Duration;

/// Opens scratch conversations and keeps which were opened and removed; a
/// part it was asked to write never gets an answer.
#[derive(Default)]
struct Scratchpad {
    opened: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
}

#[async_trait]
impl SynthesisWork for Scratchpad {
    async fn plan(&self, selection: &SynthesizeJournalEntriesRequestDto) -> Result<SynthesisPlan> {
        Ok(SynthesisPlan {
            scope: "conversation".into(),
            entry_count: 1,
            prompts: vec!["part one".into()],
            conversation_ids: selection.conversation_ids.clone(),
            citations: Vec::new(),
            sources: Vec::new(),
            part_chars: 100_000,
        })
    }

    async fn open_scratch(&self) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.opened.lock().unwrap().push(id.clone());
        Ok(id)
    }

    async fn answer(&self, _conversation_id: &str, _prompt: &str) -> Result<ChatResponse> {
        std::future::pending().await
    }

    async fn remove_scratch(&self, conversation_id: String) -> Result<()> {
        self.removed.lock().unwrap().push(conversation_id);
        Ok(())
    }
}

#[tokio::test]
async fn a_part_whose_scratch_cannot_be_recorded_leaves_no_scratch_conversation() {
    let pool = pool().await;
    sqlx::query(
        "CREATE TRIGGER refuse_scratch BEFORE INSERT ON job_checkpoints \
         WHEN NEW.key = 'scratch' BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let work = Arc::new(Scratchpad::default());
    let jobs = JobRuntime::with_poll_interval(pool.clone(), Duration::from_millis(50));
    jobs.register(
        JOURNAL_SYNTHESIS,
        Arc::new(JournalSynthesisJob::with(work.clone())),
        synthesis_job_config(),
    )
    .await
    .unwrap();

    let started = synthesize_journal_entries_impl(
        StartJournalSynthesisRequestDto {
            request: SynthesizeJournalEntriesRequestDto {
                conversation_ids: vec!["first".into()],
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
    let mut opened = work.opened.lock().unwrap().clone();
    let mut removed = work.removed.lock().unwrap().clone();
    assert!(!opened.is_empty(), "the part never reached its scratch");
    opened.sort();
    removed.sort();
    assert_eq!(removed, opened, "a scratch conversation was left behind");
    jobs.close();
    jobs.drain().await;
}
