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
pub(crate) const FIRST_BLOCK_BYTES: u64 = 16 << 20;

/// The most of a file a header read may ask for: a header read previews a file the user has not
/// chosen to download. Four times the largest header among the curated files, which is
/// `gemma-4-E4B-it-Q4_K_M.gguf` at 15,826,723 bytes (read with `scripts/catalog/gguf_pins.py`,
/// 2026-11-07).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const MAX_HEADER_BYTES: u64 = 4 * 15_826_723;

/// The Hub client's own request limit (see `HuggingFaceAdapter`).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct HuggingFaceHeaderSource {
    client: Client,
    cache_dir: PathBuf,
    base_url: String,
    #[allow(dead_code)]
    limit_bytes: u64,
}

impl HuggingFaceHeaderSource {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self::with_limit(cache_dir, "https://huggingface.co", MAX_HEADER_BYTES)
    }

    /// A source that reads from `base_url` and asks for no more than `limit_bytes` of a file.
    pub fn with_limit(cache_dir: PathBuf, base_url: impl Into<String>, limit_bytes: u64) -> Self {
        let client = reqwest_client_builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            cache_dir,
            base_url: base_url.into(),
            limit_bytes,
        }
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

    fn url(&self, repo: &str, file: &str, revision: Option<&str>) -> String {
        let path = |value: &str| {
            value
                .split('/')
                .map(|part| urlencoding::encode(part).into_owned())
                .collect::<Vec<_>>()
                .join("/")
        };
        format!(
            "{}/{}/resolve/{}/{}",
            self.base_url,
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
        let url = self.url(repo, file, revision);
        let mut length = FIRST_BLOCK_BYTES;
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
        let source = HuggingFaceHeaderSource::new(PathBuf::new());
        assert_eq!(
            source.url("pub/model", "dir/m 1.gguf", Some("abc")),
            "https://huggingface.co/pub/model/resolve/abc/dir/m%201.gguf"
        );
        assert_eq!(
            source.url("pub/model", "m.gguf", None),
            "https://huggingface.co/pub/model/resolve/main/m.gguf"
        );
    }

    /// A GGUF file: a header whose token list is `token_bytes` long, then `data_bytes` of weights.
    /// The model is untied, so its input embedding (4,096 bytes) stays on the CPU.
    fn gguf(token_bytes: usize, data_bytes: usize) -> (Vec<u8>, usize) {
        fn string(out: &mut Vec<u8>, value: &str) {
            out.extend_from_slice(&(value.len() as u64).to_le_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        let token = "x".repeat(1024);
        let tokens = token_bytes / (8 + token.len());
        let mut out = Vec::new();
        out.extend_from_slice(&0x4655_4747u32.to_le_bytes());
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&2u64.to_le_bytes());
        out.extend_from_slice(&3u64.to_le_bytes());
        string(&mut out, "general.architecture");
        out.extend_from_slice(&8u32.to_le_bytes());
        string(&mut out, "qwen35");
        string(&mut out, "qwen35.block_count");
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        string(&mut out, "tokenizer.ggml.tokens");
        out.extend_from_slice(&9u32.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(tokens as u64).to_le_bytes());
        for _ in 0..tokens {
            string(&mut out, &token);
        }
        for (name, offset) in [("token_embd.weight", 0u64), ("output.weight", 4096)] {
            string(&mut out, name);
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&8u64.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
        }
        let header_len = out.len();
        out.resize(header_len + data_bytes, 0);
        (out, header_len)
    }

    fn expected(file_len: usize) -> GgufFacts {
        GgufFacts {
            gpu_weight_bytes: file_len as u64 - 4096,
            shape: ModelShape {
                block_count: Some(32),
                ..Default::default()
            },
        }
    }

    /// Serves `file`, honouring `Range`.
    struct RangeServer(Arc<Vec<u8>>);

    impl wiremock::Respond for RangeServer {
        fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
            let len = self.0.len() as u64;
            let range = request
                .headers
                .get("range")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("bytes="))
                .and_then(|value| value.split_once('-'))
                .and_then(|(from, to)| Some((from.parse::<u64>().ok()?, to.parse::<u64>().ok()?)));
            let Some((from, to)) = range.filter(|(from, _)| *from < len) else {
                return wiremock::ResponseTemplate::new(416);
            };
            let to = to.min(len - 1);
            wiremock::ResponseTemplate::new(206)
                .insert_header("content-range", format!("bytes {from}-{to}/{len}").as_str())
                .set_body_bytes(self.0[from as usize..=to as usize].to_vec())
        }
    }

    async fn serve(file: Vec<u8>) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(RangeServer(Arc::new(file)))
            .mount(&server)
            .await;
        server
    }

    /// The `(first, last)` byte of every range asked for, in order.
    async fn ranges(server: &wiremock::MockServer) -> Vec<(u64, u64)> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| {
                let value = request.headers.get("range").unwrap().to_str().unwrap();
                let (from, to) = value
                    .strip_prefix("bytes=")
                    .unwrap()
                    .split_once('-')
                    .unwrap();
                (from.parse().unwrap(), to.parse().unwrap())
            })
            .collect()
    }

    fn source(
        server: &wiremock::MockServer,
        limit: u64,
    ) -> (HuggingFaceHeaderSource, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let source =
            HuggingFaceHeaderSource::with_limit(dir.path().to_path_buf(), server.uri(), limit);
        (source, dir)
    }

    #[tokio::test]
    async fn a_header_within_the_first_block_costs_one_request() {
        let (file, _) = gguf(64 << 10, FIRST_BLOCK_BYTES as usize + (4 << 20));
        let length = file.len();
        let server = serve(file).await;
        let (source, _dir) = source(&server, MAX_HEADER_BYTES);

        let facts = source.header("pub/model", "m.gguf", None).await;

        assert_eq!(facts, Some(expected(length)));
        assert_eq!(ranges(&server).await.len(), 1);
    }

    #[tokio::test]
    async fn a_header_longer_than_the_first_block_is_read_with_the_blocks_it_needs() {
        let (file, header_len) = gguf(FIRST_BLOCK_BYTES as usize + (1 << 20), 8192);
        let length = file.len();
        assert!(header_len as u64 > FIRST_BLOCK_BYTES);
        let server = serve(file).await;
        let (source, _dir) = source(&server, MAX_HEADER_BYTES);

        let facts = source.header("pub/model", "m.gguf", None).await;

        assert_eq!(facts, Some(expected(length)));
        let asked = ranges(&server).await;
        assert!(asked.len() >= 2, "{asked:?}");
        assert!(
            asked.iter().all(|(_, last)| *last < length as u64),
            "{asked:?}"
        );
    }

    #[tokio::test]
    async fn an_invalid_file_stops_after_the_first_block() {
        let server = serve(vec![0u8; 3 * FIRST_BLOCK_BYTES as usize]).await;
        let (source, _dir) = source(&server, MAX_HEADER_BYTES);

        let facts = source.header("pub/model", "m.gguf", None).await;

        assert_eq!(facts, None);
        let asked = ranges(&server).await;
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].1 < FIRST_BLOCK_BYTES, "{asked:?}");
    }

    #[tokio::test]
    async fn nothing_past_the_named_limit_is_requested() {
        let limit = 1 << 20;
        let (file, header_len) = gguf(4 << 20, 8192);
        assert!(header_len as u64 > limit);
        let server = serve(file).await;
        let (source, _dir) = source(&server, limit);

        let facts = source.header("pub/model", "m.gguf", None).await;

        assert_eq!(facts, None);
        let asked = ranges(&server).await;
        assert!(!asked.is_empty());
        assert!(asked.iter().all(|(_, last)| *last < limit), "{asked:?}");
    }
}
