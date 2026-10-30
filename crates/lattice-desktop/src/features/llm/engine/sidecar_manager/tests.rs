use super::*;
use tauri_plugin_shell::process::TerminatedPayload;

#[test]
fn reserve_free_port_returns_nonzero_loopback_port() {
    let ports = Arc::new(ReservedPorts::default());
    let reservation = reserve_free_port(&ports).expect("port allocation failed");
    assert!(
        reservation.port() > 0,
        "expected a non-zero port, got {}",
        reservation.port()
    );
    let port = reservation.port();
    assert!(ports.taken.lock().contains(&port));
    drop(reservation);
    assert!(!ports.taken.lock().contains(&port));
}

#[test]
fn reserve_free_port_never_hands_out_a_live_reservation() {
    // The OS is free to hand back the same just-released ephemeral port;
    // the reservation set is what stops two sidecars sharing it.
    let ports = Arc::new(ReservedPorts::default());
    let first = reserve_free_port(&ports).expect("first allocation");
    let second = reserve_free_port(&ports).expect("second allocation");
    assert_ne!(first.port(), second.port());
}

#[test]
fn sidecar_config_for_model_offloads_everything_at_the_trained_window() {
    let cfg = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
    assert_eq!(cfg.n_gpu_layers, 99);
    assert_eq!(
        cfg.context_size, 0,
        "an unreadable header leaves the window to llama-server, which reads it from the model"
    );
}

#[test]
fn without_gpu_offload_disables_gpu_and_keeps_the_window() {
    let gpu = SidecarConfig {
        context_size: 60_416,
        ..SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"))
    };
    let cpu = gpu.without_gpu_offload();
    assert_eq!(cpu.n_gpu_layers, 0);
    assert_eq!(cpu.context_size, 60_416, "the window is the user's");
    assert_eq!(cpu.model_path, gpu.model_path);
}

