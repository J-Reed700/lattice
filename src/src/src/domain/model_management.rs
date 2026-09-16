//! # Model Management Domain Types
//!
//! Rich domain models for AI model selection, compatibility scoring, and recommendations.
//!
//! ## Architecture
//!
//! Following Domain-Driven Design (DDD) patterns, this module provides:
//! - Value Objects: Immutable, validated domain primitives
//! - Aggregates: Model recommendation with compatibility analysis
//! - Domain Services: Compatibility scoring algorithm
//!
//! ## Key Concepts
//!
//! - **SystemCapabilities**: Hardware detection results (RAM, GPU, disk)
//! - **ModelMetadata**: Complete model information with requirements
//! - **CompatibilityScore**: Multi-factor scoring (RAM, GPU, disk, performance)
//! - **ModelRecommendation**: Model + compatibility + performance estimates
//!
//! ## Example
//!
//! ```rust
//! use lattice::domain::model_management::{CompatibilityScorer, SystemCapabilities, ModelMetadata};
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let capabilities = SystemCapabilities {
//!     total_ram_gb: 16.0,
//!     // available_ram_gb removed - uses total_ram_gb
//!     cpu_cores: 8,
//!     gpu_acceleration: GpuAcceleration::Metal,
//!     available_disk_gb: 100.0,
//! };
//!
//! let model = ModelMetadata {
//!     id: "phi-3-mini".into(),
//!     name: "Phi-3 Mini".into(),
//!     category: ModelCategory::LLM,
//!     size_gb: 1.8,
//!     minimum_ram_gb: 4.0,
//!     // ... other fields
//! };
//!
//! let scorer = CompatibilityScorer::new();
//! let score = scorer.score_compatibility(&model, &capabilities)?;
//!
//! assert_eq!(score.compatibility_level, CompatibilityLevel::Excellent);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Whether an embedding model can be loaded by the current local runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EmbeddingCompatibility {
    /// The model is supported by the local embedding runtime.
    Compatible { architecture: String },
    /// The architecture is recognized but not supported yet.
    Incompatible {
        architecture: String,
        reason: String,
    },
    /// The architecture could not be determined.
    Unknown,
}

impl EmbeddingCompatibility {
    /// Return whether the model is loadable by the local embedding runtime.
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible { .. })
    }
}

/// On-disk model storage format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    /// Single-file GGUF model.
    #[default]
    Gguf,
    /// Hugging Face safetensors directory.
    Safetensors,
}

/// Category of AI model by primary purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelCategory {
    /// Large Language Model for text generation and chat
    LLM,
    /// Embedding model for semantic search
    Embedding,
    /// Optical Character Recognition model
    OCR,
    /// On-device speech-to-text model (whisper family)
    Transcription,
}

impl fmt::Display for ModelCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelCategory::LLM => write!(f, "LLM"),
            ModelCategory::Embedding => write!(f, "Embedding"),
            ModelCategory::OCR => write!(f, "OCR"),
            ModelCategory::Transcription => write!(f, "Transcription"),
        }
    }
}

/// Performance tier characterizing model behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceTier {
    /// Optimized for speed (< 2 seconds/response)
    Fast,
    /// Balanced speed and quality (2-5 seconds/response)
    Balanced,
    /// Optimized for accuracy (> 5 seconds/response)
    Accurate,
}

impl fmt::Display for PerformanceTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PerformanceTier::Fast => write!(f, "Fast"),
            PerformanceTier::Balanced => write!(f, "Balanced"),
            PerformanceTier::Accurate => write!(f, "Accurate"),
        }
    }
}

/// GPU hardware type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuType {
    /// Apple Silicon GPU (M1, M2, M3)
    AppleSilicon,
    /// NVIDIA GPU (GeForce, RTX)
    Nvidia,
    /// AMD GPU (Radeon)
    AMD,
    /// Intel integrated GPU
    Intel,
    /// No discrete GPU
    None,
}

impl fmt::Display for GpuType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuType::AppleSilicon => write!(f, "Apple Silicon"),
            GpuType::Nvidia => write!(f, "NVIDIA"),
            GpuType::AMD => write!(f, "AMD"),
            GpuType::Intel => write!(f, "Intel"),
            GpuType::None => write!(f, "None"),
        }
    }
}

