use serde::{Serialize, Deserialize};
use tauri::{command, Window};

#[derive(Serialize, Clone)]
struct DraftEvent {
    request_id: String,
    content: String,
}

#[command]
#[specta::specta]
pub async fn start_drafting(
    request_id: String,
    input: String,
    brief: String,
    window: Window,
) -> Result<(), String> {
    let id = request_id.clone();
    tokio::spawn(async move {
        // Simulate streaming
        for i in 1..=5 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = window.emit("draft-progress", DraftEvent {
                request_id: id.clone(),
                content: format!("Fragment {}", i),
            });
        }
        let _ = window.emit("draft-complete", DraftEvent {
            request_id: id,
            content: "{\"draft\": \"complete\"}".to_string(),
        });
    });
    Ok(())
}
