use super::binding::{bind_conversation, SetConversationCourseRequestDto};
use super::cards::{interleaved, DueCard};
use super::course_turn::{CourseTurn, NoteWritten};
use super::prompt::{render_context, TurnFacts, TUTOR_POLICY, TUTOR_TEMPLATE};
use crate::dto::{
    AcceptLearningProgramRequestDto, LearningProgramDto, LearningSourceKind, LearningSourcePolicy,
};
use crate::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, SaveLearningRecallCardRequestDto,
};
use crate::recall_repository::LearningRecallRepository;
use crate::references::trust::capture::not_extracted;
use crate::repository::LearningRepository;
use crate::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
};
use crate::tests;
use lattice_core::application::contracts::sources::COURSE_SOURCE_PREFIX;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::turn_source::{
    SourcePassage, SourcePassages, TurnSourcePort,
};
use lattice_core::application::services::model_contract::kept::{
    KeptNames, MemoryNames, TurnNames,
};
use lattice_core::shared::persistence::Database;
use lattice_store::workspace_notes::DailyNotesRepository;
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
    database: Database,
    program: LearningProgramDto,
    conversation: String,
    version: String,
    notes_written: Arc<AtomicUsize>,
    /// What the chat's turns have named, kept between them as the app keeps it.
    names: Arc<KeptNames>,
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
        let names = self
            .names
            .open(&self.conversation)
            .await
            .expect("the chat's names are readable");
        CourseTurn::resolve(
            &self.database,
            None,
            &self.conversation,
            note_written,
            names,
        )
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
        LearningRecallRepository::new(self.database.clone())
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

    /// The name a card is shown under in `due_reviews`.
    async fn card_name(&self, front: &str) -> String {
        let due = self.text("due_reviews", json!({})).await;
        let mut lines = due.lines();
        while let Some(line) = lines.next() {
            let next = lines.clone().next().map(str::trim);
            if let (Some(name), true) = (line.strip_prefix("- "), next == Some(front)) {
                return name.to_string();
            }
        }
        panic!("{front} is not among the due cards: {due}")
    }

    /// Asks a due card through `ask_card` and returns its name.
    async fn ask(&self, front: &str) -> String {
        let name = self.card_name(front).await;
        self.text("ask_card", json!({ "card": name })).await;
        name
    }

    async fn save_choice_card(&self, front: &str, options: &[&str], correct: usize) -> String {
        let card = id();
        LearningRecallRepository::new(self.database.clone())
            .save_card(&SaveLearningRecallCardRequestDto {
                operation_id: id(),
                program_id: self.program.summary.id.clone(),
                card_id: card.clone(),
                expected_content_revision: None,
                format: LearningRecallCardFormat::MultipleChoice,
                content: LearningRecallContentDto {
                    prompt: front.into(),
                    answer: options[correct].into(),
                    explanation: String::new(),
                    options: options.iter().map(|option| option.to_string()).collect(),
                    correct_option_index: Some(correct),
                    language: None,
                    cloze_deletions: vec![],
                },
                source_version_ids: vec![],
                change_reason: "Test".into(),
            })
            .await
            .expect("the card saves");
        card
    }

    async fn rating_of(&self, card_id: &str) -> String {
        sqlx::query_scalar("SELECT rating FROM study_reviews WHERE card_id=?")
            .bind(card_id)
            .fetch_one(&self.pool)
            .await
            .expect("the review")
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

/// The bracketed name that follows `marker` in `text`.
fn named_after(text: &str, marker: &str) -> String {
    let from = text.find(marker).expect("the marker is in the text") + marker.len();
    let rest = &text[from..];
    let end = rest.find(']').expect("a bracketed name follows") + 1;
    rest[..end].to_string()
}

/// The bracketed name written just before `text` in `shown`.
fn name_before(shown: &str, text: &str) -> String {
    let at = shown.find(&format!(" {text}")).expect("the text is shown");
    shown[..at]
        .rsplit(char::is_whitespace)
        .next()
        .expect("a name comes first")
        .to_string()
}

async fn add_source(db: &Database, program_id: &str, title: &str, text: &str) -> String {
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
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let program = tests::fixture();
    repo.create(&program).await.expect("the course saves");
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await
    .expect("the course is accepted");
    let version = add_source(&db, &program.summary.id, "Search notes", SOURCE_TEXT).await;
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
        &db,
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
        database: db,
        pool,
        program,
        conversation,
        version,
        notes_written: Arc::default(),
        names: Arc::new(KeptNames::new(Arc::new(MemoryNames::new()))),
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
        &course.database,
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

    LearningRepository::new(course.database.clone())
        .delete(&course.program.summary.id)
        .await
        .expect("the course is deleted");

    assert_eq!(
        binding(&course.pool, &course.conversation).await,
        (None, None)
    );
    let note_written: NoteWritten = Arc::new(|| {});
    assert!(
        CourseTurn::resolve(
            &course.database,
            None,
            &course.conversation,
            note_written,
            Arc::new(TurnNames::unstored()),
        )
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
    assert!(bind_conversation(&course.database, &foreign_lesson)
        .await
        .is_err());

    let gone = request(&course.conversation, &id(), None);
    assert!(bind_conversation(&course.database, &gone).await.is_err());

    let draft = tests::fixture();
    let repo = LearningRepository::new(course.database.clone());
    repo.create(&draft).await.expect("the draft saves");
    let unaccepted = request(&course.conversation, &draft.summary.id, None);
    assert!(bind_conversation(&course.database, &unaccepted)
        .await
        .is_err());

    let thread = id();
    sqlx::query(
        "INSERT INTO conversations (id,title,explorer_root) VALUES (?,'Folder','/project')",
    )
    .bind(&thread)
    .execute(&course.pool)
    .await
    .expect("the thread saves");
    let folder_thread = request(&thread, program, None);
    assert!(bind_conversation(&course.database, &folder_thread)
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
    assert!(passage.label.ends_with(" [r1]"), "{}", passage.label);
    assert!(
        !passage.label.contains(&course.version),
        "{}",
        passage.label
    );
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
    let passage = first.passages.passages.first().expect("a stretch");
    assert_eq!(passage.source.content.chars().count(), 20);
    assert_eq!(first.next, Some(20));
    let note = first.passages.note.expect("a note");
    assert!(!note.contains("offset"), "{note}");

    let rest = turn
        .read_source(&course.version, 20, 20_000)
        .await
        .expect("it reads")
        .expect("the course has the source");
    let passage = rest.passages.passages.first().expect("the rest");
    assert!(passage
        .source
        .content
        .ends_with("term frequency and rarity."));
    assert_eq!(rest.next, None);
    assert!(rest
        .passages
        .note
        .expect("a note")
        .contains("the end of this source"));

    let past = turn
        .read_source(&course.version, 9_000, 20_000)
        .await
        .expect("it reads")
        .expect("the course has the source");
    assert!(past.passages.passages.is_empty());

    let nothing = course
        .call("read_course_source", json!({ "source": "[r9]" }))
        .await;
    assert!(!nothing.success, "a name that names nothing is refused");
    assert!(
        nothing
            .error_message
            .expect("why")
            .contains("names nothing shown"),
        "the model is told how to get a valid name"
    );
    let raw = course
        .call("read_course_source", json!({ "source": id() }))
        .await;
    assert!(!raw.success, "an id is not a name");
}

#[tokio::test]
async fn read_course_source_takes_the_name_a_passage_label_shows() {
    let course = course().await;
    let found = course
        .passages("search_course", json!({ "query": "rank fusion" }))
        .await;
    let label = &found.passages.first().expect("a passage").label;
    let name = label.rsplit(' ').next().expect("the name closes the label");
    let read = course
        .passages("read_course_source", json!({ "source": name }))
        .await;
    assert_eq!(read.passages.len(), 1);
    let bare = course
        .passages(
            "read_course_source",
            json!({ "source": name.trim_matches(['[', ']']) }),
        )
        .await;
    assert_eq!(bare.passages.len(), 1, "the brackets are optional");
}

#[tokio::test]
async fn a_cut_read_carries_the_call_that_reads_on() {
    let course = course().await;
    let outline = course.text("course_outline", json!({})).await;
    let name = named_after(&outline, "Search notes ");
    let turn = course.turn().await;
    let cut = turn
        .execute_tool(
            FunctionCall::new(id(), "read_course_source", json!({ "source": name })),
            640,
        )
        .await
        .expect("it runs")
        .data
        .expect("a result");
    let read_on = cut.get("read_on").expect("where to read on");
    assert_eq!(read_on["tool"], "read_course_source");
    assert_eq!(read_on["arguments"]["source"], name);
    assert_eq!(read_on["arguments"]["offset"], 40);

    let rest = turn
        .execute_tool(
            FunctionCall::new(id(), "read_course_source", read_on["arguments"].clone()),
            20_000,
        )
        .await
        .expect("it runs")
        .data
        .expect("a result");
    let rest: SourcePassages = serde_json::from_value(rest).expect("passages");
    assert!(rest
        .passages
        .first()
        .expect("the rest")
        .source
        .content
        .ends_with("term frequency and rarity."));
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
    assert!(
        outline.contains("1.1 Lesson 0-0 [finished] [l1]"),
        "{outline}"
    );
    assert!(outline.contains("1.2 Lesson 0-1 [l2]"), "{outline}");
    assert!(outline.contains("Search notes [r2]"), "{outline}");
    assert!(!outline.contains(&lesson), "{outline}");
    assert!(!outline.contains(&course.version), "{outline}");
}

#[tokio::test]
async fn due_reviews_shows_fronts_and_never_answers() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;

    let due = course.text("due_reviews", json!({})).await;

    assert!(due.contains("- [c1]\n  What does k flatten?"), "{due}");
    assert!(
        !due.contains(&card),
        "a card is shown by its name only: {due}"
    );
    assert!(!due.contains("The hidden answer"), "{due}");
}

#[tokio::test]
async fn due_reviews_says_how_many_it_had_no_room_for() {
    let course = course().await;
    for number in 0..12 {
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
    let recall = LearningRecallRepository::new(course.database.clone());
    let program = course.program.summary.id.clone();
    assert_eq!(
        recall.workspace(&program).await.expect("cards").due_count,
        1
    );
    let name = course.ask("What does k flatten?").await;

    let recorded = course
        .text("record_recall", json!({ "card": name, "rating": "good" }))
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
    let name = course.ask("What does k flatten?").await;
    let rate = json!({ "card": name, "rating": "good" });
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
/// guess, and it would move the card as a real answer does. What a reply says
/// is the tutor's own prose: only `ask_card` records an asking.
#[tokio::test]
async fn a_card_the_tutor_never_asked_with_ask_card_keeps_its_schedule() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let name = course.card_name("What does k flatten?").await;
    let before = course.schedule(&card).await;
    reply(
        &course.pool,
        &course.conversation,
        1,
        "What does k flatten?",
    )
    .await;

    let result = course
        .call("record_recall", json!({ "card": name, "rating": "good" }))
        .await;

    assert!(!result.success);
    assert!(
        result.error_message.expect("why").contains("ask_card"),
        "the model is told how to ask it"
    );
    assert_eq!(course.schedule(&card).await, before);
}

/// A card asked in one turn is rated in the next by the name it had, and it
/// has that name in the next turn's facts.
#[tokio::test]
async fn a_card_asked_in_one_turn_is_rated_in_the_next_by_the_name_it_had() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    course.save_card("What does BM25 score?").await;
    let name = course.ask("What does BM25 score?").await;

    let block = course.turn().await.context_block(4_000, true, None).await;
    assert!(
        block.contains(&format!("- {name}\n  What does BM25 score?")),
        "{block}"
    );
    course
        .text("record_recall", json!({ "card": name, "rating": "easy" }))
        .await;

    assert_eq!(course.schedule(&card).await.0, 0, "the other card waits");
}

/// An asking counts for the card as it was then: after a review the card has
/// to be asked again.
#[tokio::test]
async fn a_card_must_be_asked_again_after_its_review() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let name = course.ask("What does k flatten?").await;
    let rate = json!({ "card": name, "rating": "good" });
    course.text("record_recall", rate.clone()).await;
    sqlx::query("UPDATE study_cards SET due_at=0 WHERE id=?")
        .bind(&card)
        .execute(&course.pool)
        .await
        .expect("the card falls due again");
    let before = course.schedule(&card).await;

    let result = course.call("record_recall", rate.clone()).await;

    assert!(!result.success);
    assert!(result.error_message.expect("why").contains("ask_card"));
    assert_eq!(course.schedule(&card).await, before);
    course.text("ask_card", json!({ "card": name })).await;
    course.text("record_recall", rate).await;
    assert_eq!(course.schedule(&card).await.0, 2);
}

#[tokio::test]
async fn ask_card_gives_the_card_as_written_and_never_its_answer() {
    let course = course().await;
    course.save_card("What does k flatten?").await;
    let name = course.card_name("What does k flatten?").await;

    let asked = course.text("ask_card", json!({ "card": name })).await;

    assert!(asked.contains("What does k flatten?"), "{asked}");
    assert!(asked.contains(&name), "{asked}");
    assert!(!asked.contains("The hidden answer"), "{asked}");
}

#[tokio::test]
async fn ask_card_refuses_a_card_that_is_not_due() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let name = course.ask("What does k flatten?").await;
    course
        .text("record_recall", json!({ "card": name, "rating": "good" }))
        .await;
    let before = course.schedule(&card).await;

    let again = course.call("ask_card", json!({ "card": name })).await;

    assert!(!again.success);
    assert!(again.error_message.expect("why").contains("not due"));
    assert_eq!(course.schedule(&card).await, before);
}

