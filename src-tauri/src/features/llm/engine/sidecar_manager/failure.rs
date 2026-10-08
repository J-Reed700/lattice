use super::*;

/// Why a bundled build cannot run, at the granularity the user can act on.
/// A missing Vulkan runtime and a truncated download both used to be reported
/// as "reinstall Lattice", which fixes only one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnusableReason {
    /// The executable loaded nothing because the machine has no GPU runtime
    /// (`vulkan-1.dll`, `libvulkan.so.1`). Expected on plenty of machines;
    /// the CPU build is the answer, not a reinstall.
    MissingGpuRuntime,
    /// The file itself is wrong: missing dylib, bad architecture, truncated.
    BrokenInstall,
    /// The CPU lacks an instruction set this build was compiled for.
    UnsupportedCpu,
}

impl UnusableReason {
    pub(super) fn advice(self) -> &'static str {
        match self {
            Self::MissingGpuRuntime => {
                "Update your graphics driver to get a Vulkan runtime; until then Lattice runs \
                 local models on the CPU."
            }
            Self::BrokenInstall => "Reinstall Lattice.",
            Self::UnsupportedCpu => {
                "This CPU is missing an instruction set that build needs, so Lattice uses its \
                 compatibility build instead."
            }
        }
    }

    /// Ranks how useful this is as *the* reported error. A missing GPU runtime
    /// on the accelerated build is the least actionable: it is expected, and
    /// when a later attempt fails for its own reason that reason is what the
    /// user needs to read.
    pub(super) fn report_rank(self) -> u8 {
        match self {
            Self::MissingGpuRuntime => 1,
            Self::BrokenInstall | Self::UnsupportedCpu => 3,
        }
    }
}

/// One startup attempt: which build, with or without GPU offload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Attempt {
    pub(super) binary: SidecarBinary,
    pub(super) gpu_offload: bool,
}

impl Attempt {
    pub(super) fn mode(self) -> &'static str {
        if self.gpu_offload {
            "gpu"
        } else {
            "cpu"
        }
    }
}

/// How an attempt failed, as far as the fallback policy cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttemptFailure {
    /// The executable cannot run on this machine at all.
    BinaryUnusable(UnusableReason),
    /// It ran, then died or hung while bringing up its GPU backend.
    GpuInit,
    /// It ran, then died or hung loading the model (or for no
    /// recognizable reason).
    Startup,
    /// The same configuration deserves another go — the port we handed it
    /// was taken. Changing the configuration cannot help, and treating this
    /// as an unrecognized startup failure is what silently dropped a whole
    /// session to CPU-only at half context.
    Retry,
    /// Bad input (missing model file, no free port); retrying won't help.
    Fatal,
}

#[derive(Debug)]
pub(super) struct AttemptError {
    pub(super) kind: AttemptFailure,
    pub(super) message: String,
}

impl AttemptError {
    pub(super) fn fatal(message: String) -> Self {
        Self {
            kind: AttemptFailure::Fatal,
            message,
        }
    }

    pub(super) fn into_llm_error(self) -> LLMError {
        match self.kind {
            AttemptFailure::BinaryUnusable(_) => LLMError::SidecarBinaryUnusable(self.message),
            AttemptFailure::GpuInit | AttemptFailure::Startup | AttemptFailure::Retry => {
                LLMError::GenerationFailed(self.message)
            }
            AttemptFailure::Fatal => LLMError::Other(self.message),
        }
    }

    /// How useful this failure is as *the* reported error. See
    /// [`UnusableReason::report_rank`]; everything that is not an unusable
    /// binary ranks between the two unusable cases, so a missing GPU runtime
    /// never outranks the real reason a later attempt failed.
    pub(super) fn report_rank(&self) -> u8 {
        match self.kind {
            AttemptFailure::BinaryUnusable(reason) => reason.report_rank(),
            _ => 2,
        }
    }
}

