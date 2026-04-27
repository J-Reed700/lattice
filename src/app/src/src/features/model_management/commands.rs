//! # Model Management Commands (Phase 1 + Phase 2)
//!
//! Tauri IPC commands for AI model discovery, compatibility scoring, and recommendations.
//!
//! ## Phase 1 Commands (Curated Catalog)
//!
//! - `detect_system_capabilities` - Detect hardware (RAM, GPU, disk)
//! - `get_compatible_models` - Get compatible models for a category
//! - `get_all_recommended_models` - Get all compatible models across categories
//!
//! ## Phase 2 Commands (External Catalog)
//!
//! - `search_model_catalog` - Search external model catalogs (Hugging Face)
//! - `refresh_model_catalog` - Refresh catalog cache
//! - `clear_model_catalog_cache` - Clear all cached catalog entries
//! - `get_model_catalog_stats` - Get cache statistics
//!
//! ## Example Usage (TypeScript)
//!
//! ```typescript
//! import { invoke } from '@tauri-apps/api/core';
//!
//! // Detect system capabilities
//! const capabilities = await invoke('detect_system_capabilities');
//! console.log(`RAM: ${capabilities.total_ram_gb} GB`);
//!
//! // Get recommended LLM models
//! const models = await invoke('get_recommended_models', {
//!   category: 'LLM'
//! });
//!
//! for (const model of models) {
//!   console.log(`${model.model.name}: ${model.compatibility.compatibility_level}`);
//! }
//! ```

use crate::features::model_management::domain::ModelMetadata;
use crate::domain::{
    CompatibilityScorer, ModelCatalogService, ModelCategory, ModelRecommendation, ModelSource,
    SearchFilters, SystemCapabilities,
};
use crate::features::model_management::cache_adapter::ModelCatalogStats;
use crate::interfaces::di::Container;
use crate::interfaces::dto::{ModelRecommendationDto, ModelSearchResultDto};
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to get recommended models by category.
#[derive(Debug, Deserialize)]
pub struct GetRecommendedModelsRequest {
    /// Model category to filter by
    pub category: String,
}

/// System capabilities response.
#[derive(Debug, Serialize, specta::Type)]
pub struct SystemCapabilitiesResponse {
    pub total_ram_gb: f64,
    pub cpu_cores: u32,
    pub cpu_architecture: String,
    pub gpu_type: String,
    pub gpu_acceleration: String,
    pub vram_gb: Option<f64>,
    pub available_disk_gb: f64,
}

