//! The user's actions on a lesson's claims (§5.1): each is one event in the
//! claim's log, written under the review's revision so an action and the
//! running job never overwrite each other (§9). Edits and removals reach the
//! draft through the next pass's review step. Every verdict also feeds the
//! checker-accuracy examples (§15) and trust calibration (source provenance
//! §13). Design: docs/design/2026-10-16-lesson-claim-review.md.

use std::collections::HashSet;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimActionKind, ClaimCounts, ClaimEvent, ClaimId, ClaimLedgerEntry, EventProvenance,
    HumanBinding, Ledger, LessonReview, LessonReviewState, Revision, TrustVerdict, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;
use sqlx::SqlitePool;

use super::actions::{in_section_body, refusal, ONLY_SECTION};
use super::checker_examples::{CheckerExamples, LabelSource};
use super::dto::ClaimActionRequestDto;
use super::published_edit::{self, PublishedEdit};
use super::published_remove;
use super::published_restore;
use super::put_back;
use super::repository::{
    append_in, bump_revision, check_revision, revision_conflict, ClaimReviewRepository,
};
use super::source_deleted::deleted_versions;
use super::trust_changed::{forget_verdict, record_verdict, Redecided};

mod question_edit;
mod section_edit;
use crate::content_verification::located_quote;
use crate::curriculum::{LearningGenerationJob, LearningGenerationJobStatus};
use crate::curriculum_repository::LearningCurriculumRepository;
use crate::persistence::{db, now};
use crate::plan_dto::LearningGenerationJobActionRequestDto;
use crate::references::trust::outcomes::VerdictScope;
use lattice_core::shared::text::plain_text;
pub use question_edit::{QuestionEdit, QuestionEditOutcome, QuestionText};
pub use section_edit::{SectionEdit, SectionEditOutcome};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
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
    pool: SqlitePool,
    store: ClaimReviewRepository,
    pub(super) examples: CheckerExamples,
    drafts: LearningCurriculumRepository,
    checker_model: Option<String>,
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
struct Planned<'a> {
    entry: &'a ClaimLedgerEntry,
    resolution: Resolution,
}

