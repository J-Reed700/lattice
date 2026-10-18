//! Command bodies behind the three Tauri commands of §11; the
//! `#[tauri::command]` wrappers live in `features::web::plugin`.

use super::{WebPageSnapshotDto, WebPageSnapshotService};
use lattice_core::shared::error::Result;

pub async fn get_web_page_snapshot(
    service: &WebPageSnapshotService,
    conversation_id: &str,
    url: &str,
) -> Result<Option<WebPageSnapshotDto>> {
    service.get(conversation_id, url).await
}

/// "Capture now" for a page with no row, "Try again" for a failed one.
pub async fn capture_web_page_snapshot(
    service: &WebPageSnapshotService,
    conversation_id: &str,
    url: &str,
) -> Result<WebPageSnapshotDto> {
    service.ensure_capture(conversation_id, url, true).await
}

pub async fn list_web_page_snapshots(
    service: &WebPageSnapshotService,
    conversation_id: &str,
) -> Result<Vec<WebPageSnapshotDto>> {
    service.list(conversation_id).await
}
