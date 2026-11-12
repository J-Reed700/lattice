//! Attaching evidence to a claim (§5.4): the user's link, file, pasted text or
//! saved passage becomes a source of the course, and the claim, with every
//! other open claim of the lesson the source mentions, is checked against that
//! source alone by the publication judge. A source added to the lesson as a
//! whole is checked against every open claim of the lesson. Each event is
//! appended at the ledger's revision of the moment, after re-reading that the
//! claim still takes it (§9).

mod acquire;
mod check;
pub mod failures;
pub mod report;
#[cfg(test)]
mod tests;

use lattice_core::application::ports::{
    ClaimReviewStore, LLMPort, LibraryPassagesPort, ReferenceSnapshotPort, WebServiceTrait,
};
use lattice_core::domain::learning::claim_review::{
    Actor, AttachOutcome, Check, ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimState, EventProvenance, Ledger, Revision, TrustVerdict,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::http::host_of;
use lattice_core::shared::persistence::{Database, Transactions};
use std::sync::Arc;

use self::acquire::Acquired;
pub(crate) use self::acquire::{capture_page, PageCapture};
use self::report::{
    ClaimAttachProgressDto, ClaimAttachReportDto, OtherClaimsCheckedDto, PutBackOutcomeDto,
};
use super::checker_examples::CheckerExamples;
use super::count;
use super::dto::AttachEvidenceDto;
use super::repository::{ClaimReviewRepository, ShownPassage};
use super::trust_changed::{redecide_kinds, Redecided};
use crate::dto::LearningSourceDto;
use crate::persistence::db;
use crate::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::references::course_index::{CourseIndexes, CourseSearch};
use crate::references::trust::outcomes::{self, VerdictScope};
use crate::source_library::LearningSourceLibraryRepository;

/// What the user attaches (§5.1 "Attach evidence").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    SavedPassage {
        source_version_id: String,
        start_byte: usize,
        end_byte: usize,
    },
    Link {
        url: String,
    },
    Document {
        document_id: String,
    },
    Text {
        title: String,
        text: String,
    },
}

impl From<AttachEvidenceDto> for Evidence {
    fn from(evidence: AttachEvidenceDto) -> Self {
        match evidence {
            AttachEvidenceDto::AttachSavedSource {
                source_version_id,
                start_byte,
                end_byte,
            } => Self::SavedPassage {
                source_version_id,
                start_byte,
                end_byte,
            },
            AttachEvidenceDto::AttachLink { url } => Self::Link { url },
            AttachEvidenceDto::AttachDocument { document_id } => Self::Document { document_id },
            AttachEvidenceDto::AttachText { title, text } => Self::Text { title, text },
        }
    }
}

/// One attach, as the command received it.
pub struct AttachRequest<'a> {
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub claim_id: &'a ClaimId,
    pub expected_revision: Revision,
    pub evidence: Evidence,
}

/// One source added to a lesson as a whole, as the command received it.
pub struct LessonSourceRequest<'a> {
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub expected_revision: Revision,
    pub evidence: Evidence,
}

/// What the attach works with; the command assembles it from the container.
pub struct AttachDeps<'a> {
    pub database: &'a Database,
    pub store: &'a dyn ClaimReviewStore,
    pub web: &'a dyn WebServiceTrait,
    pub snapshots: &'a dyn ReferenceSnapshotPort,
    /// Where library documents are captured from.
    pub documents: &'a dyn LibraryPassagesPort,
    /// Every course's evidence index, which the attached source is chunked
    /// into and checked through, never waiting for embedding; `None`
    /// searches the course's sources by their words.
    pub courses: Option<&'a Arc<CourseIndexes>>,
    /// The Learning model, pinned for this command.
    pub llm: &'a dyn LLMPort,
    /// The lesson's preparation job, when one is running.
    pub job_id: Option<String>,
}

impl AttachDeps<'_> {
    fn provenance(&self, actor: Actor) -> EventProvenance {
        EventProvenance {
            actor,
            at: chrono::Utc::now().timestamp_millis(),
            job_id: self.job_id.clone(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            checker_model: Some(self.llm.model_name().to_owned()),
        }
    }
}

