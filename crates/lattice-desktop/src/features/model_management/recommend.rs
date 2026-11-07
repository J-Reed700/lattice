//! Wires the model-fit authority to this machine: the recommended chat models and the fit of any
//! catalog row.

use crate::interfaces::dto::{FitVerdictDto, RecommendationDto};
use async_trait::async_trait;
use lattice_core::domain::models::fit::{MachineProfile, ModelShape};
use lattice_core::domain::models::selection::ModelMetadata;

/// Builds the profile of this computer. A probe that cannot reach the backend answers with a
/// processor-only profile rather than failing.
#[async_trait]
pub trait MachineProfileProbe: Send + Sync {
    async fn profile(&self) -> MachineProfile;
}

/// A probe that always answers with one profile.
#[derive(Debug, Clone)]
pub struct FixedMachineProfile(pub MachineProfile);

#[async_trait]
impl MachineProfileProbe for FixedMachineProfile {
    async fn profile(&self) -> MachineProfile {
        self.0.clone()
    }
}

/// What a parsed GGUF header gives the one memory model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufFacts {
    /// What the device holds of the file: its bytes less those that stay in system RAM.
    pub gpu_weight_bytes: u64,
    pub shape: ModelShape,
}

/// Reads a GGUF file's header (a range request, cached by repository, file and revision) with the
/// runtime's reader. `None` while it cannot be read.
#[async_trait]
pub trait GgufHeaderSource: Send + Sync {
    async fn header(&self, repo: &str, file: &str, revision: Option<&str>) -> Option<GgufFacts>;
}

/// The recommended chat models for this machine, best first: the core recommender over the curated
/// catalog and the default companions, priced with the user's KV cache setting
/// (`kv_cache_quantization`), mapped to the wire types.
pub async fn recommend_chat_models(
    _probe: &dyn MachineProfileProbe,
    _kv_cache_quantization: bool,
) -> Vec<RecommendationDto> {
    todo!("stage 2")
}

/// The verdict for one catalog row. A curated row is judged from its own footprint; any other chat
/// row from its GGUF header; `None` while that header is unread, or when the row is not a chat
/// model. The row's size estimates never feed it.
pub async fn row_fit(
    _model: &ModelMetadata,
    _profile: &MachineProfile,
    _headers: &dyn GgufHeaderSource,
    _kv_cache_quantization: bool,
) -> Option<FitVerdictDto> {
    todo!("stage 2")
}

#[cfg(test)]
mod tests;
