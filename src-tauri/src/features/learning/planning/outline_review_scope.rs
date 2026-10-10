//! Reuse completed semantic checks only while their inputs still match.
use crate::features::learning::{dto::LearningSourceDto, outline_draft::OutlineDraft};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const REVIEW_VERSION: &str = "outline-incremental-review-v1";

/// Separate from the draft's content hash: checkpoints also occur before review.
/// Only a successfully parsed, validated semantic review creates this receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct ReviewReceipt {
    context_hash: String,
    structure_hash: String,
    module_hashes: Vec<String>,
}

fn hash(value: &Value) -> String {
    crate::features::learning::persistence::hash_text(&value.to_string())
}

impl ReviewReceipt {
    pub fn new(draft: &OutlineDraft, sources: &[LearningSourceDto], llm: &dyn LLMPort) -> Self {
        let modules = draft
            .candidate
            .get("modules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut structure = draft.candidate.clone();
        // Moving, renaming or adding modules/lessons invalidates the sequence.
        if let Some(object) = structure.as_object_mut() {
            object.insert("modules".into(), json!(modules.iter().map(|m| json!({
                "title":m.get("title"),
                "lessons":m.get("lessons").and_then(Value::as_array).into_iter().flatten().map(|l| l.get("title")).collect::<Vec<_>>()
            })).collect::<Vec<_>>()));
        }
        Self {
            context_hash: hash(
                &json!({"version":REVIEW_VERSION,"model":llm.model_name(),"request":draft.request,"sources":sources}),
            ),
            structure_hash: hash(&structure),
            module_hashes: modules.iter().map(hash).collect(),
        }
    }
}

pub(in crate::features::learning) struct ReviewScope {
    pub incremental: bool,
    pub indices: BTreeSet<usize>,
    pub receipt: ReviewReceipt,
}

impl ReviewScope {
    pub fn new(draft: &OutlineDraft, sources: &[LearningSourceDto], llm: &dyn LLMPort) -> Self {
        let receipt = ReviewReceipt::new(draft, sources, llm);
        let full = || Self {
            incremental: false,
            indices: (0..receipt.module_hashes.len()).collect(),
            receipt: receipt.clone(),
        };
        let Some(previous) = &draft.completed_review else {
            return full();
        };
        if previous.context_hash != receipt.context_hash
            || previous.structure_hash != receipt.structure_hash
            || previous.module_hashes.len() != receipt.module_hashes.len()
        {
            return full();
        }
        let mut indices: BTreeSet<_> = receipt
            .module_hashes
            .iter()
            .zip(&previous.module_hashes)
            .enumerate()
            .filter_map(|(i, (now, before))| (now != before).then_some(i))
            .collect();
        // Every unresolved finding must be reconsidered, including findings in
        // otherwise unchanged modules. Course-wide findings require full review.
        for issue in &draft.review.issues {
            let Some(index) = issue
                .path
                .strip_prefix("/modules/")
                .and_then(|p| p.split('/').next())
                .and_then(|i| i.parse::<usize>().ok())
                .filter(|i| *i < receipt.module_hashes.len())
            else {
                return full();
            };
            indices.insert(index);
        }
        if indices.is_empty() || indices.len() == receipt.module_hashes.len() {
            return full();
        }
        Self {
            incremental: true,
            indices,
            receipt,
        }
    }

    pub fn candidate(&self, candidate: &Value) -> Result<Value> {
        let modules = self
            .indices
            .iter()
            .map(|i| module_at(candidate, *i))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"modules":modules}))
    }

    pub fn add_context(&self, payload: &mut Value, candidate: &Value) -> Result<()> {
        let payload = payload
            .as_object_mut()
            .ok_or_else(|| AppError::InternalError("Missing outline review payload".into()))?;
        payload.insert("reviewScope".into(), json!({
            "mode":if self.incremental {"changed_modules"} else {"full"},
            "moduleIndices":self.indices,
            "rule":"Inspect every supplied module in full. For incremental review, unchanged modules retain completed checks against exactly the same sources, learner request and reviewer version. Check the entire courseSequence for contradictions and effects of the edits on other modules. Use the explicit JSON paths, never positions within modulesToReview. Report problems in any affected module. All previous findings must be rechecked."
        }));
        if !self.incremental {
            return Ok(());
        }
        payload.remove("candidate");
        let modules = self.indices.iter().map(|i| Ok(json!({
            "path":format!("/modules/{i}"),"moduleNumber":i+1,"module":module_at(candidate, *i)?
        }))).collect::<Result<Vec<_>>>()?;
        payload.insert("modulesToReview".into(), json!(modules));
        // Preserve every instructional field for the cross-course check. Only
        // repeated evidence is omitted; changed modules carry their full quotes.
        let mut sequence = candidate.get("modules").cloned().ok_or_else(|| {
            AppError::InvalidInput("The outline has no modules to review.".into())
        })?;
        if let Some(modules) = sequence.as_array_mut() {
            for (i, module) in modules.iter_mut().enumerate() {
                strip_citations(module);
                if let Some(object) = module.as_object_mut() {
                    object.insert("path".into(), json!(format!("/modules/{i}")));
                    object.insert("moduleNumber".into(), json!(i + 1));
                }
            }
        }
        payload.insert("courseSequence".into(), sequence);
        Ok(())
    }
}

fn module_at(candidate: &Value, index: usize) -> Result<&Value> {
    candidate
        .get("modules")
        .and_then(|modules| modules.get(index))
        .ok_or_else(|| {
            AppError::InvalidInput(
                "A review referred to an unavailable module. The draft remains unchecked.".into(),
            )
        })
}

fn strip_citations(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("quote");
            object.remove("sourceIndex");
            for child in object.values_mut() {
                strip_citations(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                strip_citations(child);
            }
        }
        _ => {}
    }
}
