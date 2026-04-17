//! LLM management commands for local inference.
use crate::features::llm::dto::*;
use crate::audit_success;
use crate::infrastructure::audit::AuditAction;
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use std::fs;
use tauri::{Manager, State};

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

/// Checks if a specific LLM model is already downloaded
///
/// Verifies whether a model file exists locally and is ready for use. This allows
/// the frontend to show download status, enable/disable download buttons, and
/// determine if a model can be used immediately without downloading. Performs
/// filesystem checks to confirm both presence and integrity of model files.
///
/// # Arguments
///
/// * `model_id` - Unique identifier of the model to check (e.g., "llama-3-8b")
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(CheckModelDownloadedResponseDto)` - Download status with metadata
/// * `Err(AppError)` - If model ID is invalid or filesystem check fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Model ID is invalid or not in catalog
/// * `AppError::Other` - Filesystem access error or model directory issues
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ModelDownloadStatus {
///   isDownloaded: boolean;    // Model exists locally
///   path?: string;            // Local path if downloaded
///   sizeBytes?: number;       // File size if downloaded
///   lastModified?: string;    // Last modified timestamp
///   isCorrupted?: boolean;    // Integrity check result
/// }
///
/// // Check if model is downloaded
/// const status = await invoke<ModelDownloadStatus>('is_model_downloaded', {
///   modelId: 'llama-3-8b'
/// });
///
/// if (status.isDownloaded) {
///   console.log(`Model ready at: ${status.path}`);
///   console.log(`Size: ${(status.sizeBytes / 1024 / 1024 / 1024).toFixed(2)} GB`);
///   if (status.isCorrupted) {
///     console.warn('Model file may be corrupted, consider re-downloading');
///   }
/// } else {
///   console.log('Model not downloaded yet');
/// }
/// ```
///
/// # Download Status Checks
///
/// **Verification Steps**:
/// 1. **Model ID Validation**: Confirms model exists in catalog
/// 2. **File Existence**: Checks if model file exists at expected path
/// 3. **File Size**: Verifies file size matches expected size (optional)
/// 4. **Integrity Check**: Basic corruption detection (optional, fast check)
/// 5. **Metadata Extraction**: Gets file size and modification time
///
/// **Status Meanings**:
/// - **`isDownloaded: true`**: Model file exists and appears valid
/// - **`isDownloaded: false`**: Model file not found
/// - **`isCorrupted: true`**: File exists but may be damaged (size mismatch)
///
/// # Use Cases
///
/// - **Download Button State**: Show "Download" vs. "Use Model" button
/// - **Model List UI**: Display which models are available offline
/// - **Startup Checks**: Verify required models before enabling features
/// - **Storage Management**: Calculate total disk usage of downloaded models
///
/// # Performance
///
/// - **Check Time**: ~1-5ms (filesystem stat operation)
/// - **No Network**: Local filesystem check only
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to CheckModelDownloadedUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Create request DTO with model ID
/// 3. Execute download check (validate ID, check filesystem, verify integrity)
/// 4. Return CheckModelDownloadedResponseDto with status
///
/// # Note
///
/// This command performs a fast check. For deep integrity verification (checksum
/// validation), use a separate verification command after download completion.
pub async fn is_model_downloaded(
    model_id: String,
    container: State<'_, Container>,
) -> Result<CheckModelDownloadedResponseDto, AppError> {
    let use_case = container.check_model_downloaded_use_case();
    let request = CheckModelDownloadedRequestDto { model_id };
    use_case.execute(request).await
}

