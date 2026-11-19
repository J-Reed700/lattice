//! The recording proxy against a scripted server: what the app receives,
//! and what each call records. The module is the example's, compiled here
//! because CI runs no example's tests.

#[allow(dead_code)]
#[path = "../examples/shipped_path_eval/proxy.rs"]
mod proxy;

use proxy::{start, CallRecord, Recorder};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The proxy module's result type, as the example defines it.
type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The window the app is shown, and the server's own.
const REPORTED: u64 = 16_384;
const SERVED: u64 = 32_768;

const GET_PROPS: &str = "GET /props HTTP/1.1\r\nHost: localhost\r\n\r\n";

/// How the scripted server answers one connection.
enum Answer {
    /// These bytes, then the connection closes.
    Once(Vec<u8>),
    /// This head, then this chunk again and again until the proxy hangs up.
    Endless { head: Vec<u8>, chunk: Vec<u8> },
}

/// A server that answers its connections in turn with `answers`, each once
/// it has read the request.
async fn server(answers: Vec<Answer>) -> Res<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            if read_request(&mut stream).await.is_err() {
                return;
            }
            match answer {
                Answer::Once(bytes) => {
                    stream.write_all(&bytes).await.ok();
                    stream.shutdown().await.ok();
                }
                Answer::Endless { head, chunk } => {
                    let mut written = stream.write_all(&head).await;
                    while written.is_ok() {
                        written = stream.write_all(&chunk).await;
                    }
                }
            }
        }
    });
    Ok(format!("http://{address}"))
}

/// Reads one request: its head, then as many body bytes as its
/// Content-Length gives.
async fn read_request(stream: &mut TcpStream) -> Res<()> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(at) = find(&bytes, b"\r\n\r\n") {
            break at + 4;
        }
        if stream.read_buf(&mut bytes).await? == 0 {
            return Err("the request ended inside its head".into());
        }
    };
    let head = String::from_utf8_lossy(bytes.get(..end).unwrap_or_default()).to_ascii_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .map_or("0", str::trim)
        .parse()?;
    while bytes.len() < end + length {
        if stream.read_buf(&mut bytes).await? == 0 {
            return Err("the request ended inside its body".into());
        }
    }
    Ok(())
}

/// Where `needle` first starts in `bytes`.
fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `200 OK` with this content type and body.
fn ok(content_type: &str, body: &str) -> Answer {
    Answer::Once(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes(),
    )
}

/// llama-server's `/props` for a window of `n_ctx`.
fn props(n_ctx: u64) -> Answer {
    let body = serde_json::json!({"default_generation_settings": {"n_ctx": n_ctx}});
    ok("application/json", &body.to_string())
}

