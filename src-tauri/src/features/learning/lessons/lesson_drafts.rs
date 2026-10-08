//! Unpublished candidate checkpoints for durable lesson jobs. A checkpoint is
//! never publication approval. Completed teaching checks may be reused only
//! for identical content and inputs; factual verification remains required.
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::dto::LearningSourceDto;
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default, Deserialize, Serialize)]
pub(in crate::features::learning) struct Drafts {
    pub drafts: BTreeMap<String, Draft>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(in crate::features::learning) struct Draft {
    pub input_hash: String,
    pub candidate: String,
    #[serde(default)]
    pub teaching_receipt: Option<String>,
    #[serde(default)]
    pub authoring: Option<AuthoringContext>,
    #[serde(default)]
    pub claim_inventory: Option<String>,
    /// Failed checks and evidence for a pending rewrite, never approval.
    #[serde(default)]
    pub pending_repair: Option<String>,
}

/// Keep the author's original citation indices stable when research adds new
/// references. These inputs can be reused only while every original snapshot
/// remains active and unchanged; factual approval is never inherited.
#[derive(Clone, Deserialize, Serialize)]
pub(in crate::features::learning) struct AuthoringContext {
    pub scope_hash: String,
    pub prompt: String,
    pub sources: Vec<LearningSourceDto>,
    pub reference_hashes: Vec<(String, String)>,
}

impl AuthoringContext {
    fn matches(&self, scope_hash: &str, references: &[LearningSourceDto]) -> bool {
        self.scope_hash == scope_hash
            && self.reference_hashes.iter().all(|(id, hash)| {
                references.iter().any(|source| {
                    &source.id == id
                        && crate::features::learning::content_verification::digest(&source.excerpt)
                            == *hash
                })
            })
    }
}

#[derive(Clone)]
struct Context {
    repo: LearningCurriculumRepository,
    job_id: String,
    lesson_id: String,
    base_revision: i64,
    input_hash: Arc<Mutex<Option<String>>>,
    authoring: Arc<Mutex<Option<AuthoringContext>>>,
}
tokio::task_local! { static CURRENT: Context; }

pub(in crate::features::learning) fn active() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

/// Learner progress can change the program's revision without changing these
/// authoring inputs. Keep the job's original identity through resume and retry.
pub(in crate::features::learning) fn authoring_revision(current: i64) -> i64 {
    CURRENT
        .try_with(|context| context.base_revision)
        .unwrap_or(current)
}

/// Receipts are keyed by their caller's exact inputs and policy. Repository
/// writes merge one key atomically, including when claim checks finish together.
pub(in crate::features::learning) async fn checkpoint(
    key: &str,
) -> Result<Option<serde_json::Value>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(None);
    };
    context
        .repo
        .lesson_checkpoint(&context.job_id, &context.lesson_id, key)
        .await
}

pub(in crate::features::learning) async fn checkpoints_with_prefix(
    prefix: &str,
) -> Result<Vec<serde_json::Value>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(Vec::new());
    };
    context
        .repo
        .lesson_checkpoints_with_prefix(&context.job_id, &context.lesson_id, prefix)
        .await
}

pub(in crate::features::learning) async fn record_checkpoint(
    key: &str,
    value: serde_json::Value,
) -> Result<()> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(());
    };
    context
        .repo
        .save_lesson_checkpoint(&context.job_id, &context.lesson_id, key, value)
        .await?;
    crate::features::learning::lesson_progress::checkpoint_saved();
    Ok(())
}

pub(in crate::features::learning) async fn run<T>(
    repo: &LearningCurriculumRepository,
    job_id: &str,
    lesson_id: &str,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let base_revision = i64::from(repo.job(job_id).await?.base_revision_number);
    CURRENT
        .scope(
            Context {
                repo: repo.clone(),
                job_id: job_id.into(),
                lesson_id: lesson_id.into(),
                base_revision,
                input_hash: Default::default(),
                authoring: Default::default(),
            },
            future,
        )
        .await
}

pub(in crate::features::learning) async fn reusable_authoring(
    scope_hash: &str,
    references: &[LearningSourceDto],
) -> Result<Option<(String, AuthoringContext)>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(None);
    };
    Ok(context
        .repo
        .lesson_draft(&context.job_id, &context.lesson_id)
        .await?
        .and_then(|draft| {
            draft
                .authoring
                .filter(|authoring| authoring.matches(scope_hash, references))
                .map(|authoring| (draft.input_hash, authoring))
        }))
}

pub(in crate::features::learning) fn authoring(context: AuthoringContext) {
    let _ = CURRENT.try_with(|current| {
        *current.authoring.lock().unwrap_or_else(|e| e.into_inner()) = Some(context);
    });
}

