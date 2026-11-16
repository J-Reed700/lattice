//! The course plan is made once per outline revision, whatever number of
//! lessons ask for it, and every lesson of the outline is in it exactly once.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::repository::LearningRepository;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A model that plans the course it is asked about, with the answers it is
/// told to give first, and keeps every request.
struct Planner {
    requests: Mutex<Vec<CompletionRequest>>,
    answers: Mutex<Vec<String>>,
    calls: AtomicUsize,
}

impl Planner {
    fn new() -> Self {
        Self::answering(Vec::new())
    }

    /// Answers `answers` in order, then plans the course.
    fn answering(answers: Vec<String>) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            answers: Mutex::new(answers.into_iter().rev().collect()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn request(&self, index: usize) -> CompletionRequest {
        self.requests.lock().unwrap()[index].clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Planner {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());
        // Long enough for every other caller to reach the plan meanwhile.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let given = self.answers.lock().unwrap().pop();
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let text = given
            .or_else(|| fixture_response(user))
            .unwrap_or_else(|| panic!("not a planning request: {user}"));
        Ok(CompletionResponse::from_text(text))
    }
    fn model_name(&self) -> &str {
        "planner-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

async fn saved_program(db: &Database) -> LearningProgramDto {
    let program = crate::tests::fixture();
    LearningRepository::new(db.clone())
        .create(&program)
        .await
        .unwrap();
    program
}

fn ids(program: &LearningProgramDto) -> Vec<String> {
    lesson_ids(program).into_iter().map(str::to_owned).collect()
}

/// The handles the four lessons of the fixture course are shown under.
const HANDLES: [&str; 4] = ["l1", "l2", "l3", "l4"];

fn answer(handles: &[&str]) -> String {
    answer_of(handles, "an idea")
}

fn answer_of(handles: &[&str], idea: &str) -> String {
    json!({"entries":handles.iter().map(|handle| json!({
        "of":handle,"ideas":[{"topic":idea}],"terms":[],"actions":[],"contrast":""
    })).collect::<Vec<_>>()})
    .to_string()
}

fn task_of(user: &str) -> Value {
    task_in(user).unwrap()
}

fn plan_input(program: &LearningProgramDto) -> PlanInput {
    PlanInput {
        goal: Told(program.summary.goal.clone()),
        prior_knowledge: Told(program.prior_knowledge.clone()),
        modules: outline::modules_of(program),
        lessons: program
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .map(|lesson| outline::lesson_of(lesson, true))
            .collect(),
        others: Vec::new(),
        planned: Vec::new(),
    }
}

#[test]
fn the_schema_names_every_lesson_by_its_handle() {
    use lattice_core::application::services::model_contract::shape::Held;
    use lattice_core::application::services::model_contract::test_support::samples::schema_of;
    let program = crate::tests::fixture();
    let schema = schema_of::<PlanCourse>(&plan_input(&program), Held::Enums).unwrap();
    let item = &schema.as_json()["properties"]["entries"]["items"];
    assert_eq!(item["properties"]["of"]["enum"], json!(HANDLES));
    assert_eq!(item["properties"]["ideas"]["type"], "array");
    assert_eq!(
        item["properties"]["ideas"]["items"]["required"],
        json!(["topic"])
    );
    assert_eq!(
        ids(&program).len(),
        4,
        "the fixture course has four lessons"
    );
}

#[test]
fn the_plan_request_shows_what_the_model_is_told_and_nothing_internal() {
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    let mut marks = Marks::new();
    let lesson = |marks: &mut Marks, id: &str, title: &str| outline::Lesson {
        id: marks.reference(id.to_owned()),
        title: marks.told(title.to_owned()),
        objective: marks.told(format!("Explain {title} at the harbour.")),
        minutes: Some(marks.told(20_i64)),
        taught: Some(outline::Taught {
            sections: vec![marks.told("Reading the tide tables".to_owned())],
            recap: vec![marks.told("Takeaway: the tide turns twice a day.".to_owned())],
        }),
    };
    let first = lesson(&mut marks, "lesson-uuid-0001", "Reading the tide tables");
    let second = lesson(&mut marks, "lesson-uuid-0002", "Mooring a small boat");
    let input = PlanInput {
        goal: marks.told("Run the harbour office.".to_owned()),
        prior_knowledge: marks.told("Some time on the water.".to_owned()),
        modules: vec![outline::Module {
            title: marks.told("The harbour basics".to_owned()),
            milestone: None,
            lessons: vec![
                marks.internal(first.id.clone()),
                marks.internal(second.id.clone()),
            ],
        }],
        lessons: vec![first],
        others: vec![outline::Other(second.clone())],
        planned: vec![outline::Held {
            lesson: marks.internal(second.id.clone()),
            ideas: marks.told(vec!["How a mooring holds a boat".to_owned()]),
            terms: marks.told(vec!["Cleat and bollard fittings".to_owned()]),
            actions: marks.told(vec!["Tying a bowline knot".to_owned()]),
            contrast: marks.told("Mooring against anchoring at sea.".to_owned()),
        }],
    };
    holds::<PlanCourse>(&input, &marks);
}

#[tokio::test]
async fn a_plan_comes_back_in_course_order_whatever_order_the_entries_were_written_in() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::answering(vec![answer(&["l4", "l3", "l2", "l1"])]);

    let plan = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 1);
    let planned: Vec<_> = plan.lessons.iter().map(|l| l.lesson_id.clone()).collect();
    assert_eq!(planned, ids(&program));
}

#[tokio::test]
async fn two_overlapping_calls_with_different_answers_save_one_plan_and_both_return_it() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let first = Planner::answering(vec![answer_of(&HANDLES, "an idea from the first model")]);
    let second = Planner::answering(vec![answer_of(&HANDLES, "an idea from the second model")]);

