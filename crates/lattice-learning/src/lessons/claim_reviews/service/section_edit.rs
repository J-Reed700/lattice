//! The user's rewrite of one section of a published lesson: the statements on
//! text they changed end, the ones on text they left stay as they are, and
//! each sentence they wrote becomes a statement they confirmed. One
//! transaction writes the text, the ledger, the stored report and the
//! teaching notes' marker, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    HumanBinding, Ledger, UnitKind, UnverifiedReason, UserAction,
};
use lattice_core::shared::error::{AppError, Result};

use super::{provenance, user_resolution, ClaimReviewService, LabelSource, Planned};
use crate::content_verification::Rewritten;
use crate::lessons::claim_reviews::published_edit::{placed, update_report};
use crate::lessons::claim_reviews::published_rewrite::{plan, FieldPlan, Written};
use crate::lessons::claim_reviews::repository::{append_in, bump_revision, check_revision};
use crate::persistence::{db, now};
use lattice_core::domain::learning::claim_review::LessonReviewState;

/// The text a section changed since the user opened it.
pub const SECTION_CHANGED: &str =
    "This section changed since you opened it. Copy your changes, close the editor and open it again.";
pub(super) const NOT_IN_COURSE: &str = "This lesson is not in this course.";
pub(super) const BEING_PREPARED: &str =
    "This lesson is still being prepared. You can edit it once it is ready.";
const NO_TITLE: &str = "A section needs a title.";
const NO_TEXT: &str = "A section needs some text.";

/// A section's new title and text, with the ones the user opened.
pub struct SectionEdit {
    pub program_id: String,
    pub lesson_id: String,
    pub section_index: usize,
    pub before_title: String,
    pub before_body: String,
    pub title: String,
    pub body: String,
    pub dry_run: bool,
}

/// What a rewrite does, or did, to the statements on the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEditOutcome {
    pub changed: bool,
    pub statements_kept: u32,
    pub statements_changed: u32,
    pub statements_removed: u32,
    pub sentences_written: u32,
    /// Changed or removed statements that were verified against a source.
    pub checked_statements_affected: u32,
    pub revision: Option<i64>,
}

/// A statement that ends with the rewrite, and what takes its place.
pub(super) struct Ending<'a> {
    pub(super) entry: &'a ClaimLedgerEntry,
    /// The statement's sentence as its field wrote it.
    pub(super) before: String,
    /// The sentence that replaces it and its claim; none when it was deleted.
    pub(super) after: Option<(ClaimId, String)>,
}

pub(super) fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

async fn current_text(
    connection: &mut sqlx::SqliteConnection,
    lesson_id: &str,
    unit: usize,
) -> Result<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT title, body FROM learning_blocks WHERE lesson_id=? AND ordinal=?",
    )
    .bind(lesson_id)
    .bind(i64::try_from(unit).unwrap_or(i64::MAX))
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    .ok_or_else(|| AppError::InvalidInput(SECTION_CHANGED.into()))
}

pub(super) fn endings<'a>(
    claims: &'a [ClaimLedgerEntry],
    plan: &FieldPlan,
    old: &str,
) -> Vec<Ending<'a>> {
    let find = |id: &ClaimId| claims.iter().find(|entry| entry.claim_id() == id);
    let sentence = |entry: &ClaimLedgerEntry| {
        placed(entry, old)
            .and_then(|(start, end)| old.get(start..end))
            .map_or_else(|| entry.location().quote.clone(), str::to_owned)
    };
    let edited = plan.edited.iter().filter_map(|(id, at)| {
        let entry = find(id)?;
        let written = plan.written.get(*at)?;
        Some(Ending {
            entry,
            before: sentence(entry),
            after: Some((written.successor.clone(), written.text.clone())),
        })
    });
    let removed = plan.removed.iter().filter_map(|id| {
        let entry = find(id)?;
        Some(Ending {
            entry,
            before: sentence(entry),
            after: None,
        })
    });
    edited.chain(removed).collect()
}

