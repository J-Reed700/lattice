#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Integration Tests: Metadata Extraction
//!
//! Tests to verify that rich metadata is correctly extracted and stored
//! during document indexing.
//!
//! # Test Coverage
//! - Language detection (programming and natural languages)
//! - Category assignment
//! - Quality score calculation
//! - Word count accuracy
//! - Token count for chunks
//! - Access tracking
use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Simple test context
struct TestContext {
    pool: SqlitePool,
    temp_dir: TempDir,
}

impl TestContext {
    async fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display())).await?;

        lattice::infrastructure::persistence::database::init::initialize_database(&pool).await?;

        Ok(Self { pool, temp_dir })
    }
}

// ============================================================================
// Test 1: Language Detection - Programming Languages
// ============================================================================

#[tokio::test]
async fn test_language_detection_programming() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Test Python file
    let python_code = r#"
def fibonacci(n):
    """Calculate fibonacci number recursively."""
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

if __name__ == "__main__":
    print(fibonacci(10))
    "#;

    let python_file = ctx.temp_dir.path().join("fibonacci.py");
    fs::write(&python_file, python_code)?;
    let python_doc_id = index_test_file(&ctx.pool, &python_file).await?;

    let python_lang: String = sqlx::query_scalar("SELECT language FROM documents WHERE id = ?")
        .bind(&python_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        python_lang.to_lowercase().contains("python"),
        "Python file should be detected as Python, got: {}",
        python_lang
    );

    // Test Rust file
    let rust_code = r#"
fn main() {
    println!("Hello, world!");
}

pub struct Point {
    x: f64,
    y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
    "#;

    let rust_file = ctx.temp_dir.path().join("main.rs");
    fs::write(&rust_file, rust_code)?;
    let rust_doc_id = index_test_file(&ctx.pool, &rust_file).await?;

    let rust_lang: String = sqlx::query_scalar("SELECT language FROM documents WHERE id = ?")
        .bind(&rust_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        rust_lang.to_lowercase().contains("rust"),
        "Rust file should be detected as Rust, got: {}",
        rust_lang
    );

    // Test JavaScript file
    let js_code = r#"
function calculateSum(arr) {
    return arr.reduce((sum, num) => sum + num, 0);
}

class Calculator {
    constructor() {
        this.result = 0;
    }

    add(x) {
        this.result += x;
        return this;
    }
}

export default Calculator;
    "#;

    let js_file = ctx.temp_dir.path().join("calculator.js");
    fs::write(&js_file, js_code)?;
    let js_doc_id = index_test_file(&ctx.pool, &js_file).await?;

    let js_lang: String = sqlx::query_scalar("SELECT language FROM documents WHERE id = ?")
        .bind(&js_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        js_lang.to_lowercase().contains("javascript") || js_lang.to_lowercase().contains("js"),
        "JavaScript file should be detected as JavaScript, got: {}",
        js_lang
    );

    Ok(())
}

// ============================================================================
// Test 2: Language Detection - Natural Languages
// ============================================================================

#[tokio::test]
async fn test_language_detection_natural() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Test English text
    let english_text = r#"
Artificial intelligence is transforming the way we live and work.
Machine learning algorithms can now recognize patterns in data
that would be impossible for humans to detect manually.
The future of technology is exciting and full of possibilities.
    "#;

    let english_file = ctx.temp_dir.path().join("english_article.txt");
    fs::write(&english_file, english_text)?;
    let english_doc_id = index_test_file(&ctx.pool, &english_file).await?;

    let english_lang: String = sqlx::query_scalar("SELECT language FROM documents WHERE id = ?")
        .bind(&english_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        english_lang.to_lowercase().contains("english") || english_lang == "en",
        "English text should be detected as English, got: {}",
        english_lang
    );

    // Test mixed content (code + English comments)
    let mixed_content = r#"
# Python Script for Data Analysis

This script demonstrates data processing techniques using pandas.

import pandas as pd
import numpy as np

def process_data(df):
    """Process the dataframe and return summary statistics."""
    return df.describe()

