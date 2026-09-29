//! Follow-up document tools obey the same scope as initial chat retrieval.
use crate::features::conversation::chat::focus::FocusScope;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::features::function_calling::dto::{
    AttachmentItem, DocumentResult, ListAttachmentsOutput, SearchMode, SemanticSearchInput,
    SemanticSearchOutput,
};
use crate::features::search::dto::{SearchModeDto, SearchRequestDto};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Tools that can reach the user's own documents, and therefore may only run
/// through this module's scope check. Anything else — web and wiki lookups —
/// touches nothing in the vault and goes straight to the executor.
///
/// A tool added to the registry without being listed here would read documents
/// with no space scope at all, which is exactly the leak this module exists to
/// prevent. `every_vault_tool_is_scoped` fails when that happens.
pub(super) fn reads_the_vault(name: &str) -> bool {
    matches!(
        name,
        "semantic_search" | "get_document" | "list_documents" | "list_attachments"
    )
}

pub(super) async fn execute(
    container: &Container,
    conversation_id: &str,
    focus: &FocusScope,
    call: FunctionCall,
) -> Result<FunctionResult> {
    if !reads_the_vault(&call.name) {
        return container.function_executor().execute(call).await;
    }
    let repository = ConversationRepository::new(container.db_pool().clone());
    let (_, mut allowed) = repository
        .retrieval_document_scope(conversation_id)
        .await?
        .ok_or_else(|| {
            AppError::InvalidState("Could not resolve this conversation's document scope".into())
        })?;
    // A chat pinned to particular documents is pinned here too. Retrieval and
    // the tools the model reaches for afterwards have to agree about what this
    // turn may read, or the model recovers by searching its way straight back
    // out of the focus.
    focus.confine(&mut allowed);
    if focus.blocks_everything() {
        return Err(AppError::InvalidState(format!(
            "This chat is pinned to documents outside its space, so {}",
            focus.unavailable_reason()
        )));
    }
    if call.name == "list_attachments" {
        // Answered here rather than in the executor because this is the only
        // layer that knows which conversation is asking — and the answer is
        // meaningless without that.
        return list_attachments(container, conversation_id, &allowed).await;
    }
    if call.name == "list_documents" {
        // Browsing is a read of the vault like any other. Left unscoped it
        // handed the model every filename, path and tag in the library, so a
        // chat in one space could enumerate another space's documents even
        // though `get_document` would then refuse to open them.
        let listing = container.function_executor().execute(call).await?;
        return Ok(confine_listing(listing, &allowed));
    }
    if call.name == "get_document" {
        let id = call
            .arguments
            .get("document_id")
            .and_then(|id| id.as_str())
            .unwrap_or("");
        ensure_allowed_document(id, &allowed)?;
        return container.function_executor().execute(call).await;
    }
    let input: SemanticSearchInput = serde_json::from_value(call.arguments.clone())?;
    let limit = input.limit.clamp(1, 50);
    let (hits, query_time_ms) = match input.search_mode {
        // The default mode, and the one the model almost always sends: searched
        // with first-pass retrieval's own branches and fusion, so a follow-up
        // search ranks this space the way the search that opened the turn did.
        SearchMode::Hybrid => {
            let start = std::time::Instant::now();
            let hits = crate::features::conversation::chat::retrieval::fused_search(
                container.semantic_search_use_case().as_ref(),
                container.hybrid_search_use_case().as_ref(),
                &input.query,
                &allowed,
                limit,
            )
            .await?;
            (hits, start.elapsed().as_millis() as u64)
        }
        // A single branch the model asked for by name.
        SearchMode::Semantic | SearchMode::Keyword => {
            let request = SearchRequestDto {
                query: input.query.clone(),
                limit: Some(limit),
                threshold: Some(input.threshold.clamp(0.0, 1.0)),
                mode: if matches!(input.search_mode, SearchMode::Semantic) {
                    SearchModeDto::Vector
                } else {
                    SearchModeDto::BM25
                },
            };
            let response = container
                .hybrid_search_use_case()
                // `allowed` is the space plus the chat's attachments; a membership
                // filter as well would hide the attachments from keyword search.
                .execute_scoped(request, None, Some(&allowed))
                .await?;
            (response.results, response.query_time_ms)
        }
    };
    // Only the hit documents: this runs several times a turn, and the library
    // can be thousands of rows.
    let hit_ids: Vec<String> = hits
        .iter()
        .filter_map(|hit| hit.document_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let documents: std::collections::HashMap<String, _> = container
        .document_repository()
        .find_metadata_by_ids(&hit_ids)
        .await?
        .into_iter()
        .map(|doc| (doc.id().as_str().to_owned(), doc))
        .collect();
    let mut results = Vec::new();
    for result in hits {
        let Some(doc) = result
            .document_id
            .as_deref()
            .and_then(|id| documents.get(id))
        else {
            continue;
        };
        if !allowed.contains(doc.id().as_str()) {
            continue;
        }
        if input.file_types.as_ref().is_some_and(|types| {
            !types.iter().any(|ext| {
                doc.file_extension()
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(ext.trim_start_matches('.')))
            })
        }) {
            continue;
        }
        if input
            .date_from
            .as_ref()
            .is_some_and(|date| doc.modified_at() < date)
            || input
                .date_to
                .as_ref()
                .is_some_and(|date| doc.modified_at() > date)
        {
            continue;
        }
        results.push(DocumentResult {
            document_id: doc.id().as_str().into(),
            filename: doc.file_name().into(),
            file_path: doc.file_path().display().to_string(),
            mime_type: doc.mime_type().into(),
            score: result.score,
            snippet: result.content,
            chunk_index: result.position,
            modified_at: *doc.modified_at(),
            size_bytes: doc.size_bytes(),
        });
    }
    let output = SemanticSearchOutput {
        total_found: results.len(),
        results,
        documents: vec![],
        search_time_ms: query_time_ms as f64,
        query: input.query,
    };
    Ok(FunctionResult::success(serde_json::to_value(output)?))
}

