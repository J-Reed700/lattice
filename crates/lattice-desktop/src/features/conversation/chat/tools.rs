use super::*;
use crate::features::conversation::chat::ports::ChatRetrieval;

pub(super) const CORE_TOOL_NAMES: [&str; 4] = [
    "semantic_search",
    "get_document",
    "list_documents",
    "list_attachments",
];
/// The core tools that search or list the library. An Explorer turn reads the
/// library only when the user turns it on, so only then is it offered these.
pub(super) const LIBRARY_TOOL_NAMES: [&str; 2] = ["semantic_search", "list_documents"];
pub(super) const OPTIONAL_BUILTIN_TOOL_NAMES: [&str; 7] = [
    "web_search",
    "fetch_url_content",
    "find_sources",
    "follow_links",
    "find_image",
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
/// question. Scoped document reads, attachment listing, and searches stay for
/// recovery, and — when the turn carries web pages — the ability to open one of
/// them: retrieval reads the top pages itself but cites more than it reads, and
/// without this the model can see a link that plainly holds the answer and have
/// no way to follow it.
///
/// `find_image` stays too: it illustrates the answer rather than looking for
/// evidence, so grounding does not make it redundant, and it reaches this
/// filter only when the turn's web gate let it through.
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
    matches!(
        tool_name,
        "semantic_search" | "get_document" | "list_attachments" | "find_image"
    ) || (matches!(tool_name, "fetch_url_content" | "follow_links") && can_open_pages)
        || (deep_research
            && matches!(
                tool_name,
                "web_search" | "find_sources" | "wiki_search" | "wiki_summary"
            ))
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
///
/// `find_image` rides the web-search toggle instead of a name of its own: it
/// reaches the web the same way, so a turn with the web off never sees it and
/// one with the web on always does, keeping the tool list stable turn to turn.
pub(super) fn optional_builtin_tool_allowed(
    tool_name: &str,
    allowlist: Option<&HashSet<String>>,
    allow_url_fetch: bool,
) -> bool {
    if (tool_name == "fetch_url_content" || tool_name == "follow_links") && allow_url_fetch {
        return true;
    }
    let gate = if tool_name == "find_image" {
        "web_search"
    } else {
        tool_name
    };
    allowlist.is_some_and(|list| {
        list.contains(gate)
            || (matches!(gate, "find_sources" | "follow_links") && list.contains("web_search"))
    })
}

pub(super) fn build_llm_tool_definitions(
    container: &dyn ChatRetrieval,
    llm: &Arc<dyn lattice_core::application::ports::LLMPort>,
    tool_preferences: Option<&ToolPreferences>,
    custom_tools: &[CustomToolSettingsDto],
    allow_url_fetch: bool,
) -> Vec<lattice_core::application::ports::ToolDefinition> {
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
    let mut definitions: Vec<lattice_core::application::ports::ToolDefinition> = domain_tools
        .into_iter()
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
        .map(Into::into)
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
            .map(|tool| lattice_core::application::ports::ToolDefinition {
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
