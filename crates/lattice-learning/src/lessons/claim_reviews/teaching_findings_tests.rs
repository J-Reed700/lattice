#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::{PlanCheck, ReviewFinding};
use serde_json::json;

async fn pool() -> SqlitePool {
    let pool = crate::tests::pool().await.unwrap();
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
        finding: ReviewFinding::of_kind(kind, site, note),
        repairs,
    }
}

fn with_summary(mut finding: SetAside, summary: &str) -> SetAside {
    finding.finding.summary = summary.into();
    finding
}

fn example_finding() -> SetAside {
    let mut finding = with_summary(
        set_aside(
            FindingKind::Teaching,
            FindingSite::Section(0),
            "Section 1 (candidate blocks-0): the example shows \"result\": \"4\" where the code returns the number 4.",
            2,
        ),
        "The example shows \"4\" in quotation marks where the code returns the number 4.",
    );
    finding.finding.passage = Some("The function returns the number 4.".into());
    finding
}

fn keys(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| (*key).to_owned()).collect()
}

async fn three_waiting(store: &TeachingFindingsRepository) {
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[
                example_finding(),
                set_aside(
                    FindingKind::Structure,
                    FindingSite::Section(1),
                    "Section 2 is too short.",
                    0,
                ),
                set_aside(
                    FindingKind::AnswerKey,
                    FindingSite::Question(0),
                    "Question 1 has a disputed key.",
                    1,
                ),
            ],
            true,
        )
        .await
        .unwrap();
}

async fn states(store: &TeachingFindingsRepository) -> Vec<(String, TeachingFindingStateDto)> {
    store
        .list("l1")
        .await
        .unwrap()
        .into_iter()
        .map(|finding| (finding.finding_key, finding.state))
        .collect()
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
        ["rule/schema/lesson", "reviewer/section-0", "key/question-0"]
    );
    assert_eq!(listed[0].unit_title, "Whole lesson");
    assert_eq!(listed[0].unit_index, None);
    assert_eq!(listed[1].unit_title, "Section 1: Sums");
    assert_eq!(
        listed[1].passage.as_deref(),
        Some("The function returns the number 4.")
    );
    assert_eq!(listed[1].repairs, 2);
    assert_eq!(
        listed[1].summary,
        "The example shows \"4\" in quotation marks where the code returns the number 4."
    );
    assert_eq!(listed[1].state, TeachingFindingStateDto::Waiting);
    // A question finding with no chosen passage shows the question.
    assert_eq!(listed[2].passage.as_deref(), Some("What is 2 + 2?"));
    assert_eq!(listed[2].site, TeachingFindingSiteDto::Question);
    assert_eq!(store.held("l1", &lesson()).await.unwrap().ids.len(), 3);
}

#[tokio::test]
async fn keep_as_written_holds_until_the_part_s_text_changes() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record("l1", "j1", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    let section = keys(&["reviewer/section-0"]);
    assert!(store
        .settle("p2", "l1", &section, TeachingSettleDto::Keep)
        .await
        .is_err());
    store
        .settle("p1", "l1", &section, TeachingSettleDto::Keep)
        .await
        .unwrap();
    // Not waiting any more: a second keep is refused.
    assert!(store
        .settle("p1", "l1", &section, TeachingSettleDto::Keep)
        .await
        .is_err());
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
    assert_eq!(store.held("l1", &elsewhere).await.unwrap().ids.len(), 1);
    // Once the section changes, the decision lapses: not held, and the
    // finding waits again if it is raised again.
    let mut changed = lesson();
    changed["blocks"][0]["body"] = json!("The function returns the string \"4\".");
    assert!(store.held("l1", &changed).await.unwrap().ids.is_empty());
    store
        .record("l1", "j2", &changed, &[example_finding()], true)
        .await
        .unwrap();
    assert_eq!(
        store.list("l1").await.unwrap()[0].state,
        TeachingFindingStateDto::Waiting
    );
}

fn plan_finding(subject: &str, note: &str) -> SetAside {
    SetAside {
        finding: ReviewFinding::plan(PlanCheck::Used, subject, FindingSite::Section(0), note),
        repairs: 0,
    }
}