fn make_caps(
    gpu_vendor: Option<crate::features::llm::engine::system::GPUVendor>,
    ram_gb: f64,
) -> SystemCapabilities {
    use crate::features::llm::engine::system::{GPUInfo, Platform};
    SystemCapabilities {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb,
        available_memory_bytes: None,
        gpu: gpu_vendor.map(|vendor| GPUInfo {
            vendor,
            name: format!("{:?} test GPU", vendor),
            vram_gb: None,
            free_vram_gb: None,
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
/// `from_capabilities` is a pure function of the path, the machine and the
/// window — if it ever grows a nondeterministic input (a timestamp, a port,
/// a counter), every role silently gets its own copy of the weights again
/// and the only symptom is memory.
#[test]
fn the_same_model_on_one_machine_is_one_key() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = make_caps(Some(GPUVendor::Apple), 16.0);
    let chat = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 32_768);
    let utility = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 32_768);
    assert_eq!(
        chat, utility,
        "the roles must agree on the key or they will not share"
    );

    let other_model =
        SidecarConfig::from_capabilities(PathBuf::from("/tmp/other.gguf"), &caps, 32_768);
    assert_ne!(chat, other_model, "a different model needs its own server");

    // A degraded fallback is a different server, not the same one with a
    // note on it: it runs on the CPU.
    assert_ne!(chat, chat.without_gpu_offload());
}

/// The window is whatever the caller resolved from the user's setting or the
/// model's header. The machine decides only where the layers go.
#[test]
fn from_capabilities_takes_the_window_as_given_on_every_machine() {
    use crate::features::llm::engine::system::GPUVendor;
    for (vendor, ram_gb, offloaded) in [
        (Some(GPUVendor::Apple), 16.0, 99),
        (Some(GPUVendor::Nvidia), 32.0, 99),
        (Some(GPUVendor::Apple), 4.0, 99),
        (None, 16.0, 0),
        (None, 4.0, 0),
        (Some(GPUVendor::Unknown), 16.0, 0),
    ] {
        let caps = make_caps(vendor, ram_gb);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps, 262_144);
        assert_eq!(cfg.n_gpu_layers, offloaded, "{vendor:?} with {ram_gb} GB");
        assert_eq!(cfg.context_size, 262_144, "{vendor:?} with {ram_gb} GB");
    }
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
    for (line, expected) in [
        (
            "llama_model_load: error loading model architecture: unknown model architecture: 'laguna'",
            "laguna",
        ),
        (
            "llama_model_load: error loading model architecture: unknown model architecture: 'future-model-family'",
            "future-model-family",
        ),
        (
            "gguf_init_from_reader: tensor 'output.weight' has invalid ggml type 142. should be in [0, 43)",
            "invalid ggml type 142",
        ),
        (
            "gguf_init_from_reader: tensor 'output_norm.weight' has offset 165015872, expected 174722688",
            "has offset 165015872, expected 174722688",
        ),
        (
            "llama_model_load: error loading model: missing tensor 'blk.32.nextn.eh_proj.weight'",
            "missing tensor 'blk.32.nextn.eh_proj.weight'",
        ),
        (
            "llama_model_load: error loading model: done_getting_tensors: wrong number of tensors; expected 851, got 843",
            "wrong number of tensors; expected 851, got 843",
        ),
    ] {
        let output = lines(line);
        let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &output);
        assert_eq!(err.kind, AttemptFailure::Unsupported);
        assert!(err.message.contains(expected), "{}", err.message);
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
fn an_unsupported_model_is_the_only_startup_failure_that_condemns_the_file() {
    let unsupported = classify(
        SidecarBinary::Primary,
        &exited(Some(1), None),
        &lines("llama_model_load: error loading model architecture: unknown model architecture: 'laguna'"),
    );
    assert!(matches!(
        unsupported.into_llm_error(),
        LLMError::ModelUnsupported(m) if m.contains("laguna")
    ));

    // The machine, not the file: a readiness timeout, a kill under memory
    // pressure, and an allocation the loader could not make.
    for (end, output) in [
        (
            StartupEnd::TimedOut(TimeoutKind::Budget(120)),
            lines("llama_model_load: loading model tensors"),
        ),
        (
            exited(None, Some(SIGKILL)),
            lines("load_tensors: loading model"),
        ),
        (
            exited(Some(1), None),
            lines("llama_model_load: error loading model: unable to allocate Metal buffer"),
        ),
    ] {
        let err = classify(SidecarBinary::Primary, &end, &output);
        assert_ne!(err.kind, AttemptFailure::Unsupported, "{}", err.message);
        assert!(
            !matches!(err.into_llm_error(), LLMError::ModelUnsupported(_)),
            "{end:?}"
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
        // No same-binary retry for a binary that cannot run.
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
    let lattice = std::path::Path::new("/Applications/Lattice.app/Contents/MacOS/lattice-desktop");
    assert!(has_live_lattice_parent(Some(lattice), lattice));
    // Crashed Lattice: re-parented to launchd/init, or the parent is gone.
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/sbin/launchd")),
        lattice
    ));
    assert!(!has_live_lattice_parent(None, lattice));
    // A reused PID now running something unrelated.
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/usr/bin/zsh")),
        lattice
    ));
}

#[test]
fn reaper_reaps_orphans_adopted_by_programs_beside_a_deb_install() {
    // The .deb puts Lattice and its sidecars in /usr/bin. An orphan adopted by
    // another program there (a container's `sleep` as PID 1, `tini`) is not
    // running under Lattice, and must be reaped.
    let lattice = std::path::Path::new("/usr/bin/lattice-desktop");
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/usr/bin/sleep")),
        lattice
    ));
    assert!(!has_live_lattice_parent(
        Some(std::path::Path::new("/usr/bin/tini")),
        lattice
    ));
    assert!(has_live_lattice_parent(Some(lattice), lattice));
    // After a package upgrade replaced the file, the old window still counts.
    assert!(has_live_lattice_parent(
        Some(std::path::Path::new("/usr/bin/lattice-desktop (deleted)")),
        lattice
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
        &lattice_core::shared::runtime::background::BackgroundTasks::default(),
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
    // A port collision retries the same configuration rather than degrading to
    // CPU-only.
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

/// A sticky signal survives the tail rolling over: a big model prints dozens
/// of `load_tensors:` lines after the loader banner.
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
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|at| args.get(at + 1))
        .map(String::as_str)
}