pub(in crate::features::learning) async fn resume(input_hash: String) -> Result<Option<String>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(None);
    };
    *context.input_hash.lock().unwrap_or_else(|e| e.into_inner()) = Some(input_hash.clone());
    Ok(context
        .repo
        .lesson_draft(&context.job_id, &context.lesson_id)
        .await?
        .filter(|draft| draft.input_hash == input_hash)
        .map(|draft| draft.candidate))
}

pub(in crate::features::learning) async fn save(candidate: &str) -> Result<()> {
    save_checkpoint(candidate, None).await
}

pub(in crate::features::learning) async fn save_repair(
    candidate: &str,
    pending: Option<String>,
) -> Result<()> {
    save_checkpoint(candidate, Some(pending)).await
}

async fn save_checkpoint(candidate: &str, repair_override: Option<Option<String>>) -> Result<()> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(());
    };
    let hash = context
        .input_hash
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(input_hash) = hash {
        let previous = context
            .repo
            .lesson_draft(&context.job_id, &context.lesson_id)
            .await?
            .filter(|draft| draft.input_hash == input_hash && draft.candidate == candidate);
        let teaching_receipt = previous
            .as_ref()
            .and_then(|draft| draft.teaching_receipt.clone());
        let claim_inventory = previous
            .as_ref()
            .and_then(|draft| draft.claim_inventory.clone());
        let pending_repair =
            repair_override.unwrap_or_else(|| previous.and_then(|draft| draft.pending_repair));
        let authoring = context
            .authoring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        context
            .repo
            .save_lesson_draft(
                &context.job_id,
                &context.lesson_id,
                Draft {
                    input_hash,
                    candidate: candidate.into(),
                    teaching_receipt,
                    authoring,
                    claim_inventory,
                    pending_repair,
                },
            )
            .await?;
        crate::features::learning::lesson_progress::checkpoint_saved();
    }
    Ok(())
}

async fn current(candidate: &str) -> Result<Option<(Context, Draft)>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(None);
    };
    let hash = context
        .input_hash
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let draft = context
        .repo
        .lesson_draft(&context.job_id, &context.lesson_id)
        .await?
        .filter(|draft| {
            Some(&draft.input_hash) == hash.as_ref()
                && (draft.candidate == candidate
                    || serde_json::from_str::<serde_json::Value>(&draft.candidate)
                        .ok()
                        .zip(serde_json::from_str::<serde_json::Value>(candidate).ok())
                        .is_some_and(|(saved, requested)| saved == requested))
        });
    Ok(draft.map(|draft| (context, draft)))
}

/// This is the source-independent claim inventory and its coverage audit,
/// never a factual approval. The verifier validates its policy and content.
pub(in crate::features::learning) async fn claim_inventory(
    candidate: &str,
) -> Result<Option<String>> {
    Ok(current(candidate)
        .await?
        .and_then(|(_, draft)| draft.claim_inventory))
}

pub(in crate::features::learning) async fn record_claim_inventory(
    candidate: &str,
    inventory: String,
) -> Result<()> {
    if let Some((context, mut draft)) = current(candidate).await? {
        draft.claim_inventory = Some(inventory);
        context
            .repo
            .save_lesson_draft(&context.job_id, &context.lesson_id, draft)
            .await?;
        crate::features::learning::lesson_progress::checkpoint_saved();
    }
    Ok(())
}

pub(in crate::features::learning) async fn pending_repair(
    candidate: &str,
) -> Result<Option<String>> {
    Ok(current(candidate)
        .await?
        .and_then(|(_, draft)| draft.pending_repair))
}

pub(in crate::features::learning) async fn record_pending_repair(
    candidate: &str,
    repair: String,
) -> Result<()> {
    set_pending_repair(candidate, Some(repair)).await
}

async fn set_pending_repair(candidate: &str, repair: Option<String>) -> Result<()> {
    if let Some((context, mut draft)) = current(candidate).await? {
        draft.pending_repair = repair;
        context
            .repo
            .save_lesson_draft(&context.job_id, &context.lesson_id, draft)
            .await?;
        crate::features::learning::lesson_progress::checkpoint_saved();
    }
    Ok(())
}

pub(in crate::features::learning) async fn teaching_completed(
    candidate: &str,
    receipt: &str,
) -> Result<bool> {
    Ok(current(candidate)
        .await?
        .is_some_and(|(_, draft)| draft.teaching_receipt.as_deref() == Some(receipt)))
}

pub(in crate::features::learning) async fn record_teaching(
    candidate: &str,
    receipt: String,
) -> Result<()> {
    if let Some((context, mut draft)) = current(candidate).await? {
        draft.teaching_receipt = Some(receipt);
        context
            .repo
            .save_lesson_draft(&context.job_id, &context.lesson_id, draft)
            .await?;
        crate::features::learning::lesson_progress::checkpoint_saved();
    }
    Ok(())
}
