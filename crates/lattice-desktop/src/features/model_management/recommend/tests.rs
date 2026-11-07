#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::interfaces::dto::{
    DeviceKindDto, EvidenceDto, FitDeviceDto, FitKindDto, RecommendationLabelDto, SpeedClassDto,
};
use lattice_core::application::contracts::settings::LocalModelConfigDto;
use lattice_core::domain::models::curated::{curated_chat_models, get_curated_llm_models};
use lattice_core::domain::models::fit::{
    choose_placement, fit, footprint, Accelerator, AcceleratorKind, Backend, Candidate, Companions,
    DeviceKind, Evidence, FitDevice, FitKind, FitVerdict, Label, ModelShape, Recommendation,
    Recommender, ServerBuffers, SpeedClass, COMFORTABLE_WINDOW_TOKENS, MINIMUM_WINDOW_TOKENS,
};
use std::collections::BTreeMap;
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
    recommend_chat_models(&FixedMachineProfile(profile.clone()), &BTreeMap::new()).await
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
        assert!(!dto.use_for.is_empty(), "{}", rec.id);
        assert_eq!(dto.use_for, model.use_for);
        assert_eq!(dto.download_bytes, model.size_bytes);
        assert_eq!(dto.verdict, FitVerdictDto::from(&rec.verdict));
        assert_eq!(
            dto.speed,
            SpeedClassDto::from(SpeedClass::from_active_parameters(
                model.active_parameters_b
            ))
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
fn a_verdict_maps_its_kind_numbers_and_device() {
    let verdict = FitVerdict {
        kind: FitKind::Tight,
        needs: 14_000_000_000,
        budget: 15_000_000_000,
        device: FitDevice {
            name: RTX_4080.into(),
            kind: DeviceKind::Dedicated,
            total_bytes: 16 * GIB,
        },
    };

    assert_eq!(
        FitVerdictDto::from(&verdict),
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

    for (kind, want) in [
        (DeviceKind::Unified, DeviceKindDto::Unified),
        (DeviceKind::Processor, DeviceKindDto::Processor),
    ] {
        let dto = FitVerdictDto::from(&FitVerdict {
            kind: FitKind::TooLarge,
            device: FitDevice {
                kind,
                ..verdict.device.clone()
            },
            ..verdict.clone()
        });
        assert_eq!(dto.kind, FitKindDto::TooLarge);
        assert_eq!(dto.device.kind, want);
    }
}

#[test]
fn speed_follows_the_cores_classes() {
    for (active, want) in [
        (1.9, SpeedClassDto::Fast),
        (3.8, SpeedClassDto::Fast),
        (9.0, SpeedClassDto::Balanced),
        (11.9, SpeedClassDto::Balanced),
        (27.3, SpeedClassDto::Thorough),
    ] {
        assert_eq!(
            SpeedClassDto::from(SpeedClass::from_active_parameters(active)),
            want
        );
    }
}

struct StubHeaders {
    header: Option<GgufFacts>,
    asked: Mutex<Vec<(String, String)>>,
    calls: AtomicUsize,
}

impl StubHeaders {
    fn new(header: Option<GgufFacts>) -> Self {
        Self {
            header,
            asked: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl GgufHeaderSource for StubHeaders {
    async fn header(&self, repo: &str, file: &str, _revision: Option<&str>) -> Option<GgufFacts> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap()
            .push((repo.to_string(), file.to_string()));
        self.header.clone()
    }
}

/// The sizing keys of `Qwen3.5-9B-Q4_K_M.gguf`.
fn qwen35_9b_shape() -> ModelShape {
    ModelShape {
        block_count: Some(32),
        head_count: Some(16.into()),
        head_count_kv: Some(4.into()),
        key_length: Some(256),
        value_length: Some(256),
        embedding_length: Some(4096),
        feed_forward_length: Some(12_288),
        full_attention_interval: Some(4),
        ssm_conv_kernel: Some(4),
        ssm_state_size: Some(128),
        ssm_group_count: Some(16),
        ssm_inner_size: Some(4096),
        ..Default::default()
    }
}

fn facts(gpu_weight_bytes: u64) -> GgufFacts {
    GgufFacts {
        gpu_weight_bytes,
        shape: qwen35_9b_shape(),
    }
}

fn price(facts: &GgufFacts, window: u32, quantized: bool) -> u64 {
    footprint(
        facts.gpu_weight_bytes,
        Some(&facts.shape),
        window,
        ServerBuffers::chat(quantized),
    )
    .total()
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
        total_size_bytes: (size_gb * 1e9) as u64,
        files: Vec::new(),
        ..curated_row(NINE_B)
    }
}

#[tokio::test]
async fn a_curated_row_is_judged_from_its_own_pinned_facts_without_reading_a_header() {
    let profile = rtx_4080();
    let headers = StubHeaders::new(None);
    let model = curated_chat_models()
        .iter()
        .find(|m| m.id == NINE_B)
        .unwrap();
    let pinned = Candidate::from_curated(model);
    let placement = choose_placement(&profile, &Companions::default_pair());
    let want = FitVerdictDto::from(&fit(
        pinned.gpu_weight_bytes,
        pinned.shape.as_ref(),
        ServerBuffers::chat(false),
        placement.as_ref(),
    ));

    let got = row_fit(&curated_row(NINE_B), &profile, &headers, &BTreeMap::new()).await;

    assert_eq!(got, Some(want));
    assert_eq!(headers.calls.load(Ordering::SeqCst), 0);
}

fn quantizing(model_id: &str) -> BTreeMap<String, LocalModelConfigDto> {
    BTreeMap::from([(
        model_id.to_string(),
        LocalModelConfigDto {
            kv_cache_quantization: true,
            ..Default::default()
        },
    )])
}

#[tokio::test]
async fn a_rows_verdict_follows_its_own_models_kv_setting() {
    let profile = rtx_4080();
    let headers = StubHeaders::new(None);
    let configs = quantizing(NINE_B);
    let other = "gemma-4-12b-it-q4_k_m";

    let plain = row_fit(&curated_row(NINE_B), &profile, &headers, &BTreeMap::new())
        .await
        .unwrap();
    let quantized = row_fit(&curated_row(NINE_B), &profile, &headers, &configs)
        .await
        .unwrap();
    let other_plain = row_fit(&curated_row(other), &profile, &headers, &BTreeMap::new())
        .await
        .unwrap();
    let other_with_configs = row_fit(&curated_row(other), &profile, &headers, &configs)
        .await
        .unwrap();

    assert!(quantized.needs_bytes < plain.needs_bytes);
    assert_eq!(other_with_configs, other_plain);
}

#[tokio::test]
async fn the_list_judges_each_model_at_its_own_saved_setting() {
    let profile = machine(vec![accelerator(
        RTX_4080,
        24 * GIB,
        AcceleratorKind::Dedicated,
        Backend::Cuda,
    )]);
    let (quantized_id, plain_id) = (NINE_B, "gemma-4-12b-it-q4_k_m");
    let price = |id: &str, quantized: bool| {
        let model = curated_chat_models().iter().find(|m| m.id == id).unwrap();
        let candidate = Candidate::from_curated(model);
        footprint(
            candidate.gpu_weight_bytes,
            candidate.shape.as_ref(),
            COMFORTABLE_WINDOW_TOKENS,
            ServerBuffers::chat(quantized),
        )
        .total()
    };

    let list =
        recommend_chat_models(&FixedMachineProfile(profile), &quantizing(quantized_id)).await;

    let needs_of = |id: &str| {
        list.iter()
            .find(|r| r.model_id == id)
            .unwrap()
            .verdict
            .needs_bytes
    };
    assert_eq!(needs_of(quantized_id), price(quantized_id, true));
    assert_eq!(needs_of(plain_id), price(plain_id, false));
    assert!(price(quantized_id, true) < price(quantized_id, false));
}

#[tokio::test]
async fn a_curated_row_and_the_recommended_list_give_one_verdict() {
    let profile = rtx_4080();
    let list = recommend(&profile).await;
    let nine = list.iter().find(|r| r.model_id == NINE_B).unwrap();

    let row = row_fit(
        &curated_row(NINE_B),
        &profile,
        &StubHeaders::new(None),
        &BTreeMap::new(),
    )
    .await;

    assert_eq!(row.as_ref(), Some(&nine.verdict));
}

#[tokio::test]
async fn a_hugging_face_row_has_no_verdict_until_its_header_is_read() {
    let headers = StubHeaders::new(None);

    let got = row_fit(
        &hugging_face_row(5.0),
        &rtx_4080(),
        &headers,
        &BTreeMap::new(),
    )
    .await;

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
    let file_facts = facts(5_000_000_000);
    let headers = StubHeaders::new(Some(file_facts.clone()));
    let placement = choose_placement(&profile, &Companions::default_pair());

    let got = row_fit(&hugging_face_row(5.0), &profile, &headers, &BTreeMap::new())
        .await
        .unwrap();

    assert_eq!(got.kind, FitKindDto::Fits);
    assert_eq!(
        got.needs_bytes,
        price(&file_facts, COMFORTABLE_WINDOW_TOKENS, false)
    );
    assert_eq!(got.budget_bytes, placement.budget_bytes());
    assert_eq!(got.device.name, RTX_4080);
}

#[tokio::test]
async fn the_size_guess_does_not_feed_a_hugging_face_verdict() {
    let profile = rtx_4080();
    // The row's guess (1 GB listed, 4 GB minimum) says it fits anywhere; its header says 60 GB.
    let file_facts = facts(60_000_000_000);
    let headers = StubHeaders::new(Some(file_facts.clone()));

    let got = row_fit(&hugging_face_row(1.0), &profile, &headers, &BTreeMap::new())
        .await
        .unwrap();

    assert_eq!(got.kind, FitKindDto::TooLarge);
    assert_eq!(
        got.needs_bytes,
        price(&file_facts, MINIMUM_WINDOW_TOKENS, false)
    );
}