impl SystemCapabilitiesResponse {
    fn from_domain(caps: SystemCapabilities) -> Self {
        Self {
            total_ram_gb: caps.total_ram_gb,
            cpu_cores: caps.cpu_cores,
            cpu_architecture: caps.cpu_architecture.to_string(),
            gpu_type: caps.gpu_type.to_string(),
            gpu_acceleration: caps.gpu_acceleration.to_string(),
            vram_gb: caps.vram_gb,
            available_disk_gb: caps.available_disk_gb,
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Detect system hardware capabilities.
///
/// Returns comprehensive system information:
/// - RAM (total, available)
/// - CPU (cores, model)
/// - GPU (type, acceleration, VRAM)
/// - Disk (available space)
///
/// # Example
///
/// ```typescript
/// const capabilities = await invoke('detect_system_capabilities');
/// console.log(`RAM: ${capabilities.total_ram_gb} GB`);
/// console.log(`GPU: ${capabilities.gpu_type}`);
/// ```
#[tauri::command]
#[specta::specta]
pub async fn detect_system_capabilities(
    container: State<'_, Container>,
) -> Result<SystemCapabilitiesResponse> {
    // Get SystemInfoPort from container
    let system_info_port = container.system_info();

    // Get system info from port
    let system_info = system_info_port.get_system_info().await?;

    // Convert from SystemInfo (port) to SystemCapabilities (domain)
    // We need to map the port types to domain types
    let capabilities = SystemCapabilities {
        total_ram_gb: system_info.total_ram_gb,
        // available_ram_gb removed from domain - domain uses total_ram_gb only
        cpu_cores: system_info.cpu_cores,
        cpu_architecture: map_cpu_architecture(),
        gpu_type: map_gpu_type(&system_info.gpu_info),
        gpu_acceleration: map_gpu_acceleration(&system_info.gpu_info),
        vram_gb: system_info.gpu_info.as_ref().and_then(|g| g.vram_gb),
        available_disk_gb: get_available_disk_space(),
    };

    Ok(SystemCapabilitiesResponse::from_domain(capabilities))
}

/// Get compatible models for a specific category.
///
/// Returns models ranked by compatibility score, with detailed
/// compatibility analysis for each model.
///
/// # Arguments
/// - `category` - "LLM", "Embedding", or "OCR"
///
/// # Returns
/// List of model recommendations sorted by ranking score (best first).
///
/// # Example
///
/// ```typescript
/// const models = await invoke('get_compatible_models', {
///   category: 'LLM'
/// });
///
/// for (const model of models) {
///   console.log(`${model.model.name}: ${model.compatibility.compatibility_level}`);
///   console.log(`Score: ${model.ranking_score}/100`);
/// }
/// ```
#[tauri::command]
#[specta::specta]
pub async fn get_compatible_models(
    container: State<'_, Container>,
    category: String,
) -> Result<Vec<ModelRecommendationDto>> {
    // Parse category
    let category = parse_model_category(&category)?;

    // Get system capabilities
    let system_info_port = container.system_info();
    let system_info = system_info_port.get_system_info().await?;

    let capabilities = SystemCapabilities {
        total_ram_gb: system_info.total_ram_gb,
        // available_ram_gb removed from domain - domain uses total_ram_gb only
        cpu_cores: system_info.cpu_cores,
        cpu_architecture: map_cpu_architecture(),
        gpu_type: map_gpu_type(&system_info.gpu_info),
        gpu_acceleration: map_gpu_acceleration(&system_info.gpu_info),
        vram_gb: system_info.gpu_info.as_ref().and_then(|g| g.vram_gb),
        available_disk_gb: get_available_disk_space(),
    };

    // Get downloadable Hugging Face models and filter by category
    let models = fetch_downloadable_hf_models(container.inner(), "gguf", 200)
        .await?
        .into_iter()
        .filter(|entry| entry.model.category == category)
        .collect::<Vec<_>>();
    let downloads_by_model_id: HashMap<String, u64> = models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.downloads))
        .collect();
    let likes_by_model_id: HashMap<String, u64> = models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.likes))
        .collect();

    // Score compatibility for each model
    let scorer = CompatibilityScorer::new();
    let mut recommendations: Vec<ModelRecommendation> = models
        .into_iter()
        .filter_map(|entry| {
            scorer
                .score_compatibility(&entry.model, &capabilities)
                .ok()
                .map(|score| ModelRecommendation::new(entry.model, score))
        })
        .collect();

    // Sort by ranking score (descending)
    ModelRecommendation::sort_by_ranking(&mut recommendations);

    // Convert to DTOs
    let dto_recommendations: Vec<ModelRecommendationDto> = recommendations
        .into_iter()
        .map(|recommendation| {
            let popularity_downloads = downloads_by_model_id.get(&recommendation.model.id).copied();
            let popularity_likes = likes_by_model_id.get(&recommendation.model.id).copied();
            let mut dto: ModelRecommendationDto = recommendation.into();
            dto.popularity_downloads = popularity_downloads;
            dto.popularity_likes = popularity_likes;
            dto
        })
        .collect();

    Ok(dto_recommendations)
}

