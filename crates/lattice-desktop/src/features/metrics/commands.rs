use crate::features::metrics::dto::MetricsSnapshotDto;
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Retrieves comprehensive snapshot of all application metrics.
pub async fn get_metrics(container: State<'_, Container>) -> Result<MetricsSnapshotDto> {
    let use_case = container.get_metrics_use_case();
    let metrics = use_case.execute().await?;

    Ok(metrics)
}
