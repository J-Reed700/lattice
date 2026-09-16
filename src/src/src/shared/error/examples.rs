//! Examples of using the enhanced error handling system
//!
//! This module demonstrates best practices for using AppError in the 5 critical paths:
//! - Model Download
//! - File Indexing
//! - Web Ingestion
//! - Vector Search
//! - Chat/Q&A

#![allow(dead_code)]
#![allow(unused_variables)]

use super::{AppError, ErrorResponse, Result, ResultExt};

// ============================================================================
// Example 1: Model Download Error Handling
// ============================================================================

/// Example showing error handling for model download operations
pub async fn model_download_example() -> Result<()> {
    let model_id = "llama-3";

    // Using builder methods for common errors
    if !model_exists(model_id).await? {
        return Err(AppError::not_found("Model", model_id)
            .with_suggestion("Use 'list-models' command to see available models"));
    }

    // Check disk space with structured error
    let required_space = 5_000_000_000; // 5GB
    let available_space = get_available_space()?;

    if available_space < required_space {
        return Err(AppError::FileSystem(format!(
            "Insufficient disk space: need {}GB, have {}GB",
            required_space / 1_000_000_000,
            available_space / 1_000_000_000
        ))
        .with_suggestion("Free up disk space or choose a smaller model"));
    }

    // Service availability check
    if !is_download_service_ready().await {
        return Err(AppError::service_unavailable(
            "ModelDownloadService",
            "Server is under maintenance",
        ));
    }

    // Rate limiting with retry information
    if is_rate_limited().await {
        return Err(AppError::rate_limited(Some(60)));
    }

    // Simulate download with progress
    download_model_with_progress(model_id)
        .await
        .context("Failed to download model")?;

    Ok(())
}

/// Example of handling download errors with retry logic
pub async fn download_with_retry(model_id: &str, max_retries: u32) -> Result<String> {
    let mut retries = 0;

    loop {
        match attempt_download(model_id).await {
            Ok(path) => return Ok(path),
            Err(e) if e.is_recoverable() && retries < max_retries => {
                retries += 1;
                println!("Retry {}/{}: {}", retries, max_retries, e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2u64.pow(retries))).await;
            }
            Err(e) => return Err(e),
        }
    }
}

// ============================================================================
// Example 2: File Indexing Error Handling
// ============================================================================

/// Example showing comprehensive file indexing error handling
pub async fn file_indexing_example(file_path: &str) -> Result<Vec<String>> {
    // File existence check
    if !std::path::Path::new(file_path).exists() {
        return Err(AppError::file_not_found(file_path));
    }

    // Get file metadata
    let metadata = tokio::fs::metadata(file_path)
        .await
        .context("Failed to read file metadata")?;

    // Check file size
    let file_size = metadata.len();
    let max_size = 50_000_000; // 50MB

    if file_size > max_size {
        return Err(AppError::FileTooLarge {
            path: file_path.to_string(),
            size_bytes: file_size,
            max_size_bytes: max_size,
        });
    }

    // Check file type
    let extension = std::path::Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if !is_supported_file_type(extension) {
        return Err(AppError::UnsupportedFileType {
            path: file_path.to_string(),
            detected_type: format!("application/{}", extension),
        });
    }

    // Extract content with error handling
    let content =
        extract_file_content(file_path)
            .await
            .map_err(|e| AppError::ContentExtraction {
                path: file_path.to_string(),
                reason: e.to_string(),
            })?;

    // Generate embeddings
    generate_embeddings(&content)
        .await
        .map_err(|e| AppError::EmbeddingFailed {
            reason: e.to_string(),
        })?;

    Ok(vec!["chunk1".to_string(), "chunk2".to_string()])
}

