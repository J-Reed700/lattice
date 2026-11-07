use super::companions::Companions;
use super::footprint::{Footprint, COMFORTABLE_WINDOW_TOKENS};
use super::machine::MachineProfile;
use super::placement::choose_placement;
use super::verdict::{fit, FitVerdict};
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
    pub footprint: Footprint,
    /// The catalog's capability order, highest first.
    pub rank: u8,
    pub evidence: Evidence,
}

impl Candidate {
    /// A catalog entry as a candidate.
    pub fn from_curated(model: &CuratedChatModel) -> Candidate {
        Candidate {
            id: model.id.into(),
            name: model.name.into(),
            quantization: model.quantization.into(),
            quantization_bits: model.quantization_bits,
            parameters_b: model.parameters_b,
            active_parameters_b: model.active_parameters_b,
            download_bytes: model.size_bytes,
            footprint: model.footprint(),
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
    pub verdict: FitVerdict,
    pub device: String,
    /// Marked when the model runs on the processor.
    pub slow: bool,
    pub evidence: Evidence,
    pub labels: Vec<Label>,
    pub reason: String,
}

/// Ranks the candidates that fit a machine.
#[derive(Debug, Clone)]
pub struct Recommender {
    pub companions: Companions,
}

impl Recommender {
    pub fn new(companions: Companions) -> Recommender {
        Recommender { companions }
    }

    /// The models to offer on `profile`, best first.
    pub fn recommend(
        &self,
        profile: &MachineProfile,
        catalog: &[Candidate],
    ) -> Vec<Recommendation> {
        let placement = choose_placement(profile, &self.companions);
        let judged: Vec<(&Candidate, FitVerdict)> = catalog
            .iter()
            .filter(|candidate| candidate.is_offered())
            .map(|candidate| (candidate, fit(&candidate.footprint, placement.as_ref())))
            .collect();
        let mut kept: Vec<_> = judged
            .iter()
            .filter(|(_, verdict)| matches!(verdict, FitVerdict::Fits { .. }))
            .collect();
        if kept.is_empty() {
            kept = judged
                .iter()
                .filter(|(_, verdict)| matches!(verdict, FitVerdict::Tight { .. }))
                .collect();
        }
        kept.sort_by(|(a, _), (b, _)| {
            b.rank
                .cmp(&a.rank)
                .then(
                    a.footprint
                        .at(COMFORTABLE_WINDOW_TOKENS)
                        .cmp(&b.footprint.at(COMFORTABLE_WINDOW_TOKENS)),
                )
                .then_with(|| a.id.cmp(&b.id))
        });

        let fastest = kept
            .iter()
            .map(|(c, _)| c)
            .min_by(|a, b| a.active_parameters_b.total_cmp(&b.active_parameters_b))
            .map(|c| c.id.clone());
        let smallest = kept
            .iter()
            .map(|(c, _)| c)
            .min_by_key(|c| c.download_bytes)
            .map(|c| c.id.clone());

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
                    device: placement.device_name(),
                    slow: placement.is_slow(),
                    evidence: candidate.evidence,
                    labels,
                    reason: reason(verdict, placement.is_slow()),
                }
            })
            .collect()
    }
}

fn reason(verdict: &FitVerdict, slow: bool) -> String {
    let gb = |bytes: u64| bytes as f64 / 1e9;
    match verdict {
        FitVerdict::Fits {
            needs,
            total_bytes,
            device,
            ..
        } => {
            let speed = if slow {
                ", slower on the processor"
            } else {
                ""
            };
            format!(
                "Uses {:.1} of {:.1} GB on {device}{speed}",
                gb(*needs),
                gb(*total_bytes)
            )
        }
        FitVerdict::Tight {
            needs,
            total_bytes,
            device,
            ..
        } => format!(
            "Uses {:.1} of {:.1} GB on {device} with a shorter context",
            gb(*needs),
            gb(*total_bytes)
        ),
        FitVerdict::TooLarge { device, .. } => format!("Too large for {device}"),
    }
}
