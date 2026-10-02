//! App-owned language runtime presets for contained Learning Studio projects.
//!
//! Presets intentionally expose fixed image references and fixed argv. No
//! learner- or model-provided image names or commands are accepted here.

use crate::features::learning::lab_runtime::{LearningContainerEngine, LearningLabLimits};
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

const ENGINE_TIMEOUT: Duration = Duration::from_secs(5);
const INSPECT_TIMEOUT: Duration = Duration::from_secs(20);
const PULL_TIMEOUT: Duration = Duration::from_secs(600);
const BUILD_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_COMMAND_OUTPUT: usize = 32 * 1024;
const NODE_BASE_IMAGE_REF: &str =
    "node@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c";
const NODE_PREPARED_IMAGE_REF: &str = "lattice-learning/react-ts:22.23.3-v2";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningRuntimePresetId {
    Csharp,
    Rust,
    Node,
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningRuntimePresetDto {
    pub id: LearningRuntimePresetId,
    pub name: String,
    pub description: String,
    /// Fixed, versioned app-owned image reference used only during preparation.
    pub image_ref: String,
    /// Fixed argv executed by the contained lab runner after preparation.
    pub command: Vec<String>,
    pub limits: LearningLabLimits,
    /// Describes required workspace files and what the base image supports.
    pub entrypoint_contract: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedLearningRuntime {
    pub preset: LearningRuntimePresetDto,
    /// Local immutable engine image ID, suitable for LearningLabExecutionSpec.
    pub image_id: String,
}

fn runtime_limits() -> LearningLabLimits {
    LearningLabLimits {
        timeout_seconds: 120,
        memory_megabytes: 1_024,
        cpu_millis: 1_000,
        process_limit: 64,
        output_bytes: 256 * 1024,
    }
}

/// Returns descriptors for the built-in runtimes in stable UI order.
pub fn learning_runtime_catalog() -> Vec<LearningRuntimePresetDto> {
    vec![
        LearningRuntimePresetDto {
            id: LearningRuntimePresetId::Csharp,
            name: "C#".into(),
            description: "Run a package-free .NET 10.0.401 test project in the Microsoft SDK image.".into(),
            image_ref: "mcr.microsoft.com/dotnet/sdk:10.0.401".into(),
            command: vec![
                "dotnet".into(),
                "run".into(),
                "--project".into(),
                "checks/Checks.csproj".into(),
                "--property:RestoreIgnoreFailedSources=true".into(),
                "--property:BaseIntermediateOutputPath=/workspace/.lattice-build/dotnet-obj/".into(),
                "--property:BaseOutputPath=/workspace/.lattice-build/dotnet-bin/".into(),
                "--property:UseAppHost=false".into(),
            ],
            limits: runtime_limits(),
            entrypoint_contract: ".NET SDK 10.0.401. Provide checks/Checks.csproj and its C# sources. Keep the project free of external PackageReference dependencies; the SDK restores framework references offline. The runner reserves /workspace/.lattice-build for build output; the project must not use that path. UseAppHost=false keeps execution through the installed dotnet runtime.".into(),
        },
        LearningRuntimePresetDto {
            id: LearningRuntimePresetId::Rust,
            name: "Rust".into(),
            description: "Compile and run standard-library-only Rust 1.98.1 tests.".into(),
            image_ref: "rust:1.98.1-slim-bookworm".into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "mkdir -p /workspace/.lattice-build && rustc --test checks.rs -o /workspace/.lattice-build/rust-tests && /workspace/.lattice-build/rust-tests".into(),
            ],
            limits: runtime_limits(),
            entrypoint_contract: "Rust 1.98.1. Provide checks.rs with Rust #[test] functions. This preset uses rustc and the standard library only; Cargo dependencies and network access are unavailable. The runner reserves /workspace/.lattice-build for its executable; the project must not use that path.".into(),
        },
        LearningRuntimePresetDto {
            id: LearningRuntimePresetId::Node,
            name: "React & TypeScript".into(),
            description: "Type-check and test React 19.2.8 components with TypeScript 5.9.3.".into(),
            image_ref: NODE_PREPARED_IMAGE_REF.into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "ln -s /opt/lattice/node_modules /workspace/node_modules && /opt/lattice/node_modules/.bin/tsc --project tsconfig.json --noEmit && node --import tsx --test checks/checks.test.tsx".into(),
            ],
            limits: runtime_limits(),
            entrypoint_contract: "Node.js 22.23.3 with React and React DOM 19.2.8, TypeScript 5.9.3, jsdom 30.0.1, and Testing Library 16.3.2. Provide React/TypeScript source under src/, a workspace-root tsconfig.json with react-jsx enabled that includes the source and evaluator tests, and checks/checks.test.tsx using node:test. The generated runner checks types, then executes React component tests without network access. The runner reserves /workspace/node_modules; project files must not use that path.".into(),
        },
        LearningRuntimePresetDto {
            id: LearningRuntimePresetId::Python,
            name: "Python".into(),
            description: "Run a Python checks script with isolated interpreter settings.".into(),
            image_ref: "python:3.13.16-slim".into(),
            command: vec![
                "python".into(),
                "-I".into(),
                "-B".into(),
                "-c".into(),
                "import runpy,sys; sys.path.insert(0, '/workspace'); runpy.run_path('/workspace/checks.py', run_name='__main__')".into(),
            ],
            limits: runtime_limits(),
            entrypoint_contract: "Python 3.13.16. Provide checks.py. The trusted launcher explicitly adds /workspace for imports while Python isolated mode ignores user Python configuration; use the standard library because network and package installation are unavailable.".into(),
        },
    ]
}