#[tokio::test]
async fn record_recall_refuses_a_name_that_names_nothing() {
    let course = course().await;
    for card in [json!("[c9]"), json!(id())] {
        let result = course
            .call("record_recall", json!({ "card": card, "rating": "good" }))
            .await;
        assert!(!result.success);
        assert!(result
            .error_message
            .expect("why")
            .contains("names nothing shown"));
    }
}

#[tokio::test]
async fn record_recall_refuses_a_rating_it_cannot_read() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let name = course.ask("What does k flatten?").await;
    let before = course.schedule(&card).await;
    let result = course
        .call(
            "record_recall",
            json!({ "card": name, "rating": "pretty good" }),
        )
        .await;
    assert!(!result.success);
    assert!(result.error_message.expect("why").contains("again, hard"));
    let missing = course.call("record_recall", json!({ "card": name })).await;
    assert!(!missing.success, "a flashcard needs its rating");
    assert_eq!(course.schedule(&card).await, before);
}

#[tokio::test]
async fn a_multiple_choice_card_is_rated_by_the_option_the_learner_chose() {
    let course = course().await;
    let card = course
        .save_choice_card(
            "Which flattens the top ranks?",
            &["A smaller k", "A larger k"],
            1,
        )
        .await;

    let due = course.text("due_reviews", json!({})).await;
    assert!(due.contains("[o1] A smaller k"), "{due}");
    assert!(due.contains("[o2] A larger k"), "{due}");
    let name = course.ask("Which flattens the top ranks?").await;

    let unsaid = course
        .call("record_recall", json!({ "card": name, "rating": "good" }))
        .await;
    assert!(!unsaid.success, "the option decides the rating");

    course
        .text(
            "record_recall",
            json!({ "card": name, "rating": "easy", "option": "[o1]" }),
        )
        .await;
    assert_eq!(
        course.rating_of(&card).await,
        "again",
        "a wrong choice is a miss"
    );
}

