//! The `learning.quiz_bank` reactor: restores a dropped question to the
//! bank when edited (and drops it again on revert), and flags section edits
//! that remove text quoted by bank questions.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::assessment_repository::{
    candidate_review_status_in, quoted_questions_in, set_candidate_review_status_in,
    CandidateReviewStatus,
};
use crate::edits::keys::{self, affected, section_key};
use lattice_core::shared::edits::{
    Change, Consequence, ContentRef, EditReactor, EditRegistry, Reaction, RelatedContent,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Reading, WriteTx};

struct QuizBankReactor;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_reactor(Arc::new(QuizBankReactor))
}

async fn section_consequences(
    unit: &mut Reading<'_>,
    change: &Change<'_>,
) -> Result<Vec<Consequence>> {
    let Some((lesson_id, _)) = section_key(&change.content.key) else {
        return Ok(Vec::new());
    };
    let old_body = change
        .before
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("");
    let new_body = change
        .after
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("");
    if old_body == new_body {
        return Ok(Vec::new());
    }
    let quoted = quoted_questions_in(&mut *unit, lesson_id).await?;
    let related: Vec<RelatedContent> = quoted
        .into_iter()
        .filter(|q| old_body.contains(&q.source_quote) && !new_body.contains(&q.source_quote))
        .map(|q| RelatedContent {
            content: ContentRef {
                kind: keys::QUESTION.into(),
                key: q.id,
            },
            label: format!("Question: {}", q.prompt),
        })
        .collect();
    if related.is_empty() {
        return Ok(Vec::new());
    }
    let message = if related.len() == 1 {
        "1 quiz question was written from text you changed or removed.".into()
    } else {
        format!(
            "{} quiz questions were written from text you changed or removed.",
            related.len()
        )
    };
    Ok(vec![Consequence {
        code: "quiz_bank.quote_removed".into(),
        message,
        related,
    }])
}

#[async_trait]
impl EditReactor for QuizBankReactor {
    fn name(&self) -> &'static str {
        keys::QUIZ_BANK_REACTOR
    }

    fn kinds(&self) -> &'static [&'static str] {
        &[keys::SECTION, keys::QUESTION]
    }

    async fn preview(
        &self,
        unit: &mut Reading<'_>,
        change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        if change.content.kind == keys::SECTION {
            return section_consequences(unit, change).await;
        }
        Ok(Vec::new())
    }

    async fn apply(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction> {
        if change.content.kind == keys::SECTION {
            let consequences = section_consequences(&mut tx.reading(), change).await?;
            return Ok(Reaction {
                receipt: None,
                affected: Vec::new(),
                consequences,
                follow_up: None,
            });
        }
        if change.content.kind != keys::QUESTION {
            return Ok(Reaction::default());
        }

        if let Some(reverted) = change.reverted_receipt {
            let Some(prior) = reverted.get("priorReviewStatus").and_then(Value::as_str) else {
                return Ok(Reaction::default());
            };
            let prior_status = CandidateReviewStatus::parse(prior)?;
            let current =
                candidate_review_status_in(&mut tx.reading(), &change.content.key).await?;
            set_candidate_review_status_in(tx, &change.content.key, prior_status).await?;
            return Ok(Reaction {
                receipt: current.map(|status| json!({ "priorReviewStatus": status.as_str() })),
                affected: vec![affected::QUIZZES.into()],
                consequences: Vec::new(),
                follow_up: None,
            });
        }

        let current = candidate_review_status_in(&mut tx.reading(), &change.content.key).await?;
        let receipt = if current == Some(CandidateReviewStatus::Dropped) {
            set_candidate_review_status_in(tx, &change.content.key, CandidateReviewStatus::Agreed)
                .await?;
            Some(json!({ "priorReviewStatus": CandidateReviewStatus::Dropped.as_str() }))
        } else {
            None
        };
        Ok(Reaction {
            receipt,
            affected: vec![affected::QUIZZES.into()],
            consequences: Vec::new(),
            follow_up: None,
        })
    }
}
