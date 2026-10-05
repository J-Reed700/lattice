//! Select outline evidence from whole captured sources, not library previews.
use crate::features::learning::reference_collection::ReferenceCollection;
use crate::{application::ports::LLMPort, shared::error::Result};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(in crate::features::learning) fn lesson_queries(candidate: &Value) -> Vec<String> {
    let mut queries = Vec::new();
    for module in candidate["modules"].as_array().into_iter().flatten() {
        for lesson in module["lessons"].as_array().into_iter().flatten() {
            queries.push(format!(
                "{} {} {}",
                module["title"].as_str().unwrap_or_default(),
                lesson["title"].as_str().unwrap_or_default(),
                lesson["objective"].as_str().unwrap_or_default()
            ));
        }
    }
    queries
}

/// Exact provenance is checked by code, even when the model approves a quote.
/// Field-level diagnostics let repairs copy the real punctuation and formatting
/// instead of failing only after the final model review.
pub(in crate::features::learning) fn quote_checks(
    candidate: &Value,
    sources: &[crate::features::learning::dto::LearningSourceDto],
) -> Vec<Value> {
    let mut failures = Vec::new();
    for (module_index, module) in candidate["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut fields = vec![("summary".to_owned(), &module["summary"])];
        for name in ["outcomes", "lessons"] {
            fields.extend(
                module[name]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(index, field)| (format!("{name}[{index}]"), field)),
            );
        }
        for (field_name, field) in fields {
            let index = field["sourceIndex"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok());
            let quote = field["quote"].as_str().unwrap_or_default();
            if let Err(error) =
                crate::features::learning::generation::generated_source_ids(sources, index, quote)
            {
                failures.push(json!({"path":format!("modules[{module_index}].{field_name}"),"sourceIndex":index,"quote":quote,"error":error.to_string()}));
            }
        }
    }
    failures
}

/// Keep source indices stable across authoring, review, repair and validation.
/// The budget is for evidence in one request, not a deadline or a truncation of
/// the captured source. Retrieval always searches each source's complete text.
pub(in crate::features::learning) async fn select(
    llm: &dyn LLMPort,
    collection: &ReferenceCollection<'_>,
    queries: &[String],
) -> Result<Vec<Value>> {
    let mut excerpts = vec![Vec::<String>::new(); collection.sources.len()];
    let mut seen = HashSet::new();
    let mut remaining = llm.max_context_tokens() / 4;
    // Reserve a small, exact passage for every selected source, including ones
    // not retrieved for this goal. These are never mistaken for the full source.
    for (source, excerpt) in collection.sources.iter().zip(&mut excerpts) {
        let prefix: String = source.excerpt.chars().take(600).collect();
        remaining = remaining.saturating_sub(llm.count_tokens(&prefix) + 96);
        excerpt.push(prefix);
    }
    let mut ranked = Vec::new();
    for query in queries {
        ranked.push(collection.retrieve(query, 4).await?);
    }
    // Round-robin prevents the first lesson's matches from consuming the
    // evidence allowance before later lessons get a relevant passage.
    for rank in 0..4 {
        for matches in &ranked {
            let Some(passage) = matches.get(rank) else {
                continue;
            };
            if !seen.insert((
                passage.source_id.clone(),
                passage.start_byte,
                passage.end_byte,
            )) {
                continue;
            }
            let Some((_, excerpt)) = collection
                .sources
                .iter()
                .zip(&mut excerpts)
                .find(|(s, _)| s.id == passage.source_id)
            else {
                continue;
            };
            let tokens = llm.count_tokens(&passage.text) + 16;
            if tokens > remaining {
                continue;
            }
            remaining -= tokens;
            excerpt.push(passage.text.clone());
        }
    }
    Ok(collection.sources.iter().zip(excerpts).enumerate().map(|(index, (source, passages))| {
        json!({"sourceIndex":index,"title":source.title,"url":source.url,"passages":passages,
            "evidenceScope":"Selected exact passages from the full captured source. Cite a passage only when it supports the attached statement; do not use an unrelated quote merely because it comes from the same book."})
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::dto::LearningSourceDto;
    use async_trait::async_trait;
    use futures::Stream;

    struct TokenCounter;
    #[async_trait]
    impl LLMPort for TokenCounter {
        async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
            unreachable!()
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "test"
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len().div_ceil(4)
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn outline_evidence_reaches_later_chapters_across_subjects_and_preserves_indices() {
        let cases = [
            (
                "Rust files",
                "std::fs::read_to_string reads a file into a string.",
            ),
            (
                "Apple pie pastry",
                "Chill the pastry dough before rolling the apple pie crust.",
            ),
            (
                "Biology cells",
                "Mitochondria generate ATP during cellular respiration.",
            ),
        ];
        let sources: Vec<_> = cases
            .iter()
            .enumerate()
            .map(|(index, (title, fact))| LearningSourceDto {
                id: format!("source-{index}"),
                title: (*title).into(),
                url: None,
                excerpt: format!(
                    "{}\n\n{}",
                    "A preface about the publisher and reading this guide.\n".repeat(1000),
                    fact.repeat(10)
                ),
                acquired_at: 0,
            })
            .collect();
        let collection = ReferenceCollection::lexical(&sources).unwrap();
        let selected = select(
            &TokenCounter,
            &collection,
            &cases
                .iter()
                .map(|(_, fact)| (*fact).into())
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();
        for (index, (_, fact)) in cases.iter().enumerate() {
            assert_eq!(selected[index]["sourceIndex"], index);
            assert!(selected[index]["passages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p.as_str().unwrap().contains(fact)));
            for passage in selected[index]["passages"].as_array().unwrap() {
                assert!(sources[index].excerpt.contains(passage.as_str().unwrap()));
            }
        }
    }

    #[test]
    fn retrieval_queries_follow_each_proposed_lesson_including_later_modules() {
        let queries = lesson_queries(&json!({"modules":[
            {"title":"Foundations","lessons":[{"title":"Ownership","objective":"Explain moves and borrowing"}]},
            {"title":"Application","lessons":[{"title":"Read files","objective":"Use std::fs::read_to_string"}]}
        ]}));
        assert_eq!(queries.len(), 2);
        assert!(queries[1].contains("std::fs::read_to_string"));
    }
}
