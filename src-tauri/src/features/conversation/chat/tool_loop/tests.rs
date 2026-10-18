use super::*;
use lattice_core::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN;
use std::sync::Arc;

/// The input budget the context assembler plans for a model of this size.
fn input_budget(context_tokens: usize) -> usize {
    use lattice_core::application::services::context_assembler::{
        BudgetAllocation, BudgetRequest, ModelCapacity,
    };
    BudgetAllocation::plan(
        &ModelCapacity::new("test-model", context_tokens),
        &BudgetRequest::default(),
    )
    .unwrap()
    .input_budget
}

#[test]
fn stream_sink_keeps_turn_identity_and_emits_completion_once() {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    {
        let mut stream = StreamEmitter::new(&sink, "conversation", "request");
        stream.content("hello").unwrap();
        stream.done();
        stream.done();
    }
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].content.as_deref(), Some("hello"));
    assert!(events[1].done);
    assert!(events
        .iter()
        .all(|event| event.conversation_id == "conversation" && event.request_id == "request"));
}

#[test]
fn stream_drop_reports_completion_after_early_return() {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    drop(StreamEmitter::new(&sink, "conversation", "request"));
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].done);
}

#[test]
fn an_answer_stopped_at_the_output_limit_is_kept_not_failed() {
    for reason in ["length", "max_tokens", "incomplete"] {
        assert!(answer_was_cut_short(reason, "half an answer", false).unwrap());
        // Nothing written, or a tool call cut mid-arguments: nothing to keep.
        assert!(answer_was_cut_short(reason, "  ", false).is_err());
        assert!(answer_was_cut_short(reason, "", true).is_err());
    }
    assert!(answer_was_cut_short("failed", "", false).is_err());
    assert!(!answer_was_cut_short("stop", "done", false).unwrap());
    assert!(!answer_was_cut_short("tool_calls", "", true).unwrap());
}

/// Four page fetches in one round on a 32k-token model. At the configured
/// 50,000 characters each they are ~200,000 characters — several times the
/// model's whole window — and the next generation simply failed.
#[test]
fn a_round_of_results_is_made_to_fit_a_small_window() {
    let context_tokens = 32_768;
    let window = window_chars(input_budget(context_tokens), DEFAULT_CHARS_PER_TOKEN);
    let already = 20_000;

    let mut spent = already;
    for calls_left in (1..=4).rev() {
        spent += tool_result_allowance(window, spent, calls_left);
    }
    assert!(
        spent <= window,
        "{spent} characters in a {window}-character window"
    );
}

/// The model in the log has a 131k window. Each of four results gets a
/// fifth of what is left, a share being kept for the next round, which is
/// far past the 50,000 characters a configured ceiling used to cut every
/// result to.
#[test]
fn a_large_window_gives_each_result_its_share_with_no_ceiling() {
    let window = window_chars(input_budget(131_072), DEFAULT_CHARS_PER_TOKEN);
    let allowance = tool_result_allowance(window, 20_000, 4);
    assert_eq!(allowance, (window - 20_000) / 5);
    assert!(allowance > 50_000, "got {allowance}");
}

/// Past a full window, the floor would overrun it and fail the next round.
#[test]
fn a_full_window_gives_a_result_only_what_is_left() {
    assert_eq!(
        tool_result_allowance(
            window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
            1_000_000,
            3
        ),
        0
    );
    let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
    let nearly_full = window - 900;
    assert_eq!(
        tool_result_allowance(
            window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
            nearly_full,
            1
        ),
        450
    );
}

/// With room, a result still gets enough to be worth the round.
#[test]
fn a_window_with_room_keeps_the_floor() {
    let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
    assert_eq!(
        tool_result_allowance(
            window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
            window - 4_000,
            3
        ),
        MIN_TOOL_RESULT_CHARS
    );
}