/// The files attached to this conversation, oldest first.
///
/// Named by id so the model can hand one to `get_document` and read it whole.
/// Confined by `allowed` like every other vault read: a chat pinned to
/// particular documents does not get to enumerate its way out of the pin, and a
/// document whose text never extracted is not offered as something to read.
async fn list_attachments(
    container: &Container,
    conversation_id: &str,
    allowed: &std::collections::HashSet<String>,
) -> Result<FunctionResult> {
    let owned = container
        .document_scope()
        .documents_owned_by_conversation(conversation_id)
        .await?;

    let mut attachments = Vec::new();
    for document_id in owned {
        if !allowed.contains(&document_id) {
            continue;
        }
        let Some(document) = container
            .document_repository()
            .find_by_id(&document_id)
            .await?
        else {
            continue;
        };
        attachments.push(AttachmentItem {
            document_id,
            filename: document.file_name().into(),
            extension: document.file_extension().unwrap_or_default().into(),
            size_bytes: document.size_bytes(),
            word_count: document.word_count(),
            indexed_at: *document.indexed_at(),
        });
    }

    let output = ListAttachmentsOutput {
        total: attachments.len(),
        attachments,
    };
    tracing::debug!(
        conversation_id,
        total = output.total,
        "Listed this conversation's attachments for the model"
    );
    Ok(FunctionResult::success(serde_json::to_value(output)?))
}

/// Drop documents outside the conversation's scope from a `list_documents`
/// result, and restate the counts so the model is not told about rows it
/// cannot see.
///
/// The underlying tool paginates before this runs, so a page may come back
/// partly empty; `total` is therefore reported as what the caller can actually
/// reach on this page. Under-reporting is the safe direction — the alternative
/// leaks the size of a library the chat has no access to.
fn confine_listing(
    mut listing: FunctionResult,
    allowed: &std::collections::HashSet<String>,
) -> FunctionResult {
    let Some(data) = listing.data.as_mut() else {
        return listing;
    };
    let Some(documents) = data.get_mut("documents").and_then(|d| d.as_array_mut()) else {
        return listing;
    };
    documents.retain(|document| {
        document
            .get("document_id")
            .and_then(|id| id.as_str())
            .is_some_and(|id| allowed.contains(id))
    });
    let visible = documents.len();
    if let Some(object) = data.as_object_mut() {
        object.insert("total".into(), serde_json::json!(visible));
    }
    listing
}