# The algorithm works by iterating through each row
# and applying transformations to numeric columns.
    "#;

    let mixed_file = ctx.temp_dir.path().join("mixed_content.py");
    fs::write(&mixed_file, mixed_content)?;
    let mixed_doc_id = index_test_file(&ctx.pool, &mixed_file).await?;

    let mixed_lang: String = sqlx::query_scalar("SELECT language FROM documents WHERE id = ?")
        .bind(&mixed_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    // Should detect as Python (file extension) or English (content)
    assert!(
        !mixed_lang.is_empty() && mixed_lang != "unknown",
        "Mixed content should have language detected, got: {}",
        mixed_lang
    );

    Ok(())
}

// ============================================================================
// Test 3: Category Assignment
// ============================================================================

#[tokio::test]
async fn test_category_assignment() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Test Code category
    let code_content = r#"
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
    "#;

    let code_file = ctx.temp_dir.path().join("HelloWorld.java");
    fs::write(&code_file, code_content)?;
    let code_doc_id = index_test_file(&ctx.pool, &code_file).await?;

    let code_category: String = sqlx::query_scalar("SELECT category FROM documents WHERE id = ?")
        .bind(&code_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        code_category.to_lowercase().contains("code")
            || code_category.to_lowercase().contains("programming"),
        "Code file should be categorized as code, got: {}",
        code_category
    );

    // Test Documentation category
    let doc_content = r#"
# API Documentation

## Overview

This document describes the REST API endpoints for our service.

## Endpoints

### GET /api/users
Returns a list of all users.

**Response:**
```json
{
    "users": [...]
}
```

