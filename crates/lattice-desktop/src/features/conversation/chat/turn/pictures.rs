//! The pictures the reader attached, in the one turn they were sent in.
//!
//! A model declared able to see gets each picture fitted to its geometry
//! (chat-vision §7), images first and the question last in the final user
//! message, and the tool loop carries that message unchanged through every
//! round. A model that cannot see is told, one line per picture, that the
//! reader attached it (§10.6). Later turns, memory and handoff read caption
//! lines only (§11).
//!
//! What the pictures cost is paid with the message, before evidence. When
//! the request has no room for all of them, the last attached is shrunk
//! first, down to the geometry's smallest rendering; then pictures are
//! dropped, last attached first, each becoming its caption line (§8). A
//! geometry that cannot shrink (`Fixed`) drops instead.
use super::prepare::PreparedTurn;
use super::*;
use lattice_core::application::contracts::images::ChatImageDto;
use lattice_core::application::ports::llm_port::{CompletionInput, ContentPart, ImagePart};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::models::vision::VisionGeometry;

/// What the request carries of the attached pictures.
#[derive(Debug, Clone, Default)]
pub(super) struct CarriedPictures {
    /// The pictures the model sees, in the order attached.
    pub(super) parts: Vec<ImagePart>,
    /// One line per picture the model does not see, for the message text.
    pub(super) captions: Vec<String>,
    /// What the pictures cost in prompt tokens: the image parts at their
    /// fitted size, plus the caption lines.
    pub(super) tokens: usize,
    /// Whether the model saw any of them; `false` drives the window's
    /// "Not shown to <model>" note.
    pub(super) seen: bool,
    pub(super) shrunk: usize,
    pub(super) dropped: usize,
}

impl CarriedPictures {
    /// Give the request's final user message the pictures: caption lines
    /// after its text, then the images ahead of it.
    pub(super) fn apply(&self, input: &mut [CompletionInput]) {
        if !self.captions.is_empty() {
            append_to_last_user(input, &self.captions.join("\n"));
        }
        if !self.parts.is_empty() {
            lattice_core::application::services::completion_input::attach_images_to_last_user(
                input,
                self.parts.clone(),
            );
        }
    }
}

/// The conversation's rows for the ids the composer sent, in that order.
/// A lookup that fails sends the question without them rather than failing
/// the turn; the rows stay staged and the next start sweeps them.
pub(super) async fn attached_rows(
    container: &dyn ChatRuntime,
    conversation_id: &str,
    image_ids: &[String],
) -> Vec<ChatImageDto> {
    if image_ids.is_empty() {
        return Vec::new();
    }
    match container
        .chat_images()
        .rows_for_ids(conversation_id, image_ids)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            warn!(%error, conversation_id, "Could not read the attached pictures");
            Vec::new()
        }
    }
}

/// What the pictures cost at the size the model's geometry takes them, from
/// the stored dimensions: nothing is decoded to plan. A model that cannot
/// see pays for its caption lines.
pub(super) fn planned_tokens(llm: &dyn LLMPort, pictures: &[ChatImageDto]) -> usize {
    match llm.vision() {
        Some(geometry) => pictures
            .iter()
            .map(|picture| geometry.plan(picture.width, picture.height).tokens as usize)
            .sum(),
        None => pictures
            .iter()
            .map(|picture| llm.count_tokens(&unseen_line(picture)))
            .sum(),
    }
}

/// `[Picture attached: name (W×H). <why>]`.
fn caption_line(picture: &ChatImageDto, why: &str) -> String {
    let name = lattice_core::shared::markdown_media::picture_captions(&[format!(
        "{} ({}×{})",
        picture.display_name(),
        picture.width,
        picture.height
    )]);
    format!("{}. {why}]", name.trim_end_matches(']'))
}

fn unseen_line(picture: &ChatImageDto) -> String {
    caption_line(
        picture,
        "This model cannot see pictures; say so if the answer depends on it.",
    )
}

/// One attached picture on its way into the request.
struct Planned<'a> {
    picture: &'a ChatImageDto,
    tokens: usize,
    /// The budget it was shrunk to, if it was.
    max_tokens: Option<u32>,
    shown: bool,
}

