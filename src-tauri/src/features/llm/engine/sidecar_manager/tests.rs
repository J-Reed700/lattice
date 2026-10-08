use super::*;
use tauri_plugin_shell::process::TerminatedPayload;

#[test]
fn reserve_free_port_returns_nonzero_loopback_port() {
    let reservation = reserve_free_port().expect("port allocation failed");
    assert!(
        reservation.port() > 0,
        "expected a non-zero port, got {}",
        reservation.port()
    );
    let port = reservation.port();
    assert!(RESERVED_PORTS.lock().contains(&port));
    drop(reservation);
    assert!(!RESERVED_PORTS.lock().contains(&port));
}

#[test]
fn reserve_free_port_never_hands_out_a_live_reservation() {
    // The OS is free to hand back the same just-released ephemeral port;
    // the reservation set is what stops two sidecars sharing it.
    let first = reserve_free_port().expect("first allocation");
    let second = reserve_free_port().expect("second allocation");
    assert_ne!(first.port(), second.port());
}

#[test]
fn sidecar_config_for_model_uses_gpu_defaults() {
    let cfg = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
    assert_eq!(cfg.n_gpu_layers, 99);
    assert_eq!(cfg.context_size, 8192);
}

#[test]
fn without_gpu_offload_disables_gpu_and_caps_context() {
    let gpu = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
    let cpu = gpu.without_gpu_offload();
    assert_eq!(cpu.n_gpu_layers, 0);
    assert_eq!(cpu.context_size, DEFAULT_CPU_CONTEXT_SIZE);
    assert_eq!(cpu.model_path, gpu.model_path);

    // A low-RAM window stays low rather than growing to the CPU default.
    let low_ram = SidecarConfig {
        context_size: LOW_RAM_CONTEXT_SIZE,
        ..gpu
    };
    assert_eq!(
        low_ram.without_gpu_offload().context_size,
        LOW_RAM_CONTEXT_SIZE
    );
}

fn make_caps(
    gpu_vendor: Option<crate::features::llm::engine::system::GPUVendor>,
    ram_gb: f64,
) -> SystemCapabilities {
    use crate::features::llm::engine::system::{GPUInfo, Platform};
    SystemCapabilities {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb,
        gpu: gpu_vendor.map(|vendor| GPUInfo {
            vendor,
            name: format!("{:?} test GPU", vendor),
            vram_gb: None,
            compute_capability: None,
        }),
        cpu_cores: 8,
        cpu_threads: 16,
        cpu_model: Some("test cpu".to_string()),
        cpu_architecture: Some("x86_64".to_string()),
        platform: Platform::Linux,
        os_version: None,
    }
}

/// Sharing is keyed on the whole config, so two roles only reuse one
/// server if their configs compare equal. That holds because
/// `from_capabilities` is a pure function of the path and the machine —
/// if it ever grows a nondeterministic input (a timestamp, a port, a
/// counter), every role silently gets its own copy of the weights again
/// and the only symptom is memory.
#[test]
fn the_same_model_on_one_machine_is_one_key() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Apple), 16.0);
    let chat = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    let utility = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(
        chat, utility,
        "the roles must agree on the key or they will not share"
    );

    let other_model = SidecarConfig::from_capabilities(PathBuf::from("/tmp/other.gguf"), &caps, 0);
    assert_ne!(chat, other_model, "a different model needs its own server");

    // A degraded fallback is a different server, not the same one with a
    // note on it: it holds a different context window.
    assert_ne!(chat, chat.without_gpu_offload());
}

#[test]
fn from_capabilities_apple_silicon_uses_full_offload() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Apple), 16.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(cfg.n_gpu_layers, 99);
    assert_eq!(cfg.context_size, DEFAULT_GPU_CONTEXT_SIZE);
}

#[test]
fn from_capabilities_nvidia_uses_full_offload() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Nvidia), 32.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(cfg.n_gpu_layers, 99);
    assert_eq!(cfg.context_size, DEFAULT_GPU_CONTEXT_SIZE);
}

#[test]
fn from_capabilities_no_gpu_uses_cpu() {
    let caps = make_caps(None, 16.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(cfg.n_gpu_layers, 0);
    assert_eq!(cfg.context_size, DEFAULT_CPU_CONTEXT_SIZE);
}

#[test]
fn from_capabilities_unknown_gpu_treated_as_no_acceleration() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Unknown), 16.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    // Unknown vendor → is_accelerated() returns false → CPU path
    assert_eq!(cfg.n_gpu_layers, 0);
}

