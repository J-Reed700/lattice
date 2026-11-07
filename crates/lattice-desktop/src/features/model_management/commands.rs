//! # Model management commands
//!
//! Tauri IPC commands for AI model discovery, compatibility scoring, and recommendations.
//!
//! ## Curated catalog commands
//!
//! - `detect_system_capabilities` - Detect hardware (RAM, GPU, disk)
//! - `get_compatible_models` - Get compatible models for a category
//! - `get_all_recommended_models` - Get all compatible models across categories
//!
//! ## External catalog commands
//!
//! - `search_model_catalog` - Search external model catalogs (Hugging Face)
//! - `refresh_model_catalog` - Refresh catalog cache
//! - `clear_model_catalog_cache` - Clear all cached catalog entries
//! - `get_model_catalog_stats` - Get cache statistics

use crate::features::model_management::cache_adapter::ModelCatalogStats;
use crate::features::model_management::domain::ModelMetadata;
use crate::interfaces::di::Container;
use crate::interfaces::dto::{
    FitVerdictDto, ModelFileRefDto, ModelRecommendationDto, ModelSearchResultDto, RecommendationDto,
};
use lattice_core::domain::{
    ModelCatalogService, ModelCategory, ModelSource, SearchFilters, SystemCapabilities,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

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

/// Detect system hardware capabilities.
///
/// Returns comprehensive system information:
/// - RAM (total, available)
/// - CPU (cores, model)
/// - GPU (type, acceleration, VRAM)
/// - Disk (available space)
#[tauri::command]
#[specta::specta]
pub async fn detect_system_capabilities(
    container: State<'_, Container>,
) -> Result<SystemCapabilitiesResponse> {
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

    Ok(SystemCapabilitiesResponse::from_domain(capabilities))
}

/// Get the catalog's models for a specific category, with the popularity the provider reports and,
/// for chat models, whether each fits this computer. Chat models come in the recommender's order,
/// the others in the catalog's.
#[tauri::command]
#[specta::specta]
pub async fn get_compatible_models(
    container: State<'_, Container>,
    category: String,
) -> Result<Vec<ModelRecommendationDto>> {
    let category = parse_model_category(&category)?;

    let models = if category == ModelCategory::Embedding {
        fetch_embedding_hf_models(container.inner(), 500).await?
    } else {
        fetch_downloadable_hf_models(container.inner(), "gguf", 200).await?
    }
    .into_iter()
    .filter(|entry| entry.model.category == category)
    .collect::<Vec<_>>();

    let context = FitContext::new(container.inner()).await?;
    let mut rows = context.rows(models).await;
    if category == ModelCategory::LLM {
        let order = context.recommended_order().await;
        order_by_recommendation(&mut rows, &order);
    }
    Ok(rows)
}

/// The chat models recommended for this computer, best first, each with its fit, the device it was
/// judged against and the labels it earned.
#[tauri::command]
#[specta::specta]
pub async fn recommend_chat_models(
    container: State<'_, Container>,
) -> Result<Vec<RecommendationDto>> {
    let settings = container.get_settings_use_case().execute().await?;
    let probe = container.machine_profile_probe();
    Ok(super::recommend::recommend_chat_models(
        probe.as_ref(),
        chat_kv_cache_quantization(&settings.llm),
    )
    .await)
}

/// Whether the user's saved launch settings for the chat model quantize its KV cache.
fn chat_kv_cache_quantization(
    llm: &lattice_core::application::contracts::settings::LLMSettingsDto,
) -> bool {
    llm.local_model_configs
        .get(&llm.model)
        .is_some_and(|config| config.kv_cache_quantization)
}

/// Get all the catalog's models across all categories.
#[tauri::command]
#[specta::specta]
pub async fn get_all_recommended_models(
    container: State<'_, Container>,
) -> Result<Vec<ModelRecommendationDto>> {
    let mut models = fetch_downloadable_hf_models(container.inner(), "gguf", 200).await?;
    merge_unique_models(
        &mut models,
        fetch_embedding_hf_models(container.inner(), 500).await?,
    );
    tracing::info!(
        "get_all_recommended_models: returning {} models",
        models.len()
    );

    let context = FitContext::new(container.inner()).await?;
    Ok(context.rows(models).await)
}

/// Whether each GGUF file fits this computer, in the order asked. A file whose header is not yet
/// known is read (by range request, once: the answer is kept on disk); `null` means the header
/// could not be read.
#[tauri::command]
#[specta::specta]
pub async fn read_model_fits(
    container: State<'_, Container>,
    files: Vec<ModelFileRefDto>,
) -> Result<Vec<Option<FitVerdictDto>>> {
    use futures::stream::{self, StreamExt};

    let settings = container.get_settings_use_case().execute().await?;
    let quantized = chat_kv_cache_quantization(&settings.llm);
    let profile = container.machine_profile_probe().profile().await;
    let source = container.gguf_header_source();
    let (profile, source) = (&profile, source.as_ref());
    Ok(stream::iter(files)
        .map(|file| async move {
            if !is_plain_repo(&file.repo) || !is_plain_file(&file.file) {
                return None;
            }
            super::recommend::file_fit(
                &file.repo,
                &file.file,
                file.revision.as_deref(),
                profile,
                source,
                quantized,
            )
            .await
        })
        .buffered(FIT_READS_AT_ONCE)
        .collect::<Vec<_>>()
        .await)
}

/// How many headers are read at once for one `read_model_fits` call.
const FIT_READS_AT_ONCE: usize = 4;

/// What every row of a list is judged with: this computer's profile, the KV setting and the
/// headers already read.
struct FitContext {
    profile: lattice_core::domain::models::fit::MachineProfile,
    kv_cache_quantization: bool,
    headers: super::header_source::CachedHeadersOnly,
}

impl FitContext {
    async fn new(container: &Container) -> Result<Self> {
        let settings = container.get_settings_use_case().execute().await?;
        Ok(Self {
            profile: container.machine_profile_probe().profile().await,
            kv_cache_quantization: chat_kv_cache_quantization(&settings.llm),
            headers: super::header_source::CachedHeadersOnly(container.gguf_header_source()),
        })
    }

    async fn fit(&self, model: &ModelMetadata) -> Option<FitVerdictDto> {
        super::recommend::row_fit(
            model,
            &self.profile,
            &self.headers,
            self.kv_cache_quantization,
        )
        .await
    }

    async fn rows(&self, models: Vec<DownloadableHfModel>) -> Vec<ModelRecommendationDto> {
        let mut rows = Vec::with_capacity(models.len());
        for entry in models {
            let fit = self.fit(&entry.model).await;
            rows.push(catalog_row(entry, fit));
        }
        rows
    }

    /// The ids of the chat models the recommender offers, best first.
    async fn recommended_order(&self) -> Vec<String> {
        let probe =
            crate::features::model_management::recommend::FixedMachineProfile(self.profile.clone());
        super::recommend::recommend_chat_models(&probe, self.kv_cache_quantization)
            .await
            .into_iter()
            .map(|recommendation| recommendation.model_id)
            .collect()
    }
}

/// Puts the rows the recommender offers first, in its order; the rest keep the order they came in.
fn order_by_recommendation(rows: &mut [ModelRecommendationDto], order: &[String]) {
    rows.sort_by_key(|row| {
        order
            .iter()
            .position(|id| *id == row.model.id)
            .unwrap_or(usize::MAX)
    });
}

/// A Hugging Face repository such as `publisher/model`, with nothing that could leave it.
fn is_plain_repo(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && !part.contains("..")
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// A file path inside a repository: no way out of it and no empty parts.
fn is_plain_file(file: &str) -> bool {
    !file.is_empty()
        && !file.contains('\\')
        && file
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn catalog_row(entry: DownloadableHfModel, fit: Option<FitVerdictDto>) -> ModelRecommendationDto {
    ModelRecommendationDto {
        model: with_fit(entry.model, fit.clone()),
        popularity_downloads: Some(entry.downloads),
        popularity_likes: Some(entry.likes),
        fit,
    }
}

/// A catalog row's metadata for the frontend, carrying its verdict.
fn with_fit(
    model: ModelMetadata,
    fit: Option<FitVerdictDto>,
) -> crate::interfaces::dto::ModelMetadataDto {
    let mut dto: crate::interfaces::dto::ModelMetadataDto = model.into();
    dto.fit = fit;
    dto
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

/// Discover embedding repositories through the Hub's task indexes. Searching
/// for the literal phrase `sentence-transformers` only covers one library and
/// misses models published for the standard feature-extraction and
/// sentence-similarity pipelines.
async fn fetch_embedding_hf_models(
    container: &Container,
    limit_per_task: usize,
) -> Result<Vec<DownloadableHfModel>> {
    let mut models = Vec::new();
    for query in ["filter:feature-extraction", "filter:sentence-similarity"] {
        let discovered = fetch_hf_models(container, query, limit_per_task, false).await?;
        merge_unique_models(&mut models, discovered);
    }
    Ok(models)
}

fn merge_unique_models(
    destination: &mut Vec<DownloadableHfModel>,
    incoming: Vec<DownloadableHfModel>,
) {
    let mut existing: HashSet<String> = destination
        .iter()
        .map(|entry| entry.model.id.clone())
        .collect();
    for entry in incoming {
        if existing.insert(entry.model.id.clone()) {
            destination.push(entry);
        }
    }
}

async fn fetch_downloadable_hf_models(
    container: &Container,
    query: &str,
    limit: usize,
) -> Result<Vec<DownloadableHfModel>> {
    let mut models = fetch_hf_models(container, query, limit, true).await?;
    merge_curated_chat_models(&mut models, query);
    Ok(models)
}

/// Put the catalog's chat entries into a Hugging Face listing.
///
/// A row for a catalog repository becomes the catalog entry, keeping the
/// repository's download and like counts: its footprint is pinned from the
/// GGUF header, its download is checked against
/// a pinned digest, and it is the id first run installs, so the catalog and
/// first run never offer the same file twice. Entries the search matched but
/// Hugging Face did not return are added, so a 22 GB model outside the top
/// results is still one click away.
fn merge_curated_chat_models(models: &mut Vec<DownloadableHfModel>, query: &str) {
    use lattice_core::domain::models::curated::{
        curated_chat_models_matching, get_curated_llm_models,
    };

    let catalog = get_curated_llm_models();
    let mut unlisted = curated_chat_models_matching(query);
    for entry in models.iter_mut() {
        let Some(repo) = entry.model.model_id.as_deref() else {
            continue;
        };
        let Some(replacement) = catalog.iter().find(|model| {
            model
                .model_id
                .as_deref()
                .is_some_and(|catalog_repo| catalog_repo.eq_ignore_ascii_case(repo))
        }) else {
            continue;
        };
        unlisted.retain(|model| model.id != replacement.id);
        entry.model = replacement.clone();
    }
    models.extend(unlisted.into_iter().map(|model| DownloadableHfModel {
        model,
        downloads: 0,
        likes: 0,
    }));
    // Two rows of one catalog repository would now share an id.
    let mut seen = HashSet::new();
    models.retain(|entry| seen.insert(entry.model.id.clone()));
}

async fn fetch_hf_models(
    container: &Container,
    query: &str,
    limit: usize,
    require_downloadable: bool,
) -> Result<Vec<DownloadableHfModel>> {
    let discovery_query = if query.trim().is_empty() {
        "gguf"
    } else {
        query
    };
    let model_catalog = container.model_catalog();
    let mut external_metadata = model_catalog.search_models(discovery_query, limit).await?;
    let to_catalog_models =
        |metadata: &[lattice_core::application::ports::ExternalModelMetadata]| {
            metadata
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
                .filter(|entry| !require_downloadable || is_downloadable_model(&entry.model))
                .collect::<Vec<_>>()
        };
    let mut catalog_models = to_catalog_models(&external_metadata);

    // Recovery path for stale/empty broad-catalog cache entries.
    // We only auto-invalidate for the broad "gguf" discovery query used by recommendations.
    if discovery_query.eq_ignore_ascii_case("gguf")
        && (external_metadata.is_empty()
            || catalog_models.is_empty()
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
            catalog_models = to_catalog_models(&external_metadata);
        }
    }

    Ok(catalog_models)
}

/// Parse model category string.
fn parse_model_category(category: &str) -> Result<ModelCategory> {
    match category.to_uppercase().as_str() {
        "LLM" => Ok(ModelCategory::LLM),
        "EMBEDDING" => Ok(ModelCategory::Embedding),
        "OCR" => Ok(ModelCategory::OCR),
        "TRANSCRIPTION" => Ok(ModelCategory::Transcription),
        "SPEECH" => Ok(ModelCategory::Speech),
        _ => Err(AppError::InvalidInput(format!(
            "Invalid model category: {}. Must be LLM, Embedding, OCR, Transcription or Speech",
            category
        ))),
    }
}

/// Map CPU architecture from current platform.
fn map_cpu_architecture() -> lattice_core::domain::CpuArchitecture {
    #[cfg(target_arch = "aarch64")]
    {
        lattice_core::domain::CpuArchitecture::ARM64
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        lattice_core::domain::CpuArchitecture::X86_64
    }
}

/// Map GPU type from port GpuInfo.
fn map_gpu_type(
    gpu_info: &Option<lattice_core::application::ports::GpuInfo>,
) -> lattice_core::domain::GpuType {
    match gpu_info {
        Some(info) => match info.compute_type {
            lattice_core::application::ports::ComputeType::Metal => {
                lattice_core::domain::GpuType::AppleSilicon
            }
            lattice_core::application::ports::ComputeType::Cuda => {
                lattice_core::domain::GpuType::Nvidia
            }
            lattice_core::application::ports::ComputeType::Rocm => {
                lattice_core::domain::GpuType::AMD
            }
            lattice_core::application::ports::ComputeType::None => {
                lattice_core::domain::GpuType::None
            }
        },
        None => lattice_core::domain::GpuType::None,
    }
}

/// Map GPU acceleration from port GpuInfo.
fn map_gpu_acceleration(
    gpu_info: &Option<lattice_core::application::ports::GpuInfo>,
) -> lattice_core::domain::GpuAcceleration {
    match gpu_info {
        Some(info) => match info.compute_type {
            lattice_core::application::ports::ComputeType::Metal => {
                lattice_core::domain::GpuAcceleration::Metal
            }
            lattice_core::application::ports::ComputeType::Cuda => {
                lattice_core::domain::GpuAcceleration::CUDA
            }
            lattice_core::application::ports::ComputeType::Rocm => {
                lattice_core::domain::GpuAcceleration::ROCm
            }
            lattice_core::application::ports::ComputeType::None => {
                lattice_core::domain::GpuAcceleration::None
            }
        },
        None => lattice_core::domain::GpuAcceleration::None,
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
mod tests;

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
#[tauri::command]
#[specta::specta]
pub async fn search_model_catalog(
    container: State<'_, Container>,
    request: SearchModelCatalogRequest,
) -> Result<Vec<ModelSearchResultDto>> {
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
    )?;

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
        .map(|entry| {
            let source =
                if lattice_core::domain::models::curated::get_model_category_by_id(&entry.model.id)
                    .is_some()
                {
                    ModelSource::Curated
                } else {
                    ModelSource::External
                };
            (entry.model, source)
        })
        .collect();

    let catalog_service = ModelCatalogService::new();
    let results = catalog_service.search_and_rank(external_models, &filters);

    let limited_results: Vec<_> = results.into_iter().take(request.limit).collect();

    let context = FitContext::new(container.inner()).await?;
    let mut dto_results: Vec<ModelSearchResultDto> = Vec::with_capacity(limited_results.len());
    for result in limited_results {
        let popularity_downloads = downloads_by_model_id.get(&result.model.id).copied();
        let popularity_likes = likes_by_model_id.get(&result.model.id).copied();
        let fit = context.fit(&result.model).await;
        let mut dto: ModelSearchResultDto = result.into();
        dto.popularity_downloads = popularity_downloads;
        dto.popularity_likes = popularity_likes;
        dto.model.fit = fit;
        dto_results.push(dto);
    }

    Ok(dto_results)
}

/// List the actual standalone GGUF files published in a Hugging Face repository.
/// Sizes come from the repository; each file carries its fit once its header is known.
#[tauri::command]
#[specta::specta]
pub async fn get_model_variants(
    container: State<'_, Container>,
    repo_id: String,
) -> Result<Vec<crate::interfaces::dto::ModelMetadataDto>> {
    let parts: Vec<_> = repo_id.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || part.contains("..")
                || !part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
    {
        return Err(AppError::InvalidInput(
            "Expected a Hugging Face repository such as publisher/model".into(),
        ));
    }
    let context = FitContext::new(container.inner()).await?;
    let mut variants = Vec::new();
    for metadata in container
        .model_catalog()
        .get_model_variants(&repo_id)
        .await?
    {
        // The catalog's own file keeps its catalog id and figures, so
        // picking it here is the same download first run makes.
        let curated = metadata.preferred_filename.as_deref().and_then(|file| {
            lattice_core::domain::models::curated::curated_chat_model_for_file(&repo_id, file)
        });
        let model = match curated {
            Some(model) => model,
            None => metadata.to_domain_model()?,
        };
        let fit = context.fit(&model).await;
        variants.push(with_fit(model, fit));
    }
    Ok(variants)
}

/// Refresh model catalog cache.
///
/// Clears the catalog cache to force fresh API calls on next search.
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

pub use crate::features::model_management::commands_extra::{
    delete_downloaded_model_and_file_impl, get_active_chat_model_impl,
    get_models_with_metadata_impl, is_model_already_downloaded_impl, set_active_chat_model_impl,
    DownloadedModelResponse,
};
