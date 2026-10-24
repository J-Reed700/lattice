//! Input checks and HTTP probes behind the settings commands that need no
//! container: category parsing, the Ollama URL and auth header rules, the
//! Ollama model probe and the custom search tool probe.
use super::*;
use lattice_core::shared::ipc::ErrorCode;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ollama(url: &str, name: &str, value: &str) -> TestOllamaConnectionRequest {
    TestOllamaConnectionRequest {
        ollama_url: url.into(),
        auth_header_name: name.into(),
        auth_header_value: value.into(),
    }
}

fn tool(endpoint: &str, query_param: &str, query: &str) -> TestCustomToolRequest {
    TestCustomToolRequest {
        endpoint: endpoint.into(),
        query_param: query_param.into(),
        max_results_param: None,
        default_max_results: 5,
        query: query.into(),
        max_results: None,
    }
}

#[test]
fn categories_parse_case_insensitively_and_reject_unknown_names() {
    assert_eq!(parse_category("LLM").unwrap(), SettingsCategory::Llm);
    assert_eq!(
        parse_category("sourcing").unwrap(),
        SettingsCategory::Sourcing
    );
    assert!(matches!(
        parse_category("weather"),
        Err(AppError::InvalidInput(_))
    ));
}

#[test]
fn ollama_urls_are_trimmed_and_must_be_plain_http_without_credentials() {
    assert_eq!(
        validate_and_normalize_ollama_url("  http://localhost:11434/  ").unwrap(),
        "http://localhost:11434"
    );
    assert_eq!(
        validate_and_normalize_ollama_url("https://models.example.org/ollama//").unwrap(),
        "https://models.example.org/ollama"
    );
    for bad in [
        "",
        "   ",
        "not a url",
        "ftp://models.example.org",
        "file:///tmp/ollama",
        "http://user:secret@models.example.org",
        "http://user@models.example.org",
    ] {
        assert!(
            matches!(
                validate_and_normalize_ollama_url(bad),
                Err(AppError::InvalidInput(_))
            ),
            "{bad:?} should be refused"
        );
    }
}

#[test]
fn auth_headers_must_come_as_a_valid_pair() {
    assert!(build_http_client(&ollama("http://x", "", "")).is_ok());
    assert!(build_http_client(&ollama("http://x", "  ", "  ")).is_ok());
    assert!(build_http_client(&ollama("http://x", "X-Api-Key", "secret")).is_ok());
    for (name, value) in [
        ("X-Api-Key", ""),
        ("", "secret"),
        ("bad header", "secret"),
        ("X-Api-Key", "line\nbreak"),
    ] {
        assert!(
            matches!(
                build_http_client(&ollama("http://x", name, value)),
                Err(AppError::InvalidInput(_))
            ),
            "{name:?}/{value:?} should be refused"
        );
    }
}

#[tokio::test]
async fn ollama_probe_lists_unique_trimmed_model_names_with_the_auth_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .and(header("X-Api-Key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llama3"},
                {"name": " llama3 "},
                {"name": ""},
                {"size": 12},
                {"name": "phi3"}
            ]
        })))
        .mount(&server)
        .await;

    let response =
        test_ollama_connection(ollama(&format!("{}/", server.uri()), "X-Api-Key", "secret"))
            .await
            .unwrap();
    assert_eq!(response.endpoint, "/api/tags");
    assert_eq!(response.models, vec!["llama3", "phi3"]);

    // Without the header the mock does not match and wiremock answers 404.
    let refused = test_ollama_connection(ollama(&server.uri(), "", "")).await;
    let error = refused.expect_err("unauthenticated probe fails");
    assert!(matches!(error.code, ErrorCode::NetworkError), "{error:?}");
}

#[tokio::test]
async fn ollama_probe_refuses_an_empty_model_list_and_bad_input() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models": []})))
        .mount(&server)
        .await;
    assert!(test_ollama_connection(ollama(&server.uri(), "", ""))
        .await
        .is_err());

    let bad_url = test_ollama_connection(ollama("ftp://example.org", "", ""))
        .await
        .expect_err("ftp is refused");
    assert!(
        matches!(bad_url.code, ErrorCode::InvalidInput),
        "{bad_url:?}"
    );
    let half_header = test_ollama_connection(ollama(&server.uri(), "X-Api-Key", ""))
        .await
        .expect_err("a header name needs a value");
    assert!(
        matches!(half_header.code, ErrorCode::InvalidInput),
        "{half_header:?}"
    );
}

#[tokio::test]
async fn custom_tool_probe_checks_its_inputs_before_any_request() {
    for request in [
        tool("  ", "q", "sourdough"),
        tool("https://search.example.org", "  ", "sourdough"),
        tool("not a url", "q", "sourdough"),
        tool("ftp://search.example.org", "q", "sourdough"),
        tool("https://search.example.org", "q", "   "),
    ] {
        let error = test_custom_tool(request.clone())
            .await
            .expect_err("invalid probe input");
        assert!(
            matches!(error.code, ErrorCode::InvalidInput),
            "{request:?}: {error:?}"
        );
    }
}

#[tokio::test]
async fn custom_tool_probe_appends_the_query_clamps_the_limit_and_previews_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("lang", "en"))
        .and(query_param("q", "sourdough starter"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_string("é".repeat(2_500)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("no such tool"))
        .mount(&server)
        .await;

    let mut request = tool(
        &format!("{}/search?lang=en", server.uri()),
        " q ",
        "  sourdough starter ",
    );
    request.max_results_param = Some(" limit ".into());
    request.max_results = Some(500);
    let response = test_custom_tool(request).await.unwrap();
    assert_eq!(response.status, 200);
    assert!(response
        .content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("text/plain")));
    assert!(response
        .final_url
        .contains("lang=en&q=sourdough+starter&limit=100"));
    assert_eq!(response.body_preview.chars().count(), 2_001);
    assert!(response.body_preview.ends_with('…'));

    let mut no_limit = tool(&format!("{}/search?lang=en", server.uri()), "q", "rye");
    no_limit.max_results_param = Some("   ".into());
    // The blank limit parameter is dropped, so this request misses the mock.
    let unmatched = test_custom_tool(no_limit)
        .await
        .expect_err("404 from the mock");
    assert!(
        matches!(unmatched.code, ErrorCode::NetworkError),
        "{unmatched:?}"
    );

    let missing = test_custom_tool(tool(&format!("{}/missing", server.uri()), "q", "rye"))
        .await
        .expect_err("404 is an error");
    assert!(
        matches!(missing.code, ErrorCode::NetworkError),
        "{missing:?}"
    );
    assert!(missing
        .details
        .as_deref()
        .is_some_and(|details| details.contains("404")));
}