/// A hybrid model rolls back to its last checkpoint on any prefix change; at
/// the server's default spacing that was the start of an 8k prompt.
#[test]
fn build_server_args_checkpoint_the_prompt_cache_every_thousand_tokens() {
    let args = build_server_args(
        &SidecarConfig::for_model(PathBuf::from("/models/qwen.gguf")),
        12345,
    );
    assert_eq!(flag_value(&args, "--ctx-checkpoints"), Some("32"));
    assert_eq!(flag_value(&args, "--checkpoint-min-step"), Some("1024"));
    assert!(
        !args.iter().any(|a| a == "-fa"),
        "flash attention is already the server's default"
    );
}

/// MTP drafting needs both the user's opt-in and a GGUF with the head, and
/// forces a single slot; every other combination launches as before.
#[test]
fn build_server_args_draft_with_mtp_only_when_asked_and_able() {
    let base = SidecarConfig::for_model(PathBuf::from("/models/qwen-mtp.gguf"));
    let combinations = [
        (false, SpeculativeDecoding::Off, None),
        (true, SpeculativeDecoding::Off, None),
        (false, SpeculativeDecoding::MtpDraft { n_max: 4 }, None),
        (true, SpeculativeDecoding::MtpDraft { n_max: 4 }, Some("4")),
    ];
    for (mtp_capable, speculative_decoding, drafted) in combinations {
        let config = SidecarConfig {
            mtp_capable,
            speculative_decoding,
            ..base.clone()
        };
        let args = build_server_args(&config, 12345);
        let case = format!("{mtp_capable} {speculative_decoding:?}");
        assert_eq!(flag_value(&args, "--spec-draft-n-max"), drafted, "{case}");
        match drafted {
            Some(_) => {
                assert_eq!(
                    flag_value(&args, "--spec-type"),
                    Some("draft-mtp"),
                    "{case}"
                );
                assert_eq!(flag_value(&args, "-np"), Some("1"), "{case}");
            }
            None => {
                assert!(flag_value(&args, "--spec-type").is_none(), "{case}");
                assert!(
                    flag_value(&args, "-np").is_none(),
                    "the server keeps its own slot count: {case}"
                );
            }
        }
    }
    // A CPU fallback is the same model, so it drafts the same way.
    let drafting = SidecarConfig {
        mtp_capable: true,
        speculative_decoding: SpeculativeDecoding::MtpDraft { n_max: 2 },
        ..base
    };
    assert_eq!(drafting.without_gpu_offload().mtp_draft(), Some(2));
}

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

use super::config::{footprint_from, hint_for, memory_room};
use crate::features::llm::engine::gguf_metadata::{GgufModelInfo, PerLayer};

const GIB: u64 = 1024 * 1024 * 1024;

/// Ornith-1.5-9B Q4_K_M as its header reads: Qwen 3.5 9B's 32 layers plus one
/// multi-token-prediction block, one layer in four attending to the whole
/// window, and a 248,320-token vocabulary.
fn ornith_9b() -> GgufModelInfo {
    GgufModelInfo {
        architecture: Some("qwen35".into()),
        trained_context_length: Some(262_144),
        block_count: Some(33),
        nextn_predict_layers: Some(1),
        full_attention_interval: Some(4),
        head_count_kv: Some(4.into()),
        head_count: Some(16.into()),
        key_length: Some(256),
        value_length: Some(256),
        embedding_length: Some(4096),
        feed_forward_length: Some(12_288),
        vocab_size: Some(248_320),
        ssm_conv_kernel: Some(4),
        ssm_state_size: Some(128),
        ssm_group_count: Some(16),
        ssm_inner_size: Some(4096),
        ..Default::default()
    }
}

const ORNITH_9B_BYTES: u64 = 5_780_090_816;

/// What `llama-server --list-devices` reports for an M3 Max with 36 GB:
/// `MTL0: Apple M3 Max (28753 MiB, 28753 MiB free)`.
const M3_MAX_WORKING_SET: u64 = 28_753 * 1024 * 1024;

fn caps_with_gpu(
    vendor: crate::features::llm::engine::system::GPUVendor,
    vram_gb: Option<f64>,
    free_vram_gb: Option<f64>,
) -> SystemCapabilities {
    let mut caps = make_caps(Some(vendor), 36.0);
    let gpu = caps.gpu.as_mut().unwrap();
    gpu.vram_gb = vram_gb;
    gpu.free_vram_gb = free_vram_gb;
    caps
}

