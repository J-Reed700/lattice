//! Select outline evidence from whole captured sources, not library previews.
use crate::features::learning::reference_collection::ReferenceCollection;
use crate::{application::ports::LLMPort, shared::error::Result};
use serde_json::{json, Value};

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
    let mut excerpts = std::collections::BTreeMap::<usize, Vec<String>>::new();
    let budget = llm.max_context_tokens() / 4;
    let values = |excerpts: &std::collections::BTreeMap<usize, Vec<String>>| -> Vec<Value> {
        excerpts.iter().filter_map(|(index, passages)| {
            let source = collection.sources.get(*index)?;
            Some(json!({"sourceIndex":index,"title":source.title,"url":source.url,"passages":passages,
                "evidenceScope":"Selected exact passages from the full captured source. Cite only passages that support the assertion. Other saved sources remain searchable."}))
        }).collect()
    };
    let mut include = |index: usize, text: String| {
        let passages = excerpts.entry(index).or_default();
        if passages.contains(&text) {
            return;
        }
        passages.push(text);
        if llm.count_tokens(&json!(values(&excerpts)).to_string()) > budget {
            if let Some(passages) = excerpts.get_mut(&index) {
                passages.pop();
                if passages.is_empty() {
                    excerpts.remove(&index);
                }
            }
        }
    };
    let mut ranked = Vec::new();
    for query in queries {
        ranked.push(collection.retrieve(query, 4).await?);
    }
    // Give relevant passages the prompt budget before reference prefixes.
    // Source indices still refer to the complete saved collection.
    for rank in 0..4 {
        for matches in &ranked {
            if let Some(passage) = matches.get(rank) {
                if let Some(index) = collection
                    .sources
                    .iter()
                    .position(|s| s.id == passage.source_id)
                {
                    include(index, passage.text.clone());
                }
            }
        }
    }
    for (index, source) in collection.sources.iter().enumerate() {
        include(index, source.excerpt.chars().take(600).collect());
    }
    Ok(values(&excerpts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::dto::LearningSourceDto;
    use async_trait::async_trait;
    use futures::Stream;

    struct TokenCounter(usize);
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
            self.0
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len().div_ceil(4)
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn large_libraries_fit_prompt_budgets_without_losing_original_source_indices() {
        let sources: Vec<_> = (0..300)
            .map(|index| LearningSourceDto {
                id: format!("source-{index}"),
                title: format!("Reference {index}"),
                url: None,
                excerpt: if index == 299 {
                    "Unique nebular spectroscopy calibration establishes the observed wavelength."
                        .repeat(10)
                } else {
                    "General background material about unrelated measurement.".repeat(20)
                },
                acquired_at: 0,
            })
            .collect();
        let collection = ReferenceCollection::lexical(&sources).unwrap();
        let model = TokenCounter(8192);
        let selected = select(
            &model,
            &collection,
            &["nebular spectroscopy wavelength".into()],
        )
        .await
        .unwrap();
        assert!(model.count_tokens(&json!(selected).to_string()) <= model.max_context_tokens() / 4);
        assert!(selected.iter().any(|entry| entry["sourceIndex"] == 299));
        for entry in &selected {
            let source = &sources[entry["sourceIndex"].as_u64().unwrap() as usize];
            for passage in entry["passages"].as_array().unwrap() {
                assert!(source.excerpt.contains(passage.as_str().unwrap()));
            }
        }
        let catalog = collection.catalog(&model);
        assert_eq!(catalog["totalSources"], 300);
        assert!(catalog["omittedFromThisPrompt"].as_u64().unwrap() > 0);
        assert!(model.count_tokens(&catalog.to_string()) <= model.max_context_tokens() / 16);
        assert_eq!(collection.sources.len(), 300);
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
            &TokenCounter(128_000),
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
