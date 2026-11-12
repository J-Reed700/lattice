//! A lesson's claim review as the panel shows it (§7.3): the ledger, with
//! the unit and source titles each card names, the passages its latest check
//! compared, what research and repair did, the user's verdict, each
//! checker's latest verdict (the local checker's and the Verifier's, side by
//! side, §14.5), and the actions the claim allows now.

mod presentation;
mod support;

#[cfg(test)]
pub(super) use presentation::presentation;

use std::collections::HashMap;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    AttachOutcome, Check, CheckResult, Checker, ClaimActionKind, ClaimCounts, ClaimEvent, ClaimId,
    ClaimLedgerEntry, ClaimLocation, ClaimState, EvidenceSeen, Ledger, LessonReview,
    LessonReviewState, TrustAssessment,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::http::host_of;
use lattice_core::shared::text::plain_text;
use serde_json::Value;

use self::support::SupportContext;
use super::actions::offered;
use super::dto::{
    CheckResultDto, CheckerVerdictDto, ClaimEventDto, ClaimEventPayloadDto,
    ClaimEvidencePassageDto, ClaimLocationDto, ClaimReviewItemDto, HumanVerdictDto,
    LessonClaimReviewDto, PutBackPlaceDto, RepairAttemptDto, ResearchAttemptDto, ReviewSectionDto,
    RewrittenDto, SourceAddedByDto, SourceRefDto, StopReasonDto,
};
use super::put_back::{self, Lesson};
use super::repository::{blocks, ShownPassage};
use super::service::ClaimReviewService;
use super::source_deleted;
use super::teaching_claims::name_claims;
use super::teaching_findings::TeachingFindingsRepository;
use crate::content_verification::{field_text, located};
use crate::dto::LearningSourceKind;
use crate::lesson_evidence::LearningEvidencePassageDto;
use crate::persistence::db;
use crate::reference_collection::PassageKind;
use crate::references::trust::dto_map;
use crate::repository::LearningRepository;
use crate::source_library::{workspace_in, LearningSourceLibraryRepository};
use lattice_core::shared::persistence::Transactions;

/// A stored location's quote and span narrowed to the claim's sentence in
/// the lesson's current text; as stored when its field is gone.
fn located_in(
    units: &Value,
    location: &ClaimLocation,
    statement: &str,
) -> (String, Option<(usize, usize)>) {
    match field_text(units, location.unit_index, &location.field) {
        Some(text) => located(location, statement, &text),
        None => (location.quote.clone(), location.span),
    }
}

/// What a card says about a source version.
struct SourceFacts {
    source_id: String,
    title: String,
    url: Option<String>,
    added_by: SourceAddedByDto,
    library_kind: LearningSourceKind,
}

impl SourceFacts {
    /// The host of the version's address; `None` for a file or pasted text.
    fn host(&self) -> Option<String> {
        host_of(self.url.as_deref()?)
    }

    fn reference(&self, source_version_id: &str) -> SourceRefDto {
        SourceRefDto {
            source_id: self.source_id.clone(),
            source_version_id: source_version_id.into(),
            title: self.title.clone(),
            url: self.url.clone(),
            added_by: self.added_by,
        }
    }
}

/// A passage by its source version and byte span.
type PassageKey = (String, usize, usize);

/// A passage's text and how it was found.
struct PassageText {
    text: String,
    retrieval_kind: String,
}

fn key(seen: &EvidenceSeen) -> PassageKey {
    (
        seen.source_version_id.clone(),
        seen.start_byte,
        seen.end_byte,
    )
}

/// The claim's latest check by the Verifier (`verifier`) or the local
/// checker, with when it was recorded.
fn latest_check(entry: &ClaimLedgerEntry, verifier: bool) -> Option<(&Check, i64)> {
    entry.events().iter().rev().find_map(|record| {
        let check = match &record.event {
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } => check,
            _ => return None,
        };
        (matches!(check.checker, Checker::Verifier { .. }) == verifier)
            .then_some((check, record.provenance.at))
    })
}

