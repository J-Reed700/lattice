//! `find_image`: pictures for the answer, found and stored through the core
//! port, so the conversation feature never reaches into the web feature.
//!
//! The stored pictures go to the window on the stream at once, so the answer's
//! `![caption](img-N)` lines resolve while it is still being written. They are
//! leased to the turn's request until the answer is saved, which attaches the
//! ones it shows and releases the rest (`persistence.rs`).
//!
//! When the turn's model can see, the candidates are first shown to it and
//! only those that clearly show the subject are kept ([`super::image_pick`]);
//! the rest stay leased and are released with the turn's other unused rows.
//! Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.1, §7.8;
//! docs/design/2026-10-17-chat-vision.md §9.

use super::calls::CallScope;
use crate::features::conversation::chat::ChatStreamEventDto;
use lattice_core::application::contracts::images::{
    ChatImageDto, FindImageInput, FindImageOutput, NO_PICTURE_NOTE,
};
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::ImageTurnScope;
use lattice_core::shared::error::Result;
use tracing::warn;

pub(super) const FIND_IMAGE: &str = "find_image";

/// Run one `find_image` call for this turn.
///
/// # Errors
///
/// What the port reports: the store or its database failing. A search that
/// finds nothing is not an error.
pub(super) async fn execute(scope: &CallScope<'_>, call: FunctionCall) -> Result<FunctionResult> {
    let input = match serde_json::from_value::<FindImageInput>(call.arguments) {
        Ok(input) if !input.query.trim().is_empty() => input,
        Ok(_) => {
            return Ok(FunctionResult::error(
                "invalid_arguments",
                "find_image needs a `query` naming what the picture should show",
            ))
        }
        Err(error) => {
            return Ok(FunctionResult::error(
                "invalid_arguments",
                format!("find_image arguments: {error}"),
            ))
        }
    };
    let turn = ImageTurnScope {
        conversation_id: scope.conv_id.to_string(),
        request_id: scope.request_id.to_string(),
    };
    let query = input.query.trim().to_string();
    // Nothing found is a success with the store's note, so the model answers
    // without a picture instead of retrying.
    let (mut output, mut stored) = scope
        .container
        .chat_images()
        .find_and_store(&turn, input, Some(scope.clock.deadline()))
        .await?;
    if let Some(geometry) = scope.llm.vision().filter(|_| !stored.is_empty()) {
        if let Some(picked) =
            super::image_pick::pick(scope, geometry, &query, &output.images, &stored).await
        {
            keep_picked(&mut output, &mut stored, &picked);
        }
    }
    if !stored.is_empty() {
        if let Err(error) = (scope.sink)(ChatStreamEventDto {
            images: Some(stored),
            ..ChatStreamEventDto::new(scope.conv_id, scope.request_id)
        }) {
            warn!(%error, "Failed to send the turn's pictures to the window");
        }
    }
    Ok(FunctionResult::success(
        serde_json::to_value(output).unwrap_or_default(),
    ))
}

/// Keep only the picked candidates, in the result the model reads and in the
/// pictures the window is sent, and say what the pick did.
fn keep_picked(
    output: &mut FindImageOutput,
    stored: &mut Vec<ChatImageDto>,
    picked: &super::image_pick::Picked,
) {
    output
        .images
        .retain(|image| picked.kept.contains(&image.r#ref));
    stored.retain(|image| picked.kept.contains(&image.r#ref));
    output.note = Some(if output.images.is_empty() {
        NO_PICTURE_NOTE.to_string()
    } else {
        format!(
            "{} candidates were shown to you and {} kept.",
            picked.shown,
            output.images.len()
        )
    });
}
