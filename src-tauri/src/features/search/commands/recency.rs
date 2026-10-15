//! Recency-weighted search and the boost it applies to results.

use crate::features::search::dto::{RecencySearchOptions, SearchRequestDto, SearchResultDto};
use crate::features::search::engine::recency::{RecencyConfig, RecencyScorer};
use crate::interfaces::di::container::Container;
use lattice_core::shared::error::AppError;

use super::enrichment::enrich_hybrid_results;

/// Lift recently updated documents by `recency_weight`, then re-sort. A
/// result whose document carries no readable `updated_at` keeps its score.
pub(super) fn boost_recent(
    results: &mut [SearchResultDto],
    recency_weight: f32,
    max_age_days: i64,
) {
    let recency_weight = recency_weight.clamp(0.0, 1.0);
    if results.is_empty() || recency_weight == 0.0 {
        return;
    }

    let scorer = RecencyScorer::new(RecencyConfig::with_max_age_days(max_age_days));
    let now = chrono::Utc::now();
    for result in results.iter_mut() {
        let updated_at = result
            .metadata
            .get("updated_at")
            .and_then(|value| value.as_str())
            .and_then(parse_sqlite_datetime);
        if let Some(updated_at) = updated_at {
            let age_days = scorer.calculate_age_days(updated_at, now);
            let recency_score = scorer.calculate_recency_score(age_days);
            result.score = scorer.boost_score(result.score, recency_score, recency_weight);
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn parse_sqlite_datetime(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&chrono::Utc));
    }

    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
        .map(|naive| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc))
}

/// Search with recency implementation for gateway pattern
#[tracing::instrument(skip(container), fields(query = %options.query))]
pub async fn search_with_recency_impl(
    container: &Container,
    options: RecencySearchOptions,
) -> Result<Vec<SearchResultDto>, AppError> {
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

    container
        .security_context()
        .input_validator()
        .validate_search_query(&options.query)?;

    let limit = options.limit.unwrap_or(10);
    let recency_weight = options.recency_weight.unwrap_or(0.1);
    let max_age_days = options.max_age_days.unwrap_or(730);

    if !(0.0..=1.0).contains(&recency_weight) {
        return Err(AppError::InvalidInput(
            "Recency weight must be between 0.0 and 1.0".to_string(),
        ));
    }

    if max_age_days <= 0 {
        return Err(AppError::InvalidInput(
            "Max age days must be positive".to_string(),
        ));
    }

    container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;

    let response = container
        .hybrid_search_use_case()
        .execute(SearchRequestDto::hybrid(options.query.clone(), limit))
        .await?;

    let enrichment_service = container.search_enrichment_service();
    let mut search_results = enrich_hybrid_results(response.results, enrichment_service).await?;
    boost_recent(&mut search_results, recency_weight, max_age_days);

    Ok(search_results)
}
