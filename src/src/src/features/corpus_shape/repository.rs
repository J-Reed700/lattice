//! SQLite repository for cluster runs, clusters, and cluster members.
//!
//! Single transaction per run — we never want a half-written run showing up
//! in `list_clusters`. Reads are cheap; the typical path is "load the most
//! recent run, show its clusters."

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::features::corpus_shape::entity::{
    Cluster, ClusterMember, ClusterRun, LabelSource,
};
use crate::shared::error::{AppError, Result};

/// Port for persisting cluster runs + clusters. Kept narrow — `corpus_shape`
/// is the only feature that writes this, so the contract stays here.
#[async_trait]
pub trait ClusterRepositoryPort: Send + Sync {
    /// Insert a cluster run and its clusters + members in a single transaction.
    async fn save_run(
        &self,
        run: &ClusterRun,
        clusters: &[(Cluster, Vec<ClusterMember>)],
    ) -> Result<()>;

    /// Fetch the most recent cluster run (if any).
    async fn get_latest_run(&self) -> Result<Option<ClusterRun>>;

    /// Fetch all clusters for a given run.
    async fn get_clusters_for_run(&self, run_id: &str) -> Result<Vec<Cluster>>;

    /// Fetch members of a cluster.
    async fn get_members_for_cluster(&self, cluster_id: &str) -> Result<Vec<ClusterMember>>;
}

/// Production implementation backed by SQLite.
pub struct SqliteClusterRepository {
    pool: SqlitePool,
}

impl SqliteClusterRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn encode_centroid(centroid: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(centroid.len() * 4);
        for &v in centroid {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes
    }

    fn decode_centroid(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .filter_map(|c| {
                let arr: [u8; 4] = [
                    *c.first()?,
                    *c.get(1)?,
                    *c.get(2)?,
                    *c.get(3)?,
                ];
                Some(f32::from_le_bytes(arr))
            })
            .collect()
    }
}

