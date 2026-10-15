//! Lesson notes, card drafts and the journal and deck behind a program's memory.
use super::*;

impl LearningRepository {
    pub async fn ensure_lesson_note(
        &self,
        program_id: &str,
        lesson_id: &str,
        title: &str,
        content: &str,
    ) -> Result<(String, bool)> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (journal_id, _) = ensure_memory_resources(&mut tx, program_id).await?;
        validate_memory_lesson(&mut tx, program_id, lesson_id, false).await?;
        if let Some(note_id) = sqlx::query_scalar::<_, String>(
            "SELECT note_id FROM learning_lesson_note_links WHERE program_id=? AND lesson_id=?",
        )
        .bind(program_id)
        .bind(lesson_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            tx.commit().await.map_err(db)?;
            return Ok((note_id, false));
        }
        let note_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let note = crate::features::daily_notes::repository::WorkspaceNoteRecord {
            revision: 0,
            id: note_id.clone(),
            title: title.trim().to_owned(),
            journal_id: Some(journal_id),
            content: content.to_owned(),
            linked_document_ids: "[]".into(),
            linked_conversation_ids: "[]".into(),
            highlights_json: "[]".into(),
            sticky_notes_json: "[]".into(),
            conversation_snapshots_json: "[]".into(),
            sources_json: "[]".into(),
            created_at: now.clone(),
            updated_at: now,
        };
        crate::features::daily_notes::repository::DailyNotesRepository::insert_in_transaction(
            &mut tx, &note,
        )
        .await?;
        sqlx::query("INSERT INTO learning_lesson_note_links(program_id,lesson_id,note_id,created_at) VALUES(?,?,?,?)")
            .bind(program_id).bind(lesson_id).bind(&note_id).bind(chrono::Utc::now().timestamp_millis())
            .execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok((note_id, true))
    }

    pub async fn save_generated_drafts(
        &self,
        program_id: &str,
        lesson_id: &str,
        drafts: &[crate::features::learning::generation::GeneratedLearningCardDraft],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        ensure_memory_resources(&mut tx, program_id).await?;
        validate_memory_lesson(&mut tx, program_id, lesson_id, true).await?;
        let now = chrono::Utc::now().timestamp_millis();
        for draft in drafts {
            validate_draft_sources(&mut tx, program_id, lesson_id, &draft.source_ids, true).await?;
            sqlx::query("INSERT INTO learning_card_drafts(id,program_id,lesson_id,question,answer,explanation,source_ids_json,origin,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'generated','pending',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(lesson_id).bind(&draft.question).bind(&draft.answer).bind(&draft.explanation).bind(json(&draft.source_ids)?).bind(now).bind(now)
                .execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    pub async fn save_manual_draft(
        &self,
        request: &SaveLearningCardDraftRequestDto,
    ) -> Result<String> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        ensure_memory_resources(&mut tx, &request.program_id).await?;
        validate_memory_lesson(&mut tx, &request.program_id, &request.lesson_id, true).await?;
        let existing_origin = if let Some(id) = request.draft_id.as_deref() {
            sqlx::query_scalar::<_, String>("SELECT origin FROM learning_card_drafts WHERE id=? AND program_id=? AND lesson_id=? AND status='pending'")
                .bind(id).bind(&request.program_id).bind(&request.lesson_id).fetch_optional(&mut *tx).await.map_err(db)?
        } else {
            None
        };
        let is_generated = existing_origin.as_deref() == Some("generated");
        validate_draft_sources(
            &mut tx,
            &request.program_id,
            &request.lesson_id,
            &request.source_ids,
            is_generated,
        )
        .await?;
        let now = chrono::Utc::now().timestamp_millis();
        let source_json = json(&request.source_ids)?;
        let draft_id = match request.draft_id.as_deref() {
            Some(draft_id) => {
                let result = sqlx::query("UPDATE learning_card_drafts SET question=?,answer=?,explanation=?,source_ids_json=?,updated_at=? WHERE id=? AND program_id=? AND lesson_id=? AND status='pending'")
                    .bind(request.question.trim()).bind(request.answer.trim()).bind(request.explanation.trim()).bind(source_json).bind(now).bind(draft_id).bind(&request.program_id).bind(&request.lesson_id)
                    .execute(&mut *tx).await.map_err(db)?;
                if result.rows_affected() != 1 {
                    return Err(AppError::NotFound(
                        "Editable learning card draft not found.".into(),
                    ));
                }
                draft_id.to_owned()
            }
            None => {
                let draft_id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO learning_card_drafts(id,program_id,lesson_id,question,answer,explanation,source_ids_json,origin,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'manual','pending',?,?)")
                    .bind(&draft_id).bind(&request.program_id).bind(&request.lesson_id).bind(request.question.trim()).bind(request.answer.trim()).bind(request.explanation.trim()).bind(source_json).bind(now).bind(now)
                    .execute(&mut *tx).await.map_err(db)?;
                draft_id
            }
        };
        tx.commit().await.map_err(db)?;
        Ok(draft_id)
    }

    pub async fn discard_card_draft(&self, program_id: &str, draft_id: &str) -> Result<()> {
        let result = sqlx::query("UPDATE learning_card_drafts SET status='discarded',updated_at=? WHERE id=? AND program_id=? AND status='pending'")
            .bind(chrono::Utc::now().timestamp_millis()).bind(draft_id).bind(program_id).execute(&self.pool).await.map_err(db)?;
        if result.rows_affected() == 0 {
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status FROM learning_card_drafts WHERE id=? AND program_id=?",
            )
            .bind(draft_id)
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
            if status.as_deref() != Some("discarded") {
                return Err(AppError::NotFound(
                    "Pending learning card draft not found.".into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn accept_card_draft(&self, program_id: &str, draft_id: &str) -> Result<String> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (_journal_id, deck_id) = ensure_memory_resources(&mut tx, program_id).await?;
        let row = sqlx::query("SELECT d.*,l.title lesson_title FROM learning_card_drafts d JOIN learning_lessons l ON l.id=d.lesson_id WHERE d.id=? AND d.program_id=?")
            .bind(draft_id).bind(program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning card draft not found.".into()))?;
        let status: String = row.get("status");
        if status == "accepted" {
            let card_id: Option<String> = row.try_get("accepted_card_id").map_err(db)?;
            let card_id = card_id
                .ok_or_else(|| AppError::NotFound("Accepted Study card was deleted.".into()))?;
            tx.commit().await.map_err(db)?;
            return Ok(card_id);
        }
        if status != "pending" {
            return Err(AppError::InvalidInput(
                "Discarded learning card drafts cannot be accepted.".into(),
            ));
        }
        let lesson_id: String = row.get("lesson_id");
        validate_memory_lesson(&mut tx, program_id, &lesson_id, true).await?;
        let source_ids: Vec<String> = decode(row.get("source_ids_json"))?;
        let generated = row.get::<String, _>("origin") == "generated";
        validate_draft_sources(&mut tx, program_id, &lesson_id, &source_ids, generated).await?;
        let mut citations = Vec::new();
        for source_id in &source_ids {
            let source = sqlx::query(
                "SELECT id,title,url,excerpt FROM learning_sources WHERE program_id=? AND id=?",
            )
            .bind(program_id)
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| {
                AppError::InvalidInput("Learning card refers to a missing source snapshot.".into())
            })?;
            let source_id: String = source.get("id");
            let url: Option<String> = source.get("url");
            let (document_id, chunk_id) = if url.is_none() {
                source_id
                    .split_once(':')
                    .map(|(doc, chunk)| (doc.to_owned(), chunk.to_owned()))
                    .unwrap_or_default()
            } else {
                (String::new(), source_id.clone())
            };
            let file_path = if !document_id.is_empty() {
                sqlx::query_scalar::<_, String>("SELECT file_path FROM documents WHERE id=?")
                    .bind(&document_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
                    .unwrap_or_default()
            } else {
                String::new()
            };
            citations.push(StudySourceDto {
                chunk_id,
                document_id,
                file_name: source.get("title"),
                file_path,
                excerpt: source.get("excerpt"),
                url,
            });
        }
        if citations.is_empty() {
            citations.push(StudySourceDto {
                chunk_id: format!("learning-memory:{draft_id}"),
                document_id: String::new(),
                file_name: if generated {
                    row.get("lesson_title")
                } else {
                    "Personal learning card".into()
                },
                file_path: String::new(),
                excerpt: if generated {
                    "AI-authored recall from the prepared lesson; no external source."
                } else {
                    "Learner-authored recall card without an external source."
                }
                .into(),
                url: None,
            });
        }
        let now = chrono::Utc::now().timestamp_millis();
        let card_id = uuid::Uuid::new_v4().to_string();
        let answer: String = row.get("answer");
        let source = citations
            .first()
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("Learning card needs a source record.".into()))?;
        let card = StudyCardDto {
            id: card_id.clone(),
            format: StudyCardFormat::QuestionAnswer,
            deck_id: deck_id.clone(),
            question: row.get("question"),
            answer,
            options: vec![],
            correct_index: 0,
            explanation: row.get("explanation"),
            source,
            citations: citations.clone(),
            topic: row.get("lesson_title"),
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        };
        StudyRepository::insert_learning_card_in_transaction(&mut tx, &card).await?;
        let origin: String = row.get("origin");
        sqlx::query("INSERT INTO learning_card_origins(card_id,program_id,lesson_id,origin,source_ids_json,accepted_at) VALUES(?,?,?,?,?,?)")
            .bind(&card_id).bind(program_id).bind(&lesson_id).bind(&origin).bind(json(&source_ids)?).bind(now)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE learning_card_drafts SET status='accepted',accepted_card_id=?,updated_at=? WHERE id=? AND program_id=? AND status='pending'")
            .bind(&card_id).bind(now).bind(draft_id).bind(program_id).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(card_id)
    }

    pub async fn memory_state(&self, program_id: &str) -> Result<LearningMemoryState> {
        let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        if exists == 0 {
            return Err(AppError::NotFound("Learning program not found.".into()));
        }
        let root = sqlx::query("SELECT journal_id,deck_id FROM learning_memory WHERE program_id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let (journal_id, deck_id) = match root {
            Some(r) => (
                r.try_get::<Option<String>, _>("journal_id").map_err(db)?,
                r.try_get::<Option<String>, _>("deck_id").map_err(db)?,
            ),
            None => (None, None),
        };
        let lesson_notes = sqlx::query("SELECT lesson_id,note_id FROM learning_lesson_note_links WHERE program_id=? ORDER BY rowid")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| (r.get("lesson_id"),r.get("note_id"))).collect();
        let drafts = sqlx::query("SELECT * FROM learning_card_drafts WHERE program_id=? AND status='pending' ORDER BY created_at,id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| Ok(LearningCardDraftDto {
                id:r.get("id"),lesson_id:r.get("lesson_id"),question:r.get("question"),answer:r.get("answer"),explanation:r.get("explanation"),
                source_ids:decode(r.get("source_ids_json"))?,origin:parse_draft_origin(r.get("origin"))?,created_at:r.get("created_at"),
            })).collect::<Result<Vec<_>>>()?;
        let accepted_cards = sqlx::query("SELECT card_id,lesson_id,origin,source_ids_json,accepted_at FROM learning_card_origins WHERE program_id=? ORDER BY accepted_at,card_id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| Ok(LearningCardOriginDto {
                card_id:r.get("card_id"),lesson_id:r.get("lesson_id"),origin:parse_draft_origin(r.get("origin"))?,source_ids:decode(r.get("source_ids_json"))?,accepted_at:r.get("accepted_at"),
            })).collect::<Result<Vec<_>>>()?;
        Ok(LearningMemoryState {
            journal_id,
            deck_id,
            lesson_notes,
            drafts,
            accepted_cards,
        })
    }
}

