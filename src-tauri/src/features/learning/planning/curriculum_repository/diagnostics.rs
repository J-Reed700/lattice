//! Placement diagnostics: starting, submitting, skipping and replaying attempts.
use super::{diagnostic_operation, validate_operation_id, LearningCurriculumRepository};
use crate::features::learning::curriculum::LearningCurriculumRevision;
use crate::features::learning::persistence::{db, decode, encode, hash, now};
use crate::features::learning::plan_dto::{
    LearningDiagnosticAttemptDto, LearningDiagnosticCoverageGapDto, LearningDiagnosticFindingDto,
    LearningDiagnosticPromptDto, LearningDiagnosticStatus, SkipLearningDiagnosticRequestDto,
    StartLearningDiagnosticRequestDto, SubmitLearningDiagnosticRequestDto,
};
use crate::features::learning::repository::LearningRepository;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;

impl LearningCurriculumRepository {
    /// The attempt an earlier identical diagnostic request produced, checked
    /// before any model work starts.
    pub async fn diagnostic_replay<T: serde::Serialize>(
        &self,
        operation_id: &str,
        program_id: &str,
        kind: &'static str,
        request: &T,
    ) -> Result<Option<LearningDiagnosticAttemptDto>> {
        validate_operation_id(operation_id)?;
        let payload = hash(request)?;
        match diagnostic_operation(operation_id, program_id, kind, &payload)
            .replay::<String, _>(&self.pool)
            .await?
        {
            Some(id) => Ok(Some(self.diagnostic(&id).await?)),
            None => Ok(None),
        }
    }

    pub async fn diagnostic_for_program(
        &self,
        program_id: &str,
        diagnostic_id: &str,
    ) -> Result<LearningDiagnosticAttemptDto> {
        let attempt = self.diagnostic(diagnostic_id).await?;
        if attempt.program_id != program_id {
            return Err(AppError::NotFound(
                "Diagnostic not found in this program".into(),
            ));
        }
        Ok(attempt)
    }

    pub async fn diagnostic_tasks(
        &self,
        program_id: &str,
        diagnostic_id: &str,
    ) -> Result<Vec<crate::features::learning::diagnostic_generation::DiagnosticTask>> {
        let encoded: String = sqlx::query_scalar(
            "SELECT evaluation_json FROM learning_diagnostic_attempts WHERE id=? AND program_id=?",
        )
        .bind(diagnostic_id)
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Diagnostic not found".into()))?;
        decode(&encoded)
    }

    pub async fn start_diagnostic(
        &self,
        request: &StartLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        self.start_diagnostic_authored(request, None).await
    }

