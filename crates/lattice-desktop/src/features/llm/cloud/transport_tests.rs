use super::protocol::protocols;
use super::*;
use crate::features::llm::fitting::SendUnscheduled;
use lattice_core::application::contracts::settings::LLMProvider;
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::domain::models::model_ref::CloudProvider;
use lattice_core::shared::error::ErrorClass;
use serde_json::json;
use wiremock::{
    matchers::{body_string_contains, header, method, path},
    Mock, MockServer, ResponseTemplate,
};
async fn client(server: &MockServer) -> CloudLlm {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "test-model".into(),
        ..Default::default()
    };
    let mut client =
        CloudLlm::new(&settings, "test-secret".into(), Some(settings.max_tokens)).unwrap();
    client.endpoint = format!("{}/responses", server.uri());
    client
}
#[tokio::test]
async fn interactive_completion_can_disable_the_wall_clock_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)).set_body_json(json!({
            "status":"completed", "output":[{"type":"message","content":[{"type":"output_text","text":"Completed"}]}]
        }))).mount(&server).await;
    let client = client(&server).await;
    let request = CompletionRequest {
        time_budget: Some(Duration::from_millis(10)),
        ..Default::default()
    };
    assert!(client.complete(&request).await.is_err());
    let response = client
        .complete(&CompletionRequest {
            no_time_limit: true,
            ..request
        })
        .await
        .unwrap();
    assert_eq!(response.text, "Completed");
}

#[tokio::test]
async fn streams_text_and_requires_terminal_event() {
    let server = MockServer::start().await;
    let completed = json!({"type":"response.completed","response":{"status":"completed","output":[
        {"type":"message","content":[{"type":"output_text","text":"héllo"}]}
    ]}});
    Mock::given(method("POST")).and(path("/responses")).and(header("authorization", "Bearer test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"héllo\"}}\n\ndata: {completed}\n\n")))
        .mount(&server).await;
    let client = client(&server).await;
    let shown = std::sync::Mutex::new(Vec::new());
    let on_text = |text: String| {
        shown.lock().unwrap().push(text);
        Ok(())
    };
    let response = client
        .complete_with_progress(&CompletionRequest::default(), &on_text)
        .await
        .unwrap();
    assert_eq!(response.text, "héllo");
    assert_eq!(*shown.lock().unwrap(), vec!["héllo".to_string()]);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
        ))
        .mount(&server)
        .await;
    assert!(client
        .complete_with_progress(&CompletionRequest::default(), &|_| Ok(()))
        .await
        .is_err());
}
#[tokio::test]
async fn typed_reasoning_completion_requests_a_stream_and_keeps_its_native_result() {
    let server = MockServer::start().await;
    let final_response = json!({"status":"completed","output":[
        {"type":"reasoning","summary":[{"type":"summary_text","text":"Checked evidence."}],"encrypted_content":"opaque"},
        {"type":"message","content":[{"type":"output_text","text":"Answer"}]}
    ],"usage":{"input_tokens":5,"output_tokens":8}});
    let body = [
        json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Checked evidence."}),
        json!({"type":"response.output_text.delta","delta":"Answer"}),
        json!({"type":"response.completed","response":final_response}),
    ].iter().map(|event| format!("data: {event}\n\n")).collect::<String>();
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let reasoning = std::sync::Mutex::new(String::new());
    let response = client
        .complete_with_reasoning_progress(
            &CompletionRequest {
                include_reasoning: true,
                ..Default::default()
            },
            &|_| Ok(()),
            &|delta| {
                reasoning.lock().unwrap().push_str(&delta);
                Ok(())
            },
            &|_| Ok(()),
        )
        .await
        .unwrap();
    assert_eq!(*reasoning.lock().unwrap(), "Checked evidence.");
    assert_eq!(replayed(&response), final_response["output"]);
    assert_eq!(response.text, "Answer");
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], true);
}
#[tokio::test]
async fn typed_tool_calls_keep_native_call_ids_and_replay_them() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"completed", "output":[{"type":"function_call", "call_id":"call_7", "name":"search", "arguments":"{}"}], "usage":{"input_tokens":4,"output_tokens":2}
        }))).mount(&server).await;
    let client = client(&server).await;
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Search first.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "find".into(),
            },
        ],
        tools: vec![lattice_core::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search documents".into(),
            parameters: json!({"type":"object","properties":{}}),
        }],
        ..Default::default()
    };
    let response = client.complete(&request).await.unwrap();
    assert!(matches!(
        response.tool_calls.first(),
        Some(CompletionInput::ToolCall { id, .. }) if id == "call_7"
    ));
    assert_eq!(replayed(&response)[0]["call_id"], "call_7");
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests.first().unwrap().body).unwrap();
    assert_eq!(
        body.pointer("/input/0/role").and_then(Value::as_str),
        Some("system")
    );
    assert_eq!(
        body.pointer("/tools/0/name").and_then(Value::as_str),
        Some("search")
    );
}

