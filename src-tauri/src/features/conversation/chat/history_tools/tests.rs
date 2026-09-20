//! What the history recovery path guarantees.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §9, §16.5.
//!
//! These run against real temporary SQLite databases built from the production
//! migration, because the parts most worth testing live in the schema: the
//! shared FTS index that also holds titles and bookmark notes, the ownership
//! column that keeps two conversations apart, and the trigger that fills the
//! index at all. A fake port would assert that this module calls itself.

use super::*;
use crate::domain::conversation_memory::{compute_digest, SourceRole};
use crate::features::conversation::repository::ConversationRepository;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn seed_conversation(pool: &SqlitePool, id: &str, title: &str) {
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES (?, ?, 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
    )
    .bind(id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a message the way production does — allocated sequence, real digest —
/// so these fixtures are valid evidence sources and the FTS trigger sees them.
async fn seed_message(
    pool: &SqlitePool,
    conversation_id: &str,
    id: &str,
    role: &str,
    content: &str,
    status: &str,
) -> i64 {
    let sequence: i64 =
        sqlx::query_scalar("SELECT next_message_sequence FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO conversation_messages \
            (id, conversation_id, role, content, tokens, created_at, metadata, status, \
             sequence, content_digest) \
         VALUES (?, ?, ?, ?, 10, '2026-09-01T10:00:00Z', NULL, ?, ?, ?)",
    )
    .bind(id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(status)
    .bind(sequence)
    .bind(compute_digest(content))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE conversations SET next_message_sequence = ? WHERE id = ?")
        .bind(sequence + 1)
        .bind(conversation_id)
        .execute(pool)
        .await
        .unwrap();
    sequence
}

async fn seed_bookmark(pool: &SqlitePool, conversation_id: &str, message_id: &str, note: &str) {
    sqlx::query(
        "INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id, title, note) \
         VALUES (?, ?, ?, 'saved', ?)",
    )
    .bind(format!("bm-{message_id}"))
    .bind(conversation_id)
    .bind(message_id)
    .bind(note)
    .execute(pool)
    .await
    .unwrap();
}

fn scope(conversation_id: &str) -> HistoryToolScope {
    HistoryToolScope::new(conversation_id, HistoryToolBudget::default())
}

fn search_call(arguments: serde_json::Value) -> FunctionCall {
    FunctionCall::new("call-1", TOOL_SEARCH_CONVERSATION_HISTORY, arguments)
}

fn read_call(arguments: serde_json::Value) -> FunctionCall {
    FunctionCall::new("call-2", TOOL_READ_CONVERSATION_HISTORY, arguments)
}

fn data(result: &FunctionResult) -> &serde_json::Value {
    result.data.as_ref().expect("a successful tool result")
}

fn matched_ids(result: &FunctionResult) -> Vec<String> {
    data(result)["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .map(|m| m["message_id"].as_str().unwrap().to_string())
        .collect()
}

/// A repository is the real `ConversationMemoryReadPort`; the tools only ever
/// see the trait.
fn port(pool: &SqlitePool) -> ConversationRepository {
    ConversationRepository::new(pool.clone())
}

// ---------------------------------------------------------------------------
// Scope: the decisive guarantee
// ---------------------------------------------------------------------------

/// The one that matters. Whatever the model puts in the arguments — a
/// conversation id, a space id, ids belonging to another thread — the reads stay
/// inside the conversation the turn is executing in.
#[tokio::test]
async fn a_tool_call_cannot_reach_another_conversation_by_changing_its_arguments() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Mine").await;
    seed_conversation(&pool, "conv-theirs", "Theirs").await;
    seed_message(
        &pool,
        "conv-mine",
        "mine-1",
        "user",
        "my own importer notes",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-theirs",
        "theirs-1",
        "user",
        "the passphrase is hunter2 pomegranate",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();

    // Naming the other conversation is refused outright, not silently ignored.
    let refused = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({
            "query": "pomegranate",
            "conversation_id": "conv-theirs",
        })),
    )
    .await
    .unwrap();
    assert!(!refused.success);
    assert_eq!(
        refused.error_code.as_deref(),
        Some("SCOPE_IS_NOT_AN_ARGUMENT")
    );

    // And without the argument, the other conversation's text is simply not
    // there to be found.
    let searched = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "pomegranate hunter2"})),
    )
    .await
    .unwrap();
    assert!(matched_ids(&searched).is_empty());

    // Nor by naming its message ids directly.
    let read = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["theirs-1"]})),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(data(&read)).unwrap();
    assert!(
        !body.contains("hunter2"),
        "another conversation's text reached the model: {body}"
    );
    assert_eq!(
        data(&read)["not_readable_message_ids"],
        serde_json::json!(["theirs-1"])
    );

    // Nor by a sequence interval, which is numbered per conversation.
    let read = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"from_sequence": 0, "to_sequence": 9999})),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(data(&read)).unwrap();
    assert!(body.contains("my own importer notes"));
    assert!(!body.contains("hunter2"), "sequence read crossed a thread");
}

