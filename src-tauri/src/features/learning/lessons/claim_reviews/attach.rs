//! Attaching evidence to a claim (§5.4): the user's link, file, pasted text
//! or saved passage becomes a source of the course, the claim is checked
//! against that source alone by the publication judge, and every other open
//! claim of the lesson the source mentions is checked against it too. The
//! check is recorded on the ledger (`EvidenceAttached` for the claim,
//! `Checked` for the others); a supported result resolves the claim. A
//! source the user added is never weighted differently: it is checked like
//! any other.
//!
//! Concurrency (§9): the user's `expected_revision` must be current when the
//! attach starts; fetching and checking take a while, so each event is then
//! appended at the ledger's revision of the moment, after re-reading that the
//! claim still takes it. A preparation job running meanwhile picks the
//! result up at its next sync point.

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
    Actor, AttachOutcome, CheckResult, ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimState, EventProvenance, Revision,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

use self::acquire::Acquired;
use self::report::{ClaimAttachReportDto, OtherClaimsCheckedDto};
use super::dto::ClaimActionRequestDto;
use crate::features::learning::reference_collection::ReferenceCollection;
use crate::features::learning::source_library::LearningSourceLibraryRepository;

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

impl Evidence {
    /// The evidence an attach action carries; `None` for every other action.
    pub fn of(action: &ClaimActionRequestDto) -> Option<Self> {
        Some(match action {
            ClaimActionRequestDto::AttachSavedSource {
                source_version_id,
                start_byte,
                end_byte,
            } => Self::SavedPassage {
                source_version_id: source_version_id.clone(),
                start_byte: *start_byte,
                end_byte: *end_byte,
            },
            ClaimActionRequestDto::AttachLink { url } => Self::Link { url: url.clone() },
            ClaimActionRequestDto::AttachDocument { document_id } => Self::Document {
                document_id: document_id.clone(),
            },
            ClaimActionRequestDto::AttachText { title, text } => Self::Text {
                title: title.clone(),
                text: text.clone(),
            },
            _ => return None,
        })
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

/// What the attach works with; the command assembles it from the container.
pub struct AttachDeps<'a> {
    pub pool: &'a SqlitePool,
    pub store: &'a dyn ClaimReviewStore,
    pub web: &'a dyn WebServiceTrait,
    pub snapshots: &'a dyn ReferenceSnapshotPort,
    /// Where library documents are captured from.
    pub documents: &'a dyn LibraryPassagesPort,
    /// Ranks library-captured sources by meaning, when an embedding model is
    /// loaded; keyword ranking otherwise.
    pub ranking: Option<&'a dyn LibraryPassagesPort>,
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

/// A claim that is still open: the machine's, or waiting for the user. A
/// claim extracted and never checked is left to the pass that checks it.
fn open_for_recheck(state: &ClaimState) -> bool {
    matches!(state, ClaimState::Open { .. }) || state.awaits_user()
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

/// Attaches the user's evidence to one claim and reports the result (§5.4).
pub async fn attach(
    deps: &AttachDeps<'_>,
    request: AttachRequest<'_>,
) -> Result<ClaimAttachReportDto> {
    let ledger = deps.store.load(request.lesson_id).await?;
    if ledger
        .review
        .as_ref()
        .is_some_and(|review| review.program_id != request.program_id)
    {
        return Err(AppError::NotFound("Lesson not found in this course".into()));
    }
    if ledger.revision != request.expected_revision {
        return Err(AppError::InvalidState(
            "The lesson's review changed; reload it and try again.".into(),
        ));
    }
    let entry = claim(&ledger.claims, request.claim_id)
        .ok_or_else(|| AppError::NotFound("Claim not found in this lesson".into()))?;
    if !entry.allows(ClaimActionKind::AttachEvidence) {
        return Err(AppError::InvalidState(
            "Evidence can be added to an open or confirmed claim only.".into(),
        ));
    }
    let statement = entry.statement().to_owned();
    let source = match acquire::acquire(deps, request.program_id, &request.evidence).await? {
        Acquired::Saved(source) => source,
        Acquired::NotSaved(failure) => return Ok(report::not_saved(ledger.revision, failure)),
    };
    let version_id = source.saved.version_id.clone();
    let sources = LearningSourceLibraryRepository::new(deps.pool.clone())
        .verification_sources(request.program_id)
        .await?;
    let references =
        ReferenceCollection::load(deps.pool, request.program_id, &sources, deps.ranking).await?;
    let passages = match source.span {
        Some(span) => vec![check::picked_passage(&references, &version_id, span)?],
        None => check::passages_from(deps.llm, &references, &version_id, &statement).await?,
    };
    let outcome = check::judge(deps.llm, &statement, &passages).await?;
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
            "The claim changed while the evidence was being checked; reload it. The source is saved in your references."
                .into(),
        )
    })?;
    let (others, after) = check_others(
        deps,
        &references,
        &version_id,
        request.lesson_id,
        request.claim_id,
    )
    .await?;
    if let Some(after) = after {
        revision = after;
    }
    Ok(report::checked(
        revision,
        &source.saved,
        source.already_saved,
        &outcome,
        source.snapshot,
        others,
    ))
}

/// Every other open claim of the lesson, checked against the new source
/// when it has a passage for it (§5.4 "Other open claims"). Returns the
/// tally and the revision after the last append.
async fn check_others(
    deps: &AttachDeps<'_>,
    references: &ReferenceCollection<'_>,
    version_id: &str,
    lesson_id: &str,
    attached_to: &ClaimId,
) -> Result<(OtherClaimsCheckedDto, Option<Revision>)> {
    let ledger = deps.store.load(lesson_id).await?;
    let candidates: Vec<(ClaimId, String)> = ledger
        .claims
        .iter()
        .filter(|entry| entry.claim_id() != attached_to && open_for_recheck(entry.state()))
        .map(|entry| (entry.claim_id().clone(), entry.statement().to_owned()))
        .collect();
    let mut tally = OtherClaimsCheckedDto::default();
    let mut revision = None;
    for (claim_id, statement) in candidates {
        tally.tried += 1;
        let passages = check::passages_from(deps.llm, references, version_id, &statement).await?;
        if passages.is_empty() {
            tally.no_passage += 1;
            continue;
        }
        let outcome = match check::judge(deps.llm, &statement, &passages).await {
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
        let supported = matches!(check.result, CheckResult::Supported { .. });
        let appended = append_current(
            deps.store,
            lesson_id,
            &claim_id,
            deps.provenance(Actor::Machine),
            ClaimEvent::Checked(check),
            |entry| open_for_recheck(entry.state()),
        )
        .await?;
        match appended {
            Some(after) => {
                revision = Some(after);
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
