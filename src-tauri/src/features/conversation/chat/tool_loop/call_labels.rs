//! How a tool call is named, labelled and answered from memory: the parts of
//! running one that are about words, not about running it.

use crate::features::conversation::chat::fetch_memory::{self, Recall};
use crate::features::conversation::chat::turn_record::TurnStepKind;
use lattice_core::application::ports::ToolDefinition;
use std::collections::HashSet;

pub(super) fn canonical_tool_name(tool_name: &str) -> &str {
    match tool_name {
        "browser.search" => "web_search",
        "browser.open" | "browser.fetch" => "fetch_url_content",
        "repo_browser.search" => "semantic_search",
        "repo_browser.read" => "get_document",
        "repo_browser.list" => "list_documents",
        _ => tool_name,
    }
}

pub(super) fn is_tool_allowed(tool_name: &str, tools_ref: Option<&[ToolDefinition]>) -> bool {
    tools_ref.is_some_and(|tools| tools.iter().any(|tool| tool.name.as_str() == tool_name))
}

pub(super) fn available_tool_names(tools_ref: Option<&[ToolDefinition]>) -> Vec<String> {
    let mut names = tools_ref
        .map(|tools| {
            tools
                .iter()
                .map(|tool| tool.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return vec!["none".to_string()];
    }
    let mut seen = HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names.sort();
    names
}

/// Which kind of step a tool call is, so the timeline groups a document search
/// with the retrieval pipeline's rather than filing it under "tool".
pub(super) fn tool_step_kind(tool: &str) -> TurnStepKind {
    match tool {
        "semantic_search" => TurnStepKind::SearchDocuments,
        "get_document" | "list_documents" | "list_attachments" => TurnStepKind::OpenDocument,
        "web_search" => TurnStepKind::WebSearch,
        "fetch_url_content" => TurnStepKind::ReadPage,
        "wiki_search" | "wiki_summary" => TurnStepKind::Wiki,
        _ => TurnStepKind::Tool,
    }
}

/// A tool call's arguments in one line, for the step's detail.
///
/// The interesting arguments are short and named the same way across tools; the
/// rest is noise a reader cannot act on, and `TurnRecorder` clips whatever gets
/// through.
pub(super) fn tool_argument_summary(arguments: &serde_json::Value) -> String {
    const INTERESTING: [&str; 7] = [
        "query",
        "url",
        "document_id",
        "offset",
        "page",
        "path",
        "pattern",
    ];
    let Some(object) = arguments.as_object() else {
        return arguments.to_string();
    };
    let summary = INTERESTING
        .iter()
        .filter_map(|key| object.get(*key).map(|value| (key, value)))
        .map(|(key, value)| match value.as_str() {
            Some(text) => format!("{key}: {text}"),
            None => format!("{key}: {value}"),
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if summary.is_empty() {
        arguments.to_string()
    } else {
        summary
    }
}

/// What the model is told when it asks for a page this turn already read.
///
/// Says where the text already is, because "already fetched" alone invites the
/// model to conclude the content was lost and ask a third way.
pub(super) fn recalled_page_notice(url: &str, recall: &Recall) -> String {
    match recall {
        Recall::AlreadyWhole { word_count } => format!(
            "Not fetched again: {url} was already read in full this turn ({word_count} words) and its complete text is in your context above. There is nothing more on that page. Answer from it, or choose a different source.",
        ),
        Recall::Remainder {
            text,
            shown_chars,
            next: None,
            unread: None,
        } => format!(
            "Continuation of {url} from character {shown_chars}. This part runs to the end of the page; with the earlier parts you have the whole page:\n\n{text}",
        ),
        Recall::Remainder {
            text,
            shown_chars,
            next: None,
            unread: Some(unread),
        } => format!(
            "Part of {url} from character {shown_chars} to the end of the page. The part starting at character {unread} has not been given to you yet: call fetch_url_content with this url and offset={unread} to read it.\n\n{text}",
        ),
        Recall::Remainder {
            text,
            shown_chars,
            next: Some(next),
            ..
        } => format!(
            "Continuation of {url}: characters {shown_chars} to {next}, as much as this result has room for. The page goes on: call fetch_url_content with this url and offset={next} for the next part.\n\n{text}",
        ),
        // Never answered from memory: the call runs and reads the archive.
        Recall::Reread { offset } => format!(
            "Read {url} again from character {offset}: call fetch_url_content with this url and offset={offset}."
        ),
    }
}

/// What to show while one tool call runs. Names the host for a fetch, because
/// "Reading thereviewgeek.com" is the difference between visible progress and a
/// spinner, and a blocked site is then obvious rather than mysterious.
pub(super) fn tool_activity_label(tool: &str, arguments: &serde_json::Value) -> String {
    match tool {
        "fetch_url_content" => match fetch_memory::fetch_target(arguments)
            .and_then(|url| url::Url::parse(url).ok())
            .and_then(|url| url.host_str().map(|host| host.to_string()))
        {
            Some(host) => format!("Reading {host}"),
            None => "Reading a web page".to_string(),
        },
        "web_search" => "Searching the web".to_string(),
        "wiki_search" | "wiki_summary" => "Checking Wikipedia".to_string(),
        "semantic_search" => "Searching your documents".to_string(),
        "list_documents" => "Looking through your documents".to_string(),
        "list_attachments" => "Checking what you attached".to_string(),
        "get_document" => "Opening a document".to_string(),
        "search_conversation_history" | "read_conversation_history" => {
            "Looking back through this conversation".to_string()
        }
        other if crate::features::explorer::tools::is_explorer_tool(other) => {
            crate::features::explorer::tools::activity_label(other, arguments)
        }
        other => format!("Running {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Already fetched" alone invites the model to think the text was lost.
    #[test]
    fn a_page_already_read_in_full_is_pointed_back_to_the_context() {
        let notice = recalled_page_notice(
            "https://example.test/recap",
            &Recall::AlreadyWhole { word_count: 3521 },
        );
        assert!(notice.contains("https://example.test/recap"), "{notice}");
        assert!(notice.contains("3521 words"), "{notice}");
        assert!(notice.contains("in your context above"), "{notice}");
    }

    #[test]
    fn a_part_of_a_long_page_says_where_the_next_part_starts() {
        let notice = recalled_page_notice(
            "https://example.test/book",
            &Recall::Remainder {
                text: "chapter two".to_string(),
                shown_chars: 12_000,
                next: Some(24_000),
                unread: Some(24_000),
            },
        );
        assert!(notice.contains("offset=24000"), "{notice}");
        assert!(notice.ends_with("chapter two"), "{notice}");
    }

    #[test]
    fn the_rest_of_a_clipped_page_says_where_the_first_part_is() {
        let notice = recalled_page_notice(
            "https://example.test/recap",
            &Recall::Remainder {
                text: "and then the flames came down.".to_string(),
                shown_chars: 12_000,
                next: None,
                unread: None,
            },
        );
        assert!(notice.contains("from character 12000"), "{notice}");
        assert!(notice.contains("whole page"), "{notice}");
        assert!(
            notice.ends_with("and then the flames came down."),
            "{notice}"
        );
    }

    /// A part that reaches the end while an earlier stretch is still unread
    /// says where that stretch starts, instead of calling the page whole.
    #[test]
    fn a_last_part_with_a_gap_before_it_names_the_gap() {
        let notice = recalled_page_notice(
            "https://example.test/recap",
            &Recall::Remainder {
                text: "the end.".to_string(),
                shown_chars: 20_000,
                next: None,
                unread: Some(8_000),
            },
        );
        assert!(notice.contains("offset=8000"), "{notice}");
        assert!(!notice.contains("whole page"), "{notice}");
    }
}