impl ClaimReviewService {
    /// The review of `lesson_id` in `program_id`; `None` before the
    /// lesson's first verification pass.
    pub async fn review(
        &self,
        program_id: &str,
        lesson_id: &str,
    ) -> Result<Option<LessonClaimReviewDto>> {
        let mut ledger = self.store().load(lesson_id).await?;
        if source_deleted::reopen_lost(&self.database, program_id, &ledger).await? {
            ledger = self.store().load(lesson_id).await?;
        }
        let Some(review) = ledger
            .review
            .clone()
            .filter(|review| review.program_id == program_id)
        else {
            return Ok(None);
        };
        let changed_since_stop = self.changed_in(&ledger).await?;
        let sources = self.sources(program_id).await?;
        let units = self.units(&review, program_id, lesson_id).await?;
        let titles = unit_titles(&units);
        let shown = self.store().shown_passages(lesson_id).await?;
        let texts = self.passage_texts(program_id, &ledger, &shown).await?;
        let standings = self.standings(program_id, &ledger).await?;
        let pass = self
            .drafts()
            .job(&review.job_id)
            .await
            .ok()
            .and_then(|job| job.activity)
            .map_or(0, |activity| activity.verification_pass);
        let published = review.state == LessonReviewState::Ready;
        let claims = ledger
            .claims
            .iter()
            .map(|entry| {
                let passages = shown.get(entry.claim_id()).map_or(&[][..], Vec::as_slice);
                let context = SupportContext {
                    shown: passages,
                    texts: &texts,
                    sources: &sources,
                    standings: &standings,
                };
                item(
                    entry,
                    &ledger.claims,
                    &context,
                    (&units, titles.as_slice()),
                    published,
                )
            })
            .collect();
        let mut teaching_findings = TeachingFindingsRepository::new(self.database.clone())
            .list(lesson_id)
            .await?;
        if published {
            let bodies = blocks::section_bodies(&self.database, lesson_id).await?;
            name_claims(&ledger, &bodies, &mut teaching_findings);
        }
        Ok(Some(LessonClaimReviewDto {
            lesson_id: ledger.lesson_id.clone(),
            program_id: review.program_id.clone(),
            state: review.state.into(),
            revision: ledger.revision.0,
            counts: ClaimCounts::tally(ledger.claims.iter()).into(),
            pass,
            stop_reason: review.stop_reason.as_ref().map(StopReasonDto::from),
            changed_since_stop,
            claims,
            sections: titles
                .iter()
                .take(block_count(&units))
                .enumerate()
                .map(|(index, title)| ReviewSectionDto {
                    index,
                    title: title.clone(),
                })
                .collect(),
            teaching_findings,
        }))
    }

    /// The text of every passage a card shows, and of every passage a
    /// claim's latest checks compared: from the cards' passages, else read
    /// back from the source version (a check compared it, no card shows it
    /// now).
    async fn passage_texts(
        &self,
        program_id: &str,
        ledger: &Ledger,
        shown: &HashMap<ClaimId, Vec<ShownPassage>>,
    ) -> Result<HashMap<PassageKey, PassageText>> {
        let mut texts: HashMap<PassageKey, PassageText> = shown
            .values()
            .flatten()
            .map(|passage| {
                (
                    (
                        passage.source_version_id.clone(),
                        passage.start_byte,
                        passage.end_byte,
                    ),
                    PassageText {
                        text: passage.text.clone(),
                        retrieval_kind: passage.retrieval_kind.clone(),
                    },
                )
            })
            .collect();
        let compared: Vec<&EvidenceSeen> = ledger
            .claims
            .iter()
            .flat_map(|entry| [latest_check(entry, false), latest_check(entry, true)])
            .flatten()
            .flat_map(|(check, _)| &check.evidence)
            .collect();
        let repo = LearningSourceLibraryRepository::new(self.database.clone());
        let mut versions: HashMap<String, Option<String>> = HashMap::new();
        for seen in compared {
            if texts.contains_key(&key(seen)) {
                continue;
            }
            if !versions.contains_key(&seen.source_version_id) {
                let text = repo
                    .version_text(program_id, &seen.source_version_id)
                    .await?;
                versions.insert(seen.source_version_id.clone(), text);
            }
            let slice = versions
                .get(&seen.source_version_id)
                .and_then(Option::as_deref)
                .and_then(|text| seen.text_in(text));
            if let Some(text) = slice {
                texts.insert(
                    key(seen),
                    PassageText {
                        text: text.to_owned(),
                        retrieval_kind: PassageKind::SavedVersion.label().into(),
                    },
                );
            }
        }
        Ok(texts)
    }

    /// Every version of the course's sources, deleted ones included: a card
    /// still names a source the user removed.
    async fn sources(&self, program_id: &str) -> Result<HashMap<String, SourceFacts>> {
        let mut read = self.database.begin_read().await.map_err(db)?;
        let workspace = workspace_in(&mut read, program_id, true).await?;
        read.commit().await.map_err(db)?;
        Ok(workspace
            .sources
            .into_iter()
            .flat_map(|source| {
                let added_by = source.added_by;
                let library_kind = source.kind;
                let source_id = source.id;
                source.versions.into_iter().map(move |version| {
                    (
                        version.id,
                        SourceFacts {
                            source_id: source_id.clone(),
                            title: version.title,
                            url: version.resolved_url,
                            added_by,
                            library_kind: library_kind.clone(),
                        },
                    )
                })
            })
            .collect())
    }

