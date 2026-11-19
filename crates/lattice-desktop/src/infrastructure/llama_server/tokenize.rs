//! Exact token counts from a llama-server's own tokenizer (`POST /tokenize`).
//!
//! Serves the chat budget and the embedding splitter. Counts are cached by
//! content hash: a turn's final prompt is mostly the same system policy and
//! history as the turn before, so the repeated parts cost nothing.

use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::time::Duration;

use lru::LruCache;
use parking_lot::Mutex;
use serde_json::{json, Value};

use lattice_core::shared::error::{AppError, Result};

use super::RunningLlamaServer;

/// Distinct texts remembered. A turn counts a handful; this covers many
/// conversations' worth of system prompts and histories.
const CACHE_ENTRIES: usize = 256;

/// Tokenizing is fast even for a full window; a server that takes longer is
/// busy or gone.
const TOKENIZE_TIMEOUT: Duration = Duration::from_secs(10);

/// Counts tokens with one llama-server's tokenizer.
pub(crate) struct ServerTokenizer {
    http: reqwest::Client,
    url: String,
    bearer: Option<String>,
    add_special: bool,
    cache: Mutex<LruCache<(u64, usize), usize>>,
}

impl ServerTokenizer {
    /// Counts the pieces of a chat prompt. Framing is charged per message, so
    /// special tokens are left out: counting a BOS on every piece would double
    /// it. `server_root` is the server's base URL without `/v1`; `http` carries
    /// whatever authentication the server needs.
    pub fn new(http: reqwest::Client, server_root: &str) -> Self {
        Self::counting(http, server_root, None, false)
    }

    /// Counts whole embedding inputs as `/v1/embeddings` tokenizes them,
    /// special tokens included, with the server's key.
    pub fn whole_inputs(http: reqwest::Client, server: &RunningLlamaServer) -> Self {
        Self::counting(
            http,
            server.endpoint(),
            Some(server.api_token().to_owned()),
            true,
        )
    }

    fn counting(
        http: reqwest::Client,
        server_root: &str,
        bearer: Option<String>,
        add_special: bool,
    ) -> Self {
        Self {
            http,
            url: format!("{}/tokenize", server_root.trim_end_matches('/')),
            bearer,
            add_special,
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(CACHE_ENTRIES).unwrap_or(NonZeroUsize::MIN),
            )),
        }
    }

    pub async fn count(&self, text: &str) -> Result<usize> {
        if text.is_empty() && !self.add_special {
            return Ok(0);
        }
        let key = cache_key(text);
        if let Some(tokens) = self.cache.lock().get(&key) {
            return Ok(*tokens);
        }
        let mut request = self.http.post(&self.url).timeout(TOKENIZE_TIMEOUT);
        if let Some(bearer) = &self.bearer {
            request = request.bearer_auth(bearer);
        }
        let response = request
            .json(&json!({
                "content": text,
                "add_special": self.add_special,
                "parse_special": true
            }))
            .send()
            .await
            .map_err(|_| AppError::Network("The model server's tokenizer did not answer".into()))?;
        if !response.status().is_success() {
            return Err(AppError::ServiceNotAvailable(format!(
                "The model server's tokenizer returned HTTP {}",
                response.status()
            )));
        }
        let body: Value = response.json().await.map_err(|_| {
            AppError::Network("The model server's tokenizer returned an unreadable body".into())
        })?;
        let tokens = parse_token_count(&body)?;
        self.cache.lock().put(key, tokens);
        Ok(tokens)
    }
}

/// `{"tokens": [...]}`, with ids or (`with_pieces`) `{id, piece}` objects.
fn parse_token_count(body: &Value) -> Result<usize> {
    body.get("tokens")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| AppError::InvalidData("The tokenizer response had no token list".into()))
}

/// Hash plus length: two different texts colliding on both is not a
/// practical concern for a budget estimate.
fn cache_key(text: &str) -> (u64, usize) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    (hasher.finish(), text.len())
}

#[cfg(test)]
mod tests;