/// A row recorded before finding ids: keyed by kind and place.
async fn place_row(store: &TeachingFindingsRepository, state: &str, sha: &str, note: &str) {
    sqlx::query(
        "INSERT INTO learning_lesson_teaching_findings (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, passage, repairs, state, job_id, found_at, kept_at)          VALUES ('l1', 'teaching:section-0', 'teaching', 'section', 0, 'T', ?, ?, NULL, 0, ?, NULL, 1, CASE WHEN ?='kept' THEN 5 END)",
    )
    .bind(sha)
    .bind(note)
    .bind(state)
    .bind(state)
    .execute(&store.pool)
    .await
    .unwrap();
}

async fn note_of(store: &TeachingFindingsRepository, key: &str) -> String {
    sqlx::query_scalar("SELECT note FROM learning_lesson_teaching_findings WHERE finding_key=?")
        .bind(key)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn two_findings_in_one_section_are_two_rows() {
    let store = TeachingFindingsRepository::new(pool().await);
    let first = plan_finding(
        "aaaa000000000001",
        "A first idea is used without being shown.",
    );
    let second = plan_finding(
        "aaaa000000000002",
        "A second idea is used without being shown.",
    );
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[first.clone(), second.clone()],
            true,
        )
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(
        listed
            .iter()
            .map(|f| f.finding_key.as_str())
            .collect::<Vec<_>>(),
        [first.finding.id().as_str(), second.finding.id().as_str()]
    );
}

#[tokio::test]
async fn a_row_stored_by_place_still_stands_for_its_place() {
    let store = TeachingFindingsRepository::new(pool().await);
    let sha = site_sha256(&lesson(), FindingSite::Section(0));
    place_row(&store, "kept", &sha, "The user's own note.").await;
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[set_aside(
                FindingKind::Teaching,
                FindingSite::Section(0),
                "A new note.",
                0,
            )],
            true,
        )
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].finding_key, "teaching:section-0");
    assert_eq!(listed[0].state, TeachingFindingStateDto::Kept);
    assert_eq!(
        note_of(&store, "teaching:section-0").await,
        "The user's own note."
    );
}

#[tokio::test]
async fn a_row_stored_by_place_gives_way_when_its_text_changes() {
    let store = TeachingFindingsRepository::new(pool().await);
    place_row(&store, "kept", "an earlier text", "The user's own note.").await;
    let finding = set_aside(
        FindingKind::Teaching,
        FindingSite::Section(0),
        "A new note.",
        0,
    );
    store
        .record("l1", "j1", &lesson(), std::slice::from_ref(&finding), true)
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].finding_key, finding.finding.id().as_str());
    assert_eq!(listed[0].state, TeachingFindingStateDto::Waiting);
}

#[tokio::test]
async fn a_finding_recorded_after_the_review_takes_over_a_lapsed_place_row() {
    let store = TeachingFindingsRepository::new(pool().await);
    place_row(&store, "waiting", "an earlier text", "An old note.").await;
    let finding = set_aside(
        FindingKind::Teaching,
        FindingSite::Section(0),
        "A new note.",
        0,
    );
    store
        .record("l1", "j1", &lesson(), std::slice::from_ref(&finding), false)
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].finding_key, finding.finding.id().as_str());
    assert_eq!(listed[0].state, TeachingFindingStateDto::Waiting);
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
            &[with_summary(
                set_aside(
                    FindingKind::Teaching,
                    FindingSite::Section(0),
                    "reworded",
                    0,
                ),
                "Reworded.",
            )],
            false,
        )
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed[0].finding_key, "reviewer/section-0");
    assert_eq!(
        (listed[0].summary.as_str(), listed[0].repairs),
        ("Reworded.", 2)
    );
    store
        .record("l1", "j1", &lesson(), &[rule], true)
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].finding_key, "rule/schema/section-1");
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

#[tokio::test]
async fn settling_keeps_many_findings_in_one_call() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0", "key/question-0"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap();
    assert_eq!(
        states(&store).await,
        [
            (
                "reviewer/section-0".to_owned(),
                TeachingFindingStateDto::Kept
            ),
            (
                "rule/schema/section-1".to_owned(),
                TeachingFindingStateDto::Waiting
            ),
            ("key/question-0".to_owned(), TeachingFindingStateDto::Kept),
        ]
    );
}

