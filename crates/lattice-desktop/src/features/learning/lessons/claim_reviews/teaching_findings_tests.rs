#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::review_findings::ReviewFinding;
use serde_json::json;

async fn pool() -> SqlitePool {
    let pool = crate::features::learning::tests::pool().await.unwrap();
    for statement in [
        "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES ('p1', 'Arithmetic', 'Add numbers', 'active', 1, 'none', 30, 'qwen3.5-27b', 1000)",
        "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json, prerequisite_ids_json, project_json) \
         VALUES ('m1', 'p1', 1, 'Sums', 'Summary', '[]', '[]', '{}')",
        "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed, curriculum_state) \
         VALUES ('l1', 'p1', 'm1', 1, 'Adding', 'Add two numbers', 20, 'outline', 0, 'ready')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    pool
}

fn lesson() -> Value {
    json!({"blocks":[
        {"title":"Sums","body":"The function returns the number 4.","quote":""},
        {"title":"Practice","body":"Add 2 and 2.","quote":""}
    ],"questions":[{"prompt":"What is 2 + 2?","options":["3","4","5","6"]}]})
}

fn set_aside(kind: FindingKind, site: FindingSite, note: &str, repairs: u8) -> SetAside {
    SetAside {
        finding: ReviewFinding::new(kind, Some(site), note),
        repairs,
    }
}

fn example_finding() -> SetAside {
    let mut finding = set_aside(
        FindingKind::Teaching,
        FindingSite::Section(0),
        "Section 1 (candidate blocks-0): the example shows \"result\": \"4\" where the code returns the number 4.",
        2,
    );
    finding.finding.passage = Some("The function returns the number 4.".into());
    finding
}

#[tokio::test]
async fn a_set_aside_finding_waits_with_its_note_and_the_part_it_is_about() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[
                example_finding(),
                set_aside(
                    FindingKind::AnswerKey,
                    FindingSite::Question(0),
                    "Question 1 has a disputed key.",
                    1,
                ),
                set_aside(
                    FindingKind::Structure,
                    FindingSite::Lesson,
                    "The lesson needs a recap.",
                    0,
                ),
            ],
            true,
        )
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|f| f.finding_key.as_str())
            .collect::<Vec<_>>(),
        [
            "structure:lesson",
            "teaching:section-0",
            "answer_key:question-0"
        ]
    );
    assert_eq!(listed[0].unit_title, "Whole lesson");
    assert_eq!(listed[0].unit_index, None);
    assert_eq!(listed[1].unit_title, "Section 1: Sums");
    assert_eq!(
        listed[1].passage.as_deref(),
        Some("The function returns the number 4.")
    );
    assert_eq!(listed[1].repairs, 2);
    assert_eq!(listed[1].state, TeachingFindingStateDto::Waiting);
    // A question finding with no chosen passage shows the question.
    assert_eq!(listed[2].passage.as_deref(), Some("What is 2 + 2?"));
    assert_eq!(listed[2].site, TeachingFindingSiteDto::Question);
    assert_eq!(store.held("l1", &lesson()).await.unwrap().len(), 3);
}

#[tokio::test]
async fn keep_as_written_holds_until_the_part_s_text_changes() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record("l1", "j1", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    assert!(store.keep("p2", "l1", "teaching:section-0").await.is_err());
    store.keep("p1", "l1", "teaching:section-0").await.unwrap();
    // Not waiting any more: a second keep is refused.
    assert!(store.keep("p1", "l1", "teaching:section-0").await.is_err());
    // The same finding on unchanged text stays kept, and is still held.
    store
        .record("l1", "j2", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    assert_eq!(
        store.list("l1").await.unwrap()[0].state,
        TeachingFindingStateDto::Kept
    );
    // An edit elsewhere leaves the decision alone.
    let mut elsewhere = lesson();
    elsewhere["blocks"][1]["body"] = json!("Add 3 and 3.");
    assert_eq!(store.held("l1", &elsewhere).await.unwrap().len(), 1);
    // Once the section changes, the decision lapses: not held, and the
    // finding waits again if it is raised again.
    let mut changed = lesson();
    changed["blocks"][0]["body"] = json!("The function returns the string \"4\".");
    assert!(store.held("l1", &changed).await.unwrap().is_empty());
    store
        .record("l1", "j2", &changed, &[example_finding()], true)
        .await
        .unwrap();
    assert_eq!(
        store.list("l1").await.unwrap()[0].state,
        TeachingFindingStateDto::Waiting
    );
}

#[tokio::test]
async fn a_review_that_no_longer_raises_a_finding_removes_it_and_lesson_rules_only_add() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record("l1", "j1", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    let rule = set_aside(
        FindingKind::Structure,
        FindingSite::Section(1),
        "Section 2 is too short.",
        0,
    );
    store
        .record("l1", "j1", &lesson(), std::slice::from_ref(&rule), false)
        .await
        .unwrap();
    assert_eq!(store.list("l1").await.unwrap().len(), 2);
    // Repairs already counted are not lost to a later record.
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[set_aside(
                FindingKind::Teaching,
                FindingSite::Section(0),
                "reworded",
                0,
            )],
            false,
        )
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed[0].finding_key, "teaching:section-0");
    assert_eq!(
        (listed[0].note.as_str(), listed[0].repairs),
        ("reworded", 2)
    );
    store
        .record("l1", "j1", &lesson(), &[rule], true)
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].finding_key, "structure:section-1");
    store
        .record("l1", "j1", &lesson(), &[], true)
        .await
        .unwrap();
    assert!(store.list("l1").await.unwrap().is_empty());
}

#[tokio::test]
async fn the_table_checks_its_states_and_goes_with_its_lesson() {
    let pool = pool().await;
    let insert = |site: &str, index: Option<i64>, state: &str, kept_at: Option<i64>| {
        sqlx::query(
            "INSERT INTO learning_lesson_teaching_findings (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, passage, repairs, state, job_id, found_at, kept_at) \
             VALUES ('l1', ?, 'teaching', ?, ?, 'T', 'sha', 'note', NULL, 0, ?, NULL, 1, ?)",
        )
        .bind(format!("teaching:{site}-{index:?}-{state}"))
        .bind(site.to_owned())
        .bind(index)
        .bind(state.to_owned())
        .bind(kept_at)
    };
    assert!(insert("lesson", Some(0), "waiting", None)
        .execute(&pool)
        .await
        .is_err());
    assert!(insert("section", None, "waiting", None)
        .execute(&pool)
        .await
        .is_err());
    assert!(insert("section", Some(1), "kept", None)
        .execute(&pool)
        .await
        .is_err());
    assert!(insert("section", Some(1), "waiting", Some(5))
        .execute(&pool)
        .await
        .is_err());
    insert("section", Some(1), "kept", Some(5))
        .execute(&pool)
        .await
        .unwrap();
    insert("lesson", None, "waiting", None)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM learning_lessons WHERE id='l1'")
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_lesson_teaching_findings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
