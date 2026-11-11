//! What a program's session is scored on.

use crate::practice::session::kind::{Criterion, Rubric, Statement};
use lattice_core::domain::learning::practice::work::Form;

pub struct ProgramRubric;

impl Rubric for ProgramRubric {
    fn criteria(&self, _form: Form) -> Vec<Criterion> {
        Vec::new()
    }

    fn statement(&self, _form: Form) -> Option<Statement> {
        None
    }
}
