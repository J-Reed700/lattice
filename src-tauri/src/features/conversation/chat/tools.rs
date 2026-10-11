use super::*;
use crate::features::conversation::chat::ports::ChatRetrieval;

pub(super) const CORE_TOOL_NAMES: [&str; 3] = ["semantic_search", "get_document", "list_documents"];
pub(super) const OPTIONAL_BUILTIN_TOOL_NAMES: [&str; 4] = [
    "web_search",
    "fetch_url_content",
    "wiki_search",
    "wiki_summary",
];
pub(super) const WIKI_OPTIONAL_TOOL_NAMES: [&str; 2] = ["wiki_search", "wiki_summary"];

pub(super) fn build_optional_tool_allowlist(
    tool_preferences: Option<&ToolPreferences>,
) -> Option<HashSet<String>> {
    tool_preferences
        .and_then(|preferences| preferences.enabled_tools.as_ref())
        .map(|enabled_tools| {
            enabled_tools
                .iter()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
}

/// Whether a tool stays offered once retrieval has grounded the turn.
///
/// Initial retrieval is a candidate set, not proof that it can answer the
/// question. Scoped document reads and searches stay for recovery, and — when
/// the turn carries web pages — the ability to open one of them: retrieval
/// reads the top pages itself but cites more than it reads, and without this
/// the model can see a link that plainly holds the answer and have no way to
/// follow it.
///
/// Deep research also keeps the searches the turn allows. Its first round is
/// where it starts, and the prompt asks the model to search again for what
/// that round left uncovered; without them a later round could only reopen
/// links the first one found. Each is still offered only when the turn's
/// allowlist has it.
pub(super) fn kept_with_grounded_context(
    tool_name: &str,
    can_open_pages: bool,
    deep_research: bool,
) -> bool {
    matches!(tool_name, "semantic_search" | "get_document")
        || (tool_name == "fetch_url_content" && can_open_pages)
        || (deep_research && matches!(tool_name, "web_search" | "wiki_search" | "wiki_summary"))
}

/// Whether a deep-research turn may search the web again after its first
/// round: the model can call tools and the turn allows web search. Only then
/// does the prompt ask it to.
pub(super) fn deep_research_searches_again(
    search_flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    supports_tools: bool,
) -> bool {
    search_flags.deep_research_mode
        && supports_tools
        && optional_builtin_tool_allowed(
            "web_search",
            build_optional_tool_allowlist(tool_preferences).as_ref(),
            false,
        )
}

/// Whether an optional built-in tool may be offered this turn.
///
/// `fetch_url_content` gets one exception to the explicit-allowlist rule:
/// opening a page the turn already knows about — cited by an earlier turn and
/// carried as conversation context, or returned by this turn's own search — is
/// not a web search. The prompt names it as the way to re-read such a page, so
/// withholding the tool behind the web-search toggle hands the model an
/// instruction it cannot follow and it either errors on the call or searches
/// the web again for a page it already had.
pub(super) fn optional_builtin_tool_allowed(
    tool_name: &str,
    allowlist: Option<&HashSet<String>>,
    allow_url_fetch: bool,
) -> bool {
    if tool_name == "fetch_url_content" && allow_url_fetch {
        return true;
    }
    allowlist.is_some_and(|list| list.contains(tool_name))
}

pub(super) fn build_llm_tool_definitions(
    container: &dyn ChatRetrieval,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    tool_preferences: Option<&ToolPreferences>,
    custom_tools: &[CustomToolSettingsDto],
    allow_url_fetch: bool,
) -> Vec<crate::application::ports::ToolDefinition> {
    if !llm.supports_tool_calling() {
        return Vec::new();
    }

    let enabled_custom_tools: std::collections::HashMap<String, &CustomToolSettingsDto> =
        custom_tools
            .iter()
            .filter(|tool| tool.enabled)
            .map(|tool| (tool.name.clone(), tool))
            .collect();

    let optional_allowlist = build_optional_tool_allowlist(tool_preferences);

    let registry = container.tools();
    let domain_tools = registry.list_tools();
    let mut definitions: Vec<crate::application::ports::ToolDefinition> = domain_tools
        .iter()
        .filter(|tool| {
            let tool_name = tool.name.as_str();
            if CORE_TOOL_NAMES.contains(&tool_name) {
                return true;
            }

            if OPTIONAL_BUILTIN_TOOL_NAMES.contains(&tool_name) {
                return optional_builtin_tool_allowed(
                    tool_name,
                    optional_allowlist.as_ref(),
                    allow_url_fetch,
                );
            }

            if !enabled_custom_tools.contains_key(tool_name) {
                // Skip stale custom tool definitions that are no longer configured.
                return false;
            }

            optional_allowlist
                .as_ref()
                .is_none_or(|allowlist| allowlist.contains(tool_name))
        })
        .map(|t| crate::application::ports::ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        })
        .collect();

    let existing_names: HashSet<String> =
        definitions.iter().map(|tool| tool.name.clone()).collect();

    definitions.extend(
        enabled_custom_tools
            .values()
            .filter(|tool| !existing_names.contains(&tool.name))
            .filter(|tool| {
                optional_allowlist
                    .as_ref()
                    .is_none_or(|allowlist| allowlist.contains(&tool.name))
            })
            .map(|tool| crate::application::ports::ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Query text passed to this custom endpoint"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Requested max result count",
                            "default": tool.default_max_results,
                            "minimum": 1,
                            "maximum": 100
                        }
                    },
                    "required": ["query"]
                }),
            }),
    );

    definitions
}
