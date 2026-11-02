use super::binding::{bind_conversation, SetConversationCourseRequestDto};
use super::cards::{interleaved, DueCard};
use super::course_turn::{CourseTurn, NoteWritten};
use super::prompt::{render_context, TurnFacts, TUTOR_POLICY, TUTOR_TEMPLATE};
use crate::features::learning::dto::{
    AcceptLearningProgramRequestDto, LearningProgramDto, LearningSourceKind, LearningSourcePolicy,
};
use crate::features::learning::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, SaveLearningRecallCardRequestDto,
};
use crate::features::learning::recall_repository::LearningRecallRepository;
use crate::features::learning::references::trust::capture::not_extracted;
use crate::features::learning::repository::LearningRepository;
use crate::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
};
use crate::features::learning::tests;
use crate::infrastructure::persistence::repositories::workspace_notes::DailyNotesRepository;
use lattice_core::application::contracts::sources::COURSE_SOURCE_PREFIX;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::turn_source::{
    SourcePassage, SourcePassages, TurnSourcePort,
};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const SOURCE_TEXT: &str = "Reciprocal rank fusion sums one over k plus rank for each list. \
A larger k flattens the weight of the top ranks.\n\nBM25 scores a document by term frequency and rarity.";

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

struct Course {
    pool: SqlitePool,
    program: LearningProgramDto,
    conversation: String,
    version: String,
    notes_written: Arc<AtomicUsize>,
}

impl Course {
    fn lesson(&self, module: usize, lesson: usize) -> String {
        self.program.modules[module].lessons[lesson].id.clone()
    }

    async fn turn(&self) -> CourseTurn {
        let written = Arc::clone(&self.notes_written);
        let note_written: NoteWritten = Arc::new(move || {
            written.fetch_add(1, Ordering::SeqCst);
        });
        CourseTurn::resolve(&self.pool, None, &self.conversation, note_written)
            .await
            .expect("the chat is bound to the course")
    }

    async fn call(&self, tool: &str, arguments: Value) -> FunctionResult {
        self.turn()
            .await
            .execute_tool(FunctionCall::new(id(), tool, arguments), 20_000)
            .await
            .expect("a course tool reports its failures in its result")
    }

    async fn text(&self, tool: &str, arguments: Value) -> String {
        let result = self.call(tool, arguments).await;
        assert!(result.success, "{:?}", result.error_message);
        result
            .data
            .expect("a result")
            .as_str()
            .expect("text")
            .to_string()
    }

    async fn passages(&self, tool: &str, arguments: Value) -> SourcePassages {
        let result = self.call(tool, arguments).await;
        assert!(result.success, "{:?}", result.error_message);
        serde_json::from_value(result.data.expect("a result")).expect("passages")
    }

    async fn save_card(&self, front: &str) -> String {
        let card_id = id();
        LearningRecallRepository::new(self.pool.clone())
            .save_card(&SaveLearningRecallCardRequestDto {
                operation_id: id(),
                program_id: self.program.summary.id.clone(),
                card_id: card_id.clone(),
                expected_content_revision: None,
                format: LearningRecallCardFormat::QuestionAnswer,
                content: LearningRecallContentDto {
                    prompt: front.into(),
                    answer: "The hidden answer".into(),
                    explanation: String::new(),
                    options: vec![],
                    correct_option_index: None,
                    language: None,
                    cloze_deletions: vec![],
                },
                source_version_ids: vec![],
                change_reason: "Test".into(),
            })
            .await
            .expect("the card saves");
        card_id
    }

    async fn schedule(&self, card_id: &str) -> (i64, i64) {
        let row = sqlx::query("SELECT review_count, due_at FROM study_cards WHERE id=?")
            .bind(card_id)
            .fetch_one(&self.pool)
            .await
            .expect("the card exists");
        (row.get("review_count"), row.get("due_at"))
    }
}

async fn add_source(pool: &SqlitePool, program_id: &str, title: &str, text: &str) -> String {
    let version = id();
    LearningSourceLibraryRepository::new(pool.clone())
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
                title: title.into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: text.into(),
                truncated: false,
                extraction_version: "test_v1".into(),
                signals: not_extracted("test fixture"),
            },
            format!("source-{version}"),
        )
        .await
        .expect("the source saves");
    version
}