/// Fit the pictures into `room`: shrink from the last attached to the first,
/// each down to the geometry's smallest rendering, then drop last-first.
fn fit_to_room(
    geometry: VisionGeometry,
    pictures: &[ChatImageDto],
    room: usize,
) -> Vec<Planned<'_>> {
    let mut planned: Vec<Planned<'_>> = pictures
        .iter()
        .map(|picture| Planned {
            picture,
            tokens: geometry.plan(picture.width, picture.height).tokens as usize,
            max_tokens: None,
            shown: true,
        })
        .collect();
    let mut total: usize = planned.iter().map(|plan| plan.tokens).sum();
    for plan in planned.iter_mut().rev() {
        if total <= room {
            break;
        }
        let others = total - plan.tokens;
        let allowed = room
            .saturating_sub(others)
            .max(geometry.min_tokens() as usize);
        let allowed = u32::try_from(allowed).unwrap_or(u32::MAX);
        if let Some(fit) = geometry.fit_within(plan.picture.width, plan.picture.height, allowed) {
            if (fit.tokens as usize) < plan.tokens {
                total = others + fit.tokens as usize;
                plan.tokens = fit.tokens as usize;
                plan.max_tokens = Some(allowed);
            }
        }
    }
    for plan in planned.iter_mut().rev() {
        if total <= room {
            break;
        }
        total -= plan.tokens;
        plan.shown = false;
    }
    planned
}

/// Prepare the turn's attached pictures for its request, within the room the
/// budget left them.
pub(super) async fn carry_pictures(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
) -> CarriedPictures {
    let pictures = &turn.pictures;
    if pictures.is_empty() {
        return CarriedPictures::default();
    }
    let Some(geometry) = turn.llm.vision() else {
        let captions: Vec<String> = pictures.iter().map(unseen_line).collect();
        let tokens = captions
            .iter()
            .map(|line| turn.llm.count_tokens(line))
            .sum();
        turn.recorder.note(
            turn_record::TurnStepKind::OpenDocument,
            picture_label(pictures.len()),
            None,
            Some(format!(
                "not shown: {} cannot see pictures; it was told they are there",
                turn.model.name
            )),
        );
        info!(
            conversation_id = turn.conv_id.as_str(),
            count = pictures.len(),
            tokens,
            shrunk = 0,
            dropped = pictures.len(),
            "chat.pictures_carried"
        );
        return CarriedPictures {
            captions,
            tokens,
            dropped: pictures.len(),
            ..CarriedPictures::default()
        };
    };

    let images = container.chat_images();
    let mut carried = CarriedPictures::default();
    for plan in fit_to_room(geometry, pictures, turn.budget.picture_room) {
        let picture = plan.picture;
        if !plan.shown {
            carried.captions.push(caption_line(
                picture,
                "Not shown: the request has no room for it.",
            ));
            carried.dropped += 1;
            continue;
        }
        match images
            .prepared(&picture.id, geometry, plan.max_tokens)
            .await
        {
            Ok(Some(prepared)) => {
                carried.tokens += prepared.tokens as usize;
                carried.shrunk += usize::from(plan.max_tokens.is_some());
                carried
                    .parts
                    .push(prepared.into_part(picture.display_name()));
            }
            outcome => {
                if let Err(error) = outcome {
                    warn!(%error, image_id = picture.id.as_str(), "Could not prepare an attached picture");
                }
                carried.captions.push(caption_line(
                    picture,
                    "Not shown: it could not be prepared.",
                ));
                carried.dropped += 1;
            }
        }
    }
    carried.tokens += carried
        .captions
        .iter()
        .map(|line| turn.llm.count_tokens(line))
        .sum::<usize>();
    carried.seen = !carried.parts.is_empty();
    turn.recorder.note(
        turn_record::TurnStepKind::OpenDocument,
        picture_label(pictures.len()),
        None,
        Some(match (carried.parts.len(), carried.dropped) {
            (shown, 0) => format!("{shown} shown to {}", turn.model.name),
            (0, dropped) => format!("{dropped} not shown: the request has no room"),
            (shown, dropped) => format!("{shown} shown, {dropped} not shown: no room"),
        }),
    );
    info!(
        conversation_id = turn.conv_id.as_str(),
        count = pictures.len(),
        tokens = carried.tokens,
        shrunk = carried.shrunk,
        dropped = carried.dropped,
        "chat.pictures_carried"
    );
    carried
}

fn picture_label(count: usize) -> String {
    if count == 1 {
        "The attached picture".to_string()
    } else {
        format!("The {count} attached pictures")
    }
}

/// Add `text` as the last lines of the newest user message.
fn append_to_last_user(input: &mut [CompletionInput], text: &str) {
    let Some(item) = input.iter_mut().rev().find(|item| item.is_user()) else {
        return;
    };
    match item {
        CompletionInput::Message { content, .. } => {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(text);
        }
        CompletionInput::Multimodal { parts, .. } => match parts.last_mut() {
            Some(ContentPart::Text { text: last }) => {
                last.push('\n');
                last.push_str(text);
            }
            _ => parts.push(ContentPart::Text {
                text: text.to_string(),
            }),
        },
        _ => {}
    }
}

#[cfg(test)]
mod tests;
