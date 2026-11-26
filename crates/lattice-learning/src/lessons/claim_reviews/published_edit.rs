//! Planning and recording a learner/claim-review replacement of a sentence in
//! a published lesson.

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, EventProvenance,
    HumanBinding, UnitKind, UnverifiedReason, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;

use super::actions::{in_section_body, ONLY_SECTION};
use super::repository::{append_in, blocks};
use crate::content_verification::{located, LessonVerificationReport, WrittenOver};

pub(super) const CHANGED: &str =
    "This statement changed since you opened it; reload and try again.";
pub(super) const UNDO_BLOCKED: &str =
    "This section changed after that, so it can't be undone on its own. Edit the section to change it back.";
pub(super) const BLANK: &str = "Write the sentence you want in its place.";

/// Claims ended and created by recording a text action.
pub(super) struct Recorded {
    pub acted: Vec<(ClaimId, u32)>,
    pub created: Vec<ClaimId>,
}

/// Planned replacement of `before` with `after` in a section's body.
pub(super) struct EditPlan {
    pub unit: usize,
    pub old_body: String,
    pub range: (usize, usize),
    pub new_body: String,
    pub before: String,
    pub after: String,
    pub replaced: Vec<ClaimId>,
}

/// Where the claim's sentence is in `body`: the span it recorded when the
/// text there is still the sentence, else the sentence's only occurrence.
pub(super) fn placed(entry: &ClaimLedgerEntry, body: &str) -> Option<(usize, usize)> {
    let (sentence, span) = located(entry.location(), entry.statement(), body);
    span.filter(|&(start, end)| body.get(start..end) == Some(sentence.as_str()))
        .or_else(|| span_of(body, &sentence))
}

/// Records the sentence the user wrote at `location` as claim `claim_id` on
/// `connection`: extracted and published not verified by the machine, as
/// nothing has checked it.
pub(super) async fn user_sentence_in(
    connection: &mut WriteTx,
    lesson_id: &str,
    claim_id: &ClaimId,
    location: ClaimLocation,
    user: &EventProvenance,
) -> Result<()> {
    let machine = user.as_machine();
    let statement = location.quote.clone();
    for event in [
        ClaimEvent::Extracted {
            location,
            statement,
        },
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unchecked,
        },
    ] {
        append_in(connection, lesson_id, claim_id, machine.clone(), event).await?;
    }
    Ok(())
}

/// [`user_sentence_in`], then confirmed by the user who wrote it.
pub(super) async fn confirmed_sentence_in(
    connection: &mut WriteTx,
    lesson_id: &str,
    claim_id: &ClaimId,
    location: ClaimLocation,
    user: &EventProvenance,
) -> Result<()> {
    let quote = location.quote.clone();
    user_sentence_in(connection, lesson_id, claim_id, location, user).await?;
    let confirm = ClaimEvent::UserAction {
        action: UserAction::Confirm {
            bound: HumanBinding::new(&quote, Vec::new()),
        },
        note: None,
    };
    append_in(connection, lesson_id, claim_id, user.clone(), confirm).await?;
    Ok(())
}

/// Lets `change` rewrite the lesson's stored verification report and stores
/// the result with its new hash; a lesson with no stored report is left as it
/// is.
pub(super) async fn update_report(
    connection: &mut WriteTx,
    program_id: &str,
    lesson_id: &str,
    change: impl FnOnce(&mut LessonVerificationReport),
) -> Result<()> {
    let Some(stored) = blocks::verification_report(connection, program_id, lesson_id).await? else {
        return Ok(());
    };
    let mut report: LessonVerificationReport = serde_json::from_str(&stored)?;
    change(&mut report);
    blocks::write_verification_report(
        connection,
        program_id,
        lesson_id,
        report.content_sha256(),
        &serde_json::to_string(&report)?,
    )
    .await
}

/// Computes the edited body and the live claims that overlap the replaced span.
pub(super) fn plan(
    claims: &[ClaimLedgerEntry],
    claim_id: &ClaimId,
    before: &str,
    after: &str,
    body: &str,
) -> Result<EditPlan> {
    let target = claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No statement with that id in this lesson.".into()))?;
    if !in_section_body(target) {
        return Err(AppError::InvalidInput(ONLY_SECTION.into()));
    }
    if after.trim().is_empty() {
        return Err(AppError::InvalidInput(BLANK.into()));
    }
    let unit = target.location().unit_index;
    let changed = || AppError::InvalidInput(CHANGED.into());
    let sentence = placed(target, body).ok_or_else(changed)?;
    let range = if body.get(sentence.0..sentence.1) == Some(before) {
        sentence
    } else {
        span_of(body, before).ok_or_else(changed)?
    };
    let replaced = claims
        .iter()
        .filter(|entry| {
            entry.claim_id() == claim_id
                || (in_section_body(entry)
                    && entry.location().unit_index == unit
                    && !entry.state().is_gone()
                    && placed(entry, body)
                        .is_some_and(|(start, end)| start < range.1 && range.0 < end))
        })
        .map(|entry| entry.claim_id().clone())
        .collect();
    let mut new_body = body.to_owned();
    new_body.replace_range(range.0..range.1, after);
    Ok(EditPlan {
        unit,
        old_body: body.to_owned(),
        range,
        new_body,
        before: before.to_owned(),
        after: after.to_owned(),
        replaced,
    })
}

/// Writes the edit events and report update inside `tx`; the block itself is
/// already written by `learning.section`.
pub(super) async fn record_in(
    tx: &mut WriteTx,
    program_id: &str,
    lesson_id: &str,
    edit: &EditPlan,
    user: &EventProvenance,
) -> Result<Recorded> {
    let successor = ClaimId::generate();
    let location = ClaimLocation {
        unit_index: edit.unit,
        unit_kind: UnitKind::Block,
        field: "body".into(),
        quote: edit.after.clone(),
        span: Some((edit.range.0, edit.range.0 + edit.after.len())),
    };
    confirmed_sentence_in(tx, lesson_id, &successor, location, user).await?;
    let machine = user.as_machine();
    let mut acted = Vec::with_capacity(edit.replaced.len());
    for id in &edit.replaced {
        let edited = ClaimEvent::UserAction {
            action: UserAction::Edit {
                before: edit.before.clone(),
                after: edit.after.clone(),
            },
            note: None,
        };
        let superseded = ClaimEvent::Superseded {
            successor: Some(successor.clone()),
            by: Actor::User,
        };
        let ordinal = append_in(tx, lesson_id, id, user.clone(), edited).await?;
        append_in(tx, lesson_id, id, machine.clone(), superseded).await?;
        acted.push((id.clone(), ordinal));
    }
    let ids: std::collections::HashSet<String> =
        edit.replaced.iter().map(|id| id.0.clone()).collect();
    update_report(tx, program_id, lesson_id, |report| {
        report.written_over(&WrittenOver {
            unit: edit.unit,
            old_body: &edit.old_body,
            range: edit.range,
            new_body: &edit.new_body,
            replaced: &ids,
            after: &edit.after,
            successor: &successor,
            at: user.at,
        })
    })
    .await?;
    Ok(Recorded {
        acted,
        created: vec![successor],
    })
}
