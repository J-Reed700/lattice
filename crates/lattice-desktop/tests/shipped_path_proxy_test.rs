//! The recording proxy against a scripted server: what the app receives,
//! and what each call records. The module is the example's, compiled here
//! because CI runs no example's tests.

#[allow(dead_code)]
#[path = "../examples/shipped_path_eval/proxy.rs"]
mod proxy;

use proxy::{serve, start, upstream_client, CallRecord, Recorder};
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

/// The proxy module's result type, as the example defines it.
type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The window the app is shown, and the server's own.
const REPORTED: u64 = 16_384;
const SERVED: u64 = 32_768;

const GET_PROPS: &str = "GET /props HTTP/1.1\r\nHost: localhost\r\n\r\n";

/// A `/props` answer that promises 100 body bytes and closes after 9.
const CUT_PROPS: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"default";

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

/// A loopback address where nothing listens, with the socket that holds its
/// port: until the socket is dropped, connections to the address are refused
/// and the port stays taken.
fn refusing_address() -> Res<(TcpSocket, SocketAddr)> {
    let socket = TcpSocket::new_v4()?;
    socket.bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    let address = socket.local_addr()?;
    Ok((socket, address))
}

/// An answer with this status, content type and body.
fn answer(status: &str, content_type: &str, body: &str) -> Answer {
    Answer::Once(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes(),
    )
}

/// `200 OK` with this content type and body.
fn ok(content_type: &str, body: &str) -> Answer {
    answer("200 OK", content_type, body)
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

/// An app that sends its request and hangs up before the answer: every write
/// to it fails.
struct HungUp(io::Cursor<Vec<u8>>);

impl AsyncRead for HungUp {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(context, buf)
    }
}

