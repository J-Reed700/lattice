//! Changing a lesson's topics by hand, by asking the model, and undoing.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::topics_support::*;
use super::*;
use lattice_core::application::ports::llm_port::{CompletionInput, InferencePriority};

fn at(lesson_id: &str, expected: i64) -> (&str, i64) {
    (lesson_id, expected)
}

fn edit_request(
    program: &LearningProgramDto,
    outline_revision: i64,
    expected: i64,
    topics: PlannedLesson,
) -> EditCoursePlanEntryRequestDto {
    EditCoursePlanEntryRequestDto {
        program_id: program.summary.id.clone(),
        outline_revision,
        expected_entry_revision: expected,
        topics,
    }
}

fn change_request(
    program: &LearningProgramDto,
    outline_revision: i64,
    lessons: &[(&str, i64)],
    instruction: &str,
) -> ChangeCoursePlanEntriesRequestDto {
    ChangeCoursePlanEntriesRequestDto {
        program_id: program.summary.id.clone(),
        outline_revision,
        lessons: lessons
            .iter()
            .map(|(lesson_id, expected)| CoursePlanEntryRefDto {
                lesson_id: (*lesson_id).to_owned(),
                expected_entry_revision: *expected,
            })
            .collect(),
        instruction: instruction.to_owned(),
    }
}

fn undo_request(
    program: &LearningProgramDto,
    outline_revision: i64,
    lesson_id: &str,
    expected: i64,
) -> UndoCoursePlanEntryRequestDto {
    UndoCoursePlanEntryRequestDto {
        program_id: program.summary.id.clone(),
        outline_revision,
        lesson_id: lesson_id.to_owned(),
        expected_entry_revision: expected,
    }
}

fn entry_of<'a>(shown: &'a CoursePlanViewDto, lesson_id: &str) -> &'a CoursePlanEntryDto {
    let found = shown
        .entries
        .iter()
        .find(|entry| entry.topics.lesson_id == lesson_id);
    assert!(found.is_some(), "{lesson_id} is in the plan: {shown:?}");
    found.unwrap()
}

fn the_planned_entry(lesson_id: &str) -> PlannedLesson {
    PlannedLesson {
        lesson_id: lesson_id.to_owned(),
        ideas: vec!["the central idea".into()],
        terms: vec!["a term".into()],
        actions: vec!["an action".into()],
        contrast: "a confusion".into(),
    }
}

#[tokio::test]
async fn an_edit_writes_history_and_bumps_the_entry_revision() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let wanted = topics_of(&order[1], "an idea the learner wrote");

    let shown = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, wanted.clone()),
    )
    .await
    .unwrap();

    let entry = entry_of(&shown, &order[1]);
    assert_eq!(entry.topics, wanted);
    assert_eq!(entry.entry_revision, 2);
    assert_eq!(entry.previous, Some(the_planned_entry(&order[1])));
    for other in [&order[0], &order[2], &order[3]] {
        let untouched = entry_of(&shown, other);
        assert_eq!(untouched.entry_revision, 1);
        assert_eq!(untouched.topics, the_planned_entry(other));
        assert_eq!(untouched.previous, None);
    }
    let saved = snapshot(&pool, &program.summary.id).await;
    assert_eq!(saved.history.len(), 1);
    let (lesson, outline, replaced, kept) = &saved.history[0];
    assert_eq!((lesson, *outline, *replaced), (&order[1], revision, 1));
    assert_eq!(
        serde_json::from_str::<PlannedLesson>(kept).unwrap(),
        the_planned_entry(&order[1])
    );
}

#[tokio::test]
async fn an_edit_keeps_the_learners_words_whole_and_lets_the_contrast_be_empty() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let long = "an idea that goes on and on ".repeat(2_000);
    let mut wanted = topics_of(&order[0], &long);
    wanted.contrast = String::new();

    let shown = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, wanted),
    )
    .await
    .unwrap();

    let entry = entry_of(&shown, &order[0]);
    assert_eq!(entry.topics.ideas, vec![long]);
    assert_eq!(entry.topics.contrast, "");
}