/// Example of batch file indexing with error collection
pub async fn batch_indexing_example(files: Vec<String>) -> Result<Vec<String>> {
    let mut successful = Vec::new();
    let mut errors = Vec::new();

    for file in files {
        match file_indexing_example(&file).await {
            Ok(chunks) => {
                successful.push(format!("{}: {} chunks", file, chunks.len()));
            }
            Err(e) => {
                // Collect error but continue processing
                errors.push(format!("{}: {}", file, e));

                // Log error with full context
                eprintln!("Failed to index {}: {}", file, e);
                if let Some(suggestions) = e.get_suggestions().first() {
                    eprintln!("  Suggestion: {}", suggestions);
                }
            }
        }
    }

    if !errors.is_empty() {
        eprintln!("Indexing completed with {} errors:", errors.len());
        for error in &errors {
            eprintln!("  - {}", error);
        }
    }

    Ok(successful)
}

// ============================================================================
// Example 3: Web Ingestion Error Handling
// ============================================================================

/// Example showing web ingestion error handling
pub async fn web_ingestion_example(url: &str) -> Result<String> {
    // URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::invalid_input(
            "URL must start with http:// or https://",
        ));
    }

    // Parse URL
    let parsed_url = url::Url::parse(url).map_err(|e| AppError::InvalidUrl(e.to_string()))?;

    // Check if domain is allowed
    if !is_domain_allowed(parsed_url.host_str().unwrap_or("")) {
        return Err(AppError::validation_failed(
            "domain",
            "This domain is not allowed for ingestion",
        ));
    }

    // Fetch with timeout
    let content = fetch_with_timeout(url, 30).await.map_err(|e| {
        if e.to_string().contains("timeout") {
            AppError::network("Request timed out")
                .with_suggestion("The website took too long to respond. Try again later.")
        } else {
            AppError::network(&format!("Failed to fetch {}: {}", url, e))
                .with_suggestion("Check your internet connection and try again")
        }
    })?;

    // Extract text content
    extract_web_content(&content).context("Failed to extract content from webpage")
}

/// Example of handling redirects and retries
pub async fn web_fetch_with_redirects(url: &str, max_redirects: u32) -> Result<String> {
    let mut current_url = url.to_string();
    let mut redirects = 0;

    loop {
        match fetch_url(&current_url).await {
            Ok(content) => return Ok(content),
            Err(e) if is_redirect(&e) && redirects < max_redirects => {
                redirects += 1;
                current_url = extract_redirect_url(&e)?;
                println!("Following redirect to: {}", current_url);
            }
            Err(e) => return Err(e),
        }
    }
}

// ============================================================================
// Example 4: Vector Search Error Handling
// ============================================================================

/// Example showing vector search error handling
pub async fn vector_search_example(query: &str) -> Result<Vec<SearchResult>> {
    // Query validation
    if query.trim().is_empty() {
        return Err(AppError::validation_failed(
            "query",
            "Query cannot be empty",
        ));
    }

    if query.len() > 1000 {
        return Err(AppError::validation_failed(
            "query",
            "Query exceeds maximum length of 1000 characters",
        ));
    }

    // Check if embedding service is ready
    if !is_embedding_service_ready().await {
        return Err(AppError::service_unavailable(
            "EmbeddingService",
            "Model is still loading, please wait",
        )
        .with_suggestion("Wait a few seconds for the model to load"));
    }

    // Generate query embedding
    let embedding =
        generate_query_embedding(query)
            .await
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Failed to generate query embedding: {}", e),
            })?;

    // Perform search
    search_with_embedding(&embedding)
        .await
        .context("Failed to execute vector search")
}

/// Example of hybrid search with fallback
pub async fn hybrid_search_example(query: &str) -> Result<Vec<SearchResult>> {
    // Try vector search first
    match vector_search_example(query).await {
        Ok(results) if !results.is_empty() => Ok(results),
        Ok(_) => {
            // Fall back to keyword search if no vector results
            println!("No vector search results, falling back to keyword search");
            keyword_search(query)
                .await
                .context("Keyword search also failed")
        }
        Err(e) if e.is_recoverable() => {
            // If vector search fails but is recoverable, try keyword search
            println!("Vector search failed ({}), using keyword search", e);
            keyword_search(query)
                .await
                .context("Fallback keyword search failed")
        }
        Err(e) => Err(e),
    }
}

