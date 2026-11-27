//! The user's actions on a lesson's claims (§5.1): each is one event in the
//! claim's log, written under the review's revision so an action and the
//! running job never overwrite each other (§9). Edits and removals reach the
//! draft through the next pass's review step. Every verdict also feeds the
//! checker-accuracy examples (§15) and trust calibration (source provenance
//! §13). Design: docs/design/2026-10-16-lesson-claim-review.md.

use std::collections::HashSet;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    span_of, ClaimActionKind, ClaimCounts, ClaimEvent, ClaimId, ClaimLedgerEntry, HumanBinding,
    Ledger, LessonReview, LessonReviewState, Revision, TrustVerdict, UserAction,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::edits::{ContentEdits, Uncommitted};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::Value;

use super::actions::{in_section_body, refusal, ONLY_SECTION};
use super::checker_examples::LabelSource;
use super::dto::ClaimActionRequestDto;
use super::published_edit::{self, placed};
use super::published_undo;
use super::put_back;
use super::repository::{
    append_in, bump_revision, check_revision, reviews, revision_conflict, revision_in,
    ClaimReviewRepository,
};
use crate::content_verification::located_quote;
use crate::curriculum::{LearningGenerationJob, LearningGenerationJobStatus};
use crate::curriculum_repository::LearningCurriculumRepository;
use crate::plan_dto::LearningGenerationJobActionRequestDto;
use lattice_core::shared::text::plain_text;
use lattice_core::shared::text::same_text::whole_sentences;

mod change_detection;
mod note_removal;
mod refusals;
mod section_edit;
mod settle;

pub use change_detection::changed_since;
pub use note_removal::NoteSentencesRemoved;
pub(super) use refusals::user_resolution;
pub(super) use refusals::add_kinds;
use refusals::{action_kind, batch_refusal, not_now, plain, provenance};
pub(super) use section_edit::{endings, record_edit, EditRecord, Ending};
pub(super) use settle::settle_planned_in;

/// §6.2: Resume reruns nothing that has not changed.
pub const NOTHING_CHANGED: &str =
    "Nothing changed since the last pass. Resolve a statement or add evidence first.";

/// What a batch refuses when its action needs the user's eye on one statement.
const ONE_STATEMENT: &str = "Choose one statement for that.";
const NO_STATEMENTS: &str = "Choose the statements first.";
/// Why an Undo of a batch is refused once anything else was done.
const MOVED: &str = "Something changed since then. Undo them one at a time.";
const NEEDS_NOTE: &str = "It needs a note before it can be accepted.";

#[derive(Clone)]
pub struct ClaimReviewService {
    pub(super) database: Database,
    pub(super) store: ClaimReviewRepository,
    drafts: LearningCurriculumRepository,
    pub(super) checker_model: Option<String>,
}

/// One claim's event, the accuracy label it gives (§15) or, on Undo, the
/// label it takes back (`unlabel`), and the verdict calibration records
/// (`trust`) or forgets on Undo (`forget`).
pub(super) struct Resolution {
    pub(super) event: ClaimEvent,
    pub(super) label: Option<LabelSource>,
    pub(super) unlabel: Option<LabelSource>,
    pub(super) trust: Option<TrustVerdict>,
    pub(super) forget: Option<TrustVerdict>,
}

/// A claim and the action checked for it, ready to write.
pub(super) struct Planned<'a> {
    pub(super) entry: &'a ClaimLedgerEntry,
    pub(super) resolution: Resolution,
}

impl ClaimReviewService {
    pub fn new(database: Database) -> Self {
        Self {
            store: ClaimReviewRepository::new(database.clone()),
            drafts: LearningCurriculumRepository::new(database.clone()),
            checker_model: None,
            database,
        }
    }

    /// The configured Learning checker, read without loading a model. The
    /// review screen and Resume must use the same current configuration.
    pub fn with_checker_model(mut self, model: Option<String>) -> Self {
        self.checker_model = model;
        self
    }

    pub(super) fn store(&self) -> &ClaimReviewRepository {
        &self.store
    }

    pub(super) fn drafts(&self) -> &LearningCurriculumRepository {
        &self.drafts
    }

