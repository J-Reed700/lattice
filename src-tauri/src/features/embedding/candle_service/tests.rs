//! Tests for model loading, pooling, prefixes and late chunking on a tiny BERT.

use super::config::{read_pooling_strategy, resolve_prefixes};
use super::*;
use crate::application::ports::EmbeddingPort;
use crate::domain::models::checkpoint::WEIGHTS_PYTORCH_BIN;
use crate::domain::models::checkpoint::WEIGHTS_SAFETENSORS;
use crate::features::download::artifact_identity;
use crate::features::embedding::late_chunking::vector_identity;
use candle_core::DType;
use candle_core::Tensor;
use std::path::Path;
use tempfile::TempDir;

/// A one-layer, four-wide BERT: config, tokenizer and the weight shapes.
/// `write_tiny_bert` fills them with zeros — enough for the loader to
/// accept, never run.
fn write_tiny_bert_files(dir: &Path) -> (Vec<(String, Vec<usize>)>, PathBuf) {
    std::fs::write(
        dir.join("config.json"),
        r#"{"model_type":"bert","vocab_size":2,"hidden_size":4,"num_hidden_layers":1,
                "num_attention_heads":1,"intermediate_size":8,"hidden_act":"gelu",
                "hidden_dropout_prob":0.0,"max_position_embeddings":8,"type_vocab_size":1,
                "initializer_range":0.02,"layer_norm_eps":1e-12,"pad_token_id":0}"#,
    )
    .unwrap();
    let vocab = [("[PAD]".to_owned(), 0), ("[UNK]".to_owned(), 1)]
        .into_iter()
        .collect();
    Tokenizer::new(
        tokenizers::models::wordlevel::WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .unwrap(),
    )
    .save(dir.join("tokenizer.json"), false)
    .unwrap();

    let layer = "encoder.layer.0";
    let shapes: Vec<(String, Vec<usize>)> = vec![
        ("embeddings.word_embeddings.weight".into(), vec![2, 4]),
        ("embeddings.position_embeddings.weight".into(), vec![8, 4]),
        ("embeddings.token_type_embeddings.weight".into(), vec![1, 4]),
        ("embeddings.LayerNorm.weight".into(), vec![4]),
        ("embeddings.LayerNorm.bias".into(), vec![4]),
        (format!("{layer}.attention.self.query.weight"), vec![4, 4]),
        (format!("{layer}.attention.self.query.bias"), vec![4]),
        (format!("{layer}.attention.self.key.weight"), vec![4, 4]),
        (format!("{layer}.attention.self.key.bias"), vec![4]),
        (format!("{layer}.attention.self.value.weight"), vec![4, 4]),
        (format!("{layer}.attention.self.value.bias"), vec![4]),
        (format!("{layer}.attention.output.dense.weight"), vec![4, 4]),
        (format!("{layer}.attention.output.dense.bias"), vec![4]),
        (
            format!("{layer}.attention.output.LayerNorm.weight"),
            vec![4],
        ),
        (format!("{layer}.attention.output.LayerNorm.bias"), vec![4]),
        (format!("{layer}.intermediate.dense.weight"), vec![8, 4]),
        (format!("{layer}.intermediate.dense.bias"), vec![8]),
        (format!("{layer}.output.dense.weight"), vec![4, 8]),
        (format!("{layer}.output.dense.bias"), vec![4]),
        (format!("{layer}.output.LayerNorm.weight"), vec![4]),
        (format!("{layer}.output.LayerNorm.bias"), vec![4]),
    ];
    (shapes, dir.join(WEIGHTS_SAFETENSORS))
}

pub(crate) fn write_tiny_bert(dir: &Path) {
    let (shapes, path) = write_tiny_bert_files(dir);
    let tensors: std::collections::HashMap<String, Tensor> = shapes
        .into_iter()
        .map(|(name, shape)| {
            let tensor = Tensor::zeros(shape, DType::F32, &Device::Cpu).unwrap();
            (name, tensor)
        })
        .collect();
    candle_core::safetensors::save(&tensors, path).unwrap();
}

/// The tiny BERT with weights that make every position's hidden state
/// different, and a whitespace tokenizer, so CLS and mean pooling of the
/// same input give different vectors. Attention and the feed-forward are
/// zero, so each output row is the layer-normed sum of its word and
/// position embeddings.
fn write_runnable_tiny_bert(dir: &Path) {
    let (shapes, path) = write_tiny_bert_files(dir);
    let tensors: std::collections::HashMap<String, Tensor> = shapes
        .into_iter()
        .map(|(name, shape)| {
            let len: usize = shape.iter().product();
            let values: Vec<f32> = if name.ends_with("LayerNorm.weight") {
                vec![1.0; len]
            } else if name.starts_with("embeddings.") && name.ends_with("embeddings.weight") {
                (0..len)
                    .map(|i| ((i * i + 1) as f32 * 0.37).sin())
                    .collect()
            } else {
                vec![0.0; len]
            };
            let tensor = Tensor::from_vec(values, shape, &Device::Cpu).unwrap();
            (name, tensor)
        })
        .collect();
    candle_core::safetensors::save(&tensors, path).unwrap();
    let vocab = [("[PAD]".to_owned(), 0), ("[UNK]".to_owned(), 1)]
        .into_iter()
        .collect();
    let mut tokenizer = Tokenizer::new(
        tokenizers::models::wordlevel::WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .unwrap(),
    );
    tokenizer.with_pre_tokenizer(tokenizers::pre_tokenizers::whitespace::Whitespace);
    tokenizer.save(dir.join("tokenizer.json"), false).unwrap();
}

