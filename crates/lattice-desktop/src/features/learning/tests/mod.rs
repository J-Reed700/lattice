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
mod live_coding_eval;
mod practical_tests;
mod reference_collection_tests;
mod study_tests;
mod teaching_review;

/// The test that serves a lab in a child copy of this binary.
const LAB_RUNNER_CHILD: &str = "features::learning::tests::lab_runner_child";

/// The built-in runtimes these tests run labs on.
fn runtimes() -> BuiltinRuntimes {
    BuiltinRuntimes::for_test_binary(LAB_RUNNER_CHILD)
}

/// Serves one lab when a run starts this binary as its runner; otherwise
/// there is nothing to do.
#[test]
fn lab_runner_child() {
    use lattice_learning::lab_runner;
    if std::env::var_os(lab_runner::CHILD_MODE).is_some_and(|mode| mode == "serve") {
        std::process::exit(lab_runner::run_child());
    }
}
