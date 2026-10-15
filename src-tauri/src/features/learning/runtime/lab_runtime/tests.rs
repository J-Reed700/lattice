//! Tests for path resolution, spec validation, the run argv and contained execution.
use super::*;

#[test]
fn engine_executable_resolution_handles_gui_path_and_missing_cli_without_env_mutation() {
    let temporary = tempfile::tempdir().unwrap();
    let gui_path = temporary.path().join("minimal-gui-path");
    let home = temporary.path().join("home");
    std::fs::create_dir_all(&gui_path).unwrap();
    #[cfg(unix)]
    std::fs::write(
        gui_path.join(engine_filename(LearningContainerEngine::Docker)),
        "not executable",
    )
    .unwrap();
    let docker_cli = home
        .join(".docker/bin")
        .join(engine_filename(LearningContainerEngine::Docker));
    std::fs::create_dir_all(docker_cli.parent().unwrap()).unwrap();
    std::fs::write(&docker_cli, "stub").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&docker_cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let minimal_path = std::env::join_paths([&gui_path]).unwrap();
    let fallback_locations =
        known_engine_locations(LearningContainerEngine::Docker, Some(home.as_path()));
    assert_eq!(
        resolve_engine_executable(
            &engine_filename(LearningContainerEngine::Docker),
            Some(&minimal_path),
            &fallback_locations,
        ),
        Some(docker_cli.clone())
    );

    let mixed_path = std::env::join_paths([
        gui_path.clone(),
        PathBuf::from("relative-bin"),
        PathBuf::new(),
    ])
    .unwrap();
    let child_path = extended_engine_path(Some(&mixed_path), &fallback_locations);
    let child_directories = std::env::split_paths(&child_path).collect::<Vec<_>>();
    assert!(child_directories.contains(&gui_path));
    assert!(child_directories
        .iter()
        .any(|directory| directory == docker_cli.parent().unwrap()));
    assert!(child_directories
        .iter()
        .all(|directory| directory.is_absolute()));

    let empty_path = std::env::join_paths([temporary.path().join("empty-path")]).unwrap();
    assert_eq!(
        resolve_engine_executable(
            &engine_filename(LearningContainerEngine::Docker),
            Some(&empty_path),
            &[],
        ),
        None,
        "missing engine CLI must remain unresolved without consulting process-global PATH",
    );
}

#[test]
fn engine_exit_codes_distinguish_runtime_startup_failures_from_learner_failures() {
    assert_eq!(
        classify_engine_exit(true, Some(0)),
        LearningLabRunStatus::Passed
    );
    for code in 125..=127 {
        assert_eq!(
            classify_engine_exit(false, Some(code)),
            LearningLabRunStatus::RuntimeUnavailable
        );
    }
    assert_eq!(
        classify_engine_exit(false, Some(1)),
        LearningLabRunStatus::Failed
    );
    assert_eq!(
        classify_engine_exit(false, None),
        LearningLabRunStatus::Failed
    );
}

fn spec() -> LearningLabExecutionSpec {
    LearningLabExecutionSpec {
        run_id: "8b915770-f23a-4a86-8d89-77334ce192d6".into(),
        image_id: format!("sha256:{}", "a".repeat(64)),
        command: vec!["python".into(), "-I".into(), "tests/run.py".into()],
        files: vec![LearningLabFile {
            path: "src/answer.py".into(),
            content: "print('hello')\n".into(),
        }],
        read_only_paths: vec!["src/answer.py".into()],
        limits: LearningLabLimits::default(),
    }
}

#[test]
fn rejects_every_portable_path_escape_shape() {
    for path in [
        "../secret",
        "src/../../secret",
        "/etc/passwd",
        "C:/Windows/system.ini",
        "C:\\Windows\\system.ini",
        "./answer.py",
        "src//answer.py",
        "src/answer,copy.py",
        "",
    ] {
        assert!(validate_lab_path(path).is_err(), "accepted {path:?}");
    }
    assert_eq!(
        validate_lab_path("src/lib/answer.py").unwrap(),
        PathBuf::from("src/lib/answer.py")
    );
}

