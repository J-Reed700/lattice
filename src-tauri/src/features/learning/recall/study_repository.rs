//! Canonical storage for decks, cards and review history.
use super::{
    schedule::{answered, CardSchedule},
    study_dto::*,
};
use crate::features::learning::persistence::db;
use crate::shared::error::{AppError, Result};
use serde::Deserialize;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

#[derive(Clone)]
pub struct StudyRepository {
    pool: SqlitePool,
}

fn json(error: serde_json::Error) -> AppError {
    AppError::InvalidState(format!("Invalid stored study data: {error}"))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredSources {
    One(StudySourceDto),
    Many(Vec<StudySourceDto>),
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredVerification {
    enabled: bool,
    #[serde(default)]
    supported_claim_notes: Vec<String>,
}

/// The parts of a chat answer's stored source list a claim card cites.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSource {
    document_id: String,
    chunk_id: String,
    content: String,
    file_name: String,
    file_path: String,
    #[serde(default)]
    excerpt: Option<String>,
    #[serde(default)]
    chunk_excerpts: Option<Vec<StoredChunkExcerpt>>,
    #[serde(default)]
    citation_id: Option<u32>,
}

#[derive(Deserialize)]
struct StoredChunkExcerpt {
    excerpt: String,
}

#[derive(Default, Deserialize)]
struct StoredMessageMetadata {
    #[serde(default)]
    sources: Vec<StoredSource>,
    #[serde(default)]
    verification: StoredVerification,
}

fn normalized(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

fn strip_numeric_citations(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        if chars.get(index) == Some(&'[') {
            let mut end = index + 1;
            while chars.get(end).is_some_and(|ch| ch.is_ascii_digit()) {
                end += 1;
            }
            if end > index + 1 && chars.get(end) == Some(&']') {
                index = end + 1;
                continue;
            }
        }
        if let Some(ch) = chars.get(index) {
            output.push(*ch);
        }
        index += 1;
    }
    output
}

fn citation_numbers(value: &str) -> Vec<u32> {
    let chars: Vec<char> = value.chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars.get(index) != Some(&'[') {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        let mut digits = String::new();
        while let Some(ch) = chars.get(end).filter(|ch| ch.is_ascii_digit()) {
            digits.push(*ch);
            end += 1;
        }
        if chars.get(end) == Some(&']') {
            if let Ok(number) = digits.parse::<u32>() {
                if !result.contains(&number) {
                    result.push(number);
                }
            }
            index = end + 1;
        } else {
            index += 1;
        }
    }
    result
}

fn matching_sentence<'a>(content: &'a str, claim: &str) -> Option<&'a str> {
    let target = normalized(claim);
    content
        .split_inclusive(['.', '!', '?', '\n'])
        .find(|sentence| {
            let candidate = normalized(&strip_numeric_citations(sentence));
            !candidate.is_empty()
                && (candidate == target
                    || candidate.contains(&target)
                    || target.contains(&candidate))
        })
}

fn terms(value: &str) -> std::collections::HashSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|term| {
            term.len() >= 3
                && ![
                    "the", "and", "for", "with", "that", "this", "from", "into", "were", "was",
                    "are", "has", "have",
                ]
                .contains(&term.as_str())
        })
        .collect()
}

fn best_source_index(claim: &str, sources: &[StoredSource]) -> Option<usize> {
    let claim_terms = terms(claim);
    sources
        .iter()
        .enumerate()
        .max_by_key(|(_, source)| {
            let body = source.excerpt.as_deref().unwrap_or(&source.content);
            claim_terms.intersection(&terms(body)).count()
        })
        .map(|(index, _)| index)
}

fn study_source(source: &StoredSource) -> StudySourceDto {
    let excerpt = source
        .excerpt
        .as_deref()
        .or_else(|| {
            source
                .chunk_excerpts
                .as_ref()
                .and_then(|items| items.first())
                .map(|item| item.excerpt.as_str())
        })
        .unwrap_or(&source.content);
    StudySourceDto {
        chunk_id: source.chunk_id.clone(),
        document_id: source.document_id.clone(),
        file_name: source.file_name.clone(),
        file_path: source.file_path.clone(),
        excerpt: excerpt.chars().take(2400).collect(),
        url: None,
    }
}