    /// The lesson's blocks and questions: the draft that waits for review,
    /// else the published lesson; `Null` when neither is found.
    pub(super) async fn units(
        &self,
        review: &LessonReview,
        program_id: &str,
        lesson_id: &str,
    ) -> Result<Value> {
        if review.state != LessonReviewState::Ready {
            if let Some(draft) = self
                .drafts()
                .lesson_draft(&review.job_id, lesson_id)
                .await?
            {
                return serde_json::from_str(&draft.candidate)
                    .map_err(|error| AppError::Serialization(error.to_string()));
            }
        }
        let program = LearningRepository::new(self.database.clone())
            .get(program_id)
            .await?;
        program
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .find(|lesson| lesson.id == lesson_id)
            .map_or(Ok(Value::Null), serde_json::to_value)
            .map_err(|error| AppError::Serialization(error.to_string()))
    }
}

/// The title of each unit, blocks then questions.
fn unit_titles(units: &Value) -> Vec<String> {
    let titles = |field: &str, key: &str| -> Vec<String> {
        units
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|unit| {
                unit.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    };
    let mut titles_of = titles("blocks", "title");
    titles_of.extend(titles("questions", "prompt"));
    titles_of
}

/// A compared passage as a card shows it, with its level for the claim's
/// subject when it is one of the supports the gate weighed (`trust`).
fn evidence_passage(
    source_version_id: &str,
    text: &str,
    (start_byte, end_byte): (usize, usize),
    retrieval_kind: &str,
    sources: &HashMap<String, SourceFacts>,
    trust: Option<&TrustAssessment>,
) -> ClaimEvidencePassageDto {
    let facts = sources.get(source_version_id);
    ClaimEvidencePassageDto {
        passage: LearningEvidencePassageDto {
            source_version_id: source_version_id.into(),
            title: facts.map(|facts| facts.title.clone()).unwrap_or_default(),
            url: facts.and_then(|facts| facts.url.clone()),
            text: text.into(),
            start_byte,
            end_byte,
            retrieval_kind: retrieval_kind.into(),
            trust: trust.and_then(|trust| {
                trust
                    .supports
                    .iter()
                    .find(|support| support.source_version_id == source_version_id)
                    .map(|support| dto_map::support_trust(&trust.profile, support))
            }),
        },
        added_by: facts.map(|facts| facts.added_by),
    }
}

/// One checker's column of a card: its latest verdict and what it compared.
fn checker_verdict(
    entry: &ClaimLedgerEntry,
    verifier: bool,
    sources: &HashMap<String, SourceFacts>,
    texts: &HashMap<PassageKey, PassageText>,
) -> Option<CheckerVerdictDto> {
    let (check, at) = latest_check(entry, verifier)?;
    let quoted_source = check.result.support().and_then(|(source_version_id, _)| {
        sources
            .get(source_version_id)
            .map(|facts| facts.reference(source_version_id))
    });
    Some(CheckerVerdictDto {
        checker: (&check.checker).into(),
        result: CheckResultDto::from(&check.result),
        reason: check.reason.clone(),
        quoted_source,
        passages: check
            .evidence
            .iter()
            .filter_map(|seen| {
                let found = texts.get(&key(seen))?;
                Some(evidence_passage(
                    &seen.source_version_id,
                    &found.text,
                    (seen.start_byte, seen.end_byte),
                    &found.retrieval_kind,
                    sources,
                    None,
                ))
            })
            .collect(),
        at,
    })
}

/// One claim card.
fn item(
    entry: &ClaimLedgerEntry,
    claims: &[ClaimLedgerEntry],
    context: &SupportContext<'_>,
    (units, titles): (&Value, &[String]),
    published: bool,
) -> ClaimReviewItemDto {
    let SupportContext {
        shown,
        texts,
        sources,
        ..
    } = *context;
    let location = entry.location();
    let reference = |id: &String| sources.get(id).map(|facts| facts.reference(id));
    let mut research = Vec::new();
    let mut repairs = Vec::new();
    for record in entry.events() {
        match &record.event {
            ClaimEvent::Researched {
                queries,
                sources_added,
                passages_found,
                ..
            } => research.push(ResearchAttemptDto {
                queries: queries.clone(),
                sources_added: sources_added.iter().filter_map(reference).collect(),
                passages_found: *passages_found,
                at: record.provenance.at,
            }),
            ClaimEvent::RepairAttempted {
                attempt,
                escalated,
                before,
                after,
                outcome,
                ..
            } => repairs.push(RepairAttemptDto {
                attempt: *attempt,
                escalated: *escalated,
                before: before.clone(),
                after: after.clone(),
                outcome: outcome.into(),
                at: record.provenance.at,
            }),
            _ => {}
        }
    }
    let (reason, supporting_quote, human) = match entry.state() {
        ClaimState::Open { reason, .. } | ClaimState::Queued { reason, .. } => {
            (Some(reason.clone()), None, None)
        }
        ClaimState::ResolvedByEvidence {
            supporting_quote, ..
        } => (None, Some(supporting_quote.clone()), None),
        ClaimState::Confirmed { at, note, bound } => (
            failed_reason(entry),
            None,
            Some(HumanVerdictDto {
                at: *at,
                note: note.clone(),
                bound_sources: bound
                    .source_version_ids
                    .iter()
                    .filter_map(reference)
                    .collect(),
            }),
        ),
        _ => (failed_reason(entry), None, None),
    };
    // A section that went out of the lesson moved the ones after it: the
    // title at a taken-out statement's old place is another section's.
    let unit_title = if presentation::taken_out(entry) {
        String::new()
    } else {
        titles.get(location.unit_index).cloned().unwrap_or_default()
    };
    let presentation = presentation::presentation(entry, units, &unit_title);
    let (support, also_supported) = support::supports(entry, context);
    let available = offered(entry, published);
    let put_place = available
        .contains(&ClaimActionKind::PutBack)
        .then(|| put_back::place(&Lesson { units, published }, claims, entry));
    let place = put_place.as_ref().map(|found| match found {
        Some(_) => PutBackPlaceDto::Known,
        None => PutBackPlaceDto::Choose,
    });
    let put_back_unit = put_place.flatten().map(|found| found.unit_index);
    let rewritten = entry
        .rewrite()
        .filter(|_| available.contains(&ClaimActionKind::RestoreOriginal))
        .map(|(original, rewrite)| RewrittenDto {
            original: plain_text(original),
            rewrite: plain_text(rewrite),
            why: failed_reason(entry),
        });
    ClaimReviewItemDto {
        claim_id: entry.claim_id().0.clone(),
        location: {
            // The claim's own sentence, as the next pass will locate it
            // (claim sources design §18.3).
            let (quote, span) = located_in(units, location, entry.statement());
            ClaimLocationDto {
                unit_index: location.unit_index,
                unit_kind: location.unit_kind.into(),
                unit_title,
                field: location.field.clone(),
                quote,
                span,
            }
        },
        statement: entry.statement().to_owned(),
        state: entry.state().into(),
        verdict_kind: entry.latest_failure().map(Into::into),
        reason,
        evidence: shown
            .iter()
            .map(|passage| {
                evidence_passage(
                    &passage.source_version_id,
                    &passage.text,
                    (passage.start_byte, passage.end_byte),
                    &passage.retrieval_kind,
                    sources,
                    entry.trust(),
                )
            })
            .collect(),
        supporting_quote,
        research,
        repairs,
        // The ledger keeps whether an example ran, not its run; the run is
        // in the lesson's report, which a waiting lesson does not have yet.
        executions: Vec::new(),
        human,
        available_actions: available.into_iter().map(Into::into).collect(),
        history: entry
            .events()
            .iter()
            .filter_map(|record| {
                let mut event = ClaimEventDto::of(record)?;
                if let (
                    ClaimEvent::Extracted {
                        location,
                        statement,
                    },
                    ClaimEventPayloadDto::Extracted { quote, span, .. },
                ) = (&record.event, &mut event.event)
                {
                    (*quote, *span) = located_in(units, location, statement);
                }
                Some(event)
            })
            .collect(),
        local_check: checker_verdict(entry, false, sources, texts),
        verifier_check: checker_verdict(entry, true, sources, texts),
        sufficiency: entry
            .trust()
            .map(|trust| dto_map::sufficiency(&trust.profile, &trust.decision, trust.available)),
        presentation,
        support,
        also_supported,
        tried: presentation::tried(entry, sources),
        place,
        put_back_unit,
        rewritten,
    }
}

fn block_count(units: &Value) -> usize {
    units
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// The reason of the claim's latest failing check, kept after it closes.
fn failed_reason(entry: &ClaimLedgerEntry) -> Option<String> {
    entry.events().iter().rev().find_map(|record| {
        let check = match &record.event {
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } => check,
            _ => return None,
        };
        matches!(check.result, CheckResult::Failed { .. }).then(|| check.reason.clone())
    })
}