/// The option is named, not counted: the one the learner chose is the one
/// whose name is written, however the card's list is numbered.
#[tokio::test]
async fn a_multiple_choice_option_names_the_option_and_not_its_position() {
    let course = course().await;
    let card = course
        .save_choice_card(
            "Which flattens the top ranks?",
            &["A smaller k", "A larger k"],
            1,
        )
        .await;
    let name = course.card_name("Which flattens the top ranks?").await;
    let asked = course.text("ask_card", json!({ "card": name })).await;
    let option = name_before(&asked, "A larger k");

    course
        .text("record_recall", json!({ "card": name, "option": option }))
        .await;

    assert_eq!(course.rating_of(&card).await, "good");
}

#[tokio::test]
async fn an_option_of_another_card_is_refused() {
    let course = course().await;
    course
        .save_choice_card("Which is larger?", &["Ten", "Two"], 0)
        .await;
    course
        .save_choice_card("Which is smaller?", &["Seven", "Three"], 1)
        .await;
    let larger = course.card_name("Which is larger?").await;
    course.text("ask_card", json!({ "card": larger })).await;
    let smaller = course.card_name("Which is smaller?").await;
    let asked = course.text("ask_card", json!({ "card": smaller })).await;
    let foreign = name_before(&asked, "Seven");

    let result = course
        .call(
            "record_recall",
            json!({ "card": larger, "option": foreign }),
        )
        .await;

    assert!(!result.success);
    assert!(result.error_message.expect("why").contains("another card"));
}