    let (one_lock, other_lock) = (CourseSetups::default(), CourseSetups::default());
    let database = db.clone();
    let (one, other) = tokio::join!(
        ensure(&database, &first, &program, 1, &one_lock),
        ensure(&database, &second, &program, 1, &other_lock)
    );

    let (one, other) = (one.unwrap(), other.unwrap());
    assert_eq!((first.calls(), second.calls()), (1, 1), "both planned");
    assert_eq!(one, other);
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT lesson_id,entry_json FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=1",
    )
    .bind(&program.summary.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 4);
    for (id, entry) in &rows {
        let entry: PlannedLesson = serde_json::from_str(entry).unwrap();
        assert_eq!(one.lesson(id), Some(&entry), "the result is what was saved");
    }
    let ideas: std::collections::HashSet<_> = one
        .lessons
        .iter()
        .flat_map(|entry| entry.ideas.clone())
        .collect();
    assert_eq!(ideas.len(), 1, "every entry is from one answer: {ideas:?}");
}

#[tokio::test]
async fn the_call_is_background_work_that_shows_lessons_by_handle() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    let sent = llm.request(0);
    assert_eq!(sent.priority, InferencePriority::Background);
    let (_, user) = crate::lessons::structured_call::task_messages(&sent);
    assert!(
        ids(&program).iter().all(|id| !user.contains(id.as_str())),
        "no lesson id reaches the model: {user}"
    );
    let task = task_of(user);
    assert_eq!(task["goal"], program.summary.goal.as_str());
    assert_eq!(task["priorKnowledge"], program.prior_knowledge.as_str());
    let lessons = task["modules"][0]["lessons"].as_array().unwrap();
    assert_eq!(lessons[0]["lessonId"], "l1");
    assert_eq!(lessons[0]["objective"], "Explain the central idea");
    assert_eq!(lessons[0]["estimatedMinutes"], 20);
}

#[tokio::test]
async fn a_new_outline_revision_is_planned_again() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 1);
    ensure(&db, &llm, &program, 2, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 2);
}

#[tokio::test]
async fn after_a_restart_the_plan_is_read_from_the_database_with_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let before = Planner::new();
    let made = ensure(&db, &before, &program, 1, &Default::default())
        .await
        .unwrap();

    let after = Planner::answering(vec!["not a plan".into()]);
    let read = ensure(&db, &after, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(after.calls(), 0);
    assert_eq!(read, made);
}