/// Which other claims of the lesson are checked against the new source.
pub enum Others<'a> {
    /// Every open claim of the lesson.
    Lesson,
    /// These claims, when they are open.
    Only(&'a [ClaimId]),
}

/// The claims `check_others` takes: open ones in scope, other than the one
/// the evidence was attached to.
pub(super) struct OtherClaims<'a> {
    pub scope: Others<'a>,
    pub except: Option<&'a ClaimId>,
}

impl OtherClaims<'_> {
    pub(super) fn take(&self, entry: &ClaimLedgerEntry) -> bool {
        let in_scope = match self.scope {
            Others::Lesson => true,
            Others::Only(ids) => ids.contains(entry.claim_id()),
        };
        in_scope && self.except != Some(entry.claim_id()) && open_for_recheck(entry.state())
    }
}

/// Where the checks are, for the progress line: `done` of `total` claims.
pub(super) struct Checking<'a> {
    pub report: &'a (dyn Fn(ClaimAttachProgressDto) + Send + Sync),
    pub done: u32,
    pub total: u32,
}

impl Checking<'_> {
    /// Reports the claim about to be checked.
    fn next(&mut self, statement: &str) {
        self.total = self.total.max(self.done + 1);
        (self.report)(ClaimAttachProgressDto::Checking {
            done: self.done,
            total: self.total,
            statement: statement.to_owned(),
        });
        self.done += 1;
    }
}

/// A claim that is still open: the machine's, waiting for the user, or
/// published not verified. A claim extracted and never checked is left to the
/// pass that checks it.
pub(super) fn open_for_recheck(state: &ClaimState) -> bool {
    matches!(
        state,
        ClaimState::Open { .. } | ClaimState::Unverified { .. }
    ) || state.awaits_user()
}

fn claim<'l>(claims: &'l [ClaimLedgerEntry], claim_id: &ClaimId) -> Option<&'l ClaimLedgerEntry> {
    claims.iter().find(|entry| entry.claim_id() == claim_id)
}

