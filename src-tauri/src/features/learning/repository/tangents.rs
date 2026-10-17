//! Persistence for study guide tangents: seeded conversations branching from lesson text.

use super::*;
use lattice_core::shared::error::{AppError, Result};

impl LearningRepository {
    pub async fn create_tangent(
        &self,
        request: &CreateLearningTangentRequestDto,
    ) -> Result<LearningTangentDto> {
        let trimmed = request.selected_text.trim();
        let char_count = trimmed.chars().count();
        if char_count == 0 || char_count > 8_000 {
            return Err(AppError::InvalidInput(
                "Select between 1 and 8,000 characters for a tangent.".into(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(db)?;

        #[derive(sqlx::FromRow)]
        struct ProgramRow {
            title: String,
            goal: String,
        }

        let program = sqlx::query_as::<_, ProgramRow>(
            "SELECT title, goal FROM learning_programs WHERE id = ?",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| {
            AppError::NotFound(format!("Learning program {} not found", request.program_id))
        })?;

        #[derive(sqlx::FromRow)]
        struct ModuleRow {
            title: String,
            summary: String,
        }

        let module = sqlx::query_as::<_, ModuleRow>(
            "SELECT title, summary FROM learning_modules WHERE id = ? AND program_id = ?",
        )
        .bind(&request.module_id)
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| {
            AppError::NotFound(format!("Learning module {} not found", request.module_id))
        })?;

        #[derive(sqlx::FromRow)]
        struct LessonRow {
            title: String,
            objective: String,
        }

        let lesson = sqlx::query_as::<_, LessonRow>(
            "SELECT title, objective FROM learning_lessons WHERE id = ? AND program_id = ? AND module_id = ?",
        )
        .bind(&request.lesson_id)
        .bind(&request.program_id)
        .bind(&request.module_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| {
            AppError::NotFound(format!("Learning lesson {} not found", request.lesson_id))
        })?;

        let section_title = if let Some(title) = request
            .section_title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(title.to_string())
        } else if let Some(idx) = request.section_index {
            let block_title: Option<String> = sqlx::query_scalar(
                "SELECT title FROM learning_blocks WHERE lesson_id = ? AND ordinal = ?",
            )
            .bind(&request.lesson_id)
            .bind(idx)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            block_title
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        } else {
            None
        };

        let now_utc = chrono::Utc::now();
        let now_epoch = now_utc.timestamp();
        let now_iso = now_utc.to_rfc3339();

        let space_id = format!("space_learning_{}", request.program_id);
        let space_exists: bool =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
                .bind(&space_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?
                > 0;

        if !space_exists {
            let base_name = format!("Course · {}", program.title);
            let name_taken: bool = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM conversation_spaces WHERE LOWER(name) = LOWER(?)",
            )
            .bind(&base_name)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?
                > 0;

            let space_name = if name_taken {
                let suffix: String = request.program_id.chars().take(6).collect();
                format!("Course · {} ({})", program.title, suffix)
            } else {
                base_name
            };

            let space_prompt = format!(
                "You are a study guide tutor for the course \"{}\" (Goal: {}). Help the learner explore side-questions and tangents deeply while connecting insights back to the course curriculum.",
                program.title, program.goal
            );
            let space_desc = format!("Study space for course \"{}\"", program.title);

            sqlx::query(
                r#"
                INSERT OR IGNORE INTO conversation_spaces (
                    id, name, description, icon, accent_color, space_prompt,
                    tool_preferences_json, is_archived, sort_order, created_at, updated_at
                ) VALUES (?, ?, ?, 'graduation-cap', NULL, ?, NULL, 0, 0, ?, ?)
                "#,
            )
            .bind(&space_id)
            .bind(&space_name)
            .bind(&space_desc)
            .bind(&space_prompt)
            .bind(&now_iso)
            .bind(&now_iso)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO collaborator_profiles (
                id, display_name, email, avatar_url, created_at, updated_at
            ) VALUES ('member_local_owner', 'Local Owner', NULL, NULL, ?, ?)
            "#,
        )
        .bind(&now_iso)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_space_members (
                space_id, member_id, role, created_at, updated_at
            ) VALUES (?, 'member_local_owner', 'owner', ?, ?)
            "#,
        )
        .bind(&space_id)
        .bind(&now_iso)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let conversation_id = uuid::Uuid::new_v4().to_string();

        let collapsed_title = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = if collapsed_title.chars().count() > 80 {
            let mut prefix: String = collapsed_title.chars().take(79).collect();
            prefix.push('…');
            prefix
        } else {
            collapsed_title
        };

        let mut system_prompt = format!(
            "You are an interactive AI study tutor for the course \"{}\".\nCourse Goal: {}\n\nCurrent Context:\n- Module: {}\n  Summary: {}\n- Lesson: {}\n  Objective: {}\n",
            program.title, program.goal, module.title, module.summary, lesson.title, lesson.objective
        );
        if let Some(sec) = &section_title {
            system_prompt.push_str(&format!("- Section: {}\n", sec));
        }
        system_prompt.push_str(&format!(
            "\nThe learner initiated this tangent from the following study guide passage:\n\"\"\"\n{}\n\"\"\"\nHelp the learner explore their questions deeply while connecting insights back to the course concepts.",
            trimmed
        ));

        sqlx::query(
            r#"
            INSERT INTO conversations (
                id, title, system_prompt, space_id,
                created_at, updated_at, message_count, total_tokens,
                next_message_sequence,
                tangent_parent_id, tangent_selection, tangent_context_message_count
            ) VALUES (?, ?, ?, ?, ?, ?, 1, 0, 2, NULL, ?, 1)
            "#,
        )
        .bind(&conversation_id)
        .bind(&title)
        .bind(&system_prompt)
        .bind(&space_id)
        .bind(&now_iso)
        .bind(&now_iso)
        .bind(trimmed)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let message_id = uuid::Uuid::new_v4().to_string();
        let quoted_selection = trimmed
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let message_content = if let Some(sec) = &section_title {
            format!(
                "**[Course: {} · {} › {} ({})]**\n\n{}\n\n---\n*(Tangent started from study guide selection)*",
                program.title, module.title, lesson.title, sec, quoted_selection
            )
        } else {
            format!(
                "**[Course: {} · {} › {}]**\n\n{}\n\n---\n*(Tangent started from study guide selection)*",
                program.title, module.title, lesson.title, quoted_selection
            )
        };
        let content_digest = format!("sha256:{}", hash_text(&message_content));

        sqlx::query(
            r#"
            INSERT INTO conversation_messages (
                id, conversation_id, role, content, tokens, created_at, metadata, status, sequence, content_digest
            ) VALUES (?, ?, 'assistant', ?, 0, ?, NULL, 'completed', 1, ?)
            "#,
        )
        .bind(&message_id)
        .bind(&conversation_id)
        .bind(&message_content)
        .bind(&now_iso)
        .bind(&content_digest)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let mut candidate_doc_ids = Vec::new();
        let library_doc_ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT origin FROM learning_source_library WHERE program_id = ? AND kind = 'document' AND deleted_at IS NULL",
        )
        .bind(&request.program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        candidate_doc_ids.extend(library_doc_ids);

        let source_doc_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM learning_sources WHERE program_id = ? AND url IS NULL",
        )
        .bind(&request.program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        candidate_doc_ids.extend(source_doc_ids);

        candidate_doc_ids.sort();
        candidate_doc_ids.dedup();

        for doc_id in candidate_doc_ids {
            let exists: bool =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE id = ?")
                    .bind(&doc_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?
                    > 0;

            if exists {
                sqlx::query(
                    "INSERT OR IGNORE INTO conversation_documents (conversation_id, document_id, added_at) VALUES (?, ?, ?)",
                )
                .bind(&conversation_id)
                .bind(&doc_id)
                .bind(&now_iso)
                .execute(&mut *tx)
                .await
                .map_err(db)?;

                sqlx::query(
                    "INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(&doc_id)
                .bind(&space_id)
                .bind(&now_iso)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            }
        }

        #[derive(sqlx::FromRow)]
        struct WebSourceRow {
            url: Option<String>,
            title: String,
            excerpt: String,
        }

        let web_sources = sqlx::query_as::<_, WebSourceRow>(
            "SELECT url, title, excerpt FROM learning_sources WHERE program_id = ? AND url IS NOT NULL",
        )
        .bind(&request.program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        for ws in web_sources {
            if let Some(url) = ws.url {
                let web_id = uuid::Uuid::new_v4().to_string();
                let normalized = url.trim().to_lowercase();
                sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO conversation_web_sources (
                        id, conversation_id, url, normalized_url, title, excerpt, added_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&web_id)
                .bind(&conversation_id)
                .bind(&url)
                .bind(&normalized)
                .bind(&ws.title)
                .bind(&ws.excerpt)
                .bind(&now_iso)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            }
        }

        sqlx::query(
            r#"
            INSERT INTO learning_tangents (
                conversation_id, program_id, module_id, lesson_id,
                section_index, section_title, selected_text,
                context_message_count, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)
            "#,
        )
        .bind(&conversation_id)
        .bind(&request.program_id)
        .bind(&request.module_id)
        .bind(&request.lesson_id)
        .bind(request.section_index)
        .bind(&section_title)
        .bind(trimmed)
        .bind(now_epoch)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(LearningTangentDto {
            conversation_id,
            program_id: request.program_id.clone(),
            program_title: program.title,
            space_id,
            module_id: request.module_id.clone(),
            module_title: module.title,
            lesson_id: request.lesson_id.clone(),
            lesson_title: lesson.title,
            section_index: request.section_index,
            section_title,
            selected_text: trimmed.to_string(),
            title,
            created_at: now_epoch,
            updated_at: now_iso,
            message_count: 1,
            context_message_count: 1,
        })
    }

    pub async fn list_tangents(&self, program_id: Option<&str>) -> Result<Vec<LearningTangentDto>> {
        let pid = program_id.map(str::trim).filter(|s| !s.is_empty());
        if let Some(pid) = pid {
            sqlx::query_as::<_, LearningTangentDto>(
                r#"
                SELECT
                    lt.conversation_id,
                    lt.program_id,
                    p.title AS program_title,
                    COALESCE(c.space_id, 'space_learning_' || lt.program_id) AS space_id,
                    lt.module_id,
                    m.title AS module_title,
                    lt.lesson_id,
                    l.title AS lesson_title,
                    lt.section_index,
                    lt.section_title,
                    lt.selected_text,
                    c.title,
                    lt.created_at,
                    c.updated_at,
                    c.message_count,
                    lt.context_message_count
                FROM learning_tangents lt
                INNER JOIN conversations c ON c.id = lt.conversation_id
                INNER JOIN learning_programs p ON p.id = lt.program_id
                INNER JOIN learning_modules m ON m.id = lt.module_id AND m.program_id = lt.program_id
                INNER JOIN learning_lessons l ON l.id = lt.lesson_id AND l.program_id = lt.program_id
                WHERE lt.program_id = ?
                ORDER BY c.updated_at DESC, lt.created_at DESC
                "#,
            )
            .bind(pid)
            .fetch_all(&self.pool)
            .await
            .map_err(db)
        } else {
            sqlx::query_as::<_, LearningTangentDto>(
                r#"
                SELECT
                    lt.conversation_id,
                    lt.program_id,
                    p.title AS program_title,
                    COALESCE(c.space_id, 'space_learning_' || lt.program_id) AS space_id,
                    lt.module_id,
                    m.title AS module_title,
                    lt.lesson_id,
                    l.title AS lesson_title,
                    lt.section_index,
                    lt.section_title,
                    lt.selected_text,
                    c.title,
                    lt.created_at,
                    c.updated_at,
                    c.message_count,
                    lt.context_message_count
                FROM learning_tangents lt
                INNER JOIN conversations c ON c.id = lt.conversation_id
                INNER JOIN learning_programs p ON p.id = lt.program_id
                INNER JOIN learning_modules m ON m.id = lt.module_id AND m.program_id = lt.program_id
                INNER JOIN learning_lessons l ON l.id = lt.lesson_id AND l.program_id = lt.program_id
                ORDER BY c.updated_at DESC, lt.created_at DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(db)
        }
    }
}