#[test]
fn from_capabilities_low_ram_shrinks_context() {
    use crate::features::llm::engine::system::GPUVendor;
    // GPU present + low RAM: still no GPU offload concern, but
    // context shrinks to keep KV cache manageable.
    let caps = make_caps(Some(GPUVendor::Apple), 4.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(cfg.n_gpu_layers, 99);
    assert_eq!(cfg.context_size, LOW_RAM_CONTEXT_SIZE);
}

#[test]
fn from_capabilities_low_ram_no_gpu_squeezes_both() {
    let caps = make_caps(None, 4.0);
    let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 0);
    assert_eq!(cfg.n_gpu_layers, 0);
    assert_eq!(cfg.context_size, LOW_RAM_CONTEXT_SIZE);
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn exited(code: Option<i32>, signal: Option<i32>) -> StartupEnd {
    StartupEnd::Exited { code, signal }
}

/// The readiness wait's silence bound, as the classifier sees it.
const TIMED_OUT: StartupEnd = StartupEnd::TimedOut(TimeoutKind::Silence(90));

/// Classify using the signals the drain task would have recorded from the
/// same output, so tests exercise the sticky path production uses.
fn classify(binary: SidecarBinary, end: &StartupEnd, output: &[String]) -> AttemptError {
    startup_failure(binary, end, output, StartupFacts::scan(output))
}

const BROKEN: AttemptFailure = AttemptFailure::BinaryUnusable(UnusableReason::BrokenInstall);

/// Captured verbatim from the CI-built macOS binary that linked
/// against dylibs which only existed on the build machine.
const DYLD_LIBRARY_NOT_LOADED: &str = "\
dyld[1006]: Library not loaded: @rpath/libllama-common.0.dylib
  Referenced from: <E07A64A7-7995-3D31-9684-B8F16EF758A6> /Users/josh/Code/lattice-temp/src-tauri/binaries/llama-server-aarch64-apple-darwin
  Reason: tried: '/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/System/Volumes/Preboot/Cryptexes/OS/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/System/Volumes/Preboot/Cryptexes/OS/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file)";

const DYLD_SYMBOL_NOT_FOUND: &str = "\
dyld[4242]: Symbol not found: _ggml_backend_dev_by_type
  Referenced from: <1B2C3D4E-0000-1111-2222-333344445555> /Applications/Lattice.app/Contents/MacOS/llama-server
  Expected in:     <99887766-5544-3322-1100-AABBCCDDEEFF> /Applications/Lattice.app/Contents/Frameworks/libggml-base.dylib";

const LINUX_MISSING_VULKAN_LOADER: &str = "/usr/lib/lattice/llama-server: error while loading shared libraries: libvulkan.so.1: cannot open shared object file: No such file or directory";

const LINUX_OLD_GLIBC: &str = "/usr/lib/lattice/llama-server: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by /usr/lib/lattice/llama-server)";

const LINUX_OLD_LIBSTDCXX: &str = "/usr/lib/lattice/llama-server: /lib/x86_64-linux-gnu/libstdc++.so.6: version `GLIBCXX_3.4.32' not found (required by /usr/lib/lattice/llama-server)";

const CORRUPT_MODEL: &str = "\
build: 6500 (d775992) with cc (GCC) 13.2.0 for x86_64-linux-gnu
system info: n_threads = 8, n_threads_batch = 8, total_threads = 16
main: binding port with default address family
main: loading model
srv    load_model: loading model '/models/broken.gguf'
gguf_init_from_file_impl: invalid magic characters: 'lmth', expected 'GGUF'
llama_model_load: error loading model: llama_model_loader: failed to load model from /models/broken.gguf
llama_model_load_from_file_impl: failed to load model
srv    load_model: failed to load model, '/models/broken.gguf'
main: exiting due to model loading error";

const VULKAN_DEVICE_LOST: &str = "\
build: 6500 (d775992) with cc (GCC) 13.2.0 for x86_64-linux-gnu
ggml_vulkan: Found 1 Vulkan devices:
ggml_vulkan: 0 = Intel(R) UHD Graphics 620 (Intel Corporation) | uma: 1 | fp16: 1 | bf16: 0 | warp: 32 | shared memory: 1 | int dot: 0 | matrix cores: none
terminate called after throwing an instance of 'vk::DeviceLostError'
  what():  vk::Device::waitForFences: ErrorDeviceLost";

const BUILD_BANNER: &str =
    "build: 6500 (d775992) with MSVC 19.44.35211.0 for x64\nsystem info: n_threads = 8";

#[test]
fn dyld_library_not_loaded_is_binary_unusable() {
    let output = lines(DYLD_LIBRARY_NOT_LOADED);
    let err = classify(SidecarBinary::Primary, &exited(None, Some(6)), &output);
    assert_eq!(err.kind, BROKEN);
    assert!(
        err.message.starts_with(
            "Lattice's bundled llama-server can't run on this machine: \
             dyld[1006]: Library not loaded: @rpath/libllama-common.0.dylib. \
             Reinstall Lattice."
        ),
        "{}",
        err.message
    );
    // The path to the fetch script is developer tooling, never user advice.
    assert_eq!(
        err.message.contains("fetch-llama-binaries.sh"),
        cfg!(debug_assertions)
    );
    // The tail rides along for diagnostics, clipped per line.
    assert!(err.message.contains("llama-server output (last 3 lines):"));
    assert!(err.message.contains("Reason: tried: '/Users/runner/work/"));
    assert!(!err.message.contains("GGUF"), "must not blame the model");
}

#[test]
fn loader_and_exec_failures_are_binary_unusable() {
    let cases: &[(&str, Option<i32>, Option<i32>, &str)] = &[
        (
            DYLD_SYMBOL_NOT_FOUND,
            None,
            Some(6),
            "dyld[4242]: Symbol not found: _ggml_backend_dev_by_type",
        ),
        (
            LINUX_MISSING_VULKAN_LOADER,
            Some(127),
            None,
            "error while loading shared libraries: libvulkan.so.1",
        ),
        (LINUX_OLD_GLIBC, Some(1), None, "version `GLIBC_2.38' not found"),
        (
            LINUX_OLD_LIBSTDCXX,
            Some(1),
            None,
            "version `GLIBCXX_3.4.32' not found",
        ),
        (
            "bash: ./llama-server: cannot execute binary file: Exec format error",
            Some(126),
            None,
            "cannot execute binary file",
        ),
        (
            "/usr/lib/lattice/llama-server: symbol lookup error: /usr/lib/lattice/libggml-vulkan.so: undefined symbol: ggml_backend_buffer_init",
            Some(127),
            None,
            "undefined symbol: ggml_backend_buffer_init",
        ),
    ];
    for (output, code, signal, expected_reason) in cases {
        let evidence =
            binary_unusable_reason(SidecarBinary::Primary, *code, *signal, &lines(output))
                .unwrap_or_else(|| panic!("not classified as unusable: {output}"));
        assert!(
            evidence.detail.contains(expected_reason),
            "detail {:?} lacks {expected_reason:?}",
            evidence.detail
        );
        assert!(evidence.conclusive, "loader evidence is deterministic");
        let err = classify(
            SidecarBinary::Primary,
            &exited(*code, *signal),
            &lines(output),
        );
        assert!(
            matches!(err.kind, AttemptFailure::BinaryUnusable(_)),
            "{output}"
        );
    }
}

#[test]
fn windows_loader_exit_codes_are_binary_unusable() {
    let cases = [
        (0xC000_0135_u32, "STATUS_DLL_NOT_FOUND"),
        (0xC000_0139_u32, "STATUS_ENTRYPOINT_NOT_FOUND"),
        (0xC000_007B_u32, "STATUS_INVALID_IMAGE_FORMAT"),
        (0xC000_001D_u32, "AVX2"),
    ];
    for (status, expected) in cases {
        // The loader fails before the program prints anything.
        let err = classify(SidecarBinary::Cpu, &exited(Some(status as i32), None), &[]);
        assert!(matches!(err.kind, AttemptFailure::BinaryUnusable(_)));
        assert!(err.message.contains(expected), "{}", err.message);
        assert!(err
            .message
            .starts_with("Lattice's bundled llama-server-cpu can't run on this machine: "));
    }
    // A missing DLL is the graphics driver's business on the GPU build and
    // our own on the CPU build, and each gets its own advice.
    let gpu = classify(
        SidecarBinary::Primary,
        &exited(Some(0xC000_0135_u32 as i32), None),
        &[],
    );
    assert_eq!(
        gpu.kind,
        AttemptFailure::BinaryUnusable(UnusableReason::MissingGpuRuntime)
    );
    assert!(gpu.message.contains("Update your graphics driver"));
    assert!(!gpu.message.contains("Reinstall Lattice"));
    assert!(windows_loader_status(SidecarBinary::Primary, 1).is_none());
    // STATUS_ACCESS_VIOLATION is a crash inside the program, not a loader failure.
    assert!(windows_loader_status(SidecarBinary::Primary, 0xC000_0005_u32 as i32).is_none());
}

#[test]
fn unix_signals_that_mean_the_binary_cannot_run() {
    // SIGILL after the banner: the CPU lacks an instruction set.
    let primary = SidecarBinary::Primary;
    let evidence = binary_unusable_reason(primary, None, Some(SIGILL), &lines(BUILD_BANNER))
        .expect("SIGILL is unusable");
    assert!(evidence.detail.contains("AVX2"));
    assert_eq!(evidence.reason, UnusableReason::UnsupportedCpu);
    // SIGKILL before any output: the OS refused to run it — but memory
    // pressure looks the same, so the verdict must not be cached.
    let signal_only =
        binary_unusable_reason(primary, None, Some(SIGKILL), &[]).expect("SIGKILL is unusable");
    assert!(
        !signal_only.conclusive,
        "signal-only evidence is inconclusive"
    );
    // SIGKILL after output is an OOM kill or similar, not the binary.
    assert!(binary_unusable_reason(primary, None, Some(SIGKILL), &lines(BUILD_BANNER)).is_none());
    // A plain abort without a loader message says nothing about the binary.
    assert!(binary_unusable_reason(primary, None, Some(6), &lines(BUILD_BANNER)).is_none());
}

#[test]
fn corrupt_model_is_a_startup_failure_not_a_binary_failure() {
    let output = lines(CORRUPT_MODEL);
    let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &output);
    assert_eq!(err.kind, AttemptFailure::Startup);
    assert!(err.message.starts_with(
        "llama-server exited during startup (exit code 1). The model file may be corrupt"
    ));
    assert!(err.message.contains("invalid magic characters"));
    assert!(matches!(
        err.into_llm_error(),
        LLMError::GenerationFailed(_)
    ));
}