// ============================================================================
// Example 5: Chat/Q&A Error Handling
// ============================================================================

/// Example showing chat/Q&A error handling
pub async fn chat_qa_example(question: &str) -> Result<String> {
    // Input validation
    if question.trim().is_empty() {
        return Err(AppError::validation_failed(
            "question",
            "Question cannot be empty",
        ));
    }

    if question.len() > 10_000 {
        return Err(AppError::validation_failed(
            "question",
            "Question exceeds maximum length of 10,000 characters",
        )
        .with_suggestion("Please shorten your question"));
    }

    // Check if model is installed
    if !is_chat_model_installed().await {
        return Err(
            AppError::AiModelsNotInstalled("Default chat model not installed".to_string())
                .with_suggestion("Download the model from Settings → Models"),
        );
    }

    // Check if model is loaded
    if !is_chat_model_loaded().await {
        return Err(
            AppError::ModelLoadFailed("Chat model is not loaded in memory".to_string())
                .with_suggestion("Wait for the model to load or restart the application"),
        );
    }

    // Check rate limiting
    if is_user_rate_limited().await {
        return Err(AppError::rate_limited(Some(30)));
    }

    // Check queue capacity
    if is_processing_queue_full().await {
        return Err(AppError::QueueFull
            .with_suggestion("The system is busy. Please try again in a moment."));
    }

    // Generate answer with timeout
    generate_chat_response(question).await.map_err(|e| {
        if e.to_string().contains("timeout") {
            AppError::internal("Request timed out after 30 seconds")
                .with_suggestion("Try a shorter question or retry later")
        } else {
            AppError::Other(format!("Failed to generate response: {}", e))
        }
    })
}

/// Example of conversation with context management
pub async fn conversation_example(
    messages: Vec<String>,
    max_context_size: usize,
) -> Result<Vec<String>> {
    let mut responses = Vec::new();
    let mut context_size = 0;

    for message in messages {
        // Check context size
        context_size += message.len();
        if context_size > max_context_size {
            return Err(AppError::validation_failed(
                "context",
                &format!(
                    "Conversation context exceeds {} characters",
                    max_context_size
                ),
            )
            .with_suggestion("Start a new conversation to continue"));
        }

        // Process message
        match chat_qa_example(&message).await {
            Ok(response) => {
                context_size += response.len();
                responses.push(response);
            }
            Err(e) if e.error_code() == "RATE_LIMIT_EXCEEDED" => {
                // Handle rate limiting gracefully
                return Err(e.with_suggestion(
                    "You've sent too many messages. Please wait before continuing.",
                ));
            }
            Err(e) => return Err(e),
        }
    }

    Ok(responses)
}

// ============================================================================
// Example 6: Using ErrorResponse for Frontend Communication
// ============================================================================

/// Example showing how to convert errors for frontend consumption
pub fn frontend_error_example() -> String {
    use serde_json;

    // Create various types of errors
    let errors = vec![
        AppError::validation_failed("email", "Invalid email format"),
        AppError::not_found("Document", "doc-123"),
        AppError::rate_limited(Some(60)),
        AppError::QueueFull,
        AppError::service_unavailable("SearchService", "Index is rebuilding"),
    ];

    let mut responses = Vec::new();

    for error in errors {
        let response = ErrorResponse::from(error);
        responses.push(response);
    }

    // The frontend receives structured JSON with:
    // - code: Error code for programmatic handling
    // - message: User-friendly message
    // - context: Structured data about the error
    // - suggestions: Recovery suggestions
    // - recoverable: Whether retry might help
    // - status_code: HTTP status code

    serde_json::to_string_pretty(&responses).unwrap()
}

/// Example of handling errors in a Tauri command
#[cfg(feature = "custom-protocol")]
#[tauri::command]
pub async fn indexed_search(query: String) -> std::result::Result<Vec<String>, String> {
    // AppError automatically converts to String for Tauri
    vector_search_example(&query)
        .await
        .map(|results| results.into_iter().map(|r| r.id).collect())
        .map_err(|e| {
            // Log full error for debugging
            tracing::error!("Search failed: {:?}", e);

            // Return user-friendly message
            e.to_user_friendly_message()
        })
}