/// An accepted course with one source, cited by its first lesson, and a chat
/// bound to it.
async fn course() -> Course {
    let pool = tests::pool().await.expect("a database");
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await.expect("the course saves");
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await
    .expect("the course is accepted");
    let version = add_source(&pool, &program.summary.id, "Search notes", SOURCE_TEXT).await;
    let lesson = program.modules[0].lessons[0].id.clone();
    sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,0,'explanation','Fusion','Body',?)")
        .bind(&lesson)
        .bind(json!([version]).to_string())
        .execute(&pool)
        .await
        .expect("the block saves");
    let conversation = id();
    sqlx::query("INSERT INTO conversations (id,title) VALUES (?,'Tutor')")
        .bind(&conversation)
        .execute(&pool)
        .await
        .expect("the chat saves");
    bind_conversation(
        &pool,
        &SetConversationCourseRequestDto {
            conversation_id: conversation.clone(),
            program_id: program.summary.id.clone(),
            lesson_id: None,
        },
    )
    .await
    .expect("the chat binds");
    let program = repo.get(&program.summary.id).await.expect("the course");
    Course {
        pool,
        program,
        conversation,
        version,
        notes_written: Arc::default(),
    }
}

async fn binding(pool: &SqlitePool, conversation: &str) -> (Option<String>, Option<String>) {
    let row = sqlx::query("SELECT program_id, lesson_id FROM conversations WHERE id=?")
        .bind(conversation)
        .fetch_one(pool)
        .await
        .expect("the chat survives");
    (row.get("program_id"), row.get("lesson_id"))
}

#[tokio::test]
async fn deleting_a_course_leaves_its_tutor_chats_as_ordinary_chats() {
    let course = course().await;
    let lesson = course.lesson(0, 0);
    bind_conversation(
        &course.pool,
        &SetConversationCourseRequestDto {
            conversation_id: course.conversation.clone(),
            program_id: course.program.summary.id.clone(),
            lesson_id: Some(lesson.clone()),
        },
    )
    .await
    .expect("the chat binds to the lesson");
    assert_eq!(
        binding(&course.pool, &course.conversation).await,
        (Some(course.program.summary.id.clone()), Some(lesson))
    );

    LearningRepository::new(course.pool.clone())
        .delete(&course.program.summary.id)
        .await
        .expect("the course is deleted");

    assert_eq!(
        binding(&course.pool, &course.conversation).await,
        (None, None)
    );
    let note_written: NoteWritten = Arc::new(|| {});
    assert!(
        CourseTurn::resolve(&course.pool, None, &course.conversation, note_written)
            .await
            .is_none(),
        "its turns run as a plain chat's"
    );
}

#[tokio::test]
async fn a_chat_binds_only_to_an_accepted_course_a_lesson_of_it_and_no_folder() {
    let course = course().await;
    let request = |conversation_id: &str, program_id: &str, lesson_id: Option<String>| {
        SetConversationCourseRequestDto {
            conversation_id: conversation_id.to_string(),
            program_id: program_id.to_string(),
            lesson_id,
        }
    };
    let program = course.program.summary.id.as_str();

    let foreign_lesson = request(&course.conversation, program, Some(id()));
    assert!(bind_conversation(&course.pool, &foreign_lesson)
        .await
        .is_err());

    let gone = request(&course.conversation, &id(), None);
    assert!(bind_conversation(&course.pool, &gone).await.is_err());

    let draft = tests::fixture();
    let repo = LearningRepository::new(course.pool.clone());
    repo.create(&draft).await.expect("the draft saves");
    let unaccepted = request(&course.conversation, &draft.summary.id, None);
    assert!(bind_conversation(&course.pool, &unaccepted).await.is_err());

    let thread = id();
    sqlx::query(
        "INSERT INTO conversations (id,title,explorer_root) VALUES (?,'Folder','/project')",
    )
    .bind(&thread)
    .execute(&course.pool)
    .await
    .expect("the thread saves");
    let folder_thread = request(&thread, program, None);
    assert!(bind_conversation(&course.pool, &folder_thread)
        .await
        .is_err());
    assert_eq!(binding(&course.pool, &thread).await, (None, None));
}

