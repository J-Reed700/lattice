use super::*;
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
    assert_eq!(digest("Compute error."), digest("Compute error."));
    assert_ne!(digest("Compute error."), digest("Compute error"));
    assert_eq!(digest("x").len(), 64);
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
fn a_prompt_that_fills_the_window_is_refused_with_its_numbers() {
    let size = RequestSize {
        prompt_tokens: 140_000,
        counted_exactly: false,
        ..RequestSize::default()
    };
    let refused = window_refusal("m", size, 131_072);
    assert!(matches!(refused, AppError::InvalidInput(_)));
    let told = refused.to_string();
    for expected in [
        "prompt is 140000 tokens (estimated), which leaves no room for an answer",
        "configured context window is 131072",
        "not sent",
    ] {
        assert!(told.contains(expected), "{told}");
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
    let estimated = rejected(
        "llama.cpp",
        false,
        "(400 exceed_context_size_error) Context size has been exceeded.",
        Some((size, 131_072)),
        true,
    )
    .to_string();
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
fn a_refusal_names_the_pictures_and_how_they_were_charged() {
    let size = RequestSize {
        prompt_tokens: 8_300,
        counted_exactly: true,
        output_tokens: 0,
        image_tokens: 2_052,
        image_count: 2,
    };
    let told = window_refusal("m", size, 8_192).to_string();
    assert!(
        told.contains(
            "prompt is 8300 tokens (text counted exactly; 2052 tokens for 2 pictures estimated \
             from the model's declared geometry)"
        ),
        "{told}"
    );
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
