//! The synthetic course: read from its JSON file, put into the database through
//! the app's own repositories, and read back after each turn.

use crate::Res;
use lattice::features::learning::dto::{
    AcceptLearningProgramRequestDto, LearningLessonDto, LearningModuleDto, LearningPreparation,
    LearningProgramDto, LearningProgramStatus, LearningProgramSummaryDto, LearningSourceKind,
    LearningSourcePolicy,
};
use lattice::features::learning::generation::GeneratedLearningCardDraft;
use lattice::features::learning::recall_repository::LearningRecallRepository;
use lattice::features::learning::references::trust::capture::not_extracted;
use lattice::features::learning::repository::LearningRepository;
use lattice::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
    PASTED_TEXT_EXTRACTION,
};
use lattice::infrastructure::persistence::repositories::workspace_notes::DailyNotesRepository;
use lattice_core::shared::persistence::Database;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize)]
pub struct Course {
    pub title: String,
    pub goal: String,
    pub prior_knowledge: String,
    pub sources: Vec<Source>,
    modules: Vec<Module>,
    cards: Vec<Card>,
}

#[derive(Deserialize)]
pub struct Source {
    key: String,
    pub title: String,
    pub text: String,
}

#[derive(Deserialize)]
struct Module {
    title: String,
    summary: String,
    outcomes: Vec<String>,
    lessons: Vec<Lesson>,
}

#[derive(Deserialize)]
struct Lesson {
    key: String,
    title: String,
    objective: String,
    completed: bool,
    blocks: Vec<Block>,
}

#[derive(Deserialize)]
struct Block {
    title: String,
    body: String,
    sources: Vec<String>,
}

#[derive(Deserialize)]
struct Card {
    key: String,
    lesson: String,
    question: String,
    answer: String,
    explanation: String,
    source: String,
}

impl Course {
    pub fn load(path: &Path) -> Res<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
}

/// One fresh copy of the course in the database.
pub struct Seeded {
    pub program_id: String,
    /// Lesson ids by the course file's keys.
    pub lessons: HashMap<String, String>,
    /// Card ids to the course file's keys.
    card_keys: HashMap<String, String>,
    /// Source version ids to the course file's keys.
    source_keys: HashMap<String, String>,
}

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn program_of(course: &Course) -> (LearningProgramDto, HashMap<String, String>) {
    let mut lesson_ids = HashMap::new();
    let modules: Vec<LearningModuleDto> = course
        .modules
        .iter()
        .map(|module| LearningModuleDto {
            prerequisite_module_ids: vec![],
            project: None,
            id: id(),
            title: module.title.clone(),
            summary: module.summary.clone(),
            outcomes: module.outcomes.clone(),
            lessons: module
                .lessons
                .iter()
                .map(|lesson| {
                    let lesson_id = id();
                    lesson_ids.insert(lesson.key.clone(), lesson_id.clone());
                    LearningLessonDto {
                        id: lesson_id,
                        title: lesson.title.clone(),
                        objective: lesson.objective.clone(),
                        estimated_minutes: 20,
                        preparation: LearningPreparation::Outline,
                        blocks: vec![],
                        completed: lesson.completed,
                    }
                })
                .collect(),
        })
        .collect();
    let lessons = || modules.iter().flat_map(|module| &module.lessons);
    let program = LearningProgramDto {
        outline_review: None,
        summary: LearningProgramSummaryDto {
            id: id(),
            title: course.title.clone(),
            goal: course.goal.clone(),
            status: LearningProgramStatus::Draft,
            revision: 0,
            module_count: modules.len() as i64,
            lesson_count: lessons().count() as i64,
            completed_lessons: lessons().filter(|lesson| lesson.completed).count() as i64,
            current_lesson_id: lessons()
                .find(|lesson| !lesson.completed)
                .map(|lesson| lesson.id.clone()),
            created_at: chrono::Utc::now().timestamp_millis(),
        },
        prior_knowledge: course.prior_knowledge.clone(),
        minutes_per_session: 30,
        model_name: "synthetic".into(),
        modules,
        sources: vec![],
    };
    (program, lesson_ids)
}

