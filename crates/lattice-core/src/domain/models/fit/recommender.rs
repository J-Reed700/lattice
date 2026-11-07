use super::companions::Companions;
use super::footprint::{footprint, ServerBuffers, COMFORTABLE_WINDOW_TOKENS};
use super::machine::MachineProfile;
use super::placement::choose_placement;
use super::shape::ModelShape;
use super::verdict::{fit, DeviceKind, FitKind, FitVerdict};
use crate::domain::models::curated::CuratedChatModel;

/// One result file a model was measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasuredSuite {
    pub name: &'static str,
    pub result_file: &'static str,
}

/// What Lattice knows about a model's quality beyond the catalog's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Measured { suites: &'static [MeasuredSuite] },
    Untested,
}

/// The catalog entry Lattice has measured.
const MEASURED_CHAT_MODEL_ID: &str = "qwen3.5-9b-q4_k_m";

const MEASURED_SUITES: &[MeasuredSuite] = &[
    MeasuredSuite {
        name: "Course generation (local 9B recipe)",
        result_file: "evals/courses/recipes/course-eval-9b-local.json",
    },
    MeasuredSuite {
        name: "Memory families",
        result_file: "evals/conversation-memory/results/qwen3.5-9b-local/2026-09-26-invariant-families.md",
    },
    MeasuredSuite {
        name: "Grounded chat, shipped path",
        result_file: "evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-13-shipped-path-desktop-path-v1-default-roles.scored.jsonl.summary.json",
    },
];

/// Models below this many parameters are not offered at under four bits.
const SMALL_MODEL_PARAMETERS_B: f32 = 20.0;

/// Active parameters up to which a model is fast.
const FAST_MAX_ACTIVE_PARAMETERS_B: f32 = 5.0;

/// Active parameters up to which a model is balanced; above it a model is thorough.
const BALANCED_MAX_ACTIVE_PARAMETERS_B: f32 = 14.0;

/// How quickly a model answers, from its active parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedClass {
    Fast,
    Balanced,
    Thorough,
}

impl SpeedClass {
    /// Up to 5 billion active parameters is fast, up to 14 balanced, above that thorough.
    pub fn from_active_parameters(active_parameters_b: f32) -> Self {
        if active_parameters_b <= FAST_MAX_ACTIVE_PARAMETERS_B {
            Self::Fast
        } else if active_parameters_b <= BALANCED_MAX_ACTIVE_PARAMETERS_B {
            Self::Balanced
        } else {
            Self::Thorough
        }
    }
}

/// A model the recommender may offer.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub quantization: String,
    /// Nominal bits per weight of the quantization (3 for IQ3_S, 4 for Q4_K_M).
    pub quantization_bits: u8,
    pub parameters_b: f32,
    pub active_parameters_b: f32,
    pub download_bytes: u64,
    /// What the device holds of the file: its bytes less those that stay in system RAM.
    pub gpu_weight_bytes: u64,
    /// The header's memory facts; `None` when they are unread, which prices the model at its
    /// weights alone.
    pub shape: Option<ModelShape>,
    /// The catalog's capability order, highest first.
    pub rank: u8,
    pub evidence: Evidence,
}

impl Candidate {
    /// A catalog entry as a candidate.
    pub fn from_curated(model: &CuratedChatModel) -> Candidate {
        let pinned = model.pinned();
        let projector_bytes = model
            .vision
            .as_ref()
            .map_or(0, |vision| vision.projector_bytes);
        Candidate {
            id: model.id.into(),
            name: model.name.into(),
            quantization: model.quantization.into(),
            quantization_bits: model.quantization_bits,
            parameters_b: model.parameters_b,
            active_parameters_b: model.active_parameters_b,
            download_bytes: model.size_bytes,
            gpu_weight_bytes: model
                .size_bytes
                .saturating_sub(pinned.cpu_weight_bytes)
                .saturating_add(projector_bytes),
            shape: Some(pinned.shape),
            rank: model.rank,
            evidence: if model.id == MEASURED_CHAT_MODEL_ID {
                Evidence::Measured {
                    suites: MEASURED_SUITES,
                }
            } else {
                Evidence::Untested
            },
        }
    }