#[tokio::test]
async fn search_course_labels_each_passage_module_lesson_source_as_a_citation() {
    let course = course().await;
    let found = course
        .passages(
            "search_course",
            json!({ "query": "reciprocal rank fusion" }),
        )
        .await;

    let passage = found.passages.first().expect("a passage");
    assert!(
        passage
            .label
            .starts_with("Module 0 › Lesson 0-0 › Search notes"),
        "{}",
        passage.label
    );
    assert!(passage.label.contains(&course.version), "{}", passage.label);
    assert!(passage.source.content.contains("sums one over k plus rank"));
    assert_eq!(
        passage.source.document_id,
        format!("{COURSE_SOURCE_PREFIX}{}", course.version)
    );
    assert!(passage.source.citation_id.is_none(), "the turn numbers it");
    assert!(passage.source.file_path.is_empty(), "it is no file on disk");
    assert_eq!(found.note, None);

    let nothing = course
        .passages("search_course", json!({ "query": "zebra" }))
        .await;
    assert!(nothing.passages.is_empty());
    assert!(nothing.note.expect("a note").contains("does not cover it"));
}

#[tokio::test]
async fn the_passages_of_one_search_have_ids_of_their_own_and_a_repeat_keeps_them() {
    let course = course().await;
    let query = json!({ "query": "rank fusion" });
    let first = course.passages("search_course", query.clone()).await;
    let second = course.passages("search_course", query).await;
    let ids = |found: &SourcePassages| -> Vec<String> {
        found
            .passages
            .iter()
            .map(|passage| passage.source.chunk_id.clone())
            .collect()
    };
    assert_eq!(ids(&first), ids(&second));
}

#[tokio::test]
async fn read_course_source_reads_on_from_where_it_stopped() {
    let course = course().await;
    let turn = course.turn().await;
    let first = turn
        .read_source(&course.version, 0, 620)
        .await
        .expect("it reads")
        .expect("the course has the source");
    let passage = first.passages.first().expect("a stretch");
    assert_eq!(passage.source.content.chars().count(), 20);
    let note = first.note.expect("a note");
    assert!(note.contains("offset=20"), "{note}");

    let rest = turn
        .read_source(&course.version, 20, 20_000)
        .await
        .expect("it reads")
        .expect("the course has the source");
    let passage = rest.passages.first().expect("the rest");
    assert!(passage
        .source
        .content
        .ends_with("term frequency and rarity."));
    assert!(rest
        .note
        .expect("a note")
        .contains("the end of this source"));

    let past = turn
        .read_source(&course.version, 9_000, 20_000)
        .await
        .expect("it reads")
        .expect("the course has the source");
    assert!(past.passages.is_empty());

    let other = course
        .call("read_course_source", json!({ "source_id": id() }))
        .await;
    assert!(!other.success, "a source outside the course is refused");
}

#[tokio::test]
async fn read_course_source_takes_the_id_a_retained_passage_shows() {
    let course = course().await;
    let read = course
        .passages(
            "read_course_source",
            json!({ "source_id": format!("{COURSE_SOURCE_PREFIX}{}", course.version) }),
        )
        .await;
    assert_eq!(read.passages.len(), 1);
}

#[tokio::test]
async fn the_outline_names_what_the_learner_finished_and_the_sources() {
    let course = course().await;
    let lesson = course.lesson(0, 0);
    sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
        .bind(&lesson)
        .execute(&course.pool)
        .await
        .expect("the lesson finishes");

    let outline = course.text("course_outline", json!({})).await;

    assert!(outline.contains("Module 1: Module 0"), "{outline}");
    assert!(outline.contains("1.1 Lesson 0-0 [finished]"), "{outline}");
    assert!(outline.contains("1.2 Lesson 0-1 (lesson_id:"), "{outline}");
    assert!(
        outline.contains(&format!("Search notes (source_id: {})", course.version)),
        "{outline}"
    );
}

#[tokio::test]
async fn due_reviews_shows_fronts_and_never_answers() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;

    let due = course.text("due_reviews", json!({})).await;

    assert!(due.contains(&card), "{due}");
    assert!(due.contains("What does k flatten?"), "{due}");
    assert!(!due.contains("The hidden answer"), "{due}");
}