#[tokio::test]
async fn a_settle_with_one_stale_key_is_refused_and_writes_nothing() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap();
    let refused = store
        .settle(
            "p1",
            "l1",
            &keys(&["rule/schema/section-1", "reviewer/section-0"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("Some of these notes changed since you opened them. Reload the lesson."),
        "{refused}"
    );
    // A key that names no finding is stale too.
    assert!(store
        .settle(
            "p1",
            "l1",
            &keys(&["rule/schema/section-1", "reviewer/section-9"]),
            TeachingSettleDto::Keep,
        )
        .await
        .is_err());
    assert_eq!(
        states(&store).await,
        [
            (
                "reviewer/section-0".to_owned(),
                TeachingFindingStateDto::Kept
            ),
            (
                "rule/schema/section-1".to_owned(),
                TeachingFindingStateDto::Waiting
            ),
            (
                "key/question-0".to_owned(),
                TeachingFindingStateDto::Waiting
            ),
        ]
    );
}

#[tokio::test]
async fn a_settle_with_no_keys_or_a_lesson_of_another_course_is_refused() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    assert!(store
        .settle("p1", "l1", &[], TeachingSettleDto::Keep)
        .await
        .is_err());
    assert!(store
        .settle(
            "p2",
            "l1",
            &keys(&["reviewer/section-0"]),
            TeachingSettleDto::Keep
        )
        .await
        .is_err());
    assert!(states(&store)
        .await
        .iter()
        .all(|(_, state)| *state == TeachingFindingStateDto::Waiting));
}

#[tokio::test]
async fn reopening_returns_kept_findings_to_waiting_and_clears_when_they_were_kept() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    let both = keys(&["reviewer/section-0", "rule/schema/section-1"]);
    store
        .settle("p1", "l1", &both, TeachingSettleDto::Keep)
        .await
        .unwrap();
    store
        .settle("p1", "l1", &both, TeachingSettleDto::Reopen)
        .await
        .unwrap();
    assert!(states(&store)
        .await
        .iter()
        .all(|(_, state)| *state == TeachingFindingStateDto::Waiting));
    let kept_at: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_lesson_teaching_findings WHERE kept_at IS NOT NULL",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(kept_at, 0);
    // A waiting finding cannot be reopened.
    assert!(store
        .settle("p1", "l1", &both, TeachingSettleDto::Reopen)
        .await
        .is_err());
}

#[tokio::test]
async fn a_summary_recorded_with_the_finding_is_what_is_listed() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record("l1", "j1", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    let listed = store.list("l1").await.unwrap();
    assert_eq!(
        listed[0].summary,
        "The example shows \"4\" in quotation marks where the code returns the number 4."
    );
    // The next pass's wording replaces it.
    store
        .record(
            "l1",
            "j2",
            &lesson(),
            &[with_summary(example_finding(), "Now said differently.")],
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        store.list("l1").await.unwrap()[0].summary,
        "Now said differently."
    );
}

#[tokio::test]
async fn a_finding_recorded_with_no_summary_is_listed_with_the_plain_line_for_its_kind() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    let summaries: Vec<_> = store
        .list("l1")
        .await
        .unwrap()
        .into_iter()
        .map(|finding| (finding.finding_key, finding.summary))
        .collect();
    assert_eq!(
        summaries,
        [
            (
                "reviewer/section-0".to_owned(),
                "The example shows \"4\" in quotation marks where the code returns the number 4."
                    .to_owned()
            ),
            (
                "rule/schema/section-1".to_owned(),
                "Part of this lesson\u{2019}s shape did not pass its check.".to_owned()
            ),
            (
                "key/question-0".to_owned(),
                "A second, independent solution disagreed with the answer key.".to_owned()
            ),
        ]
    );
}

/// A row recorded before findings had a summary: the note and no summary.
async fn old_row(
    store: &TeachingFindingsRepository,
    (kind, site, index): (&str, &str, Option<i64>),
    note: &str,
    passage: Option<&str>,
) -> String {
    let key = format!("{kind}:{site}-{index:?}");
    sqlx::query(
        "INSERT INTO learning_lesson_teaching_findings (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, passage, repairs, state, job_id, found_at, kept_at) \
         VALUES ('l1', ?, ?, ?, ?, 'T', 'sha', ?, ?, 1, 'waiting', NULL, 1, NULL)",
    )
    .bind(&key)
    .bind(kind)
    .bind(site)
    .bind(index)
    .bind(note)
    .bind(passage)
    .execute(&store.pool)
    .await
    .unwrap();
    key
}

