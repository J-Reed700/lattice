//! Embedding servers for tools that run without the app: the retrieval and
//! shipped-path evals and the ignored live tests. The starter spawns a
//! llama-server binary itself, with the flags the app's sidecar gets for the
//! same model, and kills every server it started when it drops.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

use super::{read_header, sidecar_config};
use crate::features::embedding::llama_cpp::{
    EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter, LlamaEmbeddingService,
    RunningEmbeddingServer,
};
use crate::features::llm::engine::sidecar_manager::{build_server_args, new_api_token};
use crate::features::llm::engine::system::backend_devices::parse_list_devices;

/// How often a starting server's `/health` is asked while it loads.
const HEALTH_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Starts embedding servers from one llama-server binary, as child processes
/// of this one.
pub struct StandaloneEmbeddingStarter {
    binary: PathBuf,
    children: parking_lot::Mutex<Vec<Arc<parking_lot::Mutex<Child>>>>,
}

impl StandaloneEmbeddingStarter {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            children: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// The binary `LATTICE_LLAMA_SERVER` names, else the bundled build for
    /// this platform in this crate's `binaries/`.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var_os("LATTICE_LLAMA_SERVER")
                .map(PathBuf::from)
                .unwrap_or_else(bundled_binary),
        )
    }
}

/// The bundled build for this platform, as `binaries/` holds it.
fn bundled_binary() -> PathBuf {
    let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc.exe"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("llama-server-{triple}"))
}

#[async_trait]
impl EmbeddingServerStarter for StandaloneEmbeddingStarter {
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader> {
        read_header(gguf).await
    }

    /// What the binary's own `--list-devices` reports, as the app asks it.
    async fn accelerated(&self) -> bool {
        let binary = self.binary.clone();
        tokio::task::spawn_blocking(move || Command::new(binary).arg("--list-devices").output())
            .await
            .ok()
            .and_then(std::result::Result::ok)
            .is_some_and(|output| {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                parse_list_devices(&text)
                    .iter()
                    .any(|device| device.is_accelerator())
            })
    }

    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningEmbeddingServer>> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|e| AppError::ModelLoadFailed(format!("No free local port: {e}")))?
            .port();
        let key = new_api_token();
        let args = build_server_args(&sidecar_config(launch), port);
        let child = Command::new(&self.binary)
            .args(&args)
            .env("LLAMA_API_KEY", &key)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                AppError::ModelLoadFailed(format!("{} did not spawn: {e}", self.binary.display()))
            })?;
        let child = Arc::new(parking_lot::Mutex::new(child));
        self.children.lock().push(Arc::clone(&child));
        let endpoint = format!("http://127.0.0.1:{port}");
        let http = reqwest::Client::new();
        loop {
            if let Ok(Some(status)) = child.lock().try_wait() {
                return Err(AppError::ModelLoadFailed(format!(
                    "llama-server exited before it was ready ({status}); run it by hand to see \
                     why: {} {}",
                    self.binary.display(),
                    args.join(" ")
                )));
            }
            let health = http
                .get(format!("{endpoint}/health"))
                .bearer_auth(&key)
                .send()
                .await;
            if health.is_ok_and(|response| response.status().is_success()) {
                break;
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }
        let watched = Arc::clone(&child);
        Ok(Arc::new(RunningEmbeddingServer::new(
            endpoint,
            key,
            None,
            Arc::new(move || matches!(watched.lock().try_wait(), Ok(None))),
        )))
    }
}

impl Drop for StandaloneEmbeddingStarter {
    fn drop(&mut self) {
        for child in self.children.lock().drain(..) {
            let mut child = child.lock();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The embedding model in `gguf` (with its `tokenizer.json` beside it) on a
/// llama-server this process starts from [`StandaloneEmbeddingStarter::from_env`]:
/// what the app's loader does for the active model, from a file. The server
/// stops when the service drops.
pub async fn open_gguf(gguf: &Path) -> Result<LlamaEmbeddingService> {
    let identity =
        crate::infrastructure::ml::artifact_identity::compute_in_background(gguf.to_path_buf())
            .await?;
    let name = gguf.file_name().map_or_else(
        || gguf.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    LlamaEmbeddingService::start_gguf(
        Arc::new(StandaloneEmbeddingStarter::from_env()),
        gguf,
        &name,
        &name,
        identity,
    )
    .await
}