/// GPU acceleration framework available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuAcceleration {
    /// Apple Metal API
    Metal,
    /// NVIDIA CUDA
    CUDA,
    /// AMD ROCm
    ROCm,
    /// Vulkan API (cross-platform)
    Vulkan,
    /// No GPU acceleration (CPU only)
    None,
}

impl fmt::Display for GpuAcceleration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuAcceleration::Metal => write!(f, "Metal"),
            GpuAcceleration::CUDA => write!(f, "CUDA"),
            GpuAcceleration::ROCm => write!(f, "ROCm"),
            GpuAcceleration::Vulkan => write!(f, "Vulkan"),
            GpuAcceleration::None => write!(f, "None"),
        }
    }
}

/// CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuArchitecture {
    /// ARM64 (Apple Silicon, ARM servers)
    ARM64,
    /// x86_64 (Intel, AMD)
    X86_64,
}

impl fmt::Display for CpuArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpuArchitecture::ARM64 => write!(f, "ARM64"),
            CpuArchitecture::X86_64 => write!(f, "x86_64"),
        }
    }
}

/// Compatibility level between model and system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompatibilityLevel {
    /// Model will NOT run (insufficient resources)
    Incompatible,
    /// Model may run poorly (< 60% compatibility)
    Poor,
    /// Model will run acceptably (60-85% compatibility)
    Good,
    /// Model will run well (> 85% compatibility)
    Excellent,
}

impl fmt::Display for CompatibilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompatibilityLevel::Incompatible => write!(f, "Incompatible"),
            CompatibilityLevel::Poor => write!(f, "Poor"),
            CompatibilityLevel::Good => write!(f, "Good"),
            CompatibilityLevel::Excellent => write!(f, "Excellent"),
        }
    }
}

/// System hardware capabilities detected from the user's machine.
///
/// This value object represents the result of hardware detection,
/// used for compatibility scoring and model recommendations.
///
/// # Invariants
/// - RAM values must be positive
/// - Disk space must be positive
/// - CPU cores must be positive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCapabilities {
    /// Total system RAM in gigabytes
    pub total_ram_gb: f64,
    // Removed: available_ram_gb - domain uses total RAM only
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// CPU architecture (ARM64, x86_64)
    pub cpu_architecture: CpuArchitecture,
    /// GPU type if present
    pub gpu_type: GpuType,
    /// GPU acceleration framework available
    pub gpu_acceleration: GpuAcceleration,
    /// VRAM in gigabytes (if detectable)
    pub vram_gb: Option<f64>,
    /// Available disk space in gigabytes
    pub available_disk_gb: f64,
}

impl SystemCapabilities {
    /// Check if system has sufficient RAM for a model.
    pub fn has_sufficient_ram(&self, required_gb: f64) -> bool {
        self.total_ram_gb >= required_gb
    }

    /// Check if system has sufficient disk space for a model.
    pub fn has_sufficient_disk(&self, required_gb: f64) -> bool {
        self.available_disk_gb >= required_gb
    }

    /// Check if system has GPU acceleration.
    pub fn has_gpu_acceleration(&self) -> bool {
        !matches!(self.gpu_acceleration, GpuAcceleration::None)
    }

    /// Estimate effective RAM (includes unified memory on Apple Silicon).
    pub fn effective_ram_gb(&self) -> f64 {
        match self.gpu_type {
            GpuType::AppleSilicon => {
                // Apple Silicon has unified memory (RAM = VRAM)
                self.total_ram_gb
            }
            _ => {
                // Discrete GPU: add VRAM if available
                self.total_ram_gb + self.vram_gb.unwrap_or(0.0)
            }
        }
    }
}

