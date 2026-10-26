//! A recording proxy between Lattice and llama-server.
//!
//! The chat stack does not report what it sent the model, nor the server's
//! cached-versus-evaluated prompt split (`timings.cache_n` / `prompt_n`), and
//! the LLM adapter belongs to other work in flight. Sitting on the wire sees
//! both without changing a line of it: every `/v1/chat/completions` body is
//! kept verbatim, and the streamed reply is passed through byte for byte while
//! a copy is parsed for its text, tool calls, finish reason and timings.
//!
//! It speaks just enough HTTP/1.1 for one local client: a request with a
//! `Content-Length` body, answered with `Connection: close` so the end of the
//! body is the end of the connection.

use crate::Res;
use futures::StreamExt;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// One request that crossed the proxy.
#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    pub seq: usize,
    pub method: String,
    pub path: String,
    /// Milliseconds since the proxy started, when the request arrived.
    pub started_ms: u64,
    pub duration_ms: u64,
    /// Until the first body byte came back: prompt processing plus queueing.
    pub first_byte_ms: Option<u64>,
    pub status: Option<u16>,
    /// The chat request exactly as forwarded (after any thinking switch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<Value>,
    /// Whether this proxy added `enable_thinking: false` to the request.
    pub thinking_switch_added: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<Reply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a chat completion answered, reassembled from the stream.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Reply {
    pub content: String,
    pub reasoning_content: String,
    pub tool_calls: Vec<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    /// The model the reply says served it, which may differ from the one
    /// requested (an alias, a snapshot): what a result was measured on.
    pub model: Option<String>,
    /// llama-server's own accounting: `cache_n`, `prompt_n`, `prompt_ms`,
    /// `predicted_n`, `predicted_ms`.
    pub timings: Option<Value>,
}

pub struct Recorder {
    started: Instant,
    calls: Mutex<Vec<CallRecord>>,
    in_flight: AtomicUsize,
    last_activity: Mutex<Instant>,
}