    fn is_offered(&self) -> bool {
        self.quantization_bits >= 4 || self.parameters_b >= SMALL_MODEL_PARAMETERS_B
    }

    /// The model's speed class, from its active parameters.
    pub fn speed(&self) -> SpeedClass {
        SpeedClass::from_active_parameters(self.active_parameters_b)
    }

    fn needs_at_comfortable_window(&self, buffers: ServerBuffers) -> u64 {
        footprint(
            self.gpu_weight_bytes,
            self.shape.as_ref(),
            COMFORTABLE_WINDOW_TOKENS,
            buffers,
        )
        .total()
    }
}

/// A badge on a recommendation; one model can carry several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// "Best for this computer"
    Best,
    /// "Fastest"
    Fastest,
    /// "Smallest download"
    SmallestDownload,
}

/// One entry of the recommended list.
#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub id: String,
    pub name: String,
    /// What it needs, the budget and the device it was judged against, which also says whether it
    /// runs on the processor.
    pub verdict: FitVerdict,
    pub evidence: Evidence,
    pub labels: Vec<Label>,
}

/// Ranks the candidates that fit a machine.
#[derive(Debug, Clone)]
pub struct Recommender {
    pub companions: Companions,
    /// Whether the KV cache is quantized, as the runtime launches chat servers.
    pub kv_cache_quantization: bool,
}

impl Recommender {
    pub fn new(companions: Companions, kv_cache_quantization: bool) -> Recommender {
        Recommender {
            companions,
            kv_cache_quantization,
        }
    }

    /// The models to offer on `profile`, best first.
    pub fn recommend(
        &self,
        profile: &MachineProfile,
        catalog: &[Candidate],
    ) -> Vec<Recommendation> {
        let placement = choose_placement(profile, &self.companions);
        let buffers = ServerBuffers::chat(self.kv_cache_quantization);
        let on_processor = placement.device().kind == DeviceKind::Processor;
        let judged: Vec<(&Candidate, FitVerdict)> = catalog
            .iter()
            .filter(|candidate| candidate.is_offered())
            .filter(|candidate| !on_processor || candidate.speed() != SpeedClass::Thorough)
            .map(|candidate| {
                let verdict = fit(
                    candidate.gpu_weight_bytes,
                    candidate.shape.as_ref(),
                    buffers,
                    placement.as_ref(),
                );
                (candidate, verdict)
            })
            .collect();
        let of_kind = |kind: FitKind| {
            judged
                .iter()
                .filter(|(_, verdict)| verdict.kind == kind)
                .collect::<Vec<_>>()
        };
        let mut kept = of_kind(FitKind::Fits);
        if kept.is_empty() {
            kept = of_kind(FitKind::Tight);
        }
        kept.sort_by(|(a, _), (b, _)| {
            b.rank
                .cmp(&a.rank)
                .then(
                    a.needs_at_comfortable_window(buffers)
                        .cmp(&b.needs_at_comfortable_window(buffers)),
                )
                .then_with(|| a.id.cmp(&b.id))
        });

        let fastest = kept
            .iter()
            .map(|(candidate, _)| candidate)
            .min_by(|a, b| a.active_parameters_b.total_cmp(&b.active_parameters_b))
            .map(|candidate| candidate.id.clone());
        let smallest = kept
            .iter()
            .map(|(candidate, _)| candidate)
            .min_by_key(|candidate| candidate.download_bytes)
            .map(|candidate| candidate.id.clone());

        kept.iter()
            .enumerate()
            .map(|(index, (candidate, verdict))| {
                let mut labels = Vec::new();
                if index == 0 {
                    labels.push(Label::Best);
                }
                if fastest.as_deref() == Some(candidate.id.as_str()) {
                    labels.push(Label::Fastest);
                }
                if smallest.as_deref() == Some(candidate.id.as_str()) {
                    labels.push(Label::SmallestDownload);
                }
                Recommendation {
                    id: candidate.id.clone(),
                    name: candidate.name.clone(),
                    verdict: verdict.clone(),
                    evidence: candidate.evidence,
                    labels,
                }
            })
            .collect()
    }
}