#[test]
fn unknown_architectures_report_the_engine_mismatch_without_cpu_retries() {
    for architecture in ["laguna", "future-model-family"] {
        let output = lines(&format!(
            "llama_model_load: error loading model architecture: unknown model architecture: '{architecture}'"
        ));
        let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &output);
        assert_eq!(err.kind, AttemptFailure::Fatal);
        assert!(err.message.contains(architecture));
        assert!(err.message.contains("Update Lattice's model engine"));
        assert!(!err.message.contains("corrupt"));
        assert_eq!(
            next_attempt(
                Attempt {
                    binary: SidecarBinary::Primary,
                    gpu_offload: true
                },
                err.kind,
                true,
            ),
            None
        );
    }
}

#[test]
fn gpu_backend_failures_are_classified_as_gpu_init() {
    let err = classify(
        SidecarBinary::Primary,
        &exited(None, Some(6)),
        &lines(VULKAN_DEVICE_LOST),
    );
    assert_eq!(err.kind, AttemptFailure::GpuInit);
    assert!(err.message.contains("GPU backend"));

    // Windows: abort() surfaces as STATUS_STACK_BUFFER_OVERRUN.
    let err = classify(
        SidecarBinary::Primary,
        &exited(Some(0xC000_0409_u32 as i32), None),
        &lines("ggml_vulkan: Device memory allocation of size 1073741824 failed."),
    );
    assert_eq!(err.kind, AttemptFailure::GpuInit);
    assert!(
        err.message.contains("exit code 0xC0000409"),
        "{}",
        err.message
    );

    // A crash or hang before the model loader spoke happened in backend init.
    for end in [exited(None, Some(11)), TIMED_OUT] {
        let err = classify(SidecarBinary::Primary, &end, &lines(BUILD_BANNER));
        assert_eq!(err.kind, AttemptFailure::GpuInit, "{end:?}");
    }
}

#[test]
fn failures_after_model_load_began_are_startup_failures() {
    let mut output = lines(BUILD_BANNER);
    output.push("llama_model_loader: loaded meta data with 30 key-value pairs".to_string());
    for end in [
        exited(None, Some(11)),
        exited(None, Some(SIGKILL)),
        TIMED_OUT,
        StartupEnd::StreamClosed,
        StartupEnd::EventError("wait failed".to_string()),
    ] {
        let err = classify(SidecarBinary::Primary, &end, &output);
        assert_eq!(err.kind, AttemptFailure::Startup, "{end:?}");
    }
    // An orderly non-zero exit is not a crash, banner or not.
    let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &[]);
    assert_eq!(err.kind, AttemptFailure::Startup);
    assert!(err.message.contains("exit code 1"));
}

#[test]
fn normal_vulkan_banner_is_not_a_gpu_failure() {
    let banner = lines(
        "ggml_vulkan: Found 1 Vulkan devices:\n\
         ggml_vulkan: 0 = NVIDIA GeForce RTX 3060 (NVIDIA) | uma: 0 | fp16: 1 | bf16: 0 | warp: 32 | shared memory: 0 | int dot: 1 | matrix cores: KHR_coopmat\n\
         ggml_metal_device_init: GPU name:   MTL0 (Apple M3 Max)",
    );
    assert!(!looks_like_gpu_failure(&banner));
    assert!(!looks_like_gpu_failure(&lines(CORRUPT_MODEL)));
}

