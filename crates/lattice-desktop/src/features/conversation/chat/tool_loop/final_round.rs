//! The round that has to answer. The tools stay in the prompt with calls
//! forbidden, because llama.cpp renders tool schemas into the system message
//! and withdrawing them would invalidate the cached prompt. The instruction is
//! said at the end of the last tool result rather than as a new user message:
//! Qwen 3.5's template then strips earlier turns' empty thinking blocks, which
//! also changes the cached prefix. A model that calls anyway is asked once more
//! with no tools at all.

use super::written_calls::WrittenCalls;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, ThinkingMode, ToolChoice,
};
use serde_json::Value;

/// Why a round is the last one. The model is told which, so the answer does
/// not blame the wrong limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LastRound {
    /// Every tool round the turn allows is used.
    OutOfRounds,
    /// The window has no room left for another result.
    WindowFull,
    /// The turn's deadline passed. This round alone runs past it, bounded by
    /// Stop and its output limit: the turn's sources and results are kept
    /// rather than lost with an error.
    OutOfTime,
}

impl LastRound {
    /// What the model is told on the last round.
    fn instruction(self) -> &'static str {
        match self {
            Self::OutOfRounds => OUT_OF_ROUNDS_INSTRUCTION,
            Self::WindowFull => WINDOW_FULL_INSTRUCTION,
            Self::OutOfTime => OUT_OF_TIME_INSTRUCTION,
        }
    }

    /// What the answer says where it stops, when it stops to write a call it
    /// cannot make.
    pub(super) fn note(self) -> &'static str {
        match self {
            Self::OutOfRounds => super::OUT_OF_ROUNDS_NOTE,
            Self::WindowFull => super::WINDOW_FULL_NOTE,
            Self::OutOfTime => super::OUT_OF_TIME_NOTE,
        }
    }
}

/// Said to the model on the last round when the rounds ran out.
pub(super) const OUT_OF_ROUNDS_INSTRUCTION: &str = "You have used every tool round this turn allows. Tools can no longer be used, so do not write any tool calls. Answer my question now from what you have already gathered. If you could not read enough to answer fully, say what you covered and what is left, so I can ask you to continue.";

/// Said instead when the window has no room for another result.
pub(super) const WINDOW_FULL_INSTRUCTION: &str = "Your context window has no room left for more tool results. Tools can no longer be used, so do not write any tool calls. Answer my question now from what you have already gathered. If you could not read enough to answer fully, say what you covered and what is left.";

/// Said instead when the turn's deadline passed.
pub(super) const OUT_OF_TIME_INSTRUCTION: &str = "This turn has run out of time. Tools can no longer be used, so do not write any tool calls. Answer my question now from what you have already gathered. If you could not read enough to answer fully, say what you covered and what is left, so I can ask you to continue.";

/// The result of a call that never ran because the turn's deadline passed.
pub(super) const OUT_OF_TIME_RESULT: &str =
    "Not run: this turn ran out of time before this call could finish.";

/// Said instead when the model answered the last round with calls anyway.
const INSIST_INSTRUCTION: &str = "Do not call any tools: none can run now, and a tool call here ends this turn with no answer. Write your answer to my question in plain prose, now, from the tool results and notes above. If they are not enough to answer fully, say what you found and what is still missing.";

/// Make the next round the answering one: the same tools, none callable, and
/// an instruction to answer from what the turn already holds.
pub(super) fn ask_for_answer(request: &mut CompletionRequest, last_round: LastRound) {
    request.tool_choice = Some(ToolChoice::None);
    say_last(&mut request.input, last_round.instruction());
}

/// The answering round when the answer may think: the same instruction and no
/// tools in the prompt, since a thinking model with calls forbidden writes the
/// call as text instead.
pub(super) fn ask_for_answer_without_tools(request: &mut CompletionRequest, last_round: LastRound) {
    say_last(&mut request.input, last_round.instruction());
    request.tools.clear();
    // Nothing is offered, so there is nothing to choose.
    request.tool_choice = None;
}

