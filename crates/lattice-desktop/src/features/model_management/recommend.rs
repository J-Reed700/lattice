//! Wires the model-fit authority to this machine: the recommended chat models and the fit of any
//! catalog row.

use crate::features::llm::engine::vision::projector_bytes_on_disk;
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use crate::interfaces::dto::{
    EvidenceDto, FitVerdictDto, RecommendationDto, RecommendationLabelDto,
};
use async_trait::async_trait;
use lattice_core::application::contracts::settings::LocalModelConfigDto;
use lattice_core::domain::models::curated::{curated_chat_models, CuratedChatModel};
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

/// Which catalog models have their vision projector on disk. A projector is a separate download,
/// fetched when a picture is first attached, and a server is priced with it once it is there.
pub trait DownloadedProjectors: Send + Sync {
    /// The projector bytes on disk for `model`; 0 when it has none there.
    fn bytes(&self, model: &CuratedChatModel) -> u64;
}

/// The projectors in this machine's model store, read where the runtime reads them.
pub struct ProjectorsOnDisk;

impl DownloadedProjectors for ProjectorsOnDisk {
    fn bytes(&self, model: &CuratedChatModel) -> u64 {
        ModelPaths::models_root().map_or(0, |root| {
            projector_bytes_on_disk(&root.join(model.id).join(model.file))
        })
    }
}

/// A catalog entry as the runtime would price its server: its own saved KV cache setting in
/// `configs` (the chat default when it has none), and its projector once that is downloaded.
fn candidate(
    model: &CuratedChatModel,
    configs: &BTreeMap<String, LocalModelConfigDto>,
    projectors: &dyn DownloadedProjectors,
) -> Candidate {
    Candidate {
        buffers: ServerBuffers::chat(quantizes_kv_cache(configs, model.id)),
        ..Candidate::from_curated(model)
    }
    .with_downloaded_projector(projectors.bytes(model))
}

/// The recommended chat models for this machine, best first: the core recommender over the curated
/// catalog and the default companions, each model priced as [`candidate`] says, mapped to the wire
/// types.
pub async fn recommend_chat_models(
    probe: &dyn MachineProfileProbe,
    configs: &BTreeMap<String, LocalModelConfigDto>,
    projectors: &dyn DownloadedProjectors,
) -> Vec<RecommendationDto> {
    let profile = probe.profile().await;
    let catalog: Vec<Candidate> = curated_chat_models()
        .iter()
        .map(|model| candidate(model, configs, projectors))
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
    configs: &BTreeMap<String, LocalModelConfigDto>,
    projectors: &dyn DownloadedProjectors,
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
        quantizes_kv_cache(configs, &model.id),
        projectors,
    )
    .await
}

/// Whether the saved launch settings of `model_id` quantize its KV cache: each model is launched
/// with its own, and a model with none is launched without.
fn quantizes_kv_cache(configs: &BTreeMap<String, LocalModelConfigDto>, model_id: &str) -> bool {
    configs
        .get(model_id)
        .is_some_and(|config| config.kv_cache_quantization)
}

/// The verdict for the GGUF `file` of `repo` at `revision`: a curated file from its pinned facts
/// and its projector once downloaded, any other from its header, which `headers` reads when it is
/// not yet known.
pub async fn file_fit(
    repo: &str,
    file: &str,
    revision: Option<&str>,
    profile: &MachineProfile,
    headers: &dyn GgufHeaderSource,
    kv_cache_quantization: bool,
    projectors: &dyn DownloadedProjectors,
) -> Option<FitVerdictDto> {
    let placement = choose_placement(profile, &Companions::default_pair());
    let buffers = ServerBuffers::chat(kv_cache_quantization);
    let curated = curated_chat_models()
        .iter()
        .find(|model| model.repo.eq_ignore_ascii_case(repo) && model.file == file);
    let verdict = match curated {
        Some(model) => {
            let candidate =
                Candidate::from_curated(model).with_downloaded_projector(projectors.bytes(model));
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
