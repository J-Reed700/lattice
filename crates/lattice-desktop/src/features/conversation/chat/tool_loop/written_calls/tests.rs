use super::*;
use serde_json::json;

fn tool(name: &str, properties: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: format!("The {name} tool."),
        parameters: json!({"type": "object", "properties": properties}),
    }
}

fn offered() -> Vec<ToolDefinition> {
    vec![
        tool(
            "web_search",
            json!({"query": {"type": "string"}, "max_results": {"type": "integer"}}),
        ),
        tool("fetch_url_content", json!({"url": {"type": "string"}})),
    ]
}

/// Each recovered call's tool and arguments, in order.
fn calls_of(recovered: &Recovered) -> Vec<(&str, &Value)> {
    recovered
        .calls
        .iter()
        .map(|call| match call {
            CompletionInput::ToolCall {
                name, arguments, ..
            } => (name.as_str(), arguments),
            other => panic!("not a call: {other:?}"),
        })
        .collect()
}

/// llama.cpp issue #30118: Qwen3.6-35B-A3B left out the opener, and the
/// server returned the whole call as content.
#[test]
fn qwen_xml_without_its_opener_is_read_as_a_call() {
    let text = "<function=web_search>\n<parameter=query>\nBrooklyn Bridge opening year date\n</parameter>\n</function>\n</tool_call>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(recovered.text, "");
    assert_eq!(
        calls_of(&recovered),
        [(
            "web_search",
            &json!({"query": "Brooklyn Bridge opening year date"})
        )]
    );
}

#[test]
fn qwen_xml_with_its_opener_is_read_as_a_call() {
    let text = "<tool_call>\n<function=fetch_url_content>\n<parameter=url>\nhttps://pages.example/a\n</parameter>\n</function>\n</tool_call>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(
        calls_of(&recovered),
        [(
            "fetch_url_content",
            &json!({"url": "https://pages.example/a"})
        )]
    );
}

#[test]
fn hermes_json_is_read_as_a_call() {
    let text = "<tool_call>\n{\"name\": \"web_search\", \"arguments\": {\"query\": \"bridge\", \"max_results\": 3}}\n</tool_call>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(
        calls_of(&recovered),
        [("web_search", &json!({"query": "bridge", "max_results": 3}))]
    );
}

#[test]
fn two_calls_in_one_reply_are_both_read() {
    let text = "<tool_call>\n<function=web_search>\n<parameter=query>\nbridge\n</parameter>\n</function>\n</tool_call>\n<tool_call>\n{\"name\": \"fetch_url_content\", \"arguments\": \"{\\\"url\\\": \\\"https://pages.example/b\\\"}\"}\n</tool_call>";
    let recovered = recover(text, &offered()).expect("two calls");
    assert_eq!(
        calls_of(&recovered),
        [
            ("web_search", &json!({"query": "bridge"})),
            (
                "fetch_url_content",
                &json!({"url": "https://pages.example/b"})
            ),
        ]
    );
    let ids: Vec<_> = recovered
        .calls
        .iter()
        .filter_map(|call| match call {
            CompletionInput::ToolCall { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_ne!(ids[0], ids[1], "each call has an id of its own");
}

#[test]
fn a_call_to_a_tool_not_offered_is_not_read() {
    let text = "<function=delete_everything>\n<parameter=path>\n/\n</parameter>\n</function>";
    assert!(recover(text, &offered()).is_none());
    // One unknown call keeps the whole reply as text.
    let mixed = format!(
        "<tool_call>{{\"name\": \"web_search\", \"arguments\": {{\"query\": \"x\"}}}}</tool_call>\n{text}"
    );
    assert!(recover(&mixed, &offered()).is_none());
}

#[test]
fn malformed_xml_is_not_read() {
    for text in [
        // No closing function tag.
        "<function=web_search>\n<parameter=query>\nbridge\n</parameter>\n",
        // A parameter that never closes runs into the next one.
        "<function=web_search>\n<parameter=query>\nbridge\n<parameter=max_results>\n3\n</parameter>\n</function>",
        // No closing angle bracket on the parameter's name.
        "<function=web_search>\n<parameter=query\n</function>",
        // Only the wrapper, with no call inside.
        "<tool_call>\n</tool_call>",
        // A marker it does not read.
        "[TOOL_CALLS][{\"name\": \"web_search\", \"arguments\": {\"query\": \"x\"}}]",
    ] {
        assert!(recover(text, &offered()).is_none(), "{text}");
    }
}

#[test]
fn a_parameter_typed_other_than_string_is_decoded() {
    let text = "<function=web_search>\n<parameter=query>\n1883\n</parameter>\n<parameter=max_results>\n5\n</parameter>\n<parameter=untyped>\n[1, 2]\n</parameter>\n</function>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(
        calls_of(&recovered),
        [(
            "web_search",
            &json!({"query": "1883", "max_results": 5, "untyped": "[1, 2]"})
        )],
        "a string stays a string even when it reads as JSON"
    );
}

#[test]
fn only_one_framing_newline_is_taken_off_a_value() {
    let text = "<function=web_search>\n<parameter=query>\n\n  two lines\nof query  \n\n</parameter>\n</function>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(
        calls_of(&recovered),
        [(
            "web_search",
            &json!({"query": "\n  two lines\nof query  \n"})
        )]
    );
}

#[test]
fn text_before_the_call_is_kept() {
    let text = "Let me look that up.\n\n<function=web_search>\n<parameter=query>\nbridge\n</parameter>\n</function>";
    let recovered = recover(text, &offered()).expect("a call");
    assert_eq!(recovered.text, "Let me look that up.");
    assert_eq!(calls_of(&recovered).len(), 1);
}

#[test]
fn an_answer_that_goes_on_after_a_call_is_not_read() {
    let text = "Qwen writes a call like <function=web_search> and then </function>, which the server reads.";
    assert!(recover(text, &offered()).is_none());
    assert!(recover("It opened in 1883 [1].", &offered()).is_none());
}

#[test]
fn a_recovered_reply_replays_its_text_then_its_calls() {
    let mut response = CompletionResponse::from_text(
        "Checking.\n<function=web_search>\n<parameter=query>\nbridge\n</parameter>\n</function>",
    );
    assert!(run_written_calls(&mut response, &offered()));
    assert_eq!(response.text, "Checking.");
    assert_eq!(response.tool_calls.len(), 1);
    assert!(matches!(
        &response.replay[..],
        [
            CompletionInput::Message { role, content },
            CompletionInput::ToolCall { name, .. },
        ] if role == "assistant" && content == "Checking." && name == "web_search"
    ));
}

#[test]
fn a_reply_with_calls_of_its_own_is_left_alone() {
    let mut response = CompletionResponse {
        text: "<function=web_search>\n</function>".into(),
        tool_calls: vec![CompletionInput::ToolCall {
            id: "native".into(),
            name: "web_search".into(),
            arguments: json!({"query": "bridge"}),
        }],
        ..Default::default()
    };
    assert!(!run_written_calls(&mut response, &offered()));
    assert_eq!(response.tool_calls.len(), 1);
}
