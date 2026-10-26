use super::*;
use lattice_core::application::ports::llm_port::CompletionInput;
use std::sync::Mutex;

/// What the response replays, as the provider's own JSON.
fn replayed(response: &CompletionResponse) -> Value {
    Value::Array(
        response
            .replay
            .iter()
            .map(|item| match item {
                CompletionInput::Native { value } => value.clone(),
                other => panic!("cloud replay is native, not {other:?}"),
            })
            .collect(),
    )
}

fn frames(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| format!("data: {event}\r\n\r\n"))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn openai_stream_keeps_native_tools_and_opaque_state_out_of_displayable_reasoning() {
    let terminal = json!({"status":"completed","usage":{"input_tokens":14,"output_tokens":9},"output":[
        {"type":"reasoning","id":"r1","summary":[{"type":"summary_text","text":"Compare 世界"},{"type":"summary_text","text":"Choose evidence"}],"encrypted_content":"opaque-state"},
        {"type":"message","content":[{"type":"output_text","text":"Hello"}]},
        {"type":"function_call","id":"f1","call_id":"call-1","name":"search","arguments":"{\"q\":\"世界\"}"}
    ]});
    let events = frames(&[
        json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Compare 世界"}),
        json!({"type":"response.reasoning_text.delta","delta":"private raw reasoning"}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":1,"delta":"Choose evidence"}),
        json!({"type":"response.output_text.delta","delta":"Hello"}),
        json!({"type":"response.completed","response":terminal}),
    ]);
    let text = Mutex::new(String::new());
    let reasoning = Mutex::new(String::new());
    let on_text = |delta: String| {
        text.lock().unwrap().push_str(&delta);
        Ok(())
    };
    let on_reasoning = |delta: String| {
        reasoning.lock().unwrap().push_str(&delta);
        Ok(())
    };
    let mut decoder = Decoder::new(LLMProvider::Openai);
    // Split every UTF-8 character and SSE boundary across network packets.
    for byte in events.chunks(1) {
        decoder.push(byte, &on_text, &on_reasoning).unwrap();
    }
    let response = decoder.finish(&on_text, &on_reasoning).unwrap();
    assert_eq!(*text.lock().unwrap(), "Hello");
    assert_eq!(
        *reasoning.lock().unwrap(),
        "Compare 世界\n\nChoose evidence"
    );
    assert_eq!(
        response.reasoning.as_deref(),
        Some("Compare 世界\n\nChoose evidence")
    );
    assert_eq!(replayed(&response), terminal["output"]);
    assert_eq!((response.input_tokens, response.output_tokens), (14, 9));
    assert!(
        matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, arguments, .. } if id == "call-1" && arguments["q"] == "世界")
    );
}

#[test]
fn anthropic_stream_preserves_signatures_tool_arguments_and_cumulative_usage() {
    let events = frames(&[
        json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":23,"output_tokens":1},"stop_reason":null}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Compare the sources."}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed-"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"state"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"redacted-state"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Answer"}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"page_location","start_page_number":2}}}),
        json!({"type":"content_block_stop","index":2}),
        json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"call-2","name":"search","input":{}}}),
        json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\""}}),
        json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"世界\"}"}}),
        json!({"type":"content_block_stop","index":3}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":18}}),
        json!({"type":"message_stop"}),
    ]);
    let reasoning = Mutex::new(String::new());
    let on_reasoning = |delta: String| {
        reasoning.lock().unwrap().push_str(&delta);
        Ok(())
    };
    let mut decoder = Decoder::new(LLMProvider::Anthropic);
    for chunk in events.chunks(7) {
        decoder.push(chunk, &|_| Ok(()), &on_reasoning).unwrap();
    }
    let response = decoder.finish(&|_| Ok(()), &on_reasoning).unwrap();
    assert_eq!(*reasoning.lock().unwrap(), "Compare the sources.");
    assert_eq!(response.reasoning.as_deref(), Some("Compare the sources."));
    assert_eq!(response.text, "Answer");
    // One assistant message, its signed thinking included.
    let message = &replayed(&response)[0];
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["content"][0]["signature"], "signed-state");
    assert_eq!(message["content"][1]["data"], "redacted-state");
    assert_eq!(
        message["content"][2]["citations"][0]["start_page_number"],
        2
    );
    assert_eq!((response.input_tokens, response.output_tokens), (23, 18));
    assert_eq!(response.finish_reason, "tool_use");
    assert!(
        matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, arguments, .. } if id == "call-2" && arguments["query"] == "世界")
    );
}