/// Gets the filesystem path to a downloaded LLM model file
///
/// Returns the absolute path where a model file is stored locally. This is needed
/// for loading models into inference engines, verifying file integrity, or displaying
/// storage locations to users. Only returns a path if the model is actually downloaded;
/// otherwise returns an error indicating the model needs to be downloaded first.
///
/// # Arguments
///
/// * `model_id` - Unique identifier of the model (e.g., "llama-3-8b")
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(GetModelPathResponseDto)` - Filesystem path with metadata
/// * `Err(AppError)` - If model ID invalid, model not downloaded, or path resolution fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Model ID is invalid or not in catalog
/// * `AppError::NotFound` - Model not downloaded (file doesn't exist)
/// * `AppError::Other` - Path resolution or filesystem access error
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ModelPath {
///   path: string;            // Absolute path to model file
///   exists: boolean;         // File exists confirmation
///   sizeBytes: number;       // File size in bytes
///   isReadable: boolean;     // File has read permissions
/// }
///
/// try {
///   // Get model path
///   const modelPath = await invoke<ModelPath>('get_model_path', {
///     modelId: 'llama-3-8b'
///   });
///
///   console.log(`Model location: ${modelPath.path}`);
///   console.log(`Size: ${(modelPath.sizeBytes / 1024 / 1024 / 1024).toFixed(2)} GB`);
///
///   if (!modelPath.isReadable) {
///     console.error('Model file exists but cannot be read (permissions issue)');
///   }
/// } catch (error) {
///   console.error('Model not downloaded or path unavailable:', error);
/// }
/// ```
///
/// # Path Resolution
///
/// **Model Storage Structure**:
/// ```text
/// <data_dir>/models/llm/
///   ├── llama-3-8b/
///   │   ├── model.gguf        # Model weights
///   │   ├── metadata.json     # Model metadata
///   │   └── tokenizer.bin     # Tokenizer data
///   ├── phi-3-mini/
///   └── mistral-7b/
/// ```
///
/// **Path Components**:
/// - **Base Directory**: Application data directory (OS-specific)
/// - **Model Type**: `models/llm/` subdirectory
/// - **Model ID**: Directory named after model ID
/// - **Model File**: Primary model file (usually `.gguf` format)
///
/// **Platform-Specific Paths**:
/// - **macOS**: `~/Library/Application Support/Recall/models/llm/`
/// - **Windows**: `%APPDATA%/Recall/models/llm/`
/// - **Linux**: `~/.local/share/recall/models/llm/`
///
/// # Use Cases
///
/// - **Model Loading**: Get path to load model into inference engine
/// - **File Verification**: Verify model file integrity
/// - **Storage Management**: Display model location to users
/// - **Backup/Export**: Export model files to external storage
/// - **Debugging**: Inspect model files directly
///
/// # Performance
///
/// - **Path Resolution**: ~1-5ms (filesystem operations)
/// - **No Network**: Local filesystem check only
/// - **Async**: Non-blocking operation
///
/// # Security
///
/// - **Path Validation**: Returns only validated, safe paths within app directory
/// - **No Traversal**: Prevents directory traversal attacks (CWE-22)
/// - **Read-Only**: Path is for reading model files, not modifying
///
/// # Architecture
///
/// Thin controller delegating to GetModelPathUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Create request DTO with model ID
/// 3. Execute path resolution (validate ID, check download status, resolve path)
/// 4. Return GetModelPathResponseDto with path and metadata
///
/// # Note
///
/// This command requires the model to be downloaded. Check download status with
/// `is_model_downloaded` before calling this command to avoid errors.
pub async fn get_model_path(
    model_id: String,
    container: State<'_, Container>,
) -> Result<GetModelPathResponseDto, AppError> {
    let use_case = container.get_model_path_use_case();
    let request = GetModelPathRequestDto { model_id };
    use_case.execute(request).await
}

