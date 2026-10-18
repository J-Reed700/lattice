//! Saving, deduplicating and reviewing versioned recall cards.
use super::*;

impl LearningRecallRepository {
    pub async fn save_card(
        &self,
        req: &SaveLearningRecallCardRequestDto,
    ) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.card_id, "card")?;
        if req.source_version_ids.len() > 20
            || req.source_version_ids.iter().collect::<HashSet<_>>().len()
                != req.source_version_ids.len()
        {
            return Err(invalid(
                "A recall card may cite up to 20 unique source versions.",
            ));
        }
        validate_content(req.format, &req.content)?;
        if req.change_reason.trim().is_empty() || req.change_reason.chars().count() > 500 {
            return Err(invalid("Card change reason must contain 1–500 characters."));
        }
        let hash = payload_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        let operation = recall_operation(&req.operation_id, &req.program_id, "save_card", &hash);
        if let Some(card_id) = operation.replay::<String, _>(&mut *tx).await? {
            if card_id != req.card_id {
                return Err(invalid("Operation replay does not match this card."));
            }
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let deck_id = memory_deck(&mut tx, &req.program_id).await?;
        for version_id in &req.source_version_ids {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Recall source version does not belong to this program.",
                ));
            }
        }
        let prior: Option<sqlx::sqlite::SqliteRow> = sqlx::query("SELECT content_revision FROM learning_recall_card_profiles WHERE card_id=? AND program_id=?")
            .bind(&req.card_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let new_card = prior.is_none();
        let revision = if let Some(row) = &prior {
            let old_revision: i64 = row.get("content_revision");
            if req.expected_content_revision != Some(old_revision) {
                return Err(invalid("Recall card changed; reload and retry."));
            }
            old_revision + 1
        } else {
            if req.expected_content_revision.is_some() {
                return Err(AppError::NotFound(
                    "Recall card not found in this program.".into(),
                ));
            }
            let existing_card: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM study_cards WHERE id=?")
                    .bind(&req.card_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?;
            if existing_card.is_some() {
                return Err(invalid("Card ID is already in use."));
            }
            1
        };
        let mut sources = self
            .canonical_sources(&mut tx, &req.program_id, &req.source_version_ids)
            .await?;
        if sources.is_empty() {
            // A deck card always names a source; this one is the learner's own.
            sources.push(StudySourceDto {
                chunk_id: format!("learning-recall:{}", req.card_id),
                document_id: String::new(),
                file_name: "Personal recall card".into(),
                file_path: String::new(),
                excerpt: "Learner-authored recall card without an external source.".into(),
                url: None,
            });
        }
        let source_json = json(&sources)?;
        let study_format = match req.format {
            LearningRecallCardFormat::MultipleChoice => "multiple_choice",
            _ => "question_answer",
        };
        let answer = if req.format == LearningRecallCardFormat::MultipleChoice {
            req.content
                .options
                .get(req.content.correct_option_index.unwrap_or(0))
                .cloned()
                .ok_or_else(|| invalid("Correct answer option is missing."))?
        } else {
            req.content.answer.clone()
        };
        let timestamp = now();
        if new_card {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM study_cards WHERE deck_id=?")
                .bind(&deck_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
            if count >= MAX_CARDS {
                return Err(invalid(
                    "A Learning recall deck can contain at most 2,000 cards.",
                ));
            }
            sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,format) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&req.card_id)
                .bind(&deck_id)
                .bind(req.content.prompt.trim())
                .bind(answer)
                .bind(json(&req.content.options)?)
                .bind(req.content.correct_option_index.unwrap_or(0) as i64)
                .bind(req.content.explanation.trim())
                .bind(source_json)
                .bind("Learning Recall")
                .bind(timestamp)
                .bind(study_format)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,content_revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(&req.card_id)
                .bind(&req.program_id)
                .bind(format_name(req.format))
                .bind(json(&req.content.prompt)?)
                .bind(json(&req.content)?)
                .bind(json(&req.source_version_ids)?)
                .bind(revision)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        } else {
            sqlx::query("UPDATE study_cards SET question=?,answer=?,options_json=?,correct_index=?,explanation=?,source_json=?,format=? WHERE id=?")
                .bind(req.content.prompt.trim())
                .bind(answer)
                .bind(json(&req.content.options)?)
                .bind(req.content.correct_option_index.unwrap_or(0) as i64)
                .bind(req.content.explanation.trim())
                .bind(source_json)
                .bind(study_format)
                .bind(&req.card_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("UPDATE learning_recall_card_profiles SET format=?,prompt_json=?,answer_json=?,source_version_ids_json=?,content_revision=?,updated_at=? WHERE card_id=? AND program_id=?")
                .bind(format_name(req.format))
                .bind(json(&req.content.prompt)?)
                .bind(json(&req.content)?)
                .bind(json(&req.source_version_ids)?)
                .bind(revision)
                .bind(timestamp)
                .bind(&req.card_id)
                .bind(&req.program_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("INSERT INTO learning_recall_card_versions(card_id,revision,format,prompt_json,answer_json,source_version_ids_json,change_reason,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&req.card_id)
            .bind(revision)
            .bind(format_name(req.format))
            .bind(json(&req.content.prompt)?)
            .bind(json(&req.content)?)
            .bind(json(&req.source_version_ids)?)
            .bind(req.change_reason.trim())
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        self.suggest_duplicates(
            &mut tx,
            &req.program_id,
            &req.card_id,
            &req.content.prompt,
            timestamp,
        )
        .await?;
        operation.record(&mut *tx, &req.card_id).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn decide_duplicate(
        &self,
        req: &DecideLearningRecallDuplicateRequestDto,
    ) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.suggestion_id, "duplicate suggestion")?;
        let hash = payload_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        let operation = recall_operation(
            &req.operation_id,
            &req.program_id,
            "decide_duplicate",
            &hash,
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let status = if req.accept { "confirmed" } else { "dismissed" };
        let result = sqlx::query("UPDATE learning_recall_duplicate_suggestions SET status=?,decided_at=? WHERE program_id=? AND id=? AND status='pending'")
            .bind(status)
            .bind(now())
            .bind(&req.program_id)
            .bind(&req.suggestion_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound(
                "Pending duplicate suggestion not found in this program.".into(),
            ));
        }
        operation.record(&mut *tx, &req.suggestion_id).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn review_card(
        &self,
        req: &ReviewLearningRecallCardRequestDto,
    ) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(&req.review_id, "review")?;
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.card_id, "card")?;
        if req.expected_review_count < 0 {
            return Err(invalid("Expected review count cannot be negative."));
        }
        self.bootstrap_deck_cards(&req.program_id).await?;
        let hash = payload_hash(&(
            &req.program_id,
            &req.card_id,
            req.rating,
            req.selected_option,
            req.expected_review_count,
        ))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        let operation = recall_operation(&req.review_id, &req.program_id, "review", &hash);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let row = sqlx::query("SELECT c.format,c.options_json,c.correct_index FROM study_cards c JOIN learning_recall_card_profiles p ON p.card_id=c.id WHERE c.id=? AND p.program_id=?")
            .bind(&req.card_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Recall card not found in this program.".into()))?;
        let multiple_choice = row.get::<String, _>("format") == "multiple_choice";
        let options: Vec<String> = serde_json::from_str(row.get("options_json"))
            .map_err(|error| AppError::InvalidState(format!("Invalid recall options: {error}")))?;
        let correct_index = row.get::<i64, _>("correct_index").max(0) as usize;
        if multiple_choice != req.selected_option.is_some() {
            return Err(invalid(
                "Choose an answer only for multiple-choice recall cards.",
            ));
        }
        if req
            .selected_option
            .is_some_and(|index| index >= options.len())
        {
            return Err(invalid("Selected answer is outside the card's options."));
        }
        let (correct, rating) = answered(req.selected_option, correct_index, req.rating);
        let timestamp = now();
        let outcome = record_review(
            &mut tx,
            &CardReview {
                review_id: &req.review_id,
                card_id: &req.card_id,
                rating,
                correct,
                selected_option: req.selected_option,
                expected_reviews: req.expected_review_count,
            },
            timestamp,
        )
        .await?;
        if outcome == ReviewOutcome::Stale {
            return Err(invalid("Recall card changed; reload and retry."));
        }
        if outcome == ReviewOutcome::Recorded {
            sqlx::query("UPDATE learning_recall_card_profiles SET updated_at=? WHERE card_id=? AND program_id=?")
                .bind(timestamp)
                .bind(&req.card_id)
                .bind(&req.program_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        operation.record(&mut *tx, &req.review_id).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    async fn canonical_sources(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        version_ids: &[String],
    ) -> Result<Vec<StudySourceDto>> {
        let mut out = Vec::with_capacity(version_ids.len());
        for version_id in version_ids {
            let row = sqlx::query("SELECT v.id,v.title,v.excerpt,v.resolved_url,v.requested_url,s.id source_id,s.origin,s.kind,d.file_name,d.file_path FROM learning_source_versions v JOIN learning_source_library s ON s.program_id=v.program_id AND s.id=v.source_id LEFT JOIN documents d ON d.id=s.origin WHERE v.program_id=? AND v.id=?")
                .bind(program_id)
                .bind(version_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?
                .ok_or_else(|| invalid("Source version does not belong to this program."))?;
            let url: Option<String> = row.get("resolved_url");
            let requested: Option<String> = row.get("requested_url");
            let is_web: String = row.get("kind");
            out.push(StudySourceDto {
                chunk_id: row.get("id"),
                document_id: row.get("source_id"),
                file_name: row
                    .get::<Option<String>, _>("file_name")
                    .unwrap_or(row.get("title")),
                file_path: row
                    .get::<Option<String>, _>("file_path")
                    .unwrap_or_default(),
                excerpt: row.get("excerpt"),
                url: (is_web == "web").then(|| url.or(requested)).flatten(),
            });
        }
        Ok(out)
    }

    async fn suggest_duplicates(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        card_id: &str,
        prompt: &str,
        timestamp: i64,
    ) -> Result<()> {
        let new_terms = lexical_terms(prompt);
        let rows = sqlx::query("SELECT card_id,prompt_json FROM learning_recall_card_profiles WHERE program_id=? AND card_id<>?")
            .bind(program_id)
            .bind(card_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;
        for row in rows {
            let other_id: String = row.get("card_id");
            let other_prompt: String =
                serde_json::from_str(row.get("prompt_json")).map_err(|error| {
                    AppError::InvalidState(format!("Invalid recall prompt: {error}"))
                })?;
            let similarity = jaccard(&new_terms, &lexical_terms(&other_prompt));
            if similarity < 0.65 {
                continue;
            }
            let (first, second) = if card_id < other_id.as_str() {
                (card_id, other_id.as_str())
            } else {
                (other_id.as_str(), card_id)
            };
            let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_recall_duplicate_suggestions WHERE program_id=? AND card_id=? AND possible_duplicate_card_id=? AND status='pending'")
                .bind(program_id)
                .bind(first)
                .bind(second)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
            if exists.is_some() {
                continue;
            }
            sqlx::query("INSERT INTO learning_recall_duplicate_suggestions(id,program_id,card_id,possible_duplicate_card_id,reason,similarity,status,created_at) VALUES(?,?,?,?,?,?,'pending',?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(program_id)
                .bind(first)
                .bind(second)
                .bind("similar_prompt")
                .bind(similarity)
                .bind(timestamp)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
}

fn lexical_terms(value: &str) -> HashSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| word.chars().count() >= 2)
        .collect()
}
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let intersection = a.intersection(b).count();
    if intersection == 0 {
        return 0.0;
    }
    intersection as f64 / a.union(b).count().max(1) as f64
}