#[tokio::test]
async fn due_reviews_says_how_many_it_had_no_room_for() {
    let course = course().await;
    for number in 0..4 {
        course.save_card(&format!("Question {number}")).await;
    }
    let result = course
        .turn()
        .await
        .execute_tool(FunctionCall::new(id(), "due_reviews", json!({})), 300)
        .await
        .expect("it runs");
    let due = result.data.expect("a result");
    let due = due.as_str().expect("text");
    assert!(due.chars().count() <= 300, "{}", due.chars().count());
    assert!(due.contains("more did not fit"), "{due}");
}

#[test]
fn due_reviews_come_from_across_modules() {
    let card = |id: &str, group: &str| DueCard {
        id: id.into(),
        front: String::new(),
        options: vec![],
        group: group.into(),
    };
    let mixed: Vec<String> = interleaved(vec![
        card("a1", "module-a"),
        card("a2", "module-a"),
        card("a3", "module-a"),
        card("b1", "module-b"),
        card("c1", "module-c"),
    ])
    .into_iter()
    .map(|card| card.id)
    .collect();
    assert_eq!(mixed, ["a1", "b1", "c1", "a2", "a3"]);
}

#[tokio::test]
async fn a_card_answered_in_the_tutor_is_a_review_in_recall() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let recall = LearningRecallRepository::new(course.pool.clone());
    let program = course.program.summary.id.clone();
    assert_eq!(
        recall.workspace(&program).await.expect("cards").due_count,
        1
    );

    let recorded = course
        .text(
            "record_recall",
            json!({ "card_id": card, "rating": "good" }),
        )
        .await;

    assert!(recorded.contains("Cards still due: 0"), "{recorded}");
    let workspace = recall.workspace(&program).await.expect("cards");
    assert_eq!(workspace.due_count, 0, "the Recall badge goes down");
    let scheduled = &workspace.cards[0].scheduler;
    assert_eq!(scheduled.review_count, 1);
    assert!(scheduled.stability.is_some(), "FSRS has its memory");
    assert!(scheduled.interval_days >= 1);
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM study_reviews WHERE card_id=?")
        .bind(&card)
        .fetch_one(&course.pool)
        .await
        .expect("the review row");
    assert_eq!(reviews, 1);
}

/// A second rating of the same answer, a regenerated reply's or a card the
/// tutor never asked, must not move the schedule twice.
#[tokio::test]
async fn a_card_that_is_not_due_keeps_its_schedule() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let rate = json!({ "card_id": card, "rating": "good" });
    course.text("record_recall", rate.clone()).await;
    let reviewed = course.schedule(&card).await;

    let again = course.call("record_recall", rate).await;

    assert!(!again.success);
    assert!(
        again.error_message.expect("why").contains("not due"),
        "the model is told why"
    );
    assert_eq!(course.schedule(&card).await, reviewed);
}

/// A review is an answer to a question: a rating for a card nobody asked is a
/// guess, and it would move the card as a real answer does.
#[tokio::test]
async fn a_card_the_tutor_never_asked_keeps_its_schedule() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let before = course.schedule(&card).await;
    reply(
        &course.pool,
        &course.conversation,
        1,
        "Fusion sums ranks [1].

What does BM25 score?",
    )
    .await;

    let result = course
        .call(
            "record_recall",
            json!({ "card_id": card, "rating": "good" }),
        )
        .await;

    assert!(!result.success);
    assert!(
        result.error_message.expect("why").contains("as written"),
        "the model is told how to ask it"
    );
    assert_eq!(course.schedule(&card).await, before);
}

/// Replies dress a card up; its words are what count.
#[tokio::test]
async fn a_card_asked_in_a_reply_s_own_emphasis_counts_as_asked() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    reply(
        &course.pool,
        &course.conversation,
        1,
        "Card 1 of 6:

**What does *k*   flatten?**",
    )
    .await;

    course
        .text(
            "record_recall",
            json!({ "card_id": card, "rating": "good" }),
        )
        .await;

    assert_eq!(course.schedule(&card).await.0, 1);
}

#[tokio::test]
async fn record_recall_refuses_a_card_of_another_course() {
    let course = course().await;
    let foreign = id();
    let result = course
        .call(
            "record_recall",
            json!({ "card_id": foreign, "rating": "good" }),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error_message
        .expect("why")
        .contains("not in this course"));
}

