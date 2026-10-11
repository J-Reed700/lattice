//! Exact token counts from a llama-server's own tokenizer (`POST /tokenize`).
//!
//! Used where a budget decision is final, not for every count, and cached by
//! content hash: a turn's final prompt is mostly the same system policy and
//! history as the turn before, so the repeated parts cost nothing.

use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::time::Duration;

use lru::LruCache;
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::shared::error::{AppError, Result};

/// Distinct texts remembered. A turn counts a handful; this covers many
/// conversations' worth of system prompts and histories.
const CACHE_ENTRIES: usize = 256;

/// Tokenizing is fast even for a full window; a server that takes longer is
/// busy or gone, and the caller falls back to its estimate.
const TOKENIZE_TIMEOUT: Duration = Duration::from_secs(10);

/// Counts tokens with one llama-server's tokenizer.
pub(crate) struct ServerTokenizer {
    http: reqwest::Client,
    url: String,
    cache: Mutex<LruCache<(u64, usize), usize>>,
}

impl ServerTokenizer {
    /// `server_root` is the server's base URL without `/v1`; `http` carries
    /// whatever authentication the server needs.
    pub fn new(http: reqwest::Client, server_root: &str) -> Self {
        Self {
            http,
            url: format!("{}/tokenize", server_root.trim_end_matches('/')),
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(CACHE_ENTRIES).unwrap_or(NonZeroUsize::MIN),
            )),
        }
    }

    pub async fn count(&self, text: &str) -> Result<usize> {
        if text.is_empty() {
            return Ok(0);
        }
        let key = cache_key(text);
        if let Some(tokens) = self.cache.lock().get(&key) {
            return Ok(*tokens);
        }
        let response = self
            .http
            .post(&self.url)
            .timeout(TOKENIZE_TIMEOUT)
            // Special tokens are framing, which the budget already charges per
            // message; counting a BOS on every piece would double it.
            .json(&json!({"content": text, "add_special": false}))
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
pub(super) fn parse_token_count(body: &Value) -> Result<usize> {
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
