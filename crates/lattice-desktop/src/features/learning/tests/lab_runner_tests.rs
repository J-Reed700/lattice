//! The built-in runtimes these tests run labs on, as lattice-learning starts their runner from this binary.

use super::runtimes;

#[cfg(feature = "learning-labs")]
#[tokio::test]
async fn a_lab_runs_in_a_child_copy_of_this_test_binary() -> lattice_core::shared::error::Result<()> {
    use lattice_learning::embedded_runtime::{BuiltinLabExecutionSpec, LearningBuiltinRuntime};
    use lattice_learning::lab_runtime::{LearningLabFile, LearningLabLimits, LearningLabRunStatus};

    let spec = BuiltinLabExecutionSpec {
        run_id: uuid::Uuid::new_v4().to_string(),
        runtime: LearningBuiltinRuntime::Javascript,
        files: vec![LearningLabFile {
            path: "checks.mjs".into(),
            content: "console.log('from the child')".into(),
        }],
        read_only_paths: vec!["checks.mjs".into()],
        limits: LearningLabLimits {
            timeout_seconds: 5,
            ..Default::default()
        },
    };
    let result = runtimes()
        .execute(spec, tokio_util::sync::CancellationToken::new())
        .await?;
    assert_eq!(
        result.status,
        LearningLabRunStatus::Passed,
        "{}",
        result.stderr
    );
    assert!(result.stdout.contains("from the child"));
    Ok(())
}

#[test]
fn python_runs_keep_their_compiled_interpreter_out_of_the_app_cache() {
    let bundle = runtimes()
        .python_bundle()
        .expect("the source tree's Python bundle");
    assert_eq!(
        bundle.module_cache,
        Some(std::env::temp_dir().join("lattice-learning-python-test-modules"))
    );
}
