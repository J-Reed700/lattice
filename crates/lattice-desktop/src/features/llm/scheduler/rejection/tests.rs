use super::*;
use lattice_core::shared::hash::sha256_hex;
use serde_json::json;

const PROMPT: &str = "Rewrite section two so that every claim cites the passage that supports it.";

#[test]
fn a_quoted_prompt_is_cut_and_the_servers_own_words_are_kept() {
    let server = "failed to process:   Rewrite section two so that every claim cites\n the passage. Context size has been exceeded.";
    assert_eq!(
        sanitized_message(server, &[PROMPT]),
        "failed to process: […] passage. Context size has been exceeded."
    );
    // Five words in a row are not yet a quotation.
    assert_eq!(
        sanitized_message("Rewrite section two so that failed", &[PROMPT]),
        "Rewrite section two so that failed"
    );
    // Two quotations separated by the server's own words stay apart.
    let twice = "Rewrite section two so that every then: claim cites the passage that supports it.";
    assert_eq!(sanitized_message(twice, &[PROMPT]), "[…] then: […]");
}

#[test]
fn the_servers_words_are_ascii_and_bounded() {
    assert_eq!(
        sanitized_message("slot ünavailable — retry", &[]),
        "slot ?navailable ? retry"
    );
    let long = "word ".repeat(SERVER_WORDS_SHOWN);
    let shown = sanitized_message(&long, &[]);
    assert_eq!(shown.chars().count(), SERVER_WORDS_SHOWN + 1);
    assert!(shown.ends_with('…'));
    assert!(sanitized_message("", &[PROMPT]).is_empty());
}

#[test]
fn the_log_compares_failures_by_digest_not_words() {
    assert_eq!(sha256_hex("Compute error."), sha256_hex("Compute error."));
    assert_ne!(sha256_hex("Compute error."), sha256_hex("Compute error"));
    assert_eq!(sha256_hex("x").len(), 64);
}

#[test]
fn every_string_in_a_request_can_be_quoted_back() {
    let body = json!({
        "model": "m",
        "messages": [{"role": "user", "content": PROMPT}],
        "tools": [{"function": {"description": "Search the web"}}],
        "max_tokens": 10,
    });
    let texts = request_texts(&body);
    for expected in ["m", "user", PROMPT, "Search the web"] {
        assert!(texts.contains(&expected), "{expected}");
    }
}

#[test]
fn a_refusal_for_size_is_told_apart_from_other_failures() {
    for (status, kind, message) in [
        (Some(500), "server_error", "Context size has been exceeded."),
        (Some(400), "exceed_context_size_error", "request exceeds"),
        (
            Some(400),
            "invalid_request_error",
            "This model's maximum context length is 8192 tokens.",
        ),
        (
            Some(400),
            "",
            "prompt is too long: 210000 tokens > 200000 maximum",
        ),
        (Some(413), "", ""),
    ] {
        assert!(is_size_refusal(status, kind, message), "{message}");
    }
    for (status, kind, message) in [
        (Some(500), "server_error", "Compute error."),
        (Some(500), "", "Invalid input batch."),
        (
            Some(400),
            "",
            "Unable to generate parser for this template.",
        ),
    ] {
        assert!(!is_size_refusal(status, kind, message), "{message}");
    }
}

#[test]
fn a_rejection_carries_the_servers_account_the_numbers_and_the_fix() {
    let size = RequestSize {
        prompt_tokens: 110_000,
        counted_exactly: true,
        output_tokens: 8_192,
        ..RequestSize::default()
    };
    let AppError::ModelRejected(too_large) = rejected(
        "llama.cpp",
        true,
        "(500 server_error) Context size has been exceeded.",
        Some((size, 131_072)),
        true,
    ) else {
        panic!("a rejection is ModelRejected");
    };
    for expected in [
        "llama.cpp rejected this request twice: (500 server_error) Context size has been exceeded.",
        "The request was 118192 tokens",
        "refused a request of 118192 tokens even though it fits the configured context window of 131072",
        "lower the context window in Settings to match the server",
    ] {
        assert!(too_large.contains(expected), "{too_large}");
    }
    assert!(!too_large.contains(".."), "{too_large}");

    let unknown = rejected(
        "llama.cpp",
        true,
        "(500) Compute error.",
        Some((size, 131_072)),
        false,
    )
    .to_string();
    assert!(
        unknown.contains("would fail the same way again"),
        "{unknown}"
    );
    assert!(!unknown.contains("lower the context window"), "{unknown}");

    let outright = rejected("llama.cpp", false, "(422) unprocessable", None, false).to_string();
    assert_eq!(
        outright,
        "llama.cpp rejected this request: (422) unprocessable."
    );
}

