//! The names a blueprint's scope, purpose and formats are stored under, and
//! the checks a scope passes before a blueprint is drawn for it.
use crate::features::learning::assessment_engine::{LearningAssessmentPurpose, LearningItemFormat};
use crate::features::learning::dto::{LearningQuizScopeDto, LearningQuizScopeKind};
use lattice_core::shared::error::{AppError, Result};

pub(super) fn scope_kind_name(kind: LearningQuizScopeKind) -> &'static str {
    match kind {
        LearningQuizScopeKind::Lesson => "lesson",
        LearningQuizScopeKind::Module => "module",
        LearningQuizScopeKind::Course => "course",
    }
}

pub fn purpose_name(purpose: LearningAssessmentPurpose) -> &'static str {
    match purpose {
        LearningAssessmentPurpose::Practice => "practice",
        LearningAssessmentPurpose::Checkpoint => "checkpoint",
        LearningAssessmentPurpose::ModuleTest => "module_test",
        LearningAssessmentPurpose::Cumulative => "cumulative",
        LearningAssessmentPurpose::Transfer => "transfer",
    }
}

pub fn format_name(format: LearningItemFormat) -> &'static str {
    match format {
        LearningItemFormat::SingleChoice => "single_choice",
        LearningItemFormat::MultiSelect => "multi_select",
        LearningItemFormat::TrueFalse => "true_false",
        LearningItemFormat::ShortAnswer => "short_answer",
        LearningItemFormat::Cloze => "cloze",
        LearningItemFormat::Matching => "matching",
        LearningItemFormat::Ordering => "ordering",
        LearningItemFormat::Explanation => "explanation",
        LearningItemFormat::Artifact => "artifact",
    }
}

pub fn parse_format(value: &str) -> Result<LearningItemFormat> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| AppError::Database("Invalid assessment item format".into()))
}

pub fn validate_scope(scope: &LearningQuizScopeDto) -> Result<()> {
    match (scope.kind, scope.id.as_deref()) {
        (LearningQuizScopeKind::Course, None) => Ok(()),
        (LearningQuizScopeKind::Lesson | LearningQuizScopeKind::Module, Some(id))
            if uuid::Uuid::parse_str(id).is_ok() =>
        {
            Ok(())
        }
        _ => Err(AppError::InvalidInput(
            "A lesson or module quiz names its lesson or module; the course exam names neither."
                .into(),
        )),
    }
}