#[tokio::test]
async fn late_chunking_pools_queries_and_fallback_chunks_like_late_chunks() {
    let dir = TempDir::new().unwrap();
    write_runnable_tiny_bert(dir.path());
    let identity = ArtifactIdentity::from_digest(&[7; 32]);
    let chunk_first = CandleEmbeddingService::open(dir.path(), identity.clone()).unwrap();
    assert_eq!(chunk_first.pooling, PoolingStrategy::Cls);
    let late = CandleEmbeddingService::open(dir.path(), identity)
        .unwrap()
        .with_strategy(EmbeddingStrategy::LateChunking);

    let text = "one two three four";
    let whole = 0..text.len();
    let late_chunked = late
        .embed_late_chunked(text, std::slice::from_ref(&whole))
        .await
        .unwrap();
    let query = EmbeddingPort::embed_query(&late, text).await.unwrap();
    let cls = EmbeddingPort::embed_query(&chunk_first, text)
        .await
        .unwrap();
    let close = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
    // Without the fix the query came from CLS pooling: a different space.
    assert!(!close(&cls, &late_chunked[0]));
    assert!(close(&query, &late_chunked[0]));

    // A chunk embedded on its own pools its own tokens, not the prefix.
    let span = "p q\n\none two three";
    let ranges = [5..8, 8..18];
    let late_pair = late.embed_late_chunked(span, &ranges).await.unwrap();
    let alone = late.embed_chunks_individually(span, &ranges).await.unwrap();
    let expected = late
        .embed_late_chunked("p q\n\n two three", std::slice::from_ref(&(5..15)))
        .await
        .unwrap();
    let batch_cls = EmbeddingPort::embed_batch(&chunk_first, &["p q\n\n two three".into()])
        .await
        .unwrap();
    assert_eq!(alone.len(), 2);
    assert!(close(&alone[1], &expected[0]));
    assert!(!close(&alone[1], &batch_cls[0]));
    assert_eq!(late_pair.len(), 2);
}

#[test]
fn open_labels_the_service_with_the_identity_it_was_given() {
    let dir = TempDir::new().unwrap();
    write_tiny_bert(dir.path());
    // Deliberately not the digest of these files: `open` takes the stored
    // identity as given and never rehashes.
    let stored = ArtifactIdentity::from_digest(&[7; 32]);
    assert_ne!(stored, artifact_identity::compute(dir.path()).unwrap());

    let service = CandleEmbeddingService::open(dir.path(), stored.clone()).unwrap();
    assert_eq!(service.embedding_strategy(), EmbeddingStrategy::ChunkFirst);
    assert_eq!(EmbeddingPort::model_identity(&service), stored.as_str());

    let late = service.with_strategy(EmbeddingStrategy::LateChunking);
    assert_eq!(
        EmbeddingPort::model_identity(&late),
        crate::features::embedding::late_chunking::strategy_identity(
            stored.as_str(),
            EmbeddingStrategy::LateChunking
        )
    );
}

#[test]
fn a_document_prefix_joins_the_identity_search_binds_before_loading() {
    let root = TempDir::new().unwrap();
    // No `_name_or_path` in the tiny config, so the directory name picks E5.
    let dir = root.path().join("multilingual-e5-small");
    std::fs::create_dir(&dir).unwrap();
    write_tiny_bert(&dir);
    let stored = ArtifactIdentity::from_digest(&[7; 32]);
    let prefixes = prefixes_for_model_dir(&dir);
    assert_eq!(prefixes.id, "e5");

    for strategy in [
        EmbeddingStrategy::ChunkFirst,
        EmbeddingStrategy::LateChunking,
    ] {
        let service = CandleEmbeddingService::open(&dir, stored.clone())
            .unwrap()
            .with_strategy(strategy);
        let identity = EmbeddingPort::model_identity(&service);
        assert!(identity.ends_with("+prefix-e5"), "{identity}");
        assert_eq!(
            identity,
            vector_identity(stored.as_str(), strategy, prefixes)
        );
    }
}

#[test]
fn open_unregistered_labels_the_service_with_the_computed_identity() {
    let dir = TempDir::new().unwrap();
    write_tiny_bert(dir.path());
    let service = CandleEmbeddingService::open_unregistered(dir.path()).unwrap();
    assert_eq!(
        EmbeddingPort::model_identity(&service),
        artifact_identity::compute(dir.path()).unwrap().as_str()
    );
}

#[test]
fn pooling_defaults_to_cls_when_no_hints() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"bert","hidden_size":768}"#,
    )
    .unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Cls);
}

