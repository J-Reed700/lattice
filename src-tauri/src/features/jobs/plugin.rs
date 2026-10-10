//! Jobs plugin: what is pending or running, for any job kinds.

use crate::interfaces::di::Container;
use crate::shared::{ipc::ApiError, runtime::jobs::JobDto};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

/// The pending and running jobs of `kinds`, oldest first. Each later change
/// to one, finishing included, arrives on `jobs://status`.
#[tauri::command]
#[specta::specta]
pub async fn list_jobs(
    kinds: Vec<String>,
    container: State<'_, Container>,
) -> Result<Vec<JobDto>, ApiError> {
    let mut jobs = Vec::new();
    for kind in &kinds {
        jobs.extend(
            container
                .jobs()
                .store()
                .live(kind)
                .await?
                .iter()
                .map(JobDto::from),
        );
    }
    jobs.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(jobs)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("jobs")
        .invoke_handler(tauri::generate_handler![list_jobs])
        .build()
}
