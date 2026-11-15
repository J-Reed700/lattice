//! What the reference planner is told, and what the app keeps to itself.
use super::*;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};

#[test]
fn what_the_planner_reads_is_told() {
    let mut marks = Marks::new();
    let input = PlanInput {
        goal: marks.told("Keep a harbour gate and its tide tables".to_owned()),
        prior_knowledge: marks.told("Has coiled rope before".to_owned()),
        minutes_per_session: Told(30),
        min_modules: Told(2),
        max_modules: Told(3),
        existing_sources: marks.told(json!({"sources": [{"title": "A harbour master's handbook"}]})),
        planned: vec![marks.told("Harbour gates and their keepers".to_owned())],
    };
    holds::<OutlineReferencePlan>(&input, &marks);
}

#[test]
fn a_continued_request_names_the_subtopics_that_stand() {
    let input = PlanInput {
        goal: Told("Keep a harbour gate".into()),
        prior_knowledge: Told("none".into()),
        minutes_per_session: Told(30),
        min_modules: Told(2),
        max_modules: Told(3),
        existing_sources: Told(json!([])),
        planned: Vec::new(),
    };
    let standing = [Subtopic {
        title: "Harbour gates".into(),
        needs: Vec::new(),
    }];
    assert_eq!(
        json!(input.continued(&standing).planned),
        json!(["Harbour gates"])
    );
}