#[tokio::test]
async fn record_recall_refuses_a_rating_it_cannot_read() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let before = course.schedule(&card).await;
    let result = course
        .call(
            "record_recall",
            json!({ "card_id": card, "rating": "pretty good" }),
        )
        .await;
    assert!(!result.success);
    assert_eq!(course.schedule(&card).await, before);
}

#[tokio::test]
async fn a_multiple_choice_card_is_rated_by_the_option_the_learner_chose() {
    let course = course().await;
    let card = id();
    LearningRecallRepository::new(course.pool.clone())
        .save_card(&SaveLearningRecallCardRequestDto {
            operation_id: id(),
            program_id: course.program.summary.id.clone(),
            card_id: card.clone(),
            expected_content_revision: None,
            format: LearningRecallCardFormat::MultipleChoice,
            content: LearningRecallContentDto {
                prompt: "Which flattens the top ranks?".into(),
                answer: "A larger k".into(),
                explanation: String::new(),
                options: vec!["A smaller k".into(), "A larger k".into()],
                correct_option_index: Some(1),
                language: None,
                cloze_deletions: vec![],
            },
            source_version_ids: vec![],
            change_reason: "Test".into(),
        })
        .await
        .expect("the card saves");

    let due = course.text("due_reviews", json!({})).await;
    assert!(due.contains("1. A larger k"), "{due}");

    let unsaid = course
        .call(
            "record_recall",
            json!({ "card_id": card, "rating": "good" }),
        )
        .await;
    assert!(!unsaid.success, "the option decides the rating");

    course
        .text(
            "record_recall",
            json!({ "card_id": card, "rating": "easy", "selected_option": 0 }),
        )
        .await;
    let rating: String = sqlx::query_scalar("SELECT rating FROM study_reviews WHERE card_id=?")
        .bind(&card)
        .fetch_one(&course.pool)
        .await
        .expect("the review");
    assert_eq!(rating, "again", "a wrong choice is a miss");
}

#[tokio::test]
async fn save_card_cites_its_source_and_joins_the_recall_deck() {
    let course = course().await;
    let saved = course
        .text(
            "save_card",
            json!({
                "question": "What does a larger k do?",
                "answer": "It flattens the weight of the top ranks.",
                "source_ids": [format!("{COURSE_SOURCE_PREFIX}{}", course.version)],
            }),
        )
        .await;
    let card_id = saved
        .rsplit(' ')
        .next()
        .expect("the card's id")
        .trim_end_matches('.');

    let workspace = LearningRecallRepository::new(course.pool.clone())
        .workspace(&course.program.summary.id)
        .await
        .expect("cards");
    let card = workspace.cards.first().expect("the card");
    assert_eq!(card.id, card_id);
    assert_eq!(
        card.source_version_ids,
        std::slice::from_ref(&course.version)
    );
    assert_eq!(workspace.due_count, 1, "the tutor can ask it at once");
    course
        .text(
            "record_recall",
            json!({ "card_id": card_id, "rating": "hard" }),
        )
        .await;
}

#[tokio::test]
async fn save_card_refuses_a_card_with_no_citation_when_the_course_has_sources() {
    let course = course().await;
    let result = course
        .call(
            "save_card",
            json!({ "question": "Q?", "answer": "A.", "source_ids": [] }),
        )
        .await;
    assert!(!result.success);
    assert!(result.error_message.expect("why").contains("source_id"));

    let outside = course
        .call(
            "save_card",
            json!({ "question": "Q?", "answer": "A.", "source_ids": [id()] }),
        )
        .await;
    assert!(!outside.success, "a source outside the course is refused");
}

#[tokio::test]
async fn save_note_adds_the_summary_to_the_lesson_s_note() {
    let course = course().await;
    let lesson = course.lesson(0, 1);
    let saved = course
        .text(
            "save_note",
            json!({ "summary": "Fusion adds up reciprocal ranks.", "lesson_id": lesson }),
        )
        .await;
    assert!(saved.contains("Lesson 0-1"), "{saved}");
    assert_eq!(course.notes_written.load(Ordering::SeqCst), 1);

    let memory = LearningRepository::new(course.pool.clone())
        .memory_state(&course.program.summary.id)
        .await
        .expect("the notebook");
    let (noted, note_id) = memory.lesson_notes.first().expect("the note");
    assert_eq!(noted, &lesson);
    let notes = DailyNotesRepository::new(course.pool.clone());
    let note = notes.get(note_id).await.expect("the note");
    assert!(note.content.contains("Fusion adds up reciprocal ranks."));

    course
        .text(
            "save_note",
            json!({ "summary": "And k flattens them.", "lesson_id": lesson }),
        )
        .await;
    let note = notes.get(note_id).await.expect("the note");
    assert!(
        note.content.contains("Fusion adds up reciprocal ranks.")
            && note.content.contains("And k flattens them."),
        "a later summary adds to the earlier one: {}",
        note.content
    );
}

