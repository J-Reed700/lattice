//! The rate-limited Hub HTTP client that implements `ModelCatalogPort`.
use super::*;

/// Hugging Face Model Hub adapter.
///
/// Implements `ModelCatalogPort` by querying the Hugging Face API.
pub struct HuggingFaceAdapter {
    client: Client,
    pub(super) last_request: Arc<Mutex<Option<Instant>>>,
    min_request_interval: Duration,
}

impl HuggingFaceAdapter {
    /// Create a new Hugging Face adapter.
    ///
    /// # Configuration
    /// - HTTP client with 10s timeout
    /// - Rate limiting: 500ms between requests
    pub fn new() -> Self {
        let client = reqwest_client_builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            last_request: Arc::new(Mutex::new(None)),
            min_request_interval: Duration::from_millis(500),
        }
    }

    pub(super) fn search_url(query: &str, limit: usize) -> String {
        if let Some(filter) = query.strip_prefix("filter:") {
            format!(
                "https://huggingface.co/api/models?filter={}&limit={}&full=true&sort=downloads&direction=-1",
                urlencoding::encode(filter),
                limit
            )
        } else {
            format!(
                "https://huggingface.co/api/models?search={}&limit={}&full=true&sort=downloads&direction=-1",
                urlencoding::encode(query),
                limit
            )
        }
    }

    /// Enforce rate limiting.
    ///
    /// Sleeps if necessary to maintain minimum interval between requests.
    pub(super) async fn enforce_rate_limit(&self) {
        // Scoped locking: compute delay while holding lock, then release before await
        let delay_needed = {
            let mut guard = self.last_request.lock();

            if let Some(last_time) = *guard {
                let elapsed = last_time.elapsed();
                if elapsed < self.min_request_interval {
                    Some(self.min_request_interval - elapsed)
                } else {
                    *guard = Some(Instant::now());
                    None
                }
            } else {
                *guard = Some(Instant::now());
                None
            }
        }; // Guard dropped here

        // Safe: no guard held across await point
        if let Some(delay) = delay_needed {
            tokio::time::sleep(delay).await;
            // Update timestamp after sleep
            *self.last_request.lock() = Some(Instant::now());
        }
    }

    /// Search Hugging Face models via API.
    ///
    /// # Arguments
    /// - `query` - Search query
    /// - `limit` - Maximum number of results
    ///
    /// # Returns
    /// List of models or error.
    async fn search_api(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<HuggingFaceModel>, AppError> {
        self.enforce_rate_limit().await;

        // `filter:<tag>` is an internal catalog query used for category
        // discovery. The Hub's filter searches model metadata, while its
        // `search` parameter mostly searches repository names and leaves out
        // many valid embedding models.
        let url = Self::search_url(query, limit);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "RecallDesktop/1.0")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Hugging Face API request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Hugging Face API returned status: {}",
                response.status()
            )));
        }

        let models: Vec<HuggingFaceModel> = response.json().await.map_err(|e| {
            AppError::Network(format!("Failed to parse Hugging Face response: {}", e))
        })?;

        Ok(models)
    }

    /// Get a specific model by ID.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier (e.g., "meta-llama/Llama-3.2-7B")
    ///
    /// # Returns
    /// Model metadata or error.
    async fn get_model_api(&self, model_id: &str) -> Result<HuggingFaceModel, AppError> {
        self.enforce_rate_limit().await;

        let encoded_model_id = model_id
            .split('/')
            .map(urlencoding::encode)
            .collect::<Vec<_>>()
            .join("/");
        let url = format!(
            "https://huggingface.co/api/models/{}?blobs=true",
            encoded_model_id
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "RecallDesktop/1.0")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Hugging Face API request failed: {}", e)))?;

        if response.status().as_u16() == 404 {
            return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
        }

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Hugging Face API returned status: {}",
                response.status()
            )));
        }

        let model: HuggingFaceModel = response.json().await.map_err(|e| {
            AppError::Network(format!("Failed to parse Hugging Face response: {}", e))
        })?;

        Ok(model)
    }
}

impl Default for HuggingFaceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelCatalogPort for HuggingFaceAdapter {
    async fn get_model_variants(
        &self,
        model_id: &str,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        // Unlike broad discovery, an explicit file listing must surface errors
        // so the user can retry instead of being told no versions exist.
        Ok(self.get_model_api(model_id).await?.gguf_variants())
    }

    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        // Graceful degradation: return empty on error instead of crashing
        match self.search_api(query, limit).await {
            Ok(models) => Ok(models
                .into_iter()
                .map(|m| m.to_external_metadata())
                .collect()),
            Err(e) => {
                tracing::warn!(
                    "Hugging Face API search failed: {}. Returning empty results.",
                    e
                );
                Ok(Vec::new())
            }
        }
    }

    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError> {
        // Graceful degradation: return None on error instead of crashing
        match self.get_model_api(model_id).await {
            Ok(model) => Ok(Some(model.to_external_metadata())),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(e) => {
                tracing::warn!("Hugging Face API get_model failed: {}. Returning None.", e);
                Ok(None)
            }
        }
    }
}