// ============================================================================
// Example 7: Error Propagation and Context
// ============================================================================

/// Example showing error propagation with context
pub async fn complex_operation_example(config_path: &str) -> Result<()> {
    // Load configuration with context
    let config = load_config(config_path).context("Failed to load configuration")?;

    // Initialize services with context
    initialize_services(&config).await.with_context(|| {
        format!(
            "Failed to initialize services with config from {}",
            config_path
        )
    })?;

    // Process data with context
    process_data()
        .await
        .context("Failed during data processing phase")?;

    Ok(())
}

/// Example of adding context through multiple layers
pub async fn layered_operation_example(user_id: u64, doc_id: &str) -> Result<String> {
    // Layer 1: User validation
    validate_user(user_id)
        .await
        .with_context(|| format!("User validation failed for ID {}", user_id))?;

    // Layer 2: Document retrieval
    let document = fetch_document(doc_id)
        .await
        .with_context(|| format!("Failed to fetch document {} for user {}", doc_id, user_id))?;

    // Layer 3: Processing
    process_user_document(user_id, &document)
        .await
        .with_context(|| {
            format!(
                "Processing failed for user {} and document {}",
                user_id, doc_id
            )
        })
}

// ============================================================================
// Example 8: Error Categorization and Handling
// ============================================================================

/// Example showing how to handle errors based on their properties
pub async fn categorized_error_handling(operation: impl Fn() -> Result<String>) {
    match operation() {
        Ok(result) => println!("Success: {}", result),
        Err(e) => {
            // Log error details
            eprintln!("Error: {}", e);
            eprintln!("Code: {}", e.error_code());
            eprintln!("HTTP Status: {}", e.http_status_code());

            // Handle based on category
            if e.is_recoverable() {
                eprintln!("This error is recoverable. Retrying might help.");
                // Implement retry logic
            }

            if e.is_user_fixable() {
                eprintln!("User action required: {}", e.to_user_friendly_message());
                for suggestion in e.get_suggestions() {
                    eprintln!("  - {}", suggestion);
                }
            }

            if e.is_fatal() {
                eprintln!("FATAL ERROR: Application may need to restart");
                // Trigger graceful shutdown or recovery
            }

            // Get structured context for logging
            let context = e.get_context();
            if !context.is_empty() {
                eprintln!("Context:");
                for (key, value) in context {
                    eprintln!("  {}: {}", key, value);
                }
            }
        }
    }
}

// ============================================================================
// Mock Helper Functions
// ============================================================================

#[derive(Debug)]
pub struct SearchResult {
    id: String,
    score: f32,
}

async fn model_exists(_model_id: &str) -> Result<bool> {
    Ok(true)
}

fn get_available_space() -> Result<u64> {
    Ok(10_000_000_000) // 10GB
}

async fn is_download_service_ready() -> bool {
    true
}

async fn is_rate_limited() -> bool {
    false
}

async fn download_model_with_progress(_model_id: &str) -> Result<()> {
    Ok(())
}

async fn attempt_download(_model_id: &str) -> Result<String> {
    Ok("/models/llama-3".to_string())
}

fn is_supported_file_type(ext: &str) -> bool {
    matches!(ext, "txt" | "md" | "pdf" | "docx" | "html")
}

async fn extract_file_content(_path: &str) -> std::io::Result<String> {
    Ok("file content".to_string())
}

async fn generate_embeddings(_content: &str) -> std::io::Result<Vec<f32>> {
    Ok(vec![0.1, 0.2, 0.3])
}

fn is_domain_allowed(_domain: &str) -> bool {
    true
}

async fn fetch_with_timeout(_url: &str, _timeout: u64) -> std::io::Result<String> {
    Ok("web content".to_string())
}

fn extract_web_content(_html: &str) -> Result<String> {
    Ok("extracted text".to_string())
}

