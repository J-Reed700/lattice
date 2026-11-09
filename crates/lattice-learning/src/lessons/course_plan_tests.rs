//! The course plan is made once per outline revision, whatever number of
//! lessons ask for it, and every lesson of the outline is in it exactly once.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::repository::LearningRepository;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
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
}

async fn saved_program(pool: &SqlitePool) -> LearningProgramDto {
    let program = crate::tests::fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await
        .unwrap();
    program
}

fn ids(program: &LearningProgramDto) -> Vec<String> {
    lesson_ids(program).into_iter().map(str::to_owned).collect()
}

fn answer(lessons: &[&str]) -> String {
    answer_of(lessons, "an idea")
}

fn answer_of(lessons: &[&str], idea: &str) -> String {
    json!({"lessons":lessons.iter().map(|id| json!({
        "lessonId":id,"ideas":[idea],"terms":[],"actions":[],"contrast":""
    })).collect::<Vec<_>>()})
    .to_string()
}

#[test]
fn the_schema_lists_every_lesson_id_as_a_choice() {
    let program = crate::tests::fixture();
    let schema = plan_schema(&program);
    let choices = &schema["properties"]["lessons"]["items"]["properties"]["lessonId"]["enum"];
    assert_eq!(*choices, json!(ids(&program)));
    assert_eq!(
        ids(&program).len(),
        4,
        "the fixture course has four lessons"
    );
    let item = &schema["properties"]["lessons"]["items"];
    assert_eq!(
        item["required"],
        json!(["lessonId", "ideas", "terms", "actions", "contrast"])
    );
}

#[test]
fn a_plan_names_every_lesson_once_and_comes_back_in_course_order() {
    let program = crate::tests::fixture();
    let order = ids(&program);
    let shuffled: Vec<&str> = order.iter().rev().map(String::as_str).collect();
    let plan = parse_plan(&answer(&shuffled), &program).unwrap();
    let planned: Vec<_> = plan.lessons.iter().map(|l| l.lesson_id.clone()).collect();
    assert_eq!(planned, order);
}

#[test]
fn a_plan_that_leaves_out_or_repeats_a_lesson_is_refused_naming_it() {
    let program = crate::tests::fixture();
    let order = ids(&program);
    let all: Vec<&str> = order.iter().map(String::as_str).collect();

    let missing = parse_plan(&answer(&all[1..]), &program).unwrap_err();
    assert!(matches!(missing, AppError::InvalidInput(_)), "{missing:?}");
    assert!(missing.to_string().contains(&order[0]), "{missing}");

    let mut twice = all.clone();
    twice.push(all[2]);
    let repeated = parse_plan(&answer(&twice), &program).unwrap_err();
    assert!(
        matches!(repeated, AppError::InvalidInput(_)),
        "{repeated:?}"
    );
    assert!(repeated.to_string().contains(&order[2]), "{repeated}");

    let mut stranger = all.clone();
    stranger.push("not-a-lesson");
    let unknown = parse_plan(&answer(&stranger), &program).unwrap_err();
    assert!(unknown.to_string().contains("not-a-lesson"), "{unknown}");
}

#[tokio::test]
async fn two_overlapping_calls_with_different_answers_save_one_plan_and_both_return_it() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let order = ids(&program);
    let all: Vec<&str> = order.iter().map(String::as_str).collect();
    let first = Planner::answering(vec![answer_of(&all, "an idea from the first model")]);
    let second = Planner::answering(vec![answer_of(&all, "an idea from the second model")]);

    let (one, other) = tokio::join!(
        ensure(&pool, &first, &program, 1, &Default::default()),
        ensure(&pool, &second, &program, 1, &Default::default())
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
async fn the_call_is_background_work_asked_under_the_plans_schema() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let llm = Planner::new();

    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    let sent = llm.request(0);
    assert_eq!(sent.priority, InferencePriority::Background);
    assert_eq!(sent.json_schema, Some(plan_schema(&program)));
    let (_, user) = crate::lessons::structured_call::task_messages(&sent);
    let task: Value = serde_json::from_str(user).unwrap();
    assert_eq!(task["goal"], program.summary.goal.as_str());
    assert_eq!(task["priorKnowledge"], program.prior_knowledge.as_str());
    let lessons = task["modules"][0]["lessons"].as_array().unwrap();
    assert_eq!(lessons[0]["lessonId"], ids(&program)[0].as_str());
    assert_eq!(lessons[0]["objective"], "Explain the central idea");
    assert_eq!(lessons[0]["estimatedMinutes"], 20);
}