fn card(row: &SqliteRow) -> Result<StudyCardDto> {
    let stored: StoredSources = serde_json::from_str(row.get("source_json")).map_err(json)?;
    let citations = match stored {
        StoredSources::One(source) => vec![source],
        StoredSources::Many(sources) => sources,
    };
    let source = citations
        .first()
        .cloned()
        .ok_or_else(|| AppError::InvalidState("A study card has no source citation".into()))?;
    Ok(StudyCardDto {
        id: row.get("id"),
        format: parse_card_format(row.get("format"))?,
        deck_id: row.get("deck_id"),
        question: row.get("question"),
        answer: row.get("answer"),
        options: serde_json::from_str(row.get("options_json")).map_err(json)?,
        correct_index: row.get::<i64, _>("correct_index") as usize,
        explanation: row.get("explanation"),
        source,
        citations,
        topic: row.get("topic"),
        due_at: row.get("due_at"),
        interval_days: row.get("interval_days"),
        review_count: row.get("review_count"),
        lapses: row.get("lapses"),
    })
}

pub(super) fn parse_card_format(value: &str) -> Result<StudyCardFormat> {
    match value {
        "multiple_choice" => Ok(StudyCardFormat::MultipleChoice),
        "question_answer" => Ok(StudyCardFormat::QuestionAnswer),
        _ => Err(AppError::InvalidState(
            "A study card has an unsupported format".into(),
        )),
    }
}

fn format_name(format: StudyCardFormat) -> &'static str {
    match format {
        StudyCardFormat::MultipleChoice => "multiple_choice",
        StudyCardFormat::QuestionAnswer => "question_answer",
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

fn schedule(row: &SqliteRow) -> CardSchedule {
    CardSchedule {
        stability: row.get("stability"),
        difficulty: row.get("difficulty"),
        last_reviewed_at: row.get("last_reviewed_at"),
        due_at: row.get("due_at"),
        interval_days: row.get("interval_days"),
        review_count: row.get("review_count"),
        lapses: row.get("lapses"),
    }
}

/// One answer to one card. A quiz answer has already been turned into its
/// rating: right is Good, wrong is Again.
pub(in crate::features::learning) struct CardReview<'a> {
    pub review_id: &'a str,
    pub card_id: &'a str,
    pub rating: StudyRating,
    pub correct: Option<bool>,
    pub selected_option: Option<usize>,
    pub expected_reviews: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::learning) enum ReviewOutcome {
    Recorded,
    /// The same review ID was already recorded with the same answer.
    Replayed,
    /// The card was reviewed since the caller loaded it.
    Stale,
}

