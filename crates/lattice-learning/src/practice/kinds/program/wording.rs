//! The words a program is practised in.

use crate::practice::session::kind::Wording;
use lattice_core::domain::learning::practice::work::{Brief, CheckScope, Form, Rung, Stage};

#[cfg(test)]
mod tests;

pub struct ProgramWording;

impl Wording for ProgramWording {
    fn card(&self) -> String {
        "Code".to_owned()
    }

    fn form_label(&self, form: Form) -> String {
        match form {
            Form::Build => "Problem".to_owned(),
            Form::Repair => "BugFix".to_owned(),
            Form::Complete | Form::Arrange => format!("{form:?}"),
        }
    }

    fn stage_label(&self, _brief: &Brief, _form: Form, stage: Stage) -> String {
        match stage {
            Stage::Make => "Code".to_owned(),
            Stage::Check => "Test".to_owned(),
            other => format!("{other:?}"),
        }
    }

    fn check_noun(&self) -> String {
        "run".to_owned()
    }

    fn scope_label(&self, scope: CheckScope) -> String {
        match scope {
            CheckScope::Shown => "Examples".to_owned(),
            CheckScope::Own => "Input".to_owned(),
            CheckScope::All => format!("{scope:?}"),
        }
    }

    fn rung_label(&self, rung: Rung) -> String {
        match rung {
            Rung::Point => "Line".to_owned(),
            other => format!("{other:?}"),
        }
    }

    fn stage_instruction(&self, _brief: &Brief, _form: Form, _stage: Stage) -> String {
        String::new()
    }
}
