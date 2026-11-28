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

/// How a server-reported structured-output failure should be handled across
/// backends, regardless of backend-specific phrasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredOutputFault {
    /// Generation exhausted its output token budget before closing the
    /// structured value (`finish_reason=length`, `max_tokens`, incomplete,
    /// truncated). Surface as [`AppError::AnswerCutOff`] so callers can retry
    /// with a larger budget or smaller batch.
    CutOff,
    /// The server rejected the JSON schema or grammar itself before generating
    /// (invalid/unsupported schema, grammar compilation error). Retrying the
    /// same request cannot succeed.
    SchemaRejected,
    /// The model attempted to generate structured output and produced malformed
    /// or schema-violating output (invalid JSON, duplicate keys, missing
    /// required fields, wrong types, enum/validation failure). Surface as
    /// [`AppError::ModelAnswerUnusable`] so loop-breaking retry and
    /// contract-level recovery can react.
    UnusableOutput,
}

impl StructuredOutputFault {
    /// Converts a recoverable structured-output fault into the typed
    /// [`AppError`] callers recover on, leaving [`Self::SchemaRejected`] to the
    /// backend's refusal path.
    pub(crate) fn into_error(self, description: String) -> Option<AppError> {
        match self {
            Self::CutOff => Some(AppError::AnswerCutOff(description)),
            Self::UnusableOutput => Some(AppError::ModelAnswerUnusable(description)),
            Self::SchemaRejected => None,
        }
    }
}

/// Classifies a server-reported error as a structured-output fault when its
/// error `kind` or `message` identifies a structured-output / JSON-schema
/// generation failure.
///
/// Context-size refusals always return `None` so they stay on the window-size
/// refusal path. Once a failure is identified as structured-output related,
/// schema/grammar rejections map to [`StructuredOutputFault::SchemaRejected`],
/// length/budget cutoffs map to [`StructuredOutputFault::CutOff`], and every
/// other generation or validation failure defaults to
/// [`StructuredOutputFault::UnusableOutput`] so new server phrasings do not
/// fall through to fatal [`AppError::InvalidState`].
pub(crate) fn classify_structured_output(
    kind: &str,
    message: &str,
) -> Option<StructuredOutputFault> {
    if is_size_refusal(None, kind, message) {
        return None;
    }
    let kind_lower = kind.to_ascii_lowercase();
    let msg_lower = message.to_ascii_lowercase();

    let kind_is_structured = [
        "structured_output",
        "json_validate",
        "failed_generation",
        "invalid_json",
        "grammar",
    ]
    .iter()
    .any(|needle| kind_lower.contains(needle));

    let msg_is_structured = [
        "structured output",
        "structured_output",
        "did not return valid json",
        "failed to generate valid json",
        "failed to generate json",
        "failed to validate json",
        "duplicate json key",
        "json_validate_failed",
    ]
    .iter()
    .any(|needle| msg_lower.contains(needle))
        || ((msg_lower.contains("response_format") || msg_lower.contains("json_schema"))
            && [
                "invalid",
                "unsupported",
                "compil",
                "grammar",
                "too complex",
                "failed",
                "schema",
            ]
            .iter()
            .any(|needle| msg_lower.contains(needle)));

    if !kind_is_structured && !msg_is_structured {
        return None;
    }

    let schema_rejected = kind_lower.contains("grammar")
        || [
            "grammar",
            "invalid schema",
            "invalid_json_schema",
            "invalid json schema",
            "unsupported schema",
            "schema is too complex",
            "schema compilation",
            "failed to compile schema",
            "failed to parse schema",
            "output_config.format",
            "schemas contains",
        ]
        .iter()
        .any(|needle| msg_lower.contains(needle));
    if schema_rejected {
        return Some(StructuredOutputFault::SchemaRejected);
    }

    let cut_off = [
        "finish_reason=length",
        "finish_reason: length",
        "\"finish_reason\":\"length\"",
        "\"finish_reason\": \"length\"",
        "max_tokens",
        "max_output_tokens",
        "output budget",
        "token limit",
        "token budget",
        "incomplete",
        "truncated",
        "cut off",
        "unexpected end of json",
        "eof while parsing",
    ]
    .iter()
    .any(|needle| msg_lower.contains(needle));
    if cut_off {
        return Some(StructuredOutputFault::CutOff);
    }

    Some(StructuredOutputFault::UnusableOutput)
}

/// Infers a standard HTTP status class for a streamed or structured error event
/// when the server omitted a numeric `code` and identified the error only by
/// its `type`/`kind` label (for example `{"type": "server_error", ...}`).
///
/// An explicit `status` is always preserved unchanged.
pub(crate) fn inferred_status(status: Option<u16>, kind: &str) -> Option<u16> {
    if status.is_some() {
        return status;
    }
    let kind = kind.to_ascii_lowercase();
    if kind.contains("rate_limit") || kind.contains("too_many_requests") {
        Some(429)
    } else if kind.contains("timeout") || kind.contains("timed_out") {
        Some(408)
    } else if kind.contains("overloaded")
        || kind.contains("service_unavailable")
        || kind.contains("unavailable")
        || kind.contains("bad_gateway")
    {
        Some(503)
    } else if kind.contains("server_error")
        || kind.contains("internal_error")
        || kind.contains("engine_error")
        || kind.contains("api_error")
    {
        Some(500)
    } else if kind.contains("exceed_context_size")
        || kind.contains("context_length_exceeded")
        || kind.contains("payload_too_large")
    {
        Some(413)
    } else if kind.contains("invalid_request")
        || kind.contains("bad_request")
        || kind.contains("unprocessable")
    {
        Some(400)
    } else if kind.contains("authentication") || kind.contains("unauthorized") {
        Some(401)
    } else if kind.contains("permission") || kind.contains("forbidden") {
        Some(403)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
