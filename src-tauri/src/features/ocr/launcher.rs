//! Starting the bundled `llama-server` as the OCR server.
//!
//! This is deliberately not the chat sidecar manager. The chat server is
//! resident for hours, shared between roles and tuned for long contexts and
//! speculative decoding, which `--mmproj` rules out. The OCR server is a
//! two-gigabyte process wanted for a minute at a time: spawn, wait for
//! `/health`, kill. What it shares with the chat sidecar is the binary and the
//! safety rules: loopback only, a per-process bearer key passed in the
//! environment, no web UI.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::warn;

use super::model::OcrModel;
use super::server::{RunningServer, ServerLauncher, ServerProcess};
use crate::application::ports::OcrError;

/// Installed name of the bundled server (tauri `externalBin`, triple
/// stripped), and of the CPU-only build bundled on Windows and Linux.
const BUNDLED_BIN: &str = "llama-server";
const BUNDLED_CPU_BIN: &str = "llama-server-cpu";

/// Context per request: the page image costs about 1,250 tokens at the
/// projector's default pixel cap, and a dense page reads back in about 900.
/// The one slot gets all of it.
pub(crate) const OCR_CONTEXT_TOKENS: u32 = 4096;

/// A cold start reads 1.8 GB of weights; a warm one takes under a second.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const HEALTH_POLL: Duration = Duration::from_millis(250);
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Spawns `llama-server` with the OCR model and its projector.
pub struct LlamaServerLauncher {
    /// Builds to try in order; later ones run without GPU offload.
    binaries: Vec<PathBuf>,
    /// A fixed port, for the live smoke check. `None` picks a free one.
    port: Option<u16>,
    log_path: PathBuf,
}

impl LlamaServerLauncher {
    /// The builds bundled next to the app executable.
    pub fn bundled() -> Self {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let binaries = [BUNDLED_BIN, BUNDLED_CPU_BIN]
            .into_iter()
            .map(|name| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
            .collect();
        Self::with_binaries(binaries)
    }

    /// One explicit binary.
    pub fn with_binary(binary: PathBuf) -> Self {
        Self::with_binaries(vec![binary])
    }

    fn with_binaries(binaries: Vec<PathBuf>) -> Self {
        Self {
            binaries,
            port: None,
            // One log per app process, rewritten by each start: the startup
            // failure is what is worth reading, and it is never in our stderr.
            log_path: std::env::temp_dir()
                .join(format!("lattice-ocr-server-{}.log", std::process::id())),
        }
    }

    pub fn on_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    async fn start(
        &self,
        binary: &Path,
        model: &OcrModel,
        gpu_layers: u32,
    ) -> Result<RunningServer, String> {
        let port = match self.port {
            Some(port) => port,
            None => free_port().map_err(|error| format!("no free port: {error}"))?,
        };
        let api_key = new_api_key();
        let log = std::fs::File::create(&self.log_path)
            .map_err(|error| format!("cannot write {}: {error}", self.log_path.display()))?;
        let mut child = tokio::process::Command::new(binary)
            .args(server_args(model, port, gpu_layers))
            // The key goes in the environment, not argv: argv is readable by
            // every process on the machine.
            .env("LLAMA_API_KEY", &api_key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // A file rather than a pipe: llama-server logs every request, and
            // an undrained pipe would eventually block it mid-page.
            .stderr(Stdio::from(log))
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", binary.display()))?;
        let endpoint = format!("http://127.0.0.1:{port}");
        match wait_ready(&mut child, &endpoint, &api_key).await {
            Ok(()) => Ok(RunningServer::new(
                endpoint,
                api_key,
                Box::new(ChildProcess(child)),
            )),
            Err(reason) => {
                let _ = child.start_kill();
                Err(format!("{reason}{}", log_tail(&self.log_path)))
            }
        }
    }
}

#[async_trait]
impl ServerLauncher for LlamaServerLauncher {
    async fn launch(&self, model: &OcrModel) -> Result<RunningServer, OcrError> {
        let mut failures = Vec::new();
        for (index, binary) in self.binaries.iter().enumerate() {
            // repository-barrier-allow: the bundled executable is the resource; the CPU build exists on some platforms only.
            if !binary.is_file() {
                continue;
            }
            // The CPU-only build exists for machines where the accelerated
            // one cannot start; offload makes no sense there.
            let gpu_layers = if index == 0 { 99 } else { 0 };
            // A port taken between choosing it and the server binding it is
            // the one failure worth an immediate retry.
            for _ in 0..2 {
                match self.start(binary, model, gpu_layers).await {
                    Ok(server) => return Ok(server),
                    Err(reason) => {
                        warn!(binary = %binary.display(), %reason, "OCR server did not start");
                        let port_race = self.port.is_none() && reason.contains("bind");
                        failures.push(reason);
                        if !port_race {
                            break;
                        }
                    }
                }
            }
        }
        Err(OcrError::Unavailable {
            reason: if failures.is_empty() {
                "the bundled llama-server was not found".into()
            } else {
                failures.join("; ")
            },
        })
    }
}

/// The OCR server's command line.
pub(crate) fn server_args(model: &OcrModel, port: u16, gpu_layers: u32) -> Vec<String> {
    vec![
        "-m".into(),
        model.weights.to_string_lossy().into_owned(),
        "--mmproj".into(),
        model.projector.to_string_lossy().into_owned(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--ctx-size".into(),
        OCR_CONTEXT_TOKENS.to_string(),
        // One page at a time; see `OcrServer`.
        "-np".into(),
        "1".into(),
        "-ngl".into(),
        gpu_layers.to_string(),
        // The model card runs at temperature 0; requests say so too.
        "--temp".into(),
        "0".into(),
        // Every page is a new image, so the prompt cache only costs memory
        // (its default ceiling is 8 GB).
        "--cache-ram".into(),
        "0".into(),
        "--no-webui".into(),
    ]
}

/// Poll `/health` until it answers 200, the process exits, or time runs out.
async fn wait_ready(
    child: &mut tokio::process::Child,
    endpoint: &str,
    api_key: &str,
) -> Result<(), String> {
    // Loopback never goes through a proxy.
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(HEALTH_PROBE_TIMEOUT)
        .build()
        .map_err(|error| format!("no HTTP client: {error}"))?;
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!("llama-server exited during startup ({status})"));
        }
        let health = http
            .get(format!("{endpoint}/health"))
            .bearer_auth(api_key)
            .send()
            .await;
        if health.is_ok_and(|response| response.status().is_success()) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "llama-server was not ready after {} s",
                READY_TIMEOUT.as_secs()
            ));
        }
        tokio::time::sleep(HEALTH_POLL).await;
    }
}