#[test]
fn neither_tool_schema_offers_a_conversation_argument() {
    for definition in history_tool_definitions() {
        let properties = definition.parameters["properties"]
            .as_object()
            .expect("an object schema");
        for key in SCOPE_ARGUMENT_KEYS {
            assert!(
                !properties.contains_key(key),
                "{} exposes {key}",
                definition.name
            );
        }
        // Declared to the model as well as enforced at runtime.
        assert_eq!(definition.parameters["additionalProperties"], false);
    }
}

// ---------------------------------------------------------------------------
// The shared index
// ---------------------------------------------------------------------------

/// `conversation_search_fts` also indexes conversation titles and bookmark
/// notes, and a bookmark row carries a real message id, so it would join
/// straight through to a message. Returning either as a source would attribute
/// the app's own text, or a private note, to the user.
#[tokio::test]
async fn title_and_bookmark_rows_never_come_back_as_message_sources() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Pomegranate planning").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "plain message text",
        "completed",
    )
    .await;
    seed_bookmark(&pool, "conv-mine", "m1", "zeppelin reminder").await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();

    for term in ["Pomegranate", "zeppelin"] {
        let result = execute(
            &port,
            &scope,
            &mut memo,
            search_call(serde_json::json!({"query": term})),
        )
        .await
        .unwrap();
        assert!(
            matched_ids(&result).is_empty(),
            "'{term}' came back as a message source: {:?}",
            data(&result)
        );
    }
}

// ---------------------------------------------------------------------------
// What retrieval finds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exact_identifiers_and_non_latin_queries_return_their_passages() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "path",
        "user",
        "the importer lives in src-tauri/src/features/importer.rs",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "version",
        "user",
        "pin the runtime at v2.14.3 exactly",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "symbol",
        "user",
        "call AuthToken::refresh before retrying",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "cyrillic",
        "user",
        "Бюджет составляет сто двадцать тысяч",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "japanese",
        "user",
        "予算は十二万円までにしてください",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");

    for (query, expected) in [
        ("src-tauri/src/features/importer.rs", "path"),
        ("v2.14.3", "version"),
        ("AuthToken::refresh", "symbol"),
        ("Бюджет", "cyrillic"),
        // The FTS tokenizer cannot segment Japanese, so this passage is only
        // reachable through the exact-substring probe.
        ("予算", "japanese"),
    ] {
        let mut memo = HistoryToolMemo::default();
        let result = execute(
            &port,
            &scope,
            &mut memo,
            search_call(serde_json::json!({"query": query})),
        )
        .await
        .unwrap();
        assert!(
            matched_ids(&result).contains(&expected.to_string()),
            "'{query}' did not find {expected}: {:?}",
            data(&result)
        );
    }
}

/// Case matters for a name: the exact probe must not fold `AuthToken` into
/// `authtoken`, because they are different symbols.
#[tokio::test]
async fn an_exact_identifier_is_reported_as_an_exact_match_not_a_ranked_guess() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "symbol",
        "user",
        "call AuthToken::refresh before retrying",
        "completed",
    )
    .await;
    let mut memo = HistoryToolMemo::default();
    let result = execute(
        &port(&pool),
        &scope("conv-mine"),
        &mut memo,
        search_call(serde_json::json!({"query": "AuthToken::refresh"})),
    )
    .await
    .unwrap();
    assert_eq!(
        data(&result)["matches"][0]["matched_by"],
        "exact_identifier"
    );
}

