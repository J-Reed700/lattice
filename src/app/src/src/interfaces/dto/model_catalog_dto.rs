//! Model Catalog DTOs
//!
//! Data Transfer Objects for model catalog commands.
//! These types are exposed to the frontend via Tauri IPC and Specta type generation.

use serde::{Deserialize, Serialize};

// ============================================================================
// Enum DTOs
// ============================================================================

/// Model category DTO for frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum ModelCategoryDto {
    /// Large Language Model
    LLM,
    /// Embedding model
    Embedding,
    /// OCR model
    OCR,
    /// On-device speech-to-text model
    Transcription,
}

impl From<crate::features::model_management::domain::ModelCategory> for ModelCategoryDto {
    fn from(domain: crate::features::model_management::domain::ModelCategory) -> Self {
        match domain {
            crate::features::model_management::domain::ModelCategory::LLM => Self::LLM,
            crate::features::model_management::domain::ModelCategory::Embedding => Self::Embedding,
            crate::features::model_management::domain::ModelCategory::OCR => Self::OCR,
            crate::features::model_management::domain::ModelCategory::Transcription => {
                Self::Transcription
            }
        }
    }
}

/// Performance tier DTO for frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum PerformanceTierDto {
    /// Fast models (< 2s/response)
    Fast,
    /// Balanced speed/quality (2-5s/response)
    Balanced,
    /// Accurate models (> 5s/response)
    Accurate,
}

impl From<crate::features::model_management::domain::PerformanceTier> for PerformanceTierDto {
    fn from(domain: crate::features::model_management::domain::PerformanceTier) -> Self {
        match domain {
            crate::features::model_management::domain::PerformanceTier::Fast => Self::Fast,
            crate::features::model_management::domain::PerformanceTier::Balanced => Self::Balanced,
            crate::features::model_management::domain::PerformanceTier::Accurate => Self::Accurate,
        }
    }
}

/// Compatibility level DTO for frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum CompatibilityLevelDto {
    /// Model will NOT run
    Incompatible,
    /// Model may run poorly
    Poor,
    /// Model will run acceptably
    Good,
    /// Model will run well
    Excellent,
}

impl From<crate::features::model_management::domain::CompatibilityLevel> for CompatibilityLevelDto {
    fn from(domain: crate::features::model_management::domain::CompatibilityLevel) -> Self {
        match domain {
            crate::features::model_management::domain::CompatibilityLevel::Incompatible => {
                Self::Incompatible
            }
            crate::features::model_management::domain::CompatibilityLevel::Poor => Self::Poor,
            crate::features::model_management::domain::CompatibilityLevel::Good => Self::Good,
            crate::features::model_management::domain::CompatibilityLevel::Excellent => {
                Self::Excellent
            }
        }
    }
}

/// Model source DTO for frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum ModelSourceDto {
    /// Curated internal catalog
    Curated,
    /// External API (Hugging Face)
    External,
}

impl From<crate::domain::model_catalog::ModelSource> for ModelSourceDto {
    fn from(domain: crate::domain::model_catalog::ModelSource) -> Self {
        match domain {
            crate::domain::model_catalog::ModelSource::Curated => Self::Curated,
            crate::domain::model_catalog::ModelSource::External => Self::External,
        }
    }
}

// ============================================================================
// Struct DTOs
// ============================================================================

/// Model file metadata DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelFileMetadataDto {
    /// Filename
    pub filename: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Optional checksum (SHA256)
    pub checksum: Option<String>,
    /// Download URL
    pub url: String,
    /// Whether downloaded
    pub downloaded: bool,
}

impl From<crate::domain::ModelFileMetadata> for ModelFileMetadataDto {
    fn from(domain: crate::domain::ModelFileMetadata) -> Self {
        Self {
            filename: domain.filename,
            size_bytes: domain.size_bytes,
            checksum: domain.checksum,
            url: domain.url,
            downloaded: domain.downloaded,
        }
    }
}

/// Model metadata DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelMetadataDto {
    /// Unique model identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Model category
    pub category: ModelCategoryDto,
    /// Description
    pub description: String,
    /// Size in GB
    pub size_gb: f64,
    /// Minimum RAM in GB
    pub minimum_ram_gb: f64,
    /// Recommended RAM in GB
    pub recommended_ram_gb: f64,
    /// Context length in tokens
    pub context_length: u32,
    /// Performance tier
    pub performance_tier: PerformanceTierDto,
    /// Supported quantizations
    pub supported_quantizations: Vec<String>,
    /// Capabilities
    pub capabilities: Vec<String>,
    /// Download URL (deprecated)
    pub download_url: Option<String>,
    /// License
    pub license: String,
    /// Requires auth to download
    pub requires_auth: bool,
    /// HuggingFace model ID
    pub model_id: Option<String>,
    /// Default GGUF filename
    pub default_filename: Option<String>,
    /// Files for multi-file models
    pub files: Vec<ModelFileMetadataDto>,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Output dimension for embedding models (e.g., 384, 768, 1024).
    /// Null for non-embedding models.
    pub embedding_dimensions: Option<usize>,
    /// Compatibility verdict for embedding models. Null for non-embedding
    /// models. Frontend uses this to disable + badge incompatible rows so
    /// users don't waste a 1GB+ download on a model that won't load.
    pub embedding_compatibility: Option<EmbeddingCompatibilityDto>,
}

