//! The one request a page costs: the page image and the model card's prompt
//! to `/v1/chat/completions`, the page's text back.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use super::render::PageImage;
use super::server::ServerAddress;
use lattice_core::application::ports::OcrError;

/// Most tokens one page may produce. The context holds the image (about 1,250
/// tokens) plus the answer; a dense page of prose reads back in about 900, so
/// this bound is reached by a model stuck repeating itself, not by a page.
pub(crate) const MAX_PAGE_TOKENS: u32 = 2560;

/// HTTP client for the OCR server.
pub struct OcrClient {
    http: reqwest::Client,
}

impl OcrClient {
    pub fn new() -> Result<Self, OcrError> {
        let http = reqwest::Client::builder()
            // Loopback never goes through a proxy.
            .no_proxy()
            .build()
            .map_err(|error| OcrError::Unavailable {
                reason: format!("no HTTP client: {error}"),
            })?;
        Ok(Self { http })
    }

    /// Read one page. `timeout` bounds the whole request.
    pub async fn recognize(
        &self,
        server: &ServerAddress,
        image: &PageImage,
        prompt: &str,
        page: u32,
        timeout: Duration,
    ) -> Result<String, OcrError> {
        let fail = |reason: String| OcrError::Recognition { page, reason };
        let response = self
            .http
            .post(format!("{}/v1/chat/completions", server.endpoint))
            .bearer_auth(&server.api_key)
            .timeout(timeout)
            .json(&request_body(image, prompt))
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    fail(format!("no answer within {} s", timeout.as_secs()))
                } else {
                    fail(error.to_string())
                }
            })?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| fail(error.to_string()))?;
        if !status.is_success() {
            let excerpt: String = body.chars().take(300).collect();
            return Err(fail(format!("the OCR server answered {status}: {excerpt}")));
        }
        page_text(&body, page)
    }
}

fn request_body(image: &PageImage, prompt: &str) -> Value {
    json!({
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": image.data_uri() } },
                { "type": "text", "text": prompt },
            ],
        }],
        "temperature": 0,
        "max_tokens": MAX_PAGE_TOKENS,
        "stream": false,
    })
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

/// The page's text from a chat completion body.
fn page_text(body: &str, page: u32) -> Result<String, OcrError> {
    let completion: Completion =
        serde_json::from_str(body).map_err(|error| OcrError::Recognition {
            page,
            reason: format!("unreadable answer: {error}"),
        })?;
    let Some(choice) = completion.choices.into_iter().next() else {
        return Err(OcrError::Recognition {
            page,
            reason: "the answer has no choices".into(),
        });
    };
    if choice.finish_reason.as_deref() == Some("length") {
        // Kept: a page cut short is still mostly right, and dropping it would
        // leave the page unindexed with no better option.
        warn!(
            page,
            max_tokens = MAX_PAGE_TOKENS,
            "OCR stopped at its token limit; the page may be cut short"
        );
    }
    Ok(choice
        .message
        .content
        .unwrap_or_default()
        .trim()
        .to_string())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn png() -> PageImage {
        PageImage {
            bytes: b"\x89PNG\r\n\x1a\npixels".to_vec(),
            mime: "image/png",
        }
    }

    fn address(server: &MockServer) -> ServerAddress {
        ServerAddress {
            endpoint: server.uri(),
            api_key: "secret".into(),
        }
    }

    #[test]
    fn the_request_carries_the_image_before_the_prompt_at_temperature_zero() {
        let body = request_body(&png(), "OCR:");
        let content = &body["messages"][0]["content"];

        assert_eq!(content[0]["type"], "image_url");
        assert!(content[0]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(content[1]["text"], "OCR:");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["stream"], false);
    }

    #[tokio::test]
    async fn a_page_comes_back_trimmed_from_an_authenticated_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer secret"))
            .and(body_partial_json(json!({ "max_tokens": MAX_PAGE_TOKENS })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "\n1 Introduction\n\nRecurrent networks...\n" },
                    "finish_reason": "stop",
                }],
            })))
            .expect(1)
            .mount(&server)
            .await;

        let text = OcrClient::new()
            .unwrap()
            .recognize(&address(&server), &png(), "OCR:", 3, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(text, "1 Introduction\n\nRecurrent networks...");
    }

    #[tokio::test]
    async fn a_server_error_names_the_page_and_the_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("failed to load image"))
            .mount(&server)
            .await;

        let error = OcrClient::new()
            .unwrap()
            .recognize(&address(&server), &png(), "OCR:", 7, Duration::from_secs(5))
            .await
            .unwrap_err();

        match error {
            OcrError::Recognition { page, reason } => {
                assert_eq!(page, 7);
                assert!(reason.contains("500") && reason.contains("failed to load image"));
            }
            other => panic!("expected a page failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_page_that_takes_too_long_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;

        let error = OcrClient::new()
            .unwrap()
            .recognize(
                &address(&server),
                &png(),
                "OCR:",
                1,
                Duration::from_millis(200),
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no answer within"), "{error}");
    }

    #[test]
    fn a_truncated_page_is_kept_and_an_empty_answer_is_blank() {
        let cut = r#"{"choices":[{"message":{"content":"half a page"},"finish_reason":"length"}]}"#;
        assert_eq!(page_text(cut, 1).unwrap(), "half a page");
        let blank = r#"{"choices":[{"message":{"content":null},"finish_reason":"stop"}]}"#;
        assert_eq!(page_text(blank, 1).unwrap(), "");
        assert!(page_text(r#"{"choices":[]}"#, 1).is_err());
    }
}