#[tokio::test]
async fn an_edit_that_does_not_apply_is_refused_and_writes_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let before = snapshot(&pool, &program.summary.id).await;
    let fine = topics_of(&order[1], "an idea");

    let stale = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 7, fine.clone()),
    )
    .await;
    assert!(
        matches!(&stale, Err(AppError::InvalidState(text)) if text.contains("changed")),
        "{stale:?}"
    );

    let mut no_ideas = fine.clone();
    no_ideas.ideas.clear();
    let empty = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, no_ideas),
    )
    .await;
    assert!(
        matches!(&empty, Err(AppError::InvalidInput(text)) if text.contains("at least one idea")),
        "{empty:?}"
    );

    for blank in 0..3 {
        let mut topics = fine.clone();
        match blank {
            0 => topics.ideas.push("   ".into()),
            1 => topics.terms = vec![String::new()],
            _ => topics.actions = vec!["\t".into()],
        }
        let refused = edit_entry(
            &Database::from(pool.clone()),
            edit_request(&program, revision, 1, topics),
        )
        .await;
        assert!(
            matches!(&refused, Err(AppError::InvalidInput(text)) if text.contains("blank")),
            "{blank}: {refused:?}"
        );
    }

    let stranger = topics_of("not-a-lesson", "an idea");
    let unknown = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, stranger),
    )
    .await;
    assert!(matches!(unknown, Err(AppError::NotFound(_))), "{unknown:?}");

    assert_eq!(snapshot(&pool, &program.summary.id).await, before);
}

#[tokio::test]
async fn an_edit_after_the_outline_changed_is_refused_and_writes_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    change_outline(&pool, &program.summary.id).await;
    let before = snapshot(&pool, &program.summary.id).await;

    let refused = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, topics_of(&order[1], "an idea")),
    )
    .await;

    assert!(
        matches!(&refused, Err(AppError::InvalidState(text)) if text.contains("outline changed")),
        "{refused:?}"
    );
    assert_eq!(snapshot(&pool, &program.summary.id).await, before);
}

#[tokio::test]
async fn an_edit_of_a_lesson_being_written_is_refused_and_writes_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    lesson_job(&pool, &program, &order[1], true).await;
    let before = snapshot(&pool, &program.summary.id).await;

    let refused = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, topics_of(&order[1], "an idea")),
    )
    .await;

    assert!(
        matches!(&refused, Err(AppError::InvalidState(text)) if text.contains("being written")),
        "{refused:?}"
    );
    assert_eq!(snapshot(&pool, &program.summary.id).await, before);

    let other = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, topics_of(&order[2], "an idea")),
    )
    .await;
    assert!(
        other.is_ok(),
        "the lessons nothing is writing stay open: {other:?}"
    );
}

#[tokio::test]
async fn an_edit_of_a_ready_lesson_is_refused() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(&order[3])
        .execute(&pool)
        .await
        .unwrap();

    let refused = edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, topics_of(&order[3], "an idea")),
    )
    .await;

    assert!(
        matches!(&refused, Err(AppError::InvalidState(text)) if text.contains("already written")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_change_replaces_only_the_selected_entries() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let model = Topics::new();

    let shown = change_entries(
        &Database::from(pool.clone()),
        &model,
        change_request(
            &program,
            revision,
            &[at(&order[1], 1), at(&order[2], 1)],
            "Move the terms to the next lesson",
        ),
    )
    .await
    .unwrap();

    for changed in [&order[1], &order[2]] {
        let entry = entry_of(&shown, changed);
        assert_eq!(entry.entry_revision, 2);
        assert_eq!(
            entry.topics.ideas,
            vec![format!("a changed idea of {changed}")]
        );
        assert_eq!(entry.previous, Some(the_planned_entry(changed)));
    }
    for kept in [&order[0], &order[3]] {
        let entry = entry_of(&shown, kept);
        assert_eq!(entry.entry_revision, 1);
        assert_eq!(entry.topics, the_planned_entry(kept));
        assert_eq!(entry.previous, None);
    }
    assert_eq!(snapshot(&pool, &program.summary.id).await.history.len(), 2);
}

#[tokio::test]
async fn a_change_is_asked_at_interactive_priority_under_a_schema_of_the_selected_ids() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let model = Topics::new();

    change_entries(
        &Database::from(pool.clone()),
        &model,
        change_request(
            &program,
            revision,
            &[at(&order[3], 1), at(&order[1], 1)],
            "Make these two shorter",
        ),
    )
    .await
    .unwrap();

    assert_eq!(model.calls(), 1, "the model is asked once");
    let sent = model.request(0);
    assert_eq!(sent.priority, InferencePriority::Interactive);
    let schema = sent.json_schema.unwrap();
    assert_eq!(
        schema["properties"]["lessons"]["items"]["properties"]["lessonId"]["enum"],
        json!([order[3], order[1]])
    );
    let task = model.task(0);
    assert_eq!(task["goal"], program.summary.goal.as_str());
    assert_eq!(task["priorKnowledge"], program.prior_knowledge.as_str());
    assert_eq!(task["learnersRequest"], "Make these two shorter");
    assert_eq!(task["lessonsToChange"], json!([order[3], order[1]]));
    let outline = task["modules"][0]["lessons"][0].clone();
    assert_eq!(outline["lessonId"], order[0].as_str());
    assert_eq!(outline["objective"], "Explain the central idea");
    let plan = task["plan"].as_array().unwrap();
    let planned: Vec<_> = plan.iter().map(|entry| entry["lessonId"].clone()).collect();
    assert_eq!(
        planned,
        order.iter().map(|id| json!(id)).collect::<Vec<_>>(),
        "the whole plan, in course order"
    );
    assert_eq!(plan[0]["ideas"][0], "the central idea");
}

