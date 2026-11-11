//! What a program's session is scored on.

use crate::practice::session::kind::{Criterion, Rubric, Statement};
use lattice_core::domain::learning::practice::work::Form;

/// The keys of what the debrief states beside the scores.
pub const APPROACH: &str = "approach";
pub const COMPLEXITY: &str = "complexity";

pub struct ProgramRubric;

fn criterion(id: &str, title: &str, earns: &str) -> Criterion {
    Criterion {
        id: id.to_owned(),
        title: title.to_owned(),
        earns: earns.to_owned(),
    }
}

impl Rubric for ProgramRubric {
    fn criteria(&self, _form: Form) -> Vec<Criterion> {
        vec![
            criterion(
                "problem_solving",
                "Problem solving",
                "Understood and clarified the problem, chose a sound approach, and reached the target complexity or explained the trade-off.",
            ),
            criterion("code", "Code", "Correct on the cases, clear, idiomatic."),
            criterion(
                "testing",
                "Testing",
                "Examples before code, edge cases, ran the code, read the failures and fixed them.",
            ),
            criterion(
                "communication",
                "Communication",
                "Explained the plan and the decisions, answered probes, and used hints well.",
            ),
        ]
    }

    fn statement(&self, _form: Form) -> Option<Statement> {
        Some(Statement {
            names: "the approach and complexity of a strong answer".to_owned(),
            keys: vec![APPROACH.to_owned(), COMPLEXITY.to_owned()],
            says: "The approach is the strong answer's approach in a few sentences, and the complexity states its time and space cost.".to_owned(),
        })
    }
}