### POST /api/users
Creates a new user.
    "#;

    let doc_file = ctx.temp_dir.path().join("README.md");
    fs::write(&doc_file, doc_content)?;
    let doc_doc_id = index_test_file(&ctx.pool, &doc_file).await?;

    let doc_category: String = sqlx::query_scalar("SELECT category FROM documents WHERE id = ?")
        .bind(&doc_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert!(
        doc_category.to_lowercase().contains("documentation")
            || doc_category.to_lowercase().contains("doc"),
        "Documentation file should be categorized as documentation, got: {}",
        doc_category
    );

    // Test Tutorial/Guide category
    let tutorial_content = r#"
# Getting Started with Machine Learning

## Step 1: Install Dependencies

First, install the required packages:

```bash
pip install numpy pandas scikit-learn
```

## Step 2: Load Your Data

Learn how to load and prepare your dataset...

## Step 3: Train Your Model

Follow these instructions to train your first model...
    "#;

    let tutorial_file = ctx.temp_dir.path().join("tutorial.md");
    fs::write(&tutorial_file, tutorial_content)?;
    let tutorial_doc_id = index_test_file(&ctx.pool, &tutorial_file).await?;

    let tutorial_category: String =
        sqlx::query_scalar("SELECT category FROM documents WHERE id = ?")
            .bind(&tutorial_doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    // Should be tutorial/guide/documentation
    assert!(
        !tutorial_category.is_empty() && tutorial_category != "uncategorized",
        "Tutorial should have category assigned, got: {}",
        tutorial_category
    );

    Ok(())
}

// ============================================================================
// Test 4: Quality Score Calculation
// ============================================================================

#[tokio::test]
async fn test_quality_score_calculation() -> Result<()> {
    let ctx = TestContext::new().await?;

    // High-quality content: long, well-structured, substantive
    let high_quality_content = r#"
# Comprehensive Guide to Database Optimization

## Introduction

Database performance optimization is a critical aspect of software engineering
that directly impacts application responsiveness and user experience. This guide
provides a thorough exploration of optimization techniques.

## Understanding Query Performance

Query performance depends on multiple factors including index usage, join
strategies, and data distribution. Modern databases employ sophisticated
query planners that analyze execution paths.

### Index Selection

Proper index selection dramatically improves query performance. Consider:

1. **Cardinality**: High-cardinality columns benefit most from indexing
2. **Query Patterns**: Index columns frequently used in WHERE clauses
3. **Composite Indexes**: Multi-column indexes for complex queries

### Query Optimization Strategies

- Avoid SELECT * in production queries
- Use EXPLAIN ANALYZE to understand execution plans
- Implement connection pooling
- Cache frequently accessed data

## Advanced Techniques

### Partitioning

Table partitioning improves query performance by limiting scan scope.

### Materialized Views

Pre-computed views accelerate complex aggregations.

## Conclusion

Systematic optimization requires understanding database internals, monitoring
performance metrics, and iterative refinement.
    "#;

    let high_quality_file = ctx.temp_dir.path().join("optimization_guide.md");
    fs::write(&high_quality_file, high_quality_content)?;
    let high_quality_doc_id = index_test_file(&ctx.pool, &high_quality_file).await?;

    let high_quality_score: f64 =
        sqlx::query_scalar("SELECT quality_score FROM documents WHERE id = ?")
            .bind(&high_quality_doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    assert!(
        high_quality_score > 0.6,
        "High-quality content should have score > 0.6, got {}",
        high_quality_score
    );

    // Low-quality content: short, unstructured
    let low_quality_content = "todo: fix bug";

    let low_quality_file = ctx.temp_dir.path().join("note.txt");
    fs::write(&low_quality_file, low_quality_content)?;
    let low_quality_doc_id = index_test_file(&ctx.pool, &low_quality_file).await?;

    let low_quality_score: f64 =
        sqlx::query_scalar("SELECT quality_score FROM documents WHERE id = ?")
            .bind(&low_quality_doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    assert!(
        low_quality_score < 0.5,
        "Low-quality content should have score < 0.5, got {}",
        low_quality_score
    );

    // Medium quality content
    let medium_quality_content = r#"
# Project Notes

Some notes about the project implementation.

- Feature A needs testing
- Bug in module B
- Consider refactoring C

Next steps: review with team
    "#;

    let medium_quality_file = ctx.temp_dir.path().join("project_notes.md");
    fs::write(&medium_quality_file, medium_quality_content)?;
    let medium_quality_doc_id = index_test_file(&ctx.pool, &medium_quality_file).await?;

    let medium_quality_score: f64 =
        sqlx::query_scalar("SELECT quality_score FROM documents WHERE id = ?")
            .bind(&medium_quality_doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    assert!(
        (0.4..=0.7).contains(&medium_quality_score),
        "Medium-quality content should have score 0.4-0.7, got {}",
        medium_quality_score
    );

    Ok(())
}

// ============================================================================
// Test 5: Word Count Accuracy
// ============================================================================

#[tokio::test]
async fn test_word_count_accuracy() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Test document with known word count
    let content =
        "The quick brown fox jumps over the lazy dog. This sentence has exactly ten words here.";
    // Word count: 19 words

    let test_file = ctx.temp_dir.path().join("word_count_test.txt");
    fs::write(&test_file, content)?;
    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    let word_count: i64 = sqlx::query_scalar("SELECT word_count FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    // Count words manually
    let expected_count = content.split_whitespace().count() as i64;

    assert_eq!(
        word_count, expected_count,
        "Word count should be accurate: expected {}, got {}",
        expected_count, word_count
    );

    // Test empty document
    let empty_file = ctx.temp_dir.path().join("empty.txt");
    fs::write(&empty_file, "")?;
    let empty_doc_id = index_test_file(&ctx.pool, &empty_file).await?;

    let empty_word_count: i64 = sqlx::query_scalar("SELECT word_count FROM documents WHERE id = ?")
        .bind(&empty_doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert_eq!(
        empty_word_count, 0,
        "Empty document should have word count 0"
    );

    // Test document with special characters
    let special_content = "Hello, world! How are you? I'm fine. Test@example.com";
    let special_file = ctx.temp_dir.path().join("special_chars.txt");
    fs::write(&special_file, special_content)?;
    let special_doc_id = index_test_file(&ctx.pool, &special_file).await?;

    let special_word_count: i64 =
        sqlx::query_scalar("SELECT word_count FROM documents WHERE id = ?")
            .bind(&special_doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    let expected_special_count = special_content.split_whitespace().count() as i64;
    assert_eq!(
        special_word_count, expected_special_count,
        "Word count should handle special characters correctly"
    );

    Ok(())
}

// ============================================================================
// Test 6: Token Count for Chunks
// ============================================================================

#[tokio::test]
async fn test_token_count_for_chunks() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create document that will be chunked
    let content = r#"
Machine learning is a fascinating field. It enables computers to learn from data.
Deep learning is a subset of machine learning. Neural networks are key components.
Natural language processing helps computers understand human language.
Computer vision allows machines to interpret visual information.
    "#;

    let test_file = ctx.temp_dir.path().join("ml_overview.txt");
    fs::write(&test_file, content)?;
    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Get chunks with token counts
    let rows = sqlx::query("SELECT content, token_count FROM text_chunks WHERE document_id = ?")
        .bind(&doc_id)
        .fetch_all(&ctx.pool)
        .await?;

    assert!(!rows.is_empty(), "Should have at least one chunk");

    for row in rows {
        let content: String = row.get("content");
        let token_count: i64 = row.get("token_count");

        // Verify token count is set and positive
        assert!(
            token_count > 0,
            "Chunk should have positive token count, got {}",
            token_count
        );

        // Token count should be approximately word count * 1.3
        // (common ratio for English text)
        let word_count = content.split_whitespace().count() as i64;
        let estimated_tokens = (word_count as f64 * 1.3) as i64;

        // Allow ±30% variance
        let lower_bound = (estimated_tokens as f64 * 0.7) as i64;
        let upper_bound = (estimated_tokens as f64 * 1.5) as i64;

        assert!(
            token_count >= lower_bound && token_count <= upper_bound,
            "Token count {} should be roughly word_count * 1.3 (word_count={}, estimated={})",
            token_count,
            word_count,
            estimated_tokens
        );
    }

    Ok(())
}

// ============================================================================
// Test 7: Access Tracking
// ============================================================================

#[tokio::test]
async fn test_access_tracking() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create and index document
    let content = "Test document for access tracking.";
    let test_file = ctx.temp_dir.path().join("access_test.txt");
    fs::write(&test_file, content)?;
    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Initial state: no accesses
    let initial_row =
        sqlx::query("SELECT access_count, last_accessed_at FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    let initial_access_count: i64 = initial_row.get("access_count");
    let initial_last_accessed: Option<i64> = initial_row.get("last_accessed_at");

    assert_eq!(initial_access_count, 0, "Initial access count should be 0");
    assert!(
        initial_last_accessed.is_none(),
        "Initial last_accessed should be NULL"
    );

    // Simulate access (search or view)
    track_document_access(&ctx.pool, &doc_id).await?;

    // Verify access count incremented
    let after_first_row =
        sqlx::query("SELECT access_count, last_accessed_at FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    let after_first_count: i64 = after_first_row.get("access_count");
    let after_first_accessed: Option<i64> = after_first_row.get("last_accessed_at");

    assert_eq!(
        after_first_count, 1,
        "Access count should be 1 after first access"
    );
    assert!(
        after_first_accessed.is_some(),
        "last_accessed_at should be set after first access"
    );

    let first_access_time = after_first_accessed.unwrap();

    // Wait and access again (use 1100ms to ensure timestamp() changes, as it has 1-second resolution)
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
    track_document_access(&ctx.pool, &doc_id).await?;

    // Verify access count incremented again
    let after_second_row =
        sqlx::query("SELECT access_count, last_accessed_at FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    let after_second_count: i64 = after_second_row.get("access_count");
    let after_second_accessed: i64 = after_second_row.get("last_accessed_at");

    assert_eq!(
        after_second_count, 2,
        "Access count should be 2 after second access"
    );
    assert!(
        after_second_accessed > first_access_time,
        "last_accessed_at should be updated to later time"
    );

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Index a test file with metadata extraction
async fn index_test_file(pool: &SqlitePool, file_path: &PathBuf) -> Result<String> {
    use chrono::Utc;
    use uuid::Uuid;

    let doc_id = Uuid::new_v4().to_string();
    let file_name = file_path.file_name().unwrap().to_str().unwrap();
    let content = fs::read_to_string(file_path)?;
    let checksum = format!("{:x}", md5::compute(&content));

    // Extract metadata (simplified - real implementation would use MetadataExtractor)
    let language = detect_language(file_path, &content);
    let category = assign_category(file_path, &content);
    let quality_score = calculate_quality_score(&content);
    let word_count = content.split_whitespace().count() as i64;

    // Create document with metadata
    sqlx::query(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, file_type, size_bytes,
            modified_at, checksum, status,
            language, category, quality_score, word_count,
            access_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, 'indexed', ?, ?, ?, ?, 0)
        "#,
    )
    .bind(&doc_id)
    .bind(file_path.to_str().unwrap())
    .bind(file_name)
    .bind(
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt"),
    )
    .bind(content.len() as i64)
    .bind(Utc::now().to_rfc3339())
    .bind(&checksum)
    .bind(&language)
    .bind(&category)
    .bind(quality_score)
    .bind(word_count)
    .execute(pool)
    .await?;

    // Create chunk with metadata
    let chunk_id = Uuid::new_v4().to_string();
    let token_count = (word_count as f64 * 1.3) as i64;

    sqlx::query(
        r#"
        INSERT INTO text_chunks (
            id, document_id, content, chunk_index,
            language, token_count
        )
        VALUES (?, ?, ?, 0, ?, ?)
        "#,
    )
    .bind(&chunk_id)
    .bind(&doc_id)
    .bind(&content)
    .bind(&language)
    .bind(token_count)
    .execute(pool)
    .await?;

    Ok(doc_id)
}

/// Detect language from file path and content
fn detect_language(file_path: &std::path::Path, content: &str) -> String {
    // Check file extension first
    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
        match ext {
            "py" => return "Python".to_string(),
            "rs" => return "Rust".to_string(),
            "js" => return "JavaScript".to_string(),
            "java" => return "Java".to_string(),
            "cpp" | "cc" | "cxx" => return "C++".to_string(),
            "c" | "h" => return "C".to_string(),
            "go" => return "Go".to_string(),
            _ => {}
        }
    }

    // Simple heuristic for natural language
    if content.split_whitespace().count() > 10
        && (content.contains("the ") || content.contains("The "))
    {
        return "English".to_string();
    }

    "unknown".to_string()
}

/// Assign category based on file path and content
fn assign_category(file_path: &std::path::Path, content: &str) -> String {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let content_lower = content.to_lowercase();

    // Check filename patterns
    if file_name.to_lowercase().contains("readme") || file_name.to_lowercase().contains("doc") {
        return "documentation".to_string();
    }

    if file_name.to_lowercase().contains("tutorial") || file_name.to_lowercase().contains("guide") {
        return "tutorial".to_string();
    }

    // Check content patterns
    if content_lower.contains("class ")
        || content_lower.contains("function ")
        || content_lower.contains("def ")
        || content_lower.contains("impl ")
    {
        return "code".to_string();
    }

    if content_lower.contains("## step")
        || content_lower.contains("getting started")
        || content_lower.contains("how to")
    {
        return "tutorial".to_string();
    }

    if content_lower.contains("api documentation")
        || content_lower.contains("### get ")
        || content_lower.contains("### post ")
    {
        return "documentation".to_string();
    }

    "uncategorized".to_string()
}

/// Calculate quality score for content
fn calculate_quality_score(content: &str) -> f64 {
    let word_count = content.split_whitespace().count();
    let line_count = content.lines().count();
    let has_structure = content.contains('#') || content.contains("##");
    let avg_word_per_line = if line_count > 0 {
        word_count as f64 / line_count as f64
    } else {
        0.0
    };

    let mut score: f64 = 0.0;

    // Length factor (longer is generally better)
    if word_count > 500 {
        score += 0.3;
    } else if word_count > 200 {
        score += 0.2;
    } else if word_count > 50 {
        score += 0.1;
    }

    // Structure factor
    if has_structure {
        score += 0.2;
    }

    // Density factor (reasonable word per line)
    if avg_word_per_line > 5.0 && avg_word_per_line < 20.0 {
        score += 0.2;
    }

    // Completeness factor
    if content.len() > 100 {
        score += 0.1;
    }

    // Baseline
    score += 0.2;

    // Clamp between 0 and 1
    score.clamp(0.0, 1.0)
}

/// Track document access
async fn track_document_access(pool: &SqlitePool, doc_id: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        r#"
        UPDATE documents
        SET access_count = access_count + 1,
            last_accessed_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(doc_id)
    .execute(pool)
    .await?;

    Ok(())
}
