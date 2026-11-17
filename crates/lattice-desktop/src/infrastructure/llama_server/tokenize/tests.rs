use super::*;

#[test]
fn a_tokenize_reply_counts_ids_or_pieces() {
    assert_eq!(
        parse_token_count(&serde_json::json!({"tokens": [1, 2, 3]})).unwrap(),
        3
    );
    assert_eq!(
        parse_token_count(
            &serde_json::json!({"tokens": [{"id": 1, "piece": "a"}, {"id": 2, "piece": "b"}]})
        )
        .unwrap(),
        2
    );
    assert!(parse_token_count(&serde_json::json!({"error": "x"})).is_err());
}

#[tokio::test]
async fn exact_counts_come_from_the_server_once_per_text() {
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .and(body_partial_json(
            serde_json::json!({"content": "hello world", "add_special": false}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"tokens": [9, 8]})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let tokenizer = ServerTokenizer::new(reqwest::Client::new(), &server.uri());
    assert_eq!(tokenizer.count("hello world").await.unwrap(), 2);
    assert_eq!(
        tokenizer.count("hello world").await.unwrap(),
        2,
        "the second count is served from the cache"
    );
    assert_eq!(tokenizer.count("").await.unwrap(), 0);
}