#[test]
fn metal_offers_its_whole_working_set_less_what_other_models_hold() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = caps_with_gpu(GPUVendor::Apple, Some(28_753.0 / 1024.0), Some(1.0));

    let room = memory_room(&caps, true, GIB).unwrap();

    assert_eq!(room.source, MemorySource::MetalWorkingSet);
    assert_eq!(room.reported, M3_MAX_WORKING_SET, "nothing is held back");
    assert_eq!(room.held, GIB);
    assert_eq!(room.spare(), M3_MAX_WORKING_SET - GIB);
}

#[test]
fn a_discrete_gpu_keeps_llama_cpp_s_fit_target_free() {
    use crate::features::llm::engine::system::GPUVendor;
    let caps = caps_with_gpu(GPUVendor::Nvidia, Some(24.0), Some(23.0));

    let room = memory_room(&caps, true, 2 * GIB).unwrap();

    assert_eq!(room.source, MemorySource::GpuFree);
    assert_eq!(
        room.reported,
        22 * GIB,
        "its free memory, less the 1 GiB --fit-target"
    );
    assert_eq!(room.spare(), 20 * GIB);
}

#[test]
fn a_server_on_the_cpu_is_priced_against_available_ram_less_the_fit_target() {
    let mut caps = make_caps(None, 16.0);
    caps.available_memory_bytes = Some(12 * GIB);

    let room = memory_room(&caps, false, 4 * GIB).unwrap();

    assert_eq!(room.source, MemorySource::SystemRam);
    assert_eq!(room.reported, 11 * GIB);
    assert_eq!(
        room.held, 0,
        "available RAM already leaves out what other models hold"
    );

    // A GPU machine's server that runs on the CPU is priced the same way.
    let mut on_cpu = caps_with_gpu(
        crate::features::llm::engine::system::GPUVendor::Nvidia,
        Some(24.0),
        Some(23.0),
    );
    on_cpu.available_memory_bytes = Some(12 * GIB);
    assert_eq!(
        memory_room(&on_cpu, false, 0).map(|room| room.source),
        Some(MemorySource::SystemRam)
    );
}

#[test]
fn a_device_that_does_not_report_its_memory_offers_no_room() {
    use crate::features::llm::engine::system::GPUVendor;
    assert_eq!(
        memory_room(&caps_with_gpu(GPUVendor::Apple, None, None), true, 0),
        None
    );
    assert_eq!(
        memory_room(&caps_with_gpu(GPUVendor::Nvidia, Some(24.0), None), true, 0),
        None
    );
    assert_eq!(memory_room(&make_caps(None, 16.0), false, 0), None);
}

fn metal_room(held: u64) -> MemoryRoom {
    MemoryRoom {
        source: MemorySource::MetalWorkingSet,
        reported: M3_MAX_WORKING_SET,
        held,
    }
}

/// 60,416 tokens saved for Ornith-1.5-9B, beside the embedding model's
/// 1.19 GB on an M3 Max.
#[test]
fn a_hint_prices_weights_cache_recurrent_state_and_compute() {
    let info = ornith_9b();

    let hint = hint_for(
        ORNITH_9B_BYTES,
        Some(&info),
        60_416,
        false,
        metal_room(1_191_586_416),
    );

    assert_eq!(
        hint.needed,
        Footprint {
            weights: ORNITH_9B_BYTES,
            // Nine layers keep a cache (eight full-attention, one MTP) at
            // 4 KiB a token each.
            cache: 60_416 * 9 * 4 * 1024,
            state: info.recurrent_state_bytes(),
            compute: info.compute_buffer_bytes(60_416),
            window: 60_416,
        }
    );
    assert_eq!(hint.needed.total(), 8_726_589_376);
    assert!(hint.fits());
    assert_eq!(
        hint.fits_window,
        Some(262_144),
        "the whole trained window fits beside it"
    );
}

#[test]
fn the_fitting_window_is_the_largest_thousand_that_fits_and_never_past_the_trained_length() {
    let info = ornith_9b();
    let spare = 10 * GIB;
    let room = MemoryRoom {
        source: MemorySource::GpuFree,
        reported: spare,
        held: 0,
    };

    let hint = hint_for(ORNITH_9B_BYTES, Some(&info), 262_144, false, room);

    assert!(!hint.fits());
    let fits = hint.fits_window.unwrap();
    assert_eq!(fits % 1024, 0);
    let total = |window| footprint_from(ORNITH_9B_BYTES, Some(&info), window, false).total();
    assert!(total(fits) <= spare, "the window it names fits");
    assert!(total(fits + 1024) > spare, "and the next thousand does not");

    let quantized = hint_for(ORNITH_9B_BYTES, Some(&info), 262_144, true, room);
    assert!(
        quantized.fits_window.unwrap() > fits,
        "a q8_0 cache fits a larger window in the same memory"
    );
}