fn preset_by_id(id: LearningRuntimePresetId) -> Option<LearningRuntimePresetDto> {
    learning_runtime_catalog()
        .into_iter()
        .find(|preset| preset.id == id)
}

fn validate_resolved_image_id(value: &str) -> Result<()> {
    let digest = value.strip_prefix("sha256:").ok_or_else(|| {
        AppError::InvalidData("Container engine returned an unpinned image ID.".into())
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::InvalidData(
            "Container engine returned an invalid SHA-256 image ID.".into(),
        ));
    }
    Ok(())
}

async fn bounded_read<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(8 * 1024));
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if let Some(retained) = chunk.get(..count.min(remaining)) {
            output.extend_from_slice(retained);
        }
    }
    Ok(output)
}

struct CommandOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn run_engine_command(
    engine: LearningContainerEngine,
    args: &[impl AsRef<OsStr>],
    timeout: Duration,
    action: &str,
) -> Result<CommandOutput> {
    let mut child = engine.command()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            AppError::ServiceNotAvailable(format!(
                "{} is unavailable. Install Docker or Podman and start its daemon before {action}: {error}",
                engine.executable()
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        AppError::InternalError("Container setup process did not provide stdout.".into())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        AppError::InternalError("Container setup process did not provide stderr.".into())
    })?;
    let stdout_task = tokio::spawn(bounded_read(stdout, MAX_COMMAND_OUTPUT));
    let stderr_task = tokio::spawn(bounded_read(stderr, MAX_COMMAND_OUTPUT));
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => return Err(error.into()),
        Err(_) => {
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            return Err(AppError::ServiceNotAvailable(format!(
                "Timed out while {action} with {}.",
                engine.executable()
            )));
        }
    };
    let stdout = stdout_task
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))??;
    let stderr = stderr_task
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))??;
    Ok(CommandOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

fn output_hint(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .trim()
        .chars()
        .take(500)
        .collect()
}

async fn resolve_existing_image(
    engine: LearningContainerEngine,
    image_ref: &str,
) -> Result<Option<String>> {
    let output = run_engine_command(
        engine,
        &["image", "inspect", "--format", "{{.Id}}", image_ref],
        INSPECT_TIMEOUT,
        "checking the selected runtime image",
    )
    .await?;
    if !output.success {
        return Ok(None);
    }
    let image_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    validate_resolved_image_id(&image_id)?;
    Ok(Some(image_id))
}

async fn ensure_official_image(engine: LearningContainerEngine, image_ref: &str) -> Result<String> {
    if let Some(image_id) = resolve_existing_image(engine, image_ref).await? {
        return Ok(image_id);
    }
    let pull = run_engine_command(
        engine,
        &["image", "pull", image_ref],
        PULL_TIMEOUT,
        "downloading the selected runtime image",
    )
    .await?;
    if !pull.success {
        let reason = output_hint(&pull.stderr);
        return Err(AppError::ServiceNotAvailable(if reason.is_empty() {
            format!("Could not download runtime image {image_ref}.")
        } else {
            format!("Could not download runtime image {image_ref}: {reason}")
        }));
    }
    resolve_existing_image(engine, image_ref)
        .await?
        .ok_or_else(|| {
            AppError::InvalidData(format!(
                "{} reported a successful image pull but the image is still unavailable.",
                engine.executable()
            ))
        })
}

struct TemporaryBuildContext(PathBuf);

impl Drop for TemporaryBuildContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn node_build_context() -> Result<TemporaryBuildContext> {
    let root = TemporaryBuildContext(
        std::env::temp_dir().join(format!("lattice-node-runtime-{}", uuid::Uuid::new_v4())),
    );
    std::fs::create_dir(&root.0)?;
    std::fs::write(
        root.0.join("Dockerfile"),
        include_str!("../../../resources/learning-node/Dockerfile"),
    )?;
    std::fs::write(
        root.0.join("package.json"),
        include_str!("../../../resources/learning-node/package.json"),
    )?;
    std::fs::write(
        root.0.join("package-lock.json"),
        include_str!("../../../resources/learning-node/package-lock.json"),
    )?;
    Ok(root)
}

async fn prepare_node_image(engine: LearningContainerEngine) -> Result<String> {
    let _base_image_id = ensure_official_image(engine, NODE_BASE_IMAGE_REF).await?;
    let context = node_build_context()?;
    let context_path = context.0.to_str().ok_or_else(|| {
        AppError::InvalidInput("Runtime build context path is not valid Unicode.".into())
    })?;
    let args = vec![
        "build".to_owned(),
        "--tag".to_owned(),
        NODE_PREPARED_IMAGE_REF.to_owned(),
        "--file".to_owned(),
        format!("{context_path}/Dockerfile"),
        context_path.to_owned(),
    ];
    // The build has a fixed context containing only app-owned metadata and
    // package pins. Learner files and project manifests never enter the build.
    let build = run_engine_command(
        engine,
        &args,
        BUILD_TIMEOUT,
        "preparing the React and TypeScript runtime image",
    )
    .await?;
    // Drop early so the temporary build context does not linger if a later
    // engine inspection fails.
    drop(context);
    if !build.success {
        let mut reason = output_hint(&build.stderr);
        if reason.is_empty() {
            reason = output_hint(&build.stdout);
        }
        return Err(AppError::ServiceNotAvailable(if reason.is_empty() {
            "Could not build the React and TypeScript runtime image.".into()
        } else {
            format!("Could not build the React and TypeScript runtime image: {reason}")
        }));
    }
    resolve_existing_image(engine, NODE_PREPARED_IMAGE_REF)
        .await?
        .ok_or_else(|| {
            AppError::InvalidData(
                "Container engine reported a successful Node runtime build but the image is unavailable."
                    .into(),
            )
        })
}

/// Pulls a fixed app-owned preset image when absent and returns its immutable
/// local image ID for use with the existing Learning Studio container runner.
pub async fn prepare_container_preset(
    engine: LearningContainerEngine,
    preset: LearningRuntimePresetId,
) -> Result<PreparedLearningRuntime> {
    let descriptor = preset_by_id(preset).ok_or_else(|| {
        AppError::InvalidInput("The selected runtime preset is unavailable.".into())
    })?;
    let capability = run_engine_command(
        engine,
        &["version", "--format", "{{.Server.Version}}"],
        ENGINE_TIMEOUT,
        "preparing a runtime",
    )
    .await?;
    if !capability.success {
        let reason = output_hint(&capability.stderr);
        return Err(AppError::ServiceNotAvailable(if reason.is_empty() {
            format!(
                "{} daemon is not available. Start the container engine and retry.",
                engine.executable()
            )
        } else {
            format!("{} daemon is not available: {reason}", engine.executable())
        }));
    }

    let image_id = if preset == LearningRuntimePresetId::Node {
        prepare_node_image(engine).await?
    } else {
        ensure_official_image(engine, &descriptor.image_ref).await?
    };

    Ok(PreparedLearningRuntime {
        preset: descriptor,
        image_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::lab_runtime::{
        execute_container_lab, materialize_workspace, LearningLabExecutionSpec, LearningLabFile,
        LearningLabRunStatus,
    };
    use tokio_util::sync::CancellationToken;

    #[test]
    fn catalog_has_fixed_versioned_official_images_and_commands() {
        let catalog = learning_runtime_catalog();
        assert_eq!(catalog.len(), 4);
        assert_eq!(
            catalog.first().map(|item| item.id),
            Some(LearningRuntimePresetId::Csharp)
        );
        for item in &catalog {
            assert!(item.image_ref.contains(':'));
            assert!(!item.image_ref.contains("@sha256:"));
            assert!(!item.command.is_empty());
            assert!(item.limits.validate().is_ok());
        }
        assert!(catalog
            .first()
            .is_some_and(|item| item.image_ref.starts_with("mcr.microsoft.com/dotnet/sdk:")));
        assert!(catalog.get(1).is_some_and(|item| item
            .command
            .iter()
            .any(|arg| arg.contains("rustc --test checks.rs"))));
        assert!(catalog
            .get(2)
            .is_some_and(|item| item.name == "React & TypeScript"
                && item.image_ref == NODE_PREPARED_IMAGE_REF
                && item
                    .command
                    .iter()
                    .any(|argument| argument.contains("tsc --project"))
                && item
                    .command
                    .iter()
                    .any(|argument| argument.contains("node_modules"))
                && item.entrypoint_contract.contains("jsdom")));
        assert!(catalog.get(3).is_some_and(|item| item
            .command
            .iter()
            .any(|arg| arg.contains("sys.path.insert(0, '/workspace')"))));
    }

    #[test]
    fn resolved_image_id_parser_accepts_only_immutable_sha256_ids() {
        assert!(validate_resolved_image_id(&format!("sha256:{}", "a".repeat(64))).is_ok());
        assert!(validate_resolved_image_id("python:3.13-slim").is_err());
        assert!(validate_resolved_image_id("sha256:abcd").is_err());
        assert!(validate_resolved_image_id(&format!("sha256:{}g", "a".repeat(63))).is_err());
    }

    fn smoke_fixture(
        preset: LearningRuntimePresetId,
        correct: bool,
    ) -> (Vec<LearningLabFile>, Vec<String>) {
        match preset {
            LearningRuntimePresetId::Csharp => {
                let solution = if correct {
                    "public static class Solution { public static int Add(int a, int b) => a + b; }"
                } else {
                    "public static class Solution { public static int Add(int a, int b) => a - b; }"
                };
                let project = "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable></PropertyGroup><ItemGroup><Compile Include=\"../Solution.cs\" Link=\"Solution.cs\" /></ItemGroup></Project>";
                let checks =
                    "if (Solution.Add(2, 3) != 5) return 1; Console.WriteLine(\"PASS\"); return 0;";
                (
                    vec![
                        LearningLabFile {
                            path: "Solution.cs".into(),
                            content: solution.into(),
                        },
                        LearningLabFile {
                            path: "checks/Checks.csproj".into(),
                            content: project.into(),
                        },
                        LearningLabFile {
                            path: "checks/Program.cs".into(),
                            content: checks.into(),
                        },
                    ],
                    vec!["checks/Checks.csproj".into(), "checks/Program.cs".into()],
                )
            }
            LearningRuntimePresetId::Rust => {
                let solution = if correct {
                    "pub fn add(a: i32, b: i32) -> i32 { a + b }"
                } else {
                    "pub fn add(a: i32, b: i32) -> i32 { a - b }"
                };
                (
                    vec![
                        LearningLabFile {
                            path: "Solution.rs".into(),
                            content: solution.into(),
                        },
                        LearningLabFile {
                            path: "checks.rs".into(),
                            content: "#[path = \"/workspace/Solution.rs\"] mod solution; #[test] fn adds() { assert_eq!(solution::add(2, 3), 5); }".into(),
                        },
                    ],
                    vec!["checks.rs".into()],
                )
            }
            LearningRuntimePresetId::Node => {
                let count = if correct { "count + 1" } else { "count + 2" };
                let solution = format!(
                    "import {{ useState }} from 'react'; export function Counter() {{ const [count, setCount] = useState(3); return <button onClick={{() => setCount({count})}}>Count: {{count}}</button>; }}"
                );
                let config = "{\"compilerOptions\":{\"target\":\"ES2022\",\"lib\":[\"ES2022\",\"DOM\"],\"module\":\"commonjs\",\"moduleResolution\":\"node\",\"jsx\":\"react-jsx\",\"strict\":true,\"esModuleInterop\":true,\"allowSyntheticDefaultImports\":true,\"skipLibCheck\":true,\"noEmit\":true},\"include\":[\"src/**/*.ts\",\"src/**/*.tsx\",\"checks/checks.test.tsx\"]}";
                let checks = "import assert from 'node:assert/strict'; import { test } from 'node:test'; import { JSDOM } from 'jsdom'; import React from 'react'; test('Counter renders and increments', async () => { const dom = new JSDOM('<!doctype html><html><body></body></html>', { url: 'http://localhost/' }); const globals = globalThis as unknown as Record<string, unknown>; globals.window = dom.window; globals.document = dom.window.document; globals.HTMLElement = dom.window.HTMLElement; Object.defineProperty(globalThis, 'navigator', { value: dom.window.navigator, configurable: true }); const { cleanup, fireEvent, render, screen } = await import('@testing-library/react'); const { Counter } = await import('../src/Counter'); render(React.createElement(Counter)); const button = screen.getByRole('button', { name: 'Count: 3' }); fireEvent.click(button); assert.equal(screen.getByRole('button').textContent, 'Count: 4'); cleanup(); dom.window.close(); });";
                (
                    vec![
                        LearningLabFile {
                            path: "src/Counter.tsx".into(),
                            content: solution,
                        },
                        LearningLabFile {
                            path: "tsconfig.json".into(),
                            content: config.into(),
                        },
                        LearningLabFile {
                            path: "checks/checks.test.tsx".into(),
                            content: checks.into(),
                        },
                    ],
                    vec!["tsconfig.json".into(), "checks/checks.test.tsx".into()],
                )
            }
            LearningRuntimePresetId::Python => {
                let solution = if correct {
                    "def add(a, b):\n    return a + b\n"
                } else {
                    "def add(a, b):\n    return a - b\n"
                };
                (
                    vec![
                        LearningLabFile {
                            path: "solution.py".into(),
                            content: solution.into(),
                        },
                        LearningLabFile {
                            path: "checks.py".into(),
                            content:
                                "import solution\nassert solution.add(2, 3) == 5\nprint('PASS')\n"
                                    .into(),
                        },
                    ],
                    vec!["checks.py".into()],
                )
            }
        }
    }

    async fn run_smoke_fixture(prepared: &PreparedLearningRuntime, correct: bool) -> Result<()> {
        let (files, read_only_paths) = smoke_fixture(prepared.preset.id, correct);
        let spec = LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id: prepared.image_id.clone(),
            command: prepared.preset.command.clone(),
            files,
            read_only_paths,
            limits: prepared.preset.limits.clone(),
        };
        let temporary_root = tempfile::tempdir()?;
        let workspace = temporary_root.path().join("workspace");
        materialize_workspace(&workspace, &spec)?;
        let result = execute_container_lab(
            LearningContainerEngine::Docker,
            &workspace,
            &spec,
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(
            result.status,
            if correct {
                LearningLabRunStatus::Passed
            } else {
                LearningLabRunStatus::Failed
            },
            "stdout: {}\nstderr: {}",
            result.stdout,
            result.stderr
        );
        Ok(())
    }

    async fn smoke_preset(preset: LearningRuntimePresetId) -> Result<()> {
        let prepared = prepare_container_preset(LearningContainerEngine::Docker, preset).await?;
        run_smoke_fixture(&prepared, true).await?;
        run_smoke_fixture(&prepared, false).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires Docker and downloads the selected official runtime image"]
    async fn docker_csharp_catalog_smoke() -> Result<()> {
        smoke_preset(LearningRuntimePresetId::Csharp).await
    }

    #[tokio::test]
    #[ignore = "requires Docker and downloads the selected official runtime image"]
    async fn docker_rust_catalog_smoke() -> Result<()> {
        smoke_preset(LearningRuntimePresetId::Rust).await
    }

    #[tokio::test]
    #[ignore = "requires Docker and builds the app-owned React/TypeScript image"]
    async fn docker_react_typescript_catalog_smoke() -> Result<()> {
        smoke_preset(LearningRuntimePresetId::Node).await
    }

    #[tokio::test]
    #[ignore = "requires Docker and downloads the selected official runtime image"]
    async fn docker_python_catalog_smoke() -> Result<()> {
        smoke_preset(LearningRuntimePresetId::Python).await
    }
}