async fn add_source(db: &Database, program_id: &str, source: &Source) -> Res<String> {
    let version = id();
    LearningSourceLibraryRepository::new(db.clone())
        .add(
            NewLearningSource {
                operation_id: &id(),
                source_id: &id(),
                version_id: &version,
                program_id,
                kind: LearningSourceKind::Pasted,
                operation_kind: "add_text",
                origin: "My text",
                requested_url: None,
                freshness: LearningSourcePolicy::Fixed,
            },
            CapturedLearningSource {
                title: source.title.clone(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: source.text.clone(),
                truncated: false,
                extraction_version: PASTED_TEXT_EXTRACTION.into(),
                signals: not_extracted("synthetic course"),
            },
            format!("source-{version}"),
        )
        .await?;
    Ok(version)
}

/// A lesson as lesson preparation leaves it: ready, its blocks citing their
/// sources, so its passages are labelled module › lesson › source.
async fn prepare_lesson(
    db: &Database,
    lesson_id: &str,
    lesson: &Lesson,
    versions: &HashMap<String, String>,
) -> Res<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;
    for (ordinal, block) in lesson.blocks.iter().enumerate() {
        let cited: Vec<&String> = block
            .sources
            .iter()
            .filter_map(|key| versions.get(key))
            .collect();
        sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,?,'explanation',?,?,?)")
            .bind(lesson_id)
            .bind(ordinal as i64)
            .bind(&block.title)
            .bind(&block.body)
            .bind(json!(cited).to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// A card as a learner accepts it from a lesson: drafted, then accepted, so it
/// joins the deck with its lesson as its origin.
async fn add_card(
    db: &Database,
    program_id: &str,
    lesson_id: &str,
    card: &Card,
    version: &str,
) -> Res<String> {
    let repo = LearningRepository::new(db.clone());
    repo.save_generated_drafts(
        program_id,
        lesson_id,
        &[GeneratedLearningCardDraft {
            question: card.question.clone(),
            answer: card.answer.clone(),
            explanation: card.explanation.clone(),
            source_ids: vec![version.to_string()],
        }],
    )
    .await?;
    let mut read = db.begin_read().await?;
    let draft: String = sqlx::query_scalar(
        "SELECT id FROM learning_card_drafts WHERE program_id=? AND lesson_id=? AND status='pending'",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_one(&mut *read)
    .await?;
    read.commit().await?;
    Ok(repo.accept_card_draft(program_id, &draft).await?)
}

pub async fn seed(db: &Database, course: &Course) -> Res<Seeded> {
    let (program, lessons) = program_of(course);
    let program_id = program.summary.id.clone();
    let repo = LearningRepository::new(db.clone());
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program_id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let mut versions = HashMap::new();
    let mut source_keys = HashMap::new();
    for source in &course.sources {
        let version = add_source(db, &program_id, source).await?;
        versions.insert(source.key.clone(), version.clone());
        source_keys.insert(version, source.key.clone());
    }
    for lesson in course.modules.iter().flat_map(|module| &module.lessons) {
        let lesson_id = lessons.get(&lesson.key).ok_or("a lesson with no id")?;
        prepare_lesson(db, lesson_id, lesson, &versions).await?;
    }
    let mut card_keys = HashMap::new();
    for card in &course.cards {
        let lesson = lessons.get(&card.lesson).ok_or("a card with no lesson")?;
        let version = versions.get(&card.source).ok_or("a card with no source")?;
        let card_id = add_card(db, &program_id, lesson, card, version).await?;
        card_keys.insert(card_id, card.key.clone());
    }
    Ok(Seeded {
        program_id,
        lessons,
        card_keys,
        source_keys,
    })
}

/// What Recall and the Notebook hold now, read as those tabs read it. Ids the
/// course file knows are shown as its keys, so two runs read alike.
pub async fn read_back(db: &Database, seeded: &Seeded) -> Res<Value> {
    let workspace = LearningRecallRepository::new(db.clone())
        .workspace(&seeded.program_id)
        .await?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut cards = Vec::new();
    let mut read = db.begin_read().await?;
    for card in &workspace.cards {
        let ratings: Vec<String> =
            sqlx::query("SELECT rating FROM study_reviews WHERE card_id=? ORDER BY rowid")
                .bind(&card.id)
                .fetch_all(&mut *read)
                .await?
                .iter()
                .map(|row| row.get("rating"))
                .collect();
        cards.push(json!({
            "key": seeded.card_keys.get(&card.id),
            "question": card.content.prompt,
            "answer": card.content.answer,
            "review_count": card.scheduler.review_count,
            "interval_days": card.scheduler.interval_days,
            "stability": card.scheduler.stability,
            "due_now": card.scheduler.due_at <= now,
            "ratings": ratings,
            "sources": card
                .source_version_ids
                .iter()
                .map(|version| seeded.source_keys.get(version).unwrap_or(version))
                .collect::<Vec<_>>(),
        }));
    }
    read.commit().await?;
    let memory = LearningRepository::new(db.clone())
        .memory_state(&seeded.program_id)
        .await?;
    let notes = DailyNotesRepository::new(db.clone());
    let mut lesson_notes = Vec::new();
    for (lesson_id, note_id) in &memory.lesson_notes {
        let lesson = seeded
            .lessons
            .iter()
            .find_map(|(key, id)| (id == lesson_id).then_some(key));
        lesson_notes.push(json!({
            "lesson": lesson,
            "content": notes.get(note_id).await?.content,
        }));
    }
    Ok(json!({
        "cards_due": workspace.due_count,
        "cards": cards,
        "notes": lesson_notes,
    }))
}