#[tokio::test]
async fn a_new_outline_revision_is_planned_again() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let llm = Planner::new();

    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 1);
    ensure(&pool, &llm, &program, 2, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 2);
}

#[tokio::test]
async fn after_a_restart_the_plan_is_read_from_the_database_with_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let before = Planner::new();
    let made = ensure(&pool, &before, &program, 1, &Default::default())
        .await
        .unwrap();

    let after = Planner::answering(vec!["not a plan".into()]);
    let read = ensure(&pool, &after, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(after.calls(), 0);
    assert_eq!(read, made);
}

#[tokio::test]
async fn a_gap_in_the_plan_is_asked_for_again_with_the_lesson_it_left_out() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let order = ids(&program);
    let gap: Vec<&str> = order[1..].iter().map(String::as_str).collect();
    let llm = Planner::answering(vec![answer(&gap)]);

    let plan = ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 2);
    assert_eq!(plan.lessons.len(), 4);
    let asked = llm.request(1);
    let said = asked
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains(&order[0]) && said.contains("exactly once"),
        "{said}"
    );
}

#[tokio::test]
async fn a_plan_that_stays_wrong_is_not_saved_and_the_next_caller_asks_again() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let order = ids(&program);
    let gap: Vec<&str> = order[1..].iter().map(String::as_str).collect();
    let llm = Planner::answering(vec![answer(&gap), answer(&gap)]);

    let error = ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(llm.calls(), 2);

    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 3);
}

#[tokio::test]
async fn the_outline_revision_is_the_programs_content_revision() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let stored: i64 =
        sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        outline_revision(&pool, &program.summary.id).await.unwrap(),
        stored
    );
    assert!(matches!(
        outline_revision(&pool, "no-such-program").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn one_lessons_entry_is_replaced_without_touching_the_others_and_the_next_read_sees_it() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let llm = Planner::new();
    let made = ensure(&pool, &llm, &program, 1, &Default::default())
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

    let read = ensure(&pool, &llm, &program, 1, &Default::default())
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
    let program = saved_program(&pool).await;
    let llm = Planner::new();
    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    sqlx::query("DELETE FROM learning_course_plan_lessons WHERE program_id=? AND lesson_id=?")
        .bind(&program.summary.id)
        .bind(&ids(&program)[0])
        .execute(&pool)
        .await
        .unwrap();

    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();

    assert_eq!(llm.calls(), 2);
}

#[tokio::test]
async fn a_plan_made_before_accept_is_reused_after_it_with_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let llm = Planner::new();
    let before = outline_revision(&pool, &program.summary.id).await.unwrap();
    let made = ensure(&pool, &llm, &program, before, &Default::default())
        .await
        .unwrap();

    LearningRepository::new(pool.clone())
        .accept(&crate::dto::AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await
        .unwrap();

    let after = outline_revision(&pool, &program.summary.id).await.unwrap();
    assert_eq!(after, before, "accepting does not change the outline");
    let accepted = LearningRepository::new(pool.clone())
        .get(&program.summary.id)
        .await
        .unwrap();
    let read = ensure(&pool, &llm, &accepted, after, &Default::default())
        .await
        .unwrap();
    assert_eq!(llm.calls(), 1);
    assert_eq!(read, made);
}

#[tokio::test]
async fn an_unreadable_entry_is_an_error_naming_its_lesson_and_costs_no_call() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let llm = Planner::new();
    ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap();
    let target = ids(&program)[2].clone();
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json='{\"x\":1}' WHERE program_id=? AND lesson_id=?")
        .bind(&program.summary.id)
        .bind(&target)
        .execute(&pool)
        .await
        .unwrap();

    let error = ensure(&pool, &llm, &program, 1, &Default::default())
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::InternalError(_)), "{error:?}");
    assert!(error.to_string().contains(&target), "{error}");
    assert_eq!(llm.calls(), 1, "no new plan is paid for");
}
