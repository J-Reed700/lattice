//! Requests lost on their way to the provider are sent again under the
//! backends' shared rule.
use super::*;
use crate::features::llm::fitting::SendUnscheduled;
use lattice_core::application::contracts::settings::LLMProvider;
use lattice_core::application::services::model_contract::legacy;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

fn client_at(endpoint: String) -> CloudLlm {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "test-model".into(),
        ..Default::default()
    };
    CloudLlm::new(&settings, "test-secret".into(), Some(settings.max_tokens))
        .unwrap()
        .with_endpoint(endpoint)
}

/// A client whose own timeout ends every exchange after `timeout`.
fn impatient_client_at(endpoint: String, timeout: Duration) -> CloudLlm {
    let mut client = client_at(endpoint);
    client.client = reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()
        .unwrap();
    client
}

fn unlimited() -> CompletionRequest {
    let mut request = legacy::completion(legacy::Completion::default());
    request.no_time_limit = true;
    request
}

fn reply(text: &str) -> Value {
    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":text}]}]})
}

fn delta(text: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({"type":"response.output_text.delta","delta":text})
    )
}

fn whole(text: &str) -> Vec<u8> {
    let body = reply(text).to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn full_stream(text: &str) -> Vec<u8> {
    let completed = json!({"type":"response.completed","response":reply(text)});
    let events = format!("{}data: {completed}\n\n", delta(text));
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{events}",
        events.len()
    )
    .into_bytes()
}

/// Promises more than it sends: one delta, then the connection closes.
fn cut_off_stream(text: &str) -> Vec<u8> {
    let event = delta(text);
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{event}",
        event.len() + 100
    )
    .into_bytes()
}

/// Reads one HTTP request through the end of its body; false when the
/// client closed first.
async fn read_request(socket: &mut TcpStream) -> bool {
    let mut seen = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        if let Some(end) = seen.windows(4).position(|four| four == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&seen[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map_or(0, |value| value.trim().parse::<usize>().unwrap_or(0));
            if seen.len() >= end + 4 + length {
                return true;
            }
        }
        match socket.read(&mut buffer).await {
            Ok(0) | Err(_) => return false,
            Ok(read) => seen.extend_from_slice(&buffer[..read]),
        }
    }
}

/// A server on a raw socket that reads each request, sends the next of
/// `answers` (the last one again once they run out) and closes the
/// connection. Returns the endpoint and how many requests it read.
async fn scripted(answers: Vec<Vec<u8>>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    let read = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&read);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let answer = answers[count.load(Ordering::SeqCst).min(answers.len() - 1)].clone();
            if read_request(&mut socket).await {
                count.fetch_add(1, Ordering::SeqCst);
                let _ = socket.write_all(&answer).await;
            }
            let _ = socket.shutdown().await;
        }
    });
    (endpoint, read)
}

#[tokio::test]
async fn a_request_whose_connection_drops_before_the_reply_is_sent_again() {
    let (endpoint, requests) = scripted(vec![Vec::new(), whole("Recovered")]).await;
    match client_at(endpoint).complete(&unlimited()).await {
        Ok(response) => assert_eq!(response.text, "Recovered"),
        Err(error) => panic!("not sent again: {error:?}"),
    }
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// The caller hears of the new attempt before its text, and keeps only the
/// text of the attempt that finished.
#[tokio::test]
async fn a_stream_cut_off_part_way_is_sent_again_once_the_caller_is_told() {
    let (endpoint, requests) =
        scripted(vec![cut_off_stream("Partial"), full_stream("Recovered")]).await;
    let shown = Mutex::new(Vec::new());
    let retries = Mutex::new(Vec::new());
    let on_text = |text: String| {
        shown.lock().unwrap().push(text);
        Ok(())
    };
    let on_retry = |attempt: usize| {
        retries.lock().unwrap().push(attempt);
        shown.lock().unwrap().clear();
        Ok(())
    };
    match client_at(endpoint)
        .complete_with_retry_progress(&unlimited(), &on_text, &on_retry)
        .await
    {
        Ok(response) => assert_eq!(response.text, "Recovered"),
        Err(error) => panic!("not sent again: {error:?}"),
    }
    assert_eq!(*retries.lock().unwrap(), vec![2]);
    assert_eq!(*shown.lock().unwrap(), vec!["Recovered".to_string()]);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// A caller that cannot be told to start over would be handed both
/// attempts' text.
#[tokio::test]
async fn a_cut_off_stream_whose_text_reached_a_caller_that_cannot_start_over_is_not_sent_again() {
    let (endpoint, requests) = scripted(vec![cut_off_stream("Partial")]).await;
    let error = client_at(endpoint)
        .complete_with_progress(&unlimited(), &|_| Ok(()))
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// With no deadline, the client's own timeout ending an exchange is a lost
/// request, not a spent budget.
#[tokio::test]
async fn a_request_timed_out_by_the_client_with_no_deadline_is_sent_again() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(reply("Late")),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("Recovered")))
        .mount(&server)
        .await;
    let client = impatient_client_at(
        format!("{}/responses", server.uri()),
        Duration::from_millis(200),
    );
    match client.complete(&unlimited()).await {
        Ok(response) => assert_eq!(response.text, "Recovered"),
        Err(error) => panic!("not sent again: {error:?}"),
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_client_timeout_with_no_deadline_reaches_the_caller_as_a_lost_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    let error = impatient_client_at(endpoint, Duration::from_millis(200))
        .complete(&unlimited())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert!(!error.to_string().contains("time budget"), "{error}");
}

#[tokio::test]
async fn server_errors_are_sent_again_under_the_shared_rule() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("Recovered")))
        .mount(&server)
        .await;
    match client_at(format!("{}/responses", server.uri()))
        .complete(&unlimited())
        .await
    {
        Ok(response) => assert_eq!(response.text, "Recovered"),
        Err(error) => panic!("gave up after server errors: {error:?}"),
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn a_server_error_is_sent_again_after_the_wait_it_asked_for() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "2"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("Recovered")))
        .mount(&server)
        .await;
    let started = Instant::now();
    match client_at(format!("{}/responses", server.uri()))
        .complete(&unlimited())
        .await
    {
        Ok(response) => assert_eq!(response.text, "Recovered"),
        Err(error) => panic!("not sent again: {error:?}"),
    }
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(2), "{waited:?}");
}

/// A refusal for load keeps the scheduler's own count: never three for each
/// resend.
#[tokio::test]
async fn a_persistent_overload_is_sent_three_times_in_all() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529))
        .mount(&server)
        .await;
    let error = client_at(format!("{}/responses", server.uri()))
        .complete(&unlimited())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}