/// "yes, option B" is not an answer on its own. The group around it is.
#[tokio::test]
async fn adjacent_turn_expansion_explains_a_short_reply_and_stays_inside_budget() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "q",
        "assistant",
        "Should the importer use option A (batch) or option B (streaming)?",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "a",
        "user",
        "yes, option B",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "ack",
        "assistant",
        "Streaming it is.",
        "completed",
    )
    .await;

    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "which option did we settle on",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();

    let ids: Vec<&str> = recalled
        .groups
        .iter()
        .flat_map(|group| group.passages.iter().map(|p| p.message_id.as_str()))
        .collect();
    assert!(
        ids.contains(&"a") && ids.contains(&"q"),
        "the short reply came back without the question that explains it: {ids:?}"
    );
    let spent: usize = recalled
        .passages()
        .iter()
        .map(|passage| passage.text.len())
        .sum();
    assert!(
        spent <= 4096,
        "the packed group overran its budget: {spent}"
    );
    assert_eq!(recalled.availability(), RecallAvailability::Selected);
    // Chronological inside the group, so the exchange reads in order.
    for group in &recalled.groups {
        let sequences: Vec<i64> = group.passages.iter().map(|p| p.sequence).collect();
        let mut sorted = sequences.clone();
        sorted.sort();
        assert_eq!(sequences, sorted);
    }
}

/// §9.1 step 8. A passage the prompt already carries is named, not repeated:
/// the link survives and the bytes are not spent twice.
#[tokio::test]
async fn a_passage_already_in_the_prompt_is_referenced_rather_than_repeated() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "old",
        "user",
        "the deploy freeze lasts until Friday",
        "completed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "next",
        "assistant",
        "Noted, nothing ships before then.",
        "completed",
    )
    .await;

    let already = ["User: the deploy freeze lasts until Friday".to_string()];
    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "deploy freeze",
            recent_turns: &already,
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();

    let texts: Vec<&str> = recalled
        .groups
        .iter()
        .flat_map(|group| group.passages.iter().map(|p| p.text.as_str()))
        .collect();
    assert!(
        !texts.iter().any(|text| text.contains("lasts until Friday")),
        "a span already in the prompt was sent again: {texts:?}"
    );
    assert!(recalled
        .groups
        .iter()
        .any(|group| group.already_in_context.contains(&"old".to_string())));
}

/// A group whose every message is already in the prompt earns nothing and is
/// dropped rather than emitted empty.
#[tokio::test]
async fn a_group_with_nothing_new_to_add_is_dropped() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "only",
        "user",
        "the deploy freeze lasts until Friday",
        "completed",
    )
    .await;

    let already = ["the deploy freeze lasts until Friday".to_string()];
    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "deploy freeze",
            recent_turns: &already,
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();

    assert!(recalled.groups.is_empty());
    // Nothing selected, but retrieval did run and did match: that is not the
    // same as finding nothing at all.
    assert!(recalled.diagnostics.candidates_considered > 0);
}

// ---------------------------------------------------------------------------
// Three different kinds of nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_empty_query_a_miss_and_an_unavailable_index_are_three_different_answers() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "notes about the importer",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");

    // 1. Nothing was asked, so nothing was established.
    let mut memo = HistoryToolMemo::default();
    let empty = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "   "})),
    )
    .await
    .unwrap();
    assert!(!empty.success);
    assert_eq!(empty.error_code.as_deref(), Some("EMPTY_QUERY"));

    // 2. Asked and not found.
    let miss = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "helicopter"})),
    )
    .await
    .unwrap();
    assert!(miss.success);
    assert_eq!(
        data(&miss)["retrieval"]["availability"],
        "searched_no_match"
    );
    assert!(data(&miss)["retrieval"]["index_error"].is_null());

    // 3. Could not ask at all. A fresh memo, because the point here is what the
    // index says, not what this turn already asked.
    sqlx::query("DROP TABLE conversation_search_fts")
        .execute(&pool)
        .await
        .unwrap();
    let mut memo = HistoryToolMemo::default();
    let broken = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "helicopter"})),
    )
    .await
    .unwrap();
    assert!(broken.success, "a dead index must not fail the turn");
    assert_eq!(data(&broken)["retrieval"]["availability"], "unavailable");
    assert_eq!(
        data(&broken)["retrieval"]["index_error"],
        "index_unavailable"
    );
}