/// Appends `event` to the claim at the ledger's current revision, re-reading
/// the claim first: `None` when the claim is gone or no longer `eligible`.
/// A refusal at a revision the job has moved on from is tried again on the
/// new one; a refusal at an unchanged revision is the ledger's rule and is
/// returned.
async fn append_current(
    store: &dyn ClaimReviewStore,
    lesson_id: &str,
    claim_id: &ClaimId,
    provenance: EventProvenance,
    event: ClaimEvent,
    eligible: impl Fn(&ClaimLedgerEntry) -> bool,
) -> Result<Option<Revision>> {
    loop {
        let ledger = store.load(lesson_id).await?;
        if !claim(&ledger.claims, claim_id).is_some_and(&eligible) {
            return Ok(None);
        }
        let at = ledger.revision;
        match store
            .append(lesson_id, claim_id, provenance.clone(), event.clone(), at)
            .await
        {
            Ok(revision) => return Ok(Some(revision)),
            Err(AppError::InvalidState(reason)) => {
                if store.load(lesson_id).await?.revision == at {
                    return Err(AppError::InvalidState(reason));
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// After a check is on the ledger: the card shows the passages it compared,
/// a flag the new source overturned is a labelled example of the checker's
/// accuracy (§15), and, when `count_verdict`, a supported check is a
/// calibration outcome (§13). All three land in one unit of work; the claims
/// a moved bar re-decides follow it. `flagged` is the claim as it was before
/// the check; `in_collection` says whether the source was already a
/// reference.
async fn record_check(
    deps: &AttachDeps<'_>,
    (program_id, lesson_id): (&str, &str),
    flagged: &ClaimLedgerEntry,
    passages: &[ReferencePassage],
    outcome: &AttachOutcome,
    in_collection: bool,
    count_verdict: bool,
) -> Result<Redecided> {
    let AttachOutcome::Checked(check) = outcome else {
        return Ok(Redecided::default());
    };
    let shown: Vec<ShownPassage> = passages
        .iter()
        .map(|passage| ShownPassage {
            source_version_id: passage.source_id.clone(),
            start_byte: passage.start_byte,
            end_byte: passage.end_byte,
            text: passage.text.clone(),
            retrieval_kind: passage.retrieval_kind.clone(),
        })
        .collect();
    let moved = write_check(
        deps.database,
        (program_id, lesson_id),
        flagged,
        &shown,
        check,
        in_collection,
        count_verdict,
    )
    .await?;
    redecide_kinds(deps.database, moved).await
}

/// The unit of work of [`record_check`]; returns the kinds whose bar the
/// outcome moved.
async fn write_check(
    database: &Database,
    (program_id, lesson_id): (&str, &str),
    flagged: &ClaimLedgerEntry,
    shown: &[ShownPassage],
    check: &Check,
    in_collection: bool,
    count_verdict: bool,
) -> Result<Vec<ClaimKind>> {
    let mut tx = database.begin_write().await.map_err(db)?;
    ClaimReviewRepository::show_passages_in(&mut tx, lesson_id, flagged.claim_id(), shown).await?;
    let mut moved = Vec::new();
    if check.result.passed() {
        CheckerExamples::record_evidence_added_in(
            &mut tx,
            program_id,
            lesson_id,
            flagged,
            shown,
            in_collection,
        )
        .await?;
        if count_verdict {
            moved = outcomes::record_in(
                &mut tx,
                VerdictScope {
                    program_id,
                    lesson_id,
                },
                flagged,
                TrustVerdict::EvidenceAdded,
            )
            .await?;
        }
    }
    tx.commit().await.map_err(db)?;
    Ok(moved)
}

/// The course's sources and its evidence index opened for searching: where
/// a new source's passages are found. The new source is chunked into the
/// index before it is read; its embedding is left to a background build.
pub(super) async fn course_material(
    deps: &AttachDeps<'_>,
    program_id: &str,
) -> Result<(Vec<LearningSourceDto>, Option<CourseSearch>)> {
    let sources = LearningSourceLibraryRepository::new(deps.database.clone())
        .verification_sources(program_id)
        .await?;
    let course = match deps.courses {
        Some(courses) => courses
            .search(program_id, tokio_util::sync::CancellationToken::new())
            .await
            .inspect_err(|error| tracing::warn!(%error, "Course index could not be opened"))
            .ok(),
        None => None,
    };
    Ok((sources, course))
}

/// Attaches the user's evidence to one claim and checks it for every other
/// open claim of the lesson (§5.4).
pub async fn attach(
    deps: &AttachDeps<'_>,
    request: AttachRequest<'_>,
) -> Result<ClaimAttachReportDto> {
    attach_to(deps, request, Others::Lesson, &|_| {}).await
}

/// Attaches the user's evidence to one claim, checks it for the `others`
/// in scope, and reports each stage through `progress` (UI design §5.3).
pub async fn attach_to(
    deps: &AttachDeps<'_>,
    request: AttachRequest<'_>,
    others: Others<'_>,
    progress: &(dyn Fn(ClaimAttachProgressDto) + Send + Sync),
) -> Result<ClaimAttachReportDto> {
    let report = attach_reporting(deps, request, others, progress).await;
    progress(ClaimAttachProgressDto::Done);
    report
}

/// Adds the user's evidence to the lesson as a whole and checks every open
/// claim of the lesson against it, reporting each stage through `progress`.
/// The report has no check of a claim of its own: `outcome` is null.
pub async fn attach_to_lesson(
    deps: &AttachDeps<'_>,
    request: LessonSourceRequest<'_>,
    progress: &(dyn Fn(ClaimAttachProgressDto) + Send + Sync),
) -> Result<ClaimAttachReportDto> {
    let report = lesson_source_reporting(deps, request, progress).await;
    progress(ClaimAttachProgressDto::Done);
    report
}

async fn lesson_source_reporting(
    deps: &AttachDeps<'_>,
    request: LessonSourceRequest<'_>,
    progress: &(dyn Fn(ClaimAttachProgressDto) + Send + Sync),
) -> Result<ClaimAttachReportDto> {
    let ledger = current_ledger(
        deps,
        request.program_id,
        request.lesson_id,
        request.expected_revision,
    )
    .await?;
    let others = OtherClaims {
        scope: Others::Lesson,
        except: None,
    };
    let mut checking = Checking {
        report: progress,
        done: 0,
        total: count(
            ledger
                .claims
                .iter()
                .filter(|entry| others.take(entry))
                .count(),
        ),
    };
    progress(acquiring_stage(&request.evidence));
    let source = match acquire::acquire(deps, request.program_id, &request.evidence).await? {
        Acquired::Saved(source) => source,
        Acquired::NotSaved(failure) => return Ok(report::not_saved(ledger.revision, failure)),
    };
    let version_id = source.saved.version_id.clone();
    let (sources, course) = course_material(deps, request.program_id).await?;
    let references = ReferenceCollection::searchable_now(course.as_ref(), &sources).await?;
    let (others, after) = check_others(
        deps,
        &references,
        &version_id,
        (request.program_id, request.lesson_id),
        &others,
        source.already_saved,
        &mut checking,
    )
    .await?;
    Ok(report::added_to_lesson(
        after.unwrap_or(ledger.revision),
        &source.saved,
        source.already_saved,
        source.snapshot,
        others,
    ))
}

/// The lesson's ledger, when it belongs to the course and is at the revision
/// the user acted on.
async fn current_ledger(
    deps: &AttachDeps<'_>,
    program_id: &str,
    lesson_id: &str,
    expected_revision: Revision,
) -> Result<Ledger> {
    let ledger = deps.store.load(lesson_id).await?;
    if ledger
        .review
        .as_ref()
        .is_some_and(|review| review.program_id != program_id)
    {
        return Err(AppError::NotFound("Lesson not found in this course".into()));
    }
    if ledger.revision != expected_revision {
        return Err(AppError::InvalidState(
            "The lesson's review changed; reload it and try again.".into(),
        ));
    }
    Ok(ledger)
}

/// The first stage of an attach: fetching a link, or saving anything else.
fn acquiring_stage(evidence: &Evidence) -> ClaimAttachProgressDto {
    match evidence {
        Evidence::Link { url } => ClaimAttachProgressDto::Fetching { host: host_of(url) },
        _ => ClaimAttachProgressDto::Saving,
    }
}

async fn attach_reporting(
    deps: &AttachDeps<'_>,
    request: AttachRequest<'_>,
    others: Others<'_>,
    progress: &(dyn Fn(ClaimAttachProgressDto) + Send + Sync),
) -> Result<ClaimAttachReportDto> {
    let ledger = current_ledger(
        deps,
        request.program_id,
        request.lesson_id,
        request.expected_revision,
    )
    .await?;
    let entry = claim(&ledger.claims, request.claim_id)
        .ok_or_else(|| AppError::NotFound("Statement not found in this lesson".into()))?;
    if !entry.allows(ClaimActionKind::AttachEvidence) {
        return Err(AppError::InvalidState(
            "Evidence can be added to an open or confirmed statement only.".into(),
        ));
    }
    let statement = entry.statement().to_owned();
    let others = OtherClaims {
        scope: others,
        except: Some(request.claim_id),
    };
    let mut checking = Checking {
        report: progress,
        done: 0,
        total: 1 + count(
            ledger
                .claims
                .iter()
                .filter(|entry| others.take(entry))
                .count(),
        ),
    };
    progress(acquiring_stage(&request.evidence));
    let source = match acquire::acquire(deps, request.program_id, &request.evidence).await? {
        Acquired::Saved(source) => source,
        Acquired::NotSaved(failure) => return Ok(report::not_saved(ledger.revision, failure)),
    };
    let version_id = source.saved.version_id.clone();
    let (sources, course) = course_material(deps, request.program_id).await?;
    let references = ReferenceCollection::searchable_now(course.as_ref(), &sources).await?;
    checking.next(&statement);
    let passages = match source.span {
        Some(span) => vec![check::picked_passage(&references, &version_id, span)?],
        None => check::passages_from(deps.llm, &references, &version_id, &statement).await?,
    };
    let outcome = check::judge(deps.llm, &statement, &passages).await?;
    let put_back = if entry.taken_out() {
        Some(put_back_for(deps, request.lesson_id, request.claim_id, &outcome).await?)
    } else {
        None
    };
    let event = ClaimEvent::EvidenceAttached {
        source_version_id: version_id.clone(),
        provenance: source.provenance.clone(),
        result: outcome.clone(),
    };
    let mut revision = append_current(
        deps.store,
        request.lesson_id,
        request.claim_id,
        deps.provenance(Actor::User),
        event,
        |entry| entry.allows(ClaimActionKind::AttachEvidence),
    )
    .await?
    .ok_or_else(|| {
        AppError::InvalidState(
            "The statement changed while the evidence was being checked; reload it. The source is saved in your references."
                .into(),
        )
    })?;
    let lesson = (request.program_id, request.lesson_id);
    // The user's evidence verified the claim: calibration learns that the
    // claim held (source provenance §13); a bar it moves re-decides claims.
    let redecided = record_check(
        deps,
        lesson,
        entry,
        &passages,
        &outcome,
        source.already_saved,
        true,
    )
    .await?;
    let (others, after) = check_others(
        deps,
        &references,
        &version_id,
        lesson,
        &others,
        source.already_saved,
        &mut checking,
    )
    .await?;
    if let Some(after) = after {
        revision = after;
    }
    if redecided
        .lessons
        .iter()
        .any(|lesson| lesson == request.lesson_id)
    {
        revision = deps.store.load(request.lesson_id).await?.revision;
    }
    let mut report = report::checked(
        revision,
        &source.saved,
        source.already_saved,
        &outcome,
        source.snapshot,
        others,
    );
    report.put_back = put_back;
    Ok(report)
}

/// Puts the claim's taken-out sentence back when `outcome` supports it.
async fn put_back_for(
    deps: &AttachDeps<'_>,
    lesson_id: &str,
    claim_id: &ClaimId,
    outcome: &AttachOutcome,
) -> Result<PutBackOutcomeDto> {
    let supported = matches!(
        outcome,
        AttachOutcome::Checked(check) if check.result.passed()
    );
    if !supported {
        return Ok(PutBackOutcomeDto::StaysOut);
    }
    let service = super::service::ClaimReviewService::new(deps.database.clone());
    let back = service.put_back_for_source(lesson_id, claim_id).await?;
    Ok(if back.is_some() {
        PutBackOutcomeDto::Back
    } else {
        PutBackOutcomeDto::NoPlace
    })
}

/// The lesson's other open claims in scope, checked against the new source
/// when it has a passage for them (§5.4 "Other open claims"). Returns the
/// tally and the revision after the last append.
pub(super) async fn check_others(
    deps: &AttachDeps<'_>,
    references: &ReferenceCollection<'_>,
    version_id: &str,
    (program_id, lesson_id): (&str, &str),
    others: &OtherClaims<'_>,
    in_collection: bool,
    checking: &mut Checking<'_>,
) -> Result<(OtherClaimsCheckedDto, Option<Revision>)> {
    let ledger = deps.store.load(lesson_id).await?;
    let candidates: Vec<&ClaimLedgerEntry> = ledger
        .claims
        .iter()
        .filter(|entry| others.take(entry))
        .collect();
    let mut tally = OtherClaimsCheckedDto::default();
    let mut revision = None;
    for flagged in candidates {
        let (claim_id, statement) = (flagged.claim_id(), flagged.statement());
        checking.next(statement);
        tally.tried += 1;
        let passages = check::passages_from(deps.llm, references, version_id, statement).await?;
        if passages.is_empty() {
            tally.no_passage += 1;
            continue;
        }
        let outcome = match check::judge(deps.llm, statement, &passages).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tally.tried -= 1;
                tally.stopped = Some(error.to_string());
                break;
            }
        };
        let AttachOutcome::Checked(check) = outcome else {
            tally.no_passage += 1;
            continue;
        };
        let supported = check.result.passed();
        let appended = append_current(
            deps.store,
            lesson_id,
            claim_id,
            deps.provenance(Actor::Machine),
            ClaimEvent::Checked(check.clone()),
            |entry| open_for_recheck(entry.state()),
        )
        .await?;
        match appended {
            Some(after) => {
                revision = Some(after);
                record_check(
                    deps,
                    (program_id, lesson_id),
                    flagged,
                    &passages,
                    &AttachOutcome::Checked(check),
                    in_collection,
                    false,
                )
                .await?;
                if supported {
                    tally.verified += 1;
                } else {
                    tally.not_verified += 1;
                }
            }
            // Settled meanwhile, by the user or the job: not ours to count.
            None => tally.tried -= 1,
        }
    }
    Ok((tally, revision))
}
