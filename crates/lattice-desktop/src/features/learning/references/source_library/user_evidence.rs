//! What claim review needs from the source library when the user attaches
//! evidence: the saved source a link, a file or a pasted text already is
//! (dedupe), and the frozen page a web version was read from.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.4.
use super::*;
use lattice_core::shared::http::url_identity::identity;

/// The active version of one live source of a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSource {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    /// The page the version was read from, for a web source.
    pub url: Option<String>,
    pub added_by: SourceAddedBy,
    pub snapshot_hash: Option<String>,
}

struct LiveSource {
    saved: SavedSource,
    kind: String,
    origin: String,
    requested_url: Option<String>,
    content_sha256: String,
}

impl LearningSourceLibraryRepository {
    /// The digest a captured text is stored under (`content_sha256`).
    pub fn text_digest(text: &str) -> String {
        hash_text(&normalized(text))
    }

    async fn live_sources(&self, program_id: &str) -> Result<Vec<LiveSource>> {
        Self::validate_program_id(program_id)?;
        let rows = sqlx::query("SELECT s.id AS source_id,s.kind,s.origin,s.requested_url AS source_url,s.added_by,v.id AS version_id,v.title,v.requested_url,v.resolved_url,v.content_sha256,v.snapshot_hash FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                let requested_url: Option<String> = row
                    .get::<Option<String>, _>("requested_url")
                    .or_else(|| row.get("source_url"));
                let resolved_url: Option<String> = row.get("resolved_url");
                Ok(LiveSource {
                    saved: SavedSource {
                        source_id: row.get("source_id"),
                        version_id: row.get("version_id"),
                        title: row.get("title"),
                        url: resolved_url.or_else(|| requested_url.clone()),
                        added_by: parse_added_by(row.get("added_by"))?,
                        snapshot_hash: row.get("snapshot_hash"),
                    },
                    kind: row.get("kind"),
                    origin: row.get("origin"),
                    requested_url,
                    content_sha256: row.get("content_sha256"),
                })
            })
            .collect()
    }

    /// The live web source the user already saved for `url`, by the address
    /// asked for or the one the page resolved to, compared as URLs (scheme
    /// and host case, default port and fragment do not count; the path's
    /// case does).
    pub async fn saved_web_source(
        &self,
        program_id: &str,
        url: &str,
    ) -> Result<Option<SavedSource>> {
        let wanted = identity(url);
        Ok(self
            .live_sources(program_id)
            .await?
            .into_iter()
            .find(|source| {
                source.kind == "web"
                    && [source.requested_url.as_deref(), source.saved.url.as_deref()]
                        .into_iter()
                        .flatten()
                        .any(|address| identity(address) == wanted)
            })
            .map(|source| source.saved))
    }

    /// The live source whose active text has this digest.
    pub async fn saved_with_digest(
        &self,
        program_id: &str,
        digest: &str,
    ) -> Result<Option<SavedSource>> {
        Ok(self
            .live_sources(program_id)
            .await?
            .into_iter()
            .find(|source| source.content_sha256 == digest)
            .map(|source| source.saved))
    }

    /// The live source captured from this library document.
    pub async fn saved_document(
        &self,
        program_id: &str,
        document_id: &str,
    ) -> Result<Option<SavedSource>> {
        Ok(self
            .live_sources(program_id)
            .await?
            .into_iter()
            .find(|source| source.kind == "document" && source.origin == document_id)
            .map(|source| source.saved))
    }

    /// The live source whose active version is `version_id`.
    pub async fn saved_version(
        &self,
        program_id: &str,
        version_id: &str,
    ) -> Result<Option<SavedSource>> {
        Ok(self
            .live_sources(program_id)
            .await?
            .into_iter()
            .find(|source| source.saved.version_id == version_id)
            .map(|source| source.saved))
    }

    /// Records the hash of the frozen page a version was read from.
    pub async fn link_snapshot(
        &self,
        program_id: &str,
        version_id: &str,
        sha256: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE learning_source_versions SET snapshot_hash=? WHERE program_id=? AND id=?",
        )
        .bind(sha256)
        .bind(program_id)
        .bind(version_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "user_evidence_tests.rs"]
mod tests;