#[tokio::test]
async fn authentication_failure_is_not_retried_or_echoed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("sensitive-provider-body"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server).await;
    let error = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("401"));
    assert!(!error.to_string().contains("sensitive-provider-body"));
    // A rejected key is a setting to fix, never an outage a job waits out.
    assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
    assert!(!ErrorClass::of(&error).is_temporary());
    assert!(error.to_string().contains("API key"), "{error}");
}

#[tokio::test]
async fn refusals_are_typed_by_what_waiting_can_change() {
    for (status, transient) in [
        (403, false),
        (404, false),
        (400, false),
        (429, true),
        (529, true),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        // Shorter than the first backoff: the provider's own refusal comes back.
        let request = CompletionRequest {
            time_budget: Some(Duration::from_millis(900)),
            ..Default::default()
        };
        let error = client(&server).await.complete(&request).await.unwrap_err();
        assert_eq!(
            ErrorClass::of(&error).is_temporary(),
            transient,
            "HTTP {status}: {error:?}"
        );
    }
}
fn openai_status(status: u16, body: &str) -> AppError {
    status_error(
        protocols(CloudProvider::Openai).completion,
        reqwest::StatusCode::from_u16(status).unwrap(),
        &serde_json::from_str(body).unwrap_or(Value::Null),
    )
}

/// Only the body tells an account with nothing left from a busy one. No
/// amount of waiting refills an account, so it is a setting to fix.
#[test]
fn an_account_out_of_quota_is_a_setting_to_fix_not_an_outage() {
    for body in [
        r#"{"error":{"message":"You exceeded your current quota.","type":"insufficient_quota","param":null,"code":"insufficient_quota"}}"#,
        r#"{"error":{"type":"insufficient_quota"}}"#,
        r#"{"error":{"code":"insufficient_quota"}}"#,
    ] {
        let error = openai_status(429, body);
        let AppError::InvalidConfig(text) = &error else {
            panic!("{body}: {error:?}");
        };
        assert_eq!(
            text,
            "OpenAI says the account has no credit or quota left (HTTP 429): add credit to the account, or choose another model in Settings → AI"
        );
        assert!(!ErrorClass::of(&error).is_temporary(), "{body}");
    }
}

#[test]
fn a_payment_required_refusal_says_the_accounts_billing_needs_fixing() {
    for protocol in [
        protocols(CloudProvider::Openai).completion,
        protocols(CloudProvider::Anthropic).completion,
    ] {
        let error = status_error(
            protocol,
            reqwest::StatusCode::PAYMENT_REQUIRED,
            &Value::Null,
        );
        let AppError::InvalidConfig(text) = &error else {
            panic!("{error:?}");
        };
        assert!(text.contains("HTTP 402"), "{text}");
        assert!(text.contains("billing"), "{text}");
        assert!(!ErrorClass::of(&error).is_temporary());
    }
}

#[test]
fn any_other_rate_limit_is_a_transient_rate_limit() {
    for body in [
        "",
        "not json",
        r#"{"error":{"message":"Rate limit reached for requests","type":"requests","code":"rate_limit_exceeded"}}"#,
        r#"{"error":{"type":"tokens","code":"rate_limit_exceeded"}}"#,
    ] {
        let error = openai_status(429, body);
        assert!(
            matches!(error, AppError::RateLimitExceeded(_)),
            "{body}: {error:?}"
        );
        assert!(ErrorClass::of(&error) == ErrorClass::Outage, "{body}");
    }
}

#[test]
fn a_timeout_and_a_server_error_stay_network_outages() {
    for status in [408, 500, 502, 503, 529] {
        for body in ["", r#"{"error":{"type":"insufficient_quota"}}"#] {
            let error = openai_status(status, body);
            assert!(
                matches!(error, AppError::Network(_)),
                "HTTP {status}: {error:?}"
            );
            assert!(ErrorClass::of(&error) == ErrorClass::Outage);
        }
    }
}