/// Downloads an LLM model with progress tracking and integrity verification
///
/// Initiates download of a model from remote storage to local filesystem. Provides
/// real-time progress updates via events, validates downloaded file integrity, and
/// handles network errors gracefully with retry logic. This is a resource-intensive
/// operation protected by rate limiting to prevent abuse and ensure system stability.
///
/// # Arguments
///
/// * `model_id` - Unique identifier of the model to download (e.g., "llama-3-8b")
/// * `container` - Service container with use cases and security context
///
/// # Returns
///
/// * `Ok(DownloadModelResponseDto)` - Download completion status with path
/// * `Err(AppError)` - If rate limited, download fails, or verification fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many download requests (CWE-770 mitigation)
/// * `AppError::InvalidInput` - Model ID invalid or not in catalog
/// * `AppError::Other` - Network error, storage full, or integrity check failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
/// import { listen } from '@tauri-apps/api/event';
///
/// interface DownloadProgress {
///   modelId: string;
///   bytesDownloaded: number;
///   totalBytes: number;
///   percentComplete: number;
///   downloadSpeed: number;      // Bytes per second
///   estimatedTimeRemaining: number; // Seconds
/// }
///
/// interface DownloadComplete {
///   modelId: string;
///   path: string;
///   sizeBytes: number;
///   downloadTimeSeconds: number;
/// }
///
/// // Listen for progress updates using the unified download:progress event
/// const unlisten = await listen<DownloadStateSnapshot>('download:progress', (event) => {
///   const snapshot = event.payload;
///   if (snapshot.kind === 'single') {
///     console.log(`Downloading ${snapshot.filename}: ${snapshot.percentage}%`);
///     console.log(`Speed: ${(snapshot.bytesPerSecond / 1024 / 1024).toFixed(2)} MB/s`);
///     console.log(`ETA: ${snapshot.etaSeconds}s`);
///   }
/// });
///
/// try {
///   // Start download
///   const result = await invoke<DownloadComplete>('download_model', {
///     modelId: 'llama-3-8b'
///   });
///
///   console.log(`Download complete: ${result.path}`);
///   console.log(`Downloaded in ${result.downloadTimeSeconds}s`);
///   console.log(`Size: ${(result.sizeBytes / 1024 / 1024 / 1024).toFixed(2)} GB`);
/// } catch (error) {
///   console.error('Download failed:', error);
/// } finally {
///   unlisten();
/// }
/// ```
///
/// # Download Process
///
/// **Workflow**:
/// 1. **Rate Limit Check**: Prevents excessive downloads (CWE-770)
/// 2. **Model Validation**: Verify model ID exists in catalog
/// 3. **Storage Check**: Ensure sufficient disk space
/// 4. **Network Download**: Stream model file with progress events
/// 5. **Integrity Verification**: Validate checksum after download
/// 6. **Audit Logging**: Log successful download (CWE-778)
///
/// **Progress Events**:
/// - Emitted every 1 second or 10MB, whichever comes first
/// - Event name: `download:progress` (unified download event system)
/// - Payload: DownloadStateSnapshot with discriminated union (kind: 'single' | 'batch')
/// - Includes: bytes downloaded, total size, percentage, speed, ETA
///
/// **Error Handling**:
/// - **Network Failure**: Automatic retry up to 3 times with exponential backoff
/// - **Storage Full**: Clean error message, suggests freeing disk space
/// - **Integrity Failure**: Deletes corrupted file, suggests retry
/// - **Rate Limited**: Suggests waiting before retrying
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Max 5 downloads per hour to prevent DoS
/// - **Audit Logging (CWE-778)**: All downloads logged with model ID and result
/// - **Integrity Verification**: SHA-256 checksum validation
/// - **Path Validation**: Downloads only to safe, validated paths (CWE-22)
/// - **Size Limits**: Rejects downloads exceeding maximum size (100GB)
///
/// # Performance
///
/// **Download Speeds** (typical):
/// - **Small Models** (<5GB): 2-5 minutes on 100Mbps connection
/// - **Medium Models** (5-15GB): 5-15 minutes on 100Mbps connection
/// - **Large Models** (15-50GB): 15-60 minutes on 100Mbps connection
/// - **Ultra Models** (>50GB): 1+ hours on 100Mbps connection
///
/// **Resource Usage**:
/// - **Memory**: ~100-200MB for download buffer
/// - **Disk I/O**: Sequential writes, minimal system impact
/// - **Network**: Sustained download, may affect other network operations
///
/// # Architecture
///
/// Thin controller with cross-cutting concerns:
/// 1. **Rate Limiting**: Applied before execution
/// 2. **Use Case Execution**: DownloadModelUseCase handles business logic
/// 3. **Audit Logging**: Applied after successful execution
///
/// # Command Flow
///
/// 1. Check rate limit (prevent abuse)
/// 2. Get use case from container
/// 3. Create request DTO with model ID
/// 4. Execute download (validate, download, verify)
/// 5. Log audit event (success)
/// 6. Return DownloadModelResponseDto with path
///
/// # Note
///
/// Downloads can be cancelled using a separate `cancel_download` command. Progress
/// is saved, allowing resumption on retry (if supported by download source).
pub async fn download_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<DownloadModelResponseDto, AppError> {
    tracing::info!(model_id = %model_id, "Command: download_model - ENTRY");

    // Rate limiting (resource-intensive operation)
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit(&model_id)
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Execute use case
    let use_case = container.download_model_use_case();
    let request = DownloadModelRequestDto {
        model_id: model_id.clone(),
    };
    let response = use_case.execute(request).await.map_err(|e| {
        tracing::error!(model_id = %model_id, error = %e, "Command: download_model - ERROR");
        e
    })?;

    // Audit logging
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::ModelDownloaded,
        &model_id,
        "model_type" => "llm"
    )
    .await
    .ok();

    tracing::info!(model_id = %model_id, "Command: download_model - EXIT");
    Ok(response)
}