#[tokio::test]
async fn automatic_retrieval_separates_a_miss_from_an_unavailable_index() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "notes about the importer",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let request = |input: &'static str| RecallRequest {
        user_input: input,
        recent_turns: &[],
        active_memory: &[],
        max_bytes: 4096,
    };

    let miss = recall_for_turn(&port, &scope, &request("helicopter"))
        .await
        .unwrap();
    assert_eq!(miss.availability(), RecallAvailability::NotFoundByRetrieval);
    assert!(miss.diagnostics.is_clean_miss());

    sqlx::query("DROP TABLE conversation_search_fts")
        .execute(&pool)
        .await
        .unwrap();
    let broken = recall_for_turn(&port, &scope, &request("helicopter"))
        .await
        .unwrap();
    assert_eq!(broken.availability(), RecallAvailability::Unavailable);
    assert!(!broken.diagnostics.is_clean_miss());
    assert_ne!(
        RecallAvailability::NotFoundByRetrieval.note(),
        RecallAvailability::Unavailable.note()
    );
}

// ---------------------------------------------------------------------------
// Budget and repetition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_repeated_read_of_an_exhausted_range_returns_a_reference_not_the_text() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "keep the importer under a hundred megabytes",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();

    let first = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    assert!(serde_json::to_string(data(&first))
        .unwrap()
        .contains("hundred megabytes"));
    assert_eq!(data(&first)["truncated"], false);

    let again = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    assert_eq!(data(&again)["already_delivered"], true);
    assert!(
        !serde_json::to_string(data(&again))
            .unwrap()
            .contains("hundred megabytes"),
        "the same text was sent twice"
    );
    assert_eq!(
        data(&again)["message_ids"],
        serde_json::json!(["m1"]),
        "the reference must still name what the model already has"
    );
}

/// A read cut by the budget resumes where it stopped. The alternative — sending
/// the same prefix again — spends a generation round to make no progress.
#[tokio::test]
async fn a_truncated_read_resumes_instead_of_resending_its_prefix() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    let long = "A".repeat(2000) + "TAIL";
    seed_message(&pool, "conv-mine", "m1", "user", &long, "completed").await;
    let port = port(&pool);
    let scope = HistoryToolScope::new(
        "conv-mine",
        HistoryToolBudget {
            max_response_bytes: 1024,
            deadline: None,
        },
    );
    let mut memo = HistoryToolMemo::default();

    let first = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    assert_eq!(data(&first)["truncated"], true);
    let cursor = data(&first)["continuation"]["start_byte"].as_u64().unwrap();
    assert!(cursor > 0);
    assert_eq!(data(&first)["messages"][0]["start_byte"], 0);
    assert_eq!(data(&first)["messages"][0]["is_whole_message"], false);

    let second = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    assert_eq!(
        data(&second)["messages"][0]["start_byte"].as_u64(),
        Some(cursor),
        "the repeat resent text the model already had"
    );
}

/// Byte offsets are what evidence spans are made of, so a cut must land on a
/// character boundary or the offsets it reports are unusable.
#[tokio::test]
async fn a_read_cut_inside_multibyte_text_stays_on_a_character_boundary() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    let text = "é".repeat(800);
    seed_message(&pool, "conv-mine", "m1", "user", &text, "completed").await;
    let scope = HistoryToolScope::new(
        "conv-mine",
        HistoryToolBudget {
            max_response_bytes: 1001,
            deadline: None,
        },
    );
    let mut memo = HistoryToolMemo::default();

    let result = execute(
        &port(&pool),
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    let end = data(&result)["messages"][0]["end_byte"].as_u64().unwrap();
    assert_eq!(end % 2, 0, "cut inside a two-byte character");
    assert!(text.is_char_boundary(end as usize));
}