/// Record a review and move the card's FSRS schedule, inside the caller's
/// transaction. Every review of every card goes through here. A review ID
/// already recorded with a different answer is refused.
pub(in crate::features::learning) async fn record_review(
    tx: &mut Transaction<'_, Sqlite>,
    review: &CardReview<'_>,
    now: i64,
) -> Result<ReviewOutcome> {
    let mode = if review.correct.is_some() {
        "quiz"
    } else {
        "flashcard"
    };
    let selected_option = review.selected_option.map(|index| index as i64);
    let existing = sqlx::query(
        "SELECT card_id,rating,mode,correct,selected_option FROM study_reviews WHERE id=?",
    )
    .bind(review.review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?;
    if let Some(existing) = existing {
        let matches = existing.get::<String, _>("card_id") == review.card_id
            && existing.get::<String, _>("rating") == rating_name(review.rating)
            && existing.get::<String, _>("mode") == mode
            && existing.get::<Option<i64>, _>("correct") == review.correct.map(i64::from)
            && existing.get::<Option<i64>, _>("selected_option") == selected_option;
        if !matches {
            return Err(AppError::InvalidInput(
                "Review ID was already used with different review data.".into(),
            ));
        }
        return Ok(ReviewOutcome::Replayed);
    }
    let row = sqlx::query("SELECT stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses FROM study_cards WHERE id=?")
        .bind(review.card_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Study card no longer exists".into()))?;
    let current = schedule(&row);
    if current.review_count != review.expected_reviews {
        return Ok(ReviewOutcome::Stale);
    }
    let next = current.review(review.rating, now)?;
    sqlx::query("INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option) VALUES(?,?,?,?,?,?,?)")
        .bind(review.review_id)
        .bind(review.card_id)
        .bind(now)
        .bind(rating_name(review.rating))
        .bind(mode)
        .bind(review.correct)
        .bind(selected_option)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE study_cards SET stability=?,difficulty=?,last_reviewed_at=?,due_at=?,interval_days=?,review_count=?,lapses=? WHERE id=?")
        .bind(next.stability)
        .bind(next.difficulty)
        .bind(next.last_reviewed_at)
        .bind(next.due_at)
        .bind(next.interval_days)
        .bind(next.review_count)
        .bind(next.lapses)
        .bind(review.card_id)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(ReviewOutcome::Recorded)
}

/// Insert a card row. Its schedule starts as a new FSRS card due at `due_at`.
async fn insert_card(
    tx: &mut Transaction<'_, Sqlite>,
    deck_id: &str,
    card: &StudyCardDto,
) -> Result<()> {
    match card.format {
        StudyCardFormat::MultipleChoice
            if card.options.is_empty() || card.correct_index >= card.options.len() =>
        {
            return Err(AppError::InvalidInput(
                "Multiple-choice cards need a valid answer option.".into(),
            ));
        }
        StudyCardFormat::QuestionAnswer if !card.options.is_empty() => {
            return Err(AppError::InvalidInput(
                "Question-answer cards do not have answer choices.".into(),
            ));
        }
        _ => {}
    }
    let citations = if card.citations.is_empty() {
        std::slice::from_ref(&card.source)
    } else {
        &card.citations
    };
    sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,format) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&card.id)
        .bind(deck_id)
        .bind(&card.question)
        .bind(&card.answer)
        .bind(serde_json::to_string(&card.options).map_err(json)?)
        .bind(card.correct_index as i64)
        .bind(&card.explanation)
        .bind(serde_json::to_string(citations).map_err(json)?)
        .bind(&card.topic)
        .bind(card.due_at)
        .bind(format_name(card.format))
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

impl StudyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert the canonical Study deck in a caller-owned transaction. Learning
    /// Studio uses this only when creating its one linked deck per program.
    pub async fn insert_learning_deck_in_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        id: &str,
        title: &str,
        focus: &str,
        study_goal: &str,
        model_name: &str,
        created_at: i64,
    ) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO study_decks(id,title,focus,study_goal,model_name,created_at) VALUES(?,?,?,?,?,?)")
            .bind(id).bind(title).bind(focus).bind(study_goal).bind(model_name).bind(created_at)
            .execute(&mut **tx).await.map_err(db)?;
        Ok(())
    }

    /// Insert a Learning-origin card into its program's deck.
    pub async fn insert_learning_card_in_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        card: &StudyCardDto,
    ) -> Result<()> {
        insert_card(tx, &card.deck_id, card).await
    }

    pub(super) async fn conversation_claims(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<VerifiedConversationClaim>> {
        let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations WHERE id=?")
            .bind(conversation_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        if exists == 0 {
            return Err(AppError::NotFound("Conversation no longer exists".into()));
        }
        let rows = sqlx::query("SELECT content, metadata FROM conversation_messages WHERE conversation_id=? AND role='assistant' AND status='completed' AND metadata IS NOT NULL ORDER BY created_at, id")
            .bind(conversation_id).fetch_all(&self.pool).await.map_err(db)?;
        let mut claims = Vec::new();
        let mut seen_claims = std::collections::HashSet::new();
        for row in rows {
            let content: String = row.get("content");
            let metadata_text: String = row.get("metadata");
            let metadata: StoredMessageMetadata = match serde_json::from_str(&metadata_text) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !metadata.verification.enabled || metadata.sources.is_empty() {
                continue;
            }
            for claim in metadata.verification.supported_claim_notes {
                let claim = claim.trim();
                if claim.is_empty() || !seen_claims.insert(normalized(claim)) {
                    continue;
                }
                let sentence = matching_sentence(&content, claim);
                let mut indices = sentence
                    .map(citation_numbers)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|number| {
                        metadata
                            .sources
                            .iter()
                            .position(|source| source.citation_id == Some(number))
                            .or_else(|| {
                                usize::try_from(number.saturating_sub(1))
                                    .ok()
                                    .filter(|index| *index < metadata.sources.len())
                            })
                    })
                    .collect::<Vec<_>>();
                if indices.is_empty() {
                    if let Some(index) = best_source_index(claim, &metadata.sources) {
                        indices.push(index);
                    }
                }
                let mut seen_sources = std::collections::HashSet::new();
                let citations = indices
                    .into_iter()
                    .filter_map(|index| metadata.sources.get(index))
                    .filter(|source| seen_sources.insert(source.chunk_id.clone()))
                    .map(study_source)
                    .collect::<Vec<_>>();
                if !citations.is_empty() {
                    claims.push(VerifiedConversationClaim {
                        answer: claim.to_owned(),
                        citations,
                    });
                }
            }
        }
        if claims.is_empty() {
            return Err(AppError::InvalidInput(
                "This conversation has no verified claims with citations yet.".into(),
            ));
        }
        Ok(claims)
    }

    pub async fn save(&self, deck: &StudyDeckDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO study_decks (id,title,focus,study_goal,model_name,created_at) VALUES (?,?,?,?,?,?)",
        )
        .bind(&deck.id)
        .bind(&deck.title)
        .bind(&deck.focus)
        .bind(&deck.study_goal)
        .bind(&deck.model_name)
        .bind(deck.created_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for card in &deck.cards {
            insert_card(&mut tx, &deck.id, card).await?;
        }
        tx.commit().await.map_err(db)
    }

    pub async fn list(&self, now: i64) -> Result<Vec<StudyDeckSummaryDto>> {
        let rows = sqlx::query("SELECT d.*, (SELECT count(*) FROM study_cards c WHERE c.deck_id=d.id) card_count, (SELECT count(*) FROM study_cards c WHERE c.deck_id=d.id AND c.due_at<=?) due_count, (SELECT count(*) FROM study_reviews r JOIN study_cards c ON r.card_id=c.id WHERE c.deck_id=d.id AND r.mode='quiz') quiz_attempts, (SELECT count(*) FROM study_reviews r JOIN study_cards c ON r.card_id=c.id WHERE c.deck_id=d.id AND r.mode='quiz' AND r.correct=1) quiz_correct FROM study_decks d ORDER BY d.created_at DESC, d.id")
            .bind(now).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| StudyDeckSummaryDto {
                id: r.get("id"),
                title: r.get("title"),
                focus: r.get("focus"),
                study_goal: r.get("study_goal"),
                created_at: r.get("created_at"),
                card_count: r.get("card_count"),
                due_count: r.get("due_count"),
                quiz_attempts: r.get("quiz_attempts"),
                quiz_correct: r.get("quiz_correct"),
            })
            .collect())
    }

    pub async fn get(&self, id: &str) -> Result<StudyDeckDto> {
        let row = sqlx::query("SELECT * FROM study_decks WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Study deck no longer exists".into()))?;
        let rows = sqlx::query("SELECT * FROM study_cards WHERE deck_id=? ORDER BY rowid")
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(StudyDeckDto {
            id: row.get("id"),
            title: row.get("title"),
            focus: row.get("focus"),
            study_goal: row.get("study_goal"),
            model_name: row.get("model_name"),
            created_at: row.get("created_at"),
            cards: rows.iter().map(card).collect::<Result<_>>()?,
        })
    }

    pub async fn review(
        &self,
        request: &ReviewStudyCardRequestDto,
        now: i64,
    ) -> Result<StudyCardDto> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query("SELECT * FROM study_cards WHERE id=?")
            .bind(&request.card_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Study card no longer exists".into()))?;
        let current = card(&row)?;
        if current.format == StudyCardFormat::QuestionAnswer && request.selected_option.is_some() {
            return Err(AppError::InvalidInput(
                "Question-answer recall cards do not accept quiz choices.".into(),
            ));
        }
        if request
            .selected_option
            .is_some_and(|index| index >= current.options.len())
        {
            return Err(AppError::InvalidInput(
                "Choose one of this question's answers".into(),
            ));
        }
        let (correct, rating) = answered(
            request.selected_option,
            current.correct_index,
            request.rating,
        );
        let outcome = record_review(
            &mut tx,
            &CardReview {
                review_id: &request.review_id,
                card_id: &current.id,
                rating,
                correct,
                selected_option: request.selected_option,
                expected_reviews: request.expected_reviews,
            },
            now,
        )
        .await?;
        if outcome == ReviewOutcome::Stale {
            return Err(AppError::InvalidState(
                "This card was already reviewed. Reopen the deck to refresh it.".into(),
            ));
        }
        let row = sqlx::query("SELECT * FROM study_cards WHERE id=?")
            .bind(&current.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let result = card(&row)?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }

    pub async fn update_card(&self, request: &UpdateStudyCardRequestDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query("SELECT * FROM study_cards WHERE id=?")
            .bind(&request.card_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Study card no longer exists".into()))?;
        let mut current = card(&row)?;
        if current.format == StudyCardFormat::QuestionAnswer {
            sqlx::query("UPDATE study_cards SET question=?,answer=?,explanation=? WHERE id=?")
                .bind(request.question.trim())
                .bind(request.answer.trim())
                .bind(request.explanation.trim())
                .bind(&request.card_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        if current.options.iter().enumerate().any(|(i, text)| {
            i != current.correct_index && text.trim().eq_ignore_ascii_case(request.answer.trim())
        }) {
            return Err(AppError::InvalidInput(
                "The answer must differ from the other choices".into(),
            ));
        }
        let correct_option = current
            .options
            .get_mut(current.correct_index)
            .ok_or_else(|| {
                AppError::InvalidState("A study card has an invalid correct option".into())
            })?;
        *correct_option = request.answer.trim().to_owned();
        sqlx::query(
            "UPDATE study_cards SET question=?,answer=?,explanation=?,options_json=? WHERE id=?",
        )
        .bind(request.question.trim())
        .bind(request.answer.trim())
        .bind(request.explanation.trim())
        .bind(serde_json::to_string(&current.options).map_err(json)?)
        .bind(&request.card_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn delete_deck(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM study_decks WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}