    /// Resume verification (§6.2): a new attempt of the lesson's job that carries
    /// the user's actions into the draft and checks what changed. Refused while a
    /// claim waits for the user and none changed; a lesson held by the Verifier
    /// audit alone resumes at once (§14.6). `expected_revision` is the course's.
    pub async fn resume(
        &self,
        program_id: &str,
        lesson_id: &str,
        expected_revision: i64,
        operation_id: String,
    ) -> Result<LearningGenerationJob> {
        let ledger = self.store.load(lesson_id).await?;
        let review = ledger
            .review
            .as_ref()
            .filter(|review| review.program_id == program_id)
            .ok_or_else(|| {
                AppError::NotFound("This lesson has no statement review to resume.".into())
            })?;
        let job = self.drafts.job(&review.job_id).await?;
        let request = LearningGenerationJobActionRequestDto {
            operation_id,
            program_id: program_id.into(),
            job_id: job.id.clone(),
            expected_revision,
        };
        let waiting = review.state == LessonReviewState::NeedsReview
            && job.status == LearningGenerationJobStatus::Completed;
        if !waiting {
            return self.drafts.retry_job(&request).await;
        }
        let held_by_claims = ClaimCounts::tally(ledger.claims.iter()).queued > 0;
        if held_by_claims && !self.changed_in(&ledger).await? {
            return Err(AppError::InvalidState(NOTHING_CHANGED.into()));
        }
        self.drafts.continue_review_job(&request).await
    }