fn ensure_allowed_document(id: &str, allowed: &std::collections::HashSet<String>) -> Result<()> {
    if allowed.contains(id) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "That document is outside this conversation's space".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_read_cannot_widen_conversation_scope() {
        let allowed = std::collections::HashSet::from(["a".into()]);
        assert!(ensure_allowed_document("a", &allowed).is_ok());
        assert!(ensure_allowed_document("b", &allowed).is_err());
        assert!(ensure_allowed_document("", &allowed).is_err());
        assert!(ensure_allowed_document("a", &Default::default()).is_err());
    }

    /// A tripwire, not a behaviour test. Any new tool in the registry has to be
    /// classified deliberately: either it reaches the vault and must be scoped
    /// here, or it does not and belongs on this list. Adding one without that
    /// decision is how a scope leak gets reintroduced.
    #[test]
    fn every_vault_tool_is_scoped() {
        use crate::features::function_calling::trait_def::FunctionRegistryTrait;

        let registry =
            crate::features::function_calling::registry::init_function_registry().unwrap();
        let mut names: Vec<String> = registry
            .list_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        names.sort();

        assert_eq!(
            names,
            [
                "fetch_url_content",
                "get_document",
                "list_attachments",
                "list_documents",
                "semantic_search",
                "web_search",
                "wiki_search",
                "wiki_summary",
            ],
            "a tool was added or removed: decide whether it reads the vault, \
             then update `reads_the_vault` and this list together"
        );

        for name in [
            "semantic_search",
            "get_document",
            "list_documents",
            "list_attachments",
        ] {
            assert!(
                reads_the_vault(name),
                "{name} must run through the scope check"
            );
        }
        for name in [
            "web_search",
            "fetch_url_content",
            "wiki_search",
            "wiki_summary",
        ] {
            assert!(!reads_the_vault(name), "{name} does not touch the vault");
        }
    }

    fn listing(ids: &[&str]) -> FunctionResult {
        FunctionResult::success(serde_json::json!({
            "documents": ids
                .iter()
                .map(|id| serde_json::json!({
                    "document_id": id,
                    "filename": format!("{id}.pdf"),
                }))
                .collect::<Vec<_>>(),
            "total": ids.len(),
            "limit": 20,
            "offset": 0,
            "has_more": false,
        }))
    }

    fn ids_in(result: &FunctionResult) -> Vec<String> {
        result.data.as_ref().unwrap()["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["document_id"].as_str().unwrap().to_string())
            .collect()
    }

    /// Browsing must not reveal that another space's documents exist: filenames
    /// and paths are content too.
    #[test]
    fn browsing_cannot_see_documents_outside_the_scope() {
        let allowed = std::collections::HashSet::from(["mine".to_string()]);

        let confined = confine_listing(listing(&["mine", "theirs"]), &allowed);

        assert_eq!(ids_in(&confined), ["mine"]);
        assert_eq!(confined.data.as_ref().unwrap()["total"], 1);
    }

    #[test]
    fn an_empty_scope_hides_every_document() {
        let confined = confine_listing(listing(&["a", "b"]), &Default::default());

        assert!(ids_in(&confined).is_empty());
        assert_eq!(confined.data.as_ref().unwrap()["total"], 0);
    }

    /// A result shaped differently — an error, or a future schema change — must
    /// pass through rather than be silently emptied or panic.
    #[test]
    fn a_result_without_a_document_list_is_left_alone() {
        let allowed = std::collections::HashSet::from(["mine".to_string()]);
        let failure = FunctionResult::error("boom", "no listing");

        let confined = confine_listing(failure, &allowed);

        assert!(!confined.success);
    }
}