#[test]
fn spawn_failures_are_binary_unusable_and_name_the_path() {
    let not_found = tauri_plugin_shell::Error::Io(std::io::ErrorKind::NotFound.into());
    let err = spawn_failure(SidecarBinary::Cpu, &not_found);
    assert_eq!(err.kind, BROKEN);
    let expected = resolved_sidecar_path(SidecarBinary::Cpu);
    assert!(
        err.message.starts_with(&format!(
            "Lattice's bundled llama-server-cpu can't run on this machine: {} does not exist. \
             Reinstall Lattice",
            expected.display()
        )),
        "{}",
        err.message
    );
    assert!(matches!(
        err.into_llm_error(),
        LLMError::SidecarBinaryUnusable(_)
    ));

    let denied = tauri_plugin_shell::Error::Io(std::io::ErrorKind::PermissionDenied.into());
    let err = spawn_failure(SidecarBinary::Primary, &denied);
    assert_eq!(err.kind, BROKEN);
    assert!(err.message.contains("can't run on this machine: starting "));
    // The two properties worth holding: the message names the exact file we
    // tried to start, and it passes the OS's own explanation through. The
    // wording of that explanation is the OS's business — POSIX says
    // "permission denied" where Windows says "Access is denied. (os error
    // 5)" — so it is asserted against the error's own `Display`, never
    // spelled out here. Likewise the path is taken from
    // `resolved_sidecar_path`, so the `.exe` suffix Windows adds is part of
    // the expectation instead of breaking it.
    let expected = resolved_sidecar_path(SidecarBinary::Primary);
    assert!(
        err.message
            .contains(&format!("starting {} failed: {denied}", expected.display())),
        "{}",
        err.message
    );
}

#[test]
fn resolved_sidecar_path_is_the_installed_name_next_to_the_executable() {
    let exe_dir = std::env::current_exe()
        .expect("current exe")
        .parent()
        .expect("exe dir")
        .to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    for (binary, name) in [
        (SidecarBinary::Primary, format!("llama-server{suffix}")),
        (SidecarBinary::Cpu, format!("llama-server-cpu{suffix}")),
    ] {
        let path = resolved_sidecar_path(binary);
        assert_eq!(path.file_name(), Some(std::ffi::OsStr::new(name.as_str())));
        // Compare the directories canonically rather than as strings: the
        // production side resolves symlinks (and on macOS `/var` really is
        // a symlink to `/private/var`), so two correct answers can be
        // spelled differently. Canonicalising both sides asks the only
        // question the test cares about — is this the *same* directory the
        // test binary is in.
        assert_eq!(
            canonical_or_raw(path.parent().expect("sidecar dir")),
            canonical_or_raw(&exe_dir),
            "{}",
            path.display()
        );
        // ...and the spelling handed onward stays fit to show a user, i.e.
        // not the `\\?\D:\…` form canonicalisation returns on Windows.
        assert!(
            !path.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            path.display()
        );
    }
}

const GPU: Attempt = Attempt {
    binary: SidecarBinary::Primary,
    gpu_offload: true,
};
const PRIMARY_CPU: Attempt = Attempt {
    binary: SidecarBinary::Primary,
    gpu_offload: false,
};
const CPU_BUILD: Attempt = Attempt {
    binary: SidecarBinary::Cpu,
    gpu_offload: false,
};

#[test]
fn fallback_policy_with_cpu_build_bundled() {
    use AttemptFailure::*;
    let table = [
        (GPU, BROKEN, Some(CPU_BUILD)),
        (GPU, GpuInit, Some(PRIMARY_CPU)),
        (GPU, Startup, Some(PRIMARY_CPU)),
        // A port collision is retried by the caller, not degraded here.
        (GPU, Retry, None),
        (GPU, Fatal, None),
        (PRIMARY_CPU, BROKEN, Some(CPU_BUILD)),
        (PRIMARY_CPU, GpuInit, Some(CPU_BUILD)),
        (PRIMARY_CPU, Startup, None),
        (PRIMARY_CPU, Retry, None),
        (PRIMARY_CPU, Fatal, None),
        (CPU_BUILD, BROKEN, None),
        (CPU_BUILD, GpuInit, None),
        (CPU_BUILD, Startup, None),
        (CPU_BUILD, Retry, None),
        (CPU_BUILD, Fatal, None),
    ];
    for (failed, failure, expected) in table {
        assert_eq!(
            next_attempt(failed, failure, true),
            expected,
            "{failed:?} / {failure:?}"
        );
    }
}

#[test]
fn fallback_policy_without_cpu_build() {
    use AttemptFailure::*;
    let table = [
        // The incident: no same-binary retry for a binary that cannot run.
        (GPU, BROKEN, None),
        (GPU, GpuInit, Some(PRIMARY_CPU)),
        (GPU, Startup, Some(PRIMARY_CPU)),
        (GPU, Retry, None),
        (GPU, Fatal, None),
        (PRIMARY_CPU, BROKEN, None),
        (PRIMARY_CPU, GpuInit, None),
        (PRIMARY_CPU, Startup, None),
        (PRIMARY_CPU, Retry, None),
        (PRIMARY_CPU, Fatal, None),
    ];
    for (failed, failure, expected) in table {
        assert_eq!(
            next_attempt(failed, failure, false),
            expected,
            "{failed:?} / {failure:?}"
        );
    }
}

#[test]
fn fallback_chain_always_terminates_without_repeating() {
    use AttemptFailure::*;
    let failures = [BROKEN, GpuInit, Startup, Retry, Fatal];
    for bundled in [true, false] {
        for start in [GPU, PRIMARY_CPU] {
            // Every sequence of failures, three deep (the longest chain).
            for a in failures {
                for b in failures {
                    for c in failures {
                        let mut seen = vec![start];
                        let mut current = start;
                        for failure in [a, b, c] {
                            match next_attempt(current, failure, bundled) {
                                Some(next) => {
                                    assert!(!seen.contains(&next), "repeat of {next:?}");
                                    seen.push(next);
                                    current = next;
                                }
                                None => break,
                            }
                        }
                        assert!(seen.len() <= 3);
                        if !bundled {
                            assert!(!seen.contains(&CPU_BUILD));
                        }
                    }
                }
            }
        }
    }
}

fn attempt_error(kind: AttemptFailure, message: &str) -> AttemptError {
    AttemptError {
        kind,
        message: message.to_string(),
    }
}

