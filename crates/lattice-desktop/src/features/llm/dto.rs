use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// System hardware capabilities for model recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCapabilitiesDto {
    pub total_ram_gb: f64,
    pub cpu_cores: u32,
    pub cpu_model: String,
    pub gpu_info: Option<GpuInfoDto>,
}

/// GPU information for hardware acceleration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfoDto {
    pub name: String,
    pub vram_gb: Option<f64>,
    pub compute_type: String,
}

/// List of available models in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModelsDto {
    pub models: Vec<ModelInfoDto>,
}

/// Model metadata and requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoDto {
    pub id: String,
    pub name: String,
    pub size_gb: f64,
    pub context_length: u32,
    pub capabilities: Vec<String>,
    pub minimum_ram_gb: f64,
}

/// List of locally downloaded models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedModelsDto {
    pub models: Vec<DownloadedModelDto>,
}

/// Information about a downloaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedModelDto {
    pub model_id: String,
    pub path: String,
    pub size_gb: f64,
    pub downloaded_at: DateTime<Utc>,
}

/// Performance tier for model recommendations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceTier {
    Low,
    Medium,
    High,
}

/// Request for model recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecommendationsRequestDto {
    pub tier: Option<PerformanceTier>,
}

/// List of recommended models based on system capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModelsDto {
    pub recommendations: Vec<RecommendedModelDto>,
}

/// A single model recommendation with reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModelDto {
    pub model: ModelInfoDto,
    pub reason: String,
    pub tier: PerformanceTier,
}

/// Best model for the current system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestModelDto {
    pub model: ModelInfoDto,
    pub reason: String,
}

/// Request to check if a model is downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckModelDownloadedRequestDto {
    pub model_id: String,
}

/// Response indicating if a model is downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckModelDownloadedResponseDto {
    pub is_downloaded: bool,
    pub local_path: Option<String>,
}

/// Request to get a model's filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPathRequestDto {
    pub model_id: String,
}

/// Response containing model's filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPathResponseDto {
    pub path: String,
}

/// Request to download a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModelRequestDto {
    pub model_id: String,
}

/// Response after model download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModelResponseDto {
    pub state: lattice_core::domain::download::DownloadOperationState,
    pub path: Option<String>,
}

/// Request to delete a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelRequestDto {
    pub model_id: String,
}

/// Response after model deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelResponseDto {
    pub success: bool,
    pub freed_space_gb: f64,
}
