#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::ReviewFinding;
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
        finding: ReviewFinding::new(kind, Some(site), note),
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
    assert_eq!(
        listed[1].summary,
        "The example shows \"4\" in quotation marks where the code returns the number 4."
    );
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
    let section = keys(&["teaching:section-0"]);
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
    assert_eq!(listed[0].finding_key, "teaching:section-0");
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

#[tokio::test]
async fn settling_keeps_many_findings_in_one_call() {
    let store = TeachingFindingsRepository::new(pool().await);
    three_waiting(&store).await;
    store
        .settle(
            "p1",
            "l1",
            &keys(&["teaching:section-0", "answer_key:question-0"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap();
    assert_eq!(
        states(&store).await,
        [
            (
                "teaching:section-0".to_owned(),
                TeachingFindingStateDto::Kept
            ),
            (
                "structure:section-1".to_owned(),
                TeachingFindingStateDto::Waiting
            ),
            (
                "answer_key:question-0".to_owned(),
                TeachingFindingStateDto::Kept
            ),
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
            &keys(&["teaching:section-0"]),
            TeachingSettleDto::Keep,
        )
        .await
        .unwrap();
    let refused = store
        .settle(
            "p1",
            "l1",
            &keys(&["structure:section-1", "teaching:section-0"]),
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
            &keys(&["structure:section-1", "teaching:section-9"]),
            TeachingSettleDto::Keep,
        )
        .await
        .is_err());
    assert_eq!(
        states(&store).await,
        [
            (
                "teaching:section-0".to_owned(),
                TeachingFindingStateDto::Kept
            ),
            (
                "structure:section-1".to_owned(),
                TeachingFindingStateDto::Waiting
            ),
            (
                "answer_key:question-0".to_owned(),
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
            &keys(&["teaching:section-0"]),
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
    let both = keys(&["teaching:section-0", "structure:section-1"]);
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
                "teaching:section-0".to_owned(),
                "The example shows \"4\" in quotation marks where the code returns the number 4."
                    .to_owned()
            ),
            (
                "structure:section-1".to_owned(),
                "Part of this lesson\u{2019}s shape did not pass its check.".to_owned()
            ),
            (
                "answer_key:question-0".to_owned(),
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
    // The note cut the sentence at 90 characters; the passage holds all of it.
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
        format!("No source is cited for this sentence: \u{201c}{LONG}\u{201d}")
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
    let ambiguous = old_row(
        &store,
        ("quotation", "section", Some(0)),
        &uncited(&cut),
        Some(&format!("{LONG} {other}")),
    )
    .await;
    assert_eq!(
        summary_of(&store, &ambiguous).await,
        "No source is cited for this sentence."
    );
    // No text holds the sentence at all.
    let unknown = old_row(
        &store,
        ("quotation", "section", Some(1)),
        &uncited(&cut),
        None,
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
    let sentence = old_row(
        &store,
        ("quotation", "section", Some(2)),
        &format!("The source citation of the sentence \"{cut}\" could not be confirmed: The generated quotation does not match a valid passage in the saved reference. No material was published."),
        Some(LONG),
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
        "These statements were never checked against a source: \u{201c}Water boils at 100 C\u{201d}, \u{201c}Ice floats\u{201d}."
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