/// Get all recommended models across all categories.
///
/// Returns models ranked by compatibility, grouped by category.
///
/// # Example
///
/// ```typescript
/// const allModels = await invoke('get_all_recommended_models');
/// console.log(`Total compatible models: ${allModels.length}`);
/// ```
#[tauri::command]
#[specta::specta]
pub async fn get_all_recommended_models(
    container: State<'_, Container>,
) -> Result<Vec<ModelRecommendationDto>> {
    tracing::info!("get_all_recommended_models: Command invoked");

    // Get system capabilities
    let system_info_port = container.system_info();
    tracing::info!("get_all_recommended_models: Getting system info");
    let system_info = system_info_port.get_system_info().await?;
    tracing::info!(
        "get_all_recommended_models: Got system info - RAM: {}GB, CPU cores: {}",
        system_info.total_ram_gb,
        system_info.cpu_cores
    );

    let capabilities = SystemCapabilities {
        total_ram_gb: system_info.total_ram_gb,
        // available_ram_gb removed from domain - domain uses total_ram_gb only
        cpu_cores: system_info.cpu_cores,
        cpu_architecture: map_cpu_architecture(),
        gpu_type: map_gpu_type(&system_info.gpu_info),
        gpu_acceleration: map_gpu_acceleration(&system_info.gpu_info),
        vram_gb: system_info.gpu_info.as_ref().and_then(|g| g.vram_gb),
        available_disk_gb: get_available_disk_space(),
    };
    tracing::info!("get_all_recommended_models: Built system capabilities");

    // Get downloadable Hugging Face models. Two passes:
    //   - GGUF for chat LLMs (mistralrs runtime)
    //   - safetensors for embeddings (Candle runtime)
    // The old code searched "onnx embedding" because we used to ship ONNX
    // for embeddings; that query biased the results toward `onnx-community`
    // / `Xenova` re-export repos that ONLY ship .onnx (no safetensors).
    // After the Candle migration we need original sentence-transformer
    // upstreams that publish `model.safetensors`, so search for that
    // explicitly.
    tracing::info!("get_all_recommended_models: Getting Hugging Face models");
    let mut models = fetch_downloadable_hf_models(container.inner(), "gguf", 200).await?;

    let embedding_models = fetch_downloadable_hf_models(
        container.inner(),
        "sentence-transformers",
        200,
    )
    .await?;
    let existing_ids: std::collections::HashSet<String> =
        models.iter().map(|m| m.model.id.clone()).collect();
    for entry in embedding_models {
        if !existing_ids.contains(&entry.model.id) {
            models.push(entry);
        }
    }

    let downloads_by_model_id: HashMap<String, u64> = models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.downloads))
        .collect();
    let likes_by_model_id: HashMap<String, u64> = models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.likes))
        .collect();
    tracing::info!(
        "get_all_recommended_models: Got {} Hugging Face models",
        models.len()
    );

    // Score compatibility for each model.
    //
    // Embedding-model filtering: only surface Compatible models. Drops
    // both Incompatible (architecture not loadable / no safetensors) and
    // Unknown (we can't tell from tags, and download_model.rs's gate
    // rejects Unknown anyway — surfacing them in the catalog would let
    // users click a button that always errors).
    //
    // This replaces the older is_gguf_only_model heuristic which assumed
    // ONNX was the runtime — no longer true after the Candle migration.
    let scorer = CompatibilityScorer::new();
    let mut recommendations: Vec<ModelRecommendation> = models
        .into_iter()
        .filter(|entry| {
            if entry.model.category == ModelCategory::Embedding {
                use crate::features::embedding::compatibility::EmbeddingCompatibility;
                matches!(
                    entry.model.embedding_compatibility,
                    Some(EmbeddingCompatibility::Compatible { .. })
                )
            } else {
                true
            }
        })
        .filter_map(|entry| {
            scorer
                .score_compatibility(&entry.model, &capabilities)
                .ok()
                .map(|score| ModelRecommendation::new(entry.model, score))
        })
        .collect();
    tracing::info!(
        "get_all_recommended_models: Scored {} compatible models",
        recommendations.len()
    );

    // Sort by ranking score (descending)
    ModelRecommendation::sort_by_ranking(&mut recommendations);

    // Convert to DTOs
    let dto_recommendations: Vec<ModelRecommendationDto> = recommendations
        .into_iter()
        .map(|recommendation| {
            let popularity_downloads = downloads_by_model_id.get(&recommendation.model.id).copied();
            let popularity_likes = likes_by_model_id.get(&recommendation.model.id).copied();
            let mut dto: ModelRecommendationDto = recommendation.into();
            dto.popularity_downloads = popularity_downloads;
            dto.popularity_likes = popularity_likes;
            dto
        })
        .collect();
    tracing::info!(
        "get_all_recommended_models: Returning {} model recommendations",
        dto_recommendations.len()
    );

    Ok(dto_recommendations)
}

