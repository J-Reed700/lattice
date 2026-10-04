//! Program-scoped source selectors, retrieval and versioned recall cards.
use super::{
    portability_dto::*,
    source_library::LearningSourceLibraryRepository,
    source_selector::{create_quote_selector, match_quote_selector, LearningQuoteMatch},
};
use crate::{
    application::ports::EmbeddingPort,
    features::{study::dto::*, study::schedule::next_review},
    shared::error::{AppError, Result},
};
use fsrs::{MemoryState, FSRS};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

const MAX_CARDS: i64 = 2_000;
const MAX_QUERY_CHARS: usize = 240;
const DAY_MS: i64 = 86_400_000;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| AppError::Serialization(error.to_string()))
}
fn payload_hash<T: Serialize>(value: &T) -> Result<String> {
    use sha2::Digest;
    let bytes =
        serde_json::to_vec(value).map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}
fn validate_uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID.")))
}
fn validate_owned_id(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(invalid(format!("Invalid {label} ID.")));
    }
    Ok(())
}
fn scheduler_name(value: LearningRecallSchedulerVersion) -> &'static str {
    match value {
        LearningRecallSchedulerVersion::ExpandingV1 => "expanding_v1",
        LearningRecallSchedulerVersion::Fsrs6V1 => "fsrs_6_v1",
    }
}
fn parse_scheduler(value: &str) -> Result<LearningRecallSchedulerVersion> {
    match value {
        "expanding_v1" => Ok(LearningRecallSchedulerVersion::ExpandingV1),
        "fsrs_6_v1" => Ok(LearningRecallSchedulerVersion::Fsrs6V1),
        _ => Err(AppError::InvalidState(
            "Unknown recall scheduler version.".into(),
        )),
    }
}
fn format_name(value: LearningRecallCardFormat) -> &'static str {
    match value {
        LearningRecallCardFormat::MultipleChoice => "multiple_choice",
        LearningRecallCardFormat::QuestionAnswer => "question_answer",
        LearningRecallCardFormat::Cloze => "cloze",
        LearningRecallCardFormat::Reverse => "reverse",
        LearningRecallCardFormat::CodePrediction => "code_prediction",
        LearningRecallCardFormat::Reconstruction => "reconstruction",
    }
}
fn parse_format(value: &str) -> Result<LearningRecallCardFormat> {
    match value {
        "multiple_choice" => Ok(LearningRecallCardFormat::MultipleChoice),
        "question_answer" => Ok(LearningRecallCardFormat::QuestionAnswer),
        "cloze" => Ok(LearningRecallCardFormat::Cloze),
        "reverse" => Ok(LearningRecallCardFormat::Reverse),
        "code_prediction" => Ok(LearningRecallCardFormat::CodePrediction),
        "reconstruction" => Ok(LearningRecallCardFormat::Reconstruction),
        _ => Err(AppError::InvalidState("Unknown recall card format.".into())),
    }
}
fn rating_name(rating: StudyRating) -> &'static str {
    match rating {
        StudyRating::Again => "again",
        StudyRating::Hard => "hard",
        StudyRating::Good => "good",
        StudyRating::Easy => "easy",
    }
}
fn parse_rating(value: &str) -> Result<StudyRating> {
    match value {
        "again" => Ok(StudyRating::Again),
        "hard" => Ok(StudyRating::Hard),
        "good" => Ok(StudyRating::Good),
        "easy" => Ok(StudyRating::Easy),
        _ => Err(AppError::InvalidState(
            "Unknown stored recall rating.".into(),
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchedulerState {
    version: LearningRecallSchedulerVersion,
    stability: Option<f64>,
    difficulty: Option<f64>,
    last_reviewed_at: Option<i64>,
    due_at: i64,
    interval_days: i64,
    review_count: i64,
}
impl SchedulerState {
    fn dto(&self) -> LearningRecallSchedulerStateDto {
        LearningRecallSchedulerStateDto {
            scheduler_version: self.version,
            stability: self.stability,
            difficulty: self.difficulty,
            last_reviewed_at: self.last_reviewed_at,
            due_at: self.due_at,
            interval_days: self.interval_days,
            review_count: self.review_count,
        }
    }
    fn from_json(value: &str) -> Result<Self> {
        let state: Self = serde_json::from_str(value)
            .map_err(|error| AppError::InvalidState(format!("Invalid scheduler state: {error}")))?;
        if !state.stability.is_none_or(f64::is_finite)
            || !state.difficulty.is_none_or(f64::is_finite)
            || state.review_count < 0
            || state.interval_days < 0
        {
            return Err(AppError::InvalidState(
                "Invalid recall scheduler state.".into(),
            ));
        }
        Ok(state)
    }
    fn fsrs_memory(&self) -> Option<MemoryState> {
        Some(MemoryState {
            stability: self.stability? as f32,
            difficulty: self.difficulty? as f32,
        })
    }
}

#[derive(Debug, Clone)]
struct SourceSelectorRecord {
    id: String,
    source_id: String,
    source_version_id: String,
    selector: super::source_selector::LearningTextQuoteSelector,
    match_status: super::source_selector::LearningQuoteMatchStatus,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
    candidate_count: usize,
    created_at: i64,
}
impl SourceSelectorRecord {
    fn dto(self) -> LearningSourceSelectorDto {
        LearningSourceSelectorDto {
            id: self.id,
            source_id: self.source_id,
            source_version_id: self.source_version_id,
            selector: self.selector,
            match_status: self.match_status,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            candidate_count: self.candidate_count,
            created_at: self.created_at,
        }
    }
}

#[derive(Clone)]
pub struct LearningRecallRepository {
    pool: SqlitePool,
}
impl LearningRecallRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_selector(
        &self,
        req: &CreateLearningSourceSelectorRequestDto,
    ) -> Result<LearningSourceSelectorDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.selector_id, "selector")?;
        validate_uuid(&req.program_id, "program")?;
        validate_owned_id(&req.source_id, "source")?;
        validate_owned_id(&req.source_version_id, "source version")?;
        let hash = payload_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        if let Some(existing) = operation_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "create_selector",
            &hash,
        )
        .await?
        {
            let selector_id = existing.ok_or_else(|| {
                AppError::InvalidState("Selector operation has no result ID.".into())
            })?;
            tx.commit().await.map_err(db)?;
            return self.get_selector(&req.program_id, &selector_id).await;
        }
        let version = sqlx::query("SELECT full_text FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?")
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(&req.source_version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Source version not found in this program.".into()))?;
        let text: String = version.get("full_text");
        let selector = create_quote_selector(&text, req.start_byte, req.end_byte)?;
        let matched = match_quote_selector(&text, &selector)?;
        let timestamp = now();
        sqlx::query("INSERT INTO learning_source_selectors(id,program_id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&req.selector_id)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(&req.source_version_id)
            .bind(&selector.exact)
            .bind(&selector.prefix)
            .bind(&selector.suffix)
            .bind(matched.start_byte.map(|x| x as i64))
            .bind(matched.end_byte.map(|x| x as i64))
            .bind(matched.candidate_count as i64)
            .bind(selector_status_name(&matched.status))
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "create_selector",
            &hash,
            Some(&req.selector_id),
            timestamp,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.get_selector(&req.program_id, &req.selector_id).await
    }

    pub async fn get_selector(
        &self,
        program_id: &str,
        selector_id: &str,
    ) -> Result<LearningSourceSelectorDto> {
        validate_uuid(program_id, "program")?;
        validate_uuid(selector_id, "selector")?;
        let row =
            sqlx::query("SELECT * FROM learning_source_selectors WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(selector_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Source selector not found.".into()))?;
        parse_selector(&row).map(SourceSelectorRecord::dto)
    }

    pub async fn match_selector(
        &self,
        req: &MatchLearningSourceSelectorRequestDto,
    ) -> Result<LearningQuoteMatch> {
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.selector_id, "selector")?;
        validate_uuid(&req.target_version_id, "source version")?;
        let row = sqlx::query("SELECT s.source_id,s.exact_text,s.prefix_text,s.suffix_text,v.full_text FROM learning_source_selectors s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.source_id=s.source_id WHERE s.program_id=? AND s.id=? AND v.id=?")
            .bind(&req.program_id)
            .bind(&req.selector_id)
            .bind(&req.target_version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Selector or source version not found.".into()))?;
        let selector = super::source_selector::LearningTextQuoteSelector {
            exact: row.get("exact_text"),
            prefix: row.get("prefix_text"),
            suffix: row.get("suffix_text"),
        };
        let text: String = row.get("full_text");
        match_quote_selector(&text, &selector)
    }

    pub async fn search_sources(
        &self,
        req: &SearchLearningSourcesSemanticallyRequestDto,
        embedder: Option<&dyn EmbeddingPort>,
    ) -> Result<Vec<LearningSourceSemanticSearchResultDto>> {
        validate_uuid(&req.program_id, "program")?;
        let query = req.query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Err(invalid(
                "Source search query must contain 1–240 characters.",
            ));
        }
        let library = LearningSourceLibraryRepository::new(self.pool.clone());
        let sources = library.verification_sources(&req.program_id).await?;
        let collection = super::reference_collection::ReferenceCollection::load(
            &self.pool,
            &req.program_id,
            &sources,
            embedder,
        )
        .await?;
        let passages = collection.retrieve(query, req.limit).await?;
        let workspace = library.workspace(&req.program_id).await?;
        let mut results = Vec::new();
        for passage in passages {
            let item = workspace
                .sources
                .iter()
                .find(|s| s.active_version_id.as_deref() == Some(&passage.source_id))
                .ok_or_else(|| {
                    invalid("The reference collection changed during search. Search again.")
                })?;
            let version = item
                .active_version
                .clone()
                .ok_or_else(|| invalid("The source version is unavailable."))?;
            let source = sources
                .iter()
                .find(|s| s.id == passage.source_id)
                .ok_or_else(|| invalid("The source version is unavailable."))?;
            results.push(LearningSourceSemanticSearchResultDto {
                source_id: item.id.clone(),
                version,
                excerpt: passage.text.clone(),
                score: passage.score,
                retrieval_kind: passage.retrieval_kind,
                selector: create_quote_selector(
                    &source.excerpt,
                    passage.start_byte,
                    passage.end_byte,
                )?,
            });
        }
        Ok(results)
    }

    pub async fn workspace(&self, program_id: &str) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(program_id, "program")?;
        let program_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if program_exists.is_none() {
            return Err(AppError::NotFound("Learning program not found.".into()));
        }
        self.bootstrap_legacy_cards(program_id).await?;
        let deck_id: Option<String> =
            sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .flatten();
        let Some(deck_id) = deck_id else {
            return Ok(LearningRecallWorkspaceDto {
                program_id: program_id.into(),
                cards: vec![],
                duplicates: vec![],
                due_count: 0,
                fsrs_available: true,
                scheduler_disclosure: "FSRS 6 scheduling is available. Scheduler changes replay the complete Study review history and can be reversed.".into(),
            });
        };
        let rows = sqlx::query("SELECT p.*,c.deck_id,c.question,c.answer,c.explanation,c.options_json,c.correct_index,c.due_at,c.interval_days,c.review_count,c.lapses,c.source_json FROM learning_recall_card_profiles p JOIN study_cards c ON c.id=p.card_id WHERE p.program_id=? AND c.deck_id=? ORDER BY p.created_at,c.id")
            .bind(program_id)
            .bind(&deck_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut cards = Vec::with_capacity(rows.len());
        for row in &rows {
            let card_id: String = row.get("card_id");
            let format = parse_format(row.get("format"))?;
            let content = content_from_row(row, format)?;
            let versions = self.card_versions(&card_id).await?;
            let state = SchedulerState::from_json(row.get("scheduler_state_json"))?;
            if state.version != parse_scheduler(row.get("scheduler_version"))? {
                return Err(AppError::InvalidState(
                    "Recall scheduler version does not match its state.".into(),
                ));
            }
            cards.push(LearningRecallCardDto {
                id: card_id,
                format,
                content,
                source_version_ids: serde_json::from_str(row.get("source_version_ids_json"))
                    .map_err(|error| {
                        AppError::InvalidState(format!("Invalid recall source references: {error}"))
                    })?,
                content_revision: row.get("content_revision"),
                scheduler: state.dto(),
                versions,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }
        let duplicates = self.duplicate_suggestions(program_id).await?;
        let due_count = cards
            .iter()
            .filter(|card| card.scheduler.due_at <= now())
            .count() as i64;
        Ok(LearningRecallWorkspaceDto {
            program_id: program_id.into(),
            cards,
            duplicates,
            due_count,
            fsrs_available: true,
            scheduler_disclosure: "FSRS 6 scheduling is available. Scheduler changes replay the complete Study review history and can be reversed.".into(),
        })
    }

    async fn bootstrap_legacy_cards(&self, program_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let deck_id: Option<String> =
            sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .flatten();
        let Some(deck_id) = deck_id else {
            tx.commit().await.map_err(db)?;
            return Ok(());
        };
        let rows = sqlx::query("SELECT c.* FROM study_cards c LEFT JOIN learning_recall_card_profiles p ON p.card_id=c.id WHERE c.deck_id=? AND p.card_id IS NULL ORDER BY c.id")
            .bind(&deck_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        for row in rows {
            let card_id: String = row.get("id");
            let study_format =
                crate::features::study::repository::parse_card_format(row.get("format"))?;
            let format = match study_format {
                StudyCardFormat::MultipleChoice => LearningRecallCardFormat::MultipleChoice,
                StudyCardFormat::QuestionAnswer => LearningRecallCardFormat::QuestionAnswer,
            };
            let content = LearningRecallContentDto {
                prompt: row.get("question"),
                answer: row.get("answer"),
                explanation: row.get("explanation"),
                options: serde_json::from_str(row.get("options_json")).map_err(|error| {
                    AppError::InvalidState(format!("Invalid Study options: {error}"))
                })?,
                correct_option_index: (format == LearningRecallCardFormat::MultipleChoice)
                    .then(|| row.get::<i64, _>("correct_index").max(0) as usize),
                language: None,
                cloze_deletions: vec![],
            };
            validate_content(format, &content)?;
            let source_ids = self
                .card_source_versions(&mut tx, program_id, &card_id)
                .await?;
            let last_reviewed: Option<i64> =
                sqlx::query_scalar("SELECT max(reviewed_at) FROM study_reviews WHERE card_id=?")
                    .bind(&card_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
            let state = SchedulerState {
                version: LearningRecallSchedulerVersion::ExpandingV1,
                stability: None,
                difficulty: None,
                last_reviewed_at: last_reviewed,
                due_at: row.get("due_at"),
                interval_days: row.get("interval_days"),
                review_count: row.get("review_count"),
            };
            let timestamp = now();
            let prompt_json = json(&content.prompt)?;
            let answer_json = json(&content)?;
            sqlx::query("INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,scheduler_version,scheduler_state_json,content_revision,created_at,updated_at) VALUES(?,?,?,?,?,?,'expanding_v1',?,1,?,?)")
                .bind(&card_id)
                .bind(program_id)
                .bind(format_name(format))
                .bind(prompt_json)
                .bind(answer_json)
                .bind(json(&source_ids)?)
                .bind(json(&state)?)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("INSERT INTO learning_recall_card_versions(card_id,revision,format,prompt_json,answer_json,source_version_ids_json,change_reason,created_at) VALUES(?,1,?,?,?,?,?,?)")
                .bind(&card_id)
                .bind(format_name(format))
                .bind(json(&content.prompt)?)
                .bind(json(&content)?)
                .bind(json(&source_ids)?)
                .bind("Imported from the existing Learning Studio Study deck")
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            self.replay_bootstrap_history(
                &mut tx,
                &card_id,
                LearningRecallSchedulerVersion::ExpandingV1,
            )
            .await?;
        }
        tx.commit().await.map_err(db)
    }

    async fn replay_bootstrap_history(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        card_id: &str,
        scheduler: LearningRecallSchedulerVersion,
    ) -> Result<()> {
        let reviews = sqlx::query("SELECT id,reviewed_at,rating FROM study_reviews WHERE card_id=? ORDER BY reviewed_at,id")
            .bind(card_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;
        let mut state = SchedulerState {
            version: scheduler,
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: 0,
            interval_days: 0,
            review_count: 0,
        };
        for review in reviews {
            let review_id: String = review.get("id");
            let reviewed_at: i64 = review.get("reviewed_at");
            let rating = parse_rating(review.get("rating"))?;
            let prior = state.clone();
            state = next_scheduler_state(&state, rating, reviewed_at)?;
            state.review_count += 1;
            let transition_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT OR IGNORE INTO learning_recall_scheduler_transitions(id,card_id,review_id,scheduler_version,prior_state_json,rating,next_state_json,due_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(transition_id)
                .bind(card_id)
                .bind(&review_id)
                .bind(scheduler_name(scheduler))
                .bind(json(&prior)?)
                .bind(rating_name(rating))
                .bind(json(&state)?)
                .bind(state.due_at)
                .bind(reviewed_at)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
        }
        if state.review_count > 0 {
            sqlx::query("UPDATE learning_recall_card_profiles SET scheduler_version=?,scheduler_state_json=? WHERE card_id=?")
                .bind(scheduler_name(scheduler))
                .bind(json(&state)?)
                .bind(card_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }

    async fn card_source_versions(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        card_id: &str,
    ) -> Result<Vec<String>> {
        let origin: Option<String> = sqlx::query_scalar(
            "SELECT source_ids_json FROM learning_card_origins WHERE program_id=? AND card_id=?",
        )
        .bind(program_id)
        .bind(card_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;
        let ids: Vec<String> = if let Some(value) = origin {
            serde_json::from_str(&value).map_err(|error| {
                AppError::InvalidState(format!("Invalid card source links: {error}"))
            })?
        } else {
            vec![]
        };
        let mut result = Vec::new();
        for id in ids {
            let version_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(program_id)
            .bind(&id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
            if version_exists.is_some() {
                result.push(id);
            }
        }
        Ok(result)
    }

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
        if let Some(result) = recall_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "save_card",
            &hash,
        )
        .await?
        {
            if result.as_deref() != Some(req.card_id.as_str()) {
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
        let prior: Option<sqlx::sqlite::SqliteRow> = sqlx::query("SELECT content_revision,scheduler_version,scheduler_state_json FROM learning_recall_card_profiles WHERE card_id=? AND program_id=?")
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
        let state = if let Some(row) = prior {
            SchedulerState::from_json(row.get("scheduler_state_json"))?
        } else {
            SchedulerState {
                version: LearningRecallSchedulerVersion::ExpandingV1,
                stability: None,
                difficulty: None,
                last_reviewed_at: None,
                due_at: now(),
                interval_days: 0,
                review_count: 0,
            }
        };
        let sources = self
            .canonical_sources(&mut tx, &req.program_id, &req.source_version_ids)
            .await?;
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
            sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,format,scheduler_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,'expanding_v1')")
                .bind(&req.card_id)
                .bind(&deck_id)
                .bind(req.content.prompt.trim())
                .bind(answer)
                .bind(json(&req.content.options)?)
                .bind(req.content.correct_option_index.unwrap_or(0) as i64)
                .bind(req.content.explanation.trim())
                .bind(source_json)
                .bind("Learning Recall")
                .bind(state.due_at)
                .bind(study_format)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,scheduler_version,scheduler_state_json,content_revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&req.card_id)
                .bind(&req.program_id)
                .bind(format_name(req.format))
                .bind(json(&req.content.prompt)?)
                .bind(json(&req.content)?)
                .bind(json(&req.source_version_ids)?)
                .bind(scheduler_name(state.version))
                .bind(json(&state)?)
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
        recall_record(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            Some(&req.card_id),
            "save_card",
            &hash,
            &req.card_id,
            timestamp,
        )
        .await?;
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
        if recall_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "decide_duplicate",
            &hash,
        )
        .await?
        .is_some()
        {
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
        recall_record(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            None,
            "decide_duplicate",
            &hash,
            &req.suggestion_id,
            now(),
        )
        .await?;
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
        self.bootstrap_legacy_cards(&req.program_id).await?;
        let hash = payload_hash(&(
            &req.program_id,
            &req.card_id,
            req.rating,
            req.selected_option,
            req.expected_review_count,
        ))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        if recall_replay(&mut tx, &req.review_id, &req.program_id, "review", &hash)
            .await?
            .is_some()
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let row = sqlx::query("SELECT c.*,p.scheduler_state_json,p.scheduler_version FROM study_cards c JOIN learning_recall_card_profiles p ON p.card_id=c.id WHERE c.id=? AND p.program_id=?")
            .bind(&req.card_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Recall card not found in this program.".into()))?;
        let format = parse_format(row.get("format"))?;
        let options: Vec<String> = serde_json::from_str(row.get("options_json"))
            .map_err(|error| AppError::InvalidState(format!("Invalid recall options: {error}")))?;
        let correct_index: i64 = row.get("correct_index");
        if (format == LearningRecallCardFormat::MultipleChoice) != req.selected_option.is_some() {
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
        let correct = req
            .selected_option
            .map(|index| index as i64 == correct_index);
        let rating = match correct {
            Some(true) => StudyRating::Good,
            Some(false) => StudyRating::Again,
            None => req.rating,
        };
        let state = SchedulerState::from_json(row.get("scheduler_state_json"))?;
        if state.review_count != req.expected_review_count {
            return Err(invalid("Recall card changed; reload and retry."));
        }
        let timestamp = now();
        let prior = state.clone();
        let mut next = next_scheduler_state(&state, rating, timestamp)?;
        next.review_count = state.review_count + 1;
        let rating_text = rating_name(rating);
        let correct_text = correct.map(i64::from);
        let mode = if correct.is_some() {
            "quiz"
        } else {
            "flashcard"
        };
        let existing = sqlx::query("SELECT card_id,rating,mode,correct,selected_option,scheduler_version FROM study_reviews WHERE id=?")
            .bind(&req.review_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if let Some(existing) = existing {
            let matches = existing.get::<String, _>("card_id") == req.card_id
                && existing.get::<String, _>("rating") == rating_text
                && existing.get::<String, _>("mode") == mode
                && existing.get::<Option<i64>, _>("correct") == correct_text
                && existing.get::<Option<i64>, _>("selected_option")
                    == req.selected_option.map(|x| x as i64)
                && existing.get::<String, _>("scheduler_version") == scheduler_name(state.version);
            if !matches {
                return Err(invalid(
                    "Review ID was already used with different review data.",
                ));
            }
            recall_record(
                &mut tx,
                &req.review_id,
                &req.program_id,
                Some(&req.card_id),
                "review",
                &hash,
                &req.review_id,
                timestamp,
            )
            .await?;
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        sqlx::query("INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option,scheduler_version) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&req.review_id)
            .bind(&req.card_id)
            .bind(timestamp)
            .bind(rating_text)
            .bind(mode)
            .bind(correct_text)
            .bind(req.selected_option.map(|x| x as i64))
            .bind(scheduler_name(state.version))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("INSERT INTO learning_recall_scheduler_transitions(id,card_id,review_id,scheduler_version,prior_state_json,rating,next_state_json,due_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&req.card_id)
            .bind(&req.review_id)
            .bind(scheduler_name(state.version))
            .bind(json(&prior)?)
            .bind(rating_text)
            .bind(json(&next)?)
            .bind(next.due_at)
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE learning_recall_card_profiles SET scheduler_state_json=?,updated_at=? WHERE card_id=? AND program_id=?")
            .bind(json(&next)?)
            .bind(timestamp)
            .bind(&req.card_id)
            .bind(&req.program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE study_cards SET scheduler_version=?,due_at=?,interval_days=?,review_count=review_count+1,lapses=lapses+? WHERE id=?")
            .bind(scheduler_name(state.version))
            .bind(next.due_at)
            .bind(next.interval_days)
            .bind(i64::from(rating == StudyRating::Again))
            .bind(&req.card_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        recall_record(
            &mut tx,
            &req.review_id,
            &req.program_id,
            Some(&req.card_id),
            "review",
            &hash,
            &req.review_id,
            timestamp,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn change_scheduler(
        &self,
        req: &ChangeLearningRecallSchedulerRequestDto,
    ) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.card_id, "card")?;
        if req.expected_review_count < 0 {
            return Err(invalid("Expected review count cannot be negative."));
        }
        let hash = payload_hash(&(
            &req.program_id,
            &req.card_id,
            req.scheduler_version,
            req.expected_review_count,
        ))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        if recall_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "change_scheduler",
            &hash,
        )
        .await?
        .is_some()
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let row = sqlx::query("SELECT scheduler_state_json FROM learning_recall_card_profiles WHERE program_id=? AND card_id=?")
            .bind(&req.program_id)
            .bind(&req.card_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Recall card not found in this program.".into()))?;
        let previous = SchedulerState::from_json(row.get("scheduler_state_json"))?;
        if previous.review_count != req.expected_review_count {
            return Err(invalid("Recall card changed; reload and retry."));
        }
        if previous.version == req.scheduler_version {
            return Err(invalid("Card already uses this scheduler."));
        }
        let migration_id = uuid::Uuid::new_v4().to_string();
        let mut state = SchedulerState {
            version: req.scheduler_version,
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: 0,
            interval_days: 0,
            review_count: 0,
        };
        let reviews = sqlx::query("SELECT id,reviewed_at,rating FROM study_reviews WHERE card_id=? ORDER BY reviewed_at,id")
            .bind(&req.card_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        for review in reviews {
            let review_id: String = review.get("id");
            let timestamp: i64 = review.get("reviewed_at");
            let rating = parse_rating(review.get("rating"))?;
            let prior = state.clone();
            state = next_scheduler_state(&state, rating, timestamp)?;
            state.review_count += 1;
            sqlx::query("INSERT OR IGNORE INTO learning_recall_scheduler_transitions(id,card_id,review_id,scheduler_version,prior_state_json,rating,next_state_json,due_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(&req.card_id)
                .bind(&review_id)
                .bind(scheduler_name(req.scheduler_version))
                .bind(json(&prior)?)
                .bind(rating_name(rating))
                .bind(json(&state)?)
                .bind(state.due_at)
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        if state.review_count != req.expected_review_count {
            return Err(AppError::InvalidState(
                "Study review history does not match the card's review count.".into(),
            ));
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_recall_scheduler_migrations(id,operation_id,program_id,card_id,from_version,to_version,prior_state_json,next_state_json,replayed_review_count,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(migration_id)
            .bind(&req.operation_id)
            .bind(&req.program_id)
            .bind(&req.card_id)
            .bind(scheduler_name(previous.version))
            .bind(scheduler_name(req.scheduler_version))
            .bind(json(&previous)?)
            .bind(json(&state)?)
            .bind(state.review_count)
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE learning_recall_card_profiles SET scheduler_version=?,scheduler_state_json=?,updated_at=? WHERE program_id=? AND card_id=?")
            .bind(scheduler_name(req.scheduler_version))
            .bind(json(&state)?)
            .bind(timestamp)
            .bind(&req.program_id)
            .bind(&req.card_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "UPDATE study_cards SET scheduler_version=?,due_at=?,interval_days=? WHERE id=?",
        )
        .bind(scheduler_name(req.scheduler_version))
        .bind(state.due_at)
        .bind(state.interval_days)
        .bind(&req.card_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        recall_record(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            Some(&req.card_id),
            "change_scheduler",
            &hash,
            &req.card_id,
            timestamp,
        )
        .await?;
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

    async fn card_versions(&self, card_id: &str) -> Result<Vec<LearningRecallCardVersionDto>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_recall_card_versions WHERE card_id=? ORDER BY revision",
        )
        .bind(card_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|row| {
                let format = parse_format(row.get("format"))?;
                let content: LearningRecallContentDto =
                    serde_json::from_str(row.get("answer_json")).map_err(|error| {
                        AppError::InvalidState(format!("Invalid saved recall content: {error}"))
                    })?;
                Ok(LearningRecallCardVersionDto {
                    revision: row.get("revision"),
                    format,
                    content,
                    source_version_ids: serde_json::from_str(row.get("source_version_ids_json"))
                        .map_err(|error| {
                            AppError::InvalidState(format!(
                                "Invalid recall version sources: {error}"
                            ))
                        })?,
                    change_reason: row.get("change_reason"),
                    created_at: row.get("created_at"),
                })
            })
            .collect()
    }

    async fn duplicate_suggestions(
        &self,
        program_id: &str,
    ) -> Result<Vec<LearningRecallDuplicateSuggestionDto>> {
        let rows = sqlx::query("SELECT * FROM learning_recall_duplicate_suggestions WHERE program_id=? ORDER BY created_at,id LIMIT 500")
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|row| {
                Ok(LearningRecallDuplicateSuggestionDto {
                    id: row.get("id"),
                    card_id: row.get("card_id"),
                    possible_duplicate_card_id: row.get("possible_duplicate_card_id"),
                    reason: row.get("reason"),
                    similarity: row.get("similarity"),
                    status: match row.get::<String, _>("status").as_str() {
                        "pending" => LearningRecallDuplicateStatus::Pending,
                        "confirmed" => LearningRecallDuplicateStatus::Confirmed,
                        "dismissed" => LearningRecallDuplicateStatus::Dismissed,
                        _ => {
                            return Err(AppError::InvalidState(
                                "Unknown duplicate decision.".into(),
                            ))
                        }
                    },
                    created_at: row.get("created_at"),
                    decided_at: row.get("decided_at"),
                })
            })
            .collect()
    }
}

fn content_from_row(
    row: &sqlx::sqlite::SqliteRow,
    format: LearningRecallCardFormat,
) -> Result<LearningRecallContentDto> {
    let content: LearningRecallContentDto =
        serde_json::from_str(row.get("answer_json")).map_err(|error| {
            AppError::InvalidState(format!("Invalid saved recall content: {error}"))
        })?;
    validate_content(format, &content)?;
    Ok(content)
}

pub(super) fn validate_content(
    format: LearningRecallCardFormat,
    content: &LearningRecallContentDto,
) -> Result<()> {
    if content.prompt.trim().is_empty()
        || content.prompt.chars().count() > 2_000
        || content.answer.trim().is_empty()
        || content.answer.chars().count() > 1_000
        || content.explanation.chars().count() > 3_000
    {
        return Err(invalid(
            "Recall prompt, answer, or explanation is outside its allowed bounds.",
        ));
    }
    let invalid_multiple_choice = !(2..=8).contains(&content.options.len())
        || content
            .options
            .iter()
            .any(|choice| choice.trim().is_empty() || choice.chars().count() > 500)
        || content
            .options
            .iter()
            .map(|choice| choice.trim().to_lowercase())
            .collect::<HashSet<_>>()
            .len()
            != content.options.len()
        || content
            .correct_option_index
            .is_none_or(|index| index >= content.options.len())
        || content
            .correct_option_index
            .and_then(|index| content.options.get(index))
            .is_none_or(|choice| choice.trim() != content.answer.trim());
    match format {
        LearningRecallCardFormat::MultipleChoice if invalid_multiple_choice => {
            return Err(invalid(
                "Multiple-choice recall needs 2–8 unique options and a valid correct option.",
            ));
        }
        LearningRecallCardFormat::MultipleChoice => {}
        _ if !content.options.is_empty() || content.correct_option_index.is_some() => {
            return Err(invalid(
                "Only multiple-choice recall cards can contain answer options.",
            ));
        }
        _ => {}
    }
    if format == LearningRecallCardFormat::Cloze {
        if content.cloze_deletions.is_empty()
            || content.cloze_deletions.len() > 12
            || content
                .cloze_deletions
                .iter()
                .any(|deletion| deletion.trim().is_empty() || !content.prompt.contains(deletion))
            || content
                .cloze_deletions
                .iter()
                .map(|deletion| deletion.trim().to_lowercase())
                .collect::<HashSet<_>>()
                .len()
                != content.cloze_deletions.len()
        {
            return Err(invalid(
                "Cloze deletions must be unique, non-empty text present in the prompt.",
            ));
        }
    } else if !content.cloze_deletions.is_empty() {
        return Err(invalid("Only cloze cards can contain cloze deletions."));
    }
    if format == LearningRecallCardFormat::CodePrediction
        && content
            .language
            .as_deref()
            .is_none_or(|value| value.trim().is_empty() || value.chars().count() > 48)
    {
        return Err(invalid("Code-prediction cards require a language name."));
    }
    if format != LearningRecallCardFormat::CodePrediction && content.language.is_some() {
        return Err(invalid(
            "Only code-prediction cards can name a programming language.",
        ));
    }
    Ok(())
}

fn next_scheduler_state(
    previous: &SchedulerState,
    rating: StudyRating,
    timestamp: i64,
) -> Result<SchedulerState> {
    let (due_at, interval_days, stability, difficulty) = match previous.version {
        LearningRecallSchedulerVersion::ExpandingV1 => {
            let (due, days) = next_review(rating, previous.interval_days, timestamp);
            (due, days, previous.stability, previous.difficulty)
        }
        LearningRecallSchedulerVersion::Fsrs6V1 => {
            let fsrs = FSRS::default();
            let elapsed_days = previous
                .last_reviewed_at
                .map(|last| timestamp.saturating_sub(last).max(0) / DAY_MS)
                .unwrap_or(0)
                .min(u32::MAX as i64) as u32;
            let next_states = fsrs
                .next_states(previous.fsrs_memory(), 0.9, elapsed_days)
                .map_err(|error| {
                    AppError::InvalidState(format!("FSRS scheduling failed: {error}"))
                })?;
            let next = match rating {
                StudyRating::Again => next_states.again,
                StudyRating::Hard => next_states.hard,
                StudyRating::Good => next_states.good,
                StudyRating::Easy => next_states.easy,
            };
            let days = (f64::from(next.interval).round() as i64).clamp(1, 36_500);
            (
                timestamp.saturating_add(days.saturating_mul(DAY_MS)),
                days,
                Some(f64::from(next.memory.stability)),
                Some(f64::from(next.memory.difficulty)),
            )
        }
    };
    Ok(SchedulerState {
        version: previous.version,
        stability,
        difficulty,
        last_reviewed_at: Some(timestamp),
        due_at,
        interval_days,
        review_count: previous.review_count,
    })
}

fn parse_selector(row: &sqlx::sqlite::SqliteRow) -> Result<SourceSelectorRecord> {
    let status = match row.get::<String, _>("status").as_str() {
        "exact" => super::source_selector::LearningQuoteMatchStatus::Exact,
        "contextual" => super::source_selector::LearningQuoteMatchStatus::ContextDisambiguated,
        "ambiguous" => super::source_selector::LearningQuoteMatchStatus::Ambiguous,
        "not_found" => super::source_selector::LearningQuoteMatchStatus::NotFound,
        _ => {
            return Err(AppError::InvalidState(
                "Invalid stored source selector status.".into(),
            ))
        }
    };
    let exact: String = row.get("exact_text");
    let prefix: String = row.get("prefix_text");
    let suffix: String = row.get("suffix_text");
    let start: Option<i64> = row.get("start_byte");
    let end: Option<i64> = row.get("end_byte");
    Ok(SourceSelectorRecord {
        id: row.get("id"),
        source_id: row.get("source_id"),
        source_version_id: row.get("source_version_id"),
        selector: super::source_selector::LearningTextQuoteSelector {
            exact,
            prefix,
            suffix,
        },
        match_status: status,
        start_byte: start.map(|value| value.max(0) as usize),
        end_byte: end.map(|value| value.max(0) as usize),
        candidate_count: row.get::<i64, _>("candidate_count").max(0) as usize,
        created_at: row.get("created_at"),
    })
}

fn selector_status_name(status: &super::source_selector::LearningQuoteMatchStatus) -> &'static str {
    match status {
        super::source_selector::LearningQuoteMatchStatus::Exact => "exact",
        super::source_selector::LearningQuoteMatchStatus::ContextDisambiguated => "contextual",
        super::source_selector::LearningQuoteMatchStatus::Ambiguous => "ambiguous",
        super::source_selector::LearningQuoteMatchStatus::NotFound => "not_found",
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
async fn lock_active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
    let result =
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=? AND status='active'")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound(
            "Active learning program not found.".into(),
        ));
    }
    Ok(())
}
async fn operation_replay(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    kind: &str,
    hash: &str,
) -> Result<Option<Option<String>>> {
    let row = sqlx::query("SELECT program_id,kind,payload_hash,result_id FROM learning_portability_operations WHERE operation_id=?")
        .bind(operation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<Option<String>, _>("program_id").as_deref() == Some(program_id)
        && row.get::<String, _>("kind") == kind
        && row.get::<String, _>("payload_hash") == hash
    {
        return Ok(Some(row.get("result_id")));
    }
    Err(invalid(
        "Operation ID was reused with different recall data.",
    ))
}
async fn record_operation(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    kind: &str,
    hash: &str,
    result_id: Option<&str>,
    timestamp: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,?,?,?)")
        .bind(operation_id)
        .bind(program_id)
        .bind(kind)
        .bind(hash)
        .bind(result_id)
        .bind(timestamp)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

async fn recall_replay(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    kind: &str,
    hash: &str,
) -> Result<Option<Option<String>>> {
    let row = sqlx::query("SELECT program_id,kind,payload_hash,result_id FROM learning_recall_operations WHERE operation_id=?")
        .bind(operation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<String, _>("program_id") == program_id
        && row.get::<String, _>("kind") == kind
        && row.get::<String, _>("payload_hash") == hash
    {
        return Ok(Some(row.get("result_id")));
    }
    Err(invalid(
        "Operation ID was reused with different recall data.",
    ))
}

async fn recall_record(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    card_id: Option<&str>,
    kind: &str,
    hash: &str,
    result_id: &str,
    timestamp: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_recall_operations(operation_id,program_id,card_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(operation_id)
        .bind(program_id)
        .bind(card_id)
        .bind(kind)
        .bind(hash)
        .bind(result_id)
        .bind(timestamp)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

async fn memory_deck(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<String> {
    super::repository::ensure_memory_resources(tx, program_id)
        .await
        .map(|(_, deck_id)| deck_id)
}
