//! The page captures in flight, for the activity view.

use super::job::KIND;
use super::WebPageSnapshotService;
use async_trait::async_trait;
use lattice_core::application::ports::{ActivityItem, ActivitySource, ActivityTarget};
use lattice_core::shared::runtime::jobs::JobRecord;
use lattice_core::shared::Result;

/// "Saving a copy of example.com", with the chat that cited it.
fn capture_item(job: &JobRecord) -> ActivityItem {
    let url = job
        .requested
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let host = url::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| host.trim_start_matches("www.").to_string())
        })
        .unwrap_or_else(|| "a cited page".to_string());
    let item = ActivityItem::of_job(job, format!("Saving a copy of {host}"))
        .subject(host)
        .queue(KIND);
    match job
        .requested
        .get("conversationId")
        .and_then(serde_json::Value::as_str)
    {
        Some(conversation) => item.target(ActivityTarget::Conversation {
            conversation: conversation.to_string(),
        }),
        None => item,
    }
}

#[async_trait]
impl ActivitySource for WebPageSnapshotService {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let jobs = self.jobs.store().live(KIND).await?;
        Ok(jobs.iter().map(capture_item).collect())
    }
}