#[test]
fn a_size_refusal_of_an_estimated_prompt_claims_no_lower_server_limit() {
    let size = RequestSize {
        prompt_tokens: 110_000,
        counted_exactly: false,
        output_tokens: 8_192,
        ..RequestSize::default()
    };
    let error = rejected(
        "llama.cpp",
        false,
        "(400 exceed_context_size_error) Context size has been exceeded.",
        Some((size, 131_072)),
        true,
    );
    // Typed with its numbers, so a caller that can send less does.
    let AppError::PromptExceedsWindow {
        prompt_tokens: 110_000,
        window: 131_072,
        counted_exactly: false,
        message: estimated,
    } = error
    else {
        panic!("an estimated prompt refused for size is PromptExceedsWindow, got {error:?}");
    };
    for expected in [
        "a prompt of 110000 (estimated)",
        "estimated, not counted, so the request may not fit the configured context window of \
         131072 after all: shorten the request",
    ] {
        assert!(estimated.contains(expected), "{estimated}");
    }
    assert!(!estimated.contains("real per-request limit"), "{estimated}");
}

#[test]
fn a_pictures_bytes_are_not_request_text() {
    let body = json!({
        "messages": [{"role":"user","content":[
            {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}},
            {"type":"text","text":"What is this?"}
        ]}],
        "ollama": {"role":"user","content":"Hi","images":["BBBB"]},
        "anthropic": {"type":"image","source":{"type":"base64","media_type":"image/png","data":"CCCC"}}
    });
    let texts = request_texts(&body);
    assert!(texts.contains(&"What is this?"));
    assert!(texts.contains(&"image/png"));
    for bytes in ["data:image/png;base64,AAAA", "BBBB", "CCCC"] {
        assert!(!texts.contains(&bytes), "{texts:?}");
    }
}

#[test]
fn structured_output_faults_are_classified_generically_across_backends() {
    for (kind, message) in [
        (
            " (structured_output_failed)",
            "structured output was incomplete (finish_reason=length); increase the output budget",
        ),
        (
            " (500 structured_output_failed)",
            "structured output truncated at max_tokens",
        ),
        (
            "",
            "failed to generate valid JSON: EOF while parsing object",
        ),
        (
            "json_validate_failed",
            "output cut off before closing JSON object",
        ),
    ] {
        assert_eq!(
            classify_structured_output(kind, message),
            Some(StructuredOutputFault::CutOff),
            "kind={kind:?} message={message:?}"
        );
    }

    for (kind, message) in [
        (
            " (structured_output_failed)",
            "model did not return valid JSON: duplicate JSON key: plan",
        ),
        (
            " (structured_output_failed)",
            "schema validation failed: missing required field 'edits'",
        ),
        (
            " (structured_output_failed)",
            "unexpected token at line 3 column 9",
        ),
        (
            "json_validate_failed",
            "Failed to generate JSON. Please adjust your prompt.",
        ),
        ("", "model did not return valid JSON: trailing comma"),
    ] {
        assert_eq!(
            classify_structured_output(kind, message),
            Some(StructuredOutputFault::UnusableOutput),
            "kind={kind:?} message={message:?}"
        );
    }

    for (kind, message) in [
        (" (structured_output_failed)", "grammar compilation error"),
        (
            "invalid_request_error",
            "Invalid schema for response_format 'response': unsupported keyword",
        ),
        (
            "invalid_request_error",
            "json_schema: Schema is too complex",
        ),
        ("grammar_error", "failed to compile"),
    ] {
        assert_eq!(
            classify_structured_output(kind, message),
            Some(StructuredOutputFault::SchemaRejected),
            "kind={kind:?} message={message:?}"
        );
    }

    for (kind, message) in [
        (" (500 server_error)", "Compute error."),
        (" (500)", "Invalid input batch."),
        (
            " (400 exceed_context_size_error)",
            "Context size has been exceeded.",
        ),
        (
            "invalid_request_error",
            "This model's maximum context length is 8192 tokens.",
        ),
        (" (400)", "Unable to generate parser for this template."),
        (
            "structured_output_failed",
            "Context size has been exceeded.",
        ),
    ] {
        assert_eq!(
            classify_structured_output(kind, message),
            None,
            "kind={kind:?} message={message:?}"
        );
    }
}

#[test]
fn streamed_error_types_infer_http_status_when_code_is_omitted() {
    assert_eq!(inferred_status(Some(502), "server_error"), Some(502));
    assert_eq!(inferred_status(None, " (server_error)"), Some(500));
    assert_eq!(inferred_status(None, " (internal_error)"), Some(500));
    assert_eq!(inferred_status(None, " (rate_limit_error)"), Some(429));
    assert_eq!(inferred_status(None, " (overloaded_error)"), Some(503));
    assert_eq!(inferred_status(None, " (request_timeout)"), Some(408));
    assert_eq!(
        inferred_status(None, " (exceed_context_size_error)"),
        Some(413)
    );
    assert_eq!(inferred_status(None, " (invalid_request_error)"), Some(400));
    assert_eq!(inferred_status(None, " (authentication_error)"), Some(401));
    assert_eq!(inferred_status(None, " (permission_error)"), Some(403));
    assert_eq!(inferred_status(None, " (structured_output_failed)"), None);
    assert_eq!(inferred_status(None, ""), None);
}