#[test]
fn weights_past_the_room_fit_no_window_and_an_unreadable_header_says_nothing() {
    let room = MemoryRoom {
        source: MemorySource::GpuFree,
        reported: 4 * GIB,
        held: 0,
    };

    let too_large = hint_for(ORNITH_9B_BYTES, Some(&ornith_9b()), 8_192, false, room);
    assert_eq!(too_large.fits_window, Some(0));
    assert!(too_large.describe().contains("Not even its weights fit."));

    let unreadable = hint_for(ORNITH_9B_BYTES, None, 8_192, false, room);
    assert_eq!(unreadable.fits_window, None);
    assert_eq!(unreadable.needed.weights, ORNITH_9B_BYTES);
    assert_eq!(unreadable.needed.total(), ORNITH_9B_BYTES);
}

#[test]
fn a_hint_tells_the_user_the_numbers_and_the_fix() {
    let hint = hint_for(
        ORNITH_9B_BYTES,
        Some(&ornith_9b()),
        262_144,
        false,
        MemoryRoom {
            source: MemorySource::MetalWorkingSet,
            reported: 16 * GIB,
            held: GIB,
        },
    );

    let text = hint.describe();
    assert!(
        text.starts_with("At 262,144 tokens this model needs about 15.4 GB (5.4 GB weights + 9.0 GB context + 0.2 GB recurrent state + 0.8 GB compute); this Mac's GPU working set is 16.0 GB, and other models hold 1.0 GB of it. About "),
        "{text}"
    );
    assert!(
        text.ends_with(" tokens fit. Lower its context length in Settings → Models."),
        "{text}"
    );
}

/// Gemma 4 12B as its header reads: 8 global layers of 1 KV head at 512 + 512,
/// 40 sliding layers of 8 heads at 256 + 256 over a 1024-token window.
fn gemma4_12b() -> GgufModelInfo {
    let pattern: Vec<bool> = (0..48).map(|layer| layer % 6 != 5).collect();
    GgufModelInfo {
        architecture: Some("gemma4".into()),
        trained_context_length: Some(262_144),
        block_count: Some(48),
        head_count: Some(16.into()),
        head_count_kv: Some(PerLayer::Layers(
            pattern
                .iter()
                .map(|&slides| if slides { 8 } else { 1 })
                .collect(),
        )),
        key_length: Some(512),
        value_length: Some(512),
        key_length_swa: Some(256),
        value_length_swa: Some(256),
        embedding_length: Some(3840),
        sliding_window: Some(1024),
        sliding_window_pattern: Some(pattern),
        shared_kv_layers: Some(0),
        ..Default::default()
    }
}

const GEMMA4_12B_BYTES: u64 = 7_121_861_440;

/// The sliding layers take a fixed 1.5 GB (320 KiB for each of 4608 cells)
/// and the global layers 16 KiB a token, so a sliding model fits a window
/// many times what charging every layer for the whole window would allow.
#[test]
fn gemma_4_sliding_layers_are_charged_their_window_not_the_whole_context() {
    let room = MemoryRoom {
        source: MemorySource::GpuFree,
        reported: GEMMA4_12B_BYTES + 2 * GIB,
        held: 0,
    };
    let every_layer_global = GgufModelInfo {
        sliding_window: None,
        ..gemma4_12b()
    };

    let sliding = hint_for(GEMMA4_12B_BYTES, Some(&gemma4_12b()), 32_768, false, room);
    let global = hint_for(
        GEMMA4_12B_BYTES,
        Some(&every_layer_global),
        32_768,
        false,
        room,
    );

    let (sliding, global) = (sliding.fits_window.unwrap(), global.fits_window.unwrap());
    assert!(sliding > 4 * global, "{sliding} against {global}");
}