/// Complete metadata for an AI model.
///
/// This value object contains all information needed for:
/// - Compatibility scoring
/// - Download management
/// - Performance estimation
/// - User presentation
///
/// # Examples
///
/// ```rust
/// use lattice::domain::model_management::{ModelMetadata, ModelCategory, PerformanceTier};
///
/// let phi3 = ModelMetadata {
///     id: "phi-3-mini-4k-instruct-q4".into(),
///     name: "Phi-3 Mini (4K context)".into(),
///     category: ModelCategory::LLM,
///     description: "Microsoft's compact 3.8B parameter model".into(),
///     size_gb: 1.8,
///     minimum_ram_gb: 4.0,
///     recommended_ram_gb: 8.0,
///     context_length: 4096,
///     performance_tier: PerformanceTier::Fast,
///     supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into()],
///     capabilities: vec!["chat".into(), "code".into()],
///     download_url: Some("https://huggingface.co/...".into()),
///     license: "MIT".into(),
///     requires_auth: false,
///     model_id: Some("microsoft/Phi-3-mini-4k-instruct-gguf".into()),
///     default_filename: Some("Phi-3-mini-4k-instruct-q4.gguf".into()),
///     files: vec![],
///     total_size_bytes: 0,
///     embedding_dimensions: None,
///     embedding_compatibility: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Unique model identifier (e.g., "phi-3-mini-4k-instruct-q4")
    pub id: String,
    /// Human-readable name (e.g., "Phi-3 Mini")
    pub name: String,
    /// Model category (LLM, Embedding, OCR)
    pub category: ModelCategory,
    /// Detailed description of model capabilities
    pub description: String,
    /// Model size in gigabytes (on disk)
    pub size_gb: f64,
    /// Minimum RAM required to load model
    pub minimum_ram_gb: f64,
    /// Recommended RAM for good performance
    pub recommended_ram_gb: f64,
    /// Maximum context length in tokens
    pub context_length: u32,
    /// Performance characteristic
    pub performance_tier: PerformanceTier,
    /// Available quantization formats (e.g., Q4_K_M, Q5_K_M, F16)
    pub supported_quantizations: Vec<String>,
    /// Model capabilities (e.g., "chat", "code", "reasoning")
    pub capabilities: Vec<String>,
    /// Direct download URL for the model's repository or file.
    ///
    /// The shipped catalog sets this for every entry; `build_download_url()`
    /// prefers `model_id` + `default_filename` when both are present and
    /// resolves to this URL otherwise.
    pub download_url: Option<String>,
    /// License (e.g., "MIT", "Apache-2.0")
    pub license: String,
    /// Whether this model requires authentication to download
    #[serde(default)]
    pub requires_auth: bool,
    /// HuggingFace model ID (e.g., "microsoft/Phi-3-mini-4k-instruct-gguf")
    /// Used to construct download URLs dynamically
    #[serde(default)]
    pub model_id: Option<String>,
    /// Default GGUF filename for this model (e.g., "Phi-3-mini-4k-instruct-q4.gguf")
    /// Used with model_id to build download URLs
    #[serde(default)]
    pub default_filename: Option<String>,
    /// Files required for this model (for multi-file models like BGE-M3 ONNX)
    /// For single-file models, this will contain one ModelFile
    /// If empty, the model is fetched as a single file from `download_url`.
    #[serde(default)]
    pub files: Vec<crate::domain::model_metadata::ModelFileMetadata>,
    /// Total size across all files in bytes
    /// Used for disk space calculations and progress tracking
    #[serde(default)]
    pub total_size_bytes: u64,
    /// Output dimension for embedding models (e.g., 384, 768, 1024).
    /// None for non-embedding models (LLM, OCR).
    /// Used to filter catalog models to those compatible with the app's vector index.
    #[serde(default)]
    pub embedding_dimensions: Option<usize>,
    /// For embedding models: whether the local Candle inference path can load
    /// this architecture. `None` for non-embedding models. Threaded through
    /// from `ExternalModelMetadata` so the catalog UI can badge incompatible
    /// rows.
    #[serde(default)]
    pub embedding_compatibility: Option<EmbeddingCompatibility>,
    /// On-disk storage format. Drives which loader path is used
    /// at load time and gates which loader-side pre-flight checks run
    /// (GGUF arch validation vs safetensors RAM check). Defaults to
    /// GGUF for backwards compat with existing catalog entries.
    #[serde(default)]
    pub format: ModelFormat,
}

