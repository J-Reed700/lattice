//! The HTTP side: one `POST /v1/embeddings` per batch, and what its answers
//! and failures mean.

use lattice_core::shared::error::AppError;
use serde::Deserialize;

use super::launch::RunningEmbeddingServer;

/// Why a request produced no vectors.
#[derive(Debug)]
pub(super) enum Failure {
    /// Nothing usable came back from the socket: the process may be gone.
    Transport(reqwest::Error),
    /// The server answered, and the answer is the error.
    Server(AppError),
}

/// Vectors for every input, in input order, and the tokens the server
/// counted across them.
#[derive(Debug)]
pub(super) struct Embedded {
    pub(super) vectors: Vec<Vec<f32>>,
    pub(super) prompt_tokens: u64,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Item>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Item {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u64,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ServerError,
}

#[derive(Deserialize)]
struct ServerError {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    n_prompt_tokens: Option<u64>,
    #[serde(default)]
    n_ctx: Option<u64>,
}

/// Embed `inputs`, already prefixed, on `server`.
pub(super) async fn embed(
    http: &reqwest::Client,
    server: &RunningEmbeddingServer,
    inputs: &[String],
) -> Result<Embedded, Failure> {
    let _in_flight = server.request();
    let response = http
        .post(format!("{}/v1/embeddings", server.endpoint()))
        .bearer_auth(server.api_token())
        .json(&serde_json::json!({ "input": inputs }))
        .send()
        .await
        .map_err(Failure::Transport)?;
    let status = response.status();
    let body = response.bytes().await.map_err(Failure::Transport)?;
    if !status.is_success() {
        return Err(Failure::Server(server_error(status, &body)));
    }
    let response: Response = serde_json::from_slice(&body).map_err(|e| {
        Failure::Server(AppError::EmbeddingFailed {
            reason: format!("llama-server sent an embeddings answer that does not parse: {e}"),
        })
    })?;
    let prompt_tokens = response.usage.map_or(0, |usage| usage.prompt_tokens);
    in_input_order(response.data, inputs.len())
        .map(|vectors| Embedded {
            vectors,
            prompt_tokens,
        })
        .map_err(Failure::Server)
}

/// The vectors of `data` in input order, by each item's `index`, or an error
/// when they do not cover `count` inputs exactly once each.
fn in_input_order(mut data: Vec<Item>, count: usize) -> Result<Vec<Vec<f32>>, AppError> {
    data.sort_by_key(|item| item.index);
    let covered = data.len() == count && data.iter().enumerate().all(|(i, item)| item.index == i);
    if !covered {
        return Err(AppError::EmbeddingFailed {
            reason: format!(
                "llama-server answered {} embeddings for {count} inputs",
                data.len()
            ),
        });
    }
    Ok(data.into_iter().map(|item| item.embedding).collect())
}

/// The error a failed answer means. An over-long input is the chunker and
/// the server disagreeing about length: a bug to report with both numbers,
/// never a reason to retry.
fn server_error(status: reqwest::StatusCode, body: &[u8]) -> AppError {
    let parsed = serde_json::from_slice::<ErrorBody>(body).ok();
    if let Some(ServerError {
        kind,
        n_prompt_tokens,
        n_ctx,
        ..
    }) = parsed.as_ref().map(|body| &body.error)
    {
        if kind == "exceed_context_size_error" {
            let count = |n: &Option<u64>| n.map_or_else(|| "?".to_owned(), |n| n.to_string());
            return AppError::InvalidInput(format!(
                "An embedding input has {} tokens; the embedding server's window is {}. The \
                 chunker and the server disagree about length.",
                count(n_prompt_tokens),
                count(n_ctx)
            ));
        }
    }
    let message = parsed
        .map(|body| body.error.message)
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(body).into_owned());
    AppError::EmbeddingFailed {
        reason: format!("llama-server answered {status}: {message}"),
    }
}
