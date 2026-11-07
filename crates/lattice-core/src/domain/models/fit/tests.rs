use super::*;
use crate::domain::models::curated::{
    curated_chat_models, curated_embedding_pinned, curated_gguf_files,
    get_curated_embedding_models, CURATED_RERANKER,
};

const GIB: u64 = 1 << 30;
const RTX_4080: &str = "NVIDIA GeForce RTX 4080";

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

fn machine(accelerators: Vec<Accelerator>, system_ram_bytes: u64) -> MachineProfile {
    MachineProfile {
        accelerators,
        system_ram_bytes,
        models_volume_free_bytes: 500 * GIB,
        os: "windows".into(),
    }
}

fn card_machine(total_bytes: u64) -> MachineProfile {
    machine(
        vec![accelerator(
            RTX_4080,
            total_bytes,
            AcceleratorKind::Dedicated,
            Backend::Cuda,
        )],
        32 * GIB,
    )
}

fn rtx_4080() -> MachineProfile {
    card_machine(16 * GIB)
}

fn rtx_4080_with_integrated_gpu() -> MachineProfile {
    let mut profile = rtx_4080();
    profile.accelerators.push(accelerator(
        "Intel(R) UHD Graphics",
        16 * GIB,
        AcceleratorKind::Integrated,
        Backend::Vulkan,
    ));
    profile
}

fn apple_silicon() -> MachineProfile {
    MachineProfile {
        os: "macos".into(),
        ..machine(
            vec![accelerator(
                "Apple M2 Pro",
                12 * GIB,
                AcceleratorKind::Unified,
                Backend::Metal,
            )],
            16 * GIB,
        )
    }
}

fn processor_only() -> MachineProfile {
    machine(Vec::new(), 32 * GIB)
}

/// Gemma 4's layer layout: five sliding layers, then one global, repeated.
fn gemma4_pattern(layers: usize) -> Vec<bool> {
    (0..layers).map(|layer| layer % 6 != 5).collect()
}

/// Gemma 4 12B's KV heads: 8 on each sliding layer, 1 on each global one.
fn gemma4_12b_kv_heads() -> Vec<u32> {
    gemma4_pattern(48)
        .iter()
        .map(|&slides| if slides { 8 } else { 1 })
        .collect()
}

