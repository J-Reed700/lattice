//! What a program's build needs from storage: the language, patterns and details its author is told, and the
//! activity a proven problem is saved as.

use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::builder::{Exercises, NothingStore, Saving, Target};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Exercise, Form, KindId};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::Value;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Program's storage for builds, over the practical repository and the practice profile.
pub struct CodingBuilds {
    pub database: Database,
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl NothingStore for CodingBuilds {
    async fn nothing_known(&self, _target: &Target<'_>) -> Result<Option<String>> {
        Ok(None)
    }

    async fn record_nothing(&self, _target: &Target<'_>, _reason: &str) -> Result<()> {
        Err(AppError::InvalidState(
            "A program always has something to make.".into(),
        ))
    }
}

#[async_trait]
impl Exercises for CodingBuilds {
    async fn facts(
        &self,
        _kind: &KindId,
        _program_id: &str,
        _form: Form,
        _options: &Value,
    ) -> Result<Value> {
        Ok(Value::Null)
    }

    async fn save(&self, _saving: &Saving<'_>, _exercise: &Exercise) -> Result<String> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }
}