/// §9.3: the tools share the turn's budget. Per-call ceilings alone let twenty
/// separately-affordable reads add up to the whole window.
#[tokio::test]
async fn history_reads_stop_when_the_turn_has_spent_its_share() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "importer notes",
        "completed",
    )
    .await;
    let mut memo = HistoryToolMemo {
        bytes_delivered: MAX_TURN_RESPONSE_BYTES,
        ..Default::default()
    };

    let port = port(&pool);
    let scope = scope("conv-mine");
    let result = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    assert!(!result.success);
    assert_eq!(result.error_code.as_deref(), Some("TURN_BUDGET_EXHAUSTED"));

    // A search too, and it must say so rather than report an empty result as a
    // miss: "no room to answer" is not "searched and found nothing".
    let searched = execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "importer"})),
    )
    .await
    .unwrap();
    assert_eq!(
        searched.error_code.as_deref(),
        Some("TURN_BUDGET_EXHAUSTED")
    );
}

#[tokio::test]
async fn an_expired_turn_deadline_stops_a_history_tool_before_it_reads() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "importer notes",
        "completed",
    )
    .await;
    let scope = HistoryToolScope::new(
        "conv-mine",
        HistoryToolBudget {
            max_response_bytes: 4096,
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
        },
    );
    let mut memo = HistoryToolMemo::default();

    let result = execute(
        &port(&pool),
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "importer"})),
    )
    .await
    .unwrap();
    assert_eq!(result.error_code.as_deref(), Some("TURN_BUDGET_EXHAUSTED"));

    // Automatic retrieval reports the same thing as a reason, not as a miss.
    let recalled = recall_for_turn(
        &port(&pool),
        &scope,
        &RecallRequest {
            user_input: "importer",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();
    assert_eq!(recalled.availability(), RecallAvailability::Unavailable);
    assert_eq!(
        recalled.diagnostics.index_error.as_deref(),
        Some("turn_budget_exhausted")
    );
}

#[tokio::test]
async fn the_same_search_twice_in_one_turn_is_answered_with_a_reference() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "importer notes",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();
    let call = || search_call(serde_json::json!({"query": "importer"}));

    let first = execute(&port, &scope, &mut memo, call()).await.unwrap();
    assert_eq!(matched_ids(&first), ["m1"]);

    let second = execute(&port, &scope, &mut memo, call()).await.unwrap();
    assert_eq!(data(&second)["repeat_of_earlier_search"], true);
    assert!(data(&second).get("matches").is_none());
}

// ---------------------------------------------------------------------------
// What the read tool refuses
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_read_needs_exactly_one_way_of_naming_what_to_read() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "importer notes",
        "completed",
    )
    .await;
    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();

    let nothing = execute(&port, &scope, &mut memo, read_call(serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(nothing.error_code.as_deref(), Some("NOTHING_REQUESTED"));

    let both = execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"], "from_sequence": 1})),
    )
    .await
    .unwrap();
    assert_eq!(both.error_code.as_deref(), Some("AMBIGUOUS_RANGE"));
}

/// A generation that failed was never an answer this conversation gave, so it
/// is not returned as history. A *user* message that failed still stands: they
/// said it (§6.2).
#[tokio::test]
async fn a_failed_assistant_draft_is_not_returned_as_history() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "u1",
        "user",
        "do not ship on Friday",
        "failed",
    )
    .await;
    seed_message(
        &pool,
        "conv-mine",
        "a1",
        "assistant",
        "abandoned draft about shipping",
        "failed",
    )
    .await;
    let mut memo = HistoryToolMemo::default();

    let result = execute(
        &port(&pool),
        &scope("conv-mine"),
        &mut memo,
        read_call(serde_json::json!({"from_sequence": 0})),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(data(&result)).unwrap();
    assert!(body.contains("do not ship on Friday"));
    assert!(!body.contains("abandoned draft"));
    assert_eq!(data(&result)["omitted_failed_outputs"], 1);
}

