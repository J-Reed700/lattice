//! Resumable outline drafts. A saved draft is not an approval to teach its content.
use crate::features::learning::{
    dto::*, outline_progress::OutlineProgress, repository::LearningRepository,
};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

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
    Quote,
    Content,
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
            review: LearningOutlineReviewDto {
                status: LearningOutlineReviewStatus::Unchecked,
                issues: vec![],
                repair_passes: 0,
                note: "Draft saved. Review has not completed; use Repair to resume if interrupted."
                    .into(),
                updated_at: 0,
                content_hash: String::new(),
            },
        };
        draft.review.issues =
            crate::features::learning::outline_repair::quote_issues(&draft.candidate, sources);
        draft.stamp();
        draft
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

// Guard concurrent manual repair/acceptance while keeping restart recovery free
// of stale locks. Database revisions also protect every checkpoint and acceptance.
static ACTIVE: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
pub(in crate::features::learning) struct DraftRun(String);
impl DraftRun {
    pub fn acquire(id: &str) -> Result<Self> {
        if !ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_owned())
        {
            return Err(AppError::InvalidInput(
                "This draft is already being reviewed or repaired.".into(),
            ));
        }
        Ok(Self(id.into()))
    }
}
impl Drop for DraftRun {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

pub(in crate::features::learning) async fn finish(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    mut program: LearningProgramDto,
    mut draft: OutlineDraft,
    mut sources: Vec<LearningSourceDto>,
    progress: &OutlineProgress,
    web: Option<&dyn crate::features::web::traits::WebServiceTrait>,
) -> Result<LearningProgramDto> {
    let mut previous_count = None;
    let mut research = crate::features::learning::outline_research::ResearchAttempts::default();
    let work: Result<()> = async {
        // Restore source typography before spending research or model calls on
        // quotation-only mismatches. This checkpoint remains unreviewed.
        crate::features::learning::outline_repair::restore_typography(repo, &mut program, &mut draft, &sources).await?;
        // Existing findings already identify evidence gaps. Research these before
        // spending another model call reviewing the same unsupported material.
        if let Some(web) = web.filter(|_| sources.is_empty() || !draft.review.issues.is_empty()) {
            crate::features::learning::outline_research::research(repo, web, &mut program, &mut draft, &mut sources, progress, &mut research).await?;
            if sources.is_empty() {
                draft.review.status = LearningOutlineReviewStatus::NeedsRepair;
                let message = "No usable references could be captured. Factual review needs supporting material; Repair will retry research.";
                if !draft.review.issues.iter().any(|issue| issue.path.is_empty() && issue.message == message) {
                    draft.review.issues.push(LearningOutlineIssueDto {
                        path: String::new(), kind: LearningOutlineIssueKind::Content,
                        claim: draft.request.goal.clone(), quote: String::new(), source_id: None,
                        message: message.into(),
                    });
                }
                draft.review.note = "Automatic research could not obtain supporting references. Your draft is saved; use Repair to retry when sources are available.".into();
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                return Ok(());
            }
        }
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
            draft.review.status = if count == 0 { LearningOutlineReviewStatus::Passed } else { LearningOutlineReviewStatus::NeedsRepair };
            draft.review.note = if count == 0 {
                "Outline checks passed for this revision. Lesson content is checked separately when prepared.".into()
            } else { "Findings saved. Repair will use the remaining issues and source passages.".into() };
            repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
            if count == 0 { break; }
            // Research the newly affected modules before attempting corrections,
            // including a stalled pass. New evidence earns a fresh repair attempt;
            // repeating the same searches or merely rewriting text does not.
            let added = if let Some(web) = web {
                crate::features::learning::outline_research::research(repo, web, &mut program, &mut draft, &mut sources, progress, &mut research).await?
            } else { 0 };
            if previous_count.is_some_and(|before| count >= before) && added == 0 {
                draft.review.note = "Automatic repair stopped because the unresolved findings did not decrease and no new references were available for another attempt. Your draft and completed edits are saved. Repair can retry research and checking.".into();
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
            previous_count = Some(count);
            progress.stage(crate::features::learning::outline_progress::OutlineStage::Repairing);
            let changed = crate::features::learning::outline_repair::repair(repo, llm, &mut program, &mut draft, &sources, progress).await?;
            if !changed {
                if added > 0 { continue; }
                draft.review.note = "The model returned no usable correction. Your draft, findings and any captured references are saved; Repair can retry research and checking.".into();
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
    llm: &dyn LLMPort,
    request: &RepairLearningOutlineRequestDto,
    progress: &OutlineProgress,
    web: Option<&dyn crate::features::web::traits::WebServiceTrait>,
) -> Result<LearningProgramDto> {
    let _run = DraftRun::acquire(&request.program_id)?;
    let program = repo.get(&request.program_id).await?;
    if program.summary.status != LearningProgramStatus::Draft
        || program.summary.revision != request.expected_revision
    {
        return Err(AppError::InvalidInput(
            "The draft changed. Reload it before repairing.".into(),
        ));
    }
    let draft = repo
        .outline_draft(&request.program_id)
        .await?
        .ok_or_else(|| {
            AppError::InvalidInput("This course has no resumable outline draft.".into())
        })?;
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
    finish(repo, llm, program, draft, sources, progress, web).await
}