    pub async fn start_diagnostic_authored(
        &self,
        request: &StartLearningDiagnosticRequestDto,
        tasks: Option<&[crate::features::learning::diagnostic_generation::DiagnosticTask]>,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        self.seed_accepted(&request.program_id).await?;
        let program = LearningRepository::new(self.pool.clone())
            .get(&request.program_id)
            .await?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation = diagnostic_operation(
            &request.operation_id,
            &request.program_id,
            "start",
            &payload,
        );
        if let Some(id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.diagnostic(&id).await;
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM learning_programs WHERE id=? AND status='active'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Active learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let accepted_json: String = sqlx::query_scalar("SELECT snapshot_json FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(&request.program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let accepted: LearningCurriculumRevision = decode(&accepted_json)?;
        let mut prompts = Vec::new();
        for module in &accepted.modules {
            let source_module = program.modules.iter().find(|source| source.id == module.id);
            let source_ids: Vec<String> = module
                .lessons
                .iter()
                .flat_map(|lesson| {
                    source_module
                        .and_then(|source| source.lessons.iter().find(|item| item.id == lesson.id))
                        .into_iter()
                        .flat_map(|item| {
                            item.blocks
                                .iter()
                                .flat_map(|block| block.source_ids.clone())
                                .chain(
                                    item.questions
                                        .iter()
                                        .flat_map(|question| question.source_ids.clone()),
                                )
                        })
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for outcome in &module.outcome_ids {
                let title = sqlx::query_scalar::<_, String>(
                    "SELECT title FROM learning_outcome_definitions WHERE program_id=? AND id=?",
                )
                .bind(&request.program_id)
                .bind(outcome)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| {
                    AppError::InvalidState(
                        "Accepted curriculum references a missing learning outcome.".into(),
                    )
                })?;
                let prompt = if let Some(tasks) = tasks {
                    let Some(task) = tasks.iter().find(|t| &t.outcome_id == outcome) else {
                        continue;
                    };
                    task.prompt.clone()
                } else {
                    format!("Self-inventory: What do you currently understand about {title}? Share a question or example if useful.")
                };
                prompts.push(LearningDiagnosticPromptDto {
                    id: uuid::Uuid::new_v4().to_string(),
                    prompt,
                    outcome_id: outcome.clone(),
                    outcome_title: title,
                    source_version_ids: source_ids.clone(),
                });
            }
        }
        if prompts.is_empty() || tasks.is_some_and(|t| t.len() != prompts.len() || t.len() > 6) {
            return Err(AppError::InvalidState(
                "The accepted program has no outcomes for a diagnostic.".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        sqlx::query("INSERT INTO learning_diagnostic_attempts(id,program_id,operation_id,status,prompt_json,response_json,findings_json,source_coverage_gaps_json,created_at,submitted_at,evaluation_json) VALUES(?,?,?,'active',?,'[]','[]','[]',?,NULL,?)")
            .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(encode(&prompts)?).bind(created).bind(encode(&tasks.unwrap_or_default())?).execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &id).await?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&id).await
    }

    pub async fn submit_diagnostic(
        &self,
        request: &SubmitLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        self.submit_diagnostic_evaluated(request, None).await
    }

    pub async fn submit_diagnostic_evaluated(
        &self,
        request: &SubmitLearningDiagnosticRequestDto,
        findings: Option<&[LearningDiagnosticFindingDto]>,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        if let Some(replay) = self
            .diagnostic_replay(
                &request.operation_id,
                &request.program_id,
                "submit",
                request,
            )
            .await?
        {
            return Ok(replay);
        }
        let attempt = self
            .diagnostic_for_program(&request.program_id, &request.diagnostic_id)
            .await?;
        let tasks = self
            .diagnostic_tasks(&request.program_id, &request.diagnostic_id)
            .await?;
        if !tasks.is_empty() && !request.save_only {
            crate::features::learning::diagnostic_generation::validate_findings(
                &attempt,
                &request.responses,
                findings.ok_or_else(|| {
                    AppError::InvalidInput(
                        "These performance tasks require feedback before submission.".into(),
                    )
                })?,
            )?;
        } else if findings.is_some() {
            return Err(AppError::InvalidInput(
                "A self-inventory cannot be presented as a scored diagnostic.".into(),
            ));
        }
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation = diagnostic_operation(
            &request.operation_id,
            &request.program_id,
            "submit",
            &payload,
        );
        if let Some(id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.diagnostic(&id).await;
        }
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let row=sqlx::query("SELECT prompt_json,status,revision FROM learning_diagnostic_attempts WHERE id=? AND program_id=?").bind(&request.diagnostic_id).bind(&request.program_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Diagnostic attempt not found".into()))?;
        if row.get::<String, _>("status") != "active" {
            return Err(AppError::InvalidState(
                "This diagnostic attempt is already closed.".into(),
            ));
        }
        let diagnostic_revision: i64 = row.get("revision");
        if request
            .expected_diagnostic_revision
            .is_some_and(|revision| revision != diagnostic_revision)
            || (!tasks.is_empty() && request.expected_diagnostic_revision.is_none())
        {
            return Err(AppError::InvalidState(
                "The starting-point answers changed elsewhere. Reload before saving.".into(),
            ));
        }
        let prompts: Vec<LearningDiagnosticPromptDto> = decode(row.get("prompt_json"))?;
        let expected: std::collections::HashSet<_> =
            prompts.iter().map(|p| p.id.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        for response in &request.responses {
            if !expected.contains(response.prompt_id.as_str())
                || !seen.insert(response.prompt_id.as_str())
            {
                return Err(AppError::InvalidInput(
                    "Diagnostic responses must use each prompt ID at most once.".into(),
                ));
            }
            if response.response.trim().chars().count() > 4000 {
                return Err(AppError::InvalidInput(
                    "Diagnostic responses must be 4000 characters or fewer.".into(),
                ));
            }
            if response.response.trim().is_empty() && !request.save_only {
                return Err(AppError::InvalidInput(
                    "Answer every self-inventory prompt or skip the diagnostic.".into(),
                ));
            }
        }
        if seen.len() != expected.len() && !request.save_only {
            return Err(AppError::InvalidInput(
                "Answer every self-inventory prompt or skip the diagnostic.".into(),
            ));
        }
        let responses_json = encode(&request.responses)?;
        let mut gaps = Vec::new();
        for prompt in &prompts {
            if prompt.source_version_ids.is_empty() {
                gaps.push(LearningDiagnosticCoverageGapDto {
                    outcome_id: prompt.outcome_id.clone(),
                    outcome_title: prompt.outcome_title.clone(),
                    source_version_ids: vec![],
                });
            }
        }
        sqlx::query("UPDATE learning_diagnostic_attempts SET status=?,response_json=?,findings_json=?,source_coverage_gaps_json=?,submitted_at=?,revision=revision+1 WHERE id=? AND status='active'")
            .bind(if request.save_only { "active" } else { "submitted" }).bind(responses_json).bind(encode(&findings.unwrap_or_default())?).bind(encode(&gaps)?).bind(if request.save_only { None } else { Some(now()) }).bind(&request.diagnostic_id).execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &request.diagnostic_id).await?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&request.diagnostic_id).await
    }

    pub async fn skip_diagnostic(
        &self,
        request: &SkipLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation =
            diagnostic_operation(&request.operation_id, &request.program_id, "skip", &payload);
        if let Some(id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.diagnostic(&id).await;
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM learning_programs WHERE id=? AND status='active'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Active learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        let prompts = serde_json::json!([]);
        sqlx::query("INSERT INTO learning_diagnostic_attempts(id,program_id,operation_id,status,prompt_json,response_json,findings_json,source_coverage_gaps_json,created_at,submitted_at) VALUES(?,?,?,'skipped',?,'[]','[]','[]',?,?)")
            .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(prompts.to_string()).bind(created).bind(created).execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &id).await?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&id).await
    }

    async fn diagnostic(&self, id: &str) -> Result<LearningDiagnosticAttemptDto> {
        let row = sqlx::query("SELECT * FROM learning_diagnostic_attempts WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Diagnostic attempt not found".into()))?;
        decode_diagnostic_row(&row)
    }
}

pub(super) fn decode_diagnostic_row(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<LearningDiagnosticAttemptDto> {
    let status = match r.get::<String, _>("status").as_str() {
        "active" => LearningDiagnosticStatus::Active,
        "submitted" => LearningDiagnosticStatus::Submitted,
        "skipped" => LearningDiagnosticStatus::Skipped,
        _ => return Err(AppError::Database("Invalid diagnostic status".into())),
    };
    Ok(LearningDiagnosticAttemptDto {
        id: r.get("id"),
        program_id: r.get("program_id"),
        status,
        revision: r.get("revision"),
        prompts: decode(r.get("prompt_json"))?,
        responses: decode(r.get("response_json"))?,
        source_coverage_gaps: decode(r.get("source_coverage_gaps_json"))?,
        findings: decode(r.get("findings_json"))?,
        interpretation: if r.get::<String, _>("evaluation_json") == "[]" {
            "Self-inventory only. Responses are not scored and do not establish mastery.".into()
        } else {
            "Starting-point feedback is provisional. Use it to choose practice or a challenge; it does not establish mastery.".into()
        },
        created_at: r.get("created_at"),
        submitted_at: r.get("submitted_at"),
    })
}
