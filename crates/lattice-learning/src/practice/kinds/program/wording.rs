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
            Form::Complete | Form::Arrange => form.label().to_owned(),
        }
    }

    fn stage_label(&self, _brief: &Brief, _form: Form, stage: Stage) -> String {
        match stage {
            Stage::Make => "Code".to_owned(),
            Stage::Check => "Test".to_owned(),
            other => other.label().to_owned(),
        }
    }

    fn check_noun(&self) -> String {
        "run".to_owned()
    }

    fn scope_label(&self, scope: CheckScope) -> String {
        match scope {
            CheckScope::Shown => "Examples".to_owned(),
            CheckScope::Own => "Input".to_owned(),
            CheckScope::All => scope.label().to_owned(),
        }
    }

    fn rung_label(&self, rung: Rung) -> String {
        match rung {
            Rung::Point => "Line".to_owned(),
            other => other.label().to_owned(),
        }
    }

    fn practice_scene(&self) -> String {
        "a coding practice session".to_owned()
    }

    fn interview_scene(&self) -> String {
        "a mock coding interview".to_owned()
    }

    fn task_noun(&self) -> String {
        "problem".to_owned()
    }

    fn piece_noun(&self) -> String {
        "code".to_owned()
    }

    fn session_noun(&self) -> String {
        "coding practice session".to_owned()
    }

    fn check_nouns(&self) -> String {
        "runs".to_owned()
    }

    fn evidence(&self) -> String {
        "the learner's code, runs and examples".to_owned()
    }

    fn refer_rule(&self) -> String {
        "Refer to code by line number.".to_owned()
    }

    fn unsolved_rule(&self) -> String {
        "any code you write must leave the problem unsolved".to_owned()
    }

    fn quoted_work(&self) -> Option<String> {
        Some("code".to_owned())
    }

    fn checked_verb(&self) -> String {
        "ran".to_owned()
    }

    fn submit_probes(&self) -> String {
        "the cost of the code, an edge, or why they chose something".to_owned()
    }

    fn checkpoint_probes(&self) -> String {
        "why they chose something, what a line costs, or an edge they have not mentioned".to_owned()
    }

    fn stage_instruction(&self, _brief: &Brief, _form: Form, stage: Stage) -> String {
        match stage {
            Stage::Understand => {
                "Say what the function takes and returns, and ask about anything the statement leaves open."
            }
            Stage::Examples => {
                "Write examples, edge cases included, and check each one against the problem."
            }
            Stage::Plan => "Say how you will solve it, and what that costs, before you write code.",
            Stage::Make => "Write the function, one small step at a time, and run it as you go.",
            Stage::Check => "Run your examples, then submit to run every case.",
            Stage::Review => "State the time and space cost, and look for anything to simplify.",
            Stage::Reproduce => "Run the code and see the failure for yourself.",
            Stage::Locate => "Find the line, or the idea, that is wrong.",
            Stage::Fix => "Change the code so that the failure goes away and nothing else does.",
            Stage::Verify => "Run every case and say why the code is right now.",
        }
        .to_owned()
    }
}
