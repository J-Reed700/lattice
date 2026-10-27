//! `find_image`'s pick: when the turn's model can see, the stored candidates
//! are shown to it in a side request and only those that clearly show the
//! subject are kept. A side request rather than the transcript, because
//! llama.cpp takes pictures in user messages only and one placed in the
//! conversation would sit in its prefix for the rest of the turn. The pick
//! never fails the tool: on any error every candidate is kept.
//! Design: docs/design/2026-10-17-chat-vision.md §9.

use std::collections::HashSet;
use std::time::Instant;

use lattice_core::application::contracts::images::{ChatImageDto, FoundImage};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, InferencePriority, SamplingOverride,
};
use lattice_core::application::services::completion_input::attach_images_to_last_user;
use lattice_core::domain::models::vision::VisionGeometry;
use tracing::info;

use super::calls::{run_tool_bounded, CallScope, ToolRun};

/// The same text on every pick, so a server can keep it cached.
pub(super) const PICK_SYSTEM: &str = "You judge candidate pictures for a reader. Keep a \
candidate only if it clearly shows the subject asked for. Keep none when none does.";

/// The verdict on one `find_image` call's candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Picked {
    /// Refs to keep: the ones judged to show the subject, and any candidate
    /// that could not be shown to the model, which is kept unjudged.
    pub(super) kept: HashSet<String>,
    /// How many candidates the model was shown.
    pub(super) shown: usize,
}

/// Ask the turn's model which candidates show `query`. `None` when the pick
/// did not reach a verdict; the reason is logged and the caller keeps every
/// candidate.
pub(super) async fn pick(
    scope: &CallScope<'_>,
    geometry: VisionGeometry,
    query: &str,
    candidates: &[FoundImage],
    stored: &[ChatImageDto],
) -> Option<Picked> {
    let started = Instant::now();
    let images = scope.container.chat_images();
    // The whole pick costs about one picture in the answer, and no candidate
    // less than the geometry's smallest rendering.
    let count = u32::try_from(stored.len().max(1)).unwrap_or(u32::MAX);
    let per = geometry.min_tokens().max(geometry.max_tokens() / count);
    let mut parts = Vec::new();
    let mut judged: Vec<&FoundImage> = Vec::new();
    let mut unjudged = HashSet::new();
    for picture in stored {
        let Some(candidate) = candidates.iter().find(|found| found.r#ref == picture.r#ref) else {
            continue;
        };
        match images.prepared(&picture.id, geometry, Some(per)).await {
            Ok(Some(prepared)) => {
                parts.push(prepared.into_part(candidate.r#ref.clone()));
                judged.push(candidate);
            }
            outcome => {
                if let Err(error) = outcome {
                    info!(%error, image_ref = picture.r#ref.as_str(), "A find_image candidate could not be prepared for the pick");
                }
                unjudged.insert(candidate.r#ref.clone());
            }
        }
    }
    if judged.is_empty() {
        skipped("no candidate could be prepared");
        return None;
    }
    let tokens: usize = parts.iter().map(|part| part.tokens as usize).sum();
    let request = pick_request(scope, query, &judged, parts);

    let remaining = match scope.clock.remaining() {
        Ok(remaining) => remaining,
        Err(_) => {
            skipped("the turn is out of time");
            return None;
        }
    };
    let response = match run_tool_bounded(
        scope.llm.complete(&request),
        remaining,
        scope.cancel.cancelled(),
    )
    .await
    {
        ToolRun::Finished(Ok(response)) => response,
        ToolRun::Finished(Err(error)) => {
            skipped(&error.to_string());
            return None;
        }
        ToolRun::TimedOut => {
            skipped("the turn ran out of time");
            return None;
        }
        ToolRun::Cancelled => {
            skipped("the turn was stopped");
            return None;
        }
    };
    let Some(verdict) = parse_keep(&response.text) else {
        skipped("the verdict could not be read");
        return None;
    };
    let mut kept: HashSet<String> = judged
        .iter()
        .filter(|candidate| verdict.contains(&candidate.r#ref))
        .map(|candidate| candidate.r#ref.clone())
        .collect();
    kept.extend(unjudged);
    info!(
        candidates = judged.len(),
        kept = kept.len(),
        tokens,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "chat.image_pick"
    );
    Some(Picked {
        kept,
        shown: judged.len(),
    })
}

fn skipped(reason: &str) {
    info!(reason, "chat.image_pick_skipped");
}

/// The side request: the constant instruction, then the candidates and what
/// they were found for, answered as JSON naming the refs to keep.
fn pick_request(
    scope: &CallScope<'_>,
    query: &str,
    judged: &[&FoundImage],
    parts: Vec<lattice_core::application::ports::llm_port::ImagePart>,
) -> CompletionRequest {
    let refs: Vec<&str> = judged.iter().map(|found| found.r#ref.as_str()).collect();
    let listed: Vec<String> = judged
        .iter()
        .map(|found| format!("{} {:?}", found.r#ref, found.caption))
        .collect();
    let user = format!(
        "Subject: {}\nContext: {}\nCandidates in order: {}\nReturn JSON.",
        query.trim(),
        scope.question.trim(),
        listed.join(", ")
    );
    let mut input = vec![
        CompletionInput::Message {
            role: "system".into(),
            content: PICK_SYSTEM.into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: user,
        },
    ];
    attach_images_to_last_user(&mut input, parts);
    CompletionRequest {
        input,
        json_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "keep": {
                    "type": "array",
                    "items": { "type": "string", "enum": refs }
                }
            },
            "required": ["keep"]
        })),
        sampling: Some(SamplingOverride::deterministic()),
        // Part of the turn the reader is waiting on: Stop cancels it, and
        // the turn's clock bounds it rather than a timeout of its own.
        priority: InferencePriority::Interactive,
        cancel: Some(scope.cancel.clone()),
        no_time_limit: true,
        cache_key: Some(format!("{}:image-pick", scope.conv_id)),
        ..Default::default()
    }
}

/// The `keep` list of a verdict, read leniently: the first JSON object in the
/// reply, whatever surrounds it.
fn parse_keep(text: &str) -> Option<HashSet<String>> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    let keep = value.get("keep")?.as_array()?;
    Some(
        keep.iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
    )
}

#[cfg(test)]
mod tests;
