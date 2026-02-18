#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use vault::domain::aggregates::Document;
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use vault::infrastructure::persistence::repositories::DocumentRepository;

pub async fn create_test_documents(doc_repo: &DocumentRepository) -> (String, Vec<Document>) {
    let documents = vec![
        create_document(
            "test-doc-1",
            "/test/documents/machine-learning.md",
            "machine-learning.md",
            "markdown",
            2500,
        ),
        create_document(
            "test-doc-2",
            "/test/documents/rust-programming.md",
            "rust-programming.md",
            "markdown",
            1800,
        ),
        create_document(
            "test-doc-3",
            "/test/documents/systems-design.md",
            "systems-design.md",
            "markdown",
            3200,
        ),
    ];

    let first_doc_id = documents[0].id.clone();

    for doc in &documents {
        let mut tx = doc_repo.pool.begin().await.unwrap();
        sqlx::query!(
            r#"
            INSERT INTO documents (id, vault_id, file_path, file_name, file_type, file_size, indexed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            doc.id,
            doc.vault_id,
            doc.file_path,
            doc.file_name,
            doc.file_type,
            doc.file_size,
            doc.indexed_at
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    (first_doc_id, documents)
}

fn create_document(
    id: &str,
    file_path: &str,
    file_name: &str,
    file_type: &str,
    file_size: i64,
) -> Document {
    use chrono::Utc;

    Document {
        id: id.to_string(),
        vault_id: "test-vault".to_string(),
        file_path: file_path.to_string(),
        file_name: file_name.to_string(),
        file_type: file_type.to_string(),
        file_size,
        content_hash: None,
        indexed_at: Utc::now().to_rfc3339(),
        updated_at: Some(Utc::now().to_rfc3339()),
        metadata: None,
    }
}

pub fn get_sample_content(doc_type: &str) -> String {
    match doc_type {
        "machine-learning" => r#"# Machine Learning Fundamentals

Machine learning is a subset of artificial intelligence that focuses on building systems
that learn from data. Key concepts include:

- Supervised Learning: Training with labeled data
- Unsupervised Learning: Finding patterns in unlabeled data
- Neural Networks: Models inspired by biological neurons
- Deep Learning: Multi-layer neural networks

## Applications

Machine learning is used in:
- Computer vision
- Natural language processing
- Recommendation systems
- Anomaly detection

References:
- @andrew-ng's course on ML
- [[neural-networks]] architecture
- Research by @geoffrey-hinton

#ai #machine-learning #deep-learning
"#
        .to_string(),
        "rust-programming" => r#"# Rust Programming Language

Rust is a systems programming language focused on safety, speed, and concurrency.

## Key Features

- Memory safety without garbage collection
- Ownership system
- Zero-cost abstractions
- Thread safety

## Use Cases

Rust excels at:
- Systems programming
- WebAssembly
- Embedded systems
- Performance-critical applications

See also:
- [[systems-programming]]
- @steve-klabnik's Rust book
- [[memory-management]]

#rust #programming #systems
"#
        .to_string(),
        "systems-design" => r#"# Systems Design Principles

Good systems design requires careful consideration of:

## Scalability

- Horizontal vs Vertical scaling
- Load balancing
- Caching strategies

## Reliability

- Fault tolerance
- Redundancy
- Monitoring

## Performance

- Latency optimization
- Throughput maximization
- Resource efficiency

Discussed with @martin-fowler and @robert-martin.

Related topics:
- [[distributed-systems]]
- [[microservices]]
- [[database-design]]

#design #systems #architecture
"#
        .to_string(),
        _ => "Default test content".to_string(),
    }
}

pub fn create_content_with_mentions(person: &str, topic: &str) -> String {
    format!(
        "Meeting notes with @{} about [[{}]]. \
         We discussed the progress and next steps. \
         Follow-up with @technical-lead required.",
        person, topic
    )
}

pub fn create_content_with_tags(tags: &[&str]) -> String {
    let tag_str = tags
        .iter()
        .map(|t| format!("#{}", t))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "This document covers important topics. \
         Keywords and categories: {}",
        tag_str
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_content_generation() {
        let ml_content = get_sample_content("machine-learning");
        assert!(ml_content.contains("Machine Learning"));
        assert!(ml_content.contains("@andrew-ng"));
        assert!(ml_content.contains("[[neural-networks]]"));

        let rust_content = get_sample_content("rust-programming");
        assert!(rust_content.contains("Rust"));
        assert!(rust_content.contains("#rust"));
    }

    #[test]
    fn test_content_with_mentions() {
        let content = create_content_with_mentions("alice-smith", "project-alpha");
        assert!(content.contains("@alice-smith"));
        assert!(content.contains("[[project-alpha]]"));
    }
}