/// A refused account is told once: the request is not resent while it is
/// refused, and nothing waits for it.
#[tokio::test]
async fn a_request_refused_for_quota_is_sent_once_and_reaches_the_caller_as_a_setting() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({"error":{
                    "message":"You exceeded your current quota, sensitive-provider-reason",
                    "type":"insufficient_quota","code":"insufficient_quota"
                }})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .await
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
    assert!(!ErrorClass::of(&error).is_temporary());
    assert!(!error.to_string().contains("sensitive-provider-reason"));
}

/// An ordinary rate limit is still retried, and when it persists the caller
/// hears a rate limit, not a lost connection.
#[tokio::test]
async fn a_persistent_rate_limit_reaches_the_caller_as_a_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(json!({"error":{"type":"requests","code":"rate_limit_exceeded"}})),
        )
        .mount(&server)
        .await;
    // Room for one resend after the first backoff, not for the second.
    let request = CompletionRequest {
        time_budget: Some(Duration::from_millis(2_500)),
        ..Default::default()
    };
    let error = client(&server).await.complete(&request).await.unwrap_err();
    assert!(matches!(error, AppError::RateLimitExceeded(_)), "{error:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

async fn anthropic_client(server: &MockServer) -> CloudLlm {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        model: "claude-test".into(),
        ..Default::default()
    };
    let mut client =
        CloudLlm::new(&settings, "test-secret".into(), Some(settings.max_tokens)).unwrap();
    client.endpoint = format!("{}/messages", server.uri());
    client
}

fn structured() -> CompletionRequest {
    CompletionRequest {
        json_schema: Some(json!({"type":"object","properties":{"ok":{"type":"boolean"}}})),
        ..Default::default()
    }
}

/// Claude refuses some schemas it cannot compile ("The compiled grammar is
/// too large"), which no shape check predicts; the request goes once more
/// with the schema asked for in words.
#[tokio::test]
async fn a_schema_claude_refuses_is_asked_for_in_words() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("json_schema"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({"type":"error","error":{
                "type":"invalid_request_error",
                "message":"The compiled grammar is too large, which would cause performance issues."
            }})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("matches this JSON Schema"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content":[{"type":"text","text":"{\"ok\":true}"}],"stop_reason":"end_turn"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let response = anthropic_client(&server)
        .await
        .complete(&structured())
        .await
        .unwrap();
    assert_eq!(response.text, "{\"ok\":true}");
}

/// The reason Claude gives for refusing a schema is what tells a schema that
/// is too large from one it cannot read, so the fallback logs it with the
/// fields of the schema it refused.
#[tokio::test]
async fn a_schema_claude_refuses_is_logged_with_the_apis_reason() {
    let logs = lattice_core::shared::testing::logs::capture_logs(tracing::Level::WARN);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("json_schema"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({"type":"error","error":{
                "type":"invalid_request_error",
                "message":"The compiled grammar is too large, which would cause performance issues."
            }})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("matches this JSON Schema"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content":[{"type":"text","text":"{\"ok\":true}"}],"stop_reason":"end_turn"
        })))
        .mount(&server)
        .await;

    anthropic_client(&server)
        .await
        .complete(&structured())
        .await
        .unwrap();

    let logs = logs.text();
    assert!(logs.contains("compiled grammar is too large"), "{logs}");
    assert!(logs.contains("schema_fields=ok"), "{logs}");
}

/// Any other bad request is the request's own fault: it is not resent, and
/// the provider's reason is never echoed.
#[tokio::test]
async fn any_other_bad_request_is_not_resent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({"type":"error","error":{
                "type":"invalid_request_error","message":"messages.0: sensitive-provider-reason"
            }})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let error = anthropic_client(&server)
        .await
        .complete(&structured())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::ModelRejected(_)), "{error:?}");
    assert!(!error.to_string().contains("sensitive-provider-reason"));
}

/// An account out of credit answers every request with a 400. It is a
/// setting to fix, so no job waits it out and no request is resent or
/// reworded, with or without a schema.
#[tokio::test]
async fn an_account_out_of_credit_is_a_setting_to_fix() {
    for request in [CompletionRequest::default(), structured()] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"type":"error","error":{
                    "type":"invalid_request_error",
                    "message":"Your credit balance is too low to access the Anthropic API. \
                               Please go to Plans & Billing to upgrade or purchase credits."
                }})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let error = anthropic_client(&server)
            .await
            .complete(&request)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
        assert!(!ErrorClass::of(&error).is_temporary());
        assert!(error.to_string().contains("credit balance"), "{error}");
        assert!(!error.to_string().contains("Plans & Billing"), "{error}");
    }
}