/// On a 4k-token local window the schemas alone are a large share, and a
/// round that ignored them handed out room the window did not have.
#[test]
fn tool_schemas_are_charged_against_a_small_window() {
    let tools = vec![lattice_core::application::ports::ToolDefinition {
        name: "web_search".into(),
        description: "Search the web.".into(),
        parameters: serde_json::json!({"type":"object","properties":{
            "query":{"type":"string","description":"x".repeat(3_000)}}}),
    }];
    let schema_chars = tool_schema_text(&tools).chars().count();
    assert!(
        schema_chars > 3_000,
        "parameters must be counted: {schema_chars}"
    );

    let window = window_chars(input_budget(4_096), DEFAULT_CHARS_PER_TOKEN);
    let prompt = vec![CompletionInput::Message {
        role: "user".into(),
        content: "p".repeat(window - schema_chars - 500),
    }];
    let in_flight = chars_in_flight(&prompt) + schema_chars;
    let allowance = tool_result_allowance(
        window_chars(input_budget(4_096), DEFAULT_CHARS_PER_TOKEN),
        in_flight,
        1,
    );
    assert_eq!(allowance, 250);
    assert!(in_flight + allowance <= window);
}

#[test]
fn a_full_window_has_no_room_for_another_result() {
    let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
    assert!(window_has_room_for_a_result(
        window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
        0
    ));
    assert!(!window_has_room_for_a_result(
        window,
        window - MIN_TOOL_RESULT_CHARS + 1
    ));
    assert!(
        window_has_room_for_a_result(window_chars(0, DEFAULT_CHARS_PER_TOKEN), usize::MAX),
        "no window reported"
    );
}

/// A malformed call reaches the loop as a call whose reply is the error,
/// quoting what the model wrote, so the next round can correct it.
#[test]
fn a_malformed_call_is_answered_with_the_error() {
    let response = crate::features::llm::llama_cpp::parse_completion(serde_json::json!({
        "choices":[{"message":{"content":"Searching.","tool_calls":[{"id":"c1","type":"function",
            "function":{"name":"web_search","arguments":"{query: rust"}}]},
            "finish_reason":"tool_calls"}]}))
    .unwrap();
    assert_eq!(response.text, "Searching.");
    let Some(CompletionInput::ToolCall { arguments, .. }) = response.tool_calls.first() else {
        panic!("the malformed call must be kept");
    };
    let reply = crate::features::llm::llama_cpp::invalid_tool_call_problem(arguments)
        .expect("marked invalid");
    assert!(reply.contains("not valid JSON"), "{reply}");
    assert!(reply.contains("{query: rust"), "{reply}");
}

/// A lone page read used to be given the whole window, so the round after
/// it had nothing left to read with and one long page ended the research.
/// Half is kept back; the rest of the page is a further call by offset.
#[test]
fn a_lone_result_leaves_the_next_round_room_to_read() {
    let window = window_chars(input_budget(131_072), DEFAULT_CHARS_PER_TOKEN);
    let allowance = tool_result_allowance(window, 0, 1);
    assert_eq!(allowance, window / 2);
    assert!(window - allowance >= window / 2);
}

/// However many results a round has, each round leaves one share behind.
#[test]
fn a_round_of_results_leaves_a_share_for_the_next_round() {
    let window = window_chars(input_budget(32_768), DEFAULT_CHARS_PER_TOKEN);
    let mut spent = 0;
    for calls_left in (1..=4).rev() {
        spent += tool_result_allowance(window, spent, calls_left);
    }
    assert!(window - spent >= window / 5 - 4, "{spent} of {window}");
}

#[test]
fn what_is_in_flight_counts_every_item_the_next_round_sends() {
    let input = vec![
        CompletionInput::Message {
            role: "user".to_string(),
            content: "x".repeat(100),
        },
        CompletionInput::ToolResult {
            id: "1".to_string(),
            output: "y".repeat(400),
        },
    ];
    assert_eq!(chars_in_flight(&input), 500);
    assert_eq!(chars_in_flight(&[]), 0);
}