#[tokio::test]
async fn save_card_cites_its_source_and_joins_the_recall_deck() {
    let course = course().await;
    let outline = course.text("course_outline", json!({})).await;
    let source = named_after(&outline, "Search notes ");
    let saved = course
        .text(
            "save_card",
            json!({
                "question": "What does a larger k do?",
                "answer": "It flattens the weight of the top ranks.",
                "sources": [source],
            }),
        )
        .await;
    let name = named_after(&saved, "as card ");
    assert!(!saved.contains(&course.version), "{saved}");

    let workspace = LearningRecallRepository::new(course.database.clone())
        .workspace(&course.program.summary.id)
        .await
        .expect("cards");
    let card = workspace.cards.first().expect("the card");
    assert_eq!(
        card.source_version_ids,
        std::slice::from_ref(&course.version)
    );
    assert_eq!(workspace.due_count, 1, "the tutor can ask it at once");
    course.text("ask_card", json!({ "card": name })).await;
    course
        .text("record_recall", json!({ "card": name, "rating": "hard" }))
        .await;
    assert_eq!(course.schedule(&card.id).await.0, 1);
}

#[tokio::test]
async fn save_card_refuses_a_card_with_no_citation_when_the_course_has_sources() {
    let course = course().await;
    let result = course
        .call(
            "save_card",
            json!({ "question": "Q?", "answer": "A.", "sources": [] }),
        )
        .await;
    assert!(!result.success);
    assert!(result.error_message.expect("why").contains("source"));

    let outside = course
        .call(
            "save_card",
            json!({ "question": "Q?", "answer": "A.", "sources": [id()] }),
        )
        .await;
    assert!(!outside.success, "a source outside the course is refused");
}