fn parse_draft_origin(value: &str) -> Result<LearningCardDraftOrigin> {
    match value {
        "generated" => Ok(LearningCardDraftOrigin::Generated),
        "manual" => Ok(LearningCardDraftOrigin::Manual),
        _ => Err(AppError::Database(
            "Invalid learning card draft origin.".into(),
        )),
    }
}

async fn validate_memory_lesson(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    require_ready: bool,
) -> Result<()> {
    let row = sqlx::query("SELECT p.status,l.preparation FROM learning_programs p JOIN learning_lessons l ON l.program_id=p.id WHERE p.id=? AND l.id=?")
        .bind(program_id).bind(lesson_id).fetch_optional(&mut **tx).await.map_err(db)?
        .ok_or_else(|| AppError::NotFound("Lesson does not belong to this learning program.".into()))?;
    if row.get::<String, _>("status") != "active" {
        return Err(AppError::InvalidInput(
            "Accept the program before using its memory tools.".into(),
        ));
    }
    if require_ready && row.get::<String, _>("preparation") != "ready" {
        return Err(AppError::InvalidInput(
            "Prepare this lesson before using its memory tools.".into(),
        ));
    }
    Ok(())
}

async fn validate_draft_sources(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    source_ids: &[String],
    generated: bool,
) -> Result<()> {
    let cited_blocks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=? AND json_array_length(source_ids_json)>0")
        .bind(lesson_id).fetch_one(&mut **tx).await.map_err(db)?;
    if generated && source_ids.is_empty() && cited_blocks > 0 {
        return Err(AppError::InvalidInput(
            "Generated card drafts must cite lesson sources.".into(),
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for source_id in source_ids {
        if !unique.insert(source_id) {
            return Err(AppError::InvalidInput(
                "A learning card cannot cite a source more than once.".into(),
            ));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_sources WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(source_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if exists == 0 {
            return Err(AppError::InvalidInput(
                "Learning card cites a source outside this program.".into(),
            ));
        }
        if generated {
            let references: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM learning_blocks WHERE lesson_id=? AND EXISTS (SELECT 1 FROM json_each(learning_blocks.source_ids_json) WHERE value=?)) + (SELECT count(*) FROM learning_questions WHERE lesson_id=? AND EXISTS (SELECT 1 FROM json_each(learning_questions.source_ids_json) WHERE value=?))")
                .bind(lesson_id).bind(source_id).bind(lesson_id).bind(source_id).fetch_one(&mut **tx).await.map_err(db)?;
            if references == 0 {
                return Err(AppError::InvalidInput(
                    "Generated card sources must be referenced by its lesson.".into(),
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn ensure_memory_resources(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
) -> Result<(String, String)> {
    // First write serializes concurrent ensures before checking resource links.
    sqlx::query("INSERT OR IGNORE INTO learning_memory(program_id,journal_id,deck_id,created_at) VALUES(?,NULL,NULL,?)")
        .bind(program_id).bind(chrono::Utc::now().timestamp_millis()).execute(&mut **tx).await.map_err(db)?;
    let program =
        sqlx::query("SELECT title,goal,status,model_name FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found.".into()))?;
    if program.get::<String, _>("status") != "active" {
        return Err(AppError::InvalidInput(
            "Accept the program before creating its memory workspace.".into(),
        ));
    }
    // The nullable root lets either canonical resource be deleted independently.
    let root = sqlx::query("SELECT journal_id,deck_id FROM learning_memory WHERE program_id=?")
        .bind(program_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    let base_title: String = program.get("title");
    let safe_id: String = program_id.chars().take(8).collect();

    let journal_id: String = match root
        .try_get::<Option<String>, _>("journal_id")
        .map_err(db)?
    {
        Some(id) => id,
        None => {
            let id = format!("journal_learning_{program_id}");
            let name = format!(
                "Studio · {} · {safe_id}",
                base_title.chars().take(70).collect::<String>()
            );
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT OR IGNORE INTO journals(id,name,description,icon,is_archived,sort_order,created_at,updated_at) VALUES(?,?,?,?,0,0,?,?)")
                .bind(&id).bind(&name).bind("Learning Studio notebook").bind("book-open").bind(&now).bind(&now)
                .execute(&mut **tx).await.map_err(db)?;
            let canonical_id: String =
                sqlx::query_scalar("SELECT id FROM journals WHERE id=? OR name=?")
                    .bind(&id)
                    .bind(&name)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(db)?;
            sqlx::query("UPDATE learning_memory SET journal_id=? WHERE program_id=?")
                .bind(&canonical_id)
                .bind(program_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
            canonical_id
        }
    };

    let deck_id: String = match root.try_get::<Option<String>, _>("deck_id").map_err(db)? {
        Some(id) => id,
        None => {
            let id = format!("learning-studio-{program_id}");
            StudyRepository::insert_learning_deck_in_transaction(
                tx,
                &id,
                &base_title,
                &program.get::<String, _>("goal"),
                "Recall concepts from Learning Studio lessons",
                "learning-studio",
                chrono::Utc::now().timestamp_millis(),
            )
            .await?;
            sqlx::query("UPDATE learning_memory SET deck_id=? WHERE program_id=?")
                .bind(&id)
                .bind(program_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
            id
        }
    };
    Ok((journal_id, deck_id))
}