#[tokio::test]
async fn a_gap_in_a_change_is_asked_for_again_with_the_lesson_it_left_out() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let model = Topics::answering(vec![change_answer(&[&order[1]], "a first idea")]);

    let shown = change_entries(
        &Database::from(pool.clone()),
        &model,
        change_request(
            &program,
            revision,
            &[at(&order[1], 1), at(&order[2], 1)],
            "Rework these",
        ),
    )
    .await
    .unwrap();

    assert_eq!(model.calls(), 2);
    let asked = model.request(1);
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
        said.contains(&order[2]) && said.contains("exactly once"),
        "{said}"
    );
    for changed in [&order[1], &order[2]] {
        assert_eq!(entry_of(&shown, changed).entry_revision, 2);
    }
}

#[tokio::test]
async fn a_change_that_cannot_apply_is_refused_before_the_model_is_asked() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    lesson_job(&pool, &program, &order[3], true).await;
    let before = snapshot(&pool, &program.summary.id).await;
    let model = Topics::new();
    let one = [(order[1].as_str(), 1)];

    let cases = [
        (
            "a blank request",
            change_request(&program, revision, &one, "  "),
        ),
        (
            "no lessons",
            change_request(&program, revision, &[], "Rework this"),
        ),
        (
            "a lesson twice",
            change_request(
                &program,
                revision,
                &[at(&order[1], 1), at(&order[1], 1)],
                "Rework this",
            ),
        ),
        (
            "a stale entry",
            change_request(&program, revision, &[at(&order[1], 4)], "Rework this"),
        ),
        (
            "a changed outline",
            change_request(&program, revision + 1, &one, "Rework this"),
        ),
        (
            "a lesson being written",
            change_request(&program, revision, &[at(&order[3], 1)], "Rework this"),
        ),
        (
            "a stranger",
            change_request(&program, revision, &[("not-a-lesson", 1)], "Rework this"),
        ),
    ];
    for (name, request) in cases {
        let refused = change_entries(&Database::from(pool.clone()), &model, request).await;
        assert!(refused.is_err(), "{name} is refused");
    }

    assert_eq!(
        model.calls(),
        0,
        "no model is asked for a request that cannot apply"
    );
    assert_eq!(snapshot(&pool, &program.summary.id).await, before);
}

#[tokio::test]
async fn a_change_whose_outline_moved_during_the_call_saves_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let before = snapshot(&pool, &program.summary.id).await;
    let model = Topics::new().meanwhile(
        &pool,
        "UPDATE learning_programs SET goal=goal||' (changed)' WHERE id=?",
        &program.summary.id,
    );

    let refused = change_entries(
        &Database::from(pool.clone()),
        &model,
        change_request(&program, revision, &[at(&order[1], 1)], "Rework this"),
    )
    .await;

    assert_eq!(model.calls(), 1);
    assert!(
        matches!(&refused, Err(AppError::InvalidState(text)) if text.contains("outline changed")),
        "{refused:?}"
    );
    assert_eq!(snapshot(&pool, &program.summary.id).await, before);
}