/// The one retry after a last round that wrote only calls: no tools in the
/// prompt at all, and a firmer instruction in place of the first.
pub(super) fn insist_on_answer(request: &mut CompletionRequest, last_round: LastRound) {
    // With no tools offered, a default that turns thinking on for tool-free
    // rounds would bring back the reply this retry exists to replace.
    request.thinking = ThinkingMode::Off;
    request.tools.clear();
    // Nothing is offered, so there is nothing to choose; some providers
    // refuse a choice that names no tools.
    request.tool_choice = None;
    unsay_last(&mut request.input, last_round.instruction());
    say_last(&mut request.input, INSIST_INSTRUCTION);
}

/// Sets an instruction off from the result it is appended to.
const INSTRUCTION_SEPARATOR: &str = "\n\n---\nFrom me, not part of the result above: ";

/// Says `instruction` as the last thing the model reads, changing nothing
/// before it: at the end of the last tool result, or as a user message when
/// the prompt does not end with one. Never as a system message: llama.cpp
/// templates merge a late one into the top of the prompt.
pub(super) fn say_last(input: &mut Vec<CompletionInput>, instruction: &str) {
    match input.last_mut() {
        Some(CompletionInput::ToolResult { output, .. }) => {
            output.push_str(INSTRUCTION_SEPARATOR);
            output.push_str(instruction);
        }
        _ => input.push(CompletionInput::Message {
            role: "user".into(),
            content: instruction.into(),
        }),
    }
}

/// Take back the last round's instruction, or the firmer one that replaced
/// it, so a round asked again for another reason is told only what holds.
pub(super) fn take_back(request: &mut CompletionRequest, last_round: LastRound) {
    unsay_last(&mut request.input, last_round.instruction());
    unsay_last(&mut request.input, INSIST_INSTRUCTION);
}

/// Answer every call in `ids` that `input` holds no result for with
/// `output`. A provider refuses a transcript with a call left unanswered, so
/// a round cut off between its calls has to say what became of the rest.
pub(super) fn answer_open_calls<'i>(
    input: &mut Vec<CompletionInput>,
    ids: impl IntoIterator<Item = &'i str>,
    output: &str,
) {
    let answered: std::collections::HashSet<String> = input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::ToolResult { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let open: Vec<String> = ids
        .into_iter()
        .filter(|id| !answered.contains(*id))
        .map(str::to_owned)
        .collect();
    input.extend(open.into_iter().map(|id| CompletionInput::ToolResult {
        id,
        output: output.to_owned(),
    }));
}

/// Take back an instruction [`say_last`] gave, wherever it put it.
fn unsay_last(input: &mut Vec<CompletionInput>, instruction: &str) {
    match input.last_mut() {
        Some(CompletionInput::ToolResult { output, .. }) => {
            let said = format!("{INSTRUCTION_SEPARATOR}{instruction}");
            if output.ends_with(&said) {
                output.truncate(output.len() - said.len());
            }
        }
        Some(CompletionInput::Message { role, content })
            if role == "user" && content == instruction =>
        {
            input.pop();
        }
        _ => {}
    }
}

/// Whether a last-round reply asked for tools and said nothing else: real
/// calls with no text, or text that is only a call written out.
pub(super) fn wrote_only_calls(response: &CompletionResponse) -> bool {
    let text = response.text.trim();
    if text.is_empty() {
        return !response.tool_calls.is_empty();
    }
    written_call_start(text).is_some_and(|start| text[..start].trim().is_empty())
}

/// How tool calls look when a model writes them as text: Qwen and Hermes
/// `<tool_call>`, the XML `<function=` form, Mistral's `[TOOL_CALLS]`, and
/// the `<|tool_call` special-token spelling.
const LEAKED_CALL_MARKERS: [&str; 4] = ["<tool_call>", "<function=", "[TOOL_CALLS]", "<|tool_call"];

/// Where the first tool call written as text begins, if there is one.
pub(super) fn leaked_tool_call_start(text: &str) -> Option<usize> {
    LEAKED_CALL_MARKERS
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
}

/// Where a call written into a reply begins: at a call marker, or at the
/// start when the whole reply is a call in JSON.
pub(super) fn written_call_start(text: &str) -> Option<usize> {
    if is_json_call(text) {
        return Some(0);
    }
    leaked_tool_call_start(text)
}

/// A reply that is nothing but one call, or a list of them, in the JSON shapes
/// models write when they have no native call format to use.
fn is_json_call(text: &str) -> bool {
    match serde_json::from_str::<Value>(without_code_fence(text.trim())) {
        Ok(Value::Array(items)) => !items.is_empty() && items.iter().all(names_a_call),
        Ok(value) => names_a_call(&value),
        Err(_) => false,
    }
}

fn without_code_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    // The opening fence's line may carry a language tag.
    let body = rest
        .split_once('\n')
        .map_or("", |(_, body)| body)
        .trim_end();
    body.strip_suffix("```").unwrap_or(body).trim()
}

