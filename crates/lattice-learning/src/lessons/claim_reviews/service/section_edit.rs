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
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;

use super::{add_kinds, provenance, user_resolution, ClaimReviewService, LabelSource, Planned};
use crate::content_verification::Rewritten;
use crate::lessons::claim_reviews::published_edit::{placed, update_report};
use crate::lessons::claim_reviews::published_rewrite::{plan, FieldPlan, Written};
use crate::lessons::claim_reviews::repository::{
    append_in, blocks, bump_revision, check_revision, teaching_findings,
};
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
        let mut read = self.database.begin_read().await.map_err(db)?;
        let current = blocks::section_title_body(&mut read, &lesson_id, unit).await?;
        read.commit().await.map_err(db)?;
        let (old_title, old_body) =
            current.ok_or_else(|| AppError::InvalidInput(SECTION_CHANGED.into()))?;
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

        let write = SectionWrite {
            program_id: &program_id,
            lesson_id: &lesson_id,
            unit,
            old: (&old_title, &old_body),
            new: (&title, &body),
            written: &written,
            ending: &ending,
            at: now(),
        };
        let moved = self.write_section_edit(&ledger, &write).await?;
        outcome.revision = self.settled_revision(&ledger, &lesson_id, moved).await?;
        Ok(outcome)
    }

    /// The edit and its settlement in one unit of work: the section, the
    /// ledger's events, the stored report, the teaching notes' marker and
    /// what each ended statement gives the checker examples and calibration.
    /// Returns the kinds whose bar the settlement moved.
    async fn write_section_edit(
        &self,
        ledger: &Ledger,
        write: &SectionWrite<'_>,
    ) -> Result<Vec<ClaimKind>> {
        let SectionWrite {
            program_id,
            lesson_id,
            unit,
            old,
            new,
            written,
            ending,
            at,
        } = *write;
        let user = ledger.review.as_ref().map(provenance);
        let mut tx = self.database.begin_write().await.map_err(db)?;
        if ledger.review.is_some() {
            check_revision(&mut tx, lesson_id, ledger.revision).await?;
        }
        let now_text = blocks::section_title_body(&mut tx, lesson_id, unit).await?;
        if now_text
            .as_ref()
            .is_none_or(|(title, body)| *title != old.0 || *body != old.1)
        {
            return Err(AppError::InvalidInput(SECTION_CHANGED.into()));
        }
        let placed_written: Vec<(&str, UnitKind, &Written)> = written
            .iter()
            .map(|(field, item)| (*field, UnitKind::Block, *item))
            .collect();
        let planned = match &user {
            Some(user) => {
                record_edit(&mut tx, lesson_id, unit, &placed_written, ending, user).await?
            }
            None => Vec::new(),
        };
        blocks::write_section(&mut tx, lesson_id, unit, new.0, new.1).await?;
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
        update_report(&mut tx, program_id, lesson_id, |report| {
            report.rewritten(&Rewritten {
                unit,
                new_title: new.0,
                new_body: new.1,
                ended: &ended,
                written: &report_written,
                at,
            })
        })
        .await?;
        teaching_findings::mark_part_changed(&mut tx, lesson_id, "section", unit, at).await?;
        if ledger.review.is_some() {
            bump_revision(&mut tx, lesson_id).await?;
        }
        let moved = self
            .settle_planned_in(&mut tx, ledger, lesson_id, &planned)
            .await?;
        tx.commit().await.map_err(db)?;
        Ok(moved)
    }
}

/// What a section edit writes: the section's text before and after as
/// (title, body), the statements it writes and the ones it ends.
#[derive(Clone, Copy)]
struct SectionWrite<'a> {
    program_id: &'a str,
    lesson_id: &'a str,
    unit: usize,
    old: (&'a str, &'a str),
    new: (&'a str, &'a str),
    written: &'a [(&'a str, &'a Written)],
    ending: &'a [Ending<'a>],
    at: i64,
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
        if !blocks::lesson_in_course(&self.database, program_id, lesson_id).await? {
            return Err(AppError::NotFound(NOT_IN_COURSE.into()));
        }
        Ok(())
    }

    /// Settles each ending on the edit's own unit of work.
    pub(super) async fn settle_planned_in(
        &self,
        tx: &mut WriteTx,
        ledger: &Ledger,
        lesson_id: &str,
        planned: &[Planned<'_>],
    ) -> Result<Vec<ClaimKind>> {
        let Some(review) = &ledger.review else {
            return Ok(Vec::new());
        };
        let mut moved = Vec::new();
        for item in planned {
            let kinds = self.settle_in(tx, review, lesson_id, item).await?;
            add_kinds(&mut moved, kinds);
        }
        Ok(moved)
    }

    /// Re-decides what the settlement's moved bars touch after the commit
    /// and answers the review's revision now; none for a lesson without a
    /// review.
    pub(super) async fn settled_revision(
        &self,
        ledger: &Ledger,
        lesson_id: &str,
        moved: Vec<ClaimKind>,
    ) -> Result<Option<i64>> {
        if ledger.review.is_none() {
            return Ok(None);
        }
        Ok(Some(if self.redecide(lesson_id, moved).await? {
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
    tx: &mut WriteTx,
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
            append_in(tx, lesson_id, &item.successor, by.clone(), event).await?;
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
            tx,
            lesson_id,
            item.entry.claim_id(),
            machine.clone(),
            ClaimEvent::SectionEdited {},
        )
        .await?;
        append_in(
            tx,
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
                tx,
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
