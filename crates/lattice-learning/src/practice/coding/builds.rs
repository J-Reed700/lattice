//! What a program's build needs from storage: the language, patterns and details its author is told, and the
//! activity a proven problem is saved as.

use crate::persistence::now;
use crate::practice::coding::jobs::{save_problem, ProblemSave, EASY_PATTERNS, HARDER_PATTERNS};
use crate::practice::coding::patterns::targets_in;
use crate::practice::coding::problem::ProblemDifficulty;
use crate::practice::coding::source::{ProgramFacts, ProgramOptions};
use crate::practice::kinds::program::shape::built_problem_of;
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::kinds::program::KIND;
use crate::practice::session::builder::{Exercises, NothingStore, Saving, Target};
use crate::practice_repository::profile_in;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Exercise, Form, KindId};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Transactions};
use serde_json::Value;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Program's storage for builds, over the practical repository and the practice profile.
pub struct CodingBuilds {
    pub database: Database,
    pub tools: Arc<dyn Toolchains>,
}

fn options_of(value: &Value) -> Result<ProgramOptions> {
    serde_json::from_value(value.clone()).map_err(|error| {
        AppError::InvalidInput(format!("The build options are not a program's: {error}."))
    })
}

fn database_error(error: impl std::fmt::Display) -> AppError {
    AppError::Database(error.to_string())
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
        kind: &KindId,
        program_id: &str,
        _form: Form,
        options: &Value,
    ) -> Result<Value> {
        if kind.as_str() != KIND {
            return Err(AppError::InvalidState(format!(
                "These builds are a program's, not {}'s.",
                kind.as_str()
            )));
        }
        let options = options_of(options)?;
        let mut transaction = self.database.begin_read().await.map_err(database_error)?;
        let profile = profile_in(&mut transaction, program_id).await?;
        let language = Some(options.language.trim())
            .filter(|language| !language.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                profile
                    .as_ref()
                    .and_then(|profile| profile.language.clone())
            })
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "Choose a language for the problem; the practice profile names none.".into(),
                )
            })?;
        let wanted = if options.difficulty == ProblemDifficulty::Easy {
            EASY_PATTERNS
        } else {
            HARDER_PATTERNS
        };
        let patterns: Vec<String> = targets_in(&mut transaction, program_id, &language, now())
            .await?
            .into_iter()
            .take(wanted)
            .collect();
        transaction.commit().await.map_err(database_error)?;
        serde_json::to_value(ProgramFacts {
            language,
            patterns,
            details: profile.map(|profile| profile.details),
        })
        .map_err(|error| AppError::Serialization(error.to_string()))
    }

    async fn save(&self, saving: &Saving<'_>, exercise: &Exercise) -> Result<String> {
        let options = options_of(saving.options)?;
        let problem = built_problem_of(exercise)?;
        let harness = self.tools.harness(&problem.spec.language)?;
        save_problem(
            &self.database,
            &ProblemSave {
                job_id: saving.job_id,
                program_id: saving.program_id,
                lesson_id: saving.lesson_id,
                focus: options.focus.as_deref(),
                language: &problem.spec.language,
                model: saving.model,
            },
            &problem,
            harness.as_ref(),
        )
        .await
    }
}