#[tokio::test]
async fn save_note_adds_the_summary_to_the_lesson_s_note() {
    let course = course().await;
    let lesson = course.lesson(0, 1);
    let outline = course.text("course_outline", json!({})).await;
    let name = named_after(&outline, "Lesson 0-1 ");
    let saved = course
        .text(
            "save_note",
            json!({ "summary": "Fusion adds up reciprocal ranks.", "lesson": name }),
        )
        .await;
    assert!(saved.contains("Lesson 0-1"), "{saved}");
    assert_eq!(course.notes_written.load(Ordering::SeqCst), 1);

    let memory = LearningRepository::new(course.database.clone())
        .memory_state(&course.program.summary.id)
        .await
        .expect("the notebook");
    let (noted, note_id) = memory.lesson_notes.first().expect("the note");
    assert_eq!(noted, &lesson);
    let notes = DailyNotesRepository::new(course.database.clone());
    let note = notes.get(note_id).await.expect("the note");
    assert!(note.content.contains("Fusion adds up reciprocal ranks."));

    course
        .text(
            "save_note",
            json!({ "summary": "And k flattens them.", "lesson": name }),
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
    let memory = LearningRepository::new(course.database.clone())
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
        &course.database,
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

    bind_conversation(&course.database, &on_lesson(course.lesson(0, 0)))
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

    bind_conversation(&course.database, &on_lesson(course.lesson(1, 0)))
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

/// Tool results are gone by the next turn, so the name a card is rated by has
/// to come with the turn that rates it.
#[tokio::test]
async fn each_turn_shows_the_next_due_cards_by_name_and_never_their_answers() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;

    let block = course.turn().await.context_block(4_000, true, None).await;

    assert!(block.contains("- [c1]\n  What does k flatten?"), "{block}");
    assert!(!block.contains(&card), "{block}");
    assert!(!block.contains("The hidden answer"), "{block}");
    assert!(
        !course
            .turn()
            .await
            .context_block(4_000, false, None)
            .await
            .contains("- [c1]"),
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
    assert_eq!(block.matches("- [c").count(), 5, "{block}");
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
    for tool in ["ask_card", "record_recall", "save_card", "save_note"] {
        assert!(turn.owns_tool(tool) && !turn.reads_only(tool), "{tool}");
    }
    assert!(!turn.owns_tool("read_file"));

    let mut offered = Vec::new();
    turn.add_tools(&mut offered);
    turn.add_tools(&mut offered);
    assert_eq!(offered.len(), 8, "each tool once");
}

/// The model writes names it was shown and its own words: never an id, an
/// offset or a position.
#[test]
fn no_tool_asks_the_model_for_an_id_an_offset_or_a_position() {
    for definition in super::tools::definitions() {
        let properties = definition.parameters["properties"]
            .as_object()
            .expect("properties");
        for key in properties.keys() {
            assert!(
                ![
                    "card_id",
                    "source_id",
                    "source_ids",
                    "lesson_id",
                    "offset",
                    "selected_option"
                ]
                .contains(&key.as_str()),
                "{} takes {key}",
                definition.name
            );
        }
    }
}

/// Nothing a tool shows holds an id of the app.
#[tokio::test]
async fn no_tool_result_shows_an_id() {
    let course = course().await;
    let card = course.save_card("What does k flatten?").await;
    let shown = [
        course.text("course_outline", json!({})).await,
        course.text("due_reviews", json!({})).await,
        course.ask("What does k flatten?").await,
        serde_json::to_string(
            &course
                .passages("search_course", json!({ "query": "rank fusion" }))
                .await
                .passages
                .iter()
                .map(|passage| passage.label.clone())
                .collect::<Vec<_>>(),
        )
        .expect("labels"),
        course.turn().await.context_block(4_000, true, None).await,
    ]
    .join(
        "
",
    );
    for id in [
        card,
        course.version.clone(),
        course.lesson(0, 0),
        course.program.summary.id.clone(),
    ] {
        assert!(!shown.contains(&id), "{id} is shown: {shown}");
    }
}

/// What the model reads, byte for byte. The digest moves only when one of them
/// does, and then the tutor's evals run again (`evals/tutor`).
#[test]
fn the_tutor_s_prompt_and_tool_schemas_are_pinned() {
    use sha2::{Digest, Sha256};
    let schemas = serde_json::to_string(&super::tools::definitions()).expect("the schemas");
    let digest = Sha256::digest(format!("{TUTOR_TEMPLATE}\u{0}{schemas}"));
    assert_eq!(
        format!("{digest:x}"),
        "192d1a5ad20cd7d6892122f00e787780658989b5c34534a70326600cd289df3a"
    );
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
        "ask_card",
        "record_recall",
        "save_card",
        "save_note",
        "mermaid",
    ] {
        assert!(TUTOR_TEMPLATE.contains(method), "{method}");
    }
    assert!(
        TUTOR_TEMPLATE.ends_with("Answer:"),
        "the learner's words end on the cue"
    );
}
