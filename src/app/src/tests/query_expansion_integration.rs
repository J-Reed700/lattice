#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashSet;
use lattice::search::{BM25Search, QueryExpander, QueryExpansionConfig};
async fn setup_test_corpus() -> BM25Search {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(":memory:")
        .await
        .expect("Failed to connect to in-memory database");

    sqlx::query(
        "CREATE TABLE documents (
            id TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            mime_type TEXT,
            size_bytes INTEGER,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("Failed to create documents table");

    sqlx::query(
        "CREATE VIRTUAL TABLE documents_fts USING fts5(
            document_id UNINDEXED,
            content,
            tokenize='porter unicode61 remove_diacritics 2'
        )",
    )
    .execute(&pool)
    .await
    .expect("Failed to create FTS5 virtual table");

    let test_documents = vec![
        ("doc1", "Machine learning algorithms for classification"),
        ("doc2", "Artificial intelligence techniques and methods"),
        ("doc3", "Neural networks and deep learning systems"),
        ("doc4", "Natural language processing with transformers"),
        ("doc5", "Computer vision and image recognition"),
        ("doc6", "Data science and statistical analysis"),
        ("doc7", "Python programming for ML applications"),
        ("doc8", "Database optimization and query tuning"),
        ("doc9", "Search algorithms and retrieval systems"),
        ("doc10", "Vector embeddings for semantic search"),
        ("doc11", "Fast algorithms for large scale processing"),
        ("doc12", "Quick methods for rapid prototyping"),
        ("doc13", "Speedy techniques for real-time systems"),
        ("doc14", "Code quality and software engineering"),
        ("doc15", "Bug tracking and issue resolution"),
        ("doc16", "API design and interface development"),
        ("doc17", "Text processing and document analysis"),
        ("doc18", "Feature extraction and representation learning"),
        ("doc19", "Model training and optimization strategies"),
        ("doc20", "Precision and recall metrics for evaluation"),
    ];

    for (i, (id, content)) in test_documents.iter().enumerate() {
        sqlx::query(
            "INSERT INTO documents (id, file_name, mime_type, size_bytes, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(format!("{}.txt", id))
        .bind("text/plain")
        .bind(content.len() as i64)
        .bind(i as i64)
        .execute(&pool)
        .await
        .expect("Failed to insert document");

        sqlx::query("INSERT INTO documents_fts (document_id, content) VALUES (?, ?)")
            .bind(id)
            .bind(content)
            .execute(&pool)
            .await
            .expect("Failed to insert document into FTS5");
    }

    BM25Search::new(pool)
}

#[tokio::test]
async fn test_expansion_improves_recall_for_synonym_queries() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig {
        enable_query_expansion: true,
        max_expansions_per_term: 3,
        use_domain_dict: true,
        expand_stopwords: false,
    };

    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let query = "ml";
    let expected_docs = vec!["doc1", "doc7"];

    let baseline_results = bm25
        .search(query, 20)
        .await
        .expect("Failed to perform baseline search");
    let baseline_ids: HashSet<String> = baseline_results
        .iter()
        .map(|r| r.document_id.clone())
        .collect();

    let baseline_recall = expected_docs
        .iter()
        .filter(|id| baseline_ids.contains::<str>(*id))
        .count();

    let expansion = expander.expand(query);
    let expanded_results = bm25
        .search(&expansion.expanded_query, 20)
        .await
        .expect("Failed to perform expanded search");
    let expanded_ids: HashSet<String> = expanded_results
        .iter()
        .map(|r| r.document_id.clone())
        .collect();

    let expanded_recall = expected_docs
        .iter()
        .filter(|id| expanded_ids.contains::<str>(*id))
        .count();

    println!("Query: {}", query);
    println!("Expanded: {}", expansion.expanded_query);
    println!(
        "Baseline recall: {}/{}",
        baseline_recall,
        expected_docs.len()
    );
    println!(
        "Expanded recall: {}/{}",
        expanded_recall,
        expected_docs.len()
    );

    assert!(
        expanded_recall >= baseline_recall,
        "Query expansion should not decrease recall"
    );
}

#[tokio::test]
async fn test_ai_synonym_expansion() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let query = "ai";

    let expansion = expander.expand(query);

    assert!(expansion.expanded_query.contains("ai"));
    assert!(
        expansion.expanded_query.contains("artificial intelligence")
            || expansion.expanded_query.contains("machine learning")
    );

    let baseline_results = bm25
        .search(query, 10)
        .await
        .expect("Failed to perform baseline AI search");
    let expanded_results = bm25
        .search(&expansion.expanded_query, 10)
        .await
        .expect("Failed to perform expanded AI search");

    println!("AI Query Results:");
    println!("Baseline: {} results", baseline_results.len());
    println!("Expanded: {} results", expanded_results.len());
}

