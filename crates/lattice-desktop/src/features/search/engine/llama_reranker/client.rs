//! The HTTP side: one `POST /v1/rerank` per query, and what its answers and
//! failures mean.

use std::time::Duration;

use lattice_core::shared::error::AppError;
use lattice_core::shared::text::safe_truncate;
use serde::Deserialize;

use crate::features::search::engine::reranker::RerankResult;
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Tokens the model's rerank prompt takes around a query and a passage, with
/// room to spare.
const PROMPT_TOKENS: usize = 128;

/// Why a request produced no scores.
#[derive(Debug)]
pub(super) enum Failure {
    /// Nothing usable came back from the socket: the process may be gone, or
    /// the request ran out of time.
    Transport(reqwest::Error),
    /// The server answered, and the answer is the error.
    Server(AppError),
}

#[derive(Deserialize)]
struct Response {
    results: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    index: usize,
    relevance_score: f32,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ServerError,
}

#[derive(Deserialize)]
struct ServerError {
    #[serde(default)]
    message: String,
}

/// The query and each passage cut to fit one slot's window, counting a token
/// a character, which no common script exceeds: the query to a quarter of the
/// window, each passage to what the query and the prompt leave.
pub(super) fn fitted(
    query: &str,
    documents: Vec<String>,
    window_tokens: u32,
) -> (String, Vec<String>) {
    let window = window_tokens as usize;
    let query = safe_truncate(query, window / 4);
    let room = window
        .saturating_sub(PROMPT_TOKENS + query.chars().count())
        .max(1);
    let documents = documents
        .into_iter()
        .map(|document| {
            if document.chars().count() <= room {
                document
            } else {
                safe_truncate(&document, room)
            }
        })
        .collect();
    (query, documents)
}

/// Score `documents` against `query` on `server`, within `timeout`: one
/// result per document, by its index in `documents`.
pub(super) async fn rerank(
    http: &reqwest::Client,
    server: &RunningLlamaServer,
    query: &str,
    documents: &[String],
    timeout: Duration,
) -> Result<Vec<RerankResult>, Failure> {
    let _in_flight = server.request();
    let response = http
        .post(format!("{}/v1/rerank", server.endpoint()))
        .bearer_auth(server.api_token())
        .timeout(timeout)
        .json(&serde_json::json!({ "query": query, "documents": documents }))
        .send()
        .await
        .map_err(Failure::Transport)?;
    let status = response.status();
    let body = response.bytes().await.map_err(Failure::Transport)?;
    if !status.is_success() {
        return Err(Failure::Server(server_error(status, &body)));
    }
    let response: Response = serde_json::from_slice(&body).map_err(|e| {
        Failure::Server(AppError::Other(format!(
            "llama-server sent a rerank answer that does not parse: {e}"
        )))
    })?;
    Ok(response
        .results
        .into_iter()
        .map(|item| RerankResult {
            index: item.index,
            score: item.relevance_score,
        })
        .collect())
}

/// The error a failed answer means, in the server's words when it gave any.
fn server_error(status: reqwest::StatusCode, body: &[u8]) -> AppError {
    let message = serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .map(|body| body.error.message)
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(body).into_owned());
    AppError::Other(format!(
        "llama-server answered {status} to a rerank: {message}"
    ))
}
