//! The values the `models.model_type` CHECK constraint accepts, for each kind of model.

use lattice_core::domain::models::metadata::ModelType;
use lattice_core::domain::models::selection::ModelCategory;

/// Map domain model types to the models-table CHECK values.
///
/// The domain uses modern values (`language_model`, `text_embeddings`), while the
/// legacy `models` table constraint still expects (`chat`, `embedding`, ...).
/// `custom` and `unknown` stand in for kinds the constraint has no value for.
pub(crate) fn normalize_model_type_for_models_table(model_type: ModelType) -> &'static str {
    match model_type {
        ModelType::LanguageModel => "chat",
        ModelType::TextEmbeddings => "embedding",
        ModelType::Vision => "multi_modal",
        ModelType::Reranker => "qa",
        ModelType::Transcription => "custom",
        ModelType::Speech => "unknown",
    }
}

/// The models-table CHECK value for a catalog category.
/// `custom` and `unknown` stand in for kinds the constraint has no value for.
pub(crate) fn models_table_type(category: ModelCategory) -> &'static str {
    match category {
        ModelCategory::LLM => "chat",
        ModelCategory::Embedding => "embedding",
        ModelCategory::OCR => "ocr",
        ModelCategory::Transcription => "custom",
        ModelCategory::Speech => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_maps_to_a_value_the_models_table_accepts() {
        let accepted = [
            "chat",
            "embedding",
            "qa",
            "multi_modal",
            "custom",
            "ocr",
            "unknown",
        ];
        let expected = [
            (ModelCategory::LLM, "chat"),
            (ModelCategory::Embedding, "embedding"),
            (ModelCategory::OCR, "ocr"),
            (ModelCategory::Transcription, "custom"),
            (ModelCategory::Speech, "unknown"),
        ];
        for (category, value) in expected {
            assert_eq!(models_table_type(category), value);
            assert!(accepted.contains(&value));
        }
    }
}