/// A model file of `bytes` weights whose header cannot be read, so its cache
/// costs nothing and every price below is its weights.
fn weights_file(dir: &tempfile::TempDir, name: &str, bytes: u64) -> PathBuf {
    let path = dir.path().join(name);
    weights_fixture::sparse_file(&path, bytes);
    path
}

fn running(path: &std::path::Path, context_size: u32, n_gpu_layers: u32) -> RunningServer {
    RunningServer {
        config: SidecarConfig {
            context_size,
            ..SidecarConfig::for_model(path.to_path_buf())
        },
        n_gpu_layers,
        context_size,
    }
}

/// A Mac whose GPU working set is 10 GB.
fn gpu_with_ten_gigabytes() -> SystemCapabilities {
    caps_with_gpu(
        crate::features::llm::engine::system::GPUVendor::Apple,
        Some(10.0),
        Some(10.0),
    )
}

#[test]
fn a_second_window_of_the_same_model_counts_against_the_gpu_but_does_not_size_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let ornith = weights_file(&dir, "ornith.gguf", 5 * GIB);
    let qwen = weights_file(&dir, "qwen.gguf", 3 * GIB);
    let servers = [
        running(&ornith, 8_704, 99),
        running(&qwen, 8_192, 99),
        running(&weights_file(&dir, "cpu.gguf", 4 * GIB), 4_096, 0),
    ];
    let larger_window = SidecarConfig {
        context_size: 32_768,
        ..SidecarConfig::for_model(ornith.clone())
    };

    assert_eq!(
        gpu_bytes_where(&servers, |running| running != &larger_window),
        8 * GIB,
        "the 8,704-token server is a second process with its own weights"
    );
    assert_eq!(
        gpu_bytes_where(&servers, |running| running.model_path != ornith),
        3 * GIB,
        "a window is sized beside other models only, so roles on one model agree on it"
    );
    assert_eq!(
        gpu_bytes_where(&servers, |running| running != &servers[0].config),
        3 * GIB,
        "a server never counts against itself, and a CPU server holds no GPU memory"
    );
}

#[test]
fn admission_shares_a_running_server_and_otherwise_prices_against_what_is_left() {
    let dir = tempfile::tempdir().unwrap();
    let resident = weights_file(&dir, "resident.gguf", 4 * GIB);
    let small = weights_file(&dir, "small.gguf", 2 * GIB);
    let large = weights_file(&dir, "large.gguf", 7 * GIB);
    let servers = [running(&resident, 8_192, 99)];
    let gpu = gpu_with_ten_gigabytes();
    let config = |path: &PathBuf| SidecarConfig::for_model(path.clone());

    assert_eq!(
        admission(&servers, &servers[0].config, &gpu),
        Admission::Shared
    );
    assert_eq!(
        admission(&servers, &config(&small), &gpu),
        Admission::Fits {
            needed: Footprint {
                weights: 2 * GIB,
                cache: 0,
                state: 0,
                compute: 0,
                window: 0,
            },
            spare: 6 * GIB,
        }
    );
    assert!(matches!(
        admission(&servers, &config(&large), &gpu),
        Admission::Short { needed, spare } if needed.total() == 7 * GIB && spare == 6 * GIB
    ));
    assert_eq!(
        admission(&servers, &config(&large).without_gpu_offload(), &gpu),
        Admission::NoOffload
    );
    assert_eq!(
        admission(
            &servers,
            &config(&large),
            &make_caps(
                Some(crate::features::llm::engine::system::GPUVendor::Apple),
                36.0
            )
        ),
        Admission::Unmetered,
        "a GPU that does not report its memory cannot be checked"
    );
}

/// A server that may see loads its projector beside the model and keeps MTP
/// drafting and its single slot exactly as a text-only one does.
#[test]
fn a_seeing_server_loads_its_projector_and_keeps_drafting() {
    let drafting = SidecarConfig {
        mtp_capable: true,
        speculative_decoding: SpeculativeDecoding::MtpDraft { n_max: 3 },
        ..SidecarConfig::for_model(PathBuf::from("/models/qwen/Qwen3.5-9B-Q4_K_M.gguf"))
    };
    let seeing = SidecarConfig {
        projector_path: Some(PathBuf::from("/models/qwen/mmproj.gguf")),
        ..drafting.clone()
    };
    let args = build_server_args(&seeing, 12345);
    assert_eq!(
        flag_value(&args, "--mmproj"),
        Some("/models/qwen/mmproj.gguf")
    );
    assert_eq!(flag_value(&args, "--spec-type"), Some("draft-mtp"));
    assert_eq!(flag_value(&args, "-np"), Some("1"));
    assert!(flag_value(&build_server_args(&drafting, 12345), "--mmproj").is_none());
    assert_ne!(seeing, drafting, "a server that sees is a different server");
    assert_eq!(
        seeing.without_gpu_offload().projector_path,
        seeing.projector_path
    );
}