/// The sizing keys of `gemma-4-12b-it-Q4_K_M.gguf`.
fn gemma4_12b_shape() -> ModelShape {
    ModelShape {
        block_count: Some(48),
        head_count: Some(16.into()),
        head_count_kv: Some(PerLayer::Layers(gemma4_12b_kv_heads())),
        key_length: Some(512),
        value_length: Some(512),
        key_length_swa: Some(256),
        value_length_swa: Some(256),
        embedding_length: Some(3840),
        feed_forward_length: Some(15_360),
        sliding_window: Some(1024),
        sliding_window_pattern: Some(gemma4_pattern(48)),
        shared_kv_layers: Some(0),
        ..Default::default()
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

/// The sizing keys of `gemma-4-E4B-it-Q4_K_M.gguf`: the last 18 of 42 layers share caches.
fn gemma4_e4b_shape() -> ModelShape {
    ModelShape {
        block_count: Some(42),
        head_count: Some(8.into()),
        head_count_kv: Some(2.into()),
        key_length: Some(512),
        value_length: Some(512),
        key_length_swa: Some(256),
        value_length_swa: Some(256),
        embedding_length: Some(2560),
        sliding_window: Some(512),
        sliding_window_pattern: Some(gemma4_pattern(42)),
        shared_kv_layers: Some(18),
        ..Default::default()
    }
}

/// A one-layer shape that costs `per_token` bytes of cache a token.
fn flat_shape(per_token: u32) -> ModelShape {
    ModelShape {
        block_count: Some(1),
        head_count_kv: Some(1.into()),
        key_length: Some(per_token / 4),
        value_length: Some(per_token / 4),
        ..Default::default()
    }
}

/// Qwen3-0.6B's shape, the embedder's and the reranker's.
fn small_shape() -> ModelShape {
    ModelShape {
        block_count: Some(28),
        head_count: Some(16.into()),
        head_count_kv: Some(8.into()),
        key_length: Some(128),
        value_length: Some(128),
        embedding_length: Some(1024),
        feed_forward_length: Some(3072),
        ..Default::default()
    }
}

fn server_bytes(server: &CompanionServer) -> u64 {
    footprint(
        server.gpu_weight_bytes,
        server.shape.as_ref(),
        server.window_tokens * server.slots,
        server.buffers,
    )
    .total()
}

fn companions() -> Companions {
    Companions {
        embedding: CompanionServer {
            gpu_weight_bytes: 1_197_629_632,
            shape: Some(small_shape()),
            window_tokens: 2_048,
            slots: 4,
            buffers: ServerBuffers {
                kv_cache_quantization: false,
                micro_batch: 2_048,
            },
        },
        reranker: CompanionServer {
            gpu_weight_bytes: 639_153_184,
            shape: Some(small_shape()),
            window_tokens: 4_096,
            slots: 1,
            buffers: ServerBuffers {
                kv_cache_quantization: false,
                micro_batch: 4_096,
            },
        },
    }
}

fn curated(id: &str) -> Candidate {
    let model = curated_chat_models()
        .iter()
        .find(|model| model.id == id)
        .unwrap_or_else(|| panic!("no curated chat model {id}"));
    Candidate::from_curated(model)
}

fn candidate(
    id: &str,
    quantization: &str,
    quantization_bits: u8,
    parameters_b: f32,
    gpu_weight_bytes: u64,
    shape: Option<ModelShape>,
    rank: u8,
) -> Candidate {
    Candidate {
        id: id.into(),
        name: id.into(),
        quantization: quantization.into(),
        quantization_bits,
        parameters_b,
        active_parameters_b: parameters_b,
        download_bytes: gpu_weight_bytes,
        gpu_weight_bytes,
        shape,
        rank,
        evidence: Evidence::Untested,
    }
}

/// A candidate whose header says nothing, so it costs its weights and no more.
fn plain(
    id: &str,
    quantization: &str,
    quantization_bits: u8,
    parameters_b: f32,
    weights: u64,
    rank: u8,
) -> Candidate {
    candidate(
        id,
        quantization,
        quantization_bits,
        parameters_b,
        weights,
        None,
        rank,
    )
}

fn gemma_12b(rank: u8) -> Candidate {
    candidate(
        "gemma-12b",
        "Q4_K_M",
        4,
        12.0,
        7_121_861_440,
        Some(gemma4_12b_shape()),
        rank,
    )
}

fn qwen_9b_local(rank: u8) -> Candidate {
    candidate(
        "qwen-9b-local",
        "Q4_K_M",
        4,
        9.0,
        5_680_522_464 - 572_000_000,
        Some(qwen35_9b_shape()),
        rank,
    )
}

fn judge(candidate: &Candidate, placement: &dyn Placement) -> FitVerdict {
    fit(
        candidate.gpu_weight_bytes,
        candidate.shape.as_ref(),
        ServerBuffers::chat(false),
        placement,
    )
}

fn ids(recommendations: &[Recommendation]) -> Vec<&str> {
    recommendations.iter().map(|r| r.id.as_str()).collect()
}

fn recommend(profile: &MachineProfile, catalog: &[Candidate]) -> Vec<Recommendation> {
    Recommender::new(companions(), false).recommend(profile, catalog)
}

fn recommend_alone(profile: &MachineProfile, catalog: &[Candidate]) -> Vec<Recommendation> {
    Recommender::new(Companions::none(), false).recommend(profile, catalog)
}

fn needs(verdict: &FitVerdict) -> u64 {
    verdict.needs
}

fn is_processor(placement: &dyn Placement) -> bool {
    placement.device().kind == DeviceKind::Processor
}

#[test]
fn rtx_4080_finds_the_27b_too_large_and_the_9b_fitting() {
    let placement = choose_placement(&rtx_4080(), &companions());
    let big = curated("qwen3.8-27b-ud-q4_k_m");
    let small = curated("qwen3.5-9b-q4_k_m");

    let too_large = judge(&big, placement.as_ref());
    assert_eq!(too_large.kind, FitKind::TooLarge);
    assert!(too_large.needs > too_large.budget);
    assert_eq!(
        too_large.device,
        FitDevice {
            name: RTX_4080.into(),
            kind: DeviceKind::Dedicated,
            total_bytes: 16 * GIB,
        }
    );
    assert!(too_large.budget < too_large.device.total_bytes);

    let fits = judge(&small, placement.as_ref());
    assert_eq!(fits.kind, FitKind::Fits);
    assert!(fits.needs <= fits.budget);
    assert_eq!(fits.device, too_large.device);
    assert!(fits.budget < fits.device.total_bytes);
}

#[test]
fn rtx_4080_ranks_a_whole_on_the_card_12b_above_the_9b() {
    let nine = curated("qwen3.5-9b-q4_k_m");
    let twelve = gemma_12b(nine.rank + 1);
    let catalog = [nine, curated("qwen3.8-27b-ud-q4_k_m"), twelve];

    let list = recommend(&rtx_4080(), &catalog);

    assert_eq!(ids(&list), ["gemma-12b", "qwen3.5-9b-q4_k_m"]);
    assert!(list
        .iter()
        .all(|r| r.verdict.device.kind == DeviceKind::Dedicated));
    assert!(list.iter().all(|r| r.verdict.device.name == RTX_4080));
    assert_eq!(list[0].verdict.kind, FitKind::Fits);
}

#[test]
fn a_dedicated_card_budget_is_its_total_less_the_fit_target_the_reserve_and_the_companions() {
    let placement = choose_placement(&rtx_4080(), &companions());

    assert_eq!(
        placement.budget_bytes(),
        16 * GIB - FIT_TARGET_BYTES - DISPLAY_RESERVE_BYTES - companions().bytes()
    );
    assert_eq!(placement.device().name, RTX_4080);
    assert_eq!(placement.device().kind, DeviceKind::Dedicated);
}

#[test]
fn companions_are_priced_by_the_one_footprint_at_their_own_window_and_slots() {
    let c = companions();

    assert_eq!(
        c.bytes(),
        server_bytes(&c.embedding) + server_bytes(&c.reranker)
    );
    assert!(c.bytes() > c.embedding.gpu_weight_bytes + c.reranker.gpu_weight_bytes);
    assert_eq!(Companions::none().bytes(), 0);
}

#[test]
fn an_integrated_gpu_takes_the_companions_off_the_card() {
    let c = companions();
    let alone = choose_placement(&rtx_4080(), &c);
    let beside = choose_placement(&rtx_4080_with_integrated_gpu(), &c);

    assert!(c.bytes() > 0);
    assert_eq!(beside.budget_bytes(), 16 * GIB - FIT_TARGET_BYTES);
    assert_eq!(
        beside.budget_bytes() - alone.budget_bytes(),
        c.bytes() + DISPLAY_RESERVE_BYTES
    );
    assert_eq!(beside.device().name, RTX_4080);
    assert_eq!(beside.device().kind, DeviceKind::Dedicated);
}

#[test]
fn a_model_that_needs_exactly_the_companions_share_fits_only_beside_an_integrated_gpu() {
    let c = companions();
    let budget_beside = choose_placement(&rtx_4080_with_integrated_gpu(), &c).budget_bytes();
    let just_fits = plain("just-fits", "Q4_K_M", 4, 12.0, budget_beside, 50);

    let beside = recommend(
        &rtx_4080_with_integrated_gpu(),
        std::slice::from_ref(&just_fits),
    );
    let alone = recommend(&rtx_4080(), std::slice::from_ref(&just_fits));

    assert_eq!(ids(&beside), ["just-fits"]);
    assert_eq!(beside[0].verdict.kind, FitKind::Fits);
    assert!(alone.is_empty());
}

#[test]
fn apple_silicon_uses_the_metal_working_set_size() {
    let c = companions();
    let placement = choose_placement(&apple_silicon(), &c);

    assert_eq!(placement.budget_bytes(), 12 * GIB - c.bytes());
    assert_eq!(
        placement.device(),
        FitDevice {
            name: "Apple M2 Pro".into(),
            kind: DeviceKind::Unified,
            total_bytes: 12 * GIB,
        }
    );
}

#[test]
fn processor_only_is_marked_slow() {
    let profile = processor_only();
    let placement = choose_placement(&profile, &companions());

    assert!(is_processor(placement.as_ref()));
    assert!(placement.budget_bytes() > 0);
    assert!(placement.budget_bytes() < profile.system_ram_bytes);

    let list = recommend(&profile, &[curated("qwen3.5-9b-q4_k_m")]);
    assert_eq!(ids(&list), ["qwen3.5-9b-q4_k_m"]);
    assert!(list
        .iter()
        .all(|r| r.verdict.device.kind == DeviceKind::Processor));
}

#[test]
fn an_integrated_gpu_alone_is_treated_as_the_processor() {
    let profile = machine(
        vec![accelerator(
            "Intel(R) UHD Graphics",
            16 * GIB,
            AcceleratorKind::Integrated,
            Backend::Vulkan,
        )],
        32 * GIB,
    );
    let placement = choose_placement(&profile, &companions());

    assert!(is_processor(placement.as_ref()));
    assert_eq!(placement.device().total_bytes, 32 * GIB);
    assert_eq!(
        placement.budget_bytes(),
        choose_placement(&processor_only(), &companions()).budget_bytes()
    );
}

#[test]
fn a_sub_4_bit_quant_of_a_model_under_20b_is_never_recommended() {
    let catalog = [
        plain("nine-iq3", "IQ3_S", 3, 9.0, 4_000_000_000, 200),
        plain("nineteen-iq3", "IQ3_S", 3, 19.9, 9_000_000_000, 190),
        plain("nine-q4", "Q4_K_M", 4, 9.0, 5_000_000_000, 10),
    ];

    let list = recommend_alone(&card_machine(24 * GIB), &catalog);

    assert_eq!(ids(&list), ["nine-q4"]);
}

#[test]
fn a_sub_4_bit_quant_of_a_model_of_20b_or_more_may_be_recommended() {
    let catalog = [
        plain("twenty-iq3", "IQ3_S", 3, 20.0, 9_000_000_000, 60),
        plain("twenty-seven-iq3", "IQ3_S", 3, 27.0, 12_000_000_000, 80),
        plain("nine-q4", "Q4_K_M", 4, 9.0, 5_000_000_000, 10),
    ];

    let list = recommend_alone(&card_machine(24 * GIB), &catalog);

    assert_eq!(ids(&list), ["twenty-seven-iq3", "twenty-iq3", "nine-q4"]);
}

fn labelled(
    id: &str,
    parameters_b: f32,
    active_parameters_b: f32,
    download_bytes: u64,
    rank: u8,
) -> Candidate {
    Candidate {
        active_parameters_b,
        download_bytes,
        ..plain(id, "Q4_K_M", 4, parameters_b, download_bytes, rank)
    }
}

#[test]
fn labels_go_to_the_best_the_fastest_and_the_smallest_download() {
    let catalog = [
        labelled("small", 4.0, 2.0, 2_500_000_000, 50),
        labelled("big", 27.0, 27.0, 12_000_000_000, 90),
        labelled("moe", 26.0, 4.0, 14_000_000_000, 80),
    ];

    let list = recommend_alone(&card_machine(24 * GIB), &catalog);

    assert_eq!(ids(&list), ["big", "moe", "small"]);
    assert_eq!(list[0].labels, [Label::Best]);
    assert!(list[1].labels.is_empty());
    assert_eq!(list[2].labels, [Label::Fastest, Label::SmallestDownload]);
}

#[test]
fn the_order_and_labels_do_not_depend_on_the_catalog_order_or_the_run() {
    let a = labelled("a", 9.0, 9.0, 5_000_000_000, 70);
    let b = labelled("b", 9.0, 9.0, 6_000_000_000, 70);
    let c = labelled("c", 12.0, 12.0, 7_000_000_000, 90);
    let forward = [a.clone(), b.clone(), c.clone()];
    let backward = [c, b, a];
    let profile = card_machine(24 * GIB);

    let first = recommend_alone(&profile, &forward);

    assert_eq!(ids(&first), ["c", "a", "b"]);
    assert_eq!(first, recommend_alone(&profile, &forward));
    assert_eq!(first, recommend_alone(&profile, &backward));
}

#[test]
fn the_9b_carries_what_lattice_measured_and_an_unmeasured_model_does_not() {
    let nine = curated("qwen3.5-9b-q4_k_m");
    let twelve = gemma_12b(nine.rank + 1);

    let list = recommend(&rtx_4080(), &[nine, twelve]);

    let measured = list.iter().find(|r| r.id == "qwen3.5-9b-q4_k_m").unwrap();
    match measured.evidence {
        Evidence::Measured { suites } => {
            assert!(!suites.is_empty());
            assert!(suites.iter().all(|s| !s.result_file.is_empty()));
        }
        Evidence::Untested => panic!("the 9B was measured"),
    }
    let untested = list.iter().find(|r| r.id == "gemma-12b").unwrap();
    assert_eq!(untested.evidence, Evidence::Untested);
}

#[test]
fn tight_is_offered_only_when_nothing_fits() {
    let profile = card_machine(16 * GIB);
    let budget = choose_placement(&profile, &Companions::none()).budget_bytes();
    let shape = flat_shape(100_000);
    let slack = 100_000_000;
    let at_minimum = footprint(
        0,
        Some(&shape),
        MINIMUM_WINDOW_TOKENS,
        ServerBuffers::chat(false),
    );
    let tight = candidate(
        "tight",
        "Q4_K_M",
        4,
        12.0,
        budget - at_minimum.total() - slack,
        Some(shape),
        90,
    );
    let roomy = plain("roomy", "Q4_K_M", 4, 4.0, 3_000_000_000, 10);
    let too_large = plain("too-large", "Q4_K_M", 4, 27.0, 40_000_000_000, 95);

    let only_tight = recommend_alone(&profile, &[tight.clone(), too_large.clone()]);
    assert_eq!(ids(&only_tight), ["tight"]);
    assert_eq!(only_tight[0].verdict.kind, FitKind::Tight);

    let both = recommend_alone(&profile, &[tight, roomy, too_large]);
    assert_eq!(ids(&both), ["roomy"]);
    assert_eq!(both[0].verdict.kind, FitKind::Fits);
}

#[test]
fn nothing_that_is_too_large_is_ever_offered() {
    let catalog = [plain("huge", "Q4_K_M", 4, 70.0, 40_000_000_000, 99)];

    assert!(recommend_alone(&rtx_4080(), &catalog).is_empty());
}

/// Copied from the runtime's `gguf_metadata` tests, against the same numbers.
#[test]
fn a_grouped_query_cache_costs_what_its_dimensions_say() {
    let shape = ModelShape {
        block_count: Some(32),
        head_count_kv: Some(4.into()),
        key_length: Some(256),
        value_length: Some(256),
        ..Default::default()
    };

    assert_eq!(shape.kv_cache_bytes(1, false), Some(128 * 1024));
    assert_eq!(shape.kv_cache_bytes(8192, false), Some(8192 * 128 * 1024));
}

#[test]
fn a_plain_model_costs_the_same_for_every_token_of_its_window() {
    let shape = ModelShape {
        block_count: Some(32),
        embedding_length: Some(4096),
        feed_forward_length: Some(14_336),
        head_count: Some(32.into()),
        head_count_kv: Some(8.into()),
        ..Default::default()
    };

    assert_eq!(shape.kv_cache_bytes(1, false), Some(131_072));
    for window in [4096, 8192, 32_768, 131_072] {
        assert_eq!(
            shape.kv_cache_bytes(window, false),
            Some(u64::from(window) * 131_072)
        );
    }
}

#[test]
fn sliding_window_layers_stop_growing_at_their_window() {
    let shape = gemma4_12b_shape();
    let global = 8 * 1024 * 2;
    let sliding = 40 * 8 * 512 * 2;

    assert_eq!(shape.kv_cache_bytes(1, false), Some(global + sliding));
    assert_eq!(
        shape.kv_cache_bytes(4096, false),
        Some((global + sliding) * 4096)
    );
    assert_eq!(
        shape.kv_cache_bytes(32_768, false),
        Some(global * 32_768 + sliding * 4608)
    );
    assert_eq!(shape.kv_cache_bytes(32_768, false), Some(2_046_820_352));
}

#[test]
fn layers_that_share_an_earlier_cache_cost_nothing() {
    let shape = gemma4_e4b_shape();
    let global = 4 * 2 * 1024 * 2;
    let sliding = 20 * 2 * 512 * 2;

    assert_eq!(shape.kv_cache_bytes(1, false), Some(global + sliding));
    assert_eq!(
        shape.kv_cache_bytes(32_768, false),
        Some(global * 32_768 + sliding * 2560)
    );
}

#[test]
fn a_qwen_hybrid_s_recurrent_state_is_fixed_per_linear_layer_and_slot() {
    let per_layer_and_slot = (3 * (4096 + 2 * 16 * 128) + 128 * 4096) * 4;

    assert_eq!(
        qwen35_9b_shape().recurrent_state_bytes(),
        24 * 4 * per_layer_and_slot
    );
}

#[test]
fn a_model_without_linear_attention_keeps_no_recurrent_state() {
    let hybrid = qwen35_9b_shape();
    let plain = ModelShape {
        full_attention_interval: None,
        ..hybrid.clone()
    };
    let undescribed = ModelShape {
        ssm_inner_size: None,
        ..hybrid
    };

    assert_eq!(plain.recurrent_state_bytes(), 0);
    assert_eq!(undescribed.recurrent_state_bytes(), 0);
}

#[test]
fn the_compute_buffer_is_a_micro_batch_of_a_layer_and_holds_no_logits() {
    let shape = qwen35_9b_shape();

    assert_eq!(
        shape.compute_buffer_bytes(16_384, 512),
        512 * (16_384 * 6 + 12_288 * 2 * 4)
    );
    assert_eq!(
        shape.compute_buffer_bytes(65_536, 512),
        512 * (65_536 * 6 + 12_288 * 2 * 4)
    );
}

#[test]
fn the_compute_buffer_grows_with_the_micro_batch() {
    let shape = ModelShape {
        feed_forward_length: Some(3_072),
        ..Default::default()
    };

    assert_eq!(
        shape.compute_buffer_bytes(4_096, 4_096),
        4_096 * (4_096 * 6 + 3_072 * 2 * 4)
    );
}

#[test]
fn the_footprint_is_the_runtimes_weights_cache_state_and_compute() {
    let shape = qwen35_9b_shape();
    let buffers = ServerBuffers::chat(false);

    let got = footprint(5_000_000_000, Some(&shape), 32_768, buffers);

    assert_eq!(got.weights, 5_000_000_000);
    assert_eq!(got.cache, shape.kv_cache_bytes(32_768, false).unwrap());
    assert_eq!(got.state, shape.recurrent_state_bytes());
    assert_eq!(got.compute, shape.compute_buffer_bytes(32_768, 512));
    assert_eq!(got.window, 32_768);
    assert_eq!(
        got.total(),
        got.weights + got.cache + got.state + got.compute
    );
    assert_eq!(footprint(7, None, 4_096, buffers).total(), 7);
}

#[test]
fn the_recommender_judges_a_sliding_window_model_by_the_runtimes_footprint() {
    let model = gemma_12b(50);
    let want = footprint(
        model.gpu_weight_bytes,
        model.shape.as_ref(),
        COMFORTABLE_WINDOW_TOKENS,
        ServerBuffers::chat(false),
    )
    .total();

    let list = recommend(&rtx_4080(), std::slice::from_ref(&model));

    assert_eq!(needs(&list[0].verdict), want);
}

#[test]
fn the_recommender_judges_a_hybrid_by_the_runtimes_footprint() {
    let model = qwen_9b_local(50);
    let want = footprint(
        model.gpu_weight_bytes,
        model.shape.as_ref(),
        COMFORTABLE_WINDOW_TOKENS,
        ServerBuffers::chat(false),
    )
    .total();

    let list = recommend(&rtx_4080(), std::slice::from_ref(&model));

    assert_eq!(needs(&list[0].verdict), want);
}

#[test]
fn the_kv_cache_setting_changes_what_a_model_needs() {
    for model in [gemma_12b(50), qwen_9b_local(50)] {
        let price = |quantized: bool| {
            footprint(
                model.gpu_weight_bytes,
                model.shape.as_ref(),
                COMFORTABLE_WINDOW_TOKENS,
                ServerBuffers::chat(quantized),
            )
            .total()
        };

        let plain = Recommender::new(companions(), false)
            .recommend(&rtx_4080(), std::slice::from_ref(&model));
        let quantized = Recommender::new(companions(), true)
            .recommend(&rtx_4080(), std::slice::from_ref(&model));

        assert_eq!(needs(&plain[0].verdict), price(false), "{}", model.id);
        assert_eq!(needs(&quantized[0].verdict), price(true), "{}", model.id);
        assert!(price(true) < price(false), "{}", model.id);
    }
}

#[test]
fn a_curated_candidate_is_priced_from_its_pinned_shape_and_cpu_weights() {
    let model = curated_chat_models()
        .iter()
        .find(|m| m.id == "qwen3.5-9b-q4_k_m")
        .unwrap();
    let pinned = model.pinned();

    let candidate = Candidate::from_curated(model);

    let projector = model.vision.as_ref().map_or(0, |v| v.projector_bytes);
    assert!(projector > 0);
    assert_eq!(
        candidate.gpu_weight_bytes,
        model.size_bytes - pinned.cpu_weight_bytes + projector
    );
    assert_eq!(candidate.shape, Some(pinned.shape));
    assert_eq!(candidate.download_bytes, model.size_bytes);
}

#[test]
fn the_default_companions_are_priced_as_the_runtime_launches_them() {
    let embedder = curated_embedding_pinned();
    let embedder_bytes = get_curated_embedding_models()
        .iter()
        .flat_map(|model| model.files.iter())
        .find(|file| file.filename.ends_with(".gguf"))
        .map(|file| file.size_bytes)
        .unwrap();
    let reranker = CURATED_RERANKER.pinned();
    let want = footprint(
        embedder_bytes - embedder.cpu_weight_bytes,
        Some(&embedder.shape),
        2_048 * 4,
        ServerBuffers {
            kv_cache_quantization: false,
            micro_batch: 2_048,
        },
    )
    .total()
        + footprint(
            CURATED_RERANKER.bytes - reranker.cpu_weight_bytes,
            Some(&reranker.shape),
            4_096,
            ServerBuffers {
                kv_cache_quantization: false,
                micro_batch: 4_096,
            },
        )
        .total();

    let pair = Companions::default_pair();

    assert_eq!(pair.bytes(), want);
    assert_eq!(pair.embedding.slots, 4);
    assert_eq!(pair.embedding.window_tokens, 2_048);
    assert_eq!(pair.reranker.slots, 1);
    assert_eq!(pair.reranker.window_tokens, 4_096);
}

#[test]
fn every_curated_gguf_pins_a_shape_and_its_cpu_weight_bytes() {
    let files = curated_gguf_files();

    assert_eq!(files.len(), curated_chat_models().len() + 2);
    for file in &files {
        assert!(
            file.url.starts_with("https://huggingface.co/"),
            "{}",
            file.name
        );
        assert!(file.pinned.shape.block_count.is_some(), "{}", file.name);
        assert!(
            file.pinned.shape.kv_cache_bytes(8_192, false).is_some(),
            "{}",
            file.name
        );
    }
    for model in curated_chat_models() {
        assert!(
            model.pinned().cpu_weight_bytes < model.size_bytes,
            "{}",
            model.id
        );
    }
}

fn gib(bytes: u64) -> String {
    format!("{:.2}", bytes as f64 / GIB as f64)
}

fn reference_machines() -> Vec<(&'static str, MachineProfile)> {
    vec![
        ("RTX 4080 alone (16 GiB)", rtx_4080()),
        (
            "RTX 4070 Laptop (8 GiB) beside an Iris Xe",
            machine(
                vec![
                    accelerator(
                        "NVIDIA GeForce RTX 4070 Laptop GPU",
                        7948 * (1 << 20),
                        AcceleratorKind::Dedicated,
                        Backend::Vulkan,
                    ),
                    accelerator(
                        "Intel(R) Iris(R) Xe Graphics",
                        32581 * (1 << 20),
                        AcceleratorKind::Integrated,
                        Backend::Vulkan,
                    ),
                ],
                32 * GIB,
            ),
        ),
        (
            "Apple M-series, 21 GiB working set",
            machine(
                vec![accelerator(
                    "Apple M3 Pro",
                    21 * GIB,
                    AcceleratorKind::Unified,
                    Backend::Metal,
                )],
                32 * GIB,
            ),
        ),
        ("processor only, 32 GiB RAM", processor_only()),
    ]
}

#[test]
#[ignore = "prints the recommended lists for reference machines; run with --ignored --nocapture"]
fn prints_the_recommended_lists_for_reference_machines() {
    let catalog: Vec<Candidate> = curated_chat_models()
        .iter()
        .map(Candidate::from_curated)
        .collect();
    for (label, profile) in reference_machines() {
        for quantized in [false, true] {
            println!(
                "== {label}, KV cache quantization {}",
                if quantized { "on" } else { "off" }
            );
            let list = Recommender::new(Companions::default_pair(), quantized)
                .recommend(&profile, &catalog);
            for (position, rec) in list.iter().enumerate() {
                println!(
                    "{:>2}. {} {:?} needs {} GiB of {} GiB budget, labels {:?}",
                    position + 1,
                    rec.id,
                    rec.verdict.kind,
                    gib(rec.verdict.needs),
                    gib(rec.verdict.budget),
                    rec.labels
                );
            }
        }
    }

    let placement = choose_placement(&rtx_4080(), &Companions::default_pair());
    println!(
        "== every curated chat entry on the RTX 4080, budget {} GiB",
        gib(placement.budget_bytes())
    );
    for quantized in [false, true] {
        for model in &catalog {
            let price = |window: u32| {
                footprint(
                    model.gpu_weight_bytes,
                    model.shape.as_ref(),
                    window,
                    ServerBuffers::chat(quantized),
                )
                .total()
            };
            println!(
                "KV quantization {:>3}: {} needs {} GiB at {} tokens, {} GiB at {}",
                if quantized { "on" } else { "off" },
                model.id,
                gib(price(COMFORTABLE_WINDOW_TOKENS)),
                COMFORTABLE_WINDOW_TOKENS,
                gib(price(MINIMUM_WINDOW_TOKENS)),
                MINIMUM_WINDOW_TOKENS
            );
        }
    }
}

fn laptop_card_beside_an_integrated_gpu() -> MachineProfile {
    machine(
        vec![
            accelerator(
                "NVIDIA GeForce RTX 4070 Laptop GPU",
                7948 * (1 << 20),
                AcceleratorKind::Dedicated,
                Backend::Vulkan,
            ),
            accelerator(
                "Intel(R) Iris(R) Xe Graphics",
                32581 * (1 << 20),
                AcceleratorKind::Integrated,
                Backend::Vulkan,
            ),
        ],
        32 * GIB,
    )
}

#[test]
fn a_card_beside_an_integrated_gpu_holds_no_desktop_so_only_the_fit_target_comes_off() {
    let placement = choose_placement(&laptop_card_beside_an_integrated_gpu(), &companions());

    assert_eq!(
        placement.budget_bytes(),
        7948 * (1 << 20) - FIT_TARGET_BYTES
    );
    assert_eq!(placement.device().kind, DeviceKind::Dedicated);
}

#[test]
fn the_9b_is_tight_on_an_8_gib_laptop_card_with_an_f16_cache_and_fits_with_a_quantized_one() {
    let nine = curated("qwen3.5-9b-q4_k_m");
    let on_laptop = |quantized: bool| {
        Recommender::new(Companions::default_pair(), quantized).recommend(
            &laptop_card_beside_an_integrated_gpu(),
            std::slice::from_ref(&nine),
        )
    };

    let f16 = on_laptop(false);
    let quantized = on_laptop(true);

    assert_eq!(ids(&f16), ["qwen3.5-9b-q4_k_m"]);
    assert_eq!(f16[0].verdict.kind, FitKind::Tight);
    assert_eq!(ids(&quantized), ["qwen3.5-9b-q4_k_m"]);
    assert_eq!(quantized[0].verdict.kind, FitKind::Fits);
}

#[test]
fn a_card_alone_keeps_the_display_reserve() {
    let placement = choose_placement(&card_machine(8 * GIB), &Companions::none());

    assert_eq!(
        placement.budget_bytes(),
        8 * GIB - FIT_TARGET_BYTES - DISPLAY_RESERVE_BYTES
    );
}

fn with_active_parameters(id: &str, active_parameters_b: f32, rank: u8) -> Candidate {
    Candidate {
        active_parameters_b,
        ..plain(id, "Q4_K_M", 4, active_parameters_b, 3_000_000_000, rank)
    }
}

#[test]
fn on_the_processor_only_fast_and_balanced_models_are_offered() {
    let catalog = [
        with_active_parameters("thorough", 14.1, 90),
        with_active_parameters("balanced-edge", 14.0, 50),
        with_active_parameters("fast-edge", 5.0, 40),
    ];

    let list = recommend_alone(&processor_only(), &catalog);

    assert_eq!(ids(&list), ["balanced-edge", "fast-edge"]);
}

#[test]
fn a_dedicated_card_still_offers_thorough_models() {
    let catalog = [
        with_active_parameters("thorough", 14.1, 90),
        with_active_parameters("balanced-edge", 14.0, 50),
        with_active_parameters("fast-edge", 5.0, 40),
    ];

    let list = recommend_alone(&card_machine(24 * GIB), &catalog);

    assert_eq!(ids(&list), ["thorough", "balanced-edge", "fast-edge"]);
}

#[test]
fn a_32_gib_processor_only_machine_is_not_given_a_thorough_model_as_best() {
    let catalog: Vec<Candidate> = curated_chat_models()
        .iter()
        .map(Candidate::from_curated)
        .collect();
    let placement = choose_placement(&processor_only(), &Companions::default_pair());
    let fits = |c: &&Candidate| judge(c, placement.as_ref()).kind == FitKind::Fits;
    assert!(
        catalog
            .iter()
            .filter(fits)
            .any(|c| c.active_parameters_b > 14.0),
        "the fixture must have a thorough model that would otherwise be offered"
    );
    let want = catalog
        .iter()
        .filter(|c| c.active_parameters_b <= 14.0)
        .filter(fits)
        .max_by_key(|c| c.rank)
        .unwrap();

    let list =
        Recommender::new(Companions::default_pair(), false).recommend(&processor_only(), &catalog);

    let by_id = |id: &str| catalog.iter().find(|c| c.id == id).unwrap();
    assert!(list
        .iter()
        .all(|r| by_id(&r.id).active_parameters_b <= 14.0));
    assert_eq!(list[0].id, want.id);
}

#[test]
fn every_curated_chat_entry_says_what_it_is_for() {
    for model in curated_chat_models() {
        assert!(!model.use_for.trim().is_empty(), "{}", model.id);
    }
}

/// Moved from the runtime's `gguf_metadata` tests with the math they check.
#[test]
fn a_header_without_key_widths_derives_them_from_the_embedding() {
    let shape = ModelShape {
        block_count: Some(32),
        head_count: Some(32.into()),
        head_count_kv: Some(32.into()),
        embedding_length: Some(4096),
        ..Default::default()
    };

    assert_eq!(shape.kv_cache_bytes(1, false), Some(32 * 32 * 256 * 2));
}

#[test]
fn an_incomplete_header_refuses_to_estimate() {
    let shape = ModelShape {
        block_count: Some(32),
        ..Default::default()
    };

    assert_eq!(shape.kv_cache_bytes(8192, false), None);
}

#[test]
fn a_per_layer_list_shorter_than_the_model_refuses_to_estimate() {
    let shape = ModelShape {
        head_count_kv: Some(PerLayer::Layers(vec![8; 10])),
        ..gemma4_12b_shape()
    };

    assert_eq!(shape.kv_cache_bytes(8192, false), None);
}

#[test]
fn a_header_without_a_feed_forward_width_still_prices_the_mask() {
    assert_eq!(
        ModelShape::default().compute_buffer_bytes(1_024, 512),
        512 * 1_024 * 6
    );
}

#[test]
fn a_qwen_hybrid_is_charged_for_its_full_attention_layers_only() {
    let shape = qwen35_9b_shape();

    assert_eq!(shape.kv_cache_bytes(1, false), Some(32 * 1024));
    assert_eq!(shape.kv_cache_bytes(32_768, false), Some(1 << 30));

    let every_layer = ModelShape {
        full_attention_interval: None,
        ..shape.clone()
    };
    assert_eq!(
        every_layer.kv_cache_bytes(32_768, false),
        shape.kv_cache_bytes(32_768, false).map(|bytes| bytes * 4)
    );
}

#[test]
fn the_mixture_of_experts_hybrid_counts_one_layer_in_four() {
    let shape = ModelShape {
        block_count: Some(40),
        embedding_length: Some(2048),
        head_count: Some(16.into()),
        head_count_kv: Some(2.into()),
        key_length: Some(256),
        value_length: Some(256),
        full_attention_interval: Some(4),
        ..Default::default()
    };

    assert_eq!(shape.kv_cache_bytes(1, false), Some(10 * 2 * 512 * 2));
    assert_eq!(shape.kv_cache_bytes(32_768, false), Some(671_088_640));
}

#[test]
fn multi_token_prediction_blocks_are_charged_as_full_attention() {
    let mtp_9b = ModelShape {
        block_count: Some(33),
        nextn_predict_layers: Some(1),
        ..qwen35_9b_shape()
    };
    let qwen38_27b = ModelShape {
        block_count: Some(65),
        nextn_predict_layers: Some(1),
        head_count: Some(24.into()),
        head_count_kv: Some(4.into()),
        key_length: Some(256),
        value_length: Some(256),
        embedding_length: Some(5120),
        full_attention_interval: Some(4),
        ..Default::default()
    };

    assert_eq!(mtp_9b.kv_cache_bytes(1, false), Some(9 * 4 * 512 * 2));
    assert_eq!(qwen38_27b.kv_cache_bytes(1, false), Some(17 * 4 * 512 * 2));
}