/// A chat request as the app sends one.
fn chat(body: &str) -> String {
    format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Sends `request` to the proxy at `base` and reads its whole answer: the
/// status, the head's other lines, the body.
async fn exchange(base: &str, request: &str) -> Res<(u16, Vec<String>, Vec<u8>)> {
    let mut stream = TcpStream::connect(base.trim_start_matches("http://")).await?;
    stream.write_all(request.as_bytes()).await?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await?;
    let end = find(&answer, b"\r\n\r\n").ok_or("the answer has no head")?;
    let head = String::from_utf8_lossy(answer.get(..end).unwrap_or_default()).into_owned();
    let mut lines = head.split("\r\n").map(str::to_owned);
    let status_line = lines.next().ok_or("the answer has no status line")?;
    let status = status_line
        .split(' ')
        .nth(1)
        .ok_or("the status line has no status")?
        .parse::<u16>()?;
    let body = answer.get(end + 4..).unwrap_or_default().to_vec();
    Ok((status, lines.collect(), body))
}

/// The proxy's calls once it has recorded `count`. It records a call when
/// its answer to the app ends, which can be after the app has read that
/// answer. The wait gives up after 30 seconds, half of nextest's CI limit,
/// so a proxy that never records fails with the count it reached.
async fn recorded(recorder: &Recorder, count: usize) -> Res<Vec<CallRecord>> {
    let all_in = async {
        while recorder.len() < count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(30), all_in)
        .await
        .map_err(|_| format!("the proxy recorded {} of {count} calls", recorder.len()))?;
    Ok(recorder.since(0))
}

#[tokio::test]
async fn the_start_check_reads_the_reported_window_and_records_no_first_byte_time() -> Res<()> {
    let recorder = Recorder::new();
    let upstream = server(vec![props(SERVED)]).await?;
    start(upstream, false, Some(REPORTED), None, Arc::clone(&recorder)).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!(
        (call.method.as_str(), call.path.as_str(), call.status),
        ("GET", "/props", Some(200))
    );
    assert_eq!(call.error, None);
    assert_eq!(call.first_byte_ms, None);
    Ok(())
}

#[tokio::test]
async fn a_props_body_that_is_not_json_keeps_the_server_content_type() -> Res<()> {
    let recorder = Recorder::new();
    let upstream = server(vec![props(SERVED), ok("text/html", "<p>sign in</p>")]).await?;
    let base = start(upstream, false, Some(REPORTED), None, Arc::clone(&recorder)).await?;
    recorded(&recorder, 1).await?;
    let (status, head, body) = exchange(&base, GET_PROPS).await?;
    let calls = recorded(&recorder, 2).await?;
    let call = calls.get(1).ok_or("the second call was not recorded")?;
    assert_eq!(
        (status, body.as_slice()),
        (200, b"<p>sign in</p>".as_slice())
    );
    assert_eq!(call.status, Some(200));
    assert!(call.error.is_some(), "the parse error is recorded");
    assert!(
        head.iter()
            .any(|line| line.eq_ignore_ascii_case("content-type: text/html")),
        "{head:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_props_body_that_breaks_off_answers_502_and_records_the_read_error() -> Res<()> {
    let cut = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"default";
    let recorder = Recorder::new();
    let upstream = server(vec![props(SERVED), Answer::Once(cut.as_bytes().to_vec())]).await?;
    let base = start(upstream, false, Some(REPORTED), None, Arc::clone(&recorder)).await?;
    recorded(&recorder, 1).await?;
    let (status, _, body) = exchange(&base, GET_PROPS).await?;
    let calls = recorded(&recorder, 2).await?;
    let call = calls.get(1).ok_or("the second call was not recorded")?;
    assert_eq!(call.status, Some(200));
    assert!(call.error.is_some(), "the read error is recorded");
    assert_eq!((status, body.len()), (502, 0));
    Ok(())
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_answers_502_and_records_the_request() -> Res<()> {
    let nobody = TcpListener::bind("127.0.0.1:0").await?.local_addr()?;
    let recorder = Recorder::new();
    let base = start(
        format!("http://{nobody}"),
        false,
        None,
        None,
        Arc::clone(&recorder),
    )
    .await?;
    let (status, _, _) = exchange(&base, &chat(r#"{"model":"m","messages":[]}"#)).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!(status, 502);
    assert_eq!(
        (call.path.as_str(), call.status),
        ("/v1/chat/completions", None)
    );
    assert!(call.error.is_some(), "the connection error is recorded");
    assert!(call.request.is_some(), "the request is recorded");
    Ok(())
}

#[tokio::test]
async fn an_app_that_hangs_up_mid_reply_still_has_its_call_recorded() -> Res<()> {
    let head =
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n".to_vec();
    let chunk = b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n".to_vec();
    let recorder = Recorder::new();
    let upstream = server(vec![Answer::Endless { head, chunk }]).await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let mut app = TcpStream::connect(base.trim_start_matches("http://")).await?;
    app.write_all(chat(r#"{"model":"m","messages":[],"stream":true}"#).as_bytes())
        .await?;
    let mut answer = Vec::new();
    while find(&answer, b"\r\n\r\n").is_none() {
        if app.read_buf(&mut answer).await? == 0 {
            return Err("the proxy closed before its head".into());
        }
    }
    drop(app);
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!(call.status, Some(200));
    assert!(
        call.error
            .as_deref()
            .is_some_and(|error| error.starts_with("client closed: ")),
        "{:?}",
        call.error
    );
    assert!(
        call.reply
            .as_ref()
            .is_some_and(|reply| reply.content.starts_with('x')),
        "the reply so far is recorded: {:?}",
        call.reply
    );
    Ok(())
}