fn is_downloadable_model(model: &ModelMetadata) -> bool {
    model.default_filename.is_some() || !model.files.is_empty()
}

#[derive(Debug, Clone)]
struct DownloadableHfModel {
    model: ModelMetadata,
    downloads: u64,
    likes: u64,
}

async fn fetch_downloadable_hf_models(
    container: &Container,
    query: &str,
    limit: usize,
) -> Result<Vec<DownloadableHfModel>> {
    let discovery_query = if query.trim().is_empty() {
        "gguf"
    } else {
        query
    };
    let model_catalog = container.model_catalog();
    let mut external_metadata = model_catalog.search_models(discovery_query, limit).await?;
    let mut downloadable = external_metadata
        .iter()
        .filter_map(|meta| {
            meta.to_domain_model()
                .ok()
                .map(|model| DownloadableHfModel {
                    model,
                    downloads: meta.downloads,
                    likes: meta.likes,
                })
        })
        .filter(|entry| is_downloadable_model(&entry.model))
        .collect::<Vec<_>>();

    // Recovery path for stale/empty broad-catalog cache entries.
    // We only auto-invalidate for the broad "gguf" discovery query used by recommendations.
    if discovery_query.eq_ignore_ascii_case("gguf")
        && (external_metadata.is_empty()
            || downloadable.is_empty()
            || external_metadata
                .iter()
                .all(|meta| meta.preferred_filename.is_none()))
    {
        tracing::warn!(
            "Hugging Face query '{}' returned no downloadable models; invalidating catalog cache and retrying once",
            discovery_query
        );
        let model_catalog_cache = container.model_catalog_cache();
        if let Err(e) = model_catalog_cache.clear_all().await {
            tracing::warn!("Failed to clear model catalog cache for retry: {}", e);
        } else {
            external_metadata = model_catalog.search_models(discovery_query, limit).await?;
            downloadable = external_metadata
                .iter()
                .filter_map(|meta| {
                    meta.to_domain_model()
                        .ok()
                        .map(|model| DownloadableHfModel {
                            model,
                            downloads: meta.downloads,
                            likes: meta.likes,
                        })
                })
                .filter(|entry| is_downloadable_model(&entry.model))
                .collect();
        }
    }

    Ok(downloadable)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse model category string.
fn parse_model_category(category: &str) -> Result<ModelCategory> {
    match category.to_uppercase().as_str() {
        "LLM" => Ok(ModelCategory::LLM),
        "EMBEDDING" => Ok(ModelCategory::Embedding),
        "OCR" => Ok(ModelCategory::OCR),
        _ => Err(AppError::InvalidInput(format!(
            "Invalid model category: {}. Must be LLM, Embedding, or OCR",
            category
        ))),
    }
}

/// Map CPU architecture from current platform.
fn map_cpu_architecture() -> crate::domain::CpuArchitecture {
    #[cfg(target_arch = "aarch64")]
    {
        crate::domain::CpuArchitecture::ARM64
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        crate::domain::CpuArchitecture::X86_64
    }
}

/// Map GPU type from port GpuInfo.
fn map_gpu_type(gpu_info: &Option<crate::application::ports::GpuInfo>) -> crate::domain::GpuType {
    match gpu_info {
        Some(info) => match info.compute_type {
            crate::application::ports::ComputeType::Metal => crate::domain::GpuType::AppleSilicon,
            crate::application::ports::ComputeType::Cuda => crate::domain::GpuType::Nvidia,
            crate::application::ports::ComputeType::Rocm => crate::domain::GpuType::AMD,
            crate::application::ports::ComputeType::None => crate::domain::GpuType::None,
        },
        None => crate::domain::GpuType::None,
    }
}

/// Map GPU acceleration from port GpuInfo.
fn map_gpu_acceleration(
    gpu_info: &Option<crate::application::ports::GpuInfo>,
) -> crate::domain::GpuAcceleration {
    match gpu_info {
        Some(info) => match info.compute_type {
            crate::application::ports::ComputeType::Metal => crate::domain::GpuAcceleration::Metal,
            crate::application::ports::ComputeType::Cuda => crate::domain::GpuAcceleration::CUDA,
            crate::application::ports::ComputeType::Rocm => crate::domain::GpuAcceleration::ROCm,
            crate::application::ports::ComputeType::None => crate::domain::GpuAcceleration::None,
        },
        None => crate::domain::GpuAcceleration::None,
    }
}

/// Get available disk space (simplified - use first disk).
fn get_available_disk_space() -> f64 {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|disk| disk.available_space() as f64 / (1024.0 * 1024.0 * 1024.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_category_llm() {
        let result = parse_model_category("LLM").unwrap();
        assert_eq!(result, ModelCategory::LLM);
    }

    #[test]
    fn test_parse_model_category_embedding() {
        let result = parse_model_category("Embedding").unwrap();
        assert_eq!(result, ModelCategory::Embedding);
    }

    #[test]
    fn test_parse_model_category_ocr() {
        let result = parse_model_category("OCR").unwrap();
        assert_eq!(result, ModelCategory::OCR);
    }

    #[test]
    fn test_parse_model_category_invalid() {
        let result = parse_model_category("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_map_cpu_architecture() {
        let arch = map_cpu_architecture();
        // Just verify it returns a valid value
        assert!(matches!(
            arch,
            crate::domain::CpuArchitecture::ARM64 | crate::domain::CpuArchitecture::X86_64
        ));
    }
}

// ============================================================================
// Phase 2: External Model Catalog Commands
// ============================================================================

/// Request to search external model catalogs.
#[derive(Debug, Deserialize, specta::Type)]
pub struct SearchModelCatalogRequest {
    /// Search query (model name, keywords)
    pub query: String,
    /// Optional category filter
    pub category: Option<String>,
    /// Maximum size in GB
    pub max_size_gb: Option<f64>,
    /// Required capabilities
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

/// Search external model catalogs (Hugging Face).
///
/// Uses Hugging Face as the source of truth for downloadable models,
/// ranks them by relevance, and returns unified results.
///
/// # Features
/// - Searches Hugging Face (cached)
/// - Filters by category, size, capabilities
/// - Ranks by relevance score (0-100)
/// - Caches results for 1 hour (TTL)
/// - Graceful degradation on API errors
///
/// # Example
///
/// ```typescript
/// const results = await invoke('search_model_catalog', {
///   query: 'llama chat',
///   category: 'LLM',
///   max_size_gb: 5.0,
///   required_capabilities: ['chat', 'code'],
///   limit: 10
/// });
///
/// for (const result of results) {
///   console.log(`${result.model.name}: ${result.relevance_score}/100`);
///   console.log(`Source: ${result.source}`);
/// }
/// ```
#[tauri::command]
#[specta::specta]
pub async fn search_model_catalog(
    container: State<'_, Container>,
    request: SearchModelCatalogRequest,
) -> Result<Vec<ModelSearchResultDto>> {
    // Parse filters
    let category = if let Some(cat_str) = request.category {
        Some(parse_model_category(&cat_str)?)
    } else {
        None
    };

    let filters = SearchFilters::new(
        category,
        request.max_size_gb,
        request.required_capabilities,
        Some(request.query.clone()),
    )
    .map_err(AppError::InvalidInput)?;

    // Get external models from catalog port (cached Hugging Face)
    let discovered_models = fetch_downloadable_hf_models(
        container.inner(),
        &request.query,
        request.limit.saturating_mul(2).max(20),
    )
    .await?;
    let downloads_by_model_id: HashMap<String, u64> = discovered_models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.downloads))
        .collect();
    let likes_by_model_id: HashMap<String, u64> = discovered_models
        .iter()
        .map(|entry| (entry.model.id.clone(), entry.likes))
        .collect();
    let external_models: Vec<_> = discovered_models
        .into_iter()
        .map(|entry| (entry.model, ModelSource::External))
        // Embedding-model gate (search path): only surface models the
        // local Candle runtime can actually load. Compatible-only —
        // Unknown is filtered out because download_model.rs rejects it,
        // so showing it here would let users click a button that always
        // errors. Mirrors the gate in get_all_recommended_models.
        .filter(|(model, _)| {
            if model.category == ModelCategory::Embedding {
                use crate::features::embedding::compatibility::EmbeddingCompatibility;
                matches!(
                    model.embedding_compatibility,
                    Some(EmbeddingCompatibility::Compatible { .. })
                )
            } else {
                true
            }
        })
        .collect();

    // Use ModelCatalogService to search and rank
    let catalog_service = ModelCatalogService::new();
    let results = catalog_service.search_and_rank(external_models, &filters);

    // Apply limit
    let limited_results: Vec<_> = results.into_iter().take(request.limit).collect();

    // Convert to DTOs
    let dto_results: Vec<ModelSearchResultDto> = limited_results
        .into_iter()
        .map(|result| {
            let popularity_downloads = downloads_by_model_id.get(&result.model.id).copied();
            let popularity_likes = likes_by_model_id.get(&result.model.id).copied();
            let mut dto: ModelSearchResultDto = result.into();
            dto.popularity_downloads = popularity_downloads;
            dto.popularity_likes = popularity_likes;
            dto
        })
        .collect();

    Ok(dto_results)
}

/// Refresh model catalog cache.
///
/// Clears the catalog cache to force fresh API calls on next search.
///
/// # Example
///
/// ```typescript
/// await invoke('refresh_model_catalog');
/// console.log('Catalog cache refreshed');
/// ```
#[tauri::command]
#[specta::specta]
pub async fn refresh_model_catalog(container: State<'_, Container>) -> Result<()> {
    let model_catalog_cache = container.model_catalog_cache();
    let deleted = model_catalog_cache.clear_all().await?;
    tracing::info!(
        "Model catalog cache refresh requested ({} entries removed)",
        deleted
    );
    Ok(())
}

/// Clear model catalog cache.
///
/// Removes all cached catalog entries, forcing fresh API calls.
///
/// # Example
///
/// ```typescript
/// const deleted = await invoke('clear_model_catalog_cache');
/// console.log(`Cleared ${deleted} cache entries`);
/// ```
#[tauri::command]
#[specta::specta]
pub async fn clear_model_catalog_cache(container: State<'_, Container>) -> Result<u64> {
    let model_catalog_cache = container.model_catalog_cache();
    let deleted = model_catalog_cache.clear_all().await?;
    tracing::info!(
        "Model catalog cache clear requested ({} entries removed)",
        deleted
    );
    Ok(deleted)
}

/// Get model catalog cache statistics.
///
/// Returns metrics about cache usage and effectiveness.
///
/// # Example
///
/// ```typescript
/// const stats = await invoke('get_model_catalog_stats');
/// console.log(`Total: ${stats.total_entries}`);
/// console.log(`Valid: ${stats.valid_entries}`);
/// console.log(`Expired: ${stats.expired_entries}`);
/// ```
#[tauri::command]
#[specta::specta]
pub async fn get_model_catalog_stats(container: State<'_, Container>) -> Result<ModelCatalogStats> {
    let model_catalog_cache = container.model_catalog_cache();
    let stats = model_catalog_cache.get_stats().await?;
    tracing::info!(
        "Model catalog stats requested (total={}, valid={}, expired={})",
        stats.total_entries,
        stats.valid_entries,
        stats.expired_entries
    );
    Ok(stats)
}

// ============================================================================
// Downloaded Models Tracking (Phase 3)
// ============================================================================

// Re-export impl functions from model_management_commands module (for gateway dispatch)
pub use crate::features::model_management::commands_extra::{
    delete_downloaded_model_and_file_impl, get_active_chat_model_impl,
    get_models_with_metadata_impl, is_model_already_downloaded_impl, set_active_chat_model_impl,
    DownloadedModelResponse,
};
