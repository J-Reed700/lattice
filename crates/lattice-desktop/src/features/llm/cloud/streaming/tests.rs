use super::*;
use crate::features::llm::cloud::protocol::protocols;
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::domain::models::model_ref::CloudProvider;
use serde_json::json;
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Openai).completion);
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Anthropic).completion);
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Anthropic).completion);
    decoder.push(&events, &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    assert_eq!((response.input_tokens, response.output_tokens), (9340, 12));
}

#[tokio::test]
async fn reasoning_is_delivered_before_the_completion_arrives() {
    let (sender, receiver) =
        tokio::sync::mpsc::unbounded_channel::<std::result::Result<Vec<u8>, std::io::Error>>();
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
        protocols(CloudProvider::Openai).completion,
        tokio_stream::wrappers::UnboundedReceiverStream::new(receiver),
        ResponseBudget::default(),
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Openai).completion);
    decoder.push(&frames(&[json!({"type":"response.incomplete","response":{"status":"incomplete","output":[{"type":"message","content":[{"type":"output_text","text":"Partial answer"}]}]}})]), &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    assert_eq!(response.finish_reason, "incomplete");
    assert_eq!(response.text, "Partial answer");
    let mut decoder = Decoder::new(protocols(CloudProvider::Openai).completion);
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
        futures::stream::iter([Ok::<_, std::io::Error>(started)]).chain(futures::stream::pending());
    let error = drain(
        protocols(CloudProvider::Openai).completion,
        pending,
        ResponseBudget::default(),
        Duration::from_secs(2),
        Instant::now(),
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("stalled"));

    let budget = ResponseBudget::for_tokens(16);
    let mut decoder = Decoder::with_budget(protocols(CloudProvider::Openai).completion, budget);
    let err = decoder
        .push(
            &vec![b'x'; budget.max_event_bytes + 1],
            &|_| Ok(()),
            &|_| Ok(()),
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    assert!(err.to_string().contains("output budget"), "{err}");
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Google).completion);
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
    let mut decoder = Decoder::new(protocols(CloudProvider::Google).completion);
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
        tokio::sync::mpsc::unbounded_channel::<std::result::Result<Vec<u8>, std::io::Error>>();
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
        protocols(CloudProvider::Openai).completion,
        tokio_stream::wrappers::UnboundedReceiverStream::new(receiver),
        ResponseBudget::default(),
        Duration::from_secs(2),
        Instant::now(),
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(response.text, "Thought it through.");
}

/// A call that takes no arguments can stream only empty deltas; one cut off
/// at the output limit streams half an object. Neither fails the reply: the
/// first is `{}`, the second is answered with why it was not run.
#[test]
fn anthropic_calls_with_no_or_broken_arguments_do_not_fail_the_reply() {
    let events = frames(&[
        json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":10,"output_tokens":1},"stop_reason":null}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call-1","name":"list_attachments","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call-2","name":"search","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"bri"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":8}}),
        json!({"type":"message_stop"}),
    ]);
    let mut decoder = Decoder::new(protocols(CloudProvider::Anthropic).completion);
    decoder.push(&events, &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    let arguments: Vec<_> = response
        .tool_calls
        .iter()
        .filter_map(|call| match call {
            CompletionInput::ToolCall { arguments, .. } => Some(arguments.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(arguments[0], json!({}));
    let problem =
        lattice_core::application::ports::llm_port::invalid_tool_call_problem(&arguments[1]);
    assert!(problem.is_some_and(|problem| problem.contains("not JSON")));
}

/// A message is not cut off after 1024 content blocks.
#[test]
fn anthropic_accepts_a_content_block_at_a_high_index() {
    let events = frames(&[
        json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":10,"output_tokens":1},"stop_reason":null}}),
        json!({"type":"content_block_start","index":1024,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":1024,"delta":{"type":"text_delta","text":"Far block"}}),
        json!({"type":"content_block_stop","index":1024}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}),
        json!({"type":"message_stop"}),
    ]);
    let mut decoder = Decoder::new(protocols(CloudProvider::Anthropic).completion);
    decoder.push(&events, &|_| Ok(()), &|_| Ok(())).unwrap();
    let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
    assert_eq!(response.text, "Far block");
}

/// Whether `events`, pushed whole, left the model's answer begun.
fn begun_after(provider: CloudProvider, events: &[Value]) -> bool {
    let mut decoder = Decoder::new(protocols(provider).completion);
    decoder
        .push(&frames(events), &|_| Ok(()), &|_| Ok(()))
        .unwrap();
    decoder.events.answer_begun()
}

#[test]
fn a_claude_answer_begins_with_its_first_content_block() {
    let start = json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":10,"output_tokens":1},"stop_reason":null}});
    let thinking = json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}});
    assert!(!begun_after(
        CloudProvider::Anthropic,
        std::slice::from_ref(&start)
    ));
    assert!(begun_after(CloudProvider::Anthropic, &[start, thinking]));
}

#[test]
fn an_openai_answer_begins_with_its_first_output_item() {
    let created = json!({"type":"response.created","response":{"status":"in_progress"}});
    let item = json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[]}});
    assert!(!begun_after(
        CloudProvider::Openai,
        std::slice::from_ref(&created)
    ));
    assert!(begun_after(CloudProvider::Openai, &[created, item]));
}