#[test]
fn reported_error_prefers_binary_unusable_and_summarizes_the_rest() {
    let err = reported_error(vec![
        (
            GPU,
            attempt_error(AttemptFailure::GpuInit, "llama-server crashed\n\noutput"),
        ),
        (
            PRIMARY_CPU,
            attempt_error(AttemptFailure::GpuInit, "llama-server crashed again"),
        ),
        (
            CPU_BUILD,
            attempt_error(
                AttemptFailure::BinaryUnusable(UnusableReason::UnsupportedCpu),
                "Lattice's bundled llama-server-cpu can't run on this machine: AVX2",
            ),
        ),
    ]);
    let message = match err {
        LLMError::SidecarBinaryUnusable(message) => message,
        other => panic!("expected SidecarBinaryUnusable, got {other:?}"),
    };
    assert!(message.starts_with("Lattice's bundled llama-server-cpu can't run"));
    assert!(message.contains("\n- llama-server (gpu): llama-server crashed\n"));
    assert!(message.ends_with("\n- llama-server (cpu): llama-server crashed again"));
    assert!(
        !message.contains("output"),
        "only first lines are summarized"
    );
}

#[test]
fn reported_error_falls_back_to_the_requested_configuration() {
    let err = reported_error(vec![
        (GPU, attempt_error(AttemptFailure::Startup, "first")),
        (
            PRIMARY_CPU,
            attempt_error(AttemptFailure::Startup, "second"),
        ),
    ]);
    assert!(
        matches!(&err, LLMError::GenerationFailed(m) if m.starts_with("first\n\nOther startup attempts:"))
    );

    let single = reported_error(vec![(
        GPU,
        attempt_error(AttemptFailure::Fatal, "no model"),
    )]);
    assert!(matches!(&single, LLMError::Other(m) if m == "no model"));

    assert!(matches!(reported_error(Vec::new()), LLMError::Other(_)));
}

#[test]
fn output_tail_is_bounded_by_lines_and_bytes() {
    let mut tail = OutputTail::default();
    for i in 0..100 {
        tail.push(&format!("line {i}"));
    }
    let lines = tail.snapshot();
    assert_eq!(lines.len(), OUTPUT_TAIL_MAX_LINES);
    assert_eq!(lines.last().map(String::as_str), Some("line 99"));

    let mut tail = OutputTail::default();
    let long = "x".repeat(OUTPUT_LINE_MAX_BYTES * 3);
    for _ in 0..10 {
        tail.push(&long);
    }
    let lines = tail.snapshot();
    assert!(lines.iter().all(|l| l.len() == OUTPUT_LINE_MAX_BYTES));
    assert!(lines.iter().map(String::len).sum::<usize>() <= OUTPUT_TAIL_MAX_BYTES);
    assert_eq!(tail.bytes, lines.iter().map(String::len).sum::<usize>());
}

#[test]
fn render_tail_keeps_the_last_lines() {
    assert_eq!(render_tail(&[]), "");
    assert_eq!(render_tail(&lines("\n  \n")), "");
    let output: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
    let rendered = render_tail(&output);
    assert!(rendered.starts_with("\n\nllama-server output (last 12 lines):\n  line 8\n"));
    assert!(rendered.ends_with("\n  line 19"));
    assert!(!rendered.contains("line 7\n"));
}

#[test]
fn clip_respects_char_boundaries() {
    assert_eq!(clip("hello", 10), "hello");
    assert_eq!(clip("hello", 3), "hel");
    // 'é' is two bytes; cutting inside it backs off to the boundary.
    assert_eq!(clip("aé", 2), "a");
}

#[test]
fn describe_exit_formats_ntstatus_as_hex() {
    assert_eq!(
        describe_exit(Some(0xC000_0135_u32 as i32), None),
        "exit code 0xC0000135"
    );
    assert_eq!(describe_exit(Some(1), None), "exit code 1");
    assert_eq!(describe_exit(None, Some(6)), "signal 6");
    assert_eq!(describe_exit(None, None), "no exit status");
}

#[test]
fn reaper_matches_only_installed_sidecars_next_to_lattice() {
    let dir = std::path::Path::new("/Applications/Lattice.app/Contents/MacOS");
    for name in [
        "llama-server",
        "llama-server.exe",
        "LLAMA-SERVER.EXE",
        "llama-server-cpu",
        "llama-server-cpu.exe",
    ] {
        assert!(is_installed_sidecar(&dir.join(name), dir), "{name}");
    }
    for name in [
        "llama-cli",
        "llama-server-gpu",
        "llama-server-cp",
        "lattice-desktop",
    ] {
        assert!(!is_installed_sidecar(&dir.join(name), dir), "{name}");
    }
    // A developer's own build, or another app's bundled copy, elsewhere.
    for path in [
        "/Users/dev/code/llama.cpp/build/bin/llama-server",
        "/Applications/Other.app/Contents/MacOS/llama-server",
        "/Applications/Lattice.app/Contents/MacOS/binaries/llama-server",
        "llama-server",
    ] {
        assert!(
            !is_installed_sidecar(std::path::Path::new(path), dir),
            "{path}"
        );
    }
}

#[test]
fn reaper_spares_sidecars_whose_lattice_is_still_running() {
    let dir = std::path::Path::new("/Applications/Lattice.app/Contents/MacOS");
    let lattice = dir.join("lattice-desktop");
    assert!(has_live_lattice_parent(Some(&lattice), dir));
    // Crashed Lattice: re-parented to launchd/init, or the parent is gone.
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/sbin/launchd")),
        dir
    ));
    assert!(!has_live_lattice_parent(None, dir));
    // A reused PID now running something unrelated.
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/usr/bin/zsh")),
        dir
    ));
}

#[test]
fn bundled_binaries_follow_the_platform() {
    let bundled = SidecarBinary::bundled();
    assert_eq!(bundled.first(), Some(&SidecarBinary::Primary));
    assert_eq!(
        bundled.contains(&SidecarBinary::Cpu),
        cfg!(any(target_os = "windows", target_os = "linux"))
    );
    assert_eq!(SidecarBinary::Primary.label(), SIDECAR_BIN);
    assert_eq!(SidecarBinary::Cpu.label(), SIDECAR_CPU_BIN);
}