async fn summary_of(store: &TeachingFindingsRepository, key: &str) -> String {
    store
        .list("l1")
        .await
        .unwrap()
        .into_iter()
        .find(|finding| finding.finding_key == key)
        .unwrap()
        .summary
}

async fn dto_of(store: &TeachingFindingsRepository, key: &str) -> TeachingFindingDto {
    store
        .list("l1")
        .await
        .unwrap()
        .into_iter()
        .find(|finding| finding.finding_key == key)
        .unwrap()
}

async fn section_text(store: &TeachingFindingsRepository, ordinal: i64, body: &str) {
    sqlx::query(
        "INSERT INTO learning_blocks (lesson_id, ordinal, kind, title, body, source_ids_json) \
         VALUES ('l1', ?, 'explanation', 'Sums', ?, '[]')",
    )
    .bind(ordinal)
    .bind(body)
    .execute(&store.pool)
    .await
    .unwrap();
}

const LONG: &str = "The default timeout for every outgoing request in the client library is 30 seconds unless the caller sets another value.";

fn uncited(shown: &str) -> String {
    format!("Section 1 (candidate blocks-0) states a specific in \"{shown}\" without citing it. Cite it when a supplied excerpt states it: add an entry to the citations of /blocks/0 with the sentence, the excerpt's sourceIndex and an exact quote of the excerpt that contains the figure or wording. Write the general form only when no supplied excerpt does.")
}

#[tokio::test]
async fn an_old_row_for_a_missing_citation_gets_its_sentence_in_full() {
    let store = TeachingFindingsRepository::new(pool().await);
    let short = old_row(
        &store,
        ("quotation", "section", Some(0)),
        &uncited("The default timeout is 30 seconds."),
        None,
    )
    .await;
    assert_eq!(
        summary_of(&store, &short).await,
        "No source is cited for this sentence: \u{201c}The default timeout is 30 seconds.\u{201d}"
    );
    // The note cut the sentence at 90 characters. The stored passage is the
    // quote the section cites, not the sentence, so the sentence is left out.
    let cut: String = LONG.chars().take(90).collect();
    let long = old_row(
        &store,
        ("quotation", "section", Some(1)),
        &uncited(&cut),
        Some(&format!("An opening line. {LONG} A closing line.")),
    )
    .await;
    assert_eq!(
        summary_of(&store, &long).await,
        "No source is cited for this sentence."
    );
}

#[tokio::test]
async fn an_old_row_completes_a_cut_sentence_from_the_published_section() {
    let store = TeachingFindingsRepository::new(pool().await);
    sqlx::query(
        "INSERT INTO learning_blocks (lesson_id, ordinal, kind, title, body, source_ids_json) \
         VALUES ('l1', 0, 'explanation', 'Sums', ?, '[]')",
    )
    .bind(format!(
        "Requests are retried. {LONG} Nothing else applies."
    ))
    .execute(&store.pool)
    .await
    .unwrap();
    let cut: String = LONG.chars().take(90).collect();
    let key = old_row(
        &store,
        ("quotation", "section", Some(0)),
        &uncited(&cut),
        None,
    )
    .await;
    assert_eq!(
        summary_of(&store, &key).await,
        format!("No source is cited for this sentence: \u{201c}{LONG}\u{201d}")
    );
}

#[tokio::test]
async fn an_old_row_leaves_the_sentence_out_rather_than_cut_it() {
    let store = TeachingFindingsRepository::new(pool().await);
    let cut: String = LONG.chars().take(90).collect();
    // Two sentences start with the cut-off text, so which one is not known.
    let other = format!("{cut} and nothing else.");
    section_text(&store, 0, &format!("{LONG} {other}")).await;
    let ambiguous = old_row(
        &store,
        ("quotation", "section", Some(0)),
        &uncited(&cut),
        None,
    )
    .await;
    assert_eq!(
        summary_of(&store, &ambiguous).await,
        "No source is cited for this sentence."
    );
    // No section text holds the sentence; the stored passage does, but it is
    // the quote the section cites, so it is not used.
    let unknown = old_row(
        &store,
        ("quotation", "section", Some(1)),
        &uncited(&cut),
        Some(LONG),
    )
    .await;
    assert_eq!(
        summary_of(&store, &unknown).await,
        "No source is cited for this sentence."
    );
}

