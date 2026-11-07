use super::*;
use crate::domain::models::curated::{curated_chat_models, get_curated_llm_models};

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

fn weights_only(weights_bytes: u64) -> Footprint {
    Footprint {
        weights_bytes,
        kv_per_token_bytes: 0,
        windowed_kv_bytes: 0,
        recurrent_state_bytes: 0,
    }
}

fn companions() -> Companions {
    Companions {
        embedding: CompanionServer {
            footprint: Footprint {
                weights_bytes: 1_197_629_632,
                kv_per_token_bytes: 114_688,
                windowed_kv_bytes: 0,
                recurrent_state_bytes: 0,
            },
            window_tokens: 2_048,
            slots: 4,
        },
        reranker: CompanionServer {
            footprint: Footprint {
                weights_bytes: 639_153_184,
                kv_per_token_bytes: 114_688,
                windowed_kv_bytes: 0,
                recurrent_state_bytes: 0,
            },
            window_tokens: 4_096,
            slots: 1,
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
    footprint: Footprint,
    rank: u8,
) -> Candidate {
    Candidate {
        id: id.into(),
        name: id.into(),
        quantization: quantization.into(),
        quantization_bits,
        parameters_b,
        active_parameters_b: parameters_b,
        download_bytes: footprint.weights_bytes,
        footprint,
        rank,
        evidence: Evidence::Untested,
    }
}

/// Gemma 4 12B at Q4_K_M, with the footprint the catalog pins for it today.
fn gemma_12b(rank: u8) -> Candidate {
    candidate(
        "gemma-12b",
        "Q4_K_M",
        4,
        12.0,
        Footprint {
            weights_bytes: 7_121_861_440,
            kv_per_token_bytes: 8 * 1024 * 2,
            windowed_kv_bytes: 40 * 8 * 512 * 2 * (1024 + 512),
            recurrent_state_bytes: 0,
        },
        rank,
    )
}

fn ids(recommendations: &[Recommendation]) -> Vec<&str> {
    recommendations.iter().map(|r| r.id.as_str()).collect()
}

fn recommend(profile: &MachineProfile, catalog: &[Candidate]) -> Vec<Recommendation> {
    Recommender::new(companions()).recommend(profile, catalog)
}

fn recommend_alone(profile: &MachineProfile, catalog: &[Candidate]) -> Vec<Recommendation> {
    Recommender::new(Companions::none()).recommend(profile, catalog)
}

#[test]
fn rtx_4080_finds_the_27b_too_large_and_the_9b_fitting() {
    let placement = choose_placement(&rtx_4080(), &companions());
    let big = curated("qwen3.8-27b-ud-q4_k_m");
    let small = curated("qwen3.5-9b-q4_k_m");

    match fit(&big.footprint, placement.as_ref()) {
        FitVerdict::TooLarge {
            needs,
            budget,
            device,
            total_bytes,
        } => {
            assert!(needs > budget);
            assert_eq!(device, RTX_4080);
            assert_eq!(total_bytes, 16 * GIB);
            assert!(budget < total_bytes);
        }
        other => panic!("27B should be too large for 16 GiB, got {other:?}"),
    }
    match fit(&small.footprint, placement.as_ref()) {
        FitVerdict::Fits {
            needs,
            budget,
            device,
            total_bytes,
        } => {
            assert!(needs <= budget);
            assert_eq!(device, RTX_4080);
            assert_eq!(total_bytes, 16 * GIB);
            assert!(budget < total_bytes);
        }
        other => panic!("9B should fit 16 GiB, got {other:?}"),
    }
}

#[test]
fn rtx_4080_ranks_a_whole_on_the_card_12b_above_the_9b() {
    let nine = curated("qwen3.5-9b-q4_k_m");
    let twelve = gemma_12b(nine.rank + 1);
    let catalog = [nine, curated("qwen3.8-27b-ud-q4_k_m"), twelve];

    let list = recommend(&rtx_4080(), &catalog);

    assert_eq!(ids(&list), ["gemma-12b", "qwen3.5-9b-q4_k_m"]);
    assert!(list.iter().all(|r| !r.slow));
    assert!(list.iter().all(|r| r.device == RTX_4080));
    assert!(matches!(list[0].verdict, FitVerdict::Fits { .. }));
}

#[test]
fn a_dedicated_card_budget_is_its_memory_less_the_reserve_and_the_companions() {
    let placement = choose_placement(&rtx_4080(), &companions());

    assert_eq!(
        placement.budget_bytes(),
        16 * GIB - DISPLAY_RESERVE_BYTES - companions().bytes()
    );
    assert_eq!(placement.device_name(), RTX_4080);
    assert!(!placement.is_slow());
}

#[test]
fn companions_are_counted_at_their_own_window_and_slots() {
    let c = companions();

    assert_eq!(
        c.bytes(),
        c.embedding.footprint.at(2_048 * 4) + c.reranker.footprint.at(4_096)
    );
    assert_eq!(Companions::none().bytes(), 0);
}

#[test]
fn an_integrated_gpu_takes_the_companions_off_the_card() {
    let c = companions();
    let alone = choose_placement(&rtx_4080(), &c);
    let beside = choose_placement(&rtx_4080_with_integrated_gpu(), &c);

    assert!(c.bytes() > 0);
    assert_eq!(beside.budget_bytes(), 16 * GIB - DISPLAY_RESERVE_BYTES);
    assert_eq!(beside.budget_bytes() - alone.budget_bytes(), c.bytes());
    assert_eq!(beside.device_name(), RTX_4080);
    assert!(!beside.is_slow());
}

#[test]
fn a_model_that_needs_exactly_the_companions_share_fits_only_beside_an_integrated_gpu() {
    let c = companions();
    let budget_beside = choose_placement(&rtx_4080_with_integrated_gpu(), &c).budget_bytes();
    let just_fits = candidate(
        "just-fits",
        "Q4_K_M",
        4,
        12.0,
        weights_only(budget_beside - RUNTIME_OVERHEAD_BYTES),
        50,
    );

    let beside = recommend(
        &rtx_4080_with_integrated_gpu(),
        std::slice::from_ref(&just_fits),
    );
    let alone = recommend(&rtx_4080(), std::slice::from_ref(&just_fits));

    assert_eq!(ids(&beside), ["just-fits"]);
    assert!(matches!(beside[0].verdict, FitVerdict::Fits { .. }));
    assert!(alone.is_empty());
}

#[test]
fn apple_silicon_uses_the_metal_working_set_size() {
    let c = companions();
    let placement = choose_placement(&apple_silicon(), &c);

    assert_eq!(placement.budget_bytes(), 12 * GIB - c.bytes());
    assert_eq!(placement.device_name(), "Apple M2 Pro");
    assert!(!placement.is_slow());
}

#[test]
fn processor_only_is_marked_slow() {
    let profile = processor_only();
    let placement = choose_placement(&profile, &companions());

    assert!(placement.is_slow());
    assert!(placement.budget_bytes() > 0);
    assert!(placement.budget_bytes() < profile.system_ram_bytes);

    let list = recommend(&profile, &[curated("qwen3.5-9b-q4_k_m")]);
    assert_eq!(ids(&list), ["qwen3.5-9b-q4_k_m"]);
    assert!(list.iter().all(|r| r.slow));
}

#[test]
fn a_sub_4_bit_quant_of_a_model_under_20b_is_never_recommended() {
    let catalog = [
        candidate(
            "nine-iq3",
            "IQ3_S",
            3,
            9.0,
            weights_only(4_000_000_000),
            200,
        ),
        candidate(
            "nineteen-iq3",
            "IQ3_S",
            3,
            19.9,
            weights_only(9_000_000_000),
            190,
        ),
        candidate("nine-q4", "Q4_K_M", 4, 9.0, weights_only(5_000_000_000), 10),
    ];

    let list = recommend_alone(&card_machine(24 * GIB), &catalog);

    assert_eq!(ids(&list), ["nine-q4"]);
}

#[test]
fn a_sub_4_bit_quant_of_a_model_of_20b_or_more_may_be_recommended() {
    let catalog = [
        candidate(
            "twenty-iq3",
            "IQ3_S",
            3,
            20.0,
            weights_only(9_000_000_000),
            60,
        ),
        candidate(
            "twenty-seven-iq3",
            "IQ3_S",
            3,
            27.0,
            weights_only(12_000_000_000),
            80,
        ),
        candidate("nine-q4", "Q4_K_M", 4, 9.0, weights_only(5_000_000_000), 10),
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
        ..candidate(
            id,
            "Q4_K_M",
            4,
            parameters_b,
            weights_only(download_bytes),
            rank,
        )
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
    let per_token = 100_000;
    let slack = 100_000_000;
    let weights_for_tight =
        budget - RUNTIME_OVERHEAD_BYTES - per_token * MINIMUM_WINDOW_TOKENS - slack;
    let tight = candidate(
        "tight",
        "Q4_K_M",
        4,
        12.0,
        Footprint {
            weights_bytes: weights_for_tight,
            kv_per_token_bytes: per_token,
            windowed_kv_bytes: 0,
            recurrent_state_bytes: 0,
        },
        90,
    );
    let roomy = candidate("roomy", "Q4_K_M", 4, 4.0, weights_only(3_000_000_000), 10);
    let too_large = candidate(
        "too-large",
        "Q4_K_M",
        4,
        27.0,
        weights_only(40_000_000_000),
        95,
    );

    let only_tight = recommend_alone(&profile, &[tight.clone(), too_large.clone()]);
    assert_eq!(ids(&only_tight), ["tight"]);
    assert!(matches!(only_tight[0].verdict, FitVerdict::Tight { .. }));

    let both = recommend_alone(&profile, &[tight, roomy, too_large]);
    assert_eq!(ids(&both), ["roomy"]);
    assert!(matches!(both[0].verdict, FitVerdict::Fits { .. }));
}

#[test]
fn nothing_that_is_too_large_is_ever_offered() {
    let catalog = [candidate(
        "huge",
        "Q4_K_M",
        4,
        70.0,
        weights_only(40_000_000_000),
        99,
    )];

    assert!(recommend_alone(&rtx_4080(), &catalog).is_empty());
}

#[test]
fn a_footprint_adds_each_part_once_and_the_runtime_overhead() {
    let footprint = Footprint {
        weights_bytes: 5_000_000_000,
        kv_per_token_bytes: 65_536,
        windowed_kv_bytes: 300_000_000,
        recurrent_state_bytes: 50_000_000,
    };

    assert_eq!(
        footprint.at(10_000),
        5_000_000_000 + 65_536 * 10_000 + 300_000_000 + 50_000_000 + RUNTIME_OVERHEAD_BYTES
    );
}

#[test]
fn a_footprint_read_from_a_header_matches_the_pinned_catalog_values() {
    let qwen_9b = GgufHeader {
        weights_bytes: 5_680_522_464,
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
    };
    let gemma_12b_header = GgufHeader {
        weights_bytes: 7_121_861_440,
        full_attention_layers: 8,
        full_attention_kv_heads: 1,
        full_attention_key_length: 512,
        full_attention_value_length: 512,
        sliding_layers: 40,
        sliding_kv_heads: 8,
        sliding_key_length: 256,
        sliding_value_length: 256,
        sliding_window_tokens: 1_024,
        recurrent_state_bytes: 0,
    };

    assert_eq!(
        Footprint::from_header(&qwen_9b),
        Footprint {
            weights_bytes: 5_680_522_464,
            kv_per_token_bytes: 8 * 4 * 512 * 2,
            windowed_kv_bytes: 0,
            recurrent_state_bytes: 0,
        }
    );
    assert_eq!(
        Footprint::from_header(&gemma_12b_header),
        gemma_12b(0).footprint
    );
}

#[test]
fn the_one_estimator_keeps_the_catalogs_numbers_for_every_curated_entry() {
    let metadata = get_curated_llm_models();
    assert!(!metadata.is_empty());
    assert_eq!(metadata.len(), curated_chat_models().len());

    for model in curated_chat_models() {
        let entry = metadata.iter().find(|m| m.id == model.id).unwrap();
        let footprint = model.footprint();
        let gb = |window: u64| (footprint.at(window) as f64 / 100_000_000.0).ceil() / 10.0;

        assert_eq!(
            gb(MINIMUM_WINDOW_TOKENS),
            entry.minimum_ram_gb,
            "{}",
            model.id
        );
        assert_eq!(
            gb(COMFORTABLE_WINDOW_TOKENS),
            entry.recommended_ram_gb,
            "{}",
            model.id
        );
        assert_eq!(footprint.weights_bytes, model.size_bytes, "{}", model.id);
    }
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

    assert!(placement.is_slow());
    assert_eq!(placement.device_total_bytes(), 32 * GIB);
    assert_eq!(
        placement.budget_bytes(),
        choose_placement(&processor_only(), &companions()).budget_bytes()
    );
}

#[test]
fn the_default_companions_are_the_curated_embedder_and_reranker() {
    let pair = Companions::default_pair();

    assert_eq!(pair.embedding.slots, 4);
    assert_eq!(pair.embedding.window_tokens, 2_048);
    assert_eq!(pair.reranker.slots, 1);
    assert_eq!(pair.reranker.window_tokens, 4_096);
    assert!(pair.embedding.footprint.weights_bytes > 1_000_000_000);
    assert!(pair.bytes() > 3_000_000_000);
}