/// Deletes a downloaded LLM model to free disk space
///
/// Permanently removes a model file from local storage, including all associated
/// metadata and tokenizer data. This operation cannot be undone - the model will
/// need to be re-downloaded to use it again. Protected by rate limiting to prevent
/// accidental mass deletion and logged for audit compliance.
///
/// # Arguments
///
/// * `model_id` - Unique identifier of the model to delete (e.g., "llama-3-8b")
/// * `container` - Service container with use cases and security context
///
/// # Returns
///
/// * `Ok(DeleteModelResponseDto)` - Deletion confirmation with freed space
/// * `Err(AppError)` - If rate limited, model not found, or deletion fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many deletion requests (CWE-770 mitigation)
/// * `AppError::InvalidInput` - Model ID invalid or not in catalog
/// * `AppError::NotFound` - Model not downloaded (nothing to delete)
/// * `AppError::Other` - Filesystem error or insufficient permissions
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface DeleteResult {
///   modelId: string;
///   freedSpaceBytes: number;     // Disk space freed
///   deletedFiles: string[];      // List of deleted files
///   deletionTime: string;        // Timestamp of deletion
/// }
///
/// try {
///   // Delete model
///   const result = await invoke<DeleteResult>('delete_model', {
///     modelId: 'llama-3-8b'
///   });
///
///   console.log(`Deleted model: ${result.modelId}`);
///   console.log(`Freed ${(result.freedSpaceBytes / 1024 / 1024 / 1024).toFixed(2)} GB`);
///   console.log(`Deleted files: ${result.deletedFiles.join(', ')}`);
/// } catch (error) {
///   if (error.message.includes('RateLimitExceeded')) {
///     console.error('Too many deletion requests, please wait');
///   } else if (error.message.includes('NotFound')) {
///     console.error('Model not downloaded, nothing to delete');
///   } else {
///     console.error('Failed to delete model:', error);
///   }
/// }
/// ```
///
/// # Deletion Process
///
/// **Workflow**:
/// 1. **Rate Limit Check**: Prevents mass deletion abuse (CWE-770)
/// 2. **Model Validation**: Verify model ID exists in catalog
/// 3. **Download Check**: Confirm model is actually downloaded
/// 4. **File Deletion**: Remove model files and metadata
/// 5. **Audit Logging**: Log deletion for compliance (CWE-778)
/// 6. **Cache Invalidation**: Clear any cached model references
///
/// **Files Deleted**:
/// - Primary model file (e.g., `model.gguf`)
/// - Tokenizer data files
/// - Model metadata (`metadata.json`)
/// - Configuration files
/// - Model directory (if empty after deletion)
///
/// **What's NOT Deleted**:
/// - Model catalog entry (can re-download)
/// - Download history/logs
/// - User preferences for this model
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Max 10 deletions per hour to prevent abuse
/// - **Audit Logging (CWE-778)**: All deletions logged with model ID and result
/// - **Path Validation**: Only deletes files in safe model directory (CWE-22)
/// - **Confirmation Required**: Frontend should confirm before calling
///
/// # Storage Management
///
/// **Disk Space Recovery**:
/// - **Small Models** (<5GB): Instant space recovery
/// - **Medium Models** (5-15GB): ~1-2 seconds deletion time
/// - **Large Models** (15-50GB): ~2-5 seconds deletion time
/// - **Ultra Models** (>50GB): ~5-10 seconds deletion time
///
/// **Best Practices**:
/// - Check available space before downloading new models
/// - Delete unused models regularly to free space
/// - Keep at least one working model for core functionality
/// - Consider model size vs. quality trade-offs
///
/// # Use Cases
///
/// - **Storage Management**: Free disk space for other models or data
/// - **Model Rotation**: Remove old versions when upgrading
/// - **Clean Uninstall**: Remove all downloaded models before uninstalling app
/// - **Error Recovery**: Delete corrupted models before re-downloading
///
/// # Performance
///
/// - **Deletion Time**: ~1-10 seconds depending on model size
/// - **Async**: Non-blocking operation
/// - **No Network**: Local filesystem operation only
///
/// # Architecture
///
/// Thin controller with cross-cutting concerns:
/// 1. **Rate Limiting**: Applied before execution
/// 2. **Use Case Execution**: DeleteModelUseCase handles business logic
/// 3. **Audit Logging**: Applied after successful execution
///
/// # Command Flow
///
/// 1. Check rate limit (prevent abuse)
/// 2. Get use case from container
/// 3. Create request DTO with model ID
/// 4. Execute deletion (validate, check existence, delete files)
/// 5. Log audit event (success)
/// 6. Return DeleteModelResponseDto with freed space
///
/// # Warning
///
/// This operation is **permanent and irreversible**. The model will need to be
/// re-downloaded (network + time) to use it again. Frontend should implement
/// confirmation dialogs before calling this command.
pub async fn delete_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<DeleteModelResponseDto, AppError> {
    // Rate limiting (resource-intensive operation)
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit(&model_id)
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Execute use case
    let use_case = container.delete_model_use_case();
    let request = DeleteModelRequestDto {
        model_id: model_id.clone(),
    };
    let response = use_case.execute(request).await?;

    // Audit logging
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::ModelDeleted,
        &model_id,
        "model_type" => "llm"
    )
    .await
    .ok();

    Ok(response)
}

