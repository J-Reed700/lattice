//! Canonical storage for decks, cards and review history.
use super::{dto::*, schedule::next_review};
use crate::features::qa::dto::SourceDto;
use crate::shared::error::{AppError, Result};
use serde::Deserialize;
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Clone)]
pub struct StudyRepository {
    pool: SqlitePool,
}

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
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

#[derive(Default, Deserialize)]
struct StoredMessageMetadata {
    #[serde(default)]
    sources: Vec<SourceDto>,
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

fn best_source_index(claim: &str, sources: &[SourceDto]) -> Option<usize> {
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

fn study_source(source: &SourceDto) -> StudySourceDto {
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

impl StudyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn sources(&self, ids: &[String], focus: &str) -> Result<Vec<StudySourceDto>> {
        let terms: Vec<_> = focus
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| {
                word.len() >= 4 && !["with", "from", "this", "that", "about"].contains(word)
            })
            .take(8)
            .map(str::to_lowercase)
            .collect();
        let mut sources = Vec::new();
        let per_document = (24 / ids.len().max(1)).max(4) as i64;
        for id in ids {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM text_chunks WHERE document_id = ?")
                    .bind(id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db)?;
            if count == 0 {
                return Err(AppError::InvalidInput(
                    "A selected document has no indexed text yet. Let its import finish first."
                        .into(),
                ));
            }
            let mut query = QueryBuilder::<Sqlite>::new("SELECT c.id, c.document_id, d.file_name, d.file_path, c.content FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ");
            query.push_bind(id);
            if terms.is_empty() {
                query
                    .push(" AND c.chunk_index % ")
                    .push_bind((count / per_document).max(1))
                    .push(" = 0");
            } else {
                query.push(" AND (");
                let mut separated = query.separated(" OR ");
                for term in &terms {
                    separated
                        .push("instr(lower(c.content), ")
                        .push_bind_unseparated(term)
                        .push_unseparated(") > 0");
                }
                query.push(")");
            }
            query
                .push(" ORDER BY c.chunk_index LIMIT ")
                .push_bind(per_document);
            for row in query.build().fetch_all(&self.pool).await.map_err(db)? {
                let content: String = row.get("content");
                sources.push(StudySourceDto {
                    chunk_id: row.get("id"),
                    document_id: row.get("document_id"),
                    file_name: row.get("file_name"),
                    file_path: row.get("file_path"),
                    excerpt: content.chars().take(2400).collect(),
                });
            }
        }
        if sources.is_empty() {
            return Err(AppError::InvalidInput(
                "No passages matched that focus in the selected documents. Try a broader topic."
                    .into(),
            ));
        }
        Ok(sources)
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
        for c in &deck.cards {
            sqlx::query("INSERT INTO study_cards (id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(&c.id).bind(&deck.id).bind(&c.question).bind(&c.answer).bind(serde_json::to_string(&c.options).map_err(json)?)
                .bind(c.correct_index as i64).bind(&c.explanation).bind(serde_json::to_string(if c.citations.is_empty() { std::slice::from_ref(&c.source) } else { &c.citations }).map_err(json)?).bind(&c.topic).bind(c.due_at)
                .execute(&mut *tx).await.map_err(db)?;
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
        let existing: Option<String> =
            sqlx::query_scalar("SELECT card_id FROM study_reviews WHERE id=?")
                .bind(&request.review_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if let Some(id) = existing {
            if id != current.id {
                return Err(AppError::InvalidInput(
                    "Review ID already belongs to another card".into(),
                ));
            }
            return Ok(current);
        }
        if current.review_count != request.expected_reviews {
            return Err(AppError::InvalidState(
                "This card was already reviewed. Reopen the deck to refresh it.".into(),
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
        let correct = request
            .selected_option
            .map(|index| index == current.correct_index);
        let rating = match correct {
            Some(true) => StudyRating::Good,
            Some(false) => StudyRating::Again,
            None => request.rating,
        };
        let (due_at, interval) = next_review(rating, current.interval_days, now);
        let rating_name = match rating {
            StudyRating::Again => "again",
            StudyRating::Hard => "hard",
            StudyRating::Good => "good",
            StudyRating::Easy => "easy",
        };
        sqlx::query("INSERT INTO study_reviews (id,card_id,reviewed_at,rating,mode,correct,selected_option) VALUES (?,?,?,?,?,?,?)")
            .bind(&request.review_id).bind(&current.id).bind(now).bind(rating_name).bind(if correct.is_some() { "quiz" } else { "flashcard" })
            .bind(correct).bind(request.selected_option.map(|i| i as i64)).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE study_cards SET due_at=?, interval_days=?, review_count=review_count+1, lapses=lapses+? WHERE id=?")
            .bind(due_at).bind(interval).bind(i64::from(matches!(rating, StudyRating::Again))).bind(&current.id).execute(&mut *tx).await.map_err(db)?;
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