/// The fallback policy: what to try after `failed` ended with `failure`.
///
/// | failed attempt      | failure                    | CPU build bundled   | macOS               |
/// |---------------------|----------------------------|---------------------|---------------------|
/// | primary, gpu        | binary unusable            | CPU build           | stop                |
/// | primary, gpu        | GPU init / model startup   | primary, cpu        | primary, cpu        |
/// | primary, cpu        | binary unusable / GPU init | CPU build           | stop                |
/// | primary, cpu        | model startup              | stop                | stop                |
/// | CPU build           | anything                   | stop                | —                   |
/// | any                 | fatal                      | stop                | stop                |
pub(super) fn next_attempt(
    failed: Attempt,
    failure: AttemptFailure,
    cpu_build_bundled: bool,
) -> Option<Attempt> {
    let cpu_build = cpu_build_bundled.then_some(Attempt {
        binary: SidecarBinary::Cpu,
        gpu_offload: false,
    });
    match (failed.binary, failure) {
        (SidecarBinary::Cpu, _) | (_, AttemptFailure::Fatal) => None,
        // A bind collision that survived its retries is not fixed by a
        // different build or a smaller context.
        (_, AttemptFailure::Retry) => None,
        (SidecarBinary::Primary, AttemptFailure::BinaryUnusable(_)) => cpu_build,
        (SidecarBinary::Primary, _) if failed.gpu_offload => Some(Attempt {
            binary: SidecarBinary::Primary,
            gpu_offload: false,
        }),
        (SidecarBinary::Primary, AttemptFailure::GpuInit) => cpu_build,
        (SidecarBinary::Primary, AttemptFailure::Startup) => None,
    }
}

/// The error to report once every attempt has failed: the most actionable
/// one, earliest first among equals — an unusable binary names its fix, but a
/// missing GPU runtime ranks below the reason the CPU build then failed.
/// The remaining attempts are summarized beneath it, one line each.
pub(super) fn reported_error(mut failures: Vec<(Attempt, AttemptError)>) -> LLMError {
    if failures.is_empty() {
        return LLMError::Other("llama-server was never started".to_string());
    }
    let chosen = failures
        .iter()
        .enumerate()
        .max_by_key(|(index, (_, err))| (err.report_rank(), std::cmp::Reverse(*index)))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let (_, mut reported) = failures.remove(chosen);
    if !failures.is_empty() {
        reported.message.push_str("\n\nOther startup attempts:");
        for (attempt, err) in &failures {
            let summary = err.message.lines().next().unwrap_or_default();
            reported.message.push_str(&format!(
                "\n- {} ({}): {}",
                attempt.binary.label(),
                attempt.mode(),
                clip(summary, ERROR_LINE_MAX_BYTES)
            ));
        }
    }
    reported.into_llm_error()
}

/// How a startup attempt ended without reaching readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StartupEnd {
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    EventError(String),
    StreamClosed,
    TimedOut(TimeoutKind),
    /// The drain task went away without reporting.
    MonitorGone,
}

/// Which bound the readiness wait hit. The distinction is the difference
/// between "your model is broken" and "your machine needed longer than we
/// allow", so it is carried into the message rather than flattened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimeoutKind {
    /// Went silent: no output line and no `/health` answer for this long.
    Silence(u64),
    /// Talked the whole time but never became ready.
    Cap(u64),
    /// The budget shared by every fallback attempt ran out.
    Budget(u64),
}