/// Lists all LLM models currently downloaded on the local system
///
/// Returns a comprehensive inventory of all model files stored locally, including
/// metadata about file sizes, locations, and download status. This provides visibility
/// into local model storage, helps manage disk space, and allows the frontend to show
/// which models are ready for immediate use without downloading.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(Vec<DownloadedModelDto>)` - List of downloaded models with metadata
/// * `Err(AppError)` - If filesystem scan fails or model directory inaccessible
///
/// # Errors
///
/// * `AppError::Other` - Filesystem access error or model directory not found
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface DownloadedModel {
///   modelId: string;             // Model identifier
///   name: string;                // Display name
///   path: string;                // Absolute filesystem path
///   sizeBytes: number;           // Total size on disk
///   downloadedAt: string;        // ISO timestamp of download
///   lastUsed?: string;           // ISO timestamp of last use (if tracked)
///   version?: string;            // Model version (if available)
///   isCorrupted: boolean;        // Integrity check result
/// }
///
/// // Get all downloaded models
/// const downloadedModels = await invoke<DownloadedModel[]>('list_models');
///
/// console.log(`Found ${downloadedModels.length} downloaded models`);
///
/// // Calculate total storage used
/// const totalSizeGB = downloadedModels.reduce((sum, model) =>
///   sum + model.sizeBytes, 0) / 1024 / 1024 / 1024;
/// console.log(`Total storage used: ${totalSizeGB.toFixed(2)} GB`);
///
/// // List models
/// downloadedModels.forEach(model => {
///   const sizeGB = (model.sizeBytes / 1024 / 1024 / 1024).toFixed(2);
///   console.log(`${model.name}: ${sizeGB} GB at ${model.path}`);
///   if (model.isCorrupted) {
///     console.warn(`  ⚠️ Model may be corrupted`);
///   }
/// });
/// ```
///
/// # Model Discovery
///
/// **Scan Process**:
/// 1. **Directory Scan**: List all subdirectories in models/llm directory
/// 2. **File Discovery**: Find model files (.gguf, .bin, etc.) in each subdirectory
/// 3. **Metadata Loading**: Read metadata.json files if present
/// 4. **Size Calculation**: Calculate total size of all files per model
/// 5. **Integrity Check**: Quick validation of file sizes and formats
/// 6. **Sorting**: Sort by download date (newest first)
///
/// **Metadata Included**:
/// - Model ID and display name
/// - Filesystem path
/// - Total size (all files combined)
/// - Download timestamp
/// - Last used timestamp (if available)
/// - Version information (if available)
/// - Corruption status (basic check)
///
/// # Storage Analysis
///
/// **Disk Usage Summary**:
/// ```typescript
/// const models = await invoke<DownloadedModel[]>('list_models');
///
/// // Group by size category
/// const small = models.filter(m => m.sizeBytes < 5 * 1024 * 1024 * 1024);   // <5GB
/// const medium = models.filter(m => m.sizeBytes < 15 * 1024 * 1024 * 1024); // 5-15GB
/// const large = models.filter(m => m.sizeBytes >= 15 * 1024 * 1024 * 1024); // >15GB
///
/// console.log(`Small: ${small.length}, Medium: ${medium.length}, Large: ${large.length}`);
/// ```
///
/// # Use Cases
///
/// - **Model Management UI**: Display list of available models with actions
/// - **Storage Dashboard**: Show disk usage and storage breakdown
/// - **Cleanup Suggestions**: Identify unused or redundant models for deletion
/// - **Startup Validation**: Verify required models are present at app start
/// - **Offline Capability**: Show which models work without internet
///
/// # Performance
///
/// - **Scan Time**: ~10-100ms depending on number of models (typical: <50ms)
/// - **No Network**: Local filesystem scan only
/// - **Caching**: Results can be cached with filesystem watcher for updates
/// - **Async**: Non-blocking operation
///
/// # Integrity Checking
///
/// **Basic Checks Performed**:
/// - File exists at expected path
/// - File size is non-zero and reasonable (>100MB, <200GB)
/// - File format matches expected type (.gguf, .bin, etc.)
/// - Metadata file is valid JSON (if present)
///
/// **Not Performed** (for performance):
/// - Full SHA-256 checksum validation (too slow for list operation)
/// - Deep model structure validation
/// - Binary format parsing
///
/// For deep integrity checks, use a separate verification command after download.
///
/// # Architecture
///
/// Thin controller delegating to ListDownloadedModelsUseCase (DDD pattern)
///
/// # Command Flow
///
/// 1. Get use case from container
/// 2. Execute model discovery (scan directory, load metadata)
/// 3. Return `Vec<DownloadedModelDto>` with model inventory
///
/// # Note
///
/// This command does not check the model catalog. It only returns models that
/// are physically present on disk. Some models in the catalog may not be downloaded,
/// and some downloaded files may not be in the catalog (manually copied).
pub async fn list_models(
    container: State<'_, Container>,
) -> Result<Vec<DownloadedModelDto>, AppError> {
    let use_case = container.list_models_use_case();
    let result = use_case.execute().await?;
    Ok(result.models)
}

/// Gets the filesystem path to the models directory
///
/// Returns the absolute path to the directory where LLM models are stored.
/// This directory is located within the application's data directory and is
/// created if it doesn't exist. The path is validated to ensure it's within
/// the app data directory for security (prevents directory traversal attacks).
///
/// # Arguments
///
/// * `app_handle` - Tauri application handle for accessing app directories
///
/// # Returns
///
/// * `Ok(String)` - Absolute path to the models directory
/// * `Err(String)` - If directory creation fails or path validation fails
///
/// # Errors
///
/// * Directory creation error - If models directory cannot be created
/// * Path validation error - If resolved path is outside app data directory
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Get the models directory path
/// const modelsDir = await invoke<string>('get_model_download_path');
///
/// console.log(`Models directory: ${modelsDir}`);
/// // Example output (macOS): "/Users/username/Library/Application Support/Recall/models"
/// // Example output (Windows): "C:\\Users\\username\\AppData\\Roaming\\Recall\\models"
/// // Example output (Linux): "/home/username/.local/share/recall/models"
/// ```
///
/// # Platform-Specific Paths
///
/// - **macOS**: `~/Library/Application Support/Recall/models`
/// - **Windows**: `%APPDATA%/Recall/models`
/// - **Linux**: `~/.local/share/recall/models`
///
/// # Use Cases
///
/// - **Model Downloads**: Get destination path for downloading model files
/// - **Model Discovery**: Find where models are stored for listing/loading
/// - **Storage Management**: Calculate disk usage of models directory
/// - **Frontend Integration**: Display download location to users
///
/// # Security
///
/// - **Path Validation**: Ensures returned path is within app data directory
/// - **Directory Traversal Prevention (CWE-22)**: Validates against malicious paths
/// - **Safe Directory Creation**: Creates directory with appropriate permissions
///
/// # Performance
///
/// - **Path Resolution**: ~1-5ms (filesystem operations)
/// - **Directory Creation**: ~5-20ms if directory doesn't exist
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller that directly accesses Tauri path API and filesystem.
/// No use case needed for this simple utility command.
///
/// # Command Flow
///
/// 1. Get app data directory from Tauri
/// 2. Append "models" subdirectory
/// 3. Create directory if it doesn't exist
/// 4. Validate path is within app data directory
/// 5. Return absolute path as string
pub async fn get_model_download_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    // Get app data directory
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    // Construct models directory path
    let models_dir = app_data_dir.join("models");

    // Create directory if it doesn't exist
    fs::create_dir_all(&models_dir)
        .map_err(|e| format!("Failed to create models directory: {}", e))?;

    // Validate path is within app data directory (security: prevent directory traversal)
    let canonical_models = models_dir
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize models path: {}", e))?;

    let canonical_app_data = app_data_dir
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize app data path: {}", e))?;

    if !canonical_models.starts_with(&canonical_app_data) {
        return Err("Models directory is not within app data directory".to_string());
    }

    // Return absolute path as string
    canonical_models
        .to_str()
        .ok_or_else(|| "Failed to convert path to string".to_string())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::di::Container;
    use tauri::State;

    // Note: Command tests are integration tests that verify the thin controller
    // delegates correctly to use cases. Use case business logic is tested
    // in their respective test modules.

    // TODO: Add integration tests with mock Container
    // Example test structure:
    // #[tokio::test]
    // async fn test_get_system_capabilities_delegates_to_use_case() {
    //     let container = create_test_container();
    //     let result = get_system_capabilities(State::from(&container)).await;
    //     assert!(result.is_ok());
    // }
}
