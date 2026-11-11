//! What runs a program in a language, and how its cases are read back.

use crate::drills::runner::{runner_for, CodeRunner};
use crate::embedded_runtime::BuiltinRuntimes;
use crate::practice::coding::harness::{harness_for, CaseHarness};
use lattice_core::shared::error::{AppError, Result};

#[cfg(test)]
pub(crate) use fake::FakeToolchains;

#[cfg(test)]
mod fake;

/// The runner and the case harness for a language.
pub trait Toolchains: Send + Sync {
    fn runner(&self, language: &str) -> Result<Box<dyn CodeRunner>>;
    fn harness(&self, language: &str) -> Result<Box<dyn CaseHarness>>;
}

/// The built-in runtimes and harnesses.
pub struct BuiltinToolchains {
    runtimes: BuiltinRuntimes,
}

impl BuiltinToolchains {
    pub fn new(runtimes: BuiltinRuntimes) -> Self {
        Self { runtimes }
    }
}

impl Toolchains for BuiltinToolchains {
    fn runner(&self, language: &str) -> Result<Box<dyn CodeRunner>> {
        runner_for(language, &self.runtimes).ok_or_else(|| {
            AppError::ServiceNotAvailable(format!("No runtime is available to run {language}."))
        })
    }

    fn harness(&self, language: &str) -> Result<Box<dyn CaseHarness>> {
        harness_for(language)
            .ok_or_else(|| AppError::InvalidState(format!("No case harness for {language}.")))
    }
}