/// A cached prompt is still the whole prompt: Claude reports what it read
/// from its cache and what it wrote to it apart from `input_tokens`, and the
/// token estimate learns the prompt's size from the sum.
#[test]
fn anthropic_counts_the_cached_prompt_as_prompt() {
    let events = frames(&[
        json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":40,"cache_creation_input_tokens":300,"cache_read_input_tokens":9000,"output_tokens":1},"stop_reason":null}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Answer"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":12}}),
        json!({"type":"message_stop"}),
    ]);
    let mut decoder = Decoder::new(LLMProvider::Anthropic);
    decoder.push(&events, &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    assert_eq!((response.input_tokens, response.output_tokens), (9340, 12));
}

#[tokio::test]
async fn reasoning_is_delivered_before_the_completion_arrives() {
    let (sender, receiver) =
        tokio::sync::mpsc::unbounded_channel::<std::result::Result<Vec<u8>, String>>();
    let (shown, observed) = tokio::sync::oneshot::channel();
    let shown = Mutex::new(Some(shown));
    let on_reasoning = |delta: String| {
        if let Some(shown) = shown.lock().unwrap().take() {
            shown.send(delta).unwrap();
        }
        Ok(())
    };
    let on_text = |_| Ok(());
    sender.send(Ok(frames(&[json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Checking evidence."})]))).unwrap();
    let completion = drain(
        LLMProvider::Openai,
        tokio_stream::wrappers::UnboundedReceiverStream::new(receiver),
        Duration::from_secs(1),
        Instant::now(),
        &on_text,
        &on_reasoning,
    );
    tokio::pin!(completion);
    tokio::select! {
        result = observed => assert_eq!(result.unwrap(), "Checking evidence."),
        _ = &mut completion => panic!("the response completed before its terminal event"),
    }
    sender
        .send(Ok(frames(&[
            json!({"type":"response.completed","response":{"status":"completed","output":[]}}),
        ])))
        .unwrap();
    completion.await.unwrap();
}

#[test]
fn incomplete_answers_keep_their_status_but_truncated_streams_fail() {
    let mut decoder = Decoder::new(LLMProvider::Openai);
    decoder.push(&frames(&[json!({"type":"response.incomplete","response":{"status":"incomplete","output":[{"type":"message","content":[{"type":"output_text","text":"Partial answer"}]}]}})]), &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    assert_eq!(response.finish_reason, "incomplete");
    assert_eq!(response.text, "Partial answer");
    let mut decoder = Decoder::new(LLMProvider::Openai);
    decoder
        .push(b"data: [DONE]\n\n", &|_| Ok(()), &|_| Ok(()))
        .unwrap();
    assert!(decoder.finish(&|_| Ok(()), &|_| Ok(())).is_err());
}

#[tokio::test(start_paused = true)]
async fn stalled_streams_and_oversized_events_fail() {
    // The answer started, then went silent.
    let started = frames(&[json!({"type":"response.output_text.delta","delta":"Hel"})]);
    let pending =
        futures::stream::iter([Ok::<_, String>(started)]).chain(futures::stream::pending());
    let error = drain(
        LLMProvider::Openai,
        pending,
        Duration::from_secs(2),
        Instant::now(),
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("stalled"));
    let mut decoder = Decoder::new(LLMProvider::Openai);
    assert!(decoder
        .push(
            &vec![b'x'; Decoder::MAX_EVENT_BYTES + 1],
            &|_| Ok(()),
            &|_| Ok(())
        )
        .is_err());
}

#[test]
fn google_stream_preserves_tool_arguments_reasoning_and_usage() {
    let events = frames(&[
        json!({"choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"Step 1"}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{"content":"Answer"}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call-g1","type":"function","function":{"name":"search","arguments":""}}]}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":\""}}]}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"rust\"}"}}]}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":8}}),
    ]);
    let text = Mutex::new(String::new());
    let reasoning = Mutex::new(String::new());
    let on_text = |delta: String| {
        text.lock().unwrap().push_str(&delta);
        Ok(())
    };
    let on_reasoning = |delta: String| {
        reasoning.lock().unwrap().push_str(&delta);
        Ok(())
    };
    let mut decoder = Decoder::new(LLMProvider::Google);
    for byte in events.chunks(3) {
        decoder.push(byte, &on_text, &on_reasoning).unwrap();
    }
    let response = decoder.finish(&on_text, &on_reasoning).unwrap();
    assert_eq!(*text.lock().unwrap(), "Answer");
    assert_eq!(*reasoning.lock().unwrap(), "Step 1");
    assert_eq!(response.text, "Answer");
    assert_eq!(response.reasoning.as_deref(), Some("Step 1"));
    assert_eq!(response.finish_reason, "tool_calls");
    assert_eq!((response.input_tokens, response.output_tokens), (12, 8));
    assert!(
        matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, name, arguments } if id == "call-g1" && name == "search" && arguments["query"] == "rust")
    );
}

/// Gemini 3 refuses the next round of a tool loop unless each call comes back
/// with the thought signature it was streamed with.
#[test]
fn google_stream_keeps_each_calls_thought_signature_for_the_replay() {
    let events = frames(&[
        json!({"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call-g1","type":"function","function":{"name":"search","arguments":"{}"},"extra_content":{"google":{"thought_signature":"sig-1"}}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}),
    ]);
    let mut decoder = Decoder::new(LLMProvider::Google);
    decoder.push(&events, &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    let replay = replayed(&response).to_string();
    assert!(
        replay.contains("\"thought_signature\":\"sig-1\""),
        "{replay}"
    );
}

/// A model that thinks for minutes before its first delta has not stalled:
/// Claude streams no thinking unless asked, and an OpenAI reasoning model
/// without a summary sends only keepalives.
#[tokio::test(start_paused = true)]
async fn silence_before_the_first_delta_is_not_a_stall() {
    let (sender, receiver) =
        tokio::sync::mpsc::unbounded_channel::<std::result::Result<Vec<u8>, String>>();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(600)).await;
        sender
            .send(Ok(frames(&[
                json!({"type":"response.output_text.delta","delta":"Thought it through."}),
                json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Thought it through."}]}]}}),
            ])))
            .unwrap();
    });
    let response = drain(
        LLMProvider::Openai,
        tokio_stream::wrappers::UnboundedReceiverStream::new(receiver),
        Duration::from_secs(2),
        Instant::now(),
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(response.text, "Thought it through.");
}