#[test]
fn pooling_reads_explicit_1_pooling_config() {
    let dir = TempDir::new().unwrap();
    let pooling_dir = dir.path().join("1_Pooling");
    std::fs::create_dir(&pooling_dir).unwrap();
    std::fs::write(
        pooling_dir.join("config.json"),
        r#"{"pooling_mode_mean_tokens":true}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"bert","hidden_size":384}"#,
    )
    .unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Mean);
}

#[test]
fn pooling_infers_mean_for_sentence_transformers() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"bert","hidden_size":384,"_name_or_path":"sentence-transformers/all-MiniLM-L6-v2"}"#,
    )
    .unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Mean);
}

#[test]
fn pooling_keeps_cls_for_bge() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"bert","hidden_size":1024,"_name_or_path":"BAAI/bge-m3"}"#,
    )
    .unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Cls);
}

#[test]
fn missing_weights_error_names_both_candidates() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"bert","hidden_size":8}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), "{}").unwrap();

    let identity = ArtifactIdentity::from_digest(&[0; 32]);
    let message = match CandleEmbeddingService::open(dir.path(), identity) {
        Ok(_) => panic!("a directory with no weights must not load"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("model.safetensors"), "{message}");
    assert!(message.contains("pytorch_model.bin"), "{message}");
    assert!(
        message.contains(&dir.path().display().to_string()),
        "{message}"
    );
}

#[test]
fn length_buckets_stay_inside_the_padded_token_budget() {
    let dir = TempDir::new().unwrap();
    write_tiny_bert(dir.path());
    let service =
        CandleEmbeddingService::open(dir.path(), ArtifactIdentity::from_digest(&[1; 32])).unwrap();

    // The tiny tokenizer is whitespace-free word-level, so one "word" per
    // text: length is the word count, plus nothing (no special tokens).
    let texts: Vec<String> = [1usize, 900, 3, 400, 2, 1200, 5]
        .iter()
        .map(|words| vec!["word"; *words].join(" "))
        .collect();
    let batches = service.plan_batches(&texts).unwrap();

    let mut seen: Vec<usize> = batches.iter().flatten().copied().collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        (0..texts.len()).collect::<Vec<_>>(),
        "every input lands in exactly one batch"
    );
    for batch in &batches {
        assert!(batch.len() <= MAX_BATCH_ROWS);
        let longest = batch
            .iter()
            .map(|&index| service.input_policy.count(&texts[index]).unwrap())
            .max()
            .unwrap();
        assert!(
            batch.len() == 1 || longest * batch.len() <= ENCODER_PADDED_TOKEN_BUDGET,
            "batch of {} rows × {longest} tokens exceeds the budget",
            batch.len()
        );
    }
    // Short texts share a batch; the 1200-token one is too wide to share.
    assert!(batches.iter().any(|batch| batch.len() > 1));
}

#[test]
fn qwen3_falls_back_to_its_instruction_when_the_directory_says_nothing() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        resolve_prefixes(dir.path(), b"{}", ModelArchitecture::Qwen3).id,
        "qwen3-instruct"
    );
    assert_eq!(
        resolve_prefixes(dir.path(), b"{}", ModelArchitecture::Bert).id,
        "none"
    );
    // A recorded name wins over the directory it happens to sit in.
    assert_eq!(
        resolve_prefixes(
            dir.path(),
            br#"{"_name_or_path":"intfloat/e5-base-v2"}"#,
            ModelArchitecture::Bert
        )
        .id,
        "e5"
    );
}

#[test]
fn a_document_prefix_joins_the_identity_and_a_query_only_one_does_not() {
    let dir = TempDir::new().unwrap();
    write_tiny_bert(dir.path());
    let stored = ArtifactIdentity::from_digest(&[3; 32]);
    let mut service = CandleEmbeddingService::open(dir.path(), stored.clone()).unwrap();

    assert_eq!(EmbeddingPort::model_identity(&service), stored.as_str());
    service.prefixes = &super::super::prefixes::QWEN3_INSTRUCT;
    assert_eq!(
        EmbeddingPort::model_identity(&service),
        stored.as_str(),
        "a query-side instruction leaves stored vectors untouched"
    );
    service.prefixes = &super::super::prefixes::E5;
    assert_eq!(
        EmbeddingPort::model_identity(&service),
        format!("{}+prefix-e5", stored.as_str())
    );
}

#[test]
fn token_limit_and_pooling_do_not_depend_on_the_weights_file_name() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"model_type":"xlm-roberta","hidden_size":1024,"max_position_embeddings":8194}"#,
    )
    .unwrap();
    let pooling = read_pooling_strategy(dir.path());
    let limit = super::super::input_policy::model_token_limit(dir.path()).unwrap();

    std::fs::write(dir.path().join(WEIGHTS_PYTORCH_BIN), b"pickle").unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), pooling);
    assert_eq!(
        super::super::input_policy::model_token_limit(dir.path()).unwrap(),
        limit
    );

    std::fs::write(dir.path().join(WEIGHTS_SAFETENSORS), b"tensors").unwrap();
    assert_eq!(read_pooling_strategy(dir.path()), pooling);
    assert_eq!(
        super::super::input_policy::model_token_limit(dir.path()).unwrap(),
        limit
    );
}