/// `{"name": …, "arguments": …}`, the same under `function`, or an OpenAI
/// `tool_calls` list. A bare object with a `name` is not enough: an answer can
/// be JSON too.
fn names_a_call(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("tool_calls").is_some_and(Value::is_array) {
        return true;
    }
    let function = object.get("function").and_then(Value::as_object);
    let named = object
        .get("name")
        .or_else(|| function.and_then(|function| function.get("name")))
        .is_some_and(Value::is_string);
    let has_arguments = ["arguments", "parameters"].iter().any(|key| {
        object.contains_key(*key) || function.is_some_and(|function| function.contains_key(*key))
    });
    named && has_arguments
}

/// Streams a round's text up to the first tool call written into it and holds
/// the call back. A tail that could still become a marker waits for the next
/// chunk. In the last round only, a reply that opens like JSON is held whole,
/// since a JSON call can only be told from an answer once it is complete.
pub(super) struct LeakedCallFilter {
    active: bool,
    held: String,
    stopped: bool,
    /// Whether the reply's opening has been seen and judged.
    opened: bool,
    holding_json: bool,
}

impl LeakedCallFilter {
    pub(super) fn new(written_calls: WrittenCalls<'_>) -> Self {
        Self {
            active: !matches!(written_calls, WrittenCalls::Shown),
            held: String::new(),
            stopped: false,
            opened: !matches!(written_calls, WrittenCalls::Cut),
            holding_json: false,
        }
    }

    /// The part of `chunk` that may be shown now.
    pub(super) fn push(&mut self, chunk: &str) -> String {
        if !self.active {
            return chunk.to_string();
        }
        self.held.push_str(chunk);
        if self.stopped {
            return String::new();
        }
        if !self.opened {
            match opens_like_json(&self.held) {
                None => return String::new(),
                Some(json) => {
                    self.opened = true;
                    self.holding_json = json;
                }
            }
        }
        if self.holding_json {
            return String::new();
        }
        if let Some(start) = leaked_tool_call_start(&self.held) {
            self.stopped = true;
            return self.held.drain(..start).collect();
        }
        let keep_from = self.held.len() - partial_marker_suffix_len(&self.held);
        let shown = self.held[..keep_from].to_string();
        self.held.drain(..keep_from);
        shown
    }

    /// Once the reply is complete: what is still held and is not a call, to
    /// show now, and the call written into the reply, kept back.
    pub(super) fn finish(&mut self) -> (String, String) {
        let mut held = std::mem::take(&mut self.held);
        let start = if self.stopped {
            Some(0)
        } else {
            written_call_start(&held)
        };
        self.stopped = true;
        let call = start.map_or_else(String::new, |start| held.split_off(start));
        (held, call)
    }
}

/// `None` while the reply has shown too little to tell; then whether it
/// opens like JSON (or a code fence that may hold it).
fn opens_like_json(text: &str) -> Option<bool> {
    let lead = text.trim_start();
    if lead.is_empty() || (lead.len() < 3 && "```".starts_with(lead)) {
        return None;
    }
    Some(lead.starts_with('{') || lead.starts_with('[') || lead.starts_with("```"))
}

