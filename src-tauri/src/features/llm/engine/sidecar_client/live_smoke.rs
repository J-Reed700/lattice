//! Opt-in qualification against a running local engine and real model weights.
//! Uses the production request builder, response parser, and streaming decoder.
use super::*;

#[tokio::test]
#[ignore = "requires LATTICE_SIDECAR_TEST_URL and optional LATTICE_SIDECAR_TEST_TOKEN"]
async fn live_model_returns_text_and_structured_answers_with_reasoning_enabled() {
    let endpoint = std::env::var("LATTICE_SIDECAR_TEST_URL").expect("local test endpoint");
    let url = reqwest::Url::parse(&endpoint).unwrap();
    assert!(matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "[::1]")
    ));
    let client = reqwest::Client::new();
    let config = GenerationConfig {
        max_tokens: 4096,
        ..Default::default()
    };
    let schema = json!({
        "type":"object", "properties":{"answer":{"type":"integer"}},
        "required":["answer"], "additionalProperties":false
    });
    for (stream, structured, effort) in [
        (false, false, None),
        (false, true, None),
        (true, true, None),
        (true, true, Some("low")),
    ] {
        let prompt = if structured {
            "What is 2 + 2? Put the integer in the answer field."
        } else {
            "What is 2 + 2? Reply with only the integer."
        };
        let body = SidecarLLMClient::build_request(
            &config,
            SidecarLLMClient::build_messages(None, prompt),
            stream,
            RequestTuning {
                reasoning_effort: effort,
                json_schema: structured.then_some(&schema),
                sampling: Some(SamplingOverride::deterministic()),
                ..Default::default()
            },
        );
        let mut request = client
            .post(format!("{endpoint}/v1/chat/completions"))
            .json(&body);
        if let Ok(token) = std::env::var("LATTICE_SIDECAR_TEST_TOKEN") {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.unwrap().error_for_status().unwrap();
        let answer = if stream {
            SidecarLLMClient::drain_typed_stream(
                response.bytes_stream(),
                Duration::from_secs(120),
                &|_| Ok(()),
            )
            .await
            .unwrap()
        } else {
            parse_completion(response.json().await.unwrap()).unwrap()
        };
        assert_eq!(answer.finish_reason, "stop");
        if structured {
            assert_eq!(
                serde_json::from_str::<Value>(&answer.text).unwrap(),
                json!({"answer":4})
            );
        } else {
            assert_eq!(answer.text.trim(), "4");
        }
        eprintln!("Qualified text/JSON request: streaming={stream}, structured={structured}, effort={effort:?}, output_tokens={}", answer.output_tokens);
    }
}
