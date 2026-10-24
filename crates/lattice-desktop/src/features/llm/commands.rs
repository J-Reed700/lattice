//! LLM management commands for local inference.
use crate::features::llm::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

mod installed;

pub use installed::{
    delete_model, get_model_download_path, get_model_path, is_model_downloaded, list_models,
};

/// Gets system hardware capabilities for LLM inference
///
/// Detects CPU, RAM, GPU, and OS information to determine what local LLM models
/// the system can run. Returns detailed capabilities including available RAM,
/// CPU cores, GPU memory (if available), and performance tier recommendations.
/// Used by frontend to suggest appropriate models and warn about system limitations.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(SystemCapabilitiesDto)` - System hardware capabilities
/// * `Err(AppError)` - If capability detection fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect hardware capabilities
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface SystemCapabilities {
///   totalRam: number;        // Total RAM in GB
///   availableRam: number;    // Available RAM in GB
///   cpuCores: number;        // Number of CPU cores
///   gpuAvailable: boolean;   // GPU detected
///   gpuMemory?: number;      // GPU VRAM in GB (if available)
///   os: string;              // Operating system (macos/windows/linux)
///   arch: string;            // CPU architecture (x86_64/arm64)
///   performanceTier: string; // low/medium/high/ultra
/// }
///
/// const capabilities = await invoke<SystemCapabilities>('get_system_capabilities');
///
/// console.log(`System has ${capabilities.totalRam}GB RAM`);
/// console.log(`Performance tier: ${capabilities.performanceTier}`);
/// console.log(`GPU available: ${capabilities.gpuAvailable}`);
/// ```
///
/// # System Capabilities
///
/// **Performance Tiers**:
/// - **`low`**: <8GB RAM, no GPU (small models only)
/// - **`medium`**: 8-16GB RAM, optional GPU (medium models)
/// - **`high`**: 16-32GB RAM, GPU recommended (large models)
/// - **`ultra`**: >32GB RAM, high-end GPU (largest models)
///
/// **Detection Methods**:
/// - **RAM**: System memory query via OS APIs
/// - **CPU**: Core count detection
/// - **GPU**: CUDA/Metal/Vulkan detection
/// - **OS**: Platform identification
///
/// # Use Cases
///
/// - **Model Recommendations**: Suggest models based on hardware
/// - **UI Warnings**: Alert users about insufficient resources
/// - **Performance Tuning**: Adjust inference settings
/// - **System Requirements**: Display minimum requirements
///
/// # Performance
///
/// - **Detection Time**: ~10-50ms
/// - **Caching**: Capabilities cached after first call
/// - **Async**: Non-blocking detection
///
/// # Architecture
///
/// Thin controller delegating to GetSystemCapabilitiesUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Execute capability detection
/// 3. Return SystemCapabilitiesDto
pub async fn get_system_capabilities(
    container: State<'_, Container>,
) -> Result<SystemCapabilitiesDto, AppError> {
    let use_case = container.get_system_capabilities_use_case();
    use_case.execute().await
}

/// Gets all available LLM models from the model catalog
///
/// Returns a comprehensive list of all LLM models that can be downloaded and used
/// for local inference. Each model includes metadata about size, capabilities,
/// performance characteristics, and system requirements. Used by frontend to display
/// the model selection UI and allow users to choose appropriate models for their needs.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(Vec<ModelInfoDto>)` - List of available models with metadata
/// * `Err(AppError)` - If catalog loading or parsing fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to load model catalog or parse model definitions
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ModelInfo {
///   id: string;              // Model identifier (e.g., "llama-3-8b")
///   name: string;            // Display name
///   description: string;     // Model description
///   sizeGb: number;         // Download size in GB
///   minRam: number;         // Minimum RAM required (GB)
///   performanceTier: string; // Recommended tier (low/medium/high/ultra)
///   capabilities: string[];  // Supported capabilities (chat, completion, etc.)
///   provider: string;        // Model provider (Meta, Anthropic, etc.)
/// }
///
/// // Get all available models
/// const models = await invoke<ModelInfo[]>('get_available_models');
///
/// console.log(`Found ${models.length} available models`);
/// models.forEach(model => {
///   console.log(`${model.name}: ${model.sizeGb}GB, requires ${model.minRam}GB RAM`);
/// });
/// ```
///
/// # Model Catalog
///
/// **Model Categories**:
/// - **Small Models** (<5GB): Fast, low resource usage, good for simple tasks
/// - **Medium Models** (5-15GB): Balanced performance and quality
/// - **Large Models** (15-50GB): High quality, requires significant resources
/// - **Ultra Models** (>50GB): State-of-the-art, requires high-end hardware
///
/// **Metadata Included**:
/// - Download size and storage requirements
/// - Minimum and recommended RAM
/// - Supported capabilities (chat, completion, function calling)
/// - Performance tier recommendations
/// - Provider information
/// - License details
///
/// # Use Cases
///
/// - **Model Selection UI**: Display available models with filtering
/// - **Compatibility Check**: Filter models by system capabilities
/// - **Download Queue**: Show models available for download
/// - **Model Comparison**: Compare specifications across models
///
/// # Performance
///
/// - **Catalog Loading**: ~10-50ms (cached after first load)
/// - **No Network**: Reads from local catalog file
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to GetAvailableModelsUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Execute catalog loading
/// 3. Parse model definitions
/// 4. Return `Vec<ModelInfoDto>`
pub async fn get_available_models(
    container: State<'_, Container>,
) -> Result<Vec<ModelInfoDto>, AppError> {
    let use_case = container.get_available_models_use_case();
    let result = use_case.execute().await?;
    Ok(result.models)
}

/// Gets recommended LLM models based on system capabilities
///
/// Returns a curated list of LLM models that are suitable for the system's hardware
/// capabilities. Can optionally filter by a specific performance tier. Models are ranked
/// by suitability, with the best matches first. Helps users avoid downloading models
/// that won't run well on their system and guides them toward optimal choices.
///
/// # Arguments
///
/// * `tier` - Optional performance tier filter (low/medium/high/ultra)
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(RecommendedModelsDto)` - Recommended models with reasoning
/// * `Err(AppError)` - If capability detection or recommendation logic fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect system capabilities or generate recommendations
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface RecommendedModels {
///   systemTier: string;              // Detected tier (low/medium/high/ultra)
///   recommendations: ModelRecommendation[];
/// }
///
/// interface ModelRecommendation {
///   model: ModelInfo;                // Model details
///   suitabilityScore: number;        // 0-100 score
///   reason: string;                  // Why this model is recommended
///   warnings?: string[];             // Potential issues (e.g., "May be slow")
/// }
///
/// // Get recommendations for detected system tier
/// const recommendations = await invoke<RecommendedModels>('get_recommended_models', {
///   tier: null  // Auto-detect
/// });
///
/// console.log(`System tier: ${recommendations.systemTier}`);
/// recommendations.recommendations.forEach(rec => {
///   console.log(`${rec.model.name}: ${rec.suitabilityScore}% - ${rec.reason}`);
/// });
///
/// // Get recommendations for specific tier
/// const highTierModels = await invoke<RecommendedModels>('get_recommended_models', {
///   tier: 'high'
/// });
/// ```
///
/// # Recommendation Algorithm
///
/// **Suitability Scoring**:
/// - **100%**: Perfect match for system capabilities
/// - **80-99%**: Good match, recommended
/// - **60-79%**: Acceptable, may have minor performance issues
/// - **<60%**: Not recommended, will likely have significant issues
///
/// **Factors Considered**:
/// - System RAM vs. model minimum/recommended RAM
/// - GPU availability for GPU-accelerated models
/// - CPU architecture compatibility (x86_64 vs. ARM)
/// - Storage space availability
/// - Model performance characteristics
///
/// **Tier-Based Filtering**:
/// - **Low Tier** (specified): Only models requiring <8GB RAM, no GPU
/// - **Medium Tier** (specified): Models requiring 8-16GB RAM, optional GPU
/// - **High Tier** (specified): Models requiring 16-32GB RAM, GPU recommended
/// - **Ultra Tier** (specified): Models requiring >32GB RAM, high-end GPU
/// - **Auto-Detect** (tier = null): Uses system-detected tier
///
/// # Use Cases
///
/// - **First-Time Setup**: Guide new users to appropriate models
/// - **Model Selection**: Filter model list to suitable options
/// - **Performance Optimization**: Suggest alternatives for better performance
/// - **Compatibility Warnings**: Alert users about potential issues
///
/// # Performance
///
/// - **Detection Time**: ~10-50ms (system capabilities cached)
/// - **Recommendation Time**: ~5-20ms (scoring algorithm)
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to GetRecommendedModelsUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Create request DTO with optional tier filter
/// 3. Execute recommendation logic (detect capabilities, score models, rank)
/// 4. Return RecommendedModelsDto with ranked suggestions
pub async fn get_recommended_models(
    tier: Option<PerformanceTier>,
    container: State<'_, Container>,
) -> Result<RecommendedModelsDto, AppError> {
    let use_case = container.get_recommended_models_use_case();
    let request = GetRecommendationsRequestDto { tier };
    use_case.execute(request).await
}

/// Gets the single best LLM model recommendation for the system
///
/// Returns the optimal LLM model for the current system's hardware capabilities.
/// This is a convenience command that selects the highest-ranked model from the
/// recommendations, providing a simple "just give me the best one" option for users
/// who don't want to choose from multiple options. Ideal for quick setup and defaults.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(BestModelDto)` - The best model recommendation with metadata
/// * `Err(AppError)` - If capability detection or recommendation logic fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect capabilities or no suitable model found
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface BestModel {
///   model: ModelInfo;           // Best model details
///   suitabilityScore: number;   // Suitability score (0-100)
///   reason: string;             // Why this model is best
///   systemTier: string;         // Detected system tier
///   warnings?: string[];        // Any compatibility warnings
/// }
///
/// // Get the best model for this system
/// const best = await invoke<BestModel>('get_best_model');
///
/// console.log(`Best model: ${best.model.name}`);
/// console.log(`Suitability: ${best.suitabilityScore}%`);
/// console.log(`Reason: ${best.reason}`);
///
/// if (best.warnings && best.warnings.length > 0) {
///   console.warn('Warnings:', best.warnings);
/// }
/// ```
///
/// # Selection Criteria
///
/// **Ranking Factors** (in priority order):
/// 1. **Suitability Score**: Models with higher scores preferred
/// 2. **System Compatibility**: Must meet minimum requirements
/// 3. **Performance Tier Match**: Matches detected system tier
/// 4. **Model Quality**: Larger, more capable models preferred within tier
/// 5. **Provider Reputation**: Established providers preferred
///
/// **Tie-Breaking**:
/// - If multiple models have the same suitability score, prefer:
///   - Larger models (better quality)
///   - Newer models (latest improvements)
///   - Models with GPU support (if GPU available)
///
/// # Use Cases
///
/// - **Quick Setup**: Automatic model selection for new users
/// - **Default Configuration**: Set default model in settings
/// - **CLI/Headless**: Non-interactive model selection
/// - **Testing**: Automated testing with consistent model choice
///
/// # Performance
///
/// - **Detection Time**: ~10-50ms (system capabilities cached)
/// - **Selection Time**: ~5-20ms (ranking algorithm)
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to GetBestModelUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Execute best model selection (detect capabilities, score all models, select top)
/// 3. Return BestModelDto with top recommendation
///
/// # Note
///
/// This command is equivalent to calling `get_recommended_models` and taking the
/// first result, but optimized to return a single model directly.
pub async fn get_best_model(container: State<'_, Container>) -> Result<BestModelDto, AppError> {
    let use_case = container.get_best_model_use_case();
    use_case.execute().await
}

#[cfg(test)]
mod tests;
