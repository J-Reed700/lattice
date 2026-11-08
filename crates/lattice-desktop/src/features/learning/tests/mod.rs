//! lattice-learning's tests that need this crate's model clients, web service or search.

use lattice_learning::embedded_runtime::BuiltinRuntimes;
use lattice_learning::test_support::{fixture, pool};

mod course_generation_tests;
mod course_index;
mod drills_tests;
mod generation_jobs;
mod grading;
mod interview_answers_tests;
mod lab_runner_tests;
mod practical_tests;
mod reference_collection_tests;
mod study_tests;
mod teaching_review;

/// The built-in runtimes these tests run labs on.
fn runtimes() -> BuiltinRuntimes {
    BuiltinRuntimes::for_tests()
}
