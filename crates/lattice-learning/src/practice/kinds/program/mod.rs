//! The Program kind: the learner writes code, and running it is the truth.

use crate::practice::coding::proof::MIN_KILLED_MISREADINGS;
use crate::practice::kinds::program::account::ProgramAccount;
use crate::practice::kinds::program::budget::ProgramBudget;
use crate::practice::kinds::program::checker::ProgramChecker;
use crate::practice::kinds::program::diff::ProgramDiff;
use crate::practice::kinds::program::hinting::ProgramHinting;
use crate::practice::kinds::program::learner_cases::ProgramLearnerCases;
use crate::practice::kinds::program::parts::ProgramParts;
use crate::practice::kinds::program::reply::ProgramReply;
use crate::practice::kinds::program::rubric::ProgramRubric;
use crate::practice::kinds::program::showing::ProgramShowing;
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::kinds::program::truth::ProgramTruth;
use crate::practice::kinds::program::wording::ProgramWording;
use crate::practice::session::kind::Kind;
use lattice_core::domain::learning::practice::work::{KindId, Spacing};
use std::sync::Arc;

pub mod account;
pub mod budget;
pub mod checker;
pub mod diff;
pub mod hinting;
pub mod learner_cases;
pub mod parts;
pub mod reply;
pub mod rubric;
pub mod shape;
pub mod showing;
pub mod toolchain;
pub mod truth;
pub mod wording;

#[cfg(test)]
mod counterpart_tests;
#[cfg(test)]
mod debrief_tests;
#[cfg(test)]
mod hinting_tests;
#[cfg(test)]
mod observation_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod proof_tests;

/// The id of the Program kind.
pub const KIND: &str = "program";

/// The Program kind, running programs with `tools`.
pub fn kind(tools: Arc<dyn Toolchains>) -> Kind {
    Kind {
        id: KindId::new(KIND),
        spacing: Spacing::Technique,
        misreadings_needed: MIN_KILLED_MISREADINGS,
        wording: Arc::new(ProgramWording),
        reply_guard: Arc::new(ProgramReply {
            tools: tools.clone(),
        }),
        hinting: Arc::new(ProgramHinting {
            tools: tools.clone(),
        }),
        rubric: Arc::new(ProgramRubric),
        account: Arc::new(ProgramAccount),
        budget: Arc::new(ProgramBudget),
        checker: Arc::new(ProgramChecker {
            tools: tools.clone(),
        }),
        parts: Arc::new(ProgramParts),
        diff: Arc::new(ProgramDiff),
        showing: Arc::new(ProgramShowing),
        truth: Arc::new(ProgramTruth { tools }),
        gaps: None,
        order: None,
        learner_cases: Some(Arc::new(ProgramLearnerCases)),
    }
}
