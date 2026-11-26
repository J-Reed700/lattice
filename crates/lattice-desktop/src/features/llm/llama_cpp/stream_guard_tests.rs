use super::*;
use crate::features::llm::fitting::ResponseBudget;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[test]
fn stream_rejects_oversized_single_event_without_newline_above_output_budget() {
    let budget = ResponseBudget::for_tokens(16);
    let mut decoder = streaming::Decoder::with_budget(budget);
    let oversized = vec![b'x'; budget.max_event_bytes + 1];
    let error = decoder.push(&oversized).unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert!(error.to_string().contains("output budget"), "{error}");
}

#[test]
fn stream_rejects_total_bytes_exceeding_output_budget() {
    let budget = ResponseBudget::for_tokens(8);
    let mut decoder = streaming::Decoder::with_budget(budget);
    // Repeated valid SSE comment/keepalive lines with no token content still hit
    // the wire-byte budget so a stalled/infinite metadata stream cannot OOM.
    let chunk = format!(": {}\n", "a".repeat(256)).into_bytes();
    let mut rejected = None;
    for _ in 0..=(budget.max_stream_bytes / chunk.len() + 2) {
        if let Err(error) = decoder.push(&chunk) {
            rejected = Some(error);
            break;
        }
    }
    let error = rejected.expect("stream exceeding byte budget must fail");
    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert!(error.to_string().contains("output budget"), "{error}");
}

#[test]
fn stream_enforces_token_event_budget_while_allowing_within_budget_and_progress_frames() {
    let budget = ResponseBudget::for_tokens(4);
    let mut ok_decoder = streaming::Decoder::with_budget(budget);
    // Non-token prompt-progress frames do not consume token event budget.
    for _ in 0..10 {
        ok_decoder
            .push(b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"prompt_progress\":{\"processed\":1}}\n\n")
            .unwrap();
    }
    // Exactly 4 token events (2 reasoning + 2 content) stay within budget.
    for frame in [
        json!({"choices":[{"delta":{"reasoning_content":"r1"}}]}),
        json!({"choices":[{"delta":{"reasoning_content":"r2"}}]}),
        json!({"choices":[{"delta":{"content":"c1"}}]}),
        json!({"choices":[{"delta":{"content":"c2"},"finish_reason":"stop"}]}),
    ] {
        ok_decoder
            .push(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    ok_decoder.push(b"data: [DONE]\n\n").unwrap();
    assert_eq!(ok_decoder.into_response().unwrap().text, "c1c2");

    // A 5th token-bearing event exceeds the 4-token budget.
    let mut over_decoder = streaming::Decoder::with_budget(budget);
    for i in 0..4 {
        let frame = json!({"choices":[{"delta":{"content":format!("t{i}")}}]});
        over_decoder
            .push(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    let fifth = json!({"choices":[{"delta":{"content":"t4"}}]});
    let error = over_decoder
        .push(format!("data: {fifth}\n\n").as_bytes())
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert!(error.to_string().contains("token events"), "{error}");
}

#[test]
fn tool_calls_beyond_output_budget_are_rejected_in_stream_and_whole_response() {
    // 16 tokens -> max_tool_calls = 4.
    let budget = ResponseBudget::for_tokens(16);
    let mut decoder = streaming::Decoder::with_budget(budget);
    for idx in 0..4 {
        let frame = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": idx,
                        "id": format!("call_{idx}"),
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{}"}
                    }]
                }
            }]
        });
        decoder
            .push(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    let fifth = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 4,
                    "id": "call_4",
                    "type": "function",
                    "function": {"name": "lookup", "arguments": "{}"}
                }]
            }
        }]
    });
    let stream_err = decoder
        .push(format!("data: {fifth}\n\n").as_bytes())
        .unwrap_err();
    assert!(
        matches!(stream_err, AppError::InvalidState(_)),
        "{stream_err:?}"
    );
    assert!(
        stream_err.to_string().contains("tool call count"),
        "{stream_err}"
    );

    let raw_calls: Vec<Value> = (0..5)
        .map(|idx| {
            json!({
                "id": format!("call_{idx}"),
                "type": "function",
                "function": {"name": "lookup", "arguments": "{}"}
            })
        })
        .collect();
    let whole_err = parse_completion_with_budget(
        json!({
            "choices": [{
                "message": {"role": "assistant", "content": "", "tool_calls": raw_calls},
                "finish_reason": "tool_calls"
            }]
        }),
        budget.max_tool_calls,
    )
    .unwrap_err();
    assert!(
        matches!(whole_err, AppError::InvalidState(_)),
        "{whole_err:?}"
    );
    assert!(
        whole_err.to_string().contains("tool call count"),
        "{whole_err}"
    );
}

#[tokio::test]
async fn test_connection_rejects_oversized_non_streamed_completion_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "test-model"}]
        })))
        .mount(&server)
        .await;
    let probe_budget = ResponseBudget::for_tokens(2048);
    let oversized_text = "a".repeat(probe_budget.max_whole_bytes + 1024);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {"role": "assistant", "content": oversized_text},
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    let connection = LlamaCppSettingsDto {
        url: server.uri(),
        model: "test-model".into(),
        ..Default::default()
    };
    let error = LlamaCppLlm::test_connection(&connection, None)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert!(error.to_string().contains("output budget"), "{error}");
}
