//! Wires the model-fit authority to this machine: the recommended chat models and the fit of any
//! catalog row.

use crate::interfaces::dto::{
    EvidenceDto, FitVerdictDto, RecommendationDto, RecommendationLabelDto,
};
use async_trait::async_trait;
use lattice_core::application::contracts::settings::LocalModelConfigDto;
use lattice_core::domain::models::curated::curated_chat_models;
use lattice_core::domain::models::fit::{
    choose_placement, fit, Candidate, Companions, Evidence, Label, MachineProfile, ModelShape,
    Recommender, ServerBuffers, SpeedClass,
};
use lattice_core::domain::models::selection::{ModelCategory, ModelMetadata};
use std::collections::BTreeMap;

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
/// catalog and the default companions, each model judged at the buffers its own saved launch
/// settings in `configs` give it (the chat default when it has none), mapped to the wire types.
pub async fn recommend_chat_models(
    probe: &dyn MachineProfileProbe,
    _configs: &BTreeMap<String, LocalModelConfigDto>,
) -> Vec<RecommendationDto> {
    let profile = probe.profile().await;
    let catalog: Vec<Candidate> = curated_chat_models()
        .iter()
        .map(Candidate::from_curated)
        .collect();
    Recommender::new(Companions::default_pair())
        .recommend(&profile, &catalog)
        .into_iter()
        .filter_map(|recommendation| {
            let model = curated_chat_models()
                .iter()
                .find(|model| model.id == recommendation.id)?;
            Some(RecommendationDto {
                model_id: recommendation.id,
                name: recommendation.name,
                description: model.description.to_owned(),
                use_for: model.use_for.to_owned(),
                download_bytes: model.size_bytes,
                verdict: FitVerdictDto::from(&recommendation.verdict),
                labels: recommendation.labels.into_iter().map(label).collect(),
                evidence: evidence(recommendation.evidence),
                speed: SpeedClass::from_active_parameters(model.active_parameters_b).into(),
            })
        })
        .collect()
}

fn label(label: Label) -> RecommendationLabelDto {
    match label {
        Label::Best => RecommendationLabelDto::Best,
        Label::Fastest => RecommendationLabelDto::Fastest,
        Label::SmallestDownload => RecommendationLabelDto::SmallestDownload,
    }
}

fn evidence(evidence: Evidence) -> EvidenceDto {
    match evidence {
        Evidence::Measured { suites } => EvidenceDto::Measured {
            suites: suites.iter().map(|suite| suite.name.to_owned()).collect(),
        },
        Evidence::Untested => EvidenceDto::Untested,
    }
}

/// The verdict for one catalog row. A curated row is judged from its own pinned facts; any other
/// chat row from its GGUF header; `None` while that header is unread, or when the row is not a chat
/// model. The row's size estimates never feed it.
pub async fn row_fit(
    model: &ModelMetadata,
    profile: &MachineProfile,
    headers: &dyn GgufHeaderSource,
    _configs: &BTreeMap<String, LocalModelConfigDto>,
) -> Option<FitVerdictDto> {
    if model.category != ModelCategory::LLM {
        return None;
    }
    file_fit(
        model.model_id.as_deref()?,
        model.default_filename.as_deref()?,
        None,
        profile,
        headers,
        false,
    )
    .await
}

/// The verdict for the GGUF `file` of `repo` at `revision`: a curated file from its pinned facts,
/// any other from its header, which `headers` reads when it is not yet known.
pub async fn file_fit(
    repo: &str,
    file: &str,
    revision: Option<&str>,
    profile: &MachineProfile,
    headers: &dyn GgufHeaderSource,
    kv_cache_quantization: bool,
) -> Option<FitVerdictDto> {
    let placement = choose_placement(profile, &Companions::default_pair());
    let buffers = ServerBuffers::chat(kv_cache_quantization);
    let curated = curated_chat_models()
        .iter()
        .find(|model| model.repo.eq_ignore_ascii_case(repo) && model.file == file);
    let verdict = match curated {
        Some(model) => {
            let candidate = Candidate::from_curated(model);
            fit(
                candidate.gpu_weight_bytes,
                candidate.shape.as_ref(),
                buffers,
                placement.as_ref(),
            )
        }
        None => {
            let facts = headers.header(repo, file, revision).await?;
            fit(
                facts.gpu_weight_bytes,
                Some(&facts.shape),
                buffers,
                placement.as_ref(),
            )
        }
    };
    Some(FitVerdictDto::from(&verdict))
}

#[cfg(test)]
mod tests;