/// `externalBin` file stems in a Tauri config (target triple is added
/// by Tauri at build time, so the configured entry has none).
fn external_bin_stems(file: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let json: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
    json["bundle"]["externalBin"]
        .as_array()
        .unwrap_or_else(|| panic!("{file} has no bundle.externalBin array"))
        .iter()
        .map(|entry| {
            let entry = entry.as_str().expect("externalBin entries are strings");
            std::path::Path::new(entry)
                .file_name()
                .and_then(|name| name.to_str())
                .expect("externalBin entry has a file name")
                .to_string()
        })
        .collect()
}

/// The spawn names must be the installed stems of what the bundle
/// ships, per platform: the primary build everywhere (the base config
/// is what macOS uses), the CPU build on Windows and Linux, matching
/// `SidecarBinary::CPU_BUILD_BUNDLED`.
#[test]
fn sidecar_names_match_the_bundle_configuration() {
    assert_eq!(external_bin_stems("tauri.conf.json"), [SIDECAR_BIN]);
    for platform_conf in ["tauri.windows.conf.json", "tauri.linux.conf.json"] {
        assert_eq!(
            external_bin_stems(platform_conf),
            [SIDECAR_BIN, SIDECAR_CPU_BIN],
            "{platform_conf}"
        );
    }
}

#[test]
fn registry_starts_empty() {
    let registry = SidecarRegistry::new();
    assert_eq!(registry.live_count(), 0);
    assert_eq!(registry.kill_all(), 0);
}

#[test]
fn registry_kill_all_is_idempotent_when_empty() {
    let registry = SidecarRegistry::new();
    registry.kill_all();
    registry.kill_all();
    registry.kill_all();
    assert_eq!(registry.live_count(), 0);
}

#[test]
fn registry_shutdown_closes_the_same_gate_used_for_registration() {
    let registry = SidecarRegistry::new();
    let mut registrations = 0usize;
    assert_eq!(registry.with_open_entries(|_| registrations += 1), Some(()));
    assert_eq!(registrations, 1);

    registry.kill_all();
    assert_eq!(
        registry.with_open_entries(|_| registrations += 1),
        None,
        "registration closure must not run after shutdown closes admission"
    );
    assert_eq!(registrations, 1);
}

#[test]
fn bounded_probe_output_preserves_line_delimiters_and_caps_both_streams() {
    let mut stdout = Vec::new();
    let mut total = 0;
    assert!(append_probe_line(
        &mut stdout,
        b"Available devices:",
        &mut total
    ));
    assert!(append_probe_line(
        &mut stdout,
        b"  CUDA0: GPU (10 MiB, 9 MiB free)",
        &mut total
    ));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "Available devices:\n  CUDA0: GPU (10 MiB, 9 MiB free)\n"
    );

    let mut other_stream = Vec::new();
    let remaining = PROBE_OUTPUT_MAX_BYTES - total;
    assert!(!append_probe_line(
        &mut other_stream,
        &vec![b'x'; remaining],
        &mut total
    ));
    assert!(other_stream.is_empty());
    assert_eq!(
        total,
        "Available devices:\n  CUDA0: GPU (10 MiB, 9 MiB free)\n".len()
    );
}

/// Nothing listens on this port, so `/health` never answers and the
/// oneshot is the only thing that can resolve the wait.
const DEAD_ENDPOINT: &str = "http://127.0.0.1:18987";

async fn wait(
    rx: tokio::sync::oneshot::Receiver<Result<(), StartupEnd>>,
) -> Result<(), StartupEnd> {
    let signals = StartupSignals::default();
    await_ready(
        rx,
        DEAD_ENDPOINT,
        "token",
        &signals,
        Instant::now() + Duration::from_secs(30),
    )
    .await
}

/// `await_ready` should propagate the oneshot result faithfully:
/// Ok(()) → Ok(()), Err(end) → Err(end), and a dropped sender →
/// `MonitorGone`.
#[tokio::test]
async fn await_ready_propagates_ok() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tx.send(Ok(())).expect("send");
    assert!(wait(rx).await.is_ok());
}

#[tokio::test]
async fn await_ready_propagates_err() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let end = StartupEnd::Exited {
        code: None,
        signal: Some(6),
    };
    tx.send(Err(end.clone())).expect("send");
    assert_eq!(wait(rx).await, Err(end));
}

#[tokio::test]
async fn await_ready_handles_dropped_sender() {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), StartupEnd>>();
    drop(tx);
    assert_eq!(wait(rx).await, Err(StartupEnd::MonitorGone));
}

/// The whole start is bounded once, not once per fallback attempt: a
/// deadline already in the past ends the wait immediately.
#[tokio::test]
async fn await_ready_stops_at_the_shared_budget() {
    let (_tx, rx) = tokio::sync::oneshot::channel::<Result<(), StartupEnd>>();
    let signals = StartupSignals::default();
    let end = await_ready(rx, DEAD_ENDPOINT, "token", &signals, Instant::now()).await;
    assert!(
        matches!(end, Err(StartupEnd::TimedOut(TimeoutKind::Budget(_)))),
        "{end:?}"
    );
}

/// The pinned b8981 startup, captured from the bundled macOS binary:
/// every line on stderr, nothing on stdout.
const B8981_STARTUP: &[&str] = &[
    "main: model loaded",
    "main: server is listening on http://127.0.0.1:18099",
    "main: starting the main loop...",
];

struct DrainHarness {
    tx: tokio::sync::mpsc::Sender<CommandEvent>,
    ready: tokio::sync::oneshot::Receiver<Result<(), StartupEnd>>,
    tail: Arc<SyncMutex<OutputTail>>,
    signals: Arc<StartupSignals>,
}

/// The drain takes a `Receiver<CommandEvent>`, so a test can play a
/// sidecar's whole life through a channel — no binary, no process.
fn drain_harness() -> DrainHarness {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let tail = Arc::new(SyncMutex::new(OutputTail::default()));
    let signals = Arc::new(StartupSignals::default());
    let child = Arc::new(SyncMutex::new(None));
    let ready = spawn_event_drain(
        rx,
        DEAD_ENDPOINT.to_string(),
        child,
        Arc::clone(&tail),
        Arc::clone(&signals),
    );
    DrainHarness {
        tx,
        ready,
        tail,
        signals,
    }
}

async fn send(tx: &tokio::sync::mpsc::Sender<CommandEvent>, event: CommandEvent) {
    tx.send(event).await.expect("drain is listening");
}

