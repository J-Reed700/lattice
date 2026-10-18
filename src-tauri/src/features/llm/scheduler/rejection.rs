//! A request's size against its context window, and a server's refusal of a
//! request, told the same way whichever backend is behind the port.
//!
//! The configured context window is the one measure of what a request may
//! use: it is the window the user set for the model, and nothing here asks a
//! server how it is configured. A request that does not fit is refused before
//! it is sent, with the numbers. A request that fits and is refused anyway is
//! reported with the server's own words, sanitized, and the same numbers, so
//! a server that holds less than the configured window shows itself.

use std::collections::HashSet;

use lattice_core::shared::error::AppError;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A run of at least this many words of a server's message that also occurs
/// in the request is the prompt quoted back, and is cut.
const ECHO_RUN_WORDS: usize = 6;

/// Characters of a server's own words kept in a message.
const SERVER_WORDS_SHOWN: usize = 240;

/// What one request asks of the context window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RequestSize {
    /// The prompt's tokens, pictures included.
    pub prompt_tokens: usize,
    /// Whether the backend's tokenizer counted the prompt's text. Otherwise
    /// the text is the estimate. Pictures are never counted by a tokenizer.
    pub counted_exactly: bool,
    /// Tokens reserved for the answer.
    pub output_tokens: u32,
    /// What the prompt's pictures cost by the model's declared geometry,
    /// already inside `prompt_tokens`.
    pub image_tokens: usize,
    /// How many pictures the prompt carries.
    pub image_count: usize,
}

impl RequestSize {
    /// Prompt and answer together: what the request holds in the window.
    pub fn total(&self) -> usize {
        self.prompt_tokens
            .saturating_add(usize::try_from(self.output_tokens).unwrap_or(usize::MAX))
    }

    /// How the prompt was measured, naming the pictures when it has any.
    fn counted(&self) -> String {
        let text = if self.counted_exactly {
            "counted exactly"
        } else {
            "estimated"
        };
        if self.image_count == 0 {
            return text.into();
        }
        let pictures = if self.image_count == 1 {
            "1 picture".to_string()
        } else {
            format!("{} pictures", self.image_count)
        };
        format!(
            "text {text}; {} tokens for {pictures} estimated from the model's declared geometry",
            self.image_tokens
        )
    }

    fn describe(&self) -> String {
        format!(
            "{} tokens: a prompt of {} ({}) and {} for the answer",
            self.total(),
            self.prompt_tokens,
            self.counted(),
            self.output_tokens
        )
    }
}

/// The refusal of a request that does not fit the configured `window`, before
/// it is sent. `size.output_tokens` is what the caller asked for. With
/// `reserved`, the caller fixed the answer's size and needs all of it;
/// otherwise the answer could have shrunk to the room left, and only a prompt
/// that leaves none is refused.
pub(crate) fn window_refusal(
    model: &str,
    size: RequestSize,
    window: usize,
    reserved: bool,
) -> AppError {
    tracing::warn!(
        model,
        prompt_tokens = size.prompt_tokens,
        counted_exactly = size.counted_exactly,
        output_tokens = size.output_tokens,
        reserved,
        window,
        "llm.request_refused"
    );
    let need = if reserved {
        format!("This request needs {}", size.describe())
    } else {
        format!(
            "This request's prompt is {} tokens ({}), which leaves no room for an answer",
            size.prompt_tokens,
            size.counted()
        )
    };
    AppError::InvalidInput(format!(
        "{need}; the configured context window is {window}. It was not sent. Shorten the \
         request, or raise the context window in Settings if the model's server holds more."
    ))
}

/// A server's message made safe to show, log and compare: whitespace
/// collapsed; every run of [`ECHO_RUN_WORDS`] or more words that also occurs
/// in `request_texts` (the prompt quoted back) replaced with "[…]"; anything
/// but printable ASCII replaced with "?"; cut at [`SERVER_WORDS_SHOWN`]
/// characters.
pub(crate) fn sanitized_message(server: &str, request_texts: &[&str]) -> String {
    let words: Vec<&str> = server.split_whitespace().collect();
    let request_words: Vec<Vec<&str>> = request_texts
        .iter()
        .map(|text| text.split_whitespace().collect())
        .collect();
    let quoted: HashSet<&[&str]> = request_words
        .iter()
        .flat_map(|words| words.windows(ECHO_RUN_WORDS))
        .collect();
    let starts_quote: Vec<bool> = words
        .windows(ECHO_RUN_WORDS)
        .map(|run| quoted.contains(run))
        .collect();
    let mut kept: Vec<String> = Vec::new();
    // Words before this index belong to a quoted run.
    let mut quoted_until = 0;
    let mut in_quote = false;
    for (index, word) in words.iter().enumerate() {
        if starts_quote.get(index).copied().unwrap_or(false) {
            quoted_until = index + ECHO_RUN_WORDS;
        }
        if index < quoted_until {
            if !in_quote {
                kept.push("[…]".into());
                in_quote = true;
            }
            continue;
        }
        in_quote = false;
        kept.push(
            word.chars()
                .map(|c| if c.is_ascii_graphic() { c } else { '?' })
                .collect(),
        );
    }
    let joined = kept.join(" ");
    if joined.chars().count() <= SERVER_WORDS_SHOWN {
        return joined;
    }
    let mut cut: String = joined.chars().take(SERVER_WORDS_SHOWN).collect();
    cut.push('…');
    cut
}