/// An embedding server is launched with the flags of the design's §5.1 and
/// none of the chat server's.
#[test]
fn an_embedding_server_gets_exactly_its_own_flags() {
    let config = SidecarConfig {
        n_gpu_layers: 99,
        context_size: 8192,
        parallel_slots: Some(4),
        pooling: Some(PoolingServing {
            ubatch: 2048,
            task: PoolingTask::Embedding,
        }),
        ..SidecarConfig::for_model(PathBuf::from("/models/Qwen3-Embedding-0.6B-f16.gguf"))
    };
    let expected: Vec<String> = [
        "-m",
        "/models/Qwen3-Embedding-0.6B-f16.gguf",
        "--embedding",
        "-c",
        "8192",
        "-np",
        "4",
        "-b",
        "2048",
        "-ub",
        "2048",
        "-ngl",
        "99",
        "-fa",
        "auto",
        "--host",
        "127.0.0.1",
        "--port",
        "12345",
        "--no-webui",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect();
    assert_eq!(build_server_args(&config, 12345), expected);

    let on_cpu = build_server_args(&config.without_gpu_offload(), 12345);
    assert_eq!(flag_value(&on_cpu, "-ngl"), Some("0"));
    for chat_only in [
        "--jinja",
        "--ctx-checkpoints",
        "--cache-reuse",
        "--mmproj",
        "-ctk",
        "--spec-type",
        "--pooling",
    ] {
        assert!(!on_cpu.iter().any(|arg| arg == chat_only), "{chat_only}");
    }
}

/// A reranking server is an embedding server's flags with `--reranking`;
/// on the CPU it uses no device at all, so its scratch takes no GPU memory.
#[test]
fn a_reranking_server_gets_its_flags_and_no_device_on_the_cpu() {
    let config = SidecarConfig {
        n_gpu_layers: 99,
        context_size: 4096,
        parallel_slots: Some(1),
        pooling: Some(PoolingServing {
            ubatch: 4096,
            task: PoolingTask::Reranking,
        }),
        ..SidecarConfig::for_model(PathBuf::from("/models/qwen3-reranker-0.6b-q8_0.gguf"))
    };
    let on_gpu = build_server_args(&config, 12345);
    assert!(on_gpu.iter().any(|arg| arg == "--reranking"));
    assert!(!on_gpu.iter().any(|arg| arg == "--embedding"));
    assert_eq!(flag_value(&on_gpu, "-ub"), Some("4096"));
    assert_eq!(flag_value(&on_gpu, "-np"), Some("1"));
    assert!(!on_gpu.iter().any(|arg| arg == "--device"));

    let on_cpu = build_server_args(&config.without_gpu_offload(), 12345);
    assert_eq!(flag_value(&on_cpu, "-ngl"), Some("0"));
    assert_eq!(flag_value(&on_cpu, "--device"), Some("none"));

    let embedding = SidecarConfig {
        pooling: Some(PoolingServing {
            ubatch: 2048,
            task: PoolingTask::Embedding,
        }),
        ..config.without_gpu_offload()
    };
    assert!(
        !build_server_args(&embedding, 12345)
            .iter()
            .any(|arg| arg == "--device"),
        "an embedding server on the CPU is launched as before"
    );
}

/// The embedding field is part of a server's identity, so the pool never
/// hands a chat role an embedding server of the same file or the reverse.
#[test]
fn an_embedding_server_is_keyed_apart_from_a_chat_server_of_one_file() {
    use std::hash::{BuildHasher, RandomState};

    let chat = SidecarConfig::for_model(PathBuf::from("/models/model.gguf"));
    let embedding = SidecarConfig {
        pooling: Some(PoolingServing {
            ubatch: 2048,
            task: PoolingTask::Embedding,
        }),
        ..chat.clone()
    };
    let hasher = RandomState::new();
    assert_ne!(chat, embedding);
    assert_ne!(hasher.hash_one(&chat), hasher.hash_one(&embedding));
}

/// An older server that cannot draft and see at once is told as the two
/// switches to choose between, and no fallback start asks it again.
#[test]
fn a_server_that_cannot_draft_and_see_at_once_names_the_two_switches() {
    let output = vec![
        "srv load_model: loading multimodal projector".to_string(),
        "common_init: Speculative decoding is not supported with multimodal".to_string(),
    ];
    let refused = refused_combination(&output).expect("recognised");
    assert_eq!(refused.kind, AttemptFailure::Refused);
    assert!(refused.message.contains("Vision"));
    assert!(refused.message.contains("Speculative decoding"));
    assert!(matches!(
        refused.into_llm_error(),
        LLMError::InvalidConfig(_)
    ));
    let primary = Attempt {
        binary: SidecarBinary::Primary,
        gpu_offload: true,
    };
    assert_eq!(next_attempt(primary, AttemptFailure::Refused, true), None);
    assert!(refused_combination(&["error loading model".to_string()]).is_none());
}

/// A catalog download of the 9B in a temporary models directory, with a
/// sparse projector of `projector_len` bytes beside it when given.
fn catalog_9b(projector_len: Option<u64>) -> (tempfile::TempDir, PathBuf) {
    let models = tempfile::tempdir().unwrap();
    let directory = models.path().join("qwen3.5-9b-q4_k_m");
    std::fs::create_dir_all(&directory).unwrap();
    let weights = directory.join("Qwen3.5-9B-Q4_K_M.gguf");
    std::fs::write(&weights, b"GGUF").unwrap();
    if let Some(len) = projector_len {
        std::fs::File::create(directory.join("mmproj.gguf"))
            .unwrap()
            .set_len(len)
            .unwrap();
    }
    (models, weights)
}

/// The launch arguments for `weights` under the saved per-model `vision`
/// switch, as Settings → Models saves it.
fn launch_args(weights: &std::path::Path, vision: Option<bool>) -> (SidecarConfig, Vec<String>) {
    use crate::features::llm::engine::factory::{sidecar_config, LocalServerOptions};
    use lattice_core::application::contracts::settings::{LLMSettingsDto, LocalModelConfigDto};
    let saved = LocalModelConfigDto {
        vision,
        ..LocalModelConfigDto::default()
    };
    let server = LocalServerOptions::from_settings(&LLMSettingsDto::default())
        .with_model_config(Some(&saved));
    let config = sidecar_config(weights, -1, 8_192, server, &make_caps(None, 32.0), None);
    let args = build_server_args(&config, 12345);
    (config, args)
}

/// The per-model switch decides whether a downloaded projector is loaded:
/// unset and on load it, off leaves it out, each a different server.
#[test]
fn the_vision_switch_adds_and_removes_the_projector_from_the_launch() {
    let (_models, weights) = catalog_9b(Some(918_166_080));
    let projector = weights.with_file_name("mmproj.gguf");
    let (unset, unset_args) = launch_args(&weights, None);
    let (on, _) = launch_args(&weights, Some(true));
    let (off, off_args) = launch_args(&weights, Some(false));
    assert_eq!(
        flag_value(&unset_args, "--mmproj"),
        Some(projector.to_string_lossy().as_ref())
    );
    assert_eq!(on, unset);
    assert!(flag_value(&off_args, "--mmproj").is_none());
    assert_ne!(off, on, "switching Vision changes which server runs");
}

/// A projector is loaded only once its whole file is on disk, and finishing
/// the download makes the launch a different server from the text-only one.
#[test]
fn a_finished_projector_download_changes_the_server() {
    let (_models, weights) = catalog_9b(Some(918_166_079));
    let (partial, partial_args) = launch_args(&weights, None);
    assert!(flag_value(&partial_args, "--mmproj").is_none());
    std::fs::File::options()
        .write(true)
        .open(weights.with_file_name("mmproj.gguf"))
        .unwrap()
        .set_len(918_166_080)
        .unwrap();
    let (finished, finished_args) = launch_args(&weights, None);
    assert!(flag_value(&finished_args, "--mmproj").is_some());
    assert_ne!(finished, partial);
}