fn said_in(request: &CompletionRequest) -> String {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn a_lesson_left_out_is_asked_for_again_alone_with_the_others_shown_as_planned() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::answering(vec![answer(&HANDLES[1..])]);

    let plan = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 2);
    assert_eq!(plan.lessons.len(), 4);
    let request = llm.request(1);
    let (system, user) = crate::lessons::structured_call::task_messages(&request);
    let task = task_of(user);
    let lessons: Vec<&Value> = task["modules"][0]["lessons"]
        .as_array()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(lessons[0]["lessonId"], "l1");
    assert!(lessons[1..]
        .iter()
        .all(|lesson| lesson.get("lessonId").is_none() && lesson.get("otherLessonId").is_some()));
    assert_eq!(task["planned"].as_array().unwrap().len(), 3);
    assert!(system.contains("otherLessonId"), "{system}");
    assert!(
        ids(&program)
            .iter()
            .all(|id| !said_in(&llm.request(1)).contains(id.as_str())),
        "no lesson id reaches the model"
    );
}

#[tokio::test]
async fn a_lesson_that_stays_unanswered_fails_the_plan_and_saves_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let gap = answer(&HANDLES[1..]);
    let llm = Planner::answering(vec![gap.clone(), gap.clone(), gap]);

    let error = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(llm.calls(), 3);
    let saved: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_course_plan_lessons")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(saved, 0);

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 4);
}

#[tokio::test]
async fn an_entry_with_a_blank_topic_is_corrected_for_that_lesson_alone() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let blank = json!({"entries":HANDLES.iter().map(|handle| json!({
        "of":handle,"ideas":[{"topic":if *handle == "l2" { "  " } else { "an idea" }}],
        "terms":[],"actions":[],"contrast":""
    })).collect::<Vec<_>>()})
    .to_string();
    let llm = Planner::answering(vec![blank]);

    let plan = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 2);
    assert!(said_in(&llm.request(1)).contains("cannot be blank"));
    let request = llm.request(1);
    let (_, user) = crate::lessons::structured_call::task_messages(&request);
    let task = task_of(user);
    let asked: Vec<&Value> = task["modules"][0]["lessons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|lesson| lesson.get("lessonId").is_some())
        .collect();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["lessonId"], "l2");
    assert_eq!(plan.lessons[0].ideas, ["an idea"], "the others stand");
    assert_eq!(plan.lessons[1].ideas, ["the central idea"]);
}

#[tokio::test]
async fn the_outline_revision_is_the_programs_content_revision() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let stored: i64 =
        sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        outline_revision(&db, &program.summary.id).await.unwrap(),
        stored
    );
    assert!(matches!(
        outline_revision(&db, "no-such-program").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn one_lessons_entry_is_replaced_without_touching_the_others_and_the_next_read_sees_it() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();
    let made = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    let target = ids(&program)[1].clone();
    let edited = PlannedLesson {
        lesson_id: target.clone(),
        ideas: vec!["an idea the learner wrote".into()],
        terms: Vec::new(),
        actions: Vec::new(),
        contrast: String::new(),
    };
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json=?,entry_revision=entry_revision+1 WHERE program_id=? AND outline_revision=1 AND lesson_id=?")
        .bind(serde_json::to_string(&edited).unwrap())
        .bind(&program.summary.id)
        .bind(&target)
        .execute(&pool)
        .await
        .unwrap();

    let read = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 1, "an edit is no reason to plan again");
    assert_eq!(read.lesson(&target), Some(&edited));
    for entry in &made.lessons {
        if entry.lesson_id != target {
            assert_eq!(read.lesson(&entry.lesson_id), Some(entry));
        }
    }
}

#[tokio::test]
async fn a_course_missing_one_entry_is_planned_again() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();
    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    sqlx::query("DELETE FROM learning_course_plan_lessons WHERE program_id=? AND lesson_id=?")
        .bind(&program.summary.id)
        .bind(&ids(&program)[0])
        .execute(&pool)
        .await
        .unwrap();

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 2);
}

#[tokio::test]
async fn a_plan_made_before_accept_is_reused_after_it_with_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();
    let before = outline_revision(&db, &program.summary.id).await.unwrap();
    let made = ensure(&db, &llm, &program, before, &Default::default())
        .await
        .unwrap();

    LearningRepository::new(db.clone())
        .accept(&crate::dto::AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await
        .unwrap();

    let after = outline_revision(&db, &program.summary.id).await.unwrap();
    assert_eq!(after, before, "accepting does not change the outline");
    let accepted = LearningRepository::new(db.clone())
        .get(&program.summary.id)
        .await
        .unwrap();
    let read = ensure(&db, &llm, &accepted, after, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 1);
    assert_eq!(read, made);
}

