use super::companions::Companions;
use super::machine::MachineProfile;
use super::shape::ModelShape;
use super::verdict::FitVerdict;
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
    pub fn from_curated(_model: &CuratedChatModel) -> Candidate {
        todo!("stage 2")
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
        _profile: &MachineProfile,
        _catalog: &[Candidate],
    ) -> Vec<Recommendation> {
        todo!("stage 2")
    }
}