#[tokio::test]
async fn a_change_whose_entry_moved_during_the_call_saves_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let model = Topics::new().meanwhile(
        &pool,
        "UPDATE learning_course_plan_lessons SET entry_revision=entry_revision+1 WHERE program_id=?",
        &program.summary.id,
    );
    let bumped = {
        let mut expected = snapshot(&pool, &program.summary.id).await;
        for entry in &mut expected.entries {
            entry.3 += 1;
        }
        expected
    };

    let refused = change_entries(
        &Database::from(pool.clone()),
        &model,
        change_request(&program, revision, &[at(&order[1], 1)], "Rework this"),
    )
    .await;

    assert!(
        matches!(&refused, Err(AppError::InvalidState(text)) if text.contains("changed")),
        "{refused:?}"
    );
    assert_eq!(
        snapshot(&pool, &program.summary.id).await,
        bumped,
        "only the other writer's change is saved"
    );
}

#[tokio::test]
async fn undo_pops_the_history_one_entry_at_a_time() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    let first = topics_of(&order[1], "the first edit");
    let second = topics_of(&order[1], "the second edit");
    edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, first.clone()),
    )
    .await
    .unwrap();
    edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 2, second.clone()),
    )
    .await
    .unwrap();

    let once = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[1], 3),
    )
    .await
    .unwrap();
    let entry = entry_of(&once, &order[1]);
    assert_eq!(entry.topics, first);
    assert_eq!(entry.entry_revision, 4);
    assert_eq!(
        entry.previous,
        Some(the_planned_entry(&order[1])),
        "the next older entry is shown"
    );
    assert_eq!(snapshot(&pool, &program.summary.id).await.history.len(), 1);

    let twice = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[1], 4),
    )
    .await
    .unwrap();
    let entry = entry_of(&twice, &order[1]);
    assert_eq!(entry.topics, the_planned_entry(&order[1]));
    assert_eq!(entry.entry_revision, 5);
    assert_eq!(entry.previous, None);
    assert!(snapshot(&pool, &program.summary.id)
        .await
        .history
        .is_empty());

    let nothing = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[1], 5),
    )
    .await;
    assert!(
        matches!(&nothing, Err(AppError::InvalidState(text)) if text.contains("nothing to undo")),
        "{nothing:?}"
    );
}

#[tokio::test]
async fn an_undo_that_does_not_apply_is_refused_and_writes_nothing() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, revision) = planned_program(&pool, &Topics::new()).await;
    let order = ids(&program);
    edit_entry(
        &Database::from(pool.clone()),
        edit_request(&program, revision, 1, topics_of(&order[1], "an edit")),
    )
    .await
    .unwrap();
    let before = snapshot(&pool, &program.summary.id).await;

    let stale = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[1], 1),
    )
    .await;
    assert!(
        matches!(&stale, Err(AppError::InvalidState(text)) if text.contains("changed")),
        "{stale:?}"
    );

    let fresh = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[2], 1),
    )
    .await;
    assert!(
        matches!(&fresh, Err(AppError::InvalidState(text)) if text.contains("nothing to undo")),
        "{fresh:?}"
    );

    lesson_job(&pool, &program, &order[1], true).await;
    let locked = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[1], 2),
    )
    .await;
    assert!(
        matches!(&locked, Err(AppError::InvalidState(text)) if text.contains("being written")),
        "{locked:?}"
    );

    change_outline(&pool, &program.summary.id).await;
    let moved = undo_entry(
        &Database::from(pool.clone()),
        undo_request(&program, revision, &order[2], 1),
    )
    .await;
    assert!(
        matches!(&moved, Err(AppError::InvalidState(text)) if text.contains("outline changed")),
        "{moved:?}"
    );

    assert_eq!(snapshot(&pool, &program.summary.id).await, before);
}
