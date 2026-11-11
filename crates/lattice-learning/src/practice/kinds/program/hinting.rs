//! The hints of a program: today's ladder, asked through the kind's role.

use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::{Hint, HintAsk, Hinting};
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

pub struct ProgramHinting {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl Hinting for ProgramHinting {
    async fn hint(&self, _ask: HintAsk<'_>) -> Result<Option<Hint>> {
        Err(AppError::InvalidState(
            "A program's hints are not built yet.".into(),
        ))
    }
}
