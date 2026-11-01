//! Tests for rank fusion, admission, model resolution and loader dispatch.

use super::inference::sigmoid;
use super::loading::{detect_reranker_architecture, RerankerArchitecture, RerankerModelConfig};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_waiter_does_not_start_blocking_inference_and_running_job_keeps_slot() {
    let admission = Arc::new(tokio::sync::Semaphore::new(1));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first = {
        let admission = Arc::clone(&admission);
        tokio::spawn(async move {
            run_admitted(admission, move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .await
        })
    };
    started_rx.recv().unwrap();

    let cancelled_started = Arc::new(AtomicBool::new(false));
    let waiting = {
        let admission = Arc::clone(&admission);
        let started = Arc::clone(&cancelled_started);
        tokio::spawn(async move {
            run_admitted(admission, move || {
                started.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
        })
    };
    waiting.abort();
    let _ = waiting.await;

    let second_started = Arc::new(AtomicBool::new(false));
    let second = {
        let admission = Arc::clone(&admission);
        let started = Arc::clone(&second_started);
        tokio::spawn(async move {
            run_admitted(admission, move || {
                started.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(!cancelled_started.load(Ordering::SeqCst));
    assert!(!second_started.load(Ordering::SeqCst));
    release_tx.send(()).unwrap();
    first.abort();
    let _ = first.await;
    second.await.unwrap().unwrap();
    assert!(second_started.load(Ordering::SeqCst));
}

#[test]
fn test_rerank_result_ordering() {
    // Sanity: sort descending by score.
    let mut results = [
        RerankResult {
            index: 0,
            score: 0.5,
        },
        RerankResult {
            index: 1,
            score: 0.9,
        },
        RerankResult {
            index: 2,
            score: 0.3,
        },
    ];
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assert_eq!(results[0].index, 1);
    assert_eq!(results[1].index, 0);
    assert_eq!(results[2].index, 2);
}

fn scored(scores: &[f32]) -> Vec<RerankResult> {
    scores
        .iter()
        .enumerate()
        .map(|(index, score)| RerankResult {
            index,
            score: *score,
        })
        .collect()
}

fn order(fused: &[FusedCandidate]) -> Vec<usize> {
    fused.iter().map(|candidate| candidate.index).collect()
}

#[test]
fn fusion_maps_relevance_back_to_original_candidates() {
    let reranked = vec![
        RerankResult {
            index: 1,
            score: 0.9,
        },
        RerankResult {
            index: 0,
            score: 0.2,
        },
        RerankResult {
            index: 2,
            score: 0.4,
        },
    ];

    let fused = fuse_rerank_order(3, &reranked).unwrap();

    // 1/61 + 1/63 for the first, 1/62 + 1/61 for the second: the second
    // leads, and each keeps its own relevance.
    assert_eq!(order(&fused), [1, 0, 2]);
    assert_eq!(fused[0].relevance, 0.9);
    assert_eq!(fused[1].relevance, 0.2);
}

/// A passage the cross-encoder ranks first climbs only as far as the
/// fusion lets it: it does not leap a head the first stage agreed on.
#[test]
fn fusion_weighs_both_orders_by_rank_at_k_sixty() {
    let mut scores = vec![0.5_f32; 30];
    scores[29] = 1.0;
    let fused = fuse_rerank_order(30, &scored(&scores)).unwrap();

    // The last candidate's 1/90 + 1/61 = 0.02750 falls between the twelfth
    // (1/72 + 1/73 = 0.02759) and the thirteenth (1/73 + 1/74 = 0.02721).
    let place = order(&fused).iter().position(|&index| index == 29);
    assert_eq!(place, Some(12));
    assert_eq!(order(&fused)[0], 0);
}

#[test]
fn fusion_ties_keep_the_fused_order_and_scores_are_clamped() {
    // First by one order and second by the other is a tie.
    let swapped = fuse_rerank_order(2, &scored(&[0.0, 1.0])).unwrap();
    assert_eq!(order(&swapped), [0, 1]);

    let fused = fuse_rerank_order(3, &scored(&[1.4, 0.5, -0.3])).unwrap();
    assert_eq!(order(&fused), [0, 1, 2]);
    let relevance: Vec<f32> = fused.iter().map(|candidate| candidate.relevance).collect();
    assert_eq!(relevance, [1.0, 0.5, 0.0]);
}

#[test]
fn fusion_of_nothing_is_nothing() {
    assert!(fuse_rerank_order(0, &[]).unwrap().is_empty());
}

#[test]
fn fusion_rejects_duplicate_indices() {
    let reranked = vec![
        RerankResult {
            index: 0,
            score: 0.9,
        },
        RerankResult {
            index: 0,
            score: 0.2,
        },
    ];

    assert!(fuse_rerank_order(2, &reranked).is_err());
}

#[test]
fn fusion_rejects_incomplete_out_of_range_or_non_finite_output() {
    let incomplete = vec![RerankResult {
        index: 0,
        score: 0.9,
    }];
    assert!(fuse_rerank_order(2, &incomplete).is_err());

    let out_of_range = vec![RerankResult {
        index: 1,
        score: 0.9,
    }];
    assert!(fuse_rerank_order(1, &out_of_range).is_err());

    let non_finite = vec![RerankResult {
        index: 0,
        score: f32::NAN,
    }];
    assert!(fuse_rerank_order(1, &non_finite).is_err());
}

#[test]
fn loader_distinguishes_bert_and_qwen3_causal_rerankers() {
    let bert = RerankerModelConfig {
        model_type: "bert".to_string(),
        architectures: vec!["BertForSequenceClassification".to_string()],
    };
    assert_eq!(
        detect_reranker_architecture(&bert).unwrap(),
        RerankerArchitecture::BertSequenceClassifier
    );

    let qwen = RerankerModelConfig {
        model_type: "qwen3".to_string(),
        architectures: vec!["Qwen3ForCausalLM".to_string()],
    };
    assert_eq!(
        detect_reranker_architecture(&qwen).unwrap(),
        RerankerArchitecture::Qwen3CausalClassifier
    );
}

#[test]
fn loader_rejects_qwen3_embedding_checkpoint() {
    let embedding = RerankerModelConfig {
        model_type: "qwen3".to_string(),
        architectures: vec!["Qwen3Model".to_string()],
    };
    assert!(detect_reranker_architecture(&embedding).is_err());
}

#[test]
fn lazy_reranker_notices_artifacts_added_after_startup() {
    let tmp = tempfile::tempdir().unwrap();
    let model_path = tmp.path().join("model.safetensors");
    let reranker = LazyReranker::new(&model_path);
    assert!(!reranker.is_available());

    std::fs::write(&model_path, b"weights").unwrap();
    std::fs::write(tmp.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::write(tmp.path().join("config.json"), b"{}").unwrap();

    assert!(reranker.is_available());
}

#[test]
fn resolve_model_dir_accepts_directory() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("model.safetensors"), b"").unwrap();
    let resolved = resolve_model_dir(tmp.path()).unwrap();
    assert_eq!(resolved.dir, tmp.path());
    assert_eq!(resolved.weights, tmp.path().join("model.safetensors"));
}

#[test]
fn resolve_model_dir_accepts_safetensors_file() {
    let tmp = tempfile::tempdir().unwrap();
    let weights = tmp.path().join("model.safetensors");
    std::fs::write(&weights, b"").unwrap();
    let resolved = resolve_model_dir(&weights).unwrap();
    assert_eq!(resolved.dir, tmp.path());
    assert_eq!(resolved.weights, weights);
}

#[test]
fn resolve_model_dir_redirects_legacy_onnx_path() {
    // Old callers pass `reranker/model.onnx`. We treat that as
    // "look for model.safetensors in the same directory."
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("model.safetensors"), b"").unwrap();
    let onnx_path = tmp.path().join("model.onnx");
    // model.onnx itself doesn't need to exist; we redirect to the
    // safetensors sibling regardless.
    let resolved = resolve_model_dir(&onnx_path).unwrap();
    assert_eq!(resolved.weights, tmp.path().join("model.safetensors"));
}

#[test]
fn resolve_model_dir_errors_when_weights_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let result = resolve_model_dir(tmp.path());
    assert!(result.is_err());
}

#[test]
fn sigmoid_maps_correctly() {
    assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
    assert!(sigmoid(10.0) > 0.999);
    assert!(sigmoid(-10.0) < 0.001);
    // Monotonic on a sample of values.
    let xs = [-3.0, -1.0, 0.0, 1.0, 3.0];
    let ys: Vec<f32> = xs.iter().copied().map(sigmoid).collect();
    for w in ys.windows(2) {
        assert!(w[0] < w[1], "sigmoid not monotonic: {:?}", ys);
    }
}