impl AsyncWrite for HungUp {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// The one call the proxy records when the app sends `request` and hangs up
/// before the answer, with the server at `upstream` and `reported` as the
/// window `/props` shows the app.
async fn recorded_for_an_app_that_hung_up(
    upstream: &str,
    reported: Option<u64>,
    request: &str,
) -> Res<CallRecord> {
    let recorder = Recorder::new();
    let client = upstream_client()?;
    let app = HungUp(io::Cursor::new(request.as_bytes().to_vec()));
    let served = serve(app, &client, upstream, false, reported, None, &recorder).await;
    match recorder.since(0).as_slice() {
        [call] => Ok(call.clone()),
        calls => Err(format!(
            "the proxy recorded {} calls; serve returned {served:?}",
            calls.len()
        )
        .into()),
    }
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
    let recorder = Recorder::new();
    let upstream = server(vec![
        props(SERVED),
        Answer::Once(CUT_PROPS.as_bytes().to_vec()),
    ])
    .await?;
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
    let (held, nobody) = refusing_address()?;
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
    drop(held);
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

#[tokio::test]
async fn an_app_that_hangs_up_before_the_props_answer_still_has_its_call_recorded() -> Res<()> {
    let upstream = server(vec![props(SERVED)]).await?;
    let call = recorded_for_an_app_that_hung_up(&upstream, Some(REPORTED), GET_PROPS).await?;
    assert_eq!((call.path.as_str(), call.status), ("/props", Some(200)));
    assert!(
        call.error
            .as_deref()
            .is_some_and(|error| error.starts_with("client closed: ")),
        "{:?}",
        call.error
    );
    Ok(())
}

#[tokio::test]
async fn an_app_that_hangs_up_before_the_502_for_a_broken_props_body_keeps_the_read_error(
) -> Res<()> {
    let upstream = server(vec![Answer::Once(CUT_PROPS.as_bytes().to_vec())]).await?;
    let call = recorded_for_an_app_that_hung_up(&upstream, Some(REPORTED), GET_PROPS).await?;
    assert_eq!((call.path.as_str(), call.status), ("/props", Some(200)));
    assert!(
        call.error
            .as_deref()
            .is_some_and(|error| !error.is_empty() && !error.starts_with("client closed: ")),
        "{:?}",
        call.error
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_chat_whose_app_hung_up_before_the_head_records_the_hang_up() -> Res<()> {
    let refused = answer(
        "400 Bad Request",
        "application/json",
        r#"{"error":"too long"}"#,
    );
    let upstream = server(vec![refused]).await?;
    let request = chat(r#"{"model":"m","messages":[]}"#);
    let call = recorded_for_an_app_that_hung_up(&upstream, None, &request).await?;
    assert_eq!(
        (call.path.as_str(), call.status),
        ("/v1/chat/completions", Some(400))
    );
    assert!(
        call.error
            .as_deref()
            .is_some_and(|error| error.starts_with("client closed: ")),
        "{:?}",
        call.error
    );
    assert!(call.request.is_some(), "the request is recorded");
    Ok(())
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_for_an_app_that_hung_up_keeps_the_connection_error(
) -> Res<()> {
    let (held, nobody) = refusing_address()?;
    let request = chat(r#"{"model":"m","messages":[]}"#);
    let call =
        recorded_for_an_app_that_hung_up(&format!("http://{nobody}"), None, &request).await?;
    assert_eq!(
        (call.path.as_str(), call.status),
        ("/v1/chat/completions", None)
    );
    assert!(
        call.error
            .as_deref()
            .is_some_and(|error| !error.is_empty() && !error.starts_with("client closed: ")),
        "{:?}",
        call.error
    );
    assert!(call.request.is_some(), "the request is recorded");
    drop(held);
    Ok(())
}

#[tokio::test]
async fn a_refused_chat_reaches_the_app_and_records_the_server_s_reason() -> Res<()> {
    let reason = r#"{"error":"too long"}"#;
    let recorder = Recorder::new();
    let upstream = server(vec![answer("400 Bad Request", "application/json", reason)]).await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let (status, _, body) = exchange(&base, &chat(r#"{"model":"m","messages":[]}"#)).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!((status, body.as_slice()), (400, reason.as_bytes()));
    assert_eq!(
        (call.status, call.error.as_deref()),
        (Some(400), Some(reason))
    );
    Ok(())
}

#[tokio::test]
async fn a_chat_answered_in_full_records_its_reply_and_no_error() -> Res<()> {
    let completion = r#"{"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}]}"#;
    let recorder = Recorder::new();
    let upstream = server(vec![ok("application/json", completion)]).await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let (status, _, body) = exchange(&base, &chat(r#"{"model":"m","messages":[]}"#)).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!((status, body.as_slice()), (200, completion.as_bytes()));
    assert_eq!((call.status, call.error.as_deref()), (Some(200), None));
    assert_eq!(
        call.reply
            .as_ref()
            .map(|reply| (reply.content.as_str(), reply.finish_reason.as_deref())),
        Some(("hi", Some("stop")))
    );
    Ok(())
}

#[tokio::test]
async fn a_redirect_reaches_the_app_as_the_server_s_answer() -> Res<()> {
    let (held, nobody) = refusing_address()?;
    let moved = format!(
        "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{nobody}/props\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let recorder = Recorder::new();
    let upstream = server(vec![Answer::Once(moved.into_bytes())]).await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let (status, _, _) = exchange(&base, GET_PROPS).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!((status, call.status), (307, Some(307)));
    drop(held);
    Ok(())
}

#[tokio::test]
async fn a_refused_chat_whose_body_breaks_off_keeps_the_read_error() -> Res<()> {
    let cut = r#"{"error""#;
    let refusal = format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{cut}"
    );
    let recorder = Recorder::new();
    let upstream = server(vec![Answer::Once(refusal.into_bytes())]).await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let (status, _, _) = exchange(&base, &chat(r#"{"model":"m","messages":[]}"#)).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!((status, call.status), (400, Some(400)));
    let error = call.error.as_deref().unwrap_or_default();
    assert!(
        !error.is_empty() && error != cut,
        "the call's error is {error:?}, not the error from reading the body"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_request_other_than_a_chat_records_its_status_and_no_error() -> Res<()> {
    let reason = r#"{"error":"Loading model"}"#;
    let recorder = Recorder::new();
    let upstream = server(vec![answer(
        "503 Service Unavailable",
        "application/json",
        reason,
    )])
    .await?;
    let base = start(upstream, false, None, None, Arc::clone(&recorder)).await?;
    let (status, _, body) = exchange(&base, GET_PROPS).await?;
    let calls = recorded(&recorder, 1).await?;
    let call = calls.first().ok_or("no call was recorded")?;
    assert_eq!((status, body.as_slice()), (503, reason.as_bytes()));
    assert_eq!((call.status, call.error.as_deref()), (Some(503), None));
    Ok(())
}