// ---------------------------------------------------------------------------
// Availability across tool branches
// ---------------------------------------------------------------------------

/// A closed-book turn may not search the web or the vault. It must still be
/// able to recover a requirement the user stated twenty turns ago: the current
/// conversation is internal task context, not an external source.
#[test]
fn closed_book_keeps_internal_history_access_while_dropping_external_tools() {
    let external = vec![
        ToolDefinition {
            name: "semantic_search".into(),
            description: "vault".into(),
            parameters: serde_json::json!({"type": "object"}),
        },
        ToolDefinition {
            name: "web_search".into(),
            description: "web".into(),
            parameters: serde_json::json!({"type": "object"}),
        },
    ];

    let closed = tools_for_turn(&external, true, true);
    let names: Vec<&str> = closed.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(
        names,
        [
            TOOL_SEARCH_CONVERSATION_HISTORY,
            TOOL_READ_CONVERSATION_HISTORY
        ]
    );

    // Every other branch keeps what it selected and gains history access.
    let open = tools_for_turn(&external, false, true);
    let names: Vec<&str> = open.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "semantic_search",
            "web_search",
            TOOL_SEARCH_CONVERSATION_HISTORY,
            TOOL_READ_CONVERSATION_HISTORY
        ]
    );

    // A grounded branch that offers a single tool still gains them.
    assert_eq!(tools_for_turn(&external[..1], false, true).len(), 3);

    // And when memory is unavailable, nothing is advertised that cannot answer:
    // §9.3 forbids telling a model to call a tool it was not given.
    let names: Vec<String> = tools_for_turn(&external, false, false)
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(names, ["semantic_search", "web_search"]);
    assert!(tools_for_turn(&external, true, false).is_empty());
}

#[test]
fn only_the_two_history_names_are_claimed_by_this_module() {
    assert!(is_history_tool(TOOL_SEARCH_CONVERSATION_HISTORY));
    assert!(is_history_tool(TOOL_READ_CONVERSATION_HISTORY));
    for other in ["semantic_search", "web_search", "get_document", ""] {
        assert!(!is_history_tool(other));
    }
    let defined: Vec<String> = history_tool_definitions()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(
        defined,
        [
            TOOL_SEARCH_CONVERSATION_HISTORY,
            TOOL_READ_CONVERSATION_HISTORY
        ]
    );
}

// ---------------------------------------------------------------------------
// Query formation
// ---------------------------------------------------------------------------

#[test]
fn exact_identifiers_are_pulled_out_of_a_question_separately() {
    let found = exact_identifiers(
        "does src-tauri/build.rs still pin v2.14.3, or was it \"the older tag\"?",
        &[],
    );
    assert!(found.contains(&"src-tauri/build.rs".to_string()));
    assert!(found.contains(&"v2.14.3".to_string()));
    assert!(found.contains(&"the older tag".to_string()));
    // Ordinary words are not identifiers; they are what the lexical query is for.
    assert!(!found.contains(&"does".to_string()));
    assert!(!found.contains(&"still".to_string()));
}

