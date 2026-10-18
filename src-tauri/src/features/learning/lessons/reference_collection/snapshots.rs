//! Source snapshots as request evidence: the room a request leaves for them
//! and the passages of each that a query retrieves.
use super::*;

/// The room a request built with an empty snapshot list leaves for passages.
pub(in crate::features::learning) fn snapshot_room(
    llm: &dyn lattice_core::application::ports::LLMPort,
    system: &str,
    skeleton: &serde_json::Value,
    output_tokens: usize,
) -> usize {
    let mut request =
        lattice_core::application::services::grounded_generation::GroundedRequest::new(
            system,
            skeleton.to_string(),
        );
    request.output_tokens = output_tokens;
    crate::features::learning::model_call::source_room(llm, &request, usize::MAX)
}

/// The passages of full source snapshots that `query` retrieves, grouped per
/// snapshot, as many as `room` tokens hold. Requests that cite snapshots
/// carry these instead of a character prefix of each text; a quote is still
/// checked against the whole snapshot.
pub(in crate::features::learning) async fn snapshot_passages(
    llm: &dyn lattice_core::application::ports::LLMPort,
    sources: &[crate::features::learning::dto::LearningSourceVersionDto],
    query: &str,
    room: usize,
) -> Result<serde_json::Value> {
    let documents: Vec<LearningSourceDto> = sources
        .iter()
        .map(|source| LearningSourceDto {
            id: source.version.id.clone(),
            title: source.version.title.clone(),
            url: None,
            excerpt: source.full_text.clone(),
            acquired_at: 0,
        })
        .collect();
    let collection = ReferenceCollection::lexical(&documents)?;
    let entry = |source: &crate::features::learning::dto::LearningSourceVersionDto,
                 passages: &[&str]| {
        serde_json::json!({"sourceId":source.source_id,"versionId":source.version.id,"title":source.version.title,"passages":passages})
    };
    let snapshot = |passage: &ReferencePassage| {
        sources
            .iter()
            .find(|source| source.version.id == passage.source_id)
    };
    let passages = collection
        .retrieve_within(query, |passages| {
            let mut seen = HashSet::new();
            passages
                .iter()
                .map(|passage| {
                    llm.count_tokens(&serde_json::json!(passage.text).to_string())
                        + 2
                        + snapshot(passage)
                            .filter(|source| seen.insert(source.version.id.as_str()))
                            .map_or(0, |source| {
                                llm.count_tokens(&entry(source, &[]).to_string()) + 2
                            })
                })
                .sum::<usize>()
                <= room
        })
        .await?;
    let mut grouped: Vec<(
        &crate::features::learning::dto::LearningSourceVersionDto,
        Vec<&str>,
    )> = Vec::new();
    for passage in &passages {
        let Some(source) = snapshot(passage) else {
            continue;
        };
        match grouped
            .iter_mut()
            .find(|(known, _)| known.version.id == source.version.id)
        {
            Some((_, texts)) => texts.push(passage.text.as_str()),
            None => grouped.push((source, vec![passage.text.as_str()])),
        }
    }
    Ok(serde_json::Value::Array(
        grouped
            .iter()
            .map(|(source, texts)| entry(source, texts))
            .collect(),
    ))
}
