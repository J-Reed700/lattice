//! # LLM DTOs
//!
//! Data Transfer Objects for LLM model management and system capabilities.
//!
//! ## Purpose
//!
//! - **System Capabilities**: Hardware detection for model recommendation
//! - **Model Catalog**: Available models with metadata
//! - **Downloaded Models**: Local model inventory
//!
//! ## DTOs
//!
//! - `SystemCapabilitiesDto` - System hardware capabilities
//! - `GpuInfoDto` - GPU information (Metal, CUDA, ROCm, or none)
//! - `AvailableModelsDto` - List of models in catalog
//! - `ModelInfoDto` - Model metadata and requirements
//! - `DownloadedModelsDto` - List of locally downloaded models
//! - `DownloadedModelDto` - Downloaded model information
//! - `DownloadModelRequestDto` - Request to download a model
//! - `DownloadModelResponseDto` - Response after model download
//! - `DeleteModelRequestDto` - Request to delete a model
//! - `DeleteModelResponseDto` - Response after model deletion

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// System hardware capabilities for model recommendation.
///
/// Contains information about RAM, CPU, and GPU to help
/// determine which models can run on the current system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCapabilitiesDto {
    /// Total system RAM in gigabytes
    pub total_ram_gb: f64,
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// CPU model name
    pub cpu_model: String,
    /// GPU information if available
    pub gpu_info: Option<GpuInfoDto>,
}

/// GPU information for hardware acceleration.
///
/// Provides details about the GPU for determining
/// compute capabilities and VRAM availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfoDto {
    /// GPU name/model
    pub name: String,
    /// VRAM in gigabytes (if detectable)
    pub vram_gb: Option<f64>,
    /// Compute type: "Metal", "CUDA", "ROCm", or "None"
    pub compute_type: String,
}

/// List of available models in the catalog.
///
/// Contains all models that can be downloaded and used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModelsDto {
    /// List of available models
    pub models: Vec<ModelInfoDto>,
}

/// Model metadata and requirements.
///
/// Describes a model's capabilities, size, and system requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoDto {
    /// Unique model identifier (e.g., "phi-3-mini")
    pub id: String,
    /// Human-readable model name (e.g., "Phi-3 Mini")
    pub name: String,
    /// Model size in gigabytes
    pub size_gb: f64,
    /// Maximum context length in tokens
    pub context_length: u32,
    /// Model capabilities (e.g., ["chat", "code", "reasoning"])
    pub capabilities: Vec<String>,
    /// Minimum RAM required to run model (in GB)
    pub minimum_ram_gb: f64,
}

/// List of locally downloaded models.
///
/// Contains all models currently available on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedModelsDto {
    /// List of downloaded models
    pub models: Vec<DownloadedModelDto>,
}

/// Information about a downloaded model.
///
/// Describes a model that has been downloaded to the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedModelDto {
    /// Model identifier
    pub model_id: String,
    /// Local filesystem path
    pub path: String,
    /// Model size in gigabytes
    pub size_gb: f64,
    /// When the model was downloaded
    pub downloaded_at: DateTime<Utc>,
}

/// Performance tier for model recommendations.
///
/// Categorizes systems by available RAM to recommend appropriate models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceTier {
    /// Low-end systems (<8GB RAM)
    Low,
    /// Medium-tier systems (8-16GB RAM)
    Medium,
    /// High-end systems (>16GB RAM)
    High,
}

/// Request for model recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecommendationsRequestDto {
    /// Performance tier (if None, auto-detect from system)
    pub tier: Option<PerformanceTier>,
}

/// List of recommended models based on system capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModelsDto {
    /// Top 3 recommended models
    pub recommendations: Vec<RecommendedModelDto>,
}

/// A single model recommendation with reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModelDto {
    /// Model metadata
    pub model: ModelInfoDto,
    /// Reason for recommendation
    pub reason: String,
    /// Performance tier this recommendation is for
    pub tier: PerformanceTier,
}

/// Best model for the current system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestModelDto {
    /// Best model metadata
    pub model: ModelInfoDto,
    /// Reason for selection
    pub reason: String,
}

/// Request to check if a model is downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckModelDownloadedRequestDto {
    /// Model identifier to check
    pub model_id: String,
}

/// Response indicating if a model is downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckModelDownloadedResponseDto {
    /// True if model is downloaded and valid
    pub is_downloaded: bool,
    /// Local filesystem path (if downloaded)
    pub local_path: Option<String>,
}

/// Request to get a model's filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPathRequestDto {
    /// Model identifier
    pub model_id: String,
}

/// Response containing model's filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPathResponseDto {
    /// Canonical path to model directory
    pub path: String,
}

/// Request to download a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModelRequestDto {
    /// Model identifier to download
    pub model_id: String,
}

/// Response after model download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModelResponseDto {
    /// Operation state indicating what happened
    pub state: crate::domain::download::DownloadOperationState,
    /// Local filesystem path to downloaded model (if available)
    pub path: Option<String>,
}

/// Request to delete a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelRequestDto {
    /// Model identifier to delete
    pub model_id: String,
}

/// Response after model deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelResponseDto {
    /// True if deletion succeeded
    pub success: bool,
    /// Amount of disk space freed in gigabytes
    pub freed_space_gb: f64,
}