impl ClaimReviewService {
    /// Writes the section's new title and text, ends the statements that stood
    /// on what changed, and keeps the rest. A dry run writes nothing. A new
    /// title is written as a statement only when it replaces one on the old title.
    pub async fn edit_section(&self, edit: SectionEdit) -> Result<SectionEditOutcome> {
        let SectionEdit {
            program_id,
            lesson_id,
            section_index: unit,
            before_title,
            before_body,
            title,
            body,
            dry_run,
        } = edit;
        let ledger = self.editable_ledger(&program_id, &lesson_id).await?;
        if title.trim().is_empty() {
            return Err(AppError::InvalidInput(NO_TITLE.into()));
        }
        if body.trim().is_empty() {
            return Err(AppError::InvalidInput(NO_TEXT.into()));
        }
        let mut connection = self.pool().acquire().await.map_err(db)?;
        let (old_title, old_body) = current_text(&mut connection, &lesson_id, unit).await?;
        drop(connection);
        if old_title != before_title || old_body != before_body {
            return Err(AppError::InvalidInput(SECTION_CHANGED.into()));
        }
        let unchanged = SectionEditOutcome {
            changed: false,
            statements_kept: 0,
            statements_changed: 0,
            statements_removed: 0,
            sentences_written: 0,
            checked_statements_affected: 0,
            revision: None,
        };
        if title == old_title && body == old_body {
            return Ok(unchanged);
        }

        let claims: &[ClaimLedgerEntry] = if ledger.review.is_some() {
            &ledger.claims
        } else {
            &[]
        };
        let mut title_plan = plan(claims, unit, "title", &old_title, &title);
        if title_plan.edited.is_empty() {
            title_plan.written.clear();
        }
        let body_plan = plan(claims, unit, "body", &old_body, &body);
        let mut ending = endings(claims, &title_plan, &old_title);
        ending.extend(endings(claims, &body_plan, &old_body));
        let written: Vec<(&str, &Written)> = title_plan
            .written
            .iter()
            .map(|item| ("title", item))
            .chain(body_plan.written.iter().map(|item| ("body", item)))
            .collect();
        let checked = ending
            .iter()
            .filter(|item| matches!(item.entry.state(), ClaimState::ResolvedByEvidence { .. }))
            .count();
        let mut outcome = SectionEditOutcome {
            changed: true,
            statements_kept: count(title_plan.kept.len() + body_plan.kept.len()),
            statements_changed: count(title_plan.edited.len() + body_plan.edited.len()),
            statements_removed: count(title_plan.removed.len() + body_plan.removed.len()),
            sentences_written: count(written.len()),
            checked_statements_affected: count(checked),
            revision: None,
        };
        if dry_run {
            return Ok(outcome);
        }

        let at = now();
        let user = ledger.review.as_ref().map(provenance);
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        if ledger.review.is_some() {
            check_revision(&mut tx, &lesson_id, ledger.revision).await?;
        }
        let (now_title, now_body) = current_text(&mut tx, &lesson_id, unit).await?;
        if now_title != old_title || now_body != old_body {
            return Err(AppError::InvalidInput(SECTION_CHANGED.into()));
        }
        let placed_written: Vec<(&str, UnitKind, &Written)> = written
            .iter()
            .map(|(field, item)| (*field, UnitKind::Block, *item))
            .collect();
        let planned = match &user {
            Some(user) => {
                record_edit(&mut tx, &lesson_id, unit, &placed_written, &ending, user).await?
            }
            None => Vec::new(),
        };
        sqlx::query("UPDATE learning_blocks SET title=?, body=? WHERE lesson_id=? AND ordinal=?")
            .bind(&title)
            .bind(&body)
            .bind(&lesson_id)
            .bind(i64::try_from(unit).unwrap_or(i64::MAX))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let ended: HashSet<String> = ending
            .iter()
            .map(|item| item.entry.claim_id().0.clone())
            .collect();
        let report_written: Vec<(&str, &ClaimId)> = if user.is_some() {
            written
                .iter()
                .map(|(_, item)| (item.text.as_str(), &item.successor))
                .collect()
        } else {
            Vec::new()
        };
        update_report(&mut tx, &program_id, &lesson_id, |report| {
            report.rewritten(&Rewritten {
                unit,
                new_title: &title,
                new_body: &body,
                ended: &ended,
                written: &report_written,
                at,
            })
        })
        .await?;
        sqlx::query(
            "UPDATE learning_lesson_teaching_findings SET part_changed_at=? \
             WHERE lesson_id=? AND site='section' AND unit_index=?",
        )
        .bind(at)
        .bind(&lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if ledger.review.is_some() {
            bump_revision(&mut tx, &lesson_id).await?;
        }
        tx.commit().await.map_err(db)?;

        outcome.revision = self.settle_planned(&ledger, &lesson_id, planned).await?;
        Ok(outcome)
    }
}

impl ClaimReviewService {
    /// A lesson of this course whose review is not still being prepared, with its ledger.
    pub(super) async fn editable_ledger(
        &self,
        program_id: &str,
        lesson_id: &str,
    ) -> Result<Ledger> {
        self.ensure_in_course(program_id, lesson_id).await?;
        let ledger = self.store().load(lesson_id).await?;
        if ledger
            .review
            .as_ref()
            .is_some_and(|review| review.state != LessonReviewState::Ready)
        {
            return Err(AppError::InvalidState(BEING_PREPARED.into()));
        }
        Ok(ledger)
    }

