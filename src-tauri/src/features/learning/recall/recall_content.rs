//! What a recall card of each format may contain. Pure checks, run before
//! a card version is saved.
use crate::features::learning::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto,
};
use crate::shared::error::{AppError, Result};
use std::collections::HashSet;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

pub(crate) fn validate_content(
    format: LearningRecallCardFormat,
    content: &LearningRecallContentDto,
) -> Result<()> {
    if content.prompt.trim().is_empty()
        || content.prompt.chars().count() > 2_000
        || content.answer.trim().is_empty()
        || content.answer.chars().count() > 1_000
        || content.explanation.chars().count() > 3_000
    {
        return Err(invalid(
            "Recall prompt, answer, or explanation is outside its allowed bounds.",
        ));
    }
    let invalid_multiple_choice = !(2..=8).contains(&content.options.len())
        || content
            .options
            .iter()
            .any(|choice| choice.trim().is_empty() || choice.chars().count() > 500)
        || content
            .options
            .iter()
            .map(|choice| choice.trim().to_lowercase())
            .collect::<HashSet<_>>()
            .len()
            != content.options.len()
        || content
            .correct_option_index
            .is_none_or(|index| index >= content.options.len())
        || content
            .correct_option_index
            .and_then(|index| content.options.get(index))
            .is_none_or(|choice| choice.trim() != content.answer.trim());
    match format {
        LearningRecallCardFormat::MultipleChoice if invalid_multiple_choice => {
            return Err(invalid(
                "Multiple-choice recall needs 2–8 unique options and a valid correct option.",
            ));
        }
        LearningRecallCardFormat::MultipleChoice => {}
        _ if !content.options.is_empty() || content.correct_option_index.is_some() => {
            return Err(invalid(
                "Only multiple-choice recall cards can contain answer options.",
            ));
        }
        _ => {}
    }
    if format == LearningRecallCardFormat::Cloze {
        if content.cloze_deletions.is_empty()
            || content.cloze_deletions.len() > 12
            || content
                .cloze_deletions
                .iter()
                .any(|deletion| deletion.trim().is_empty() || !content.prompt.contains(deletion))
            || content
                .cloze_deletions
                .iter()
                .map(|deletion| deletion.trim().to_lowercase())
                .collect::<HashSet<_>>()
                .len()
                != content.cloze_deletions.len()
        {
            return Err(invalid(
                "Cloze deletions must be unique, non-empty text present in the prompt.",
            ));
        }
    } else if !content.cloze_deletions.is_empty() {
        return Err(invalid("Only cloze cards can contain cloze deletions."));
    }
    if format == LearningRecallCardFormat::CodePrediction
        && content
            .language
            .as_deref()
            .is_none_or(|value| value.trim().is_empty() || value.chars().count() > 48)
    {
        return Err(invalid("Code-prediction cards require a language name."));
    }
    if format != LearningRecallCardFormat::CodePrediction && content.language.is_some() {
        return Err(invalid(
            "Only code-prediction cards can name a programming language.",
        ));
    }
    Ok(())
}