impl ModelMetadata {
    /// Build HuggingFace download URL from model_id and default_filename.
    ///
    /// Constructs a direct download URL in the format:
    /// `https://huggingface.co/{model_id}/resolve/main/{filename}`
    ///
    /// # Security
    /// - Validates model_id format (must be "org/repo", no path traversal)
    /// - Validates filename (no path traversal or directory separators)
    /// - URL-encodes components to prevent injection attacks
    ///
    /// # Returns
    /// - Ok(url) if model_id and default_filename are valid
    /// - Ok(download_url) if `model_id` and `default_filename` are not both set
    /// - Err if validation fails
    ///
    /// # Example
    /// ```
    /// use lattice::domain::model_management::ModelMetadata;
    /// use lattice::domain::model_management::{ModelCategory, PerformanceTier};
    ///
    /// let model = ModelMetadata {
    ///     id: "phi-3-mini".into(),
    ///     name: "Phi-3 Mini".into(),
    ///     category: ModelCategory::LLM,
    ///     description: "Test".into(),
    ///     size_gb: 1.8,
    ///     minimum_ram_gb: 4.0,
    ///     recommended_ram_gb: 8.0,
    ///     context_length: 4096,
    ///     performance_tier: PerformanceTier::Fast,
    ///     supported_quantizations: vec![],
    ///     capabilities: vec![],
    ///     download_url: None,
    ///     license: "MIT".into(),
    ///     requires_auth: false,
    ///     model_id: Some("microsoft/Phi-3-mini-4k-instruct-gguf".into()),
    ///     default_filename: Some("Phi-3-mini-4k-instruct-q4.gguf".into()),
    ///     files: vec![],
    ///     total_size_bytes: 0,
    ///     embedding_dimensions: None,
    ///     embedding_compatibility: None,
    /// };
    ///
    /// assert_eq!(
    ///     model.build_download_url().unwrap(),
    ///     "https://huggingface.co/microsoft%2FPhi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf"
    /// );
    /// ```
    pub fn build_download_url(&self) -> Result<String, String> {
        if let (Some(model_id), Some(filename)) = (&self.model_id, &self.default_filename) {
            if !model_id.contains('/') || model_id.contains("..") {
                return Err(format!("Invalid model_id format: {}", model_id));
            }

            if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                return Err(format!(
                    "Invalid filename contains path separators: {}",
                    filename
                ));
            }

            // URL-encode both model_id and filename to handle special characters
            // Need to encode each path segment separately to preserve the /
            let parts: Vec<&str> = model_id.split('/').collect();
            let encoded_org = parts
                .first()
                .map(|s| urlencoding::encode(s))
                .ok_or_else(|| "Invalid model ID format: missing organization".to_string())?;
            let encoded_repo = parts
                .get(1)
                .map(|s| urlencoding::encode(s))
                .ok_or_else(|| "Invalid model ID format: missing repository".to_string())?;

            Ok(format!(
                "https://huggingface.co/{}/{}/resolve/main/{}",
                encoded_org,
                encoded_repo,
                urlencoding::encode(filename)
            ))
        } else {
            // Multi-file and repo-only entries carry no `default_filename`;
            // fall back to the catalog's direct URL.
            self.download_url
                .clone()
                .ok_or_else(|| "No download URL available for this model".to_string())
        }
    }

    /// Check if model fits in available RAM.
    pub fn fits_in_ram(&self, total_ram_gb: f64) -> bool {
        total_ram_gb >= self.minimum_ram_gb
    }

    /// Check if model fits on disk.
    pub fn fits_on_disk(&self, available_disk_gb: f64) -> bool {
        available_disk_gb >= self.size_gb
    }

    /// Estimate if model will perform well with given RAM.
    pub fn has_recommended_ram(&self, total_ram_gb: f64) -> bool {
        total_ram_gb >= self.recommended_ram_gb
    }
}