#[tokio::test]
async fn dropping_a_completion_cancels_the_request_without_a_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_string("data: {\"type\":\"response.completed\"}\n\n"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server).await;
    let abandoned = tokio::time::timeout(
        Duration::from_millis(100),
        client.complete_with_progress(&CompletionRequest::default(), &|_| Ok(())),
    )
    .await;
    assert!(abandoned.is_err());
    // No detached generation or retry task survives a dropped completion.
    server.verify().await;
}

/// The provider's answer is what its window learns from: a request that
/// completes while others are held back by the window opens it wider.
#[tokio::test]
async fn a_completed_request_tells_the_providers_scheduler_it_was_accepted() {
    use crate::features::llm::scheduler::AdmissionRequest;
    use lattice_core::application::ports::llm_port::InferencePriority;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]
        })))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let _in_flight = client
        .scheduler
        .admit(
            AdmissionRequest {
                priority: InferencePriority::Interactive,
                tokens: 1,
                cache_key: None,
                rank: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(client.scheduler.load().slots, 1);
    client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(client.scheduler.load().slots, 2);
}

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

/// The provider's `retry-after` is its own statement of when to come back:
/// resending sooner is refused again, so it is honoured as given.
#[tokio::test]
async fn a_rate_limit_is_resent_only_after_the_wait_the_provider_asked_for() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "45")
                .set_body_json(json!({"error":{"type":"requests","code":"rate_limit_exceeded"}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]
        })))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let scheduler = Arc::clone(&client.scheduler);
    let call = tokio::spawn(async move { client.complete(&CompletionRequest::default()).await });

    let closed_for = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(wait) = scheduler.reopens_in() {
                break wait;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the refusal never closed admission");
    assert!(
        closed_for > Duration::from_secs(30) && closed_for <= Duration::from_secs(45),
        "closed for {closed_for:?}, not the 45 s the provider asked for"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(45)).await;
    tokio::time::resume();
    assert_eq!(call.await.unwrap().unwrap().text, "ok");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

/// A monthly spend limit is refused with a 429 that no waiting changes until
/// the next month, so it is told once, as a setting, and never resent.
#[tokio::test]
async fn a_spend_limit_refusal_is_sent_once_and_names_the_spend_limit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429).set_body_json(json!({"type":"error","error":{
                "type":"rate_limit_error",
                "message":"sensitive-provider-reason",
                "details":{"error_code":"enforced_spend_limit_reached"}
            }})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = anthropic_client(&server)
        .await
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
    assert!(!ErrorClass::of(&error).is_temporary());
    assert!(error.to_string().contains("monthly spend limit"), "{error}");
    assert!(!error.to_string().contains("sensitive-provider-reason"));
}

/// A request with less time left than the provider asked it to wait gives the
/// provider's refusal back, and is never resent before the time asked for.
#[tokio::test]
async fn a_retry_after_beyond_the_time_left_gives_the_refusal_without_resending_early() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "45")
                .set_body_json(json!({"error":{"type":"requests","code":"rate_limit_exceeded"}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]
        })))
        .mount(&server)
        .await;
    let request = CompletionRequest {
        time_budget: Some(Duration::from_secs(35)),
        ..Default::default()
    };
    let error = client(&server).await.complete(&request).await.unwrap_err();
    assert!(matches!(error, AppError::RateLimitExceeded(_)), "{error:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// The wait before a resend is the backoff of a refused attempt, not the
/// model reading the prompt: a streamed answer is timed from the attempt that
/// was answered.
#[tokio::test]
async fn a_streamed_answer_resent_after_a_server_error_is_timed_from_the_resend() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let completed = json!({"type":"response.completed","response":{"status":"completed",
        "usage":{"input_tokens":2000,"output_tokens":300},
        "output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}}\n\ndata: {completed}\n\n"
        )))
        .mount(&server)
        .await;
    let response = client(&server)
        .await
        .complete_with_progress(&CompletionRequest::default(), &|_| Ok(()))
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    let timing = response.timing.unwrap();
    assert!(timing.prompt_seconds < 0.5, "{timing:?}");
}