#[test]
fn a_chat_answer_begins_with_its_first_content_not_its_role() {
    let role = json!({"choices":[{"index":0,"delta":{"role":"assistant"}}]});
    let content = json!({"choices":[{"index":0,"delta":{"content":"Hi"}}]});
    assert!(!begun_after(
        CloudProvider::Google,
        std::slice::from_ref(&role)
    ));
    assert!(begun_after(CloudProvider::Google, &[role, content]));
}

#[test]
fn cloud_stream_guards_reject_over_budget_events_bytes_tokens_and_tool_calls() {
    // 1. Over-budget token events (max_tokens = 4).
    let budget = ResponseBudget::for_tokens(4);
    let mut ok_decoder = Decoder::with_budget(protocols(CloudProvider::Openai).completion, budget);
    let ok_frames = frames(&[
        json!({"type":"response.output_text.delta","delta":"a"}),
        json!({"type":"response.output_text.delta","delta":"b"}),
        json!({"type":"response.output_text.delta","delta":"c"}),
        json!({"type":"response.output_text.delta","delta":"d"}),
        json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"abcd"}]}]}}),
    ]);
    ok_decoder
        .push(&ok_frames, &|_| Ok(()), &|_| Ok(()))
        .unwrap();
    assert_eq!(
        ok_decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap().text,
        "abcd"
    );

    let mut over_tokens = Decoder::with_budget(protocols(CloudProvider::Openai).completion, budget);
    let too_many = frames(&[
        json!({"type":"response.output_text.delta","delta":"1"}),
        json!({"type":"response.output_text.delta","delta":"2"}),
        json!({"type":"response.output_text.delta","delta":"3"}),
        json!({"type":"response.output_text.delta","delta":"4"}),
        json!({"type":"response.output_text.delta","delta":"5"}),
    ]);
    let err = over_tokens
        .push(&too_many, &|_| Ok(()), &|_| Ok(()))
        .unwrap_err();
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    assert!(err.to_string().contains("token event count"), "{err}");

    // 2. Over-budget total stream bytes via keepalive/ignored frames.
    let small_budget = ResponseBudget::for_tokens(4);
    let mut byte_decoder =
        Decoder::with_budget(protocols(CloudProvider::Openai).completion, small_budget);
    let line = format!(
        "data: {}\r\n\r\n",
        json!({"type":"response.reasoning_text.delta","delta":"x".repeat(256)})
    )
    .into_bytes();
    let mut rejected = None;
    for _ in 0..=(small_budget.max_stream_bytes / line.len() + 2) {
        if let Err(err) = byte_decoder.push(&line, &|_| Ok(()), &|_| Ok(())) {
            rejected = Some(err);
            break;
        }
    }
    let err = rejected.expect("oversized cloud stream must fail");
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    assert!(
        err.to_string().contains("response exceeds output budget"),
        "{err}"
    );

    // 3. Over-budget tool calls on Google stream (16 tokens -> max_tool_calls = 4).
    let tool_budget = ResponseBudget::for_tokens(16);
    let mut tool_decoder =
        Decoder::with_budget(protocols(CloudProvider::Google).completion, tool_budget);
    let mut tool_events = Vec::new();
    for idx in 0..5 {
        tool_events.push(json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":idx,"id":format!("c{idx}"),"type":"function","function":{"name":"search","arguments":"{}"}}]}}]}));
    }
    let err = tool_decoder
        .push(&frames(&tool_events), &|_| Ok(()), &|_| Ok(()))
        .unwrap_err();
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    assert!(err.to_string().contains("tool call count"), "{err}");

    // 4. Over-budget Anthropic content blocks (max_tokens = 2 -> max_blocks = 4).
    let block_budget = ResponseBudget::for_tokens(2);
    let mut block_decoder =
        Decoder::with_budget(protocols(CloudProvider::Anthropic).completion, block_budget);
    let mut block_events = vec![
        json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":5,"output_tokens":1},"stop_reason":null}}),
    ];
    for idx in 0..5 {
        block_events.push(json!({"type":"content_block_start","index":idx,"content_block":{"type":"text","text":""}}));
        block_events.push(json!({"type":"content_block_stop","index":idx}));
    }
    let err = block_decoder
        .push(&frames(&block_events), &|_| Ok(()), &|_| Ok(()))
        .unwrap_err();
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    assert!(err.to_string().contains("content block count"), "{err}");
}
