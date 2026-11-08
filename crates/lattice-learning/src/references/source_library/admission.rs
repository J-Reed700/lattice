//! The admission sweep (claim classes design §3.6): the gate's verdict on the
//! pages the library already holds. A page the gate refuses stays in the
//! library, listed for the user to delete, and leaves retrieval and citation
//! at once, so nothing is compared with it again.
use super::*;
use crate::curriculum_repository::LESSON_PREPARATION;
use crate::references::admission as gate;
use std::collections::HashSet;

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
        let rows = sqlx::query(
            "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, v.full_text, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND s.added_by<>'user' AND (s.added_by<>'outline' OR s.found_by_search=1) AND (v.admission_policy IS NULL OR v.admission_policy<>?) ORDER BY s.created_at,s.id",
        )
        .bind(program_id)
        .bind(gate::POLICY)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut refused = Vec::new();
        for row in rows {
            let source_id: String = row.get("source_id");
            let revision: i64 = row.get("revision");
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
                    revision,
                    version_id,
                    title,
                    url,
                    reason,
                });
            }
        }
        Ok(refused)
    }

    /// The pages the gate has read and admitted that hold fewer than `words`
    /// words, shortest first.
    pub async fn admitted_under(&self, program_id: &str, words: usize) -> Result<Vec<ShortSource>> {
        let rows = sqlx::query(
            "SELECT s.id AS source_id, v.id AS version_id, v.title, v.word_count, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL AND v.admission_policy=? AND v.word_count<? ORDER BY v.word_count, s.created_at, s.id",
        )
        .bind(program_id)
        .bind(gate::POLICY)
        .bind(i64::try_from(words).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| ShortSource {
                source_id: row.get("source_id"),
                version_id: row.get("version_id"),
                title: row.get("title"),
                url: row.get("url"),
                words: usize::try_from(row.get::<i64, _>("word_count")).unwrap_or_default(),
            })
            .collect())
    }

    /// The pages the gate has refused that the program still holds, for the
    /// user to delete.
    pub async fn refused_sources(&self, program_id: &str) -> Result<Vec<RefusedSource>> {
        let rows = sqlx::query(
            "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, v.not_a_source AS reason, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NOT NULL ORDER BY s.created_at,s.id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| RefusedSource {
                source_id: row.get("source_id"),
                revision: row.get("revision"),
                version_id: row.get("version_id"),
                title: row.get("title"),
                url: row.get("url"),
                reason: row.get("reason"),
            })
            .collect())
    }

    /// The sources research added that the gate admitted and no lesson cites
    /// now: no finding of a lesson's report rests on them, and no block,
    /// assessment item or recall card names them. Nothing is listed while a
    /// lesson is being prepared, as the job may be about to cite what research
    /// just added.
    pub async fn unused_research_sources(&self, program_id: &str) -> Result<Vec<UnusedSource>> {
        let preparing: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM jobs WHERE kind=? AND subject_id=? AND status IN ('pending','running') LIMIT 1",
        )
        .bind(LESSON_PREPARATION)
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        if preparing.is_some() {
            return Ok(Vec::new());
        }
        let rested_on: HashSet<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT DISTINCT json_extract(e.value, '$.source_id') FROM learning_lesson_verifications r, json_each(r.report_json, '$.findings') f, json_each(f.value, '$.evidence') e WHERE r.program_id=?",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .flatten()
        .collect();
        let rows = sqlx::query(
            "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND s.added_by=? AND v.not_a_source IS NULL ORDER BY s.created_at,s.id",
        )
        .bind(program_id)
        .bind(SourceAddedBy::Research.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut unused = Vec::new();
        for row in rows {
            let version_id: String = row.get("version_id");
            if rested_on.contains(&version_id)
                || !self
                    .version_usage(program_id, &version_id)
                    .await?
                    .is_empty()
            {
                continue;
            }
            unused.push(UnusedSource {
                source_id: row.get("source_id"),
                revision: row.get("revision"),
                title: row.get("title"),
                url: row.get("url"),
            });
        }
        Ok(unused)
    }
}
