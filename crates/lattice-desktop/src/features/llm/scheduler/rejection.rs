//! A server's refusal of a request, told the same way whichever backend is
//! behind the port.
//!
//! The configured context window is the one measure of what a request may
//! use: it is the window the user set for the model, and nothing here asks a
//! server how it is configured. A request that does not fit is refused before
//! it is sent, with the numbers. A request that fits and is refused anyway is
//! reported with the server's own words, sanitized, and the same numbers, so
//! a server that holds less than the configured window shows itself.

use std::collections::HashSet;

use lattice_core::application::ports::llm_port::RequestSize;
use lattice_core::shared::error::AppError;
use serde_json::Value;

/// A run of at least this many words of a server's message that also occurs
/// in the request is the prompt quoted back, and is cut.
const ECHO_RUN_WORDS: usize = 6;

/// Characters of a server's own words kept in a message.
const SERVER_WORDS_SHOWN: usize = 240;

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
///
/// A size refusal of a prompt that was only estimated is
/// [`AppError::PromptExceedsWindow`], not counted exactly; one of a counted
/// prompt stays [`AppError::ModelRejected`], since the prompt fit the
/// configured window and the server's real limit is lower.
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
        // An estimated prompt the server refused as too large may simply not
        // fit: typed with its numbers, so a caller that can send less does,
        // as it would for a refusal before sending.
        if too_large {
            text.push_str(&format!(
                " The server refused it as too large. The prompt's size was estimated, not \
                 counted, so the request may not fit the configured context window of {window} \
                 after all: shorten the request. If the server also refuses requests that fit, \
                 lower the context window in Settings to match the server."
            ));
            return AppError::PromptExceedsWindow {
                prompt_tokens: size.prompt_tokens,
                window,
                counted_exactly: false,
                message: text,
            };
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
