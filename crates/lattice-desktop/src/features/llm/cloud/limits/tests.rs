use super::*;
use serde_json::json;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn anthropic_reports_a_window_and_an_output_ceiling() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/claude-haiku-5-5"))
        .and(header("x-api-key", "secret"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "claude-haiku-5-5",
            "max_input_tokens": 1_000_000,
            "max_tokens": 128_000,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let reported = report_from(
        CloudProvider::Anthropic,
        &server.uri(),
        "claude-haiku-5-5",
        "secret",
    )
    .await
    .unwrap();
    assert_eq!(reported.context_window, Some(1_000_000));
    assert_eq!(reported.max_output_tokens, Some(128_000));
}

#[tokio::test]
async fn google_reports_them_under_its_own_names() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta/models/gemini-pro"))
        .and(header("x-goog-api-key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "models/gemini-pro",
            "inputTokenLimit": 1_048_576,
            "outputTokenLimit": 65_536,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let reported = report_from(CloudProvider::Google, &server.uri(), "gemini-pro", "secret")
        .await
        .unwrap();
    assert_eq!(reported.context_window, Some(1_048_576));
    assert_eq!(reported.max_output_tokens, Some(65_536));
}

/// OpenAI's model object has no limits to ask for, so nothing is asked.
#[tokio::test]
async fn openai_is_not_asked() {
    let reported = report(CloudProvider::Openai, "gpt-5", "secret")
        .await
        .unwrap();
    assert_eq!(reported, ModelWindowDto::default());
}

#[tokio::test]
async fn a_limit_of_nothing_is_no_report() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "max_input_tokens": 0,
        })))
        .mount(&server)
        .await;
    let reported = report_from(CloudProvider::Anthropic, &server.uri(), "model", "secret")
        .await
        .unwrap();
    assert_eq!(reported, ModelWindowDto::default());
}

/// The key goes to the provider's host and a model's name cannot steer it.
#[test]
fn a_model_name_stays_one_path_segment() {
    let url = model_url("https://api.example.com/", "v1", "../../other?x=1").unwrap();
    assert_eq!(
        url.as_str(),
        "https://api.example.com/v1/models/..%2F..%2Fother%3Fx=1"
    );
}

#[tokio::test]
async fn a_failed_lookup_says_so_and_is_asked_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let failure = report_from(CloudProvider::Anthropic, &server.uri(), "model", "secret")
        .await
        .unwrap_err();
    assert!(matches!(failure, AppError::Network(_)), "{failure:?}");
    assert!(!failure.to_string().contains("secret"));
}

#[tokio::test]
async fn a_provider_that_does_not_offer_the_model_is_not_a_network_failure() {
    for status in [404, 401] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let failure = report_from(CloudProvider::Anthropic, &server.uri(), "model", "secret")
            .await
            .unwrap_err();
        assert!(
            matches!(failure, AppError::InvalidConfig(_)),
            "{status}: {failure:?}"
        );
    }
}