#[test]
fn identifier_extraction_is_bounded_and_deduplicated() {
    let many = (0..40)
        .map(|index| format!("v1.2.{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(exact_identifiers(&many, &[]).len(), MAX_EXACT_TERMS);
    assert_eq!(
        exact_identifiers("v2.14.3 and v2.14.3 again", &[]),
        ["v2.14.3"]
    );
    assert!(exact_identifiers("", &[]).is_empty());
    assert!(exact_identifiers("just ordinary words here", &[]).is_empty());
}

/// Sentence punctuation is not part of a name, but a separator inside one is.
#[test]
fn trailing_sentence_punctuation_is_not_part_of_an_identifier() {
    assert_eq!(exact_identifiers("is it v2.14.3?", &[]), ["v2.14.3"]);
    assert_eq!(exact_identifiers("see src/main.rs:", &[]), ["src/main.rs"]);
}

/// §9.1 step 1: recent-turn context is folded in for pronouns, and capped so a
/// long tail cannot become the whole query.
#[test]
fn the_query_carries_bounded_recent_context_for_pronouns() {
    let recent = vec!["we chose option B".to_string()];
    let query = build_recall_query("does that still hold?", &recent);
    assert!(query.starts_with("does that still hold?"));
    assert!(query.contains("option B"));

    let huge = vec![(0..4000)
        .map(|index| format!("word{index}"))
        .collect::<Vec<_>>()
        .join(" ")];
    let query = build_recall_query("question", &huge);
    assert_eq!(
        query.split_whitespace().count(),
        MAX_CONTEXT_TOKENS + 1,
        "the recent-turn tail escaped its cap"
    );
}

/// The nearest turn is the one a pronoun points at, so it is taken first.
#[test]
fn recent_context_is_taken_newest_first() {
    let recent = vec!["oldest turn".to_string(), "newest turn".to_string()];
    let query = build_recall_query("q", &recent);
    assert!(query.find("newest").unwrap() < query.find("oldest").unwrap());
}

#[test]
fn a_clip_never_splits_a_character_and_says_when_it_cut() {
    let text = "héllo";
    // Byte 2 lands inside `é`.
    assert_eq!(clip_to_bytes(text, 2), ("h", true));
    assert_eq!(clip_to_bytes(text, text.len()), (text, false));
    assert_eq!(clip_to_bytes(text, 0), ("", true));
    assert_eq!(clip_to_bytes("", 10), ("", false));
}

#[test]
fn a_requested_response_ceiling_cannot_exceed_what_the_turn_can_afford() {
    let budget = HistoryToolBudget {
        max_response_bytes: 2048,
        deadline: None,
    };
    assert_eq!(budget.resolve_bytes(Some(1_000_000)), 2048);
    assert_eq!(budget.resolve_bytes(Some(1024)), 1024);
    assert_eq!(budget.resolve_bytes(Some(1)), MIN_RESPONSE_BYTES);
    assert_eq!(budget.resolve_bytes(None), 2048);
}

/// Diagnostics carry counts and ids, never transcript text (§13).
#[tokio::test]
async fn retrieval_diagnostics_report_ids_and_counts_only() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "the passphrase is hunter2",
        "completed",
    )
    .await;

    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "passphrase",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();

    assert_eq!(recalled.diagnostics.selected_message_ids, ["m1"]);
    assert_eq!(recalled.diagnostics.passages_selected, 1);
    assert!(recalled.diagnostics.lexical_ran);
    let rendered = format!("{:?}", recalled.diagnostics);
    assert!(
        !rendered.contains("hunter2"),
        "diagnostics carried transcript text: {rendered}"
    );
}

/// §9.1 step 7: six groups is the initial ceiling, whatever the index returns.
#[tokio::test]
async fn at_most_six_passage_groups_are_packed() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    for index in 0..20 {
        seed_message(
            &pool,
            "conv-mine",
            &format!("m{index}"),
            if index % 2 == 0 { "user" } else { "assistant" },
            &format!("importer decision number {index} recorded"),
            "completed",
        )
        .await;
    }

    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "importer decision",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 64 * 1024,
        },
    )
    .await
    .unwrap();

    assert!(
        recalled.groups.len() <= MAX_PASSAGE_GROUPS,
        "{} groups",
        recalled.groups.len()
    );
    // Groups are emitted in the order the conversation happened.
    let anchors: Vec<i64> = recalled
        .groups
        .iter()
        .map(|group| group.anchor_sequence)
        .collect();
    let mut sorted = anchors.clone();
    sorted.sort();
    assert_eq!(anchors, sorted);
    // No message is packed into two groups, which would pay twice for one span.
    let ids = recalled.passages();
    let unique: std::collections::HashSet<&str> =
        ids.iter().map(|p| p.message_id.as_str()).collect();
    assert_eq!(unique.len(), ids.len());
}