    /// Applies the user's `action` to `claim_id` and returns the review's new
    /// revision. Refused, writing nothing, when `expected_revision` is stale
    /// or the claim's state does not allow the action.
    pub async fn resolve(
        &self,
        lesson_id: &str,
        claim_id: &str,
        action: ClaimActionRequestDto,
        expected_revision: i64,
    ) -> Result<i64> {
        let ledger = self.store.load(lesson_id).await?;
        let review = ledger
            .review
            .clone()
            .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
        if ledger.revision != Revision(expected_revision) {
            return Err(revision_conflict());
        }
        let entry = ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id().0 == claim_id)
            .ok_or_else(|| {
                AppError::NotFound("No statement with that id in this lesson.".into())
            })?;
        let resolution = self.plan(&review, lesson_id, entry, action, None).await?;
        self.carry_out(
            &review,
            lesson_id,
            ledger.revision,
            vec![Planned { entry, resolution }],
            false,
        )
        .await
    }

    /// Applies one action to every claim in `claim_ids`, all or nothing:
    /// each claim is checked as `resolve` checks it, and when any is refused
    /// nothing is written. Undo takes the claims' last action back, the last
    /// one done first, and needs the revision the batch returned. Put backs
    /// are placed one after the other, each against the text the ones before
    /// it leave. Returns the review's new revision.
    pub async fn resolve_many(
        &self,
        lesson_id: &str,
        claim_ids: &[String],
        action: ClaimActionRequestDto,
        expected_revision: i64,
    ) -> Result<i64> {
        let kind = action_kind(&action);
        let in_bulk = match &action {
            ClaimActionRequestDto::Confirm {
                note,
                source_version_ids,
            } => note.is_none() && source_version_ids.is_empty(),
            ClaimActionRequestDto::Remove | ClaimActionRequestDto::Undo => true,
            ClaimActionRequestDto::PutBack { text, .. } => text.is_none(),
            _ => false,
        };
        if !in_bulk {
            return Err(AppError::InvalidInput(ONE_STATEMENT.into()));
        }
        let ledger = self.store.load(lesson_id).await?;
        let review = ledger
            .review
            .clone()
            .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
        if ledger.revision != Revision(expected_revision) {
            return Err(if kind == ClaimActionKind::Undo {
                AppError::InvalidState(MOVED.into())
            } else {
                revision_conflict()
            });
        }
        let mut chosen: Vec<&ClaimLedgerEntry> = Vec::new();
        for id in claim_ids {
            let entry = ledger
                .claims
                .iter()
                .find(|entry| entry.claim_id().0 == *id)
                .ok_or_else(|| {
                    AppError::NotFound("No statement with that id in this lesson.".into())
                })?;
            if !chosen
                .iter()
                .any(|seen| seen.claim_id() == entry.claim_id())
            {
                chosen.push(entry);
            }
        }
        if chosen.is_empty() {
            return Err(AppError::InvalidInput(NO_STATEMENTS.into()));
        }
        if kind == ClaimActionKind::Undo {
            chosen.reverse();
        }
        let published = review.state == LessonReviewState::Ready;
        let mut staged = if kind == ClaimActionKind::PutBack {
            Some(
                self.put_back_units(&review, lesson_id, &ledger.claims)
                    .await?,
            )
        } else {
            None
        };
        let mut brought_back: HashSet<ClaimId> = HashSet::new();
        let mut planned = Vec::new();
        let mut refused: Vec<AppError> = Vec::new();
        for &entry in &chosen {
            if brought_back.contains(entry.claim_id()) {
                let resolution = Self::put_back_with_sibling(entry);
                planned.push(Planned { entry, resolution });
                continue;
            }
            let result = self
                .plan(&review, lesson_id, entry, action.clone(), staged.as_ref())
                .await;
            match result {
                Ok(resolution) => {
                    if let Some(units) = staged.as_mut() {
                        put_back::stage(units, &ledger.claims, entry, &resolution.event, published);
                        brought_back.insert(entry.claim_id().clone());
                        brought_back.extend(
                            put_back::siblings(&ledger.claims, entry)
                                .into_iter()
                                .map(|other| other.claim_id().clone()),
                        );
                    }
                    planned.push(Planned { entry, resolution });
                }
                Err(
                    error @ (AppError::InvalidInput(_)
                    | AppError::InvalidState(_)
                    | AppError::NotFound(_)),
                ) => refused.push(error),
                Err(error) => return Err(error),
            }
        }
        if let Some(first) = refused.first() {
            let verb = match kind {
                ClaimActionKind::Confirm => "accepted",
                ClaimActionKind::Remove => "removed",
                ClaimActionKind::PutBack => "put back",
                _ => "undone",
            };
            return Err(batch_refusal(
                verb,
                refused.len(),
                chosen.len(),
                &plain(first),
            ));
        }
        self.carry_out(&review, lesson_id, ledger.revision, planned, true)
            .await
    }

    /// One claim's action checked against the claim and, in a published
    /// lesson, the lesson's text, with nothing written: what `resolve` and
    /// `resolve_many` both write.
    async fn plan(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        action: ClaimActionRequestDto,
        staged: Option<&Value>,
    ) -> Result<Resolution> {
        let kind = action_kind(&action);
        if review.state == LessonReviewState::Ready {
            if let Some(refusal) = refusal(entry, kind) {
                return Err(refusal);
            }
        }
        if kind != ClaimActionKind::Undo && !entry.allows(kind) {
            return Err(AppError::InvalidState(not_now(kind).into()));
        }
        if let ClaimActionRequestDto::Confirm { note, .. } = &action {
            if note.as_deref().is_none_or(|text| text.trim().is_empty())
                && entry.confirm_requires_note()
            {
                return Err(AppError::InvalidState(NEEDS_NOTE.into()));
            }
        }
        self.resolution(review, lesson_id, entry, action, staged)
            .await
    }

    /// Writes the planned claims' events and text changes, and what each
    /// gives the checker examples and trust calibration, in one unit of work
    /// under `revision`, then re-decides the claims a moved bar touches.
    /// Returns the review's revision after.
    async fn carry_out(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        revision: Revision,
        planned: Vec<Planned<'_>>,
        batch_undo: bool,
    ) -> Result<i64> {
        let edits = crate::edits::content_edits(&self.database)?;
        let (written, moved, uncommitted) = self
            .commit(&edits, review, lesson_id, revision, &planned, batch_undo)
            .await?;
        for pending in uncommitted {
            let _ = edits.committed(pending).await;
        }
        // A bar the verdicts moved may have re-decided this lesson's claims.
        if self.redecide(lesson_id, moved).await? {
            return Ok(self.store.load(lesson_id).await?.revision.0);
        }
        Ok(written.0)
    }

    /// The planned claims' events, and in a published lesson the edits and
    /// removals of its text, written with their settlement in one
    /// transaction that checks the review's revision and advances it once.
    /// Returns the revision after, the kinds whose bar the verdicts moved, and
    /// any pending edit follow-ups.
    async fn commit(
        &self,
        edits: &ContentEdits,
        review: &LessonReview,
        lesson_id: &str,
        revision: Revision,
        planned: &[Planned<'_>],
        batch_undo: bool,
    ) -> Result<(Revision, Vec<ClaimKind>, Vec<Uncommitted>)> {
        let published = review.state == LessonReviewState::Ready;
        let user = provenance(review);
        let mut tx = self.database.begin_write().await?;
        check_revision(&mut tx, lesson_id, revision).await?;
        let mut ended: HashSet<ClaimId> = HashSet::new();
        let mut uncommitted: Vec<Uncommitted> = Vec::new();
        let mut wrote_via_edits = false;
        let mut batch_text_undos: Vec<(&ClaimLedgerEntry, u32)> = Vec::new();
        for item in planned {
            if let ClaimEvent::Undone { .. } = &item.resolution.event {
                if published && batch_undo {
                    if let Some(ordinal) = published_undo::text_undo_ordinal(item.entry) {
                        batch_text_undos.push((item.entry, ordinal));
                        continue;
                    }
                }
            }
            let id = item.entry.claim_id();
            if ended.contains(id) {
                continue;
            }
            match &item.resolution.event {
                event @ ClaimEvent::UserAction {
                    action:
                        UserAction::Edit { .. }
                        | UserAction::Remove
                        | UserAction::PutBack { .. }
                        | UserAction::RestoreOriginal { .. },
                    ..
                } if published => {
                    let pending = published_undo::apply_claim_action_in(
                        &mut tx, edits, lesson_id, item.entry, event, &mut ended,
                    )
                    .await?;
                    uncommitted.push(pending);
                    wrote_via_edits = true;
                }
                event @ ClaimEvent::UserAction {
                    action: UserAction::PutBack { .. },
                    ..
                } => {
                    ended.extend(
                        put_back::write_draft_in(&mut tx, lesson_id, id, event, &user).await?,
                    );
                }
                ClaimEvent::Undone { .. } if published => {
                    if let Some(ordinal) = published_undo::text_undo_ordinal(item.entry) {
                        let pending = published_undo::revert_single_in(
                            &mut tx, edits, lesson_id, item.entry, ordinal, &mut ended,
                        )
                        .await?;
                        uncommitted.push(pending);
                        wrote_via_edits = true;
                    } else {
                        append_in(
                            &mut tx,
                            lesson_id,
                            id,
                            user.clone(),
                            item.resolution.event.clone(),
                        )
                        .await?;
                    }
                }
                event => {
                    append_in(&mut tx, lesson_id, id, user.clone(), event.clone()).await?;
                }
            }
        }
        if !batch_text_undos.is_empty() {
            uncommitted.extend(
                published_undo::revert_batch_in(&mut tx, edits, lesson_id, &batch_text_undos)
                    .await?,
            );
            wrote_via_edits = true;
        }
        let written = if wrote_via_edits {
            revision_in(&mut tx, lesson_id).await?.unwrap_or(revision)
        } else {
            bump_revision(&mut tx, lesson_id).await?
        };
        let mut moved = Vec::new();
        for item in planned {
            let kinds = settle::settle_in(&mut tx, review, lesson_id, item).await?;
            add_kinds(&mut moved, kinds);
        }
        if published {
            crate::assessment_repository::sync_disputed_questions_in(&mut tx, lesson_id).await?;
        }
        tx.commit().await?;
        Ok((written, moved, uncommitted))
    }

    async fn resolution(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        action: ClaimActionRequestDto,
        staged: Option<&Value>,
    ) -> Result<Resolution> {
        let user = |action: UserAction, note: Option<String>, label: Option<LabelSource>| {
            Ok(user_resolution(action, note, label))
        };
        match action {
            ClaimActionRequestDto::Confirm {
                note,
                source_version_ids,
            } => {
                let quote = self.bound_quote(review, lesson_id, entry).await;
                let deleted = reviews::deleted_versions(&self.database, &review.program_id).await?;
                let source_version_ids = source_version_ids
                    .into_iter()
                    .filter(|id| !deleted.contains(id))
                    .collect();
                user(
                    UserAction::Confirm {
                        bound: HumanBinding::new(&quote, source_version_ids),
                    },
                    note,
                    Some(LabelSource::UserConfirm),
                )
            }
            ClaimActionRequestDto::Unconfirm => user(UserAction::Unconfirm, None, None),
            ClaimActionRequestDto::Edit { before, after } => {
                if before.is_empty() || before == after {
                    return Err(AppError::InvalidInput(
                        "An edit names the text it replaces and different text to put there."
                            .into(),
                    ));
                }
                let text = self.field_text(review, lesson_id, entry).await?;
                // The card shows the claim's sentence as plain text; the
                // edit replaces the sentence as the lesson writes it (claim
                // sources design §18.3). Other text must occur once.
                let sentence = located_quote(entry.location(), entry.statement(), &text);
                let before = if before == sentence || before == plain_text(&sentence) {
                    sentence
                } else if span_of(&text, &before).is_some() {
                    before
                } else {
                    return Err(AppError::InvalidInput(
                        "The text to replace must occur exactly once in its section.".into(),
                    ));
                };
                if before == after {
                    return Err(AppError::InvalidInput(
                        "An edit names the text it replaces and different text to put there."
                            .into(),
                    ));
                }
                user(
                    UserAction::Edit { before, after },
                    None,
                    Some(LabelSource::UserEdit),
                )
            }
            ClaimActionRequestDto::Remove => {
                let text = self.field_text(review, lesson_id, entry).await?;
                if !placed(entry, &text).is_some_and(|span| whole_sentences(&text, span)) {
                    return Err(AppError::InvalidInput(
                        "This statement is part of a sentence. Edit the sentence to say what goes."
                            .into(),
                    ));
                }
                user(UserAction::Remove, None, Some(LabelSource::UserRemove))
            }
            ClaimActionRequestDto::ResearchWithHint { hint } => {
                let hint = hint.trim().to_owned();
                if hint.is_empty() {
                    return Err(AppError::InvalidInput("Say what to search for.".into()));
                }
                user(UserAction::ResearchWithHint { hint }, None, None)
            }
            ClaimActionRequestDto::PutBack { text, section } => {
                self.putting_back(review, lesson_id, entry, text, section, staged)
                    .await
            }
            ClaimActionRequestDto::RestoreOriginal => self.restoring_original(entry),
            ClaimActionRequestDto::Undo => self.undo(review, lesson_id, entry).await,
        }
    }

    /// Undo the claim's last user event, or the machine's removal of its
    /// sentence. An edit or a removal a pass already carried into the draft
    /// cannot be taken back from the log alone (§5.1).
    async fn undo(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
    ) -> Result<Resolution> {
        let ordinal = entry.undo_target().ok_or_else(|| {
            AppError::InvalidState("There is nothing to undo on this statement.".into())
        })?;
        let undone = entry
            .events()
            .iter()
            .find(|record| record.ordinal == ordinal)
            .ok_or_else(|| AppError::InternalError("Undo target not in the claim's log".into()))?;
        let removal_applied = || async {
            let text = self.field_text(review, lesson_id, entry).await?;
            let sentence = located_quote(entry.location(), entry.statement(), &text);
            Ok::<bool, AppError>(!text.contains(&sentence))
        };
        let published = review.state == LessonReviewState::Ready;
        let changed_draft = !published
            && match &undone.event {
                ClaimEvent::UserAction {
                    action: UserAction::Edit { .. },
                    ..
                } => entry.events().iter().any(|record| {
                    record.ordinal > ordinal
                        && matches!(record.event, ClaimEvent::Superseded { .. })
                }),
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    ..
                }
                | ClaimEvent::RepairAttempted { .. } => removal_applied().await?,
                ClaimEvent::UserAction {
                    action: UserAction::PutBack { .. } | UserAction::RestoreOriginal { .. },
                    ..
                } => {
                    self.restore_applied(review, lesson_id, entry, &undone.event)
                        .await?
                }
                _ => false,
            };
        if changed_draft {
            return Err(AppError::InvalidState(published_edit::UNDO_BLOCKED.into()));
        }
        let forgotten = match &undone.event {
            ClaimEvent::UserAction { action, .. } => match action {
                UserAction::Confirm { .. } => {
                    Some((LabelSource::UserConfirm, TrustVerdict::Confirm))
                }
                UserAction::Edit { .. } => Some((LabelSource::UserEdit, TrustVerdict::Edit)),
                UserAction::Remove => Some((LabelSource::UserRemove, TrustVerdict::Remove)),
                _ => None,
            },
            _ => None,
        };
        // The example and calibration forget after the Undo is on the
        // ledger: a bar moving back re-decides claims, which advances the
        // review's revision.
        Ok(Resolution {
            event: ClaimEvent::Undone {
                event_ordinal: ordinal,
            },
            label: None,
            unlabel: forgotten.map(|(source, _)| source),
            trust: None,
            forget: forgotten.map(|(_, verdict)| verdict),
        })
    }

    /// The claim's sentence in the draft (claim sources design §18.3), or the
    /// stored quote when the draft cannot be read, as for a published lesson.
    async fn bound_quote(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
    ) -> String {
        match self.field_text(review, lesson_id, entry).await {
            Ok(text) => located_quote(entry.location(), entry.statement(), &text),
            Err(_) => entry.location().quote.clone(),
        }
    }

    async fn field_text(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
    ) -> Result<String> {
        if review.state == LessonReviewState::Ready {
            if !in_section_body(entry) {
                return Err(AppError::InvalidInput(ONLY_SECTION.into()));
            }
            return self
                .store
                .block_body(lesson_id, entry.location().unit_index)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound("The statement's section is not in the lesson.".into())
                });
        }
        let draft = self
            .drafts
            .lesson_draft(&review.job_id, lesson_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(
                    "The lesson's draft is no longer saved; prepare it again.".into(),
                )
            })?;
        let candidate: Value = serde_json::from_str(&draft.candidate)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        let location = entry.location();
        crate::content_verification::field_text(&candidate, location.unit_index, &location.field)
            .ok_or_else(|| {
                AppError::NotFound("The statement's section is not in the draft.".into())
            })
    }
}