/// Classify a failed attempt from how it ended, the sticky signals seen while
/// it ran, and what it printed.
pub(super) fn startup_failure(
    binary: SidecarBinary,
    end: &StartupEnd,
    output: &[String],
    facts: StartupFacts,
) -> AttemptError {
    let (code, signal) = match end {
        StartupEnd::Exited { code, signal } => (*code, *signal),
        _ => (None, None),
    };
    if let Some(evidence) = binary_unusable_reason(binary, code, signal, output) {
        return AttemptError {
            kind: AttemptFailure::BinaryUnusable(evidence.reason),
            message: binary_unusable_message(binary, &evidence, output),
        };
    }

    if facts.bind_failure {
        return AttemptError {
            kind: AttemptFailure::Retry,
            message: format!(
                "{} could not bind its HTTP port; another process claimed it first.{}",
                binary.label(),
                render_tail(output)
            ),
        };
    }

    if let Some(detail) = output.iter().find_map(|line| {
        line.find("unknown model architecture:")
            .and_then(|index| line.get(index..))
            .map(|detail| clip(detail, ERROR_LINE_MAX_BYTES))
    }) {
        // CPU offload cannot add an architecture to the same executable.
        // Name the incompatibility rather than suggesting a corrupt download.
        return AttemptError::fatal(format!(
            "Lattice's bundled local model engine does not support this model ({detail}). \
             Update Lattice's model engine or select a compatible model. \
             Downloading the same file again or switching to CPU will not add support."
        ));
    }

    // llama.cpp initializes its backends before touching the model, so a
    // crash or hang before the model loader ever spoke happened in backend
    // (GPU) initialization. A plain non-zero exit is an orderly error
    // (bad arguments, unreadable file) and says nothing about the GPU.
    let crashed =
        matches!(end, StartupEnd::TimedOut(_)) || signal.is_some() || code.is_some_and(|c| c < 0);
    let kind = if facts.gpu_failure || (crashed && !facts.model_loader) {
        AttemptFailure::GpuInit
    } else {
        AttemptFailure::Startup
    };

    let what = match end {
        StartupEnd::Exited { .. } => {
            format!("exited during startup ({})", describe_exit(code, signal))
        }
        StartupEnd::EventError(err) => format!("failed while starting: {err}"),
        StartupEnd::StreamClosed => "closed its output before it became ready".to_string(),
        StartupEnd::TimedOut(TimeoutKind::Silence(secs)) => {
            format!("went silent for {secs} s before it became ready")
        }
        StartupEnd::TimedOut(TimeoutKind::Cap(secs)) => {
            format!("did not become ready within {secs} s")
        }
        StartupEnd::TimedOut(TimeoutKind::Budget(secs)) => {
            format!("ran out of the {secs} s budget shared by every startup attempt")
        }
        StartupEnd::MonitorGone => "stopped reporting before it became ready".to_string(),
    };
    let hint = match kind {
        AttemptFailure::GpuInit => "Its GPU backend appears to have failed to initialize.",
        _ => {
            "The model file may be corrupt or unsupported by this llama-server build, \
             or too large for this machine's memory."
        }
    };
    AttemptError {
        kind,
        message: format!("{} {what}. {hint}{}", binary.label(), render_tail(output)),
    }
}

/// Evidence that the executable itself cannot run.
#[derive(Debug, Clone)]
pub(super) struct UnusableEvidence {
    pub(super) reason: UnusableReason,
    /// The specific finding, for the message.
    pub(super) detail: String,
    /// Whether this will say the same thing next time. A loader diagnostic or
    /// an NTSTATUS loader code names the missing piece, so it will. A bare
    /// signal does not: macOS jetsam under memory pressure and an AV/EDR hook
    /// both look exactly like a rejected code signature, and neither survives
    /// a retry — so an inconclusive verdict must never be cached, or one
    /// transient kill disables local models until the app restarts.
    pub(super) conclusive: bool,
}

/// Why the executable itself cannot run, if the evidence says so.
///
/// `code`/`signal` are the exit status (if it exited); `output` is what it
/// printed. Loader diagnostics are the most specific evidence because they
/// name the missing piece, so they are checked first.
pub(super) fn binary_unusable_reason(
    binary: SidecarBinary,
    code: Option<i32>,
    signal: Option<i32>,
    output: &[String],
) -> Option<UnusableEvidence> {
    if let Some(line) = output
        .iter()
        .map(|line| line.trim())
        .find(|line| is_loader_failure_line(line))
    {
        let lower = line.to_ascii_lowercase();
        let reason = if mentions_gpu_runtime(&lower) {
            UnusableReason::MissingGpuRuntime
        } else if lower.contains("illegal instruction") {
            UnusableReason::UnsupportedCpu
        } else {
            UnusableReason::BrokenInstall
        };
        return Some(UnusableEvidence {
            reason,
            detail: clip(line, ERROR_LINE_MAX_BYTES).to_string(),
            conclusive: true,
        });
    }
    if let Some((reason, detail)) = code.and_then(|code| windows_loader_status(binary, code)) {
        return Some(UnusableEvidence {
            reason,
            detail: detail.to_string(),
            conclusive: true,
        });
    }
    match signal {
        Some(SIGILL) => Some(UnusableEvidence {
            reason: UnusableReason::UnsupportedCpu,
            detail: "it was stopped by SIGILL: the CPU lacks an instruction set this build \
                     requires (for example AVX2)"
                .to_string(),
            conclusive: true,
        }),
        // llama-server prints its build banner first thing, so a kill
        // before any output usually means the OS refused to run it (on
        // macOS, typically a rejected code signature) — but memory
        // pressure looks identical, so this verdict is not cached.
        Some(SIGKILL) if output.iter().all(|line| line.trim().is_empty()) => {
            Some(UnusableEvidence {
                reason: UnusableReason::BrokenInstall,
                detail: "the OS killed it before it printed anything (on macOS this usually \
                         means its code signature was rejected, though memory pressure can \
                         look the same)"
                    .to_string(),
                conclusive: false,
            })
        }
        _ => None,
    }
}

