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

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

mod model_metadata;
mod scoring;

pub use model_metadata::ModelMetadata;
pub use scoring::{CompatibilityScore, CompatibilityScorer, ModelRecommendation};

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

#[cfg(test)]
mod tests;
