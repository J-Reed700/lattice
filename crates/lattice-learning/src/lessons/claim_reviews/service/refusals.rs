use lattice_core::domain::learning::claim_review::{
    Actor, ClaimActionKind, ClaimEvent, EventProvenance, LessonReview, TrustVerdict, UserAction,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::AppError;

use super::super::checker_examples::LabelSource;
use super::super::dto::ClaimActionRequestDto;
use super::Resolution;
use crate::persistence::now;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The event of a user's `action`, the accuracy label it gives and the
/// verdict calibration records.
pub(super) fn user_resolution(
    action: UserAction,
    note: Option<String>,
    label: Option<LabelSource>,
) -> Resolution {
    let trust = match &action {
        UserAction::Confirm { .. } => Some(TrustVerdict::Confirm),
        UserAction::Edit { .. } => Some(TrustVerdict::Edit),
        UserAction::Remove => Some(TrustVerdict::Remove),
        _ => None,
    };
    Resolution {
        event: ClaimEvent::UserAction { action, note },
        label,
        unlabel: None,
        trust,
        forget: None,
    }
}

/// Adds the kinds in `more` that `kinds` lacks.
pub(super) fn add_kinds(kinds: &mut Vec<ClaimKind>, more: Vec<ClaimKind>) {
    for kind in more {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
}

/// The text of a refusal as the user reads it, without the error's kind.
pub(super) fn plain(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(message)
        | AppError::InvalidState(message)
        | AppError::NotFound(message) => message.clone(),
        other => other.to_string(),
    }
}

/// Why the claim's state does not allow `action`.
pub(super) fn not_now(action: ClaimActionKind) -> &'static str {
    match action {
        ClaimActionKind::Confirm => "It is not waiting to be accepted.",
        ClaimActionKind::Edit | ClaimActionKind::Remove => {
            "It has already been removed or replaced."
        }
        ClaimActionKind::PutBack => "Only a sentence the machine took out can be put back.",
        ClaimActionKind::RestoreOriginal => {
            "Only a sentence the machine rewrote has an original to restore."
        }
        ClaimActionKind::Unconfirm => "It has not been accepted.",
        ClaimActionKind::ResearchWithHint => "It is not waiting for a search.",
        ClaimActionKind::AttachEvidence | ClaimActionKind::Undo => "That can't be done to it now.",
    }
}

pub(super) fn action_kind(action: &ClaimActionRequestDto) -> ClaimActionKind {
    match action {
        ClaimActionRequestDto::Confirm { .. } => ClaimActionKind::Confirm,
        ClaimActionRequestDto::Unconfirm => ClaimActionKind::Unconfirm,
        ClaimActionRequestDto::Edit { .. } => ClaimActionKind::Edit,
        ClaimActionRequestDto::Remove => ClaimActionKind::Remove,
        ClaimActionRequestDto::ResearchWithHint { .. } => ClaimActionKind::ResearchWithHint,
        ClaimActionRequestDto::PutBack { .. } => ClaimActionKind::PutBack,
        ClaimActionRequestDto::RestoreOriginal => ClaimActionKind::RestoreOriginal,
        ClaimActionRequestDto::Undo => ClaimActionKind::Undo,
    }
}

/// A user action's provenance (§5.1): no job, the checker the review names.
pub(super) fn provenance(review: &LessonReview) -> EventProvenance {
    EventProvenance {
        actor: Actor::User,
        at: now(),
        job_id: None,
        app_version: APP_VERSION.into(),
        checker_model: Some(review.checker_model.clone()),
    }
}

/// The refusal of a batch of which `refused` of its `chosen` items cannot be
/// `verb`ed: nothing is written.
pub(super) fn batch_refusal(
    verb: &str,
    refused: usize,
    chosen: usize,
    message: &str,
) -> AppError {
    AppError::InvalidState(if refused == chosen {
        format!("None of these can be {verb}: {message}")
    } else {
        format!("{refused} of these can't be {verb}: {message} Nothing was changed.")
    })
}
