//! Model-fit DTOs: the recommender's verdicts and recommended list as the frontend reads them.

use lattice_core::domain::models::fit::{FitVerdict, MachineProfile};
use serde::{Deserialize, Serialize};

/// How well a model fits a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FitKindDto {
    Fits,
    Tight,
    TooLarge,
}

/// What kind of device a model was judged against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKindDto {
    Dedicated,
    Integrated,
    Unified,
    Processor,
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

impl FitVerdictDto {
    /// Maps a core verdict; the device's kind is read from `profile`, and a device the profile
    /// does not list as an accelerator (the processor, or an integrated GPU alone) is `Processor`.
    pub fn from_verdict(_verdict: &FitVerdict, _profile: &MachineProfile) -> Self {
        todo!("stage 2")
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

impl SpeedClassDto {
    /// Up to 5 billion active parameters is fast, up to 14 balanced, above that thorough.
    pub fn from_active_parameters(_active_parameters_b: f32) -> Self {
        todo!("stage 2")
    }
}

/// One entry of the recommended list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationDto {
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub download_bytes: u64,
    pub verdict: FitVerdictDto,
    pub labels: Vec<RecommendationLabelDto>,
    pub evidence: EvidenceDto,
    pub speed: SpeedClassDto,
    pub reason: String,
}