/// The last lines of the server log, for an error message.
fn log_tail(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().rev().take(6).collect();
    if lines.is_empty() {
        return String::new();
    }
    let tail: Vec<&str> = lines.into_iter().rev().collect();
    format!(": {}", tail.join(" | "))
}

fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

/// 128 bits from the OS, hex-encoded, new for every process.
fn new_api_key() -> String {
    use std::fmt::Write;

    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(32), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// A spawned `llama-server`. `kill_on_drop` covers the paths that never call
/// `kill`.
struct ChildProcess(tokio::process::Child);

impl ServerProcess for ChildProcess {
    fn has_exited(&mut self) -> bool {
        !matches!(self.0.try_wait(), Ok(None))
    }

    fn kill(&mut self) {
        let _ = self.0.start_kill();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_loopback_only_single_slot_and_loads_the_projector() {
        let model = OcrModel::in_dir("ocr", Path::new("/m"), "OCR:");
        let args = server_args(&model, 8096, 99).join(" ");

        assert!(args.contains("-m /m/model.gguf --mmproj /m/mmproj.gguf"));
        assert!(args.contains("--host 127.0.0.1 --port 8096"));
        assert!(args.contains("-np 1"));
        assert!(args.contains("--cache-ram 0"));
        assert!(args.contains("--no-webui"));
        assert!(!args.contains("--api-key"), "the key must not be in argv");
    }

    #[tokio::test]
    async fn a_missing_binary_is_reported_as_unavailable() {
        let launcher = LlamaServerLauncher::with_binary(PathBuf::from("/nonexistent/llama-server"));
        let model = OcrModel::in_dir("ocr", Path::new("/m"), "OCR:");

        let error = launcher.launch(&model).await.err().expect("no server");

        match error {
            OcrError::Unavailable { reason } => assert!(reason.contains("not found"), "{reason}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn api_keys_are_fresh_hex() {
        let (a, b) = (new_api_key(), new_api_key());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
