//! The admission sweep (claim classes design §3.6): the gate's verdict on the
//! pages the library already holds. A page the gate refuses stays in the
//! library, listed for the user to delete, and leaves retrieval and citation
//! at once, so nothing is compared with it again.
use super::*;
use crate::features::learning::references::admission as gate;

/// A saved page that is not a source, with the reason the gate gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedSource {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub reason: String,
}

impl LearningSourceLibraryRepository {
    /// Reads, under the gate, the active version of every source the machine
    /// added to the program (research, an outline, the Verifier) that the
    /// gate has not read, and marks those that are not sources. A page the
    /// user added is theirs and is never marked. No model is asked. Returns
    /// the pages refused now.
    pub async fn sweep_admission(&self, program_id: &str) -> Result<Vec<RefusedSource>> {
        let rows = sqlx::query(
            "SELECT s.id AS source_id, v.id AS version_id, v.title, v.full_text, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND s.added_by<>'user' AND (v.admission_policy IS NULL OR v.admission_policy<>?) ORDER BY s.created_at,s.id",
        )
        .bind(program_id)
        .bind(gate::POLICY)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut refused = Vec::new();
        for row in rows {
            let source_id: String = row.get("source_id");
            let version_id: String = row.get("version_id");
            let title: String = row.get("title");
            let text: String = row.get("full_text");
            let url: Option<String> = row.get("url");
            let reason = gate::admit(url.as_deref().unwrap_or_default(), &title, &text)
                .err()
                .map(|refusal| refusal.to_string());
            sqlx::query(
                "UPDATE learning_source_versions SET not_a_source=?, admission_policy=? WHERE program_id=? AND id=?",
            )
            .bind(&reason)
            .bind(gate::POLICY)
            .bind(program_id)
            .bind(&version_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
            if let Some(reason) = reason {
                refused.push(RefusedSource {
                    source_id,
                    version_id,
                    title,
                    url,
                    reason,
                });
            }
        }
        Ok(refused)
    }

    /// The pages the gate has refused that the program still holds, for the
    /// user to delete.
    pub async fn refused_sources(&self, program_id: &str) -> Result<Vec<RefusedSource>> {
        let rows = sqlx::query(
            "SELECT s.id AS source_id, v.id AS version_id, v.title, v.not_a_source AS reason, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NOT NULL ORDER BY s.created_at,s.id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| RefusedSource {
                source_id: row.get("source_id"),
                version_id: row.get("version_id"),
                title: row.get("title"),
                url: row.get("url"),
                reason: row.get("reason"),
            })
            .collect())
    }
}