#[test]
fn command_contains_every_containment_and_resource_flag() {
    let workspace = tempfile::tempdir().unwrap();
    let args = container_run_args(&spec(), workspace.path()).unwrap();
    let text = args
        .iter()
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>();
    for sequence in [
        vec!["--pull", "never"],
        vec!["--network", "none"],
        vec!["--read-only", "--cap-drop", "ALL"],
        vec!["--security-opt", "no-new-privileges=true"],
        vec!["--pids-limit", "64"],
        vec!["--memory", "256m", "--memory-swap", "256m"],
        vec!["--cpus", "1.000"],
        vec!["--ulimit", "nofile=1024:1024"],
        vec!["--user", "65534:65534"],
        vec!["--workdir", "/workspace"],
    ] {
        assert!(
            text.windows(sequence.len())
                .any(|window| window == sequence.as_slice()),
            "missing {sequence:?} in {text:?}"
        );
    }
    let input_mount = format!(
        "type=bind,src={},dst=/input,readonly",
        workspace.path().display()
    );
    assert!(text.iter().any(|arg| arg == &input_mount));
    assert!(text
        .iter()
        .any(|arg| { arg == "/workspace:rw,exec,nosuid,nodev,size=64m,mode=1777" }));
    assert!(text
        .iter()
        .any(|arg| arg == "/tmp:rw,noexec,nosuid,nodev,size=64m,mode=1777"));
    let protected_mount = format!(
        "type=bind,src={},dst=/workspace/src/answer.py,readonly",
        workspace.path().join("src/answer.py").display()
    );
    assert!(text.iter().any(|arg| arg == &protected_mount));
    let image_index = text
        .iter()
        .position(|arg| arg.starts_with("sha256:"))
        .unwrap();
    let protected_mount_index = text.iter().position(|arg| arg == &protected_mount).unwrap();
    assert!(protected_mount_index < image_index);
    let container_command = text
        .get(image_index.saturating_add(1)..)
        .unwrap_or_default();
    assert_eq!(
        container_command,
        [
            "/bin/sh",
            "-c",
            WORKSPACE_BOOTSTRAP,
            WORKSPACE_BOOTSTRAP_ARG0,
            "python",
            "-I",
            "tests/run.py"
        ]
    );
    assert_eq!(workspace_tmpfs_size(&spec().limits), 64);
    assert_eq!(
        workspace_tmpfs_size(&LearningLabLimits {
            memory_megabytes: 32,
            ..LearningLabLimits::default()
        }),
        8
    );
    assert!(container_run_args(&spec(), &workspace.path().join("lattice,lab")).is_err());
    assert!(container_run_args(&spec(), Path::new("relative-lab")).is_err());
}

#[test]
fn combined_output_limit_counts_lossy_utf8_bytes_and_prefers_stdout() {
    let mut stdout = String::from("🙂ab");
    let mut stderr = String::from("ézz");
    assert!(bound_combined_output(&mut stdout, &mut stderr, 4));
    assert_eq!(stdout, "🙂");
    assert!(stderr.is_empty());
    assert!(stdout.len() + stderr.len() <= 4);

    let mut stdout = String::from_utf8_lossy(&[0xff, 0xff]).into_owned();
    let mut stderr = String::from("tail");
    assert!(bound_combined_output(&mut stdout, &mut stderr, 4));
    assert!(stdout.len() + stderr.len() <= 4);
}

#[test]
fn requires_an_immutable_local_image_and_bounded_payload() {
    let mut value = spec();
    value.image_id = "python:latest".into();
    assert!(validate_execution_spec(&value).is_err());
    let mut value = spec();
    value.image_id = format!("sha256:{}", "A".repeat(64));
    assert!(validate_execution_spec(&value).is_err());
    let mut value = spec();
    value.command.push("x".repeat(MAX_ARG_CHARS + 1));
    assert!(validate_execution_spec(&value).is_err());
    let mut value = spec();
    value.files[0].content = "x".repeat(MAX_FILE_BYTES + 1);
    assert!(validate_execution_spec(&value).is_err());
    let mut value = spec();
    value.read_only_paths = vec!["missing.py".into()];
    assert!(validate_execution_spec(&value).is_err());
}

#[test]
fn materialization_never_overwrites_existing_files() -> anyhow::Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("run");
    let value = spec();
    materialize_workspace(&root, &value)?;
    assert_eq!(
        std::fs::read_to_string(root.join("src/answer.py"))?,
        "print('hello')\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&root)?.permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::metadata(root.join("src"))?.permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::metadata(root.join("src/answer.py"))?
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    assert!(materialize_workspace(&root, &value).is_err());
    Ok(())
}

#[tokio::test]
async fn bounded_reader_drains_but_retains_only_the_limit() -> anyhow::Result<()> {
    let (mut writer, reader) = tokio::io::duplex(128);
    let write = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        writer.write_all(&vec![b'x'; 10_000]).await.unwrap();
    });
    let (bytes, truncated) = bounded_read(reader, 1_024).await?;
    write.await?;
    assert_eq!(bytes.len(), 1_024);
    assert!(truncated);
    Ok(())
}