#[tokio::test]
async fn an_old_row_for_a_quote_that_lacks_a_figure_says_what_it_lacks() {
    let store = TeachingFindingsRepository::new(pool().await);
    let note = "Section 1 (candidate blocks-0): the quote cited for \"The default timeout is 30 seconds.\" does not contain 30. Cite a quote of a supplied excerpt that does, in an entry of the citations of /blocks/0, or write the general form when no excerpt states it.";
    let key = old_row(&store, ("quotation", "section", Some(0)), note, None).await;
    assert_eq!(
        summary_of(&store, &key).await,
        "The quotation cited for this sentence does not contain 30: \u{201c}The default timeout is 30 seconds.\u{201d}"
    );
}

#[tokio::test]
async fn an_old_row_for_a_sentence_citation_and_for_a_part_citation_is_rebuilt() {
    let store = TeachingFindingsRepository::new(pool().await);
    let cut: String = LONG.chars().take(80).collect();
    section_text(&store, 2, &format!("Before it. {LONG} After it.")).await;
    // The stored passage holds another sentence that starts with the cut
    // text, which must not make the sentence ambiguous.
    let misleading = format!("{cut} and nothing more.");
    let sentence = old_row(
        &store,
        ("quotation", "section", Some(2)),
        &format!("The source citation of the sentence \"{cut}\" could not be confirmed: The generated quotation does not match a valid passage in the saved reference. No material was published."),
        Some(&misleading),
    )
    .await;
    assert_eq!(
        summary_of(&store, &sentence).await,
        format!("The quotation cited for this sentence could not be found in its source: \u{201c}{LONG}\u{201d}")
    );
    let reason = "The generated quotation does not match a valid passage in the saved reference. No material was published.";
    let section = old_row(
        &store,
        ("quotation", "section", Some(3)),
        &format!("The source citation at blocks[3] could not be confirmed: {reason}"),
        None,
    )
    .await;
    let question = old_row(
        &store,
        ("quotation", "question", Some(1)),
        &format!("The source citation at questions[1] could not be confirmed: {reason}"),
        None,
    )
    .await;
    let lesson = old_row(
        &store,
        ("quotation", "lesson", None),
        &format!("The source citation at the lesson could not be confirmed: {reason}"),
        None,
    )
    .await;
    let mismatch = old_row(
        &store,
        ("quotation", "section", Some(4)),
        "The source quotation at blocks[4] does not match its saved reference: it was not found. Copy a relevant exact span from the cited source.",
        None,
    )
    .await;
    for (key, part) in [
        (section, "section 4"),
        (question, "question 2"),
        (lesson, "the lesson"),
        (mismatch, "section 5"),
    ] {
        assert_eq!(
            summary_of(&store, &key).await,
            format!("The quotation cited in {part} could not be found in its source."),
            "{key}"
        );
    }
}

#[tokio::test]
async fn an_old_row_for_unchecked_assertions_and_for_a_reviewer_s_critique_is_rebuilt() {
    let store = TeachingFindingsRepository::new(pool().await);
    let audit = old_row(
        &store,
        ("teaching", "section", Some(0)),
        "The claim audit found assertions here that were not extracted for checking, so no source has confirmed them: Water boils at 100 C; Ice floats",
        None,
    )
    .await;
    assert_eq!(
        summary_of(&store, &audit).await,
        "Some statements here were never checked against a source. The checker read them as: \u{201c}Water boils at 100 C\u{201d}, \u{201c}Ice floats\u{201d}."
    );
    let critique = old_row(
        &store,
        ("teaching", "section", Some(1)),
        "Section 2 (candidate blocks-1): The example shows 4 in quotation marks where the code returns the number 4.",
        None,
    )
    .await;
    assert_eq!(
        summary_of(&store, &critique).await,
        "The example shows 4 in quotation marks where the code returns the number 4."
    );
}