/// The SHA-256 of a server's raw message, for the log: two failures can be
/// compared without the words, which may quote the prompt, ever being written.
pub(crate) fn digest(server: &str) -> String {
    hex::encode(Sha256::digest(server.as_bytes()))
}

/// Every string a request body carries: messages, tool schemas, arguments.
/// Whatever a server quotes back can only have come from one of them.
/// Pictures are skipped: a data URI (OpenAI-compatible), Ollama's `images`
/// list and Anthropic's base64 `data` are bytes no server quotes as words.
pub(crate) fn request_texts(body: &Value) -> Vec<&str> {
    fn collect<'a>(value: &'a Value, texts: &mut Vec<&'a str>) {
        match value {
            Value::String(text) if text.starts_with("data:") => {}
            Value::String(text) => texts.push(text),
            Value::Array(items) => items.iter().for_each(|item| collect(item, texts)),
            Value::Object(fields) => {
                let base64_source = fields.get("type").and_then(Value::as_str) == Some("base64");
                fields
                    .iter()
                    .filter(|(key, _)| *key != "images" && !(base64_source && *key == "data"))
                    .for_each(|(_, item)| collect(item, texts));
            }
            _ => {}
        }
    }
    let mut texts = Vec::new();
    collect(body, &mut texts);
    texts
}

/// Whether a server's refusal says the request was too large for it: the
/// HTTP status for that, an error type naming the context, or words that say
/// so. Read only to choose which fix to suggest; the server's words are shown
/// either way.
pub(crate) fn is_size_refusal(status: Option<u16>, kind: &str, message: &str) -> bool {
    let kind = kind.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    status == Some(413)
        || kind.contains("context")
        || (message.contains("context")
            && [
                "exceed",
                "too long",
                "too large",
                "maximum",
                "length",
                "size",
                "capacity",
                "limit",
            ]
            .iter()
            .any(|word| message.contains(word)))
        || [
            "too many tokens",
            "prompt is too long",
            "input is too long",
            "kv cache",
        ]
        .iter()
        .any(|phrase| message.contains(phrase))
}

/// The error for a request `backend` refused in a way retrying cannot change:
/// it rejected the request outright, or (`twice`) failed it the same way two
/// times. `account` is the server's side, already sanitized: its status and
/// type, then its words. `request` carries the request's size and the
/// configured window when they are known.
pub(crate) fn rejected(
    backend: &str,
    twice: bool,
    account: &str,
    request: Option<(RequestSize, usize)>,
    too_large: bool,
) -> AppError {
    let mut text = format!(
        "{backend} rejected this request{}: {}.",
        if twice { " twice" } else { "" },
        account.trim_end_matches('.')
    );
    if let Some((size, window)) = request {
        text.push_str(&format!(
            " The request was {}; the configured context window is {window}.",
            size.describe()
        ));
        // Only a counted prompt shows that the request fit; an estimate can
        // read low on dense text, and then the window was the limit.
        if too_large && size.counted_exactly {
            text.push_str(&format!(
                " The server refused a request of {} tokens even though it fits the configured \
                 context window of {window}, so the server's real per-request limit is lower: \
                 lower the context window in Settings to match the server.",
                size.total()
            ));
            return AppError::ModelRejected(text);
        }
        if too_large {
            text.push_str(&format!(
                " The server refused it as too large. The prompt's size was estimated, not \
                 counted, so the request may not fit the configured context window of {window} \
                 after all: shorten the request. If the server also refuses requests that fit, \
                 lower the context window in Settings to match the server."
            ));
            return AppError::ModelRejected(text);
        }
    }
    if twice {
        text.push_str(
            " The same request would fail the same way again; the server's own log has the cause.",
        );
    }
    AppError::ModelRejected(text)
}

#[cfg(test)]
mod tests;
