//! Test-only smoke validation for packaged embedded learning runtimes.
//!
//! The executable entry point is compiled by `main.rs` only with the
//! `desktop-e2e` feature. It accepts an installed resource directory but no
//! learner source, commands, or shell arguments.

use lattice::features::learning::embedded_runtime::{
    execute_builtin_lab, BuiltinLabExecutionSpec, LearningBuiltinRuntime,
};
use lattice::features::learning::lab_runtime::{
    LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use lattice::features::learning::python_runtime::configure_python_runtime;
use serde_json::json;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

/// Run fixed Python and JavaScript correctness and containment checks against
/// the exact resources used by the signed desktop application.
pub fn run(resource_root: PathBuf) -> Result<(), Box<dyn Error>> {
    configure_python_runtime(resource_root)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()?;
    let results = runtime.block_on(async {
        let python_ok = execute_builtin_lab(
            python_spec(
                "def add(left, right):\n    return left + right\n",
                "import errno, os, solution\nassert solution.add(20, 22) == 42\nassert set(os.environ) <= {'PYTHONHOME', 'PYTHONDONTWRITEBYTECODE', 'PYTHONNOUSERSITE'}\ntry:\n open('/etc/passwd').read()\nexcept OSError:\n pass\nelse:\n raise AssertionError('host file access was available')\ntry:\n open('/work/self-test-write.txt', 'w').write('no')\nexcept OSError:\n pass\nelse:\n raise AssertionError('workspace writes were available')\n# Direct _socket use bypasses getaddrinfo, so missing DNS support cannot mask socket access.\ntry:\n import _socket\nexcept ImportError:\n pass\nelse:\n denied = {getattr(errno, name) for name in ('EPERM', 'EACCES', 'ENOSYS', 'ENOTSUP', 'EOPNOTSUPP', 'EAFNOSUPPORT', 'EPROTONOSUPPORT', 'ENOPROTOOPT') if hasattr(errno, name)}\n try:\n  raw = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)\n except NotImplementedError:\n  pass\n except OSError as error:\n  assert error.errno in denied, f'socket creation failed unexpectedly: {error!r}'\n else:\n  try:\n   raw.connect(('192.0.2.1', 9))\n  except NotImplementedError:\n   pass\n  except OSError as error:\n   assert error.errno in denied, f'socket connect failed unexpectedly: {error!r}'\n  else:\n   raise AssertionError('network connection succeeded')\n  finally:\n   raw.close()\nprint('python-pass')\n",
            ),
            CancellationToken::new(),
        )
        .await?;
        expect(&python_ok, LearningLabRunStatus::Passed, "Python success")?;
        expect_output(&python_ok.stdout, "python-pass", "Python success")?;

        let python_fail = execute_builtin_lab(
            python_spec(
                "def add(left, right):\n    return left - right\n",
                "import solution\nassert solution.add(20, 22) == 42, 'fixed Python assertion'\n",
            ),
            CancellationToken::new(),
        )
        .await?;
        expect(&python_fail, LearningLabRunStatus::Failed, "Python failure")?;
        expect_output(&python_fail.stderr, "fixed Python assertion", "Python failure")?;

        let javascript_ok = execute_builtin_lab(
            javascript_spec(
                "export const add = (left, right) => left + right;\n",
                "import { add } from './solution.mjs';\nif (add(20, 22) !== 42) throw Error('wrong sum');\nfor (const name of ['process', 'require', 'fetch', '__TAURI_INTERNALS__']) { if (typeof globalThis[name] !== 'undefined') throw Error(name); }\nconsole.log('javascript-pass');\n",
            ),
            CancellationToken::new(),
        )
        .await?;
        expect(&javascript_ok, LearningLabRunStatus::Passed, "JavaScript success")?;
        expect_output(&javascript_ok.stdout, "javascript-pass", "JavaScript success")?;

        let javascript_fail = execute_builtin_lab(
            javascript_spec(
                "export const add = (left, right) => left - right;\n",
                "import { add } from './solution.mjs';\nif (add(20, 22) !== 42) throw Error('fixed JavaScript assertion');\n",
            ),
            CancellationToken::new(),
        )
        .await?;
        expect(&javascript_fail, LearningLabRunStatus::Failed, "JavaScript failure")?;
        expect_output(&javascript_fail.stderr, "fixed JavaScript assertion", "JavaScript failure")?;

        Ok::<_, Box<dyn Error>>(json!({
            "status": "passed",
            "python": { "success": python_ok.status, "failure": python_fail.status, "hostAccess": "denied" },
            "javascript": { "success": javascript_ok.status, "failure": javascript_fail.status, "hostGlobals": "denied" }
        }))
    })?;

    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

fn python_spec(solution: &str, checks: &str) -> BuiltinLabExecutionSpec {
    spec(
        LearningBuiltinRuntime::Python,
        "solution.py",
        solution,
        "checks.py",
        checks,
    )
}

fn javascript_spec(solution: &str, checks: &str) -> BuiltinLabExecutionSpec {
    spec(
        LearningBuiltinRuntime::Javascript,
        "solution.mjs",
        solution,
        "checks.mjs",
        checks,
    )
}

fn spec(
    runtime: LearningBuiltinRuntime,
    solution_path: &str,
    solution: &str,
    checks_path: &str,
    checks: &str,
) -> BuiltinLabExecutionSpec {
    BuiltinLabExecutionSpec {
        run_id: uuid::Uuid::new_v4().to_string(),
        runtime,
        files: vec![
            LearningLabFile {
                path: solution_path.into(),
                content: solution.into(),
            },
            LearningLabFile {
                path: checks_path.into(),
                content: checks.into(),
            },
        ],
        read_only_paths: vec![checks_path.into()],
        limits: LearningLabLimits::default(),
    }
}

fn expect(
    result: &lattice::features::learning::lab_runtime::LearningLabExecutionResult,
    expected: LearningLabRunStatus,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    if result.status != expected {
        return Err(io::Error::other(format!(
            "{label} returned {:?}; expected {:?}; stdout={:?}; stderr={:?}",
            result.status, expected, result.stdout, result.stderr
        ))
        .into());
    }
    Ok(())
}

fn expect_output(output: &str, expected: &str, label: &str) -> Result<(), Box<dyn Error>> {
    if !output.contains(expected) {
        return Err(io::Error::other(format!(
            "{label} output did not contain {expected:?}: {output:?}"
        ))
        .into());
    }
    Ok(())
}
