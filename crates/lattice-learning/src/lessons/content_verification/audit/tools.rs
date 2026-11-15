//! The Verifier's web tools (lesson claim review §14.5), offered when the
//! course allows search and a web service is available. `search_web` returns
//! titles, links and snippets (never evidence); `fetch_page` saves the page as
//! a learning source before the model sees it, so every citable page is
//! reproducible. Rounds are bounded by the window, not a count.

use async_trait::async_trait;
use serde_json::Value;

use lattice_core::application::ports::llm_port::{
    CompletionInput, InferencePriority, SamplingOverride, ToolChoice, ToolDefinition,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::budget::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};
use lattice_core::application::services::model_contract::shown::{Drawn, Kind, Scope, Shown};
use lattice_core::application::services::model_contract::tools::{Parameters, View};
use lattice_core::application::services::model_contract::units::{Grain, Units};
use lattice_core::application::services::model_contract::{legacy, ContractCall};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::sentences;

use crate::references::web_research::{self, PlannedNeed, Room};

use super::calls::fit_text;
use super::AuditPassage;

/// One search result: what the model may choose to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// The web as the Verifier may use it. `save_page` saves the fetched page
/// as a learning source with `added_by = 'verifier'` and returns its saved
/// version as a passage over its whole text.
#[async_trait]
pub trait VerifierWeb: Send + Sync {
    /// Searches for `need`, bringing no more results than `room` holds.
    async fn search(&self, need: &PlannedNeed, room: Room<'_>) -> Result<Vec<SearchHit>>;
    async fn save_page(&self, url: &str) -> Result<AuditPassage>;
}

const SEARCH: &str = "search_web";
const FETCH: &str = "fetch_page";

/// A search result the model may fetch, shown with a handle of its own.
pub(super) struct Hit {
    title: String,
    url: String,
    snippet: String,
}

impl Kind for Hit {
    const PREFIX: &'static str = "h";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.url.clone()
    }
}

fn fetch_parameters() -> Parameters {
    Parameters::new().handle::<Hit>("hit", "The handle shown beside a search result.")
}

pub(super) fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: SEARCH.into(),
            description: format!(
                "Search for pages that could support or contradict a claim. Results are leads \
                 only: a snippet is never evidence. {}",
                web_research::NEED_GUIDE
            ),
            parameters: web_research::need_schema(),
        },
        ToolDefinition {
            name: FETCH.into(),
            description: "Fetch a page found by search_web. The page is saved to the course's \
                          sources and returned with its sentences, each beside a handle you may \
                          cite."
                .into(),
            parameters: fetch_parameters().schema().as_json().clone(),
        },
    ]
}

/// What the web tools brought in before the claims were judged.
#[derive(Debug, Default)]
pub(super) struct Research {
    /// The pages fetched and saved, in the order fetched.
    pub fetched: Vec<AuditPassage>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub web_note: Option<String>,
}