    pub(super) async fn ensure_in_course(&self, program_id: &str, lesson_id: &str) -> Result<()> {
        let in_course: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(lesson_id)
                .bind(program_id)
                .fetch_optional(self.pool())
                .await
                .map_err(db)?;
        if in_course.is_none() {
            return Err(AppError::NotFound(NOT_IN_COURSE.into()));
        }
        Ok(())
    }

    /// Settles each ending after the commit and answers the review's revision
    /// now; none for a lesson without a review.
    pub(super) async fn settle_planned(
        &self,
        ledger: &Ledger,
        lesson_id: &str,
        planned: Vec<Planned<'_>>,
    ) -> Result<Option<i64>> {
        let Some(review) = &ledger.review else {
            return Ok(None);
        };
        let mut redecided = false;
        for item in planned {
            redecided |= self.settle(review, lesson_id, item).await?;
        }
        Ok(Some(if redecided {
            self.store().load(lesson_id).await?.revision.0
        } else {
            ledger.revision.0 + 1
        }))
    }
}

/// Writes the events of an edit inside its transaction: each sentence the user
/// wrote as a statement they confirmed, and for each statement that ends the
/// edit or removal it stood under. Returns what to settle after the commit.
pub(super) async fn record_edit<'a>(
    tx: &mut sqlx::SqliteConnection,
    lesson_id: &str,
    unit: usize,
    written: &[(&str, UnitKind, &Written)],
    ending: &[Ending<'a>],
    user: &EventProvenance,
) -> Result<Vec<Planned<'a>>> {
    let machine = EventProvenance {
        actor: Actor::Machine,
        ..user.clone()
    };
    for (field, kind, item) in written {
        let location = ClaimLocation {
            unit_index: unit,
            unit_kind: *kind,
            field: (*field).into(),
            quote: item.text.clone(),
            span: Some(item.span),
        };
        for (by, event) in [
            (
                &machine,
                ClaimEvent::Extracted {
                    location,
                    statement: item.text.clone(),
                },
            ),
            (
                &machine,
                ClaimEvent::PublishedUnverified {
                    reason: UnverifiedReason::Unchecked,
                },
            ),
            (
                user,
                ClaimEvent::UserAction {
                    action: UserAction::Confirm {
                        bound: HumanBinding::new(&item.text, Vec::new()),
                    },
                    note: None,
                },
            ),
        ] {
            append_in(&mut *tx, lesson_id, &item.successor, by.clone(), event).await?;
        }
    }
    let mut planned = Vec::new();
    for item in ending {
        let (action, label) = match &item.after {
            Some((_, text)) => (
                UserAction::Edit {
                    before: item.before.clone(),
                    after: text.clone(),
                },
                LabelSource::UserEdit,
            ),
            None => (UserAction::Remove, LabelSource::UserRemove),
        };
        let resolution = user_resolution(action, None, Some(label));
        append_in(
            &mut *tx,
            lesson_id,
            item.entry.claim_id(),
            machine.clone(),
            ClaimEvent::SectionEdited {},
        )
        .await?;
        append_in(
            &mut *tx,
            lesson_id,
            item.entry.claim_id(),
            user.clone(),
            resolution.event.clone(),
        )
        .await?;
        if let Some((successor, _)) = &item.after {
            let superseded = ClaimEvent::Superseded {
                successor: Some(successor.clone()),
                by: Actor::User,
            };
            append_in(
                &mut *tx,
                lesson_id,
                item.entry.claim_id(),
                machine.clone(),
                superseded,
            )
            .await?;
        }
        planned.push(Planned {
            entry: item.entry,
            resolution,
        });
    }
    Ok(planned)
}
