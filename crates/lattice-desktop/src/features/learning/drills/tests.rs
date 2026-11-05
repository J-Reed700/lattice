//! Tests of the drill build, its proof strategies, its runner, the bank, the migrations, the attempts at a drill and
//! their grading. No test reaches a network: the model, the runner and the grader are scripted, and the real runtimes
//! run only small programs.

mod attempt_tests;
mod bank_tests;
mod blind_review_tests;
mod clean_code_tests;
mod clean_review_tests;
mod fix_grading_tests;
mod grading_job_tests;
mod job_tests;
mod plant_tests;
mod proof_tests;
mod prove_tests;
mod repository_tests;
mod resume_tests;
mod runner_tests;
mod schema_tests;
mod store_tests;
mod support;