impl ClaimReviewService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            store: ClaimReviewRepository::new(pool.clone()),
            examples: CheckerExamples::new(pool.clone()),
            drafts: LearningCurriculumRepository::new(pool.clone()),
            checker_model: None,
            pool,
        }
    }

    /// The configured Learning checker, read without loading a model. The
    /// review screen and Resume must use the same current configuration.
    pub fn with_checker_model(mut self, model: Option<String>) -> Self {
        self.checker_model = model;
        self
    }

    pub(super) fn pool(&self) -> &SqlitePool {
        &self.pool
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
            let message = plain(first);
            return Err(AppError::InvalidState(if refused.len() == chosen.len() {
                format!("None of these can be {verb}: {message}")
            } else {
                format!(
                    "{} of these can't be {verb}: {message} Nothing was changed.",
                    refused.len()
                )
            }));
        }
        self.carry_out(&review, lesson_id, ledger.revision, planned)
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

    /// Writes the planned claims' events and text changes in one
    /// transaction under `revision`, then records what each gave the checker
    /// examples and trust calibration. Returns the review's revision after.
    async fn carry_out(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        revision: Revision,
        planned: Vec<Planned<'_>>,
    ) -> Result<i64> {
        let written = self.commit(review, lesson_id, revision, &planned).await?;
        let mut redecided = false;
        for item in planned {
            redecided |= self.settle(review, lesson_id, item).await?;
        }
        // A bar the verdicts moved may have re-decided this lesson's claims.
        if redecided {
            return Ok(self.store.load(lesson_id).await?.revision.0);
        }
        Ok(written.0)
    }

    /// The planned claims' events, and in a published lesson the edits and
    /// removals of its text, written in one transaction that checks the
    /// review's revision and advances it once.
    async fn commit(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        revision: Revision,
        planned: &[Planned<'_>],
    ) -> Result<Revision> {
        let published = review.state == LessonReviewState::Ready;
        let user = provenance(review);
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        check_revision(&mut tx, lesson_id, revision).await?;
        let mut ended: HashSet<ClaimId> = HashSet::new();
        for item in planned {
            let id = item.entry.claim_id();
            if ended.contains(id) {
                continue;
            }
            match &item.resolution.event {
                ClaimEvent::UserAction {
                    action: UserAction::Edit { before, after },
                    ..
                } if published => {
                    ended.extend(
                        published_edit::apply_in(
                            &mut tx,
                            PublishedEdit {
                                lesson_id,
                                claim_id: id,
                                before,
                                after,
                                user: user.clone(),
                            },
                        )
                        .await?,
                    );
                }
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    ..
                } if published => {
                    ended.extend(published_remove::remove_in(&mut tx, lesson_id, id, &user).await?);
                }
                event @ ClaimEvent::UserAction {
                    action: UserAction::PutBack { .. } | UserAction::RestoreOriginal { .. },
                    ..
                } if published => {
                    ended.extend(
                        published_restore::apply_in(&mut tx, lesson_id, id, event, &user).await?,
                    );
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
                    ended.extend(published_restore::undo_in(&mut tx, lesson_id, id, &user).await?);
                }
                event => {
                    append_in(&mut tx, lesson_id, id, user.clone(), event.clone()).await?;
                }
            }
        }
        bump_revision(&mut tx, lesson_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(Revision(revision.0 + 1))
    }

    /// What a written claim gives the checker examples and trust
    /// calibration; whether it moved a bar that re-decided this lesson's
    /// claims.
    async fn settle(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        item: Planned<'_>,
    ) -> Result<bool> {
        let Planned { entry, resolution } = item;
        if let Some(source) = resolution.unlabel {
            self.examples.forget(&entry.claim_id().0, source).await?;
        }
        if let Some(label) = resolution.label {
            let shown = self.store.shown_passages(lesson_id).await?;
            let passages = shown.get(entry.claim_id()).cloned().unwrap_or_default();
            self.examples
                .record_human_verdict(&review.program_id, lesson_id, entry, &passages, label)
                .await?;
        }
        let scope = VerdictScope {
            program_id: &review.program_id,
            lesson_id,
        };
        let mut redecided = Redecided::default();
        if let Some(verdict) = resolution.trust {
            redecided = record_verdict(&self.pool, scope, entry, verdict).await?;
        }
        if let Some(verdict) = resolution.forget {
            redecided = forget_verdict(&self.pool, &entry.claim_id().0, verdict).await?;
        }
        Ok(redecided.lessons.iter().any(|lesson| lesson == lesson_id))
    }

    /// Whether claim activity or a different checker gives verification new
    /// work since the lesson stopped.
    pub async fn changed_since_stop(&self, lesson_id: &str) -> Result<bool> {
        let ledger = self.store.load(lesson_id).await?;
        self.changed_in(&ledger).await
    }

    /// `changed_since_stop` over a loaded ledger. A different checker counts
    /// even when the user has not acted on a claim card.
    pub(super) async fn changed_in(&self, ledger: &Ledger) -> Result<bool> {
        let Some(review) = &ledger.review else {
            return Ok(false);
        };
        if !matches!(
            review.state,
            LessonReviewState::NeedsReview | LessonReviewState::Stopped
        ) {
            return Ok(false);
        }
        if self
            .checker_model
            .as_ref()
            .is_some_and(|model| model != &review.checker_model)
        {
            return Ok(true);
        }
        let stopped_at = self
            .store
            .review_written_at(&ledger.lesson_id)
            .await?
            .unwrap_or(i64::MAX);
        Ok(changed_since(ledger, stopped_at))
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
                let deleted = deleted_versions(&self.pool, &review.program_id).await?;
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
                } else if text.matches(before.as_str()).count() == 1 {
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
                let sentence = located_quote(entry.location(), entry.statement(), &text);
                if !whole_sentence(&text, &sentence) {
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

/// The event of a user's `action`, the accuracy label it gives and the
/// verdict calibration records.
fn user_resolution(
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

/// The text of a refusal as the user reads it, without the error's kind.
fn plain(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(message)
        | AppError::InvalidState(message)
        | AppError::NotFound(message) => message.clone(),
        other => other.to_string(),
    }
}

/// Why the claim's state does not allow `action`.
fn not_now(action: ClaimActionKind) -> &'static str {
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

fn action_kind(action: &ClaimActionRequestDto) -> ClaimActionKind {
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

/// Whether the user acted on any claim at or after `at` (§6.2), a trust
/// change re-decided one (source provenance §18.7: an override or a moved
/// bar is the user's doing too), or a claim was checked outside the
/// preparation job (against a source the user added or attached, or by an
/// audit the user ran).
pub fn changed_since(ledger: &Ledger, at: i64) -> bool {
    ledger.claims.iter().any(|entry| {
        entry.events().iter().any(|record| {
            let changed = record.provenance.actor == Actor::User
                || matches!(
                    record.event,
                    ClaimEvent::StandingAssessed { cause: Some(_), .. }
                )
                || (matches!(record.event, ClaimEvent::Checked(_))
                    && record.provenance.job_id.is_none());
            changed && record.provenance.at >= at
        })
    })
}

/// Whether `quote` is a whole sentence of `text`: it ends a sentence and
/// starts where one starts, so removing it leaves the rest intact (§5.1).
fn whole_sentence(text: &str, quote: &str) -> bool {
    let quote = quote.trim();
    let Some(start) = text.find(quote).filter(|_| !quote.is_empty()) else {
        return false;
    };
    let ends = quote
        .trim_end_matches(['"', '\'', ')', '”', '’'])
        .ends_with(['.', '!', '?']);
    let starts = text
        .get(..start)
        .map(str::trim_end)
        .is_some_and(|before| before.is_empty() || before.ends_with(['.', '!', '?', ':', '\n']));
    ends && starts
}
