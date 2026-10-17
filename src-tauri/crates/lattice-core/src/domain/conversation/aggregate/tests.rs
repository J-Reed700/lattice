#![cfg(test)]

use super::*;

/// An aggregate with `count` alternating user/assistant messages, 10 tokens each.
fn aggregate_with_messages(count: usize) -> ConversationAggregate {
    let mut aggregate = ConversationAggregate::new("Test".to_string(), None).expect("aggregate");
    for i in 0..count {
        let role = if i % 2 == 0 {
            MessageRole::User
        } else {
            MessageRole::Assistant
        };
        aggregate
            .add_message(role, format!("message {}", i), 10)
            .expect("add message");
    }
    aggregate
}

/// Attach a compaction the way persistence does: a `CompactionRecord` read
/// back from `conversation_summaries`.
///
/// There is deliberately no aggregate method that creates one. A summary row
/// is written in exactly one place — the memory commit — so that the summary
/// and the ledger it belongs to can never describe different revisions.
fn with_compaction(
    aggregate: ConversationAggregate,
    summary_text: &str,
    up_to_message_id: &str,
    summary_tokens: i64,
) -> ConversationAggregate {
    let compacted = aggregate
        .messages()
        .iter()
        .position(|m| m.id == up_to_message_id)
        .map(|index| index + 1)
        .unwrap_or(0) as i64;
    let original_tokens = (compacted * 10).max(1);
    let record = CompactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: aggregate.id().clone(),
        summary_text: summary_text.to_string(),
        up_to_message_id: up_to_message_id.to_string(),
        original_message_count: compacted,
        original_tokens,
        summary_tokens,
        compression_ratio: (summary_tokens as f64 / original_tokens as f64).min(1.0),
        created_at: Utc::now(),
    };
    ConversationAggregate::from_persistence(
        aggregate.conversation().clone(),
        aggregate.messages().to_vec(),
        aggregate.document_context().to_vec(),
        Some(record),
    )
}

#[test]
fn compaction_replaces_the_folded_prefix_in_the_llm_context() {
    let aggregate = aggregate_with_messages(6);
    let boundary = aggregate.messages()[3].id.clone();
    let aggregate = with_compaction(aggregate, "distilled past", &boundary, 5);

    // Every message is still present for display...
    assert_eq!(aggregate.messages().len(), 6);
    // ...but the context carries the summary plus only what follows it.
    let live: Vec<_> = aggregate
        .live_messages()
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert_eq!(live, vec!["message 4", "message 5"]);

    // The summary itself is rendered by the context assembler, which is the
    // one place a prompt is built; the aggregate's job is only to say which
    // messages are still carried raw and what the summary text is.
    let preamble = aggregate
        .context_preamble()
        .expect("a compaction has a preamble");
    assert!(preamble.contains("distilled past"));
    // And it must say who wrote it. This preamble used to read only
    // "[Earlier conversation, summarized]", which let the QA and
    // context-window paths hand a model's paraphrase to another model as
    // though it were the transcript.
    assert!(
        preamble.contains("written by a model"),
        "a summary presented without that caveat reads as something the user said"
    );
}

#[test]
fn a_stale_boundary_keeps_the_history_instead_of_erasing_it() {
    let aggregate = aggregate_with_messages(4);
    let boundary = aggregate.messages()[1].id.clone();
    let mut aggregate = with_compaction(aggregate, "summary", &boundary, 5);

    // The boundary message is deleted out from under the summary.
    aggregate.messages.retain(|m| m.id != boundary);

    // Falling open costs tokens; falling closed would silently drop the
    // entire live conversation from the model's context.
    assert_eq!(aggregate.live_messages().len(), 3);
}

#[test]
fn test_conversation_serialization_camelcase() {
    let conv = Conversation {
        id: ConversationId::new(),
        title: "Test Conversation".to_string(),
        model_choice: ChatModelChoice::Default,
        system_prompt: Some("You are helpful".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        message_count: 5,
        total_tokens: 1000,
    };
    let json = serde_json::to_string(&conv).unwrap();
    assert!(
        json.contains("\"modelChoice\":"),
        "Expected modelChoice in JSON"
    );
    assert!(
        json.contains("\"systemPrompt\":"),
        "Expected systemPrompt in JSON"
    );
    assert!(
        json.contains("\"createdAt\":"),
        "Expected createdAt in JSON"
    );
    assert!(
        json.contains("\"updatedAt\":"),
        "Expected updatedAt in JSON"
    );
    assert!(
        json.contains("\"messageCount\":"),
        "Expected messageCount in JSON"
    );
    assert!(
        json.contains("\"totalTokens\":"),
        "Expected totalTokens in JSON"
    );
}

#[test]
fn test_conversation_message_serialization_camelcase() {
    let msg = ConversationMessage {
        id: "msg-123".to_string(),
        conversation_id: ConversationId::new(),
        role: MessageRole::User,
        content: "Hello".to_string(),
        tokens: 10,
        created_at: Utc::now(),
        metadata: Some(r#"{"key":"value"}"#.to_string()),
        status: "completed".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(
        json.contains("\"conversationId\":"),
        "Expected conversationId in JSON"
    );
    assert!(
        json.contains("\"createdAt\":"),
        "Expected createdAt in JSON"
    );
}

#[test]
fn test_document_reference_serialization_camelcase() {
    let doc_ref = DocumentReference {
        document_id: "doc-123".to_string(),
        chunk_id: Some("chunk-456".to_string()),
        relevance_score: Some(0.95),
        added_at: Utc::now(),
    };
    let json = serde_json::to_string(&doc_ref).unwrap();
    assert!(
        json.contains("\"documentId\":"),
        "Expected documentId in JSON"
    );
    assert!(json.contains("\"chunkId\":"), "Expected chunkId in JSON");
    assert!(
        json.contains("\"relevanceScore\":"),
        "Expected relevanceScore in JSON"
    );
    assert!(json.contains("\"addedAt\":"), "Expected addedAt in JSON");
}