/// Whether a loader diagnostic names a GPU runtime rather than part of our
/// own install — a missing `libvulkan.so.1` is the user's driver, not our file.
pub(super) fn mentions_gpu_runtime(lower: &str) -> bool {
    const NEEDLES: &[&str] = &["vulkan", "nvcuda", "libcuda", "amdvlk"];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

pub(super) const SIGILL: i32 = 4;
pub(super) const SIGKILL: i32 = 9;

/// Messages from the dynamic loader or the kernel's exec path. None of
/// them can come from llama-server's own code.
pub(super) fn is_loader_failure_line(line: &str) -> bool {
    const NEEDLES: &[&str] = &[
        // macOS dyld
        "Library not loaded",
        "Symbol not found",
        "incompatible architecture",
        "Bad CPU type in executable",
        // glibc ld.so
        "error while loading shared libraries",
        "symbol lookup error",
        // exec / shells
        "cannot execute binary file",
        "Exec format error",
        "Illegal instruction",
    ];
    NEEDLES.iter().any(|needle| line.contains(needle))
        // ld.so: "version `GLIBC_2.38' not found", likewise GLIBCXX_ / CXXABI_.
        || (line.contains("not found") && (line.contains("GLIBC") || line.contains("CXXABI_")))
}

/// Windows NTSTATUS exit codes raised by the image loader or the CPU before
/// the program's own code runs. They arrive as the `i32` reinterpretation
/// of the `u32` status.
pub(super) fn windows_loader_status(
    binary: SidecarBinary,
    code: i32,
) -> Option<(UnusableReason, &'static str)> {
    match code as u32 {
        // On the accelerated build the missing DLL is `vulkan-1.dll`, which
        // ships with the graphics driver; on the CPU build nothing external
        // is needed, so a missing DLL means our own files are wrong.
        0xC000_0135 => {
            let reason = match binary {
                SidecarBinary::Primary => UnusableReason::MissingGpuRuntime,
                SidecarBinary::Cpu => UnusableReason::BrokenInstall,
            };
            Some((
                reason,
                "a DLL it needs was not found (exit code 0xC0000135, STATUS_DLL_NOT_FOUND; \
                 for the GPU build this is usually a missing vulkan-1.dll)",
            ))
        }
        0xC000_0139 => Some((
            UnusableReason::BrokenInstall,
            "a DLL it needs lacks a required entry point \
             (exit code 0xC0000139, STATUS_ENTRYPOINT_NOT_FOUND)",
        )),
        0xC000_007B => Some((
            UnusableReason::BrokenInstall,
            "it or one of its DLLs is not a valid image for this system \
             (exit code 0xC000007B, STATUS_INVALID_IMAGE_FORMAT)",
        )),
        0xC000_001D => Some((
            UnusableReason::UnsupportedCpu,
            "the CPU lacks an instruction set this build requires, for example AVX2 \
             (exit code 0xC000001D, STATUS_ILLEGAL_INSTRUCTION)",
        )),
        _ => None,
    }
}

/// A line that names a GPU backend together with a failure.
pub(super) fn line_is_gpu_failure(lower: &str) -> bool {
    const BACKENDS: &[&str] = &["vulkan", "vk::", "cuda", "ggml_metal"];
    const FAILURES: &[&str] = &[
        "error",
        "fail",
        "exception",
        "abort",
        "device lost",
        "insufficient",
    ];
    BACKENDS.iter().any(|backend| lower.contains(backend))
        && FAILURES.iter().any(|failure| lower.contains(failure))
}

/// A line saying llama-server could not take the port we gave it.
pub(super) fn line_is_bind_failure(lower: &str) -> bool {
    const NEEDLES: &[&str] = &[
        // b8981: "couldn't bind HTTP server socket, hostname: 127.0.0.1, port: N"
        "couldn't bind",
        "could not bind",
        "failed to bind",
        "error binding",
        "address already in use",
        "address in use",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// A line from llama.cpp's model loader: proof it got past backend init.
pub(super) fn line_is_model_loader(lower: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "llama_model_load",
        "load_tensors:",
        "loading model",
        "model loaded",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// Output lines that name a GPU backend together with a failure. Production
/// reads the sticky signal instead; this is the same predicate over a block.
#[cfg(test)]
pub(super) fn looks_like_gpu_failure(output: &[String]) -> bool {
    output
        .iter()
        .any(|line| line_is_gpu_failure(&line.to_ascii_lowercase()))
}

pub(super) fn binary_unusable_message(
    binary: SidecarBinary,
    evidence: &UnusableEvidence,
    output: &[String],
) -> String {
    // The fetch script is internal tooling; it belongs in a developer build's
    // message, never in a user's settings row.
    let developer_hint =
        if cfg!(debug_assertions) && evidence.reason == UnusableReason::BrokenInstall {
            " (developers: run src-tauri/scripts/fetch-llama-binaries.sh)"
        } else {
            ""
        };
    format!(
        "Lattice's bundled {} can't run on this machine: {}. {}{developer_hint}{}",
        binary.label(),
        evidence.detail,
        evidence.reason.advice(),
        render_tail(output)
    )
}

/// `tauri-plugin-shell` could not start the sidecar at all.
pub(super) fn spawn_failure(
    binary: SidecarBinary,
    err: &tauri_plugin_shell::Error,
) -> AttemptError {
    let path = resolved_sidecar_path(binary);
    let missing = matches!(err, tauri_plugin_shell::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound)
        && !path.exists();
    let detail = if missing {
        format!("{} does not exist", path.display())
    } else {
        format!("starting {} failed: {err}", path.display())
    };
    let evidence = UnusableEvidence {
        reason: UnusableReason::BrokenInstall,
        detail,
        conclusive: true,
    };
    AttemptError {
        kind: AttemptFailure::BinaryUnusable(evidence.reason),
        message: binary_unusable_message(binary, &evidence, &[]),
    }
}

/// Where tauri-plugin-shell looks for a sidecar: next to the app
/// executable, with `.exe` on Windows.
///
/// The result is only ever shown to somebody — it goes into the "reinstall
/// Lattice" error text and the preflight log line — so it is deliberately
/// [`plainly_spelled`], never Windows' verbatim form.
pub(super) fn resolved_sidecar_path(binary: SidecarBinary) -> PathBuf {
    let file_name = format!(
        "{}{}",
        binary.label(),
        if cfg!(windows) { ".exe" } else { "" }
    );
    tauri::utils::platform::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file_name)))
        .map(plainly_spelled)
        .unwrap_or_else(|| PathBuf::from(file_name))
}

/// A path spelled the way a person writes it, for messages and logs.
///
/// `tauri::utils::platform::current_exe` canonicalises, to resolve symlinks
/// before trusting the executable's location, and on Windows canonicalisation
/// yields a *verbatim* path: `\\?\D:\Program Files\Lattice\llama-server.exe`.
/// That prefix turns off the Win32 path parser, which is why nobody should be
/// asked to read it, retype it or paste it into a support thread — and why it
/// is worth stripping before the path reaches a string, not merely cosmetic:
/// plenty of programs and older Win32 entry points reject `\\?\` outright.
#[cfg(not(windows))]
pub(super) fn plainly_spelled(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
pub(super) fn plainly_spelled(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };
    // Only the drive form is safe to simplify. Verbatim UNC (`\\?\UNC\…`) and
    // device paths (`\\?\PIPE\…`) are not the same path without their prefix,
    // so they keep it even though they read badly.
    let Prefix::VerbatimDisk(letter) = prefix.kind() else {
        return path;
    };
    // The prefix is consumed; `RootDir` is the separator we just rewrote.
    let mut plain = PathBuf::from(format!("{}:\\", letter as char));
    plain.extend(components.filter(|component| !matches!(component, Component::RootDir)));
    plain
}

pub(super) fn describe_exit(code: Option<i32>, signal: Option<i32>) -> String {
    match (code, signal) {
        (_, Some(signal)) => format!("signal {signal}"),
        (Some(code), None) if code < 0 => format!("exit code 0x{:08X}", code as u32),
        (Some(code), None) => format!("exit code {code}"),
        (None, None) => "no exit status".to_string(),
    }
}

/// The end of the captured output, formatted to follow an error sentence.
pub(super) fn render_tail(output: &[String]) -> String {
    let lines: Vec<&str> = output
        .iter()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let skip = lines.len().saturating_sub(ERROR_TAIL_LINES);
    let mut rendered = format!(
        "\n\nllama-server output (last {} lines):",
        lines.len() - skip
    );
    for line in lines.iter().skip(skip) {
        rendered.push_str("\n  ");
        rendered.push_str(clip(line, ERROR_LINE_MAX_BYTES));
    }
    rendered
}

/// `text` cut to at most `max_bytes`, on a char boundary.
pub(super) fn clip(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// Bounded tail of a sidecar's startup output, stderr and stdout
/// interleaved in arrival order.
#[derive(Debug, Default)]
pub(super) struct OutputTail {
    pub(super) lines: VecDeque<String>,
    pub(super) bytes: usize,
}

impl OutputTail {
    pub(super) fn push(&mut self, line: &str) {
        let line = clip(line, OUTPUT_LINE_MAX_BYTES);
        self.bytes += line.len();
        self.lines.push_back(line.to_string());
        while self.lines.len() > OUTPUT_TAIL_MAX_LINES || self.bytes > OUTPUT_TAIL_MAX_BYTES {
            match self.lines.pop_front() {
                Some(dropped) => self.bytes -= dropped.len(),
                None => break,
            }
        }
    }

    pub(super) fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

/// Facts about a starting sidecar, recorded as its output streams and never
/// forgotten.
///
/// Classification cannot read these off [`OutputTail`]: the tail keeps the
/// last 40 lines, and a large model prints dozens of `load_tensors:` lines
/// after the loader banner, so by the time an attempt fails the proof that
/// the model loader ran has rolled off — and a healthy GPU machine got walked
/// down the degradation ladder for it. Symmetrically, a Vulkan error in the
/// first seconds of a long wait was missed. The tail stays purely for the
/// human-readable report.
#[derive(Debug, Default)]
pub(super) struct StartupSignals {
    /// Output lines seen. The readiness wait watches this to tell a slow
    /// start from a hung one.
    pub(super) lines: AtomicU64,
    pub(super) model_loader: AtomicBool,
    pub(super) gpu_failure: AtomicBool,
    pub(super) bind_failure: AtomicBool,
}

impl StartupSignals {
    pub(super) fn observe(&self, line: &str) {
        self.lines.fetch_add(1, Ordering::Relaxed);
        let lower = line.to_ascii_lowercase();
        // Each flag is sticky: once seen, always true.
        if !self.model_loader.load(Ordering::Relaxed) && line_is_model_loader(&lower) {
            self.model_loader.store(true, Ordering::Relaxed);
        }
        if !self.gpu_failure.load(Ordering::Relaxed) && line_is_gpu_failure(&lower) {
            self.gpu_failure.store(true, Ordering::Relaxed);
        }
        if !self.bind_failure.load(Ordering::Relaxed) && line_is_bind_failure(&lower) {
            self.bind_failure.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn line_count(&self) -> u64 {
        self.lines.load(Ordering::Relaxed)
    }

    pub(super) fn facts(&self) -> StartupFacts {
        StartupFacts {
            model_loader: self.model_loader.load(Ordering::Relaxed),
            gpu_failure: self.gpu_failure.load(Ordering::Relaxed),
            bind_failure: self.bind_failure.load(Ordering::Relaxed),
        }
    }
}

/// The sticky signals, as classification reads them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartupFacts {
    pub(super) model_loader: bool,
    pub(super) gpu_failure: bool,
    pub(super) bind_failure: bool,
}

impl StartupFacts {
    /// Derive the same facts from a captured block of output, so a test can
    /// classify without driving the drain task.
    #[cfg(test)]
    pub(super) fn scan(output: &[String]) -> Self {
        let signals = StartupSignals::default();
        for line in output {
            signals.observe(line);
        }
        signals.facts()
    }
}
