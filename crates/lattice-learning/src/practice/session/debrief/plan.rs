//! What a form's debrief is made of, read once from the kind.

use crate::practice::session::kind::{Citation, Criterion, Kind, Statement};
use lattice_core::domain::learning::practice::work::Form;

pub(super) struct Plan<'a> {
    pub kind: &'a Kind,
    pub criteria: Vec<Criterion>,
    pub statement: Option<Statement>,
    pub part: Citation,
    pub check: Citation,
    pub case: Option<Citation>,
}

impl<'a> Plan<'a> {
    pub fn new(kind: &'a Kind, form: Form) -> Self {
        Self {
            kind,
            criteria: kind.rubric.criteria(form),
            statement: kind.rubric.statement(form),
            part: kind.account.cites_parts(),
            check: kind.account.cites_check(),
            case: kind.learner_cases.as_ref().map(|cases| cases.cited()),
        }
    }
}