#[tokio::test]
async fn an_unreadable_entry_is_an_error_naming_its_lesson_and_costs_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let llm = Planner::new();
    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    let target = ids(&program)[2].clone();
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json='{\"x\":1}' WHERE program_id=? AND lesson_id=?")
        .bind(&program.summary.id)
        .bind(&target)
        .execute(&pool)
        .await
        .unwrap();

    let error = ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::InternalError(_)), "{error:?}");
    assert!(error.to_string().contains(&target), "{error}");
    assert_eq!(llm.calls(), 1, "no new plan is paid for");
}

#[tokio::test]
async fn a_written_lesson_is_planned_from_what_it_taught() {
    use crate::dto::{LearningBlockDto, LearningBlockKind};
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let mut program = saved_program(&db).await;
    let block = |kind, title: &str, body: &str| {
        LearningBlockDto::new(kind, title.into(), body.into(), Vec::new(), Vec::new())
    };
    {
        let lessons = &mut program.modules[0].lessons;
        lessons[0].blocks = vec![
            block(
                LearningBlockKind::Explanation,
                "Flashing the card",
                "Body one.",
            ),
            block(
                LearningBlockKind::WorkedExample,
                "Writing the image",
                "Body two.",
            ),
            block(LearningBlockKind::Check, "Check yourself", "Body three."),
            block(
                LearningBlockKind::Recap,
                "Recap",
                "Takeaway: the card holds the system.",
            ),
        ];
        lessons[1].blocks = Vec::new();
    }
    let llm = Planner::new();

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    let sent = llm.request(0);
    let (system, user) = crate::lessons::structured_call::task_messages(&sent);
    let task = task_of(user);
    let lessons = task["modules"][0]["lessons"].as_array().unwrap();
    assert_eq!(
        lessons[0]["taught"]["sections"],
        json!(["Flashing the card", "Writing the image"]),
        "the written lesson's section titles reach the planner"
    );
    assert_eq!(
        lessons[0]["taught"]["recap"],
        json!(["Takeaway: the card holds the system."]),
        "the written lesson's recap reaches the planner"
    );
    assert!(
        lessons[1].get("taught").is_none(),
        "a lesson never written has no taught key"
    );
    assert!(
        system.contains("A lesson with taught is already written"),
        "the planner is told what taught means: {system}"
    );
}

#[tokio::test]
async fn prompt_snapshot_course_plan() {
    use crate::dto::{LearningBlockDto, LearningBlockKind};
    use lattice_core::application::services::model_contract::Contract;
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let mut program = crate::tests::fixture();
    crate::lessons::course_generation_tests::fixed_ids(&mut program);
    LearningRepository::new(db.clone())
        .create(&program)
        .await
        .unwrap();
    let block = |kind, title: &str, body: &str| {
        LearningBlockDto::new(kind, title.into(), body.into(), Vec::new(), Vec::new())
    };
    {
        let lessons = &mut program.modules[0].lessons;
        lessons[0].blocks = vec![
            block(
                LearningBlockKind::Explanation,
                "Flashing the card",
                "Body one.",
            ),
            block(
                LearningBlockKind::Recap,
                "Recap",
                "Takeaway: the card holds the system.",
            ),
        ];
        lessons[1].blocks = Vec::new();
    }
    let llm = Planner::new();

    ensure(&db, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    let sent: Vec<CompletionRequest> = llm
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| {
            request
                .contract()
                .is_some_and(|call| call.id == PlanCourse::ID)
        })
        .cloned()
        .collect();
    lattice_core::application::services::model_contract::test_support::snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "course-plan",
        &sent,
    );
}

#[test]
fn the_course_plan_names_actions_in_no_subjects_terms() {
    let start = PLAN_SYSTEM.find("actions, the things").unwrap();
    let end = PLAN_SYSTEM.find("; and contrast").unwrap();
    let rule = &PLAN_SYSTEM[start..end];
    for word in ["command", "option", "folder", "file", "path"] {
        assert!(
            !rule.contains(word),
            "the actions rule mentions \"{word}\": {rule}"
        );
    }
    assert_eq!(
        rule,
        "actions, the things the learner does (procedures, techniques, operations) that this lesson teaches"
    );
}
