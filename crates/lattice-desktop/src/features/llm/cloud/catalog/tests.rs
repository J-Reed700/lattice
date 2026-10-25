use super::*;
use serde_json::json;
use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn no_saved_key_returns_an_empty_catalog_without_a_request() {
    let server = MockServer::start().await;
    let result = catalog(CloudProvider::Openai, None, &server.uri())
        .await
        .unwrap();
    assert!(!result.has_api_key);
    assert!(result.models.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn openai_authenticates_filters_specialized_models_and_sorts_newest_first() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id":"gpt-old", "created":1},
            {"id":"text-embedding-3-large"},
            {"id":"gpt-new-audio"},
            {"id":"gpt-new-image"},
            {"id":"gpt-new", "created":10},
            {"id":"gpt-new", "created":10},
            {"id":"o99", "created":9},
            {"id":"ft:gpt-new:org:custom:1", "created":8},
            {"id":"gpt-retired", "shutdown_date":"2000-01-01"}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let result = catalog(
        CloudProvider::Openai,
        Some("secret"),
        &format!("{}/models", server.uri()),
    )
    .await
    .unwrap();
    assert!(result.has_api_key);
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["gpt-new", "o99", "ft:gpt-new:org:custom:1", "gpt-old"]
    );
    assert_eq!(result.models[0].name, "gpt-new");
}

#[tokio::test]
async fn anthropic_authenticates_and_follows_all_pages_preserving_display_names() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("x-api-key", "secret"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":[{"id":"claude-new", "display_name":"Claude New"}],
            "has_more":true, "last_id":"claude-new"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("after_id", "claude-new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":[{"id":"claude-small", "display_name":"Claude Small"}], "has_more":false
        })))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    let result = catalog(
        CloudProvider::Anthropic,
        Some("secret"),
        &format!("{}/models", server.uri()),
    )
    .await
    .unwrap();
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["Claude New", "Claude Small"]
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn google_follows_page_tokens_and_only_offers_text_generation_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(header("x-goog-api-key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models":[
                {"name":"models/gemini-new", "displayName":"Gemini New", "supportedGenerationMethods":["generateContent"]},
                {"name":"models/embedding", "supportedGenerationMethods":["embedContent"]},
                {"name":"models/gemini-new-tts", "supportedGenerationMethods":["generateContent"]}
            ], "nextPageToken":"next"
        }))).mount(&server).await;
    Mock::given(method("GET")).and(query_param("pageToken", "next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models":[{"name":"models/gemini-small", "supportedGenerationMethods":["generateContent"]}]
        }))).with_priority(1).mount(&server).await;
    let result = catalog(CloudProvider::Google, Some("secret"), &server.uri())
        .await
        .unwrap();
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["gemini-new", "gemini-small"]
    );
}

#[tokio::test]
async fn provider_failures_are_actionable_and_never_echo_response_bodies() {
    for status in [401, 403, 429, 500] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status).set_body_string("sensitive-provider-body"))
            .expect(1)
            .mount(&server)
            .await;
        let error = catalog(CloudProvider::Openai, Some("secret"), &server.uri())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(&status.to_string()));
        assert!(!error.contains("sensitive-provider-body"));
    }
}

#[tokio::test]
async fn malformed_lists_and_repeated_cursors_fail_instead_of_publishing_partial_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"unexpected":[]})))
        .mount(&server)
        .await;
    assert!(
        catalog(CloudProvider::Openai, Some("secret"), &server.uri())
            .await
            .is_err()
    );
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":[{"id":"claude-new"}], "has_more":true, "last_id":"same"
        })))
        .mount(&server)
        .await;
    let error = catalog(CloudProvider::Anthropic, Some("secret"), &server.uri())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("repeated"));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
