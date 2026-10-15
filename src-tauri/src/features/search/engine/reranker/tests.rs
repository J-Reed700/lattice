//! Tests for score blending, admission, model resolution and loader dispatch.

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

#[test]
fn shared_blend_maps_scores_back_to_original_candidates() {
    let reranked = vec![
        RerankResult {
            index: 1,
            score: 0.9,
        },
        RerankResult {
            index: 0,
            score: 0.2,
        },
    ];

    let scores = blend_rerank_scores(&[1.0, 0.5], &reranked).unwrap();

    assert!((scores[0] - 0.48).abs() < 0.0001);
    assert!((scores[1] - 0.76).abs() < 0.0001);
}

#[test]
fn shared_blend_rejects_duplicate_indices() {
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

    assert!(blend_rerank_scores(&[1.0, 0.5], &reranked).is_err());
}

#[test]
fn shared_blend_rejects_incomplete_or_non_finite_output() {
    let incomplete = vec![RerankResult {
        index: 0,
        score: 0.9,
    }];
    assert!(blend_rerank_scores(&[1.0, 0.5], &incomplete).is_err());

    let non_finite = vec![RerankResult {
        index: 0,
        score: f32::NAN,
    }];
    assert!(blend_rerank_scores(&[1.0], &non_finite).is_err());
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