#[async_trait]
impl ClusterRepositoryPort for SqliteClusterRepository {
    async fn save_run(
        &self,
        run: &ClusterRun,
        clusters: &[(Cluster, Vec<ClusterMember>)],
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("begin tx: {}", e)))?;

        let ran_at = run.ran_at.to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO cluster_runs (id, ran_at, doc_count, cluster_count, noise_count, params_hash, duration_ms, llm_calls)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&run.id)
        .bind(&ran_at)
        .bind(run.doc_count)
        .bind(run.cluster_count)
        .bind(run.noise_count)
        .bind(&run.params_hash)
        .bind(run.duration_ms)
        .bind(run.llm_calls)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("insert cluster_run: {}", e)))?;

        for (cluster, members) in clusters {
            let created_at = cluster.created_at.to_rfc3339();
            let centroid_bytes = Self::encode_centroid(&cluster.centroid);
            let label_source = cluster.label_source.as_str();

            sqlx::query(
                r#"
                INSERT INTO clusters (
                    id, run_id, label, description, member_count, centroid,
                    fingerprint, label_source, inherited_from_cluster_id, created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&cluster.id)
            .bind(&cluster.run_id)
            .bind(&cluster.label)
            .bind(&cluster.description)
            .bind(cluster.member_doc_ids.len() as i64)
            .bind(&centroid_bytes)
            .bind(&cluster.fingerprint)
            .bind(label_source)
            .bind(&cluster.inherited_from_cluster_id)
            .bind(&created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("insert cluster: {}", e)))?;

            for member in members {
                let is_rep = if member.is_representative { 1_i64 } else { 0_i64 };
                sqlx::query(
                    r#"
                    INSERT INTO cluster_members (cluster_id, document_id, membership_probability, is_representative)
                    VALUES (?, ?, ?, ?)
                    "#,
                )
                .bind(&cluster.id)
                .bind(&member.document_id)
                .bind(member.membership_probability as f64)
                .bind(is_rep)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("insert cluster_member: {}", e)))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("commit cluster run: {}", e)))?;
        Ok(())
    }

    async fn get_latest_run(&self) -> Result<Option<ClusterRun>> {
        let row_opt = sqlx::query(
            r#"
            SELECT id, ran_at, doc_count, cluster_count, noise_count, params_hash, duration_ms, llm_calls
            FROM cluster_runs
            ORDER BY ran_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("get_latest_run: {}", e)))?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let ran_at_str: String = row
            .try_get("ran_at")
            .map_err(|e| AppError::Database(format!("row.ran_at: {}", e)))?;
        let ran_at = DateTime::parse_from_rfc3339(&ran_at_str)
            .map_err(|e| AppError::Parsing(format!("ran_at parse: {}", e)))?
            .with_timezone(&Utc);

        Ok(Some(ClusterRun {
            id: row
                .try_get("id")
                .map_err(|e| AppError::Database(format!("row.id: {}", e)))?,
            ran_at,
            doc_count: row
                .try_get("doc_count")
                .map_err(|e| AppError::Database(format!("row.doc_count: {}", e)))?,
            cluster_count: row
                .try_get("cluster_count")
                .map_err(|e| AppError::Database(format!("row.cluster_count: {}", e)))?,
            noise_count: row
                .try_get("noise_count")
                .map_err(|e| AppError::Database(format!("row.noise_count: {}", e)))?,
            params_hash: row
                .try_get("params_hash")
                .map_err(|e| AppError::Database(format!("row.params_hash: {}", e)))?,
            duration_ms: row
                .try_get("duration_ms")
                .map_err(|e| AppError::Database(format!("row.duration_ms: {}", e)))?,
            llm_calls: row
                .try_get("llm_calls")
                .map_err(|e| AppError::Database(format!("row.llm_calls: {}", e)))?,
        }))
    }

    async fn get_clusters_for_run(&self, run_id: &str) -> Result<Vec<Cluster>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, label, description, centroid, fingerprint,
                   label_source, inherited_from_cluster_id, created_at
            FROM clusters
            WHERE run_id = ?
            ORDER BY id ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("get_clusters_for_run: {}", e)))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let cluster_id: String = row
                .try_get("id")
                .map_err(|e| AppError::Database(format!("row.id: {}", e)))?;

            // Pull member ids in a second query (keeps this query flat; row
            // count is small — one per cluster — so the N+1 is fine here).
            let member_rows = sqlx::query(
                r#"SELECT document_id FROM cluster_members WHERE cluster_id = ? ORDER BY document_id"#,
            )
            .bind(&cluster_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("members query: {}", e)))?;

            let mut member_doc_ids = Vec::with_capacity(member_rows.len());
            for mrow in member_rows {
                let doc_id: String = mrow
                    .try_get("document_id")
                    .map_err(|e| AppError::Database(format!("row.document_id: {}", e)))?;
                member_doc_ids.push(doc_id);
            }

            let created_at_str: String = row
                .try_get("created_at")
                .map_err(|e| AppError::Database(format!("row.created_at: {}", e)))?;
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| AppError::Parsing(format!("created_at parse: {}", e)))?
                .with_timezone(&Utc);

            let centroid_bytes: Vec<u8> = row
                .try_get("centroid")
                .map_err(|e| AppError::Database(format!("row.centroid: {}", e)))?;
            let centroid = Self::decode_centroid(&centroid_bytes);

            let label_source_str: String = row
                .try_get("label_source")
                .map_err(|e| AppError::Database(format!("row.label_source: {}", e)))?;

            out.push(Cluster {
                id: cluster_id,
                run_id: row
                    .try_get("run_id")
                    .map_err(|e| AppError::Database(format!("row.run_id: {}", e)))?,
                label: row
                    .try_get("label")
                    .map_err(|e| AppError::Database(format!("row.label: {}", e)))?,
                description: row
                    .try_get("description")
                    .map_err(|e| AppError::Database(format!("row.description: {}", e)))?,
                member_doc_ids,
                centroid,
                fingerprint: row
                    .try_get("fingerprint")
                    .map_err(|e| AppError::Database(format!("row.fingerprint: {}", e)))?,
                label_source: LabelSource::from_str(&label_source_str),
                inherited_from_cluster_id: row
                    .try_get("inherited_from_cluster_id")
                    .map_err(|e| {
                        AppError::Database(format!("row.inherited_from_cluster_id: {}", e))
                    })?,
                created_at,
            });
        }
        Ok(out)
    }

    async fn get_members_for_cluster(&self, cluster_id: &str) -> Result<Vec<ClusterMember>> {
        let rows = sqlx::query(
            r#"
            SELECT document_id, membership_probability, is_representative
            FROM cluster_members
            WHERE cluster_id = ?
            ORDER BY document_id
            "#,
        )
        .bind(cluster_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("get_members_for_cluster: {}", e)))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let document_id: String = row
                .try_get("document_id")
                .map_err(|e| AppError::Database(format!("row.document_id: {}", e)))?;
            let prob: f64 = row
                .try_get("membership_probability")
                .map_err(|e| AppError::Database(format!("row.prob: {}", e)))?;
            let is_rep: i64 = row
                .try_get("is_representative")
                .map_err(|e| AppError::Database(format!("row.is_rep: {}", e)))?;
            out.push(ClusterMember {
                document_id,
                membership_probability: prob as f32,
                is_representative: is_rep != 0,
            });
        }
        Ok(out)
    }
}