#[tokio::test]
async fn an_old_row_no_format_matches_gets_the_plain_line_for_its_kind() {
    let store = TeachingFindingsRepository::new(pool().await);
    let mut seen = Vec::new();
    for (kind, site, index) in [
        ("teaching", "lesson", None),
        ("answer_key", "question", Some(0)),
        ("structure", "section", Some(0)),
        ("quotation", "section", Some(1)),
    ] {
        let key = old_row(
            &store,
            (kind, site, index),
            "Fix the schema violation at /blocks/0/title: it is too long.",
            None,
        )
        .await;
        seen.push(summary_of(&store, &key).await);
    }
    assert_eq!(
        seen,
        [
            "The teaching review found a problem here that its repairs did not fix.",
            "A second, independent solution disagreed with the answer key.",
            "Part of this lesson\u{2019}s shape did not pass its check.",
            "A quotation here could not be matched to a saved source.",
        ]
    );
}

#[tokio::test]
async fn a_quotation_finding_lists_its_quote_as_a_quotation_and_the_rest_as_the_lesson() {
    let store = TeachingFindingsRepository::new(pool().await);
    let mut quote = with_summary(
        set_aside(
            FindingKind::Quotation,
            FindingSite::Section(1),
            "Section 2 cites a quote.",
            0,
        ),
        "The quotation cited in section 2 could not be found in its source.",
    );
    quote.finding.passage = Some("A quotation of the source.".into());
    let bare = with_summary(
        set_aside(
            FindingKind::Quotation,
            FindingSite::Question(0),
            "Question 1 cites nothing.",
            0,
        ),
        "The quotation cited in question 1 could not be found in its source.",
    );
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[example_finding(), quote, bare],
            true,
        )
        .await
        .unwrap();
    let teaching = dto_of(&store, "reviewer/section-0").await;
    assert_eq!(teaching.passage_role, TeachingPassageRoleDto::Lesson);
    let quote = dto_of(&store, "rule/quotation/section-1").await;
    assert_eq!(quote.passage.as_deref(), Some("A quotation of the source."));
    assert_eq!(quote.passage_role, TeachingPassageRoleDto::Quotation);
    // The part's own quote is not borrowed for a finding that cites none.
    let bare = dto_of(&store, "rule/quotation/question-0").await;
    assert_eq!(bare.passage, None);
    // A question's prompt is the lesson's text.
    let key = set_aside(
        FindingKind::AnswerKey,
        FindingSite::Question(0),
        "Question 1 has a disputed key.",
        1,
    );
    store
        .record("l1", "j2", &lesson(), &[key], false)
        .await
        .unwrap();
    let key = dto_of(&store, "key/question-0").await;
    assert_eq!(key.passage.as_deref(), Some("What is 2 + 2?"));
    assert_eq!(key.passage_role, TeachingPassageRoleDto::Lesson);
}

#[tokio::test]
async fn an_old_sentence_level_quotation_row_shows_no_passage_and_a_part_level_one_keeps_it() {
    let store = TeachingFindingsRepository::new(pool().await);
    let cut: String = LONG.chars().take(90).collect();
    let reason = "The generated quotation does not match a valid passage in the saved reference. No material was published.";
    let rows = [
        (
            ("quotation", "section", Some(0)),
            uncited("The default timeout is 30 seconds."),
        ),
        (
            ("quotation", "section", Some(1)),
            format!("Section 2 (candidate blocks-1): the quote cited for \"{cut}\" does not contain 30. Cite a quote of a supplied excerpt that does, in an entry of the citations of /blocks/1, or write the general form when no excerpt states it."),
        ),
        (
            ("quotation", "section", Some(2)),
            format!("The source citation of the sentence \"{cut}\" could not be confirmed: {reason}"),
        ),
        (
            ("quotation", "section", Some(3)),
            "Fix the schema violation at /blocks/3/quote: it is too long.".to_owned(),
        ),
    ];
    let mut sentence_level = Vec::new();
    for (place, note) in &rows {
        sentence_level
            .push(old_row(&store, *place, note, Some("The quote the section cites.")).await);
    }
    for key in &sentence_level {
        assert_eq!(dto_of(&store, key).await.passage, None, "{key}");
    }
    let part = old_row(
        &store,
        ("quotation", "section", Some(4)),
        &format!("The source citation at blocks[4] could not be confirmed: {reason}"),
        Some("The quote the section cites."),
    )
    .await;
    let part = dto_of(&store, &part).await;
    assert_eq!(
        part.passage.as_deref(),
        Some("The quote the section cites.")
    );
    assert_eq!(part.passage_role, TeachingPassageRoleDto::Quotation);
    let critique = old_row(
        &store,
        ("teaching", "section", Some(5)),
        "Section 6 (candidate blocks-5): The example is wrong.",
        Some("The sentence the reviewer chose."),
    )
    .await;
    let critique = dto_of(&store, &critique).await;
    assert_eq!(
        critique.passage.as_deref(),
        Some("The sentence the reviewer chose.")
    );
    assert_eq!(critique.passage_role, TeachingPassageRoleDto::Lesson);
}