async fn fetch_url(_url: &str) -> Result<String> {
    Ok("response".to_string())
}

fn is_redirect(_error: &AppError) -> bool {
    false
}

fn extract_redirect_url(_error: &AppError) -> Result<String> {
    Ok("http://redirected.com".to_string())
}

async fn is_embedding_service_ready() -> bool {
    true
}

async fn generate_query_embedding(_query: &str) -> std::io::Result<Vec<f32>> {
    Ok(vec![0.1, 0.2, 0.3])
}

async fn search_with_embedding(_embedding: &[f32]) -> Result<Vec<SearchResult>> {
    Ok(vec![
        SearchResult {
            id: "doc1".to_string(),
            score: 0.95,
        },
        SearchResult {
            id: "doc2".to_string(),
            score: 0.85,
        },
    ])
}

async fn keyword_search(_query: &str) -> Result<Vec<SearchResult>> {
    Ok(vec![SearchResult {
        id: "doc3".to_string(),
        score: 0.75,
    }])
}

async fn is_chat_model_installed() -> bool {
    true
}

async fn is_chat_model_loaded() -> bool {
    true
}

async fn is_user_rate_limited() -> bool {
    false
}

async fn is_processing_queue_full() -> bool {
    false
}

async fn generate_chat_response(_question: &str) -> std::io::Result<String> {
    Ok("AI response".to_string())
}

fn load_config(_path: &str) -> Result<Config> {
    Ok(Config {})
}

async fn initialize_services(_config: &Config) -> Result<()> {
    Ok(())
}

async fn process_data() -> Result<()> {
    Ok(())
}

async fn validate_user(_user_id: u64) -> Result<()> {
    Ok(())
}

async fn fetch_document(_doc_id: &str) -> Result<Document> {
    Ok(Document {})
}

async fn process_user_document(_user_id: u64, _doc: &Document) -> Result<String> {
    Ok("processed".to_string())
}

#[derive(Debug)]
struct Config {}

#[derive(Debug)]
struct Document {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_download_errors() {
        let result = model_download_example().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_indexing_errors() {
        let result = file_indexing_example("/nonexistent/file.txt").await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "FILE_NOT_FOUND");
            assert_eq!(e.http_status_code(), 404);
        }
    }

    #[tokio::test]
    async fn test_web_ingestion_validation() {
        let result = web_ingestion_example("invalid-url").await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "INVALID_INPUT");
            assert!(e.is_user_fixable());
        }
    }

    #[tokio::test]
    async fn test_vector_search_validation() {
        let result = vector_search_example("").await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "VALIDATION_FAILED");
            assert!(e.to_string().contains("Query cannot be empty"));
        }
    }

    #[tokio::test]
    async fn test_chat_qa_validation() {
        let long_question = "a".repeat(10_001);
        let result = chat_qa_example(&long_question).await;
        assert!(result.is_err());
        if let Err(e) = result {
            // The error gets wrapped in Other when using with_suggestion
            assert!(e.to_string().contains("10,000 characters"));
        }
    }

    #[test]
    fn test_error_categorization() {
        // Recoverable errors
        assert!(AppError::QueueFull.is_recoverable());
        assert!(AppError::Network("error".to_string()).is_recoverable());
        assert!(AppError::rate_limited(None).is_recoverable());

        // User-fixable errors
        assert!(AppError::validation_failed("field", "reason").is_user_fixable());
        assert!(AppError::InvalidInput("test".to_string()).is_user_fixable());

        // Fatal errors
        assert!(AppError::Database("error".to_string()).is_fatal());
        assert!(AppError::Migration("error".to_string()).is_fatal());
    }

    #[test]
    fn test_error_response_serialization() {
        let error = AppError::validation_failed("email", "Invalid format");
        let response = ErrorResponse::from(error);

        assert_eq!(response.code, "VALIDATION_FAILED");
        assert_eq!(response.status_code, 400);
        assert!(!response.recoverable);

        // Verify it serializes properly
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("VALIDATION_FAILED"));
    }
}
