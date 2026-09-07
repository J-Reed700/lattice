use crate::shared::error::Result;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RecencyConfig {
    pub max_age_days: i64,
    pub decay_factor: f32,
}

impl Default for RecencyConfig {
    fn default() -> Self {
        Self {
            max_age_days: 730, // 2 years
            decay_factor: 1.0,
        }
    }
}

impl RecencyConfig {
    pub fn new(max_age_days: i64, decay_factor: f32) -> Self {
        Self {
            max_age_days,
            decay_factor,
        }
    }

    pub fn with_max_age_days(max_age_days: i64) -> Self {
        Self {
            max_age_days,
            decay_factor: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RecencyScorer {
    config: RecencyConfig,
}

impl RecencyScorer {
    pub fn new(config: RecencyConfig) -> Self {
        Self { config }
    }
}

impl RecencyScorer {
    pub fn calculate_recency_score(&self, age_days: i64) -> f32 {
        if age_days < 0 {
            return 0.0;
        }

        let normalized_age = age_days as f32 / self.config.max_age_days as f32;
        let score = (1.0 - normalized_age).max(0.0);

        if self.config.decay_factor != 1.0 {
            score.powf(self.config.decay_factor)
        } else {
            score
        }
    }

    pub fn boost_score(&self, base_score: f32, recency_score: f32, recency_weight: f32) -> f32 {
        base_score * (1.0 - recency_weight) + recency_score * recency_weight
    }

    pub async fn get_document_timestamps(
        &self,
        pool: &SqlitePool,
        document_ids: &[String],
    ) -> Result<HashMap<String, DateTime<Utc>>> {
        if document_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders = document_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            r#"
            SELECT id, updated_at
            FROM documents
            WHERE id IN ({})
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query(&query);
        for id in document_ids {
            query_builder = query_builder.bind(id);
        }

        let rows = query_builder.fetch_all(pool).await.map_err(|e| {
            crate::error::AppError::Database(format!("Failed to fetch timestamps: {}", e))
        })?;

        let mut timestamps = HashMap::new();
        for row in rows {
            let id: String = row.try_get("id").map_err(|e| {
                crate::error::AppError::Database(format!("Failed to get document id: {}", e))
            })?;
            let updated_at: String = row.try_get("updated_at").map_err(|e| {
                crate::error::AppError::Database(format!("Failed to get updated_at: {}", e))
            })?;

            match crate::shared::time::parse_db_timestamp(&updated_at) {
                Ok(datetime) => {
                    timestamps.insert(id, datetime);
                }
                Err(error) => {
                    tracing::warn!(
                        document_id = %id,
                        timestamp = %updated_at,
                        error = %error,
                        "Skipping document with an invalid recency timestamp"
                    );
                }
            }
        }

        Ok(timestamps)
    }

    pub fn calculate_age_days(&self, timestamp: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
        (now - timestamp).num_days()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_recency_score_calculation() {
        let scorer = RecencyScorer::default();

        let score_new = scorer.calculate_recency_score(0);
        assert!((score_new - 1.0).abs() < 0.01);

        let score_mid = scorer.calculate_recency_score(365);
        assert!(score_mid > 0.4 && score_mid < 0.6);

        let score_old = scorer.calculate_recency_score(730);
        assert!(score_old < 0.1);

        let score_very_old = scorer.calculate_recency_score(1000);
        assert!(score_very_old < 0.01);
    }

    #[test]
    fn test_recency_score_with_decay() {
        let scorer = RecencyScorer::new(RecencyConfig {
            max_age_days: 365,
            decay_factor: 2.0,
        });

        let score_half = scorer.calculate_recency_score(182);
        assert!(score_half > 0.2 && score_half < 0.3);
    }

    #[test]
    fn test_boost_score() {
        let scorer = RecencyScorer::default();

        let base_score = 0.8;
        let recency_score = 1.0;
        let recency_weight = 0.1;

        let boosted = scorer.boost_score(base_score, recency_score, recency_weight);

        let expected = 0.8 * 0.9 + 1.0 * 0.1;
        assert!((boosted - expected).abs() < 0.001);
    }

    #[test]
    fn test_boost_score_with_old_document() {
        let scorer = RecencyScorer::default();

        let base_score = 0.8;
        let recency_score = 0.0;
        let recency_weight = 0.2;

        let boosted = scorer.boost_score(base_score, recency_score, recency_weight);

        let expected = 0.8 * 0.8 + 0.0 * 0.2;
        assert!((boosted - expected).abs() < 0.001);
    }

    #[test]
    fn test_age_calculation() {
        let scorer = RecencyScorer::default();
        let now = Utc::now();
        let past = now - Duration::days(30);

        let age = scorer.calculate_age_days(past, now);
        assert_eq!(age, 30);
    }

    #[test]
    fn test_parse_sqlite_datetime() {
        let datetime_str = "2024-01-15 10:30:45";
        let result = crate::shared::time::parse_db_timestamp(datetime_str);
        assert!(result.is_ok());

        let datetime_str_iso = "2024-01-15T10:30:45";
        let result_iso = crate::shared::time::parse_db_timestamp(datetime_str_iso);
        assert!(result_iso.is_ok());

        let result_offset = crate::shared::time::parse_db_timestamp("2024-01-15T05:30:45-05:00");
        assert!(result_offset.is_ok());
    }

    #[test]
    fn test_negative_age_returns_zero_score() {
        let scorer = RecencyScorer::default();
        let score = scorer.calculate_recency_score(-10);
        assert_eq!(score, 0.0);
    }
}
