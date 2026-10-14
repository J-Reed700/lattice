//! Resumable outline drafts. A saved draft is not an approval to teach its content.
use crate::features::learning::{
    dto::*, outline_progress::OutlineProgress, repository::LearningRepository,
};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use crate::features::learning::outline_citation_check::{CitationLog, CitationReceipts};
use crate::features::learning::outline_references::ReferencePlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningOutlineReviewStatus {
    Unchecked,
    NeedsRepair,
    Passed,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningOutlineIssueKind {
    /// A reviewer finding about the plan or an assertion in it.
    Content,
    /// Course-wide: no references could be gathered, so lessons cannot be prepared.
    References,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineIssueDto {
    /// JSON pointer into the saved candidate. Empty means the entire curriculum.
    pub path: String,
    pub kind: LearningOutlineIssueKind,
    pub claim: String,
    pub quote: String,
    pub source_id: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineReviewDto {
    pub status: LearningOutlineReviewStatus,
    pub issues: Vec<LearningOutlineIssueDto>,
    pub repair_passes: u32,
    pub note: String,
    pub updated_at: i64,
    pub content_hash: String,
    /// Citations in the saved candidate that passed exact match and coverage.
    pub citations_verified: u32,
    /// Citations removed in this operation because they did not match or cover.
    pub citations_removed: u32,
    /// Reference pages gathered automatically for this program.
    pub references_gathered: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairLearningOutlineRequestDto {
    pub program_id: String,
    pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineModuleBinding {
    pub module_id: String,
    pub lesson_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineDraft {
    pub request: GenerateLearningProgramRequestDto,
    pub candidate: Value,
    pub source_ids: Vec<String>,
    pub review: LearningOutlineReviewDto,
    #[serde(default)]
    pub completed_review: Option<crate::features::learning::outline_review_scope::ReviewReceipt>,
    /// App-owned identities for candidate positions. Kept outside model JSON
    /// so renaming/reordering the visible course cannot reattach its evidence.
    #[serde(default)]
    pub bindings: Option<Vec<OutlineModuleBinding>>,
    /// The subtopics and public searches the references were gathered for.
    #[serde(default)]
    pub reference_plan: Option<ReferencePlan>,
    /// Coverage verdicts for citations, keyed by policy, model, item and source.
    #[serde(default)]
    pub citation_receipts: CitationReceipts,
    /// Entailment verdicts for factual findings judged during repair.
    #[serde(default)]
    pub claim_receipts: BTreeMap<String, OutlineClaimReceipt>,
    /// Searches already run for a finding, by finding id, so a retry explores
    /// new queries instead of repeating.
    #[serde(default)]
    pub research_log: BTreeMap<String, Vec<String>>,
}

/// A strict-judge verdict on one factual assertion against saved passages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineClaimReceipt {
    pub verdict: crate::application::services::claim_verification::ClaimVerdict,
    pub reason: String,
    pub supporting_quote: Option<String>,
    pub checked_at: i64,
}
impl OutlineDraft {
    pub fn new(
        request: GenerateLearningProgramRequestDto,
        candidate: Value,
        sources: &[LearningSourceDto],
    ) -> Self {
        let mut draft = Self {
            request,
            candidate,
            source_ids: sources.iter().map(|s| s.id.clone()).collect(),
            completed_review: None,
            bindings: None,
            reference_plan: None,
            citation_receipts: CitationReceipts::new(),
            claim_receipts: BTreeMap::new(),
            research_log: BTreeMap::new(),
            review: LearningOutlineReviewDto {
                status: LearningOutlineReviewStatus::Unchecked,
                issues: vec![],
                repair_passes: 0,
                note: "Draft saved. Review has not completed; use Repair to resume if interrupted."
                    .into(),
                updated_at: 0,
                content_hash: String::new(),
                citations_verified: 0,
                citations_removed: 0,
                references_gathered: 0,
            },
        };
        draft.stamp();
        draft
    }

    /// Fold a citation check into the review counters. `verified` is the
    /// current count of cited items (every surviving citation has a receipt);
    /// removals accumulate over the operation.
    pub fn record_citations(&mut self, log: CitationLog) {
        self.review.citations_verified =
            crate::features::learning::outline_citation_check::cited_count(&self.candidate);
        self.review.citations_removed += log.removed();
    }

    /// Called only where the candidate and its materialized outline are saved
    /// together. Existing bindings survive later repairs of the same shape.
    pub fn bind_program(&mut self, program: &LearningProgramDto) -> Result<()> {
        if self.bindings.is_some() {
            return Ok(());
        }
        let modules = self
            .candidate
            .get("modules")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("Outline evidence has no modules.".into()))?;
        if modules.len() != program.modules.len()
            || modules
                .iter()
                .zip(&program.modules)
                .any(|(candidate, module)| {
                    candidate
                        .get("lessons")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        != Some(module.lessons.len())
                })
        {
            return Err(AppError::InvalidInput(
                "Outline evidence does not match the saved course structure.".into(),
            ));
        }
        self.bindings = Some(
            program
                .modules
                .iter()
                .map(|module| OutlineModuleBinding {
                    module_id: module.id.clone(),
                    lesson_ids: module
                        .lessons
                        .iter()
                        .map(|lesson| lesson.id.clone())
                        .collect(),
                })
                .collect(),
        );
        Ok(())
    }

    pub fn stamp(&mut self) {
        self.review.content_hash = format!(
            "{:x}",
            Sha256::digest(self.candidate.to_string().as_bytes())
        );
        self.review.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// The one registry of outline drafts being reviewed, repaired or accepted in
/// this process. Built by the composition root and handed to the commands
/// that touch a draft, so concurrent manual repair/acceptance of the same draft
/// is rejected against the same set — and a test never sees a draft another
/// test holds. Held in memory only, so restart recovery stays free of stale
/// locks; database revisions also protect every checkpoint and acceptance.
#[derive(Clone, Default)]
pub struct DraftRunRegistry {
    active: Arc<Mutex<HashSet<String>>>,
}

impl DraftRunRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(in crate::features::learning) fn acquire(&self, id: &str) -> Result<DraftRun> {
        if !self.lock().insert(id.to_owned()) {
            return Err(AppError::InvalidInput(
                "This draft is already being reviewed or repaired.".into(),
            ));
        }
        Ok(DraftRun {
            id: id.into(),
            registry: self.clone(),
        })
    }
}

pub(in crate::features::learning) struct DraftRun {
    id: String,
    registry: DraftRunRegistry,
}
impl Drop for DraftRun {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

const NO_REFERENCES: &str = "No references could be gathered. Lessons need saved references before they can be prepared; Repair will retry gathering.";

/// Gather references for a draft that has none, persisting each page with the
/// draft. Reuses the saved plan when there is one so a retry runs the same
/// public searches rather than planning again.
async fn gather_for_draft(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    web: &dyn crate::features::web::traits::WebServiceTrait,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    progress: &OutlineProgress,
) -> Result<()> {
    progress.stage(crate::features::learning::outline_progress::OutlineStage::GatheringReferences);
    let plan = match &draft.reference_plan {
        Some(plan) if plan.query_count() > 0 => plan.clone(),
        _ => {
            let plan = crate::features::learning::outline_references::plan(llm, &draft.request, sources, progress).await?;
            draft.reference_plan = Some(plan.clone());
            plan
        }
    };
    let found = crate::features::learning::outline_references::gather(llm, web, &plan, sources, progress).await?;
    let saved = found.sources.len();
    if saved == 0 {
        draft.review.note = format!("Reference gathering ran {} searches but saved no usable pages ({} searches or pages were unavailable).", found.searches, found.unavailable);
        repo.checkpoint_outline(program, draft, &[], &[]).await?;
        return Ok(());
    }
    for (source, reference) in found.sources.iter().zip(&found.references) {
        draft.source_ids.push(source.id.clone());
        sources.push(crate::features::learning::outline_references::authoring_copy(source, reference));
    }
    draft.review.references_gathered += saved as u32;
    draft.review.status = LearningOutlineReviewStatus::Unchecked;
    draft.review.note = format!("{saved} reference pages saved from {} searches. Citations and the plan still need review.", found.searches);
    repo.checkpoint_outline(program, draft, &found.sources, &found.references).await?;
    Ok(())
}

/// Review and repair a saved draft. `gather_if_empty` is set by Repair: a
/// draft that still has no references is gathered for first. Generation has
/// already gathered once and passes false, so a failed gathering is reported
/// as the References finding rather than retried in the same operation.
pub(in crate::features::learning) async fn finish(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    mut program: LearningProgramDto,
    mut draft: OutlineDraft,
    mut sources: Vec<LearningSourceDto>,
    progress: &OutlineProgress,
    web: Option<&dyn crate::features::web::traits::WebServiceTrait>,
    gather_if_empty: bool,
) -> Result<LearningProgramDto> {
    let mut previous_count = None;
    let work: Result<()> = async {
        // The course-wide References finding is re-derived each pass from the
        // actual source count, never carried over.
        draft.review.issues.retain(|issue| issue.kind != LearningOutlineIssueKind::References);
        if gather_if_empty && sources.is_empty() {
            if let Some(web) = web {
                gather_for_draft(repo, llm, web, &mut program, &mut draft, &mut sources, progress).await?;
            }
        }
        // Citations: exact match by code, coverage by a cached judgment. Whatever
        // fails is removed here, so the reviewer never sees a doubtful citation.
        let log = crate::features::learning::outline_citation_check::check(llm, &mut draft, &sources, None, progress).await?;
        draft.record_citations(log);
        if log.removed() > 0 {
            tracing::info!(program_id = %program.summary.id, removed_unmatched = log.removed_unmatched, removed_irrelevant = log.removed_irrelevant, verified = log.verified, "Removed outline citations that did not match or cover their items");
        }
        draft.review.status = LearningOutlineReviewStatus::Unchecked;
        repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
        let references_missing = sources.is_empty() && web.is_some();
        loop {
            progress.check_cancelled()?;
            progress.stage(if draft.review.repair_passes == 0 { crate::features::learning::outline_progress::OutlineStage::Reviewing } else { crate::features::learning::outline_progress::OutlineStage::CheckingRepair });
            let scope = crate::features::learning::outline_review_scope::ReviewScope::new(&draft, &sources, llm);
            draft.review.status = LearningOutlineReviewStatus::Unchecked;
            draft.review.note = if scope.incremental {
                format!("Rechecking {} corrected or unresolved {} and connections across the course. Completed checks for unchanged material are retained; previous findings await this review.", scope.indices.len(), if scope.indices.len() == 1 { "module" } else { "modules" })
            } else {
                "Reviewing the full outline against the saved references. Previous findings await this review.".into()
            };
            repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
            let issues = crate::features::learning::outline_repair::review(llm, &mut draft, &sources, progress, scope).await?;
            let count = issues.len();
            draft.review.issues = issues;
            if references_missing {
                draft.review.issues.push(LearningOutlineIssueDto {
                    path: String::new(), kind: LearningOutlineIssueKind::References,
                    claim: String::new(), quote: String::new(), source_id: None,
                    message: NO_REFERENCES.into(),
                });
            }
            draft.review.status = if draft.review.issues.is_empty() { LearningOutlineReviewStatus::Passed } else { LearningOutlineReviewStatus::NeedsRepair };
            draft.review.note = if count == 0 && references_missing {
                "The plan passed review, but no references could be gathered. Lessons need saved references; use Repair to retry gathering when the web is available.".into()
            } else if count == 0 {
                format!("Outline checks passed. {} references saved; {} citations verified, {} removed because the passage did not match or address the item. Lesson content is checked separately when prepared.", sources.len(), draft.review.citations_verified, draft.review.citations_removed)
            } else { "Findings saved. Repair will use the remaining issues and source passages.".into() };
            repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
            if count == 0 { break; }
            // Interim termination guard until per-finding attempts land: a pass
            // that does not reduce the findings stops the loop.
            if previous_count.is_some_and(|before| count >= before) {
                draft.review.note = "Automatic repair stopped because the unresolved findings did not decrease. Your draft and completed edits are saved. Repair can try again.".into();
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
            previous_count = Some(count);
            progress.stage(crate::features::learning::outline_progress::OutlineStage::Repairing);
            let changed = crate::features::learning::outline_repair::repair(repo, llm, &mut program, &mut draft, &sources, progress).await?;
            if !changed {
                draft.review.note = "The model returned no usable correction. Your draft, findings and saved references are kept; Repair can retry checking.".into();
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
        }
        Ok(())
    }.await;
    if let Err(error) = work {
        tracing::warn!(program_id = %program.summary.id, %error, "Outline retained for repair");
        // Only decorate the committed state; a failed transaction may have
        // left tentative edits or new source IDs in the local working copy.
        program = repo.get(&program.summary.id).await?;
        draft = repo
            .outline_draft(&program.summary.id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("The saved draft is unavailable.".into()))?;
        // A failed or interrupted check can never inherit a previous approval.
        draft.review.status = LearningOutlineReviewStatus::Unchecked;
        let safe = crate::shared::ipc::ApiError::from(error).message;
        draft.review.note = format!(
            "Review or repair stopped: {safe} Completed work is saved. Use Repair to continue."
        );
        repo.checkpoint_outline(&mut program, &mut draft, &[], &[])
            .await?;
    }
    Ok(program)
}

pub(in crate::features::learning) async fn repair_saved(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    llm: &dyn LLMPort,
    request: &RepairLearningOutlineRequestDto,
    progress: &OutlineProgress,
    web: Option<&dyn crate::features::web::traits::WebServiceTrait>,
) -> Result<LearningProgramDto> {
    let _run = drafts.acquire(&request.program_id)?;
    let program = repo.get(&request.program_id).await?;
    if program.summary.status != LearningProgramStatus::Draft
        || program.summary.revision != request.expected_revision
    {
        return Err(AppError::InvalidInput(
            "The draft changed. Reload it before repairing.".into(),
        ));
    }
    let mut draft = repo
        .outline_draft(&request.program_id)
        .await?
        .ok_or_else(|| {
            AppError::InvalidInput("This course has no resumable outline draft.".into())
        })?;
    // Removal counts describe one operation; a repair starts a new one.
    draft.review.citations_removed = 0;
    let saved = repo.verification_sources(&request.program_id).await?;
    let sources = draft
        .source_ids
        .iter()
        .map(|id| {
            saved.iter().find(|s| &s.id == id).cloned().ok_or_else(|| {
                AppError::InvalidInput(
                    "A saved reference is unavailable. Restore it before repairing.".into(),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    progress.saved(&program.summary.id);
    finish(repo, llm, program, draft, sources, progress, web, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_draft_rejects_a_second_run_until_released() {
        let drafts = DraftRunRegistry::new();
        let id = uuid::Uuid::new_v4().to_string();
        let held = drafts.acquire(&id).unwrap();
        assert!(drafts.acquire(&id).is_err());
        // A separate registry is a separate process-worth of state.
        assert!(DraftRunRegistry::new().acquire(&id).is_ok());
        drop(held);
        assert!(drafts.acquire(&id).is_ok());
    }
}