fn with_sentence(mut finding: SetAside, sentence: &str) -> SetAside {
    finding.finding.sentence = Some(sentence.into());
    finding
}

#[tokio::test]
async fn a_sentence_recorded_with_a_finding_is_listed_and_the_next_pass_replaces_it() {
    let store = TeachingFindingsRepository::new(pool().await);
    let finding = |sentence: Option<&str>| {
        let mut finding = set_aside(
            FindingKind::Quotation,
            FindingSite::Section(0),
            "Section 1 cites nothing.",
            0,
        );
        finding.finding.sentence = sentence.map(str::to_owned);
        finding
    };
    store
        .record(
            "l1",
            "j1",
            &lesson(),
            &[finding(Some("The function returns the number 4."))],
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        dto_of(&store, "rule/quotation/section-0")
            .await
            .sentence
            .as_deref(),
        Some("The function returns the number 4.")
    );
    store
        .record(
            "l1",
            "j2",
            &lesson(),
            &[finding(Some("Another sentence."))],
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        dto_of(&store, "rule/quotation/section-0")
            .await
            .sentence
            .as_deref(),
        Some("Another sentence.")
    );
    store
        .record("l1", "j3", &lesson(), &[finding(None)], true)
        .await
        .unwrap();
    assert_eq!(
        dto_of(&store, "rule/quotation/section-0").await.sentence,
        None
    );
}

#[tokio::test]
async fn an_old_row_s_sentence_is_the_one_its_note_completes_and_none_otherwise() {
    let store = TeachingFindingsRepository::new(pool().await);
    let cut: String = LONG.chars().take(90).collect();
    let short = old_row(
        &store,
        ("quotation", "section", Some(0)),
        &uncited("The default timeout is 30 seconds."),
        None,
    )
    .await;
    section_text(&store, 1, &format!("Before it. {LONG} After it.")).await;
    let completed = old_row(
        &store,
        ("quotation", "section", Some(1)),
        &uncited(&cut),
        None,
    )
    .await;
    let unknown = old_row(
        &store,
        ("quotation", "section", Some(2)),
        &uncited(&cut),
        None,
    )
    .await;
    let cited_cut: String = LONG.chars().take(80).collect();
    section_text(&store, 5, &format!("Before it. {LONG} After it.")).await;
    let cited = old_row(
        &store,
        ("quotation", "section", Some(5)),
        &format!("The source citation of the sentence \"{cited_cut}\" could not be confirmed: it was not found."),
        Some("A quotation of the source."),
    )
    .await;
    let part = old_row(
        &store,
        ("quotation", "section", Some(3)),
        "The source citation at blocks[3] could not be confirmed: it was not found.",
        None,
    )
    .await;
    let critique = old_row(
        &store,
        ("teaching", "section", Some(4)),
        "Section 5 (candidate blocks-4): The example is wrong.",
        None,
    )
    .await;
    let sentence = |key: String| {
        let store = &store;
        async move { dto_of(store, &key).await.sentence }
    };
    assert_eq!(
        sentence(short).await.as_deref(),
        Some("The default timeout is 30 seconds.")
    );
    assert_eq!(sentence(completed).await.as_deref(), Some(LONG));
    assert_eq!(sentence(cited).await.as_deref(), Some(LONG));
    assert_eq!(sentence(unknown).await, None);
    assert_eq!(sentence(part).await, None);
    assert_eq!(sentence(critique).await, None);
}

async fn dto_states(store: &TeachingFindingsRepository) -> Vec<TeachingFindingStateDto> {
    states(store)
        .await
        .into_iter()
        .map(|(_, state)| state)
        .collect()
}

