#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::interfaces::dto::{
    DeviceKindDto, EvidenceDto, FitDeviceDto, FitKindDto, RecommendationLabelDto, SpeedClassDto,
};
use lattice_core::domain::models::curated::{curated_chat_models, get_curated_llm_models};
use lattice_core::domain::models::fit::{
    choose_placement, fit, Accelerator, AcceleratorKind, Backend, Candidate, Companions, Evidence,
    FitVerdict, Footprint, Label, Recommendation, Recommender, COMFORTABLE_WINDOW_TOKENS,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const GIB: u64 = 1 << 30;
const RTX_4080: &str = "NVIDIA GeForce RTX 4080";
const NINE_B: &str = "qwen3.5-9b-q4_k_m";

fn accelerator(
    name: &str,
    total_bytes: u64,
    kind: AcceleratorKind,
    backend: Backend,
) -> Accelerator {
    Accelerator {
        name: name.into(),
        total_bytes,
        kind,
        backend,
    }
}

fn machine(accelerators: Vec<Accelerator>) -> MachineProfile {
    MachineProfile {
        accelerators,
        system_ram_bytes: 32 * GIB,
        models_volume_free_bytes: 500 * GIB,
        os: "windows".into(),
    }
}

fn rtx_4080() -> MachineProfile {
    machine(vec![accelerator(
        RTX_4080,
        16 * GIB,
        AcceleratorKind::Dedicated,
        Backend::Cuda,
    )])
}

fn processor_only() -> MachineProfile {
    machine(Vec::new())
}

fn expected(profile: &MachineProfile) -> Vec<Recommendation> {
    let catalog: Vec<Candidate> = curated_chat_models()
        .iter()
        .map(Candidate::from_curated)
        .collect();
    Recommender::new(Companions::default_pair()).recommend(profile, &catalog)
}

async fn recommend(profile: &MachineProfile) -> Vec<RecommendationDto> {
    recommend_chat_models(&FixedMachineProfile(profile.clone())).await
}

#[tokio::test]
async fn the_recommended_list_is_the_core_recommenders_in_its_order() {
    let profile = rtx_4080();
    let want = expected(&profile);
    assert!(want.len() >= 3);

    let got = recommend(&profile).await;

    assert_eq!(
        got.iter().map(|r| r.model_id.as_str()).collect::<Vec<_>>(),
        want.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
    );
    let metadata = get_curated_llm_models();
    for (dto, rec) in got.iter().zip(&want) {
        let entry = metadata.iter().find(|m| m.id == rec.id).unwrap();
        let model = curated_chat_models()
            .iter()
            .find(|m| m.id == rec.id)
            .unwrap();
        assert_eq!(dto.name, rec.name);
        assert_eq!(dto.description, entry.description);
        assert_eq!(dto.download_bytes, model.size_bytes);
        assert_eq!(
            dto.verdict,
            FitVerdictDto::from_verdict(&rec.verdict, &profile)
        );
        assert_eq!(dto.reason, rec.reason);
        assert_eq!(
            dto.speed,
            SpeedClassDto::from_active_parameters(model.active_parameters_b)
        );
        let labels: Vec<RecommendationLabelDto> = rec
            .labels
            .iter()
            .map(|label| match label {
                Label::Best => RecommendationLabelDto::Best,
                Label::Fastest => RecommendationLabelDto::Fastest,
                Label::SmallestDownload => RecommendationLabelDto::SmallestDownload,
            })
            .collect();
        assert_eq!(dto.labels, labels);
        match (&dto.evidence, rec.evidence) {
            (EvidenceDto::Untested, Evidence::Untested) => {}
            (EvidenceDto::Measured { suites }, Evidence::Measured { suites: want }) => {
                assert_eq!(
                    suites,
                    &want.iter().map(|s| s.name.to_string()).collect::<Vec<_>>()
                );
            }
            (got, want) => panic!("evidence {got:?} does not match {want:?}"),
        }
    }
}

#[tokio::test]
async fn the_first_recommendation_names_the_card_it_was_judged_against() {
    let got = recommend(&rtx_4080()).await;

    let first = &got[0];
    assert_eq!(first.verdict.kind, FitKindDto::Fits);
    assert_eq!(
        first.verdict.device,
        FitDeviceDto {
            name: RTX_4080.into(),
            kind: DeviceKindDto::Dedicated,
            total_bytes: 16 * GIB,
        }
    );
    assert!(first.verdict.needs_bytes <= first.verdict.budget_bytes);
    assert!(first.verdict.budget_bytes < 16 * GIB);
    assert!(first.labels.contains(&RecommendationLabelDto::Best));
    assert!(got.iter().all(|r| r.verdict.kind == FitKindDto::Fits));
}

#[tokio::test]
async fn the_measured_9b_says_so_on_the_wire() {
    let got = recommend(&rtx_4080()).await;

    let nine = got.iter().find(|r| r.model_id == NINE_B).unwrap();
    match &nine.evidence {
        EvidenceDto::Measured { suites } => assert!(!suites.is_empty()),
        EvidenceDto::Untested => panic!("the 9B was measured"),
    }
    assert!(got
        .iter()
        .filter(|r| r.model_id != NINE_B)
        .all(|r| r.evidence == EvidenceDto::Untested));
}

#[tokio::test]
async fn the_wire_shape_is_camel_case_fields_with_snake_case_enums() {
    let got = recommend(&rtx_4080()).await;

    let json = serde_json::to_value(&got[0]).unwrap();
    assert!(json["modelId"].is_string());
    assert!(json["downloadBytes"].is_u64());
    assert_eq!(json["verdict"]["kind"], "fits");
    assert!(json["verdict"]["needsBytes"].is_u64());
    assert!(json["verdict"]["budgetBytes"].is_u64());
    assert_eq!(json["verdict"]["device"]["kind"], "dedicated");
    assert_eq!(json["verdict"]["device"]["totalBytes"], 16 * GIB);
    assert_eq!(json["labels"][0], "best");
    let nine = got.iter().find(|r| r.model_id == NINE_B).unwrap();
    let nine = serde_json::to_value(nine).unwrap();
    assert_eq!(nine["evidence"]["kind"], "measured");
    assert!(nine["evidence"]["suites"].is_array());
}

#[tokio::test]
async fn a_machine_with_no_usable_card_gets_slow_processor_recommendations() {
    let got = recommend(&processor_only()).await;

    assert!(!got.is_empty());
    assert!(got
        .iter()
        .all(|r| r.verdict.device.kind == DeviceKindDto::Processor));
    assert!(got.iter().all(|r| r.verdict.device.total_bytes == 32 * GIB));
}

#[tokio::test]
async fn an_integrated_gpu_alone_is_reported_as_the_processor() {
    let profile = machine(vec![accelerator(
        "Intel(R) UHD Graphics",
        16 * GIB,
        AcceleratorKind::Integrated,
        Backend::Vulkan,
    )]);

    let got = recommend(&profile).await;

    assert!(!got.is_empty());
    assert!(got
        .iter()
        .all(|r| r.verdict.device.kind == DeviceKindDto::Processor));
}

#[tokio::test]
async fn apple_silicon_is_reported_as_unified_memory() {
    let profile = machine(vec![accelerator(
        "Apple M3 Max",
        28 * GIB,
        AcceleratorKind::Unified,
        Backend::Metal,
    )]);

    let got = recommend(&profile).await;

    assert_eq!(got[0].verdict.device.kind, DeviceKindDto::Unified);
    assert_eq!(got[0].verdict.device.name, "Apple M3 Max");
    assert_eq!(got[0].verdict.device.total_bytes, 28 * GIB);
}

#[test]
fn a_verdict_maps_its_kind_numbers_and_the_device_kind_from_the_profile() {
    let profile = rtx_4080();
    let verdict = FitVerdict::Tight {
        needs: 14_000_000_000,
        budget: 15_000_000_000,
        device: RTX_4080.into(),
        total_bytes: 16 * GIB,
    };

    assert_eq!(
        FitVerdictDto::from_verdict(&verdict, &profile),
        FitVerdictDto {
            kind: FitKindDto::Tight,
            needs_bytes: 14_000_000_000,
            budget_bytes: 15_000_000_000,
            device: FitDeviceDto {
                name: RTX_4080.into(),
                kind: DeviceKindDto::Dedicated,
                total_bytes: 16 * GIB,
            },
        }
    );

    let too_large = FitVerdict::TooLarge {
        needs: 20_000_000_000,
        budget: 15_000_000_000,
        device: "the processor".into(),
        total_bytes: 32 * GIB,
    };
    let dto = FitVerdictDto::from_verdict(&too_large, &processor_only());
    assert_eq!(dto.kind, FitKindDto::TooLarge);
    assert_eq!(dto.device.kind, DeviceKindDto::Processor);
}

#[test]
fn speed_follows_the_active_parameters() {
    assert_eq!(
        SpeedClassDto::from_active_parameters(1.9),
        SpeedClassDto::Fast
    );
    assert_eq!(
        SpeedClassDto::from_active_parameters(3.8),
        SpeedClassDto::Fast
    );
    assert_eq!(
        SpeedClassDto::from_active_parameters(9.0),
        SpeedClassDto::Balanced
    );
    assert_eq!(
        SpeedClassDto::from_active_parameters(11.9),
        SpeedClassDto::Balanced
    );
    assert_eq!(
        SpeedClassDto::from_active_parameters(27.3),
        SpeedClassDto::Thorough
    );
}

struct StubHeaders {
    header: Option<GgufHeader>,
    asked: Mutex<Vec<(String, String)>>,
    calls: AtomicUsize,
}

impl StubHeaders {
    fn new(header: Option<GgufHeader>) -> Self {
        Self {
            header,
            asked: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl GgufHeaderSource for StubHeaders {
    async fn header(&self, repo: &str, file: &str, _revision: Option<&str>) -> Option<GgufHeader> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap()
            .push((repo.to_string(), file.to_string()));
        self.header
    }
}

fn header(weights_bytes: u64) -> GgufHeader {
    GgufHeader {
        weights_bytes,
        full_attention_layers: 8,
        full_attention_kv_heads: 4,
        full_attention_key_length: 256,
        full_attention_value_length: 256,
        sliding_layers: 0,
        sliding_kv_heads: 0,
        sliding_key_length: 0,
        sliding_value_length: 0,
        sliding_window_tokens: 0,
        recurrent_state_bytes: 0,
    }
}

fn curated_row(id: &str) -> ModelMetadata {
    get_curated_llm_models()
        .into_iter()
        .find(|m| m.id == id)
        .unwrap()
}

/// A Hugging Face search row: the catalog's size-based guesses, no header read yet.
fn hugging_face_row(size_gb: f64) -> ModelMetadata {
    ModelMetadata {
        id: "someone--Other-9B-GGUF--Other-9B-Q4_K_M.gguf".into(),
        name: "Other 9B".into(),
        model_id: Some("someone/Other-9B-GGUF".into()),
        default_filename: Some("Other-9B-Q4_K_M.gguf".into()),
        size_gb,
        minimum_ram_gb: (size_gb * 1.5).max(4.0),
        recommended_ram_gb: (size_gb * 2.0).max(8.0),
        total_size_bytes: (size_gb * 1e9) as u64,
        files: Vec::new(),
        ..curated_row(NINE_B)
    }
}

#[tokio::test]
async fn a_curated_row_is_judged_from_its_own_footprint_without_reading_a_header() {
    let profile = rtx_4080();
    let headers = StubHeaders::new(None);
    let model = curated_chat_models()
        .iter()
        .find(|m| m.id == NINE_B)
        .unwrap();
    let placement = choose_placement(&profile, &Companions::default_pair());
    let want = FitVerdictDto::from_verdict(&fit(&model.footprint(), placement.as_ref()), &profile);

    let got = row_fit(&curated_row(NINE_B), &profile, &headers).await;

    assert_eq!(got, Some(want));
    assert_eq!(headers.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_curated_row_and_the_recommended_list_give_one_verdict() {
    let profile = rtx_4080();
    let list = recommend(&profile).await;
    let nine = list.iter().find(|r| r.model_id == NINE_B).unwrap();

    let row = row_fit(&curated_row(NINE_B), &profile, &StubHeaders::new(None)).await;

    assert_eq!(row.as_ref(), Some(&nine.verdict));
}

#[tokio::test]
async fn a_hugging_face_row_has_no_verdict_until_its_header_is_read() {
    let headers = StubHeaders::new(None);

    let got = row_fit(&hugging_face_row(5.0), &rtx_4080(), &headers).await;

    assert_eq!(got, None);
    assert_eq!(
        headers.asked.lock().unwrap().as_slice(),
        [(
            "someone/Other-9B-GGUF".to_string(),
            "Other-9B-Q4_K_M.gguf".to_string()
        )]
    );
}

#[tokio::test]
async fn a_hugging_face_row_has_an_exact_verdict_once_its_header_is_read() {
    let profile = rtx_4080();
    let file_header = header(5_000_000_000);
    let headers = StubHeaders::new(Some(file_header));
    let placement = choose_placement(&profile, &Companions::default_pair());
    let footprint = Footprint::from_header(&file_header);

    let got = row_fit(&hugging_face_row(5.0), &profile, &headers)
        .await
        .unwrap();

    assert_eq!(got.kind, FitKindDto::Fits);
    assert_eq!(got.needs_bytes, footprint.at(COMFORTABLE_WINDOW_TOKENS));
    assert_eq!(got.budget_bytes, placement.budget_bytes());
    assert_eq!(got.device.name, RTX_4080);
}

#[tokio::test]
async fn the_size_guess_does_not_feed_a_hugging_face_verdict() {
    let profile = rtx_4080();
    // The row's guess (1 GB listed, 4 GB minimum) says it fits anywhere; its header says 60 GB.
    let headers = StubHeaders::new(Some(header(60_000_000_000)));

    let got = row_fit(&hugging_face_row(1.0), &profile, &headers)
        .await
        .unwrap();

    assert_eq!(got.kind, FitKindDto::TooLarge);
    assert_eq!(
        got.needs_bytes,
        Footprint::from_header(&header(60_000_000_000)).at(8_192)
    );
}
