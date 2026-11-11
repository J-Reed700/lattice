//! What claim review needs from the source library when the user attaches
//! evidence: the saved source a link, a file or a pasted text already is
//! (dedupe), and the frozen page a web version was read from.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.4.
use super::repository::evidence as stored;
use super::*;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::http::url_identity::identity;

/// What the standing judgement's key is computed from for one live source's
/// active version (claim sources design §4.3, §7.3): its text, who added it
/// and why research selected it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChosenVersion {
    pub source_id: String,
    pub version_id: String,
    pub text: String,
    pub added_by: SourceAddedBy,
    pub selection_note: Option<String>,
}

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

impl LearningSourceLibraryRepository {
    /// The digest a captured text is stored under (`content_sha256`).
    pub fn text_digest(text: &str) -> String {
        sha256_hex(normalized(text))
    }

    async fn live_sources(&self, program_id: &str) -> Result<Vec<stored::LiveSource>> {
        Self::validate_program_id(program_id)?;
        stored::live_sources(&self.database, program_id).await
    }

    /// Who added each live source, keyed by its active version: what the
    /// standing scope reads without a model (claim sources design §7.3).
    pub async fn added_by_version(
        &self,
        program_id: &str,
    ) -> Result<HashMap<String, SourceAddedBy>> {
        Ok(self
            .live_sources(program_id)
            .await?
            .into_iter()
            .map(|source| (source.saved.version_id, source.saved.added_by))
            .collect())
    }

    /// Every live source's active version with its text, who added it and
    /// the planner's note: what the standing scope keys stored standing
    /// judgements by, without a model.
    pub async fn chosen_versions(&self, program_id: &str) -> Result<Vec<ChosenVersion>> {
        Self::validate_program_id(program_id)?;
        stored::chosen_versions(&self.database, program_id).await
    }

    /// One version of a program's source with its text, who added the
    /// source and why research selected it (claim sources design §4.3): what
    /// the version's standing judgement is keyed by. `None` when the version
    /// is not in the program.
    pub async fn chosen_version(
        &self,
        program_id: &str,
        version_id: &str,
    ) -> Result<Option<ChosenVersion>> {
        Self::validate_program_id(program_id)?;
        stored::chosen_version(&self.database, program_id, version_id).await
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
        stored::set_snapshot_hash(&self.database, program_id, version_id, sha256).await
    }
}

#[cfg(test)]
#[path = "user_evidence_tests.rs"]
mod tests;