fn stderr(line: &str) -> CommandEvent {
    CommandEvent::Stderr(line.as_bytes().to_vec())
}

async fn startup_end(harness: DrainHarness) -> Result<(), StartupEnd> {
    let DrainHarness { tx, ready, .. } = harness;
    drop(tx);
    tokio::time::timeout(Duration::from_secs(2), ready)
        .await
        .expect("drain reported within 2 s")
        .expect("drain did not drop the sender")
}

#[tokio::test]
async fn drain_signals_ready_on_the_real_b8981_startup_log() {
    let harness = drain_harness();
    for line in B8981_STARTUP {
        send(&harness.tx, stderr(line)).await;
    }
    let ready = tokio::time::timeout(Duration::from_secs(2), harness.ready)
        .await
        .expect("readiness within 2 s")
        .expect("drain did not drop the sender");
    assert_eq!(ready, Ok(()));

    let facts = harness.signals.facts();
    assert!(
        facts.model_loader,
        "`main: model loaded` is loader progress"
    );
    assert!(!facts.gpu_failure && !facts.bind_failure);
    assert!(harness
        .tail
        .lock()
        .snapshot()
        .iter()
        .any(|line| line.contains(READY_NEEDLE)));
}

#[tokio::test]
async fn drain_signals_ready_on_semver_engine_startup() {
    let harness = drain_harness();
    send(
        &harness.tx,
        stderr("I srv llama_server: listening on http://127.0.0.1:18099"),
    )
    .await;
    let ready = tokio::time::timeout(Duration::from_secs(2), harness.ready)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready, Ok(()));
}

#[tokio::test]
async fn drain_reports_an_exit_before_readiness_and_keeps_the_bind_signal() {
    let harness = drain_harness();
    send(
        &harness.tx,
        stderr("couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 18099"),
    )
    .await;
    send(
        &harness.tx,
        CommandEvent::Terminated(TerminatedPayload {
            code: Some(1),
            signal: None,
        }),
    )
    .await;
    let tail = Arc::clone(&harness.tail);
    let signals = Arc::clone(&harness.signals);
    let end = startup_end(harness).await;
    assert_eq!(
        end,
        Err(StartupEnd::Exited {
            code: Some(1),
            signal: None
        })
    );
    // A port collision retries the same configuration; reading it as a GPU
    // fault is what used to pin the session to CPU-only at half context.
    let err = startup_failure(
        SidecarBinary::Primary,
        &end.expect_err("exited"),
        &tail.lock().snapshot(),
        signals.facts(),
    );
    assert_eq!(err.kind, AttemptFailure::Retry);
}

#[tokio::test]
async fn drain_reports_a_closed_stream_before_readiness() {
    let harness = drain_harness();
    send(&harness.tx, stderr("build: 8981 (deadbee) with clang")).await;
    assert_eq!(startup_end(harness).await, Err(StartupEnd::StreamClosed));
}

#[tokio::test]
async fn drain_reports_an_error_event_before_readiness() {
    let harness = drain_harness();
    send(&harness.tx, CommandEvent::Error("pipe closed".to_string())).await;
    assert_eq!(
        startup_end(harness).await,
        Err(StartupEnd::EventError("pipe closed".to_string()))
    );
}

/// A sticky signal survives the tail rolling over, which is the whole
/// point: a big model prints dozens of `load_tensors:` lines after the
/// loader banner, and classification used to read the tail.
#[tokio::test]
async fn drain_keeps_startup_signals_after_the_tail_rolls_over() {
    let harness = drain_harness();
    send(&harness.tx, stderr("llama_model_loader: loaded meta data")).await;
    for i in 0..(OUTPUT_TAIL_MAX_LINES * 2) {
        send(&harness.tx, stderr(&format!("load_tensors: layer {i}"))).await;
    }
    send(&harness.tx, stderr("done")).await;
    let signals = Arc::clone(&harness.signals);
    let tail = Arc::clone(&harness.tail);
    let _ = startup_end(harness).await;
    assert!(signals.facts().model_loader);
    assert!(
        !tail
            .lock()
            .snapshot()
            .iter()
            .any(|line| line.contains("llama_model_loader")),
        "the banner has rolled out of the tail, as it does in production"
    );
}

/// The test that would have caught a readiness needle three llama.cpp
/// releases out of date: a binary is shipped that never prints it.
#[test]
fn ready_needle_is_present_in_every_bundled_binary() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return; // no binaries fetched on this machine
    };
    let mut checked = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_sidecar = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(SIDECAR_BIN));
        if !is_sidecar || !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        // A Git LFS pointer or placeholder is not a binary to judge.
        if bytes.len() < 1_000_000 {
            continue;
        }
        assert!(
            bytes
                .windows(READY_NEEDLE.len())
                .any(|window| window == READY_NEEDLE.as_bytes()),
            "{} does not contain the readiness needle {READY_NEEDLE:?}; \
             update the log fast path for this engine (HTTP health remains authoritative)",
            path.display()
        );
        checked += 1;
    }
    tracing::debug!(checked, "checked bundled binaries for the readiness needle");
}

#[test]
fn build_server_args_includes_required_flags() {
    let cfg = SidecarConfig::for_model(PathBuf::from("/models/llama.gguf"));
    let args = build_server_args(&cfg, 12345);

    assert!(args.iter().any(|a| a == "-m"));
    assert!(args.iter().any(|a| a == "/models/llama.gguf"));
    assert!(args.iter().any(|a| a == "--port"));
    assert!(args.iter().any(|a| a == "12345"));
    assert!(args.iter().any(|a| a == "--host"));
    assert!(args.iter().any(|a| a == "127.0.0.1"));
    assert!(args.iter().any(|a| a == "-ngl"));
    assert!(args.iter().any(|a| a == "99"));
    assert!(args.iter().any(|a| a == "--jinja"));
}

/// An unauthenticated loopback port is reachable from every other process
/// on the machine, browsers included, and the bundled web UI is an HTML
/// surface we never meant to serve. The key authenticates the port, so it
/// must not travel in argv, where `ps` hands it to those same processes.
#[test]
fn build_server_args_drop_the_web_ui_and_keep_the_key_out_of_argv() {
    let cfg = SidecarConfig::for_model(PathBuf::from("/models/llama.gguf"));
    let args = build_server_args(&cfg, 12345);

    assert!(args.iter().any(|a| a == "--no-webui"));
    assert!(
        !args.iter().any(|a| a == "--api-key"),
        "the key belongs in LLAMA_API_KEY, not on the command line"
    );
}