/// Detailed compatibility analysis between a model and system.
///
/// This value object represents the output of the compatibility scoring
/// algorithm, providing:
/// - Overall compatibility level
/// - Detailed factor scores (RAM, GPU, disk)
/// - Performance estimates
/// - Recommendations
///
/// # Scoring Algorithm
///
/// 1. **Critical Checks** (instant fail):
///    - RAM: available >= minimum (else Incompatible)
///    - Disk: available >= size (else Incompatible)
///
/// 2. **Factor Scores** (0-100):
///    - RAM Score: % of recommended RAM available
///    - GPU Score: 100 if GPU available, 50 otherwise
///    - Disk Score: 100 if ample space, 75 if tight
///
/// 3. **Overall Score**:
///    - RAM: 50% weight
///    - GPU: 30% weight
///    - Disk: 20% weight
///
/// 4. **Compatibility Level**:
///    - > 85: Excellent
///    - 60-85: Good
///    - < 60: Poor
///    - Fails critical: Incompatible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityScore {
    /// Overall compatibility level
    pub compatibility_level: CompatibilityLevel,
    /// Overall compatibility score (0-100)
    pub overall_score: f64,
    /// RAM compatibility score (0-100)
    pub ram_score: f64,
    /// GPU compatibility score (0-100)
    pub gpu_score: f64,
    /// Disk space compatibility score (0-100)
    pub disk_score: f64,
    /// Estimated tokens per second (for LLMs)
    pub estimated_tokens_per_second: Option<f64>,
    /// Estimated model loading time in seconds
    pub estimated_loading_time_seconds: f64,
    /// Human-readable recommendations
    pub recommendations: Vec<String>,
    /// Blockers preventing model from running
    pub blockers: Vec<String>,
}

impl CompatibilityScore {
    /// Check if model is compatible (can run).
    pub fn is_compatible(&self) -> bool {
        !matches!(self.compatibility_level, CompatibilityLevel::Incompatible)
    }

    /// Check if model is recommended (Good or Excellent).
    pub fn is_recommended(&self) -> bool {
        matches!(
            self.compatibility_level,
            CompatibilityLevel::Good | CompatibilityLevel::Excellent
        )
    }
}

/// Model recommendation with compatibility analysis.
///
/// This aggregate combines:
/// - Model metadata
/// - Compatibility score
/// - System-specific performance estimates
///
/// Used for presenting ranked model recommendations to users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    /// Model metadata
    pub model: ModelMetadata,
    /// Compatibility analysis
    pub compatibility: CompatibilityScore,
    /// Ranking score for sorting (0-100)
    pub ranking_score: f64,
}

impl ModelRecommendation {
    /// Create a new recommendation.
    pub fn new(model: ModelMetadata, compatibility: CompatibilityScore) -> Self {
        // Ranking score = compatibility score, adjusted for performance tier
        let tier_bonus = match model.performance_tier {
            PerformanceTier::Fast => 5.0,
            PerformanceTier::Balanced => 0.0,
            PerformanceTier::Accurate => -5.0,
        };

        let ranking_score = (compatibility.overall_score + tier_bonus).clamp(0.0, 100.0);

        Self {
            model,
            compatibility,
            ranking_score,
        }
    }