#[tokio::test]
#[ignore = "requires an explicitly selected, already-local POSIX container image"]
async fn opt_in_container_smoke_proves_runtime_containment_and_read_only_checks(
) -> anyhow::Result<()> {
    let image_id = std::env::var("LATTICE_LAB_SMOKE_IMAGE_ID").expect(
        "Set LATTICE_LAB_SMOKE_IMAGE_ID to an already-local sha256 image ID; this test never pulls.",
    );
    let engine = match std::env::var("LATTICE_LAB_SMOKE_ENGINE")
        .unwrap_or_else(|_| "docker".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "docker" => LearningContainerEngine::Docker,
        "podman" => LearningContainerEngine::Podman,
        value => anyhow::bail!("unsupported LATTICE_LAB_SMOKE_ENGINE={value}"),
    };
    let capability = detect_container_engine(engine).await;
    anyhow::ensure!(
        capability.available,
        "{} is unavailable: {}",
        engine.executable(),
        capability.reason.unwrap_or_default()
    );
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("run");
    let spec = LearningLabExecutionSpec {
        run_id: uuid::Uuid::new_v4().to_string(),
        image_id,
        command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "if printf tampered > checks/guard.txt 2>/dev/null; then exit 91; fi; \
             if printf root-write > /lattice-root-write 2>/dev/null; then exit 92; fi; \
             if printf host-write > /input/work/input.txt 2>/dev/null; then exit 93; fi; \
             printf '#!/bin/sh\\nexit 0\\n' > /tmp/noexec-smoke.sh; chmod +x /tmp/noexec-smoke.sh; \
             if /tmp/noexec-smoke.sh 2>/dev/null; then exit 94; fi; \
             printf '#!/bin/sh\\nexit 0\\n' > work/exec-smoke.sh; chmod +x work/exec-smoke.sh; work/exec-smoke.sh || exit 95; \
             printf 'guest-change\\n' > work/input.txt; \
             printf 'learner-write\\n' > work/new-output.txt; \
             printf 'GUARD_OK\\n'; cat checks/guard.txt; \
             printf 'STATUS\\n'; cat /proc/self/status; \
             printf 'NETWORK\\n'; cat /proc/net/dev; \
             printf '%300000s' '€'; printf '%300000s' '€' >&2"
                .into(),
        ],
        files: vec![
            LearningLabFile {
                path: "work/input.txt".into(),
                content: "host-intact\n".into(),
            },
            LearningLabFile {
                path: "checks/guard.txt".into(),
                content: "guard-intact\n".into(),
            },
        ],
        read_only_paths: vec!["checks/guard.txt".into()],
        limits: LearningLabLimits {
            timeout_seconds: 20,
            ..LearningLabLimits::default()
        },
    };
    materialize_workspace(&root, &spec)?;
    let result = execute_container_lab(engine, &root, &spec, CancellationToken::new()).await?;
    assert_eq!(result.status, LearningLabRunStatus::Passed, "{result:?}");
    assert!(result.stdout.contains("GUARD_OK\nguard-intact"));
    assert!(result.stdout.contains("NoNewPrivs:\t1"));
    assert!(result.stdout.contains("CapEff:\t0000000000000000"));
    let network = result.stdout.split("NETWORK\n").nth(1).unwrap_or_default();
    assert!(network.contains("lo:"), "{network}");
    assert!(!network.contains("eth0:"), "{network}");
    assert_eq!(
        std::fs::read_to_string(root.join("checks/guard.txt"))?,
        "guard-intact\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("work/input.txt"))?,
        "host-intact\n"
    );
    assert!(!root.join("work/new-output.txt").exists());
    assert!(result.output_truncated);
    assert!(result.stdout.len() + result.stderr.len() <= spec.limits.output_bytes);

    let cancelled_root = parent.path().join("cancelled-run");
    let cancelled_spec = LearningLabExecutionSpec {
        run_id: uuid::Uuid::new_v4().to_string(),
        image_id: spec.image_id.clone(),
        command: vec!["/bin/sh".into(), "-c".into(), "while :; do :; done".into()],
        files: vec![LearningLabFile {
            path: "work/input.txt".into(),
            content: "cancel me\n".into(),
        }],
        read_only_paths: vec![],
        limits: spec.limits.clone(),
    };
    materialize_workspace(&cancelled_root, &cancelled_spec)?;
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        trigger.cancel();
    });
    let cancelled =
        execute_container_lab(engine, &cancelled_root, &cancelled_spec, cancellation).await?;
    cancel_task.await?;
    assert_eq!(cancelled.status, LearningLabRunStatus::Cancelled);
    let inspect = engine
        .command()
        .args(["inspect", &container_name(&cancelled_spec.run_id)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    assert!(
        !inspect.success(),
        "cancelled container remained registered"
    );

    let missing_image_root = parent.path().join("missing-image-run");
    let missing_image_spec = LearningLabExecutionSpec {
        run_id: uuid::Uuid::new_v4().to_string(),
        image_id: format!("sha256:{}", "f".repeat(64)),
        command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf SHOULD_NOT_RUN".into(),
        ],
        files: vec![LearningLabFile {
            path: "work/input.txt".into(),
            content: "preserve learner data\n".into(),
        }],
        read_only_paths: vec![],
        limits: spec.limits.clone(),
    };
    materialize_workspace(&missing_image_root, &missing_image_spec)?;
    let missing_image = execute_container_lab(
        engine,
        &missing_image_root,
        &missing_image_spec,
        CancellationToken::new(),
    )
    .await?;
    assert_eq!(
        missing_image.status,
        LearningLabRunStatus::RuntimeUnavailable
    );
    assert_eq!(missing_image.exit_code, Some(125));
    assert!(!missing_image.stdout.contains("SHOULD_NOT_RUN"));
    assert!(!missing_image.stderr.is_empty());
    assert_eq!(
        std::fs::read_to_string(missing_image_root.join("work/input.txt"))?,
        "preserve learner data\n"
    );
    Ok(())
}