/// Length of the longest tail of `text` that is the start of a marker.
fn partial_marker_suffix_len(text: &str) -> usize {
    LEAKED_CALL_MARKERS
        .iter()
        .flat_map(|marker| (1..marker.len()).rev().map(move |len| &marker[..len]))
        .filter(|prefix| text.ends_with(prefix))
        .map(str::len)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::ToolDefinition;

    fn json_of(value: &impl serde::Serialize) -> Value {
        serde_json::to_value(value).unwrap()
    }

    fn request_with_a_tool() -> CompletionRequest {
        CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "You are helpful.".into(),
                },
                CompletionInput::ToolResult {
                    id: "c1".into(),
                    output: "file text".into(),
                },
            ],
            tools: vec![ToolDefinition {
                name: "web_search".into(),
                description: "Search".into(),
                parameters: serde_json::json!({}),
            }],
            ..Default::default()
        }
    }

    /// Withdrawing the tools changed the prompt from its first token, so the
    /// server re-read all of it to write the answer.
    #[test]
    fn the_last_round_keeps_its_tools_and_forbids_calling_them() {
        let mut request = request_with_a_tool();
        let tools = json_of(&request.tools);
        ask_for_answer(&mut request, LastRound::OutOfRounds);
        assert_eq!(json_of(&request.tools), tools);
        assert_eq!(request.tool_choice, Some(ToolChoice::None));
    }

    /// The instruction has to be the last thing the model reads, and adding
    /// it must not change anything the model read before.
    #[test]
    fn the_instruction_ends_the_last_result_and_changes_nothing_before_it() {
        let mut request = request_with_a_tool();
        let first = json_of(&request.input[0]);
        ask_for_answer(&mut request, LastRound::OutOfRounds);
        assert_eq!(request.input.len(), 2, "no new message");
        let Some(CompletionInput::ToolResult { output, .. }) = request.input.last() else {
            panic!("the prompt still ends with the result");
        };
        assert!(output.starts_with("file text"), "{output}");
        assert!(output.ends_with(OUT_OF_ROUNDS_INSTRUCTION), "{output}");
        assert_eq!(json_of(&request.input[0]), first);
    }

    /// A late system message is merged into the top of a llama.cpp prompt,
    /// tens of thousands of tokens before the model reads its last result.
    #[test]
    fn with_no_result_to_end_the_instruction_is_the_last_user_message() {
        let mut request = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "question".into(),
            }],
            ..Default::default()
        };
        ask_for_answer(&mut request, LastRound::OutOfRounds);
        assert!(matches!(request.input.last(),
            Some(CompletionInput::Message { role, content })
                if role == "user" && content == OUT_OF_ROUNDS_INSTRUCTION));
    }

    #[test]
    fn a_thinking_answer_round_sends_no_tools() {
        let mut request = request_with_a_tool();
        ask_for_answer_without_tools(&mut request, LastRound::OutOfRounds);
        assert!(request.tools.is_empty());
        assert_eq!(request.tool_choice, None);
        assert_eq!(request.input.len(), 2, "no new message");
        assert!(matches!(request.input.last(),
            Some(CompletionInput::ToolResult { output, .. })
                if output.ends_with(OUT_OF_ROUNDS_INSTRUCTION)));
        assert_eq!(ThinkingMode::for_answer(true, &request), ThinkingMode::On);
    }

    /// Each cause takes back the instruction it gave, not the other one's.
    #[test]
    fn the_retry_sends_no_tools_and_replaces_the_instruction() {
        for last_round in [
            LastRound::OutOfRounds,
            LastRound::WindowFull,
            LastRound::OutOfTime,
        ] {
            for mut request in [
                request_with_a_tool(),
                CompletionRequest {
                    input: vec![CompletionInput::Message {
                        role: "user".into(),
                        content: "question".into(),
                    }],
                    ..request_with_a_tool()
                },
            ] {
                ask_for_answer(&mut request, last_round);
                let before = request.input.len();
                insist_on_answer(&mut request, last_round);
                assert!(request.tools.is_empty());
                assert_eq!(request.tool_choice, None);
                assert_eq!(request.thinking, ThinkingMode::Off);
                assert_eq!(request.input.len(), before, "replaced, not stacked");
                let last = json_of(request.input.last().unwrap()).to_string();
                assert!(last.contains("Do not call any tools"), "{last}");
                assert!(!last.contains(last_round.instruction()), "{last}");
            }
        }
    }

    #[test]
    fn a_reply_of_only_calls_is_recognised_in_every_shape() {
        let call = CompletionInput::ToolCall {
            id: "c1".into(),
            name: "web_search".into(),
            arguments: serde_json::json!({"query": "x"}),
        };
        let native = CompletionResponse {
            tool_calls: vec![call],
            ..Default::default()
        };
        assert!(wrote_only_calls(&native));
        for text in [
            "<tool_call>\n<function=web_search>\n<parameter=query>x</parameter>\n</function>\n</tool_call>",
            r#"{"name": "web_search", "arguments": {"query": "x"}}"#,
            r#"[{"name": "web_search", "parameters": {"query": "x"}}]"#,
            "```json\n{\"function\": {\"name\": \"web_search\", \"arguments\": \"{}\"}}\n```",
            r#"{"tool_calls": [{"id": "1"}]}"#,
        ] {
            assert!(
                wrote_only_calls(&CompletionResponse::from_text(text)),
                "{text}"
            );
        }
        for text in [
            "The answer is 42.",
            "Here is what I found. <tool_call>{}</tool_call>",
            r#"{"name": "Ada Lovelace", "born": 1815}"#,
        ] {
            assert!(
                !wrote_only_calls(&CompletionResponse::from_text(text)),
                "{text}"
            );
        }
        assert!(!wrote_only_calls(&CompletionResponse::default()));
    }

    #[test]
    fn a_tool_call_written_as_text_is_found() {
        let text = "Now the engine.\n\n<tool_call> <function=list_directory> <parameter=path> src </parameter> </function> </tool_call>";
        assert_eq!(leaked_tool_call_start(text), text.find("<tool_call>"));
        assert_eq!(
            leaked_tool_call_start("<function=read_file>"),
            Some(0),
            "the XML form without the wrapper"
        );
        assert_eq!(leaked_tool_call_start("Plain answer with a < b."), None);
    }

    #[test]
    fn the_stream_stops_at_a_call_split_across_chunks() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Cut);
        let mut shown = String::new();
        for chunk in [
            "Now the engine. <to",
            "ol_c",
            "all> <function=list_directory>",
            " more",
        ] {
            shown.push_str(&filter.push(chunk));
        }
        let (held, call) = filter.finish();
        shown.push_str(&held);
        assert_eq!(shown, "Now the engine. ");
        assert_eq!(call, "<tool_call> <function=list_directory> more");
    }

    #[test]
    fn a_lone_angle_bracket_is_released_once_it_is_not_a_call() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Cut);
        let first = filter.push("if a <");
        let second = filter.push(" b then");
        assert_eq!(format!("{first}{second}"), "if a < b then");
    }

    #[test]
    fn a_tail_that_could_have_been_a_marker_is_shown_at_the_end() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Cut);
        let shown = filter.push("so a <");
        assert_eq!(format!("{shown}{}", filter.finish().0), "so a <");
    }

    /// A call written as JSON reached the bubble before anything could tell
    /// it from an answer.
    #[test]
    fn a_call_written_as_json_never_reaches_the_bubble() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Cut);
        let mut shown = String::new();
        for chunk in [
            " {\"name\": \"web_",
            "search\", \"arguments\": {\"query\": \"x\"}}",
        ] {
            shown.push_str(&filter.push(chunk));
        }
        shown.push_str(&filter.finish().0);
        assert_eq!(shown, "");
    }

    #[test]
    fn an_answer_that_opens_like_json_is_shown_whole_at_the_end() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Cut);
        let answer = "[1] is the stronger source: it measured, the other estimated.";
        assert_eq!(filter.push(answer), "");
        assert_eq!(filter.finish().0, answer);
    }

    #[test]
    fn rounds_without_tools_stream_untouched() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Shown);
        assert_eq!(filter.push("<tool_call>"), "<tool_call>");
        assert_eq!(filter.finish(), (String::new(), String::new()));
    }

    /// A round that may call keeps a written call out of the bubble, so the
    /// loop can run it or show it after all.
    #[test]
    fn a_round_that_may_call_keeps_the_call_back() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Run(&[]));
        let mut shown = String::new();
        for chunk in ["Checking. <func", "tion=web_search>\n", "</function>"] {
            shown.push_str(&filter.push(chunk));
        }
        let (held, call) = filter.finish();
        assert_eq!(format!("{shown}{held}"), "Checking. ");
        assert_eq!(call, "<function=web_search>\n</function>");
    }

    /// Only the last round holds a reply that opens like JSON; an answer
    /// opening with a code fence still streams on a round that may call.
    #[test]
    fn a_round_that_may_call_streams_an_answer_that_opens_like_json() {
        let mut filter = LeakedCallFilter::new(WrittenCalls::Run(&[]));
        assert_eq!(
            filter.push("```rust\nfn main() {}"),
            "```rust\nfn main() {}"
        );
    }
}
