//! The admission sweep (claim classes design §3.6): the gate's verdict on the
//! pages the library already holds. A page the gate refuses stays in the
//! library, listed for the user to delete, and leaves retrieval and citation
//! at once, so nothing is compared with it again.
use super::repository::admission as stored;
use super::*;
use crate::curriculum_repository::LESSON_PREPARATION;
use crate::references::admission as gate;
use lattice_core::shared::persistence::Transactions;

/// A saved page that is not a source, with the reason the gate gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedSource {
    pub source_id: String,
    pub revision: i64,
    pub version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub reason: String,
}

/// A source research added that no lesson cites now: the statement that asked
/// for it was rewritten or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedSource {
    pub source_id: String,
    pub revision: i64,
    pub title: String,
    pub url: Option<String>,
}

impl From<RefusedSource> for LearningRemovableSourceDto {
    fn from(source: RefusedSource) -> Self {
        Self {
            source_id: source.source_id,
            revision: source.revision,
            title: source.title,
            url: source.url,
            reason: Some(source.reason),
        }
    }
}

impl From<UnusedSource> for LearningRemovableSourceDto {
    fn from(source: UnusedSource) -> Self {
        Self {
            source_id: source.source_id,
            revision: source.revision,
            title: source.title,
            url: source.url,
            reason: None,
        }
    }
}

/// A saved page the gate admitted that is short, with its size: what the
/// acceptance report lists beside the refused ones so the floors can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortSource {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub words: usize,
}

impl LearningSourceLibraryRepository {
    /// Reads, under the gate, the active version of every source the machine
    /// saved for the program (found by research or by the outline's research,
    /// or added by the Verifier) that the gate has not read, and marks those
    /// that are not sources. A page the user added, or chose when making the
    /// course, is theirs and is never marked. No model is asked. Returns the
    /// pages refused now.
    pub async fn sweep_admission(&self, program_id: &str) -> Result<Vec<RefusedSource>> {
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        let pages = stored::unadmitted(&mut tx, program_id, gate::POLICY).await?;
        let mut refused = Vec::new();
        for page in pages {
            let reason = gate::admit(
                page.url.as_deref().unwrap_or_default(),
                &page.title,
                &page.text,
            )
            .err()
            .map(|refusal| refusal.to_string());
            stored::mark_admission(&mut tx, program_id, &page.version_id, &reason, gate::POLICY)
                .await?;
            if let Some(reason) = reason {
                refused.push(RefusedSource {
                    source_id: page.source_id,
                    revision: page.revision,
                    version_id: page.version_id,
                    title: page.title,
                    url: page.url,
                    reason,
                });
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(refused)
    }

    /// The pages the gate has read and admitted that hold fewer than `words`
    /// words, shortest first.
    pub async fn admitted_under(&self, program_id: &str, words: usize) -> Result<Vec<ShortSource>> {
        stored::admitted_under(
            &self.pool,
            program_id,
            gate::POLICY,
            i64::try_from(words).unwrap_or(i64::MAX),
        )
        .await
    }

    /// The pages the gate has refused that the program still holds, for the
    /// user to delete.
    pub async fn refused_sources(&self, program_id: &str) -> Result<Vec<RefusedSource>> {
        stored::refused_sources(&self.pool, program_id).await
    }

    /// The sources research added that the gate admitted and no lesson cites
    /// now: no finding of a lesson's report rests on them, and no block,
    /// assessment item or recall card names them. Nothing is listed while a
    /// lesson is being prepared, as the job may be about to cite what research
    /// just added.
    pub async fn unused_research_sources(&self, program_id: &str) -> Result<Vec<UnusedSource>> {
        if stored::job_running(&self.pool, LESSON_PREPARATION, program_id).await? {
            return Ok(Vec::new());
        }
        let rested_on = stored::cited_by_reports(&self.pool, program_id).await?;
        let mut unused = Vec::new();
        for research in
            stored::admitted_added_by(&self.pool, program_id, SourceAddedBy::Research.as_str())
                .await?
        {
            if rested_on.contains(&research.version_id)
                || !super::repository::reads::version_usage(
                    &self.pool,
                    program_id,
                    &research.version_id,
                )
                .await?
                .is_empty()
            {
                continue;
            }
            unused.push(research.source);
        }
        Ok(unused)
    }
}