#[tokio::test]
async fn settling_source_added_resolves_a_waiting_note_with_that_resolution() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["rule/schema/section-1"]),
            TeachingSettleDto::SourceAdded,
        )
        .await
        .unwrap();
    assert_eq!(
        dto_states(&store).await,
        [
            TeachingFindingStateDto::Waiting,
            TeachingFindingStateDto::Resolved,
            TeachingFindingStateDto::Waiting
        ]
    );
    let added = dto_of(&store, "rule/schema/section-1").await;
    assert_eq!(added.resolution, Some(TeachingResolutionDto::SourceAdded));
    assert!(added.resolved_at.is_some());
    let waiting = dto_of(&store, "key/question-0").await;
    assert_eq!((waiting.resolution, waiting.resolved_at), (None, None));
    let kept_at: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_lesson_teaching_findings WHERE kept_at IS NOT NULL",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(kept_at, 0);
}

#[tokio::test]
async fn a_resolved_note_cannot_be_kept_or_resolved_again_and_one_stale_key_refuses_all() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0"]),
            TeachingSettleDto::SourceAdded,
        )
        .await
        .unwrap();
    for settle in [TeachingSettleDto::Keep, TeachingSettleDto::SourceAdded] {
        assert!(store
            .settle("p1", "l1", &keys(&["reviewer/section-0"]), settle)
            .await
            .is_err());
        let refused = store
            .settle(
                "p1",
                "l1",
                &keys(&["rule/schema/section-1", "reviewer/section-0"]),
                settle,
            )
            .await
            .unwrap_err();
        assert!(refused
            .to_string()
            .contains("Some of these notes changed since you opened them. Reload the lesson."));
    }
    assert_eq!(
        dto_states(&store).await,
        [
            TeachingFindingStateDto::Resolved,
            TeachingFindingStateDto::Waiting,
            TeachingFindingStateDto::Waiting
        ]
    );
}

#[tokio::test]
async fn reopening_takes_kept_and_resolved_notes_back_and_clears_the_resolution() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0"]),
            TeachingSettleDto::SourceAdded,
        )
        .await
        .unwrap();
    store
        .settle(
            "p1",
            "l1",
            &keys(&["rule/schema/section-1"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap();
    let resolved = dto_of(&store, "reviewer/section-0").await;
    assert_eq!(resolved.state, TeachingFindingStateDto::Resolved);
    assert_eq!(
        resolved.resolution,
        Some(TeachingResolutionDto::SourceAdded)
    );
    assert!(resolved.resolved_at.is_some());
    assert_eq!(
        dto_of(&store, "rule/schema/section-1").await.state,
        TeachingFindingStateDto::Kept
    );
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0", "rule/schema/section-1"]),
            TeachingSettleDto::Reopen,
        )
        .await
        .unwrap();
    assert!(dto_states(&store)
        .await
        .iter()
        .all(|state| *state == TeachingFindingStateDto::Waiting));
    let reopened = dto_of(&store, "reviewer/section-0").await;
    assert_eq!((reopened.resolution, reopened.resolved_at), (None, None));
}

#[tokio::test]
async fn a_resolved_note_stays_resolved_while_its_text_is_unchanged() {
    let store = TeachingFindingsRepository::new(pool().await);
    store
        .record("l1", "j1", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    store
        .settle(
            "p1",
            "l1",
            &keys(&["reviewer/section-0"]),
            TeachingSettleDto::SourceAdded,
        )
        .await
        .unwrap();
    store
        .record("l1", "j2", &lesson(), &[example_finding()], true)
        .await
        .unwrap();
    let same = dto_of(&store, "reviewer/section-0").await;
    assert_eq!(same.state, TeachingFindingStateDto::Resolved);
    assert_eq!(same.resolution, Some(TeachingResolutionDto::SourceAdded));
    let mut changed = lesson();
    changed["blocks"][0]["body"] = json!("The function returns the string \"4\".");
    store
        .record(
            "l1",
            "j3",
            &changed,
            &[with_sentence(example_finding(), "x")],
            true,
        )
        .await
        .unwrap();
    let again = dto_of(&store, "reviewer/section-0").await;
    assert_eq!(again.state, TeachingFindingStateDto::Waiting);
    assert_eq!((again.resolution, again.resolved_at), (None, None));
}
