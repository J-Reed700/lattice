//! The Verifier's web tools (lesson claim review §14.5), offered only when
//! the course allows the Verifier to search and a web service is available.
//!
//! `search_web` takes a typed need, so the source finder routes it to the
//! finders that hold its kind of source, and returns titles, links and
//! snippets; a snippet is never evidence. `fetch_page` has the app fetch a page and save it as a learning
//! source added by the Verifier, and only then shows the model the saved
//! text, so every page a verdict can quote is reproducible.
//!
//! The rounds are bounded by the window, not by a count: each round's
//! results are cut to the room the planner leaves, and once no room is left
//! the model is asked to answer without tools.

use async_trait::async_trait;
use serde_json::{json, Value};

use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, InferencePriority, SamplingOverride, ToolChoice,
    ToolDefinition,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::budget::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};
use lattice_core::shared::error::Result;

use crate::features::learning::references::web_research::{self, PlannedNeed, Room};

use super::calls::{cut_off, fit_text, Answer, AskError};
use super::AuditPassage;

/// One search result: what the model may choose to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// The web as the Verifier may use it. `save_page` saves the fetched page
/// as a learning source with `added_by = 'verifier'` and returns its saved
/// version as a passage over its whole text.
#[async_trait]
pub(crate) trait VerifierWeb: Send + Sync {
    /// Searches for `need`, bringing no more results than `room` holds.
    async fn search(&self, need: &PlannedNeed, room: Room<'_>) -> Result<Vec<SearchHit>>;
    async fn save_page(&self, url: &str) -> Result<AuditPassage>;
}

const SEARCH: &str = "search_web";
const FETCH: &str = "fetch_page";

pub(super) fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: SEARCH.into(),
            description: format!(
                "Search for pages that could support or contradict a claim. Results are leads \
                 only: a snippet is never evidence. {}",
                web_research::NEED_GUIDE
            ),
            parameters: web_research::need_schema(None),
        },
        ToolDefinition {
            name: FETCH.into(),
            description: "Fetch a page found by search_web. The page is saved to the course's \
                          sources and returned with a label (W1, W2, ...) you may cite and quote."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Ask with the web tools offered, until the model answers.
pub(super) async fn ask(
    llm: &dyn LLMPort,
    system: &str,
    task: String,
    schema: Value,
    reserve: usize,
    web: &dyn VerifierWeb,
) -> std::result::Result<Answer, AskError> {
    let tools = definitions();
    let tool_tokens = llm.count_tokens(&serde_json::to_string(&tools).unwrap_or_default());
    let mut input = opening(system, task);
    let mut answer = Answer::default();
    loop {
        let room = room_left(llm, &input, tool_tokens, reserve).ok_or(AskError::TooLarge)?;
        let offer = room > 0;
        let request = CompletionRequest {
            input: input.clone(),
            tools: tools.clone(),
            json_schema: Some(schema.clone()),
            tool_choice: Some(if offer {
                ToolChoice::Auto
            } else {
                ToolChoice::None
            }),
            max_output_tokens: Some(u32::try_from(reserve).unwrap_or(u32::MAX)),
            sampling: Some(SamplingOverride::deterministic()),
            priority: InferencePriority::Verification,
            no_time_limit: true,
            ..Default::default()
        };
        let response = llm.complete(&request).await.map_err(AskError::Model)?;
        answer.input_tokens += response.input_tokens;
        answer.output_tokens += response.output_tokens;
        if response.tool_calls.is_empty() || !offer {
            answer.cut_off = cut_off(&response.finish_reason);
            answer.text = response.text;
            return Ok(answer);
        }
        input.extend(response.replay.iter().cloned());
        let calls: Vec<(String, String, Value)> = response
            .tool_calls
            .iter()
            .filter_map(|call| match call {
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some((id.clone(), name.clone(), arguments.clone())),
                _ => None,
            })
            .collect();
        let share = room / calls.len().max(1);
        let cost = |candidate: &web_research::Candidate| {
            let result = &candidate.result;
            llm.count_tokens(&hit_json(&result.title, &result.url, &result.snippet).to_string()) + 1
        };
        let search_room = Room {
            tokens: share,
            cost: &cost,
        };
        let mut outputs = Vec::with_capacity(calls.len());
        for (_, name, arguments) in &calls {
            outputs.push(run(name, arguments, web, search_room, &mut answer).await);
        }
        let mut cut: Vec<String> = outputs
            .iter()
            .map(|output| fit_text(llm, output, share))
            .collect();
        // Measured, not assumed: shrink the results until the next round's
        // request fits with its answer.
        loop {
            let mut next = input.clone();
            next.extend(calls.iter().zip(&cut).map(|((id, _, _), output)| {
                CompletionInput::ToolResult {
                    id: id.clone(),
                    output: output.clone(),
                }
            }));
            if room_left(llm, &next, tool_tokens, reserve).is_some() {
                input = next;
                break;
            }
            if cut.iter().all(String::is_empty) {
                return Err(AskError::TooLarge);
            }
            cut = cut
                .iter()
                .map(|output| fit_text(llm, output, llm.count_tokens(output) / 2))
                .collect();
        }
    }
}

/// A tool-using batch's first request: the policy and the batch.
fn opening(system: &str, task: String) -> Vec<CompletionInput> {
    vec![
        CompletionInput::Message {
            role: "system".into(),
            content: system.into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: task,
        },
    ]
}

/// Whether a tool-using batch's first request fits with its answer, as
/// [`ask`] measures each round.
pub(super) fn opening_fits(
    llm: &dyn LLMPort,
    system: &str,
    task: String,
    tool_tokens: usize,
    reserve: usize,
) -> bool {
    room_left(llm, &opening(system, task), tool_tokens, reserve).is_some()
}

/// The room left for tool results once `input`, the tool schemas and the
/// answer's reservation are paid for; `None` when the request no longer
/// fits with its answer.
fn room_left(
    llm: &dyn LLMPort,
    input: &[CompletionInput],
    tool_tokens: usize,
    reserve: usize,
) -> Option<usize> {
    let input_tokens: usize = input.iter().map(|item| input_tokens(llm, item)).sum();
    let capacity =
        ModelCapacity::new(llm.model_name(), llm.max_context_tokens()).with_output_limit(reserve);
    let allocation =
        BudgetAllocation::plan(&capacity, &BudgetRequest::new(0, tool_tokens, input_tokens))
            .ok()?;
    (allocation.output_reserved >= reserve).then_some(allocation.available)
}

fn input_tokens(llm: &dyn LLMPort, item: &CompletionInput) -> usize {
    match item {
        CompletionInput::Message { role, content } => {
            llm.count_tokens(role) + llm.count_tokens(content)
        }
        CompletionInput::ToolCall {
            name, arguments, ..
        } => llm.count_tokens(name) + llm.count_tokens(&arguments.to_string()),
        CompletionInput::ToolResult { output, .. } => llm.count_tokens(output),
        CompletionInput::Native { value } => llm.count_tokens(&value.to_string()),
        CompletionInput::Multimodal { .. } => 0,
    }
}

/// One tool call's result text. A failure is told to the model (and noted
/// on the audit) instead of ending the batch: the audit runs without it.
async fn run(
    name: &str,
    arguments: &Value,
    web: &dyn VerifierWeb,
    room: Room<'_>,
    answer: &mut Answer,
) -> String {
    let argument = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    match name {
        SEARCH => match serde_json::from_value::<PlannedNeed>(arguments.clone())
            .ok()
            .and_then(PlannedNeed::searches)
        {
            None => "Search not run: give a query and rewordings of 3 to 240 characters besides \
                     any site: operators, a site that is an address, one of the listed kinds, \
                     dates as YYYY-MM-DD, and for asOf the page's address as identifier."
                .into(),
            // A query naming several sites is one search per site, sharing
            // the room the one call has. The call fails only when every
            // search did: a site that fails leaves the others' answers.
            Some(searches) => {
                let room = room.share(searches.len());
                let mut hits: Vec<SearchHit> = Vec::new();
                let mut answered = false;
                let mut failure = None;
                for need in &searches {
                    match web.search(need, room).await {
                        Ok(found) => {
                            answered = true;
                            for hit in found {
                                if !hits.iter().any(|known| known.url == hit.url) {
                                    hits.push(hit);
                                }
                            }
                        }
                        Err(error) => failure = Some(error),
                    }
                }
                if let (false, Some(error)) = (answered, failure) {
                    answer.web_note = Some(format!("Web search was unavailable: {error}"));
                    return format!("Search failed: {error}");
                }
                json!(hits
                    .iter()
                    .map(|hit| hit_json(&hit.title, &hit.url, &hit.snippet))
                    .collect::<Vec<_>>())
                .to_string()
            }
        },
        FETCH => match web.save_page(&argument("url")).await {
            Ok(page) => {
                let label = format!("W{}", answer.fetched.len() + 1);
                let text = format!("[{label}] {}\n{}", page.title, page.text);
                answer.fetched.push((label, page));
                text
            }
            Err(error) => format!("Could not fetch or save that page: {error}"),
        },
        other => format!("There is no tool named {other}."),
    }
}

/// One search result as the model reads it.
fn hit_json(title: &str, url: &str, snippet: &str) -> Value {
    json!({"title": title, "url": url, "snippet": snippet})
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
