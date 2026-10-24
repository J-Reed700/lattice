//! Missed questions into Recall (section 4.5): each becomes a card the
//! learner can edit, through the recall deck's own save path.
use super::{assessment_operation, candidate_in, invalid, uuid, LearningAssessmentRepository};
use crate::features::learning::assessment_engine::{self as engine, LearningItemFormat};
use crate::features::learning::dto::*;
use crate::features::learning::persistence::db;
use crate::features::learning::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, SaveLearningRecallCardRequestDto,
};
use crate::features::learning::recall_repository::LearningRecallRepository;
use lattice_core::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};

/// A UUID derived from text, so a repeated request writes the same cards.
fn derived_id(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    for (slot, byte) in bytes.iter_mut().zip(digest.iter()) {
        *slot = *byte;
    }
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

fn card_for(
    candidate: &engine::LearningAssessmentCandidate,
) -> Option<(LearningRecallCardFormat, LearningRecallContentDto)> {
    use engine::LearningAssessmentKey as K;
    let option = |i: &usize| candidate.options.get(*i).cloned().unwrap_or_default();
    let one_line = candidate
        .explanation
        .split_terminator(['.', '\n'])
        .next()
        .map(|s| format!("{}.", s.trim()))
        .unwrap_or_default();
    let qa = |prompt: String, answer: String| {
        Some((
            LearningRecallCardFormat::QuestionAnswer,
            LearningRecallContentDto {
                prompt: prompt.chars().take(2_000).collect(),
                answer: answer.chars().take(1_000).collect(),
                explanation: one_line.clone(),
                options: vec![],
                correct_option_index: None,
                language: None,
                cloze_deletions: vec![],
            },
        ))
    };
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::Cloze, K::Cloze(blanks)) => {
            let mut prompt = candidate.prompt.clone();
            let mut deletions = Vec::new();
            for (index, blank) in blanks.iter().enumerate() {
                let answer = blank.variants.first().cloned().unwrap_or_default();
                prompt = prompt.replace(&format!("{{{{{}}}}}", index + 1), &answer);
                if !deletions.contains(&answer) {
                    deletions.push(answer);
                }
            }
            Some((
                LearningRecallCardFormat::Cloze,
                LearningRecallContentDto {
                    answer: deletions.join(", ").chars().take(1_000).collect(),
                    prompt: prompt.chars().take(2_000).collect(),
                    explanation: one_line,
                    options: vec![],
                    correct_option_index: None,
                    language: None,
                    cloze_deletions: deletions,
                },
            ))
        }
        (_, K::Choice(key)) if candidate.format == LearningItemFormat::TrueFalse => {
            qa(format!("True or false: {}", candidate.prompt), option(key))
        }
        (_, K::Choice(key)) => qa(candidate.prompt.clone(), option(key)),
        (_, K::Choices(keys)) => qa(
            candidate.prompt.clone(),
            keys.iter().map(option).collect::<Vec<_>>().join("; "),
        ),
        (_, K::Text(text)) => qa(
            candidate.prompt.clone(),
            match &text.numeric {
                Some(n) => {
                    engine::show_number(n.value)
                        + &n.unit_variants
                            .first()
                            .map(|u| format!(" {u}"))
                            .unwrap_or_default()
                }
                None => text.variants.first().cloned().unwrap_or_default(),
            },
        ),
        (_, K::Matching(pairs)) => qa(
            format!("{}\n{}", candidate.prompt, candidate.left.join("; ")),
            candidate
                .left
                .iter()
                .zip(pairs)
                .map(|(l, r)| format!("{l} → {}", option(r)))
                .collect::<Vec<_>>()
                .join("; "),
        ),
        (_, K::Ordering(order)) => qa(candidate.prompt.clone(), order.join(" → ")),
        (_, K::Rubric) => qa(
            candidate.prompt.clone(),
            candidate
                .rubric
                .iter()
                .map(|c| c.title.clone())
                .collect::<Vec<_>>()
                .join("; "),
        ),
        _ => None,
    }
}

impl LearningAssessmentRepository {
    /// Turn the chosen missed questions of a submitted quiz into recall cards.
    pub async fn add_misses_to_recall(
        &self,
        req: &AddLearningQuizMissesToRecallRequestDto,
        hash: &str,
    ) -> Result<LearningQuizMissesToRecallDto> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        if req.item_ids.is_empty() || req.item_ids.len() > 200 {
            return Err(invalid("Choose between one and 200 questions."));
        }
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "add_to_recall",
            hash,
            "Operation ID was reused with different questions.",
        );
        if let Some(done) = operation
            .replay::<LearningQuizMissesToRecallDto, _>(&self.pool)
            .await?
        {
            return Ok(done);
        }
        let mut connection = self.pool.acquire().await.map_err(db)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM learning_assessment_forms WHERE id=? AND program_id=?",
        )
        .bind(&req.form_id)
        .bind(&req.program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
        if status.as_deref() != Some("submitted") {
            return Err(invalid("Add questions to Recall from a submitted quiz."));
        }
        let mut cards = Vec::new();
        for item_id in &req.item_ids {
            let in_form: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?",
            )
            .bind(&req.form_id)
            .bind(item_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
            if in_form.is_none() {
                return Err(invalid("That question is not part of this quiz."));
            }
            let stored = candidate_in(&mut connection, &req.program_id, item_id).await?;
            if let Some(card) = card_for(&stored.candidate) {
                cards.push((item_id.clone(), stored.candidate.source_version_ids, card));
            }
        }
        drop(connection);
        let recall = LearningRecallRepository::new(self.pool.clone());
        let mut created = 0i64;
        for (item_id, sources, (format, content)) in cards {
            let seed = format!("{}:{item_id}", req.operation_id);
            recall
                .save_card(&SaveLearningRecallCardRequestDto {
                    operation_id: derived_id(&format!("op:{seed}")),
                    program_id: req.program_id.clone(),
                    card_id: derived_id(&format!("card:{seed}")),
                    expected_content_revision: None,
                    format,
                    content,
                    source_version_ids: sources,
                    change_reason: "Missed in a quiz".into(),
                })
                .await?;
            created += 1;
        }
        let deck_id: String =
            sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
                .bind(&req.program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("This course has no recall deck yet.".into()))?;
        let done = LearningQuizMissesToRecallDto {
            cards_created: created,
            deck_id,
        };
        operation.record(&self.pool, &done).await?;
        Ok(done)
    }
}