#[tokio::test]
async fn save_note_without_a_lesson_uses_the_lesson_the_learner_is_on() {
    let course = course().await;
    course
        .text("save_note", json!({ "summary": "In my own words." }))
        .await;
    let memory = LearningRepository::new(course.pool.clone())
        .memory_state(&course.program.summary.id)
        .await
        .expect("the notebook");
    let (noted, _) = memory.lesson_notes.first().expect("the note");
    assert_eq!(noted, &course.lesson(0, 0));
}

#[tokio::test]
async fn a_turn_s_first_passages_carry_the_course_label_and_fit_their_room() {
    let course = course().await;
    let turn = course.turn().await;
    let found = turn.passages("rank fusion", 20_000).await;
    assert!(!found.is_empty());
    assert!(
        found[0].label.contains("Search notes"),
        "{}",
        found[0].label
    );
    assert!(
        turn.passages("rank fusion", 10).await.is_empty(),
        "a passage that does not fit is left out"
    );
}

/// A lesson only focuses what the tutor reads: the first passages come from the
/// sources it cites, falling back on the whole course, and the tools still
/// reach everything.
#[tokio::test]
async fn a_chat_on_a_lesson_reads_the_sources_it_cites_first() {
    let course = course().await;
    let other = add_source(
        &course.pool,
        &course.program.summary.id,
        "Other notes",
        "Rank fusion also appears in this other source.",
    )
    .await;
    let on_lesson = |lesson: String| SetConversationCourseRequestDto {
        conversation_id: course.conversation.clone(),
        program_id: course.program.summary.id.clone(),
        lesson_id: Some(lesson),
    };
    let versions = |found: &[SourcePassage]| -> Vec<String> {
        found
            .iter()
            .map(|passage| {
                passage
                    .source
                    .document_id
                    .trim_start_matches(COURSE_SOURCE_PREFIX)
                    .to_string()
            })
            .collect()
    };

    bind_conversation(&course.pool, &on_lesson(course.lesson(0, 0)))
        .await
        .expect("the chat binds");
    let found = course.turn().await.passages("rank fusion", 20_000).await;
    assert_eq!(versions(&found), std::slice::from_ref(&course.version));
    let whole_course = course
        .passages("search_course", json!({ "query": "rank fusion" }))
        .await;
    assert!(
        versions(&whole_course.passages).contains(&other),
        "the tool reaches the whole course"
    );

    bind_conversation(&course.pool, &on_lesson(course.lesson(1, 0)))
        .await
        .expect("the chat binds");
    let found = course.turn().await.passages("rank fusion", 20_000).await;
    assert!(
        versions(&found).contains(&other),
        "a lesson that cites nothing reads the whole course"
    );
}

async fn reply(pool: &SqlitePool, conversation: &str, sequence: i64, text: &str) {
    sqlx::query("INSERT INTO conversation_messages (id,conversation_id,role,content,tokens,created_at,status,sequence) VALUES (?,?,'assistant',?,1,'2026-11-01T00:00:00Z','completed',?)")
        .bind(id())
        .bind(conversation)
        .bind(text)
        .bind(sequence)
        .execute(pool)
        .await
        .expect("the reply saves");
}

