use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shown::{Internal, Showing, Told};
use lattice_core::application::services::model_contract::units::{Grain, Units};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

use crate::dto::LearningSourceVersionDto;

/// `context` with the snapshot passages `query` retrieves, as many as the
/// request's window leaves room for, under `sourceSnapshots`.
pub(in crate::practice) async fn with_snapshots(
    llm: &dyn LLMPort,
    system: &str,
    mut context: serde_json::Value,
    sources: &[LearningSourceVersionDto],
    query: &str,
    output_tokens: usize,
) -> Result<String> {
    let room = crate::reference_collection::snapshot_room(llm, system, &context, output_tokens);
    let passages =
        crate::reference_collection::snapshot_passages(llm, sources, query, room).await?;
    if let Some(fields) = context.as_object_mut() {
        fields.insert("sourceSnapshots".into(), passages);
    }
    Ok(context.to_string())
}

/// A passage of a saved source the model may read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Excerpt {
    pub(super) source: Internal<String>,
    pub(super) version: Internal<String>,
    pub(super) title: Told<String>,
    pub(super) text: Told<String>,
}

/// The passages of `sources` that `query` retrieves, the best first, each with the tokens showing it takes.
pub(super) async fn ranked_excerpts(
    llm: &dyn LLMPort,
    sources: &[LearningSourceVersionDto],
    query: &str,
) -> Result<Packed<Excerpt>> {
    if sources.is_empty() {
        return Ok(Packed::default());
    }
    let documents: Vec<crate::dto::LearningSourceDto> = sources
        .iter()
        .map(|source| crate::dto::LearningSourceDto {
            id: source.version.id.clone(),
            title: source.version.title.clone(),
            url: None,
            excerpt: source.full_text.clone(),
            acquired_at: 0,
        })
        .collect();
    let collection = crate::reference_collection::ReferenceCollection::lexical(&documents)?;
    let found = collection.retrieve_within(query, |_| true).await?;
    let excerpts: Vec<Excerpt> = found
        .into_iter()
        .filter_map(|passage| {
            let source = sources
                .iter()
                .find(|source| source.version.id == passage.source_id)?;
            Some(Excerpt {
                source: Internal::of(source.source_id.clone()),
                version: Internal::of(source.version.id.clone()),
                title: Told(source.version.title.clone()),
                text: Told(passage.text),
            })
        })
        .collect();
    Ok(Packed::new(excerpts, |excerpt| {
        llm.count_tokens(&excerpt.title.0) + llm.count_tokens(&excerpt.text.0) + 1
    }))
}

/// `held` grouped by source, the source met first first.
pub(super) fn by_source<'a>(held: impl Iterator<Item = &'a Excerpt>) -> Vec<Vec<&'a Excerpt>> {
    let mut groups: Vec<Vec<&Excerpt>> = Vec::new();
    for excerpt in held {
        let group = groups.iter_mut().find(|group| {
            group
                .first()
                .is_some_and(|first| first.version.value() == excerpt.version.value())
        });
        match group {
            Some(group) => group.push(excerpt),
            None => groups.push(vec![excerpt]),
        }
    }
    groups
}

/// `held` as the `sourceSnapshots` entries: each passage's sentences, one to a row.
pub(super) fn sentence_snapshots(
    showing: &mut Showing<'_>,
    held: &[&Excerpt],
) -> Result<serde_json::Value> {
    let mut entries = Vec::new();
    for group in by_source(held.iter().copied()) {
        let Some(first) = group.first() else {
            continue;
        };
        let mut passages = Vec::new();
        for excerpt in &group {
            let units = Units::of(&excerpt.text.0, Grain::Sentence);
            let shown = units.shown(showing)?;
            let rows = units.rows(&shown);
            passages.push(rows.lines().map(str::to_owned).collect::<Vec<_>>());
        }
        entries.push(serde_json::json!({"title": first.title.0, "passages": passages}));
    }
    Ok(serde_json::Value::Array(entries))
}
