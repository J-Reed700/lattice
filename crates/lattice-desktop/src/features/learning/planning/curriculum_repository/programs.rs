//! A course's Verifier audit opt-in and consent (lesson claim review §14.2):
//! the `verifier_audit`, `verifier_web_search`, `verifier_consent_at` and
//! `verifier_consent_provider` columns of `learning_programs`. These columns
//! are not content, so writing them leaves the course's content revision
//! alone.

use super::LearningCurriculumRepository;
use crate::features::learning::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;

/// The course's audit setting as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProgramVerifierAudit {
    pub on: bool,
    pub web_search: bool,
    /// When consent to send lesson text to a cloud provider was confirmed.
    pub consent_at: Option<i64>,
    /// The cloud provider consented to (`openai`, `anthropic`, `google`);
    /// `None` when the Verifier needed no consent or none was given.
    pub consent_provider: Option<String>,
}

impl LearningCurriculumRepository {
    pub(crate) async fn verifier_audit(&self, program_id: &str) -> Result<ProgramVerifierAudit> {
        let row = sqlx::query(
            "SELECT verifier_audit, verifier_web_search, verifier_consent_at, \
             verifier_consent_provider FROM learning_programs WHERE id=?",
        )
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        Ok(ProgramVerifierAudit {
            on: row.get::<String, _>("verifier_audit") == "on",
            web_search: row.get::<i64, _>("verifier_web_search") == 1,
            consent_at: row.get("verifier_consent_at"),
            consent_provider: row.get("verifier_consent_provider"),
        })
    }

    /// Writes the whole setting. Turning the audit off clears the consent:
    /// turning it on again asks for it again.
    pub(crate) async fn set_verifier_audit(
        &self,
        program_id: &str,
        setting: &ProgramVerifierAudit,
    ) -> Result<()> {
        let (consent_at, consent_provider) = if setting.on {
            (setting.consent_at, setting.consent_provider.as_deref())
        } else {
            (None, None)
        };
        let written = sqlx::query(
            "UPDATE learning_programs SET verifier_audit=?, verifier_web_search=?, \
             verifier_consent_at=?, verifier_consent_provider=? WHERE id=?",
        )
        .bind(if setting.on { "on" } else { "off" })
        .bind(i64::from(setting.on && setting.web_search))
        .bind(consent_at)
        .bind(consent_provider)
        .bind(program_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if written.rows_affected() == 0 {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        Ok(())
    }
}