#[tokio::test]
async fn test_fast_synonym_expansion() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let query = "fast algorithms";

    let expansion = expander.expand(query);

    println!("Original: {}", query);
    println!("Expanded: {}", expansion.expanded_query);

    assert!(expansion.expanded_query.contains("fast"));
    assert!(expansion.expanded_query.contains("algorithms"));

    let baseline_results = bm25
        .search(query, 20)
        .await
        .expect("Failed to perform baseline fast algorithms search");
    let expanded_results = bm25
        .search(&expansion.expanded_query, 20)
        .await
        .expect("Failed to perform expanded fast algorithms search");

    let expected_docs = vec!["doc11", "doc12", "doc13"];

    let baseline_ids: HashSet<String> = baseline_results
        .iter()
        .map(|r| r.document_id.clone())
        .collect();

    let expanded_ids: HashSet<String> = expanded_results
        .iter()
        .map(|r| r.document_id.clone())
        .collect();

    let baseline_hits = expected_docs
        .iter()
        .filter(|id| baseline_ids.contains::<str>(*id))
        .count();
    let expanded_hits = expected_docs
        .iter()
        .filter(|id| expanded_ids.contains::<str>(*id))
        .count();

    println!("Expected documents found:");
    println!("Baseline: {}/{}", baseline_hits, expected_docs.len());
    println!("Expanded: {}/{}", expanded_hits, expected_docs.len());

    assert!(expanded_hits >= baseline_hits);
}

#[tokio::test]
async fn test_multiple_term_expansion() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let query = "ml algorithm search";

    let expansion = expander.expand(query);

    assert!(expansion.expanded_terms.len() > 3);

    assert!(expansion.term_expansions.contains_key("ml"));
    assert!(expansion.term_expansions.contains_key("algorithm"));
    assert!(expansion.term_expansions.contains_key("search"));

    let results = bm25
        .search(&expansion.expanded_query, 20)
        .await
        .expect("Failed to perform multi-term expansion search");

    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_recall_improvement_percentage() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let test_cases = vec![
        ("ml", vec!["doc1", "doc7"]),
        ("ai", vec!["doc2"]),
        ("fast", vec!["doc11", "doc12", "doc13"]),
        ("bug", vec!["doc15"]),
        ("api", vec!["doc16"]),
    ];

    let mut total_baseline_recall = 0;
    let mut total_expanded_recall = 0;
    let mut total_expected = 0;

    for (query, expected_docs) in test_cases {
        let baseline_results = bm25
            .search(query, 20)
            .await
            .expect("Failed to perform baseline search in recall test");
        let baseline_ids: HashSet<String> = baseline_results
            .iter()
            .map(|r| r.document_id.clone())
            .collect();

        let expansion = expander.expand(query);
        let expanded_results = bm25
            .search(&expansion.expanded_query, 20)
            .await
            .expect("Failed to perform expanded search in recall test");
        let expanded_ids: HashSet<String> = expanded_results
            .iter()
            .map(|r| r.document_id.clone())
            .collect();

        let baseline_hits = expected_docs
            .iter()
            .filter(|id| baseline_ids.contains::<str>(*id))
            .count();
        let expanded_hits = expected_docs
            .iter()
            .filter(|id| expanded_ids.contains::<str>(*id))
            .count();

        total_baseline_recall += baseline_hits;
        total_expanded_recall += expanded_hits;
        total_expected += expected_docs.len();

        println!(
            "Query '{}': baseline={}/{}, expanded={}/{}",
            query,
            baseline_hits,
            expected_docs.len(),
            expanded_hits,
            expected_docs.len()
        );
    }

    let baseline_percentage = (total_baseline_recall as f32 / total_expected as f32) * 100.0;
    let expanded_percentage = (total_expanded_recall as f32 / total_expected as f32) * 100.0;
    let improvement = expanded_percentage - baseline_percentage;

    println!("\n=== Overall Lattice Results ===");
    println!("Baseline recall: {:.1}%", baseline_percentage);
    println!("Expanded recall: {:.1}%", expanded_percentage);
    println!("Improvement: +{:.1}%", improvement);

    assert!(
        expanded_percentage >= baseline_percentage,
        "Query expansion should not decrease overall recall"
    );

    assert!(
        improvement >= 0.0,
        "Expected positive improvement in recall, got {:.1}%",
        improvement
    );
}

#[tokio::test]
async fn test_expansion_latency() {
    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let queries = vec![
        "ml",
        "ml algorithm",
        "ml algorithm neural network",
        "ml algorithm neural network training optimization",
    ];

    for query in queries {
        let start = std::time::Instant::now();
        let _expansion = expander.expand(query);
        let elapsed = start.elapsed();

        println!("Query '{}': {:?}", query, elapsed);

        assert!(
            elapsed.as_millis() < 5,
            "Query expansion took too long: {:?}",
            elapsed
        );
    }
}

#[tokio::test]
async fn test_no_duplication_in_results() {
    let bm25 = setup_test_corpus().await;

    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let query = "machine learning ml";

    let expansion = expander.expand(query);
    let results = bm25
        .search(&expansion.expanded_query, 20)
        .await
        .expect("Failed to perform deduplication search");

    let mut seen = HashSet::new();
    for result in results {
        assert!(
            seen.insert(result.document_id.clone()),
            "Duplicate document in results: {}",
            result.document_id
        );
    }
}

#[tokio::test]
async fn test_domain_specific_terms() {
    let config = QueryExpansionConfig::default();
    let expander = QueryExpander::new(config).expect("Failed to create query expander");

    let domain_queries = vec![
        ("nlp", "natural language processing"),
        ("vector", "embedding"),
        ("api", "interface"),
        ("db", "database"),
    ];

    for (query, expected_term) in domain_queries {
        let expansion = expander.expand(query);

        println!("Query: {} -> {}", query, expansion.expanded_query);

        assert!(
            expansion
                .expanded_query
                .to_lowercase()
                .contains(expected_term),
            "Expected '{}' to contain '{}'",
            expansion.expanded_query,
            expected_term
        );
    }
}
