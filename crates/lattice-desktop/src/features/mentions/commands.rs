//! Mention management commands delegating to mention use cases.

use crate::features::mentions::dto::{
    BacklinksResultDto, ExtractMentionsResultDto, GetMentionsForDocumentResultDto, MentionDto,
    SearchMentionsResultDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

pub async fn extract_mentions(
    document_id: String,
    content: String,
    container: State<'_, Container>,
) -> Result<ExtractMentionsResultDto> {
    let use_case = container.extract_mentions_use_case();
    use_case.execute(document_id, content).await
}

pub async fn search_mentions(
    query: String,
    limit: Option<i64>,
    container: State<'_, Container>,
) -> Result<SearchMentionsResultDto> {
    let use_case = container.search_mentions_use_case();
    use_case.execute(query, limit).await
}

pub async fn get_mentions_for_document(
    document_id: String,
    container: State<'_, Container>,
) -> Result<GetMentionsForDocumentResultDto> {
    let use_case = container.get_mentions_for_document_use_case();
    use_case.execute(document_id).await
}

pub async fn get_backlinks_for_mention(
    mention_name: String,
    container: State<'_, Container>,
) -> Result<BacklinksResultDto> {
    let use_case = container.get_backlinks_use_case();
    use_case.execute(mention_name).await
}

pub async fn get_mentions_by_type(
    mention_type: String,
    container: State<'_, Container>,
) -> Result<Vec<MentionDto>> {
    let use_case = container.get_mentions_by_type_use_case();
    use_case.execute(mention_type).await
}

pub async fn create_mention(
    name: String,
    mention_type: String,
    metadata: Option<String>,
    container: State<'_, Container>,
) -> Result<MentionDto> {
    let use_case = container.create_mention_use_case();
    use_case.execute(name, mention_type, metadata).await
}

pub async fn delete_mention(mention_id: String, container: State<'_, Container>) -> Result<()> {
    let use_case = container.delete_mention_use_case();
    use_case.execute(mention_id).await
}