/// Lets the model search and fetch until it stops calling tools; what it
/// answers then is not read: the claims are asked for afterwards, with the
/// fetched pages shown.
pub(super) async fn research(
    llm: &dyn LLMPort,
    scope: &mut Scope,
    system: &str,
    task: String,
    reserve: usize,
    contract: ContractCall,
    web: &dyn VerifierWeb,
) -> Result<Research> {
    let too_large = || {
        AppError::WindowTooSmall(format!(
            "The Verifier's window of {} tokens cannot hold this statement's section and \
             passages with its answer.",
            llm.max_context_tokens()
        ))
    };
    let tools = definitions();
    let tool_tokens = llm.count_tokens(&serde_json::to_string(&tools).unwrap_or_default());
    let mut input = opening(system, task);
    let mut research = Research::default();
    loop {
        let room = room_left(llm, &input, tool_tokens, reserve).ok_or_else(too_large)?;
        let offer = room > 0;
        let request = legacy::completion(legacy::Completion {
            input: input.clone(),
            tools: tools.clone(),
            contract: Some(contract.clone()),
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
        });
        let response = llm.complete(&request).await?;
        research.input_tokens += response.input_tokens;
        research.output_tokens += response.output_tokens;
        if response.tool_calls.is_empty() || !offer {
            return Ok(research);
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
            llm.count_tokens(&format!(
                "[h9999] {}\n  Link: {}\n  Snippet: {}",
                result.title, result.url, result.snippet
            )) + 1
        };
        let search_room = Room {
            tokens: share,
            cost: &cost,
        };
        let mut outputs = Vec::with_capacity(calls.len());
        for (_, name, arguments) in &calls {
            let tool = Call {
                llm,
                web,
                room: search_room,
                share,
            };
            outputs.push(tool.run(name, arguments, scope, &mut research).await);
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
                return Err(too_large());
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
/// [`research`] measures each round.
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

/// `page` cut to the whole sentences that fit `tokens`.
fn cut_page(llm: &dyn LLMPort, page: AuditPassage, tokens: usize) -> AuditPassage {
    let kept = fit_text(llm, &page.text, tokens);
    if kept.len() == page.text.len() {
        return page;
    }
    let whole = sentences(&kept)
        .iter()
        .rev()
        .nth(1)
        .map_or(kept.len(), |(_, stop)| *stop);
    AuditPassage {
        text: kept.get(..whole).unwrap_or_default().to_owned(),
        ..page
    }
}

/// The search results as the model reads them: each handle, title, link and snippet.
fn hits_view(shown: &Shown<'_, Hit>) -> View {
    shown.iter().fold(View::new(), |view, (handle, hit)| {
        view.item(handle, &hit.title).nested(
            View::new()
                .fact("Link", &hit.url)
                .fact("Snippet", &hit.snippet),
        )
    })
}

/// What one round's tool calls run against.
struct Call<'a> {
    llm: &'a dyn LLMPort,
    web: &'a dyn VerifierWeb,
    room: Room<'a>,
    /// The tokens one call's result may take.
    share: usize,
}

impl Call<'_> {
    /// One tool call's result text. A failure is told to the model (and noted
    /// on the research) instead of ending the batch: the audit runs without it.
    async fn run(
        &self,
        name: &str,
        arguments: &Value,
        scope: &mut Scope,
        research: &mut Research,
    ) -> String {
        match name {
            SEARCH => self.search(arguments, scope, research).await,
            FETCH => self.fetch(arguments, scope, research).await,
            other => format!("There is no tool named {other}."),
        }
    }

    async fn search(
        &self,
        arguments: &Value,
        scope: &mut Scope,
        research: &mut Research,
    ) -> String {
        let searches = match serde_json::from_value::<PlannedNeed>(arguments.clone())
            .map(PlannedNeed::searches)
        {
            Ok(Ok(searches)) => searches,
            Ok(Err(outside)) => return format!("Search not run. {}", outside.says()),
            Err(_) => {
                return "Search not run: give a query and one of the listed kinds.".into();
            }
        };
        // A query naming several sites is one search per site, sharing
        // the room the one call has. The call fails only when every
        // search did: a site that fails leaves the others' answers.
        let room = self.room.share(searches.len());
        let mut hits: Vec<Hit> = Vec::new();
        let mut answered = false;
        let mut failure = None;
        for need in &searches {
            match self.web.search(need, room).await {
                Ok(found) => {
                    answered = true;
                    for hit in found {
                        if !hits.iter().any(|known| known.url == hit.url) {
                            hits.push(Hit {
                                title: hit.title,
                                url: hit.url,
                                snippet: hit.snippet,
                            });
                        }
                    }
                }
                Err(error) => failure = Some(error),
            }
        }
        if let (false, Some(error)) = (answered, failure) {
            research.web_note = Some(format!("Web search was unavailable: {error}"));
            return format!("Search failed: {error}");
        }
        let mut showing = scope.request();
        let shown = match showing.show(&hits) {
            Ok(shown) => shown,
            Err(error) => return format!("Search failed: {error}"),
        };
        hits_view(&shown).text()
    }

    async fn fetch(&self, arguments: &Value, scope: &mut Scope, research: &mut Research) -> String {
        let parameters = fetch_parameters();
        let picked = match parameters.read(arguments, scope).handle::<Hit>("hit") {
            Ok(picked) => picked,
            Err(error) => return error.0,
        };
        let page = match self.web.save_page(&picked.reference).await {
            Ok(page) => cut_page(self.llm, page, self.share),
            Err(error) => return format!("Could not fetch or save that page: {error}"),
        };
        let units = Units::of(&page.text, Grain::Sentence);
        let mut showing = scope.request();
        let rows = match units.shown(&mut showing) {
            Ok(shown) => units.rows(&shown),
            Err(error) => return format!("Could not show that page: {error}"),
        };
        let text = View::new().line(&page.title).line(rows).text();
        let known = research.fetched.iter().any(|seen| {
            seen.source_version_id == page.source_version_id
                && seen.start_byte == page.start_byte
                && seen.end_byte == page.end_byte
        });
        if !known {
            research.fetched.push(page);
        }
        text
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
