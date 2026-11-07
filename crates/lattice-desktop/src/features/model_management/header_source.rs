//! Reads a GGUF file's header from Hugging Face by HTTP range request with the runtime's reader, and
//! keeps what it read on disk by repository, file and revision so a file is read once.

use super::recommend::{GgufFacts, GgufHeaderSource};
use crate::features::llm::engine::gguf_metadata::{read_model_info_from, GgufModelInfo};
use async_trait::async_trait;
use lattice_core::shared::http::reqwest_client_builder;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The first range asked for; a header that does not parse in it is asked for again, twice as long.
const FIRST_RANGE_BYTES: u64 = 16 << 20;

/// The Hub client's own request limit (see `HuggingFaceAdapter`).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct HuggingFaceHeaderSource {
    client: Client,
    cache_dir: PathBuf,
}

impl HuggingFaceHeaderSource {
    pub fn new(cache_dir: PathBuf) -> Self {
        let client = reqwest_client_builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, cache_dir }
    }

    fn cache_path(&self, repo: &str, file: &str, revision: Option<&str>) -> PathBuf {
        let key = format!("{repo}__{file}__{}", revision.unwrap_or("main"));
        let name: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        self.cache_dir.join(format!("{name}.json"))
    }

    /// What an earlier read of this file kept, without asking the network.
    pub fn cached(&self, repo: &str, file: &str, revision: Option<&str>) -> Option<GgufFacts> {
        let bytes = std::fs::read(self.cache_path(repo, file, revision)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn store(&self, repo: &str, file: &str, revision: Option<&str>, facts: &GgufFacts) {
        let path = self.cache_path(repo, file, revision);
        let written = std::fs::create_dir_all(&self.cache_dir)
            .and_then(|()| serde_json::to_vec(facts).map_err(std::io::Error::other))
            .and_then(|bytes| std::fs::write(&path, bytes));
        if let Err(error) = written {
            tracing::warn!(%error, "could not keep a read GGUF header");
        }
    }

    fn url(repo: &str, file: &str, revision: Option<&str>) -> String {
        let path = |value: &str| {
            value
                .split('/')
                .map(|part| urlencoding::encode(part).into_owned())
                .collect::<Vec<_>>()
                .join("/")
        };
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            path(repo),
            urlencoding::encode(revision.unwrap_or("main")),
            path(file)
        )
    }

    async fn range(&self, url: &str, length: u64) -> Option<(Vec<u8>, u64)> {
        let response = self
            .client
            .get(url)
            .header("User-Agent", "RecallDesktop/1.0")
            .header("Range", format!("bytes=0-{}", length - 1))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let total = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|total| total.parse::<u64>().ok())
            .or_else(|| response.content_length())?;
        let bytes = response.bytes().await.ok()?;
        Some((bytes.to_vec(), total))
    }

    async fn read(&self, repo: &str, file: &str, revision: Option<&str>) -> Option<GgufFacts> {
        let url = Self::url(repo, file, revision);
        let mut length = FIRST_RANGE_BYTES;
        let mut previous: Option<GgufModelInfo> = None;
        loop {
            let (bytes, total) = self.range(&url, length).await?;
            let info = read_model_info_from(std::io::Cursor::new(bytes), total)?;
            if previous.as_ref() == Some(&info) || length >= total {
                return Some(GgufFacts {
                    gpu_weight_bytes: total.saturating_sub(info.cpu_weight_bytes),
                    shape: info.shape,
                });
            }
            previous = Some(info);
            length = length.saturating_mul(2);
        }
    }
}

#[async_trait]
impl GgufHeaderSource for HuggingFaceHeaderSource {
    async fn header(&self, repo: &str, file: &str, revision: Option<&str>) -> Option<GgufFacts> {
        if let Some(facts) = self.cached(repo, file, revision) {
            return Some(facts);
        }
        let facts = self.read(repo, file, revision).await?;
        self.store(repo, file, revision, &facts);
        Some(facts)
    }
}

/// Answers only from what has already been read, never from the network: a list row shows a
/// verdict at once when its header is known, and `null` otherwise.
pub struct CachedHeadersOnly(pub Arc<HuggingFaceHeaderSource>);

#[async_trait]
impl GgufHeaderSource for CachedHeadersOnly {
    async fn header(&self, repo: &str, file: &str, revision: Option<&str>) -> Option<GgufFacts> {
        self.0.cached(repo, file, revision)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use lattice_core::domain::models::fit::ModelShape;

    fn facts() -> GgufFacts {
        GgufFacts {
            gpu_weight_bytes: 5_000_000_000,
            shape: ModelShape {
                block_count: Some(32),
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn a_header_that_was_read_is_kept_by_repository_file_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let source = Arc::new(HuggingFaceHeaderSource::new(dir.path().to_path_buf()));
        assert_eq!(source.cached("a/b", "m.gguf", None), None);

        source.store("a/b", "m.gguf", None, &facts());

        assert_eq!(source.cached("a/b", "m.gguf", None), Some(facts()));
        assert_eq!(source.cached("a/b", "m.gguf", Some("abc")), None);
        assert_eq!(source.cached("a/b", "other.gguf", None), None);
        let only_cached = CachedHeadersOnly(Arc::clone(&source));
        assert_eq!(
            only_cached.header("a/b", "m.gguf", None).await,
            Some(facts())
        );
        assert_eq!(only_cached.header("a/b", "other.gguf", None).await, None);
    }

    #[test]
    fn the_url_names_the_revision_and_encodes_each_part() {
        assert_eq!(
            HuggingFaceHeaderSource::url("pub/model", "dir/m 1.gguf", Some("abc")),
            "https://huggingface.co/pub/model/resolve/abc/dir/m%201.gguf"
        );
        assert_eq!(
            HuggingFaceHeaderSource::url("pub/model", "m.gguf", None),
            "https://huggingface.co/pub/model/resolve/main/m.gguf"
        );
    }
}