/// The two consumers of a recall want opposite orders. `groups` is chronological
/// so the model reads the thread as it happened; `passages()` is worst-last so
/// the assembler's tail eviction drops the weakest group, not the newest one.
#[tokio::test]
async fn groups_render_chronologically_while_the_flattened_list_stays_worst_last() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    for (id, content) in [
        ("early", "the deploy window is the usual one"),
        ("pad1", "unrelated chatter about lunch"),
        ("pad2", "more unrelated chatter"),
        ("pad3", "still nothing to do with it"),
        ("late", "pin the runtime at v9.9.9 before the deploy window"),
    ] {
        seed_message(&pool, "conv-mine", id, "user", content, "completed").await;
    }

    let recalled = recall_for_turn(
        &port(&pool),
        &scope("conv-mine"),
        &RecallRequest {
            user_input: "deploy window v9.9.9",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 8192,
        },
    )
    .await
    .unwrap();

    assert!(recalled.groups.len() >= 2, "{:?}", recalled.groups);
    // Rendered oldest first.
    assert!(recalled.groups[0].anchor_sequence < recalled.groups[1].anchor_sequence);
    // Flattened best first: the exact-identifier hit leads even though it is the
    // later message.
    let flattened = recalled.passages();
    let leading: Vec<&str> = flattened.iter().map(|p| p.message_id.as_str()).collect();
    assert_eq!(
        leading.first(),
        Some(&"pad3"),
        "the strongest group did not lead the flattened list: {leading:?}"
    );
    assert!(recalled
        .groups
        .iter()
        .any(|group| group.from_exact_identifier));
}

/// Nothing on these paths writes. Proven the way it matters: the transcript
/// revision, the bookmark rows and the memory state are identical afterwards.
#[tokio::test]
async fn reading_history_leaves_the_conversation_untouched() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "importer notes",
        "completed",
    )
    .await;
    seed_bookmark(&pool, "conv-mine", "m1", "a note").await;
    let before: (i64, i64) = sqlx::query_as(
        "SELECT transcript_revision, \
                (SELECT COUNT(*) FROM conversation_message_bookmarks) \
         FROM conversations WHERE id = 'conv-mine'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let port = port(&pool);
    let scope = scope("conv-mine");
    let mut memo = HistoryToolMemo::default();
    execute(
        &port,
        &scope,
        &mut memo,
        search_call(serde_json::json!({"query": "importer"})),
    )
    .await
    .unwrap();
    execute(
        &port,
        &scope,
        &mut memo,
        read_call(serde_json::json!({"message_ids": ["m1"]})),
    )
    .await
    .unwrap();
    recall_for_turn(
        &port,
        &scope,
        &RecallRequest {
            user_input: "importer",
            recent_turns: &[],
            active_memory: &[],
            max_bytes: 4096,
        },
    )
    .await
    .unwrap();

    let after: (i64, i64) = sqlx::query_as(
        "SELECT transcript_revision, \
                (SELECT COUNT(*) FROM conversation_message_bookmarks) \
         FROM conversations WHERE id = 'conv-mine'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    let content: String =
        sqlx::query_scalar("SELECT content FROM conversation_messages WHERE id = 'm1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(content, "importer notes");
}

/// Roles come back with the passage, because "who said this" changes what it
/// means: a constraint from the user is not the assistant's own suggestion.
#[tokio::test]
async fn a_search_result_names_the_role_and_sequence_of_every_excerpt() {
    let pool = migrated_pool().await;
    seed_conversation(&pool, "conv-mine", "Work").await;
    let sequence = seed_message(
        &pool,
        "conv-mine",
        "m1",
        "user",
        "never deploy on a Friday",
        "completed",
    )
    .await;
    let mut memo = HistoryToolMemo::default();

    let result = execute(
        &port(&pool),
        &scope("conv-mine"),
        &mut memo,
        search_call(serde_json::json!({"query": "deploy Friday"})),
    )
    .await
    .unwrap();
    let first = &data(&result)["matches"][0];
    assert_eq!(first["role"], SourceRole::User.as_str());
    assert_eq!(first["sequence"], sequence);
    assert_eq!(first["excerpt"], "never deploy on a Friday");
    assert_eq!(
        data(&result)["continuation"]["read_with"],
        TOOL_READ_CONVERSATION_HISTORY
    );
}