/// Frontend mirror of `EmbeddingCompatibility`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EmbeddingCompatibilityDto {
    /// Model is loadable today.
    Compatible { architecture: String },
    /// Recognized architecture but not yet implemented in the local runtime.
    Incompatible {
        architecture: String,
        reason: String,
    },
    /// We couldn't tell from tags. Treated as not-yet-supported.
    Unknown,
}

impl From<crate::features::embedding::compatibility::EmbeddingCompatibility>
    for EmbeddingCompatibilityDto
{
    fn from(domain: crate::features::embedding::compatibility::EmbeddingCompatibility) -> Self {
        use crate::features::embedding::compatibility::EmbeddingCompatibility as E;
        match domain {
            E::Compatible { architecture } => Self::Compatible { architecture },
            E::Incompatible {
                architecture,
                reason,
            } => Self::Incompatible {
                architecture,
                reason,
            },
            E::Unknown => Self::Unknown,
        }
    }
}

impl From<crate::features::model_management::domain::ModelMetadata> for ModelMetadataDto {
    fn from(domain: crate::features::model_management::domain::ModelMetadata) -> Self {
        Self {
            id: domain.id,
            name: domain.name,
            category: domain.category.into(),
            description: domain.description,
            size_gb: domain.size_gb,
            minimum_ram_gb: domain.minimum_ram_gb,
            recommended_ram_gb: domain.recommended_ram_gb,
            context_length: domain.context_length,
            performance_tier: domain.performance_tier.into(),
            supported_quantizations: domain.supported_quantizations,
            capabilities: domain.capabilities,
            download_url: domain.download_url,
            license: domain.license,
            requires_auth: domain.requires_auth,
            model_id: domain.model_id,
            default_filename: domain.default_filename,
            files: domain.files.into_iter().map(Into::into).collect(),
            total_size_bytes: domain.total_size_bytes,
            embedding_dimensions: domain.embedding_dimensions,
            embedding_compatibility: domain.embedding_compatibility.map(Into::into),
        }
    }
}

/// Compatibility score DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CompatibilityScoreDto {
    /// Overall compatibility level
    pub compatibility_level: CompatibilityLevelDto,
    /// Overall score (0-100)
    pub overall_score: f64,
    /// RAM score (0-100)
    pub ram_score: f64,
    /// GPU score (0-100)
    pub gpu_score: f64,
    /// Disk score (0-100)
    pub disk_score: f64,
    /// Estimated tokens/second (LLMs only)
    pub estimated_tokens_per_second: Option<f64>,
    /// Estimated loading time in seconds
    pub estimated_loading_time_seconds: f64,
    /// Recommendations
    pub recommendations: Vec<String>,
    /// Blockers
    pub blockers: Vec<String>,
}

impl From<crate::features::model_management::domain::CompatibilityScore> for CompatibilityScoreDto {
    fn from(domain: crate::features::model_management::domain::CompatibilityScore) -> Self {
        Self {
            compatibility_level: domain.compatibility_level.into(),
            overall_score: domain.overall_score,
            ram_score: domain.ram_score,
            gpu_score: domain.gpu_score,
            disk_score: domain.disk_score,
            estimated_tokens_per_second: domain.estimated_tokens_per_second,
            estimated_loading_time_seconds: domain.estimated_loading_time_seconds,
            recommendations: domain.recommendations,
            blockers: domain.blockers,
        }
    }
}

/// Model recommendation DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelRecommendationDto {
    /// Model metadata
    pub model: ModelMetadataDto,
    /// Compatibility analysis
    pub compatibility: CompatibilityScoreDto,
    /// Ranking score (0-100)
    pub ranking_score: f64,
    /// Popularity metric from upstream provider (downloads)
    #[serde(default)]
    pub popularity_downloads: Option<u64>,
    /// Popularity metric from upstream provider (likes)
    #[serde(default)]
    pub popularity_likes: Option<u64>,
}

impl From<crate::features::model_management::domain::ModelRecommendation>
    for ModelRecommendationDto
{
    fn from(domain: crate::features::model_management::domain::ModelRecommendation) -> Self {
        Self {
            model: domain.model.into(),
            compatibility: domain.compatibility.into(),
            ranking_score: domain.ranking_score,
            popularity_downloads: None,
            popularity_likes: None,
        }
    }
}

/// Model search result DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelSearchResultDto {
    /// Model metadata
    pub model: ModelMetadataDto,
    /// Relevance score (0-100)
    pub relevance_score: f64,
    /// Model source
    pub source: ModelSourceDto,
    /// Popularity metric from upstream provider (downloads)
    #[serde(default)]
    pub popularity_downloads: Option<u64>,
    /// Popularity metric from upstream provider (likes)
    #[serde(default)]
    pub popularity_likes: Option<u64>,
}

impl From<crate::domain::model_catalog::ModelSearchResult> for ModelSearchResultDto {
    fn from(domain: crate::domain::model_catalog::ModelSearchResult) -> Self {
        Self {
            model: domain.model.into(),
            relevance_score: domain.relevance_score,
            source: domain.source.into(),
            popularity_downloads: None,
            popularity_likes: None,
        }
    }
}
