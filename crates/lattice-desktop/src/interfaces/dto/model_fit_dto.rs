//! Model-fit DTOs: the recommender's verdicts and recommended list as the frontend reads them.

use lattice_core::domain::models::fit::{DeviceKind, FitKind, FitVerdict, SpeedClass};
use serde::{Deserialize, Serialize};

/// How well a model fits a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FitKindDto {
    Fits,
    Tight,
    TooLarge,
}

impl From<FitKind> for FitKindDto {
    fn from(kind: FitKind) -> Self {
        match kind {
            FitKind::Fits => Self::Fits,
            FitKind::Tight => Self::Tight,
            FitKind::TooLarge => Self::TooLarge,
        }
    }
}

/// What kind of device a model was judged against. An integrated GPU alone is the processor, so no
/// verdict is judged against one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKindDto {
    Dedicated,
    Unified,
    Processor,
}

impl From<DeviceKind> for DeviceKindDto {
    fn from(kind: DeviceKind) -> Self {
        match kind {
            DeviceKind::Dedicated => Self::Dedicated,
            DeviceKind::Unified => Self::Unified,
            DeviceKind::Processor => Self::Processor,
        }
    }
}

/// The device a verdict was judged against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FitDeviceDto {
    pub name: String,
    pub kind: DeviceKindDto,
    pub total_bytes: u64,
}

/// Whether a model fits this computer, and what it would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FitVerdictDto {
    pub kind: FitKindDto,
    pub needs_bytes: u64,
    pub budget_bytes: u64,
    pub device: FitDeviceDto,
}

impl From<&FitVerdict> for FitVerdictDto {
    fn from(verdict: &FitVerdict) -> Self {
        Self {
            kind: verdict.kind.into(),
            needs_bytes: verdict.needs,
            budget_bytes: verdict.budget,
            device: FitDeviceDto {
                name: verdict.device.name.clone(),
                kind: verdict.device.kind.into(),
                total_bytes: verdict.device.total_bytes,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationLabelDto {
    Best,
    Fastest,
    SmallestDownload,
}

/// What Lattice knows about a model's quality: the suites it was measured in, or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EvidenceDto {
    Measured { suites: Vec<String> },
    Untested,
}

/// How quickly a model answers, from its active parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SpeedClassDto {
    Fast,
    Balanced,
    Thorough,
}

impl From<SpeedClass> for SpeedClassDto {
    fn from(speed: SpeedClass) -> Self {
        match speed {
            SpeedClass::Fast => Self::Fast,
            SpeedClass::Balanced => Self::Balanced,
            SpeedClass::Thorough => Self::Thorough,
        }
    }
}

/// One entry of the recommended list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationDto {
    pub model_id: String,
    pub name: String,
    pub description: String,
    /// One plain sentence on what the model is for.
    pub use_for: String,
    pub download_bytes: u64,
    pub verdict: FitVerdictDto,
    pub labels: Vec<RecommendationLabelDto>,
    pub evidence: EvidenceDto,
    pub speed: SpeedClassDto,
}

/// A GGUF file on Hugging Face: the repository, the file in it and the revision to read, the
/// default branch when absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileRefDto {
    pub repo: String,
    pub file: String,
    pub revision: Option<String>,
}