impl Recorder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            calls: Mutex::new(Vec::new()),
            in_flight: AtomicUsize::new(0),
            last_activity: Mutex::new(Instant::now()),
        })
    }

    pub fn len(&self) -> usize {
        self.calls.lock().len()
    }

    /// Calls recorded from `from` on, in arrival order.
    pub fn since(&self, from: usize) -> Vec<CallRecord> {
        self.calls.lock().iter().skip(from).cloned().collect()
    }

    /// Waits until nothing has crossed the proxy for `quiet`, or `cap` runs
    /// out. Background work a turn leaves behind — the grounding check, memory
    /// consolidation — would otherwise run into the next turn's timings.
    /// Returns whether it went quiet.
    pub async fn wait_quiet(&self, quiet: Duration, cap: Duration) -> bool {
        let deadline = Instant::now() + cap;
        loop {
            let idle = self.in_flight.load(Ordering::SeqCst) == 0
                && self.last_activity.lock().elapsed() >= quiet;
            if idle {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn touch(&self) {
        *self.last_activity.lock() = Instant::now();
    }
}

/// Starts the proxy on a free loopback port and returns its base URL.
pub async fn start(upstream: String, thinking_off: bool, recorder: Arc<Recorder>) -> Res<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let client = reqwest::Client::builder().build()?;
    let upstream = upstream.trim_end_matches('/').to_owned();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let client = client.clone();
            let upstream = upstream.clone();
            let recorder = Arc::clone(&recorder);
            tokio::spawn(async move {
                recorder.in_flight.fetch_add(1, Ordering::SeqCst);
                recorder.touch();
                if let Err(error) = serve(stream, &client, &upstream, thinking_off, &recorder).await
                {
                    eprintln!("proxy: {error}");
                }
                recorder.touch();
                recorder.in_flight.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Ok(format!("http://{address}"))
}

struct Head {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut BufReader<TcpStream>) -> Res<Head> {
    let mut line = String::new();
    stream.read_line(&mut line).await?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("empty request line")?.to_owned();
    let path = parts
        .next()
        .ok_or("request line without a path")?
        .to_owned();
    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        line.clear();
        stream.read_line(&mut line).await?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let (name, value) = (name.trim().to_owned(), value.trim().to_owned());
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse()?;
            }
            if name.eq_ignore_ascii_case("transfer-encoding") {
                return Err("chunked request bodies are not supported".into());
            }
            headers.push((name, value));
        }
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    Ok(Head {
        method,
        path,
        headers,
        body,
    })
}

/// Adds `chat_template_kwargs.enable_thinking = false` unless the request
/// already says what it wants, so the recipe can hold thinking off for a run
/// without overriding a request that sets it explicitly.
pub fn switch_thinking_off(body: &mut Value) -> bool {
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    let kwargs = object
        .entry("chat_template_kwargs")
        .or_insert_with(|| Value::Object(Map::new()));
    match kwargs.as_object_mut() {
        Some(kwargs) if !kwargs.contains_key("enable_thinking") => {
            kwargs.insert("enable_thinking".into(), Value::Bool(false));
            true
        }
        _ => false,
    }
}

async fn serve(
    stream: TcpStream,
    client: &reqwest::Client,
    upstream: &str,
    thinking_off: bool,
    recorder: &Recorder,
) -> Res<()> {
    let mut stream = BufReader::new(stream);
    let head = read_request(&mut stream).await?;
    let started_ms = recorder.elapsed_ms();
    let started = Instant::now();
    let is_completions = head.path.ends_with("/chat/completions");
    // Anthropic's Messages API, for a run whose answering model is Claude.
    let is_messages = head.path.ends_with("/v1/messages");
    let is_chat = head.method == "POST" && (is_completions || is_messages);
    let mut body = head.body;
    let mut request_json = None;
    let mut thinking_switch_added = false;
    if is_chat {
        let mut value: Value = serde_json::from_slice(&body)?;
        if thinking_off && is_completions {
            thinking_switch_added = switch_thinking_off(&mut value);
            body = serde_json::to_vec(&value)?;
        }
        request_json = Some(value);
    }

    let method = reqwest::Method::from_bytes(head.method.as_bytes())?;
    let mut forward = client.request(method, format!("{upstream}{}", head.path));
    for (name, value) in &head.headers {
        let skip = ["host", "content-length", "connection", "accept-encoding"]
            .iter()
            .any(|hop| name.eq_ignore_ascii_case(hop));
        if !skip {
            forward = forward.header(name.as_str(), value.as_str());
        }
    }
    let mut record = CallRecord {
        seq: 0,
        method: head.method.clone(),
        path: head.path.clone(),
        started_ms,
        duration_ms: 0,
        first_byte_ms: None,
        status: None,
        request: request_json,
        thinking_switch_added,
        reply: None,
        error: None,
    };
    let client_stream = stream.get_mut();
    match forward.body(body).send().await {
        Ok(response) => {
            let status = response.status();
            record.status = Some(status.as_u16());
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_owned();
            let head = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            );
            client_stream.write_all(head.as_bytes()).await?;
            let mut copy = Vec::new();
            let mut chunks = response.bytes_stream();
            while let Some(chunk) = chunks.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        record.error = Some(error.to_string());
                        break;
                    }
                };
                if record.first_byte_ms.is_none() {
                    record.first_byte_ms = Some(elapsed_ms(started));
                }
                recorder.touch();
                if is_chat {
                    copy.extend_from_slice(&chunk);
                }
                // A client that gave up still gets its call recorded.
                if let Err(error) = client_stream.write_all(&chunk).await {
                    record.error = Some(format!("client closed: {error}"));
                    break;
                }
            }
            if is_chat {
                let streamed = content_type.contains("event-stream");
                record.reply = Some(if is_messages {
                    parse_messages_reply(&copy, streamed)
                } else {
                    parse_reply(&copy, streamed)
                });
            }
        }
        Err(error) => {
            record.error = Some(error.to_string());
            let reply =
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            client_stream.write_all(reply.as_bytes()).await?;
        }
    }
    client_stream.shutdown().await.ok();
    record.duration_ms = elapsed_ms(started);
    let mut calls = recorder.calls.lock();
    record.seq = calls.len();
    calls.push(record);
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Reassembles a chat completion from an SSE stream or a JSON body.
pub fn parse_reply(bytes: &[u8], streamed: bool) -> Reply {
    let text = String::from_utf8_lossy(bytes);
    let mut reply = Reply::default();
    if !streamed {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            let message = value.pointer("/choices/0/message");
            reply.content = string_at(message, "content");
            reply.reasoning_content = string_at(message, "reasoning_content");
            reply.tool_calls = message
                .and_then(|m| m.get("tool_calls"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            reply.finish_reason = value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                .map(str::to_owned);
            reply.usage = value.get("usage").cloned();
            reply.timings = value.get("timings").cloned();
            reply.model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        return reply;
    }
    // Tool calls stream as fragments keyed by index: a name once, then the
    // arguments a piece at a time.
    let mut calls: Vec<(String, String, String)> = Vec::new();
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(usage) = event.get("usage").filter(|u| !u.is_null()) {
            reply.usage = Some(usage.clone());
        }
        if let Some(timings) = event.get("timings").filter(|t| !t.is_null()) {
            reply.timings = Some(timings.clone());
        }
        if let Some(model) = event.get("model").and_then(Value::as_str) {
            reply.model = Some(model.to_owned());
        }
        let Some(choice) = event.pointer("/choices/0") else {
            continue;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            reply.finish_reason = Some(reason.to_owned());
        }
        let delta = choice.get("delta");
        reply.content.push_str(&string_at(delta, "content"));
        reply
            .reasoning_content
            .push_str(&string_at(delta, "reasoning_content"));
        for call in delta
            .and_then(|d| d.get("tool_calls"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = call
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
                .unwrap_or(calls.len());
            while calls.len() <= index {
                calls.push(Default::default());
            }
            if let Some(slot) = calls.get_mut(index) {
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    slot.0 = id.to_owned();
                }
                slot.1.push_str(&string_at(call.get("function"), "name"));
                slot.2
                    .push_str(&string_at(call.get("function"), "arguments"));
            }
        }
    }
    reply.tool_calls = calls
        .into_iter()
        .map(|(id, name, arguments)| json!({"id": id, "name": name, "arguments": arguments}))
        .collect();
    reply
}

/// What a Messages API reply answered, reassembled from its events or read
/// from its body: text, thinking, tool calls, the stop reason, the usage
/// and the model it was served by.
pub fn parse_messages_reply(bytes: &[u8], streamed: bool) -> Reply {
    let text = String::from_utf8_lossy(bytes);
    let mut reply = Reply::default();
    if !streamed {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            for block in value
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                messages_block_text(&mut reply, block);
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    reply.tool_calls.push(json!({
                        "id": string_at(Some(block), "id"),
                        "name": string_at(Some(block), "name"),
                        "arguments": block.get("input").map(Value::to_string).unwrap_or_default(),
                    }));
                }
            }
            reply.finish_reason = value
                .get("stop_reason")
                .and_then(Value::as_str)
                .map(str::to_owned);
            reply.usage = value.get("usage").cloned();
            reply.model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        return reply;
    }
    // A tool call streams as a block start (id, name), then its input a
    // fragment at a time, keyed by the block's index.
    let mut calls: Vec<(u64, String, String, String)> = Vec::new();
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let index = event
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                let message = event.get("message");
                reply.model = message
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                reply.usage = message.and_then(|m| m.get("usage")).cloned();
            }
            Some("content_block_start") => {
                if let Some(block) = event.get("content_block") {
                    messages_block_text(&mut reply, block);
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        calls.push((
                            index,
                            string_at(Some(block), "id"),
                            string_at(Some(block), "name"),
                            String::new(),
                        ));
                    }
                }
            }
            Some("content_block_delta") => {
                let delta = event.get("delta");
                match delta.and_then(|d| d.get("type")).and_then(Value::as_str) {
                    Some("text_delta") => reply.content.push_str(&string_at(delta, "text")),
                    Some("thinking_delta") => reply
                        .reasoning_content
                        .push_str(&string_at(delta, "thinking")),
                    Some("input_json_delta") => {
                        if let Some(call) = calls.iter_mut().find(|call| call.0 == index) {
                            call.3.push_str(&string_at(delta, "partial_json"));
                        }
                    }
                    _ => {}
                }
            }
            Some("message_delta") => {
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    reply.finish_reason = Some(reason.to_owned());
                }
                if let Some(usage) = event.get("usage") {
                    reply.usage = Some(match reply.usage.take() {
                        Some(Value::Object(mut start)) => {
                            if let Value::Object(end) = usage {
                                start.extend(end.clone());
                            }
                            Value::Object(start)
                        }
                        _ => usage.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    reply.tool_calls = calls
        .into_iter()
        .map(|(_, id, name, arguments)| json!({"id": id, "name": name, "arguments": arguments}))
        .collect();
    reply
}

/// The text or thinking a Messages API content block carries.
fn messages_block_text(reply: &mut Reply, block: &Value) {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => reply.content.push_str(&string_at(Some(block), "text")),
        Some("thinking") => reply
            .reasoning_content
            .push_str(&string_at(Some(block), "thinking")),
        _ => {}
    }
}

fn string_at(value: Option<&Value>, key: &str) -> String {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
