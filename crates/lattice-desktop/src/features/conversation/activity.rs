//! What chats and journals have running, for the activity view: deep
//! research turns named by their chat, and journal syntheses.

use super::chat::DEEP_RESEARCH;
use super::synthesis::JOURNAL_SYNTHESIS;
use async_trait::async_trait;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivitySource, ActivityTarget,
};
use lattice_core::shared::runtime::jobs::{JobRecord, JobRuntime};
use lattice_core::shared::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;

/// Research turns and syntheses, read from their jobs.
pub struct ConversationActivity {
    pool: SqlitePool,
    jobs: Arc<JobRuntime>,
}

impl ConversationActivity {
    pub fn new(pool: SqlitePool, jobs: Arc<JobRuntime>) -> Self {
        Self { pool, jobs }
    }

    async fn chat_titles(&self, jobs: &[JobRecord]) -> Result<HashMap<String, String>> {
        let ids: Vec<&str> = jobs
            .iter()
            .filter_map(|job| job.subject_id.as_deref())
            .collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids =
            serde_json::to_string(&ids).map_err(|e| AppError::Serialization(e.to_string()))?;
        let rows = sqlx::query(
            "SELECT id, title FROM conversations WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("id"), row.get::<String, _>("title")))
            .collect())
    }
}

/// A job's own steps as its progress, when it counts more than one.
fn steps(job: &JobRecord, unit: &str) -> Option<ActivityProgress> {
    (job.progress_total > 1).then(|| {
        ActivityProgress::of(
            u64::from(job.progress_current),
            u64::from(job.progress_total),
            unit,
        )
    })
}

fn research_item(job: &JobRecord, titles: &HashMap<String, String>) -> ActivityItem {
    let chat = job.subject_id.as_ref().and_then(|id| titles.get(id));
    let title = match chat {
        Some(chat) if !chat.trim().is_empty() => format!("Researching for “{chat}”"),
        _ => "Researching for a chat".to_string(),
    };
    let mut item = ActivityItem::of_job(job, title)
        .progress(steps(job, "rounds"))
        // One research turn runs per chat; another waits behind it.
        .queue(format!(
            "{DEEP_RESEARCH}:{}",
            job.subject_id.as_deref().unwrap_or_default()
        ));
    if let Some(chat) = chat {
        item = item.subject(format!("“{chat}”"));
    }
    match &job.subject_id {
        Some(conversation) => item.target(ActivityTarget::Conversation {
            conversation: conversation.clone(),
        }),
        None => item,
    }
}

fn synthesis_item(job: &JobRecord) -> ActivityItem {
    let title = job
        .requested
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .map_or_else(
            || "Writing a journal synthesis".to_string(),
            |title| format!("Writing “{title}”"),
        );
    ActivityItem::of_job(job, title).progress(steps(job, "steps"))
}

#[async_trait]
impl ActivitySource for ConversationActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let research = self.jobs.store().live(DEEP_RESEARCH).await?;
        let syntheses = self.jobs.store().live(JOURNAL_SYNTHESIS).await?;
        let titles = self.chat_titles(&research).await?;
        Ok(research
            .iter()
            .map(|job| research_item(job, &titles))
            .chain(syntheses.iter().map(synthesis_item))
            .collect())
    }
}