#[test]
fn api_tokens_are_unguessable_and_never_repeat() {
    let first = new_api_token();
    let second = new_api_token();

    assert_eq!(first.len(), 32, "128 bits of entropy, hex-encoded");
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(first, second, "a token is minted per spawn");
}

use crate::features::llm::engine::gguf_metadata::GgufModelInfo;

/// The machine that reported the bug: an M3 Max with 28753 MiB of unified
/// memory running Ornith-1.5-9B — 32 blocks, 4 KV heads, 256-wide keys, so
/// 128 KiB of cache per token, and a 262144 trained length it cannot
/// possibly hold. It ran an 8192 window because nothing had looked.
fn ornith() -> GgufModelInfo {
    GgufModelInfo {
        architecture: Some("qwen35".into()),
        trained_context_length: Some(262_144),
        block_count: Some(32),
        head_count_kv: Some(4),
        head_count: Some(16),
        key_length: Some(256),
        value_length: Some(256),
        embedding_length: Some(4096),
    }
}

const ORNITH_BYTES: u64 = 5_629_109_248;

#[test]
fn a_large_gpu_gets_the_ceiling_not_the_trained_length() {
    let chosen = context_size_for(28.1, ORNITH_BYTES, 0, &ornith()).unwrap();

    assert_eq!(chosen, AUTO_MAX_CONTEXT_SIZE);
    assert!(chosen > DEFAULT_GPU_CONTEXT_SIZE, "the old flat default");
}

/// The trained length is a ceiling, never a target. Asking a 262144-trained
/// model for 262144 would want 34 GB of KV cache.
#[test]
fn the_trained_length_is_never_treated_as_achievable() {
    let roomy = context_size_for(80.0, ORNITH_BYTES, 0, &ornith()).unwrap();

    assert_eq!(roomy, AUTO_MAX_CONTEXT_SIZE);
}

/// A mid-sized card must come back with a window its memory can hold.
#[test]
fn a_smaller_gpu_gets_a_window_that_fits_its_memory() {
    let chosen = context_size_for(12.0, ORNITH_BYTES, 0, &ornith()).unwrap();

    let budget = (12.0 * VRAM_USABLE_FRACTION * 1024.0 * 1024.0 * 1024.0) as u64;
    let cache = u64::from(chosen) * 131_072;
    assert!(
        cache + ORNITH_BYTES <= budget,
        "{chosen} tokens needs {cache} bytes on top of the weights"
    );
    assert!(chosen < AUTO_MAX_CONTEXT_SIZE, "and below the ceiling");
}

/// A card that can barely hold the weights still gets a loadable window.
/// Declining here would hand back the larger flat default — the one size
/// guaranteed not to fit.
#[test]
fn a_cramped_gpu_gets_the_floor_rather_than_the_larger_default() {
    let chosen = context_size_for(8.0, ORNITH_BYTES, 0, &ornith()).unwrap();

    assert_eq!(chosen, AUTO_MIN_CONTEXT_SIZE);
    assert!(chosen < DEFAULT_GPU_CONTEXT_SIZE);
}

/// A model trained short must not be stretched by the automatic path.
#[test]
fn a_short_trained_model_caps_at_what_it_was_trained_for() {
    let info = GgufModelInfo {
        trained_context_length: Some(4096),
        ..ornith()
    };

    assert_eq!(context_size_for(28.1, ORNITH_BYTES, 0, &info), Some(4096));
}

/// The 9B (5.6 GB) on an 8 GB Mac: the weights alone exceed the usable
/// share. That used to decline and fall to the 8192 flat default, the one
/// size guaranteed not to fit; it must get the floor.
#[test]
fn weights_past_the_usable_share_get_the_floor_not_the_flat_default() {
    assert_eq!(
        context_size_for(5.3, ORNITH_BYTES, 0, &ornith()),
        Some(AUTO_MIN_CONTEXT_SIZE)
    );
}

/// Chat plus a different utility GGUF on a 16 GB Mac (about 10.7 GB of
/// working set). The utility is sized against what the chat server leaves,
/// not against the whole card.
#[test]
fn a_second_sidecar_is_sized_against_what_the_first_leaves() {
    const UTILITY_BYTES: u64 = 1_000_000_000;
    let vram = 10.7;
    let budget = (vram * VRAM_USABLE_FRACTION * 1024.0 * 1024.0 * 1024.0) as u64;

    let alone = context_size_for(vram, UTILITY_BYTES, 0, &ornith()).unwrap();
    assert_eq!(alone, AUTO_MAX_CONTEXT_SIZE, "the whole card to itself");

    let small_chat = 3_000_000_000;
    let beside = context_size_for(vram, UTILITY_BYTES, small_chat, &ornith()).unwrap();
    let cache = u64::from(beside) * 131_072;
    assert!(beside < alone);
    assert!(
        small_chat + UTILITY_BYTES + cache <= budget,
        "both servers fit the working set"
    );

    let chat_9b_at_8k = ORNITH_BYTES + 8192 * 131_072;
    assert_eq!(
        context_size_for(vram, UTILITY_BYTES, chat_9b_at_8k, &ornith()),
        Some(AUTO_MIN_CONTEXT_SIZE),
        "no room left beside the 9B: the floor"
    );
}

/// A header without KV dimensions cannot be sized, and guessing would be
/// worse than the conservative default.
#[test]
fn an_unreadable_model_declines_to_choose() {
    let info = GgufModelInfo {
        trained_context_length: Some(32_768),
        ..Default::default()
    };

    assert_eq!(context_size_for(28.1, ORNITH_BYTES, 0, &info), None);
}

/// A missing file is the common case in tests and on a broken install; the
/// flat default has to survive it.
#[test]
fn a_missing_model_file_falls_back_to_the_flat_default() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Apple), 36.0);

    let config = SidecarConfig::from_capabilities(PathBuf::from("/nonexistent/m.gguf"), &caps, 0);

    assert_eq!(config.context_size, DEFAULT_GPU_CONTEXT_SIZE);
}