#[tokio::test]
async fn each_turn_carries_the_cards_due_and_the_replies_since_the_last_check() {
    let course = course().await;
    course.save_card("What does k flatten?").await;
    course.save_card("What does BM25 score?").await;
    reply(
        &course.pool,
        &course.conversation,
        1,
        "Fusion sums ranks [1].\n\nWhat does k do?",
    )
    .await;
    reply(
        &course.pool,
        &course.conversation,
        2,
        "Right: it flattens them [1].",
    )
    .await;
    reply(
        &course.pool,
        &course.conversation,
        3,
        "BM25 scores terms [2].",
    )
    .await;

    let block = course.turn().await.context_block(4_000, true, None).await;

    assert!(
        block.contains("Course: A generic learning program."),
        "{block}"
    );
    assert!(block.contains("Cards due now: 2."), "{block}");
    assert!(
        block.contains("since you last asked the learner a question: 2."),
        "{block}"
    );
    assert!(block.contains("Lessons finished: 0 of 4."), "{block}");
    assert!(
        block.contains("Current lesson: Module 0 › Lesson 0-0."),
        "{block}"
    );
}

/// Tool results are gone by the next turn, so the id a card is rated by has to
/// come with the turn that rates it.
#[tokio::test]
async fn each_turn_shows_the_next_due_cards_by_id_and_never_their_answers() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;

    let block = course.turn().await.context_block(4_000, true, None).await;

    assert!(block.contains(&format!("card_id: {card}")), "{block}");
    assert!(block.contains("What does k flatten?"), "{block}");
    assert!(!block.contains("The hidden answer"), "{block}");
    assert!(
        !course
            .turn()
            .await
            .context_block(4_000, false, None)
            .await
            .contains("card_id"),
        "a turn without tools cannot rate one"
    );
}

#[tokio::test]
async fn a_turn_shows_a_few_due_cards_and_counts_them_all() {
    let course = course().await;
    for number in 0..7 {
        course.save_card(&format!("Question {number}")).await;
    }

    let block = course.turn().await.context_block(4_000, true, None).await;

    assert!(block.contains("Cards due now: 7."), "{block}");
    assert_eq!(block.matches("card_id:").count(), 5, "{block}");
}

#[test]
fn a_chat_opened_from_a_lesson_names_it_and_a_turn_without_tools_says_so() {
    let facts = TurnFacts {
        title: "Search".into(),
        focus: Some(("Ranking › Fusion".into(), "Explain fusion".into())),
        ..TurnFacts::default()
    };
    let block = render_context(&facts, false, 4_000);
    assert!(
        block.contains("opened from the lesson Ranking › Fusion"),
        "{block}"
    );
    assert!(block.contains("Explain fusion"), "{block}");
    assert!(block.contains("cannot call tools"), "{block}");
    assert!(render_context(&facts, true, 4_000).contains("search_course"));
    assert!(render_context(&facts, true, 12).chars().count() <= 12);
}

#[tokio::test]
async fn the_tutor_s_reads_run_together_and_its_writes_run_alone() {
    let course = course().await;
    let turn = course.turn().await;
    for tool in [
        "search_course",
        "read_course_source",
        "course_outline",
        "due_reviews",
    ] {
        assert!(turn.owns_tool(tool) && turn.reads_only(tool), "{tool}");
    }
    for tool in ["record_recall", "save_card", "save_note"] {
        assert!(turn.owns_tool(tool) && !turn.reads_only(tool), "{tool}");
    }
    assert!(!turn.owns_tool("read_file"));

    let mut offered = Vec::new();
    turn.add_tools(&mut offered);
    turn.add_tools(&mut offered);
    assert_eq!(offered.len(), 7, "each tool once");
}

/// A tutor must cite: every claim is checked as a chat's is, in a template
/// that asks for citations and carries the study methods.
#[test]
fn a_tutor_turn_cites_and_is_checked_as_a_chat_turn_is() {
    let policy = TUTOR_POLICY;
    assert!(!policy.verifies_cited_only);
    assert!(!policy.reads_passages);
    assert!(policy.drops_session_talk);
    assert_eq!(policy.evidence_template, Some(TUTOR_TEMPLATE));
    assert!(policy.offers_library_tools, "it keeps a chat's tools");
    assert!(TUTOR_TEMPLATE.contains("{context}") && TUTOR_TEMPLATE.contains("{question}"));
    assert!(TUTOR_TEMPLATE.contains("put the bracket number of the passage"));
    for method in [
        "due_reviews",
        "record_recall",
        "save_card",
        "save_note",
        "mermaid",
    ] {
        assert!(TUTOR_TEMPLATE.contains(method), "{method}");
    }
    assert!(
        TUTOR_TEMPLATE.ends_with("Answer:"),
        "the reading takes the cue off"
    );
}