    /// Sort recommendations by ranking score (descending).
    pub fn sort_by_ranking(recommendations: &mut [ModelRecommendation]) {
        recommendations.sort_by(|a, b| {
            b.ranking_score
                .partial_cmp(&a.ranking_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Domain service for scoring model compatibility with system capabilities.
///
/// This service implements the compatibility scoring algorithm as pure business
/// logic with no infrastructure dependencies.
///
/// # Algorithm
///
/// See [`CompatibilityScore`] documentation for detailed algorithm description.
///
/// # Example
///
/// ```rust
/// use lattice::domain::model_management::{CompatibilityScorer, SystemCapabilities, ModelMetadata, ModelCategory, PerformanceTier, GpuAcceleration, GpuType, CpuArchitecture};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let scorer = CompatibilityScorer::new();
///
/// let capabilities = SystemCapabilities {
///     total_ram_gb: 16.0,
///     // available_ram_gb removed
///     cpu_cores: 8,
///     cpu_architecture: CpuArchitecture::ARM64,
///     gpu_type: GpuType::AppleSilicon,
///     gpu_acceleration: GpuAcceleration::Metal,
///     vram_gb: None,
///     available_disk_gb: 100.0,
/// };
///
/// let model = ModelMetadata {
///     id: "phi-3-mini".into(),
///     name: "Phi-3 Mini".into(),
///     category: ModelCategory::LLM,
///     description: "Compact model".into(),
///     size_gb: 1.8,
///     minimum_ram_gb: 4.0,
///     recommended_ram_gb: 8.0,
///     context_length: 4096,
///     performance_tier: PerformanceTier::Fast,
///     supported_quantizations: vec!["Q4_K_M".into()],
///     capabilities: vec!["chat".into()],
///     download_url: None,
///     license: "MIT".into(),
///     requires_auth: false,
///     model_id: None,
///     default_filename: None,
///     files: vec![],
///     total_size_bytes: 0,
///     embedding_dimensions: None,
///     embedding_compatibility: None,
/// };
///
/// let score = scorer.score_compatibility(&model, &capabilities)?;
/// println!("Compatibility: {}", score.compatibility_level);
/// # Ok(())
/// # }
/// ```
pub struct CompatibilityScorer;

impl CompatibilityScorer {
    /// Create a new compatibility scorer.
    pub fn new() -> Self {
        Self
    }

    /// Score compatibility between a model and system capabilities.
    ///
    /// # Arguments
    /// - `model` - Model to score
    /// - `capabilities` - System hardware capabilities
    ///
    /// # Returns
    /// Detailed compatibility score with recommendations.
    pub fn score_compatibility(
        &self,
        model: &ModelMetadata,
        capabilities: &SystemCapabilities,
    ) -> Result<CompatibilityScore, String> {
        let mut blockers = Vec::new();
        let mut recommendations = Vec::new();

        if !capabilities.has_sufficient_ram(model.minimum_ram_gb) {
            blockers.push(format!(
                "Insufficient RAM: need {} GB, have {} GB total",
                model.minimum_ram_gb, capabilities.total_ram_gb
            ));
        }

        if !capabilities.has_sufficient_disk(model.size_gb) {
            blockers.push(format!(
                "Insufficient disk space: need {} GB, have {} GB available",
                model.size_gb, capabilities.available_disk_gb
            ));
        }

        // If critical checks fail, return Incompatible
        if !blockers.is_empty() {
            return Ok(CompatibilityScore {
                compatibility_level: CompatibilityLevel::Incompatible,
                overall_score: 0.0,
                ram_score: 0.0,
                gpu_score: 0.0,
                disk_score: 0.0,
                estimated_tokens_per_second: None,
                estimated_loading_time_seconds: 0.0,
                recommendations: vec![],
                blockers,
            });
        }

        // RAM Score: percentage of recommended RAM available
        let ram_ratio = capabilities.total_ram_gb / model.recommended_ram_gb;
        let ram_score = (ram_ratio * 100.0).clamp(0.0, 100.0);

        if ram_score < 100.0 {
            recommendations.push(format!(
                "Model will run, but {} GB RAM recommended for best performance",
                model.recommended_ram_gb
            ));
        }

        // GPU Score: 100 if GPU available, 50 if CPU-only
        let gpu_score = if capabilities.has_gpu_acceleration() {
            100.0
        } else {
            50.0
        };

        if !capabilities.has_gpu_acceleration() {
            recommendations
                .push("Running on CPU only. GPU would improve performance significantly.".into());
        }

        // Disk Score: 100 if ample space (10x model size), 75 if tight
        let disk_ratio = capabilities.available_disk_gb / model.size_gb;
        let disk_score = if disk_ratio >= 10.0 {
            100.0
        } else if disk_ratio >= 2.0 {
            75.0
        } else {
            50.0
        };

        if disk_score < 100.0 {
            recommendations.push(format!(
                "Low disk space. Recommend {} GB free for optimal performance",
                model.size_gb * 5.0
            ));
        }

        let overall_score = (ram_score * 0.5) + (gpu_score * 0.3) + (disk_score * 0.2);

        let compatibility_level = if overall_score >= 85.0 {
            CompatibilityLevel::Excellent
        } else if overall_score >= 60.0 {
            CompatibilityLevel::Good
        } else {
            CompatibilityLevel::Poor
        };

        // Estimate tokens/second for LLMs
        let estimated_tokens_per_second = if matches!(model.category, ModelCategory::LLM) {
            let base_tokens_per_sec = match model.performance_tier {
                PerformanceTier::Fast => 30.0,
                PerformanceTier::Balanced => 20.0,
                PerformanceTier::Accurate => 10.0,
            };

            // Adjust for GPU
            let gpu_multiplier = if capabilities.has_gpu_acceleration() {
                match capabilities.gpu_type {
                    GpuType::AppleSilicon => 2.5,
                    GpuType::Nvidia => 3.0,
                    GpuType::AMD => 2.0,
                    _ => 1.0,
                }
            } else {
                1.0
            };

            // Adjust for RAM
            let ram_multiplier = if capabilities.total_ram_gb >= model.recommended_ram_gb {
                1.0
            } else {
                0.7
            };

            Some(base_tokens_per_sec * gpu_multiplier * ram_multiplier)
        } else {
            None
        };

        // Estimate loading time (based on model size and disk speed)
        let estimated_loading_time_seconds = model.size_gb * 2.0; // ~2 seconds per GB

        Ok(CompatibilityScore {
            compatibility_level,
            overall_score,
            ram_score,
            gpu_score,
            disk_score,
            estimated_tokens_per_second,
            estimated_loading_time_seconds,
            recommendations,
            blockers,
        })
    }
}

impl Default for CompatibilityScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_capabilities() -> SystemCapabilities {
        SystemCapabilities {
            total_ram_gb: 16.0,
            // Removed: available_ram_gb
            cpu_cores: 8,
            cpu_architecture: CpuArchitecture::ARM64,
            gpu_type: GpuType::AppleSilicon,
            gpu_acceleration: GpuAcceleration::Metal,
            vram_gb: None,
            available_disk_gb: 100.0,
        }
    }

    fn create_test_model() -> ModelMetadata {
        ModelMetadata {
            id: "phi-3-mini".into(),
            name: "Phi-3 Mini".into(),
            category: ModelCategory::LLM,
            description: "Compact model".into(),
            size_gb: 1.8,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 4096,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into()],
            capabilities: vec!["chat".into()],
            download_url: None,
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("microsoft/Phi-3-mini-4k-instruct-gguf".into()),
            default_filename: Some("Phi-3-mini-4k-instruct-q4.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        }
    }

    #[test]
    fn test_excellent_compatibility() {
        let scorer = CompatibilityScorer::new();
        let capabilities = create_test_capabilities();
        let model = create_test_model();

        let score = scorer.score_compatibility(&model, &capabilities).unwrap();

        assert_eq!(score.compatibility_level, CompatibilityLevel::Excellent);
        assert!(score.overall_score >= 85.0);
        assert!(score.is_compatible());
        assert!(score.is_recommended());
        assert!(score.blockers.is_empty());
    }

    #[test]
    fn test_insufficient_ram() {
        let scorer = CompatibilityScorer::new();
        let mut capabilities = create_test_capabilities();
        capabilities.total_ram_gb = 2.0; // Below minimum

        let model = create_test_model();

        let score = scorer.score_compatibility(&model, &capabilities).unwrap();

        assert_eq!(score.compatibility_level, CompatibilityLevel::Incompatible);
        assert!(!score.is_compatible());
        assert!(!score.blockers.is_empty());
    }

    #[test]
    fn test_insufficient_disk() {
        let scorer = CompatibilityScorer::new();
        let mut capabilities = create_test_capabilities();
        capabilities.available_disk_gb = 1.0; // Below model size

        let model = create_test_model();

        let score = scorer.score_compatibility(&model, &capabilities).unwrap();

        assert_eq!(score.compatibility_level, CompatibilityLevel::Incompatible);
        assert!(!score.is_compatible());
        assert!(!score.blockers.is_empty());
    }

    #[test]
    fn test_cpu_only_reduces_score() {
        let scorer = CompatibilityScorer::new();
        let mut capabilities = create_test_capabilities();
        capabilities.gpu_acceleration = GpuAcceleration::None;
        capabilities.gpu_type = GpuType::None;

        let model = create_test_model();

        let cpu_score = scorer.score_compatibility(&model, &capabilities).unwrap();

        let gpu_capabilities = create_test_capabilities();
        let gpu_score = scorer
            .score_compatibility(&model, &gpu_capabilities)
            .unwrap();

        assert!(cpu_score.is_compatible());
        assert_eq!(cpu_score.gpu_score, 50.0);
        // CPU-only should have lower score than with GPU
        assert!(cpu_score.overall_score < gpu_score.overall_score);
    }

    #[test]
    fn test_model_recommendation_ranking() {
        let scorer = CompatibilityScorer::new();
        let capabilities = create_test_capabilities();

        let mut model1 = create_test_model();
        model1.id = "model1".into();
        model1.performance_tier = PerformanceTier::Fast;

        let mut model2 = create_test_model();
        model2.id = "model2".into();
        model2.performance_tier = PerformanceTier::Accurate;

        let score1 = scorer.score_compatibility(&model1, &capabilities).unwrap();
        let score2 = scorer.score_compatibility(&model2, &capabilities).unwrap();

        let mut recommendations = vec![
            ModelRecommendation::new(model1, score1),
            ModelRecommendation::new(model2, score2),
        ];

        ModelRecommendation::sort_by_ranking(&mut recommendations);

        // Fast tier gets bonus, should rank higher
        assert_eq!(recommendations[0].model.id, "model1");
    }

    #[test]
    fn test_system_capabilities_helpers() {
        let capabilities = create_test_capabilities();

        assert!(capabilities.has_sufficient_ram(4.0));
        assert!(capabilities.has_sufficient_ram(16.0)); // 16 GB total IS sufficient for 16 GB required
        assert!(!capabilities.has_sufficient_ram(20.0)); // But NOT sufficient for 20 GB
        assert!(capabilities.has_sufficient_disk(50.0));
        assert!(capabilities.has_gpu_acceleration());
        assert_eq!(capabilities.effective_ram_gb(), 16.0);
    }

    #[test]
    fn test_model_metadata_helpers() {
        let model = create_test_model();

        assert!(model.fits_in_ram(8.0));
        assert!(!model.fits_in_ram(2.0));
        assert!(model.fits_on_disk(50.0));
        assert!(model.has_recommended_ram(8.0));
        assert!(!model.has_recommended_ram(6.0));
    }

    #[test]
    fn test_build_download_url_security_valid() {
        let model = create_test_model();
        let url = model.build_download_url();

        assert!(url.is_ok());
        assert!(url.unwrap().contains("huggingface.co"));
    }

    #[test]
    fn test_build_download_url_prevents_path_traversal_in_model_id() {
        let mut model = create_test_model();
        model.model_id = Some("../../../etc/passwd".into());

        let result = model.build_download_url();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid model_id format"));
    }

    #[test]
    fn test_build_download_url_prevents_path_traversal_in_filename() {
        let mut model = create_test_model();
        model.default_filename = Some("../../../etc/passwd".into());

        let result = model.build_download_url();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid filename contains path separators"));
    }

    #[test]
    fn test_build_download_url_prevents_directory_separators_in_filename() {
        let mut model = create_test_model();
        model.default_filename = Some("subdir/malicious.gguf".into());

        let result = model.build_download_url();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid filename contains path separators"));
    }

    #[test]
    fn test_build_download_url_requires_slash_in_model_id() {
        let mut model = create_test_model();
        model.model_id = Some("invalid-without-slash".into());

        let result = model.build_download_url();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid model_id format"));
    }

    #[test]
    fn test_build_download_url_url_encodes_components() {
        let mut model = create_test_model();
        model.model_id = Some("org/repo with spaces".into());
        model.default_filename = Some("file with spaces.gguf".into());

        let result = model.build_download_url();

        assert!(result.is_ok());
        let url = result.unwrap();
        // URL encoding should replace spaces with %20
        assert!(url.contains("%20"));
        assert!(!url.contains(" "));
    }

    #[test]
    fn test_build_download_url_falls_back_to_download_url() {
        let mut model = create_test_model();
        model.model_id = None;
        model.default_filename = None;
        model.download_url = Some("https://example.com/model.gguf".into());

        let result = model.build_download_url();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://example.com/model.gguf");
    }

    #[test]
    fn test_build_download_url_no_url_available() {
        let mut model = create_test_model();
        model.model_id = None;
        model.default_filename = None;
        model.download_url = None;

        let result = model.build_download_url();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No download URL available"));
    }
}
