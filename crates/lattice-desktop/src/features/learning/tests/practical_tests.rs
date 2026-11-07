//! Stands in for `lattice_learning::practice::practical_tests`, which declared this eval.

use lattice_learning::practice::test_support::{fixture, id};

#[path = "../practice/interview_research_live_tests.rs"]
mod interview_research_live_tests;

/// The commands take a `State<Container>`, which no test here builds, so the
/// boundary's own numbers are read from its source.
#[test]
fn the_command_boundary_carries_no_cap_of_its_own_on_a_learners_text(
) -> lattice_core::shared::error::Result<()> {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/features/learning/plugin/practical.rs"
    ))?;
    for cap in ["4_000", "4000", "8_000", "8000"] {
        assert!(
            !source.contains(cap),
            "plugin/practical.rs still holds a learner's text to {cap}, not the chat limit"
        );
    }
    Ok(())
}
