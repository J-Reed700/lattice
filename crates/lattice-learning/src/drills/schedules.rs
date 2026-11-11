//! The two schedules of review drills (docs/design/2026-11-07-drills-in-the-course.md section 5): when each pattern
//! of a material and each lesson of a course comes back, and which material a lesson fits. Grading writes the
//! schedules in the transaction that records the grade, and the bank reads them.

use crate::recall::study_dto::StudyRating;
use lattice_core::domain::learning::drills::{Material, PatternRating};

pub(crate) mod repository;
mod store;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use store::fit_basis_in;
pub(crate) use store::{
    fit_basis, lesson_fits_in, lesson_states_in, pattern_states_in, record_fit, record_fit_in,
    record_in,
};

/// The name a material's pattern schedules are kept under: `code:<language>`, or `passage`.
pub(crate) fn schedule_material(material: &Material) -> String {
    match material {
        Material::Code { language } => format!("code:{language}"),
        Material::Passage => "passage".to_owned(),
    }
}

/// The name a material is recorded under when a lesson is found to fit it or not: `code` or `passage`.
pub(crate) fn fit_material(material: &Material) -> &'static str {
    match material {
        Material::Code { .. } => "code",
        Material::Passage => "passage",
    }
}

fn study_rating(rating: PatternRating) -> StudyRating {
    match rating {
        PatternRating::Again => StudyRating::Again,
        PatternRating::Hard => StudyRating::Hard,
        PatternRating::Good => StudyRating::Good,
        PatternRating::Easy => StudyRating::Easy,
    }
}
