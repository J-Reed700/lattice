//! Python case harness: generates checks and parses per-case test results.

use super::CaseHarness;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{Case, CaseResult, CaseStatus, ProblemSpec};
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct PythonHarness;

impl PythonHarness {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
struct RawCaseOutput {
    case_id: String,
    status: String,
    #[serde(default)]
    actual: Option<String>,
    #[serde(default)]
    printed: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    elapsed_ms: Option<u64>,
}

impl CaseHarness for PythonHarness {
    fn evaluator(
        &self,
        problem: &ProblemSpec,
        cases: &[Case],
        nonce: &str,
    ) -> Vec<LearningLabFile> {
        let mut ordered_cases = Vec::with_capacity(cases.len());
        for case in cases {
            if !case.scale {
                ordered_cases.push(case);
            }
        }
        for case in cases {
            if case.scale {
                ordered_cases.push(case);
            }
        }

        let cases_json =
            serde_json::to_string_pretty(&ordered_cases).unwrap_or_else(|_| "[]".to_owned());
        let fn_name = &problem.signature.function;

        let checks_py = format!(
            r#"# Auto-generated test harness for coding problem
import ast
import copy
import io
import json
import os
import sys
import time

NONCE = {nonce:?}

def _safe_eval(expr):
    try:
        return ast.literal_eval(expr)
    except Exception:
        return eval(expr, {{"__builtins__": __builtins__}})

def _compare_unordered(a, b):
    try:
        import collections
        return collections.Counter(a) == collections.Counter(b)
    except Exception:
        pass
    try:
        if len(a) != len(b):
            return False
        b_list = list(b)
        for item in a:
            matched = False
            for i, b_item in enumerate(b_list):
                if b_item == item:
                    del b_list[i]
                    matched = True
                    break
            if not matched:
                return False
        return len(b_list) == 0
    except Exception:
        return False

def _compare_float_within(a, b, tol):
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return abs(a - b) <= tol
    if isinstance(a, (list, tuple)) and isinstance(b, (list, tuple)):
        if len(a) != len(b):
            return False
        return all(_compare_float_within(x, y, tol) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        if set(a.keys()) != set(b.keys()):
            return False
        return all(_compare_float_within(a[k], b[k], tol) for k in a)
    return a == b

def main():
    cases_path = os.path.join(os.path.dirname(__file__), "cases.json")
    if not os.path.exists(cases_path):
        cases_path = "cases.json"
    with open(cases_path, "r", encoding="utf-8") as f:
        cases = json.load(f)

    target_fn = None
    import_error = None
    try:
        import solution
        target_fn = getattr(solution, {fn_name:?}, None)
        if target_fn is None:
            import_error = f"Function '{fn_name}' was not found in solution.py"
    except Exception as e:
        import_error = f"{{type(e).__name__}}: {{e}}"

    any_failed = False

    for case in cases:
        case_id = case.get("id", "")
        category = case.get("category", "")
        args_raw = case.get("args", [])
        expected_raw = case.get("expected")
        compare_spec = case.get("compare", {{"kind": "exact"}})
        visibility = case.get("visibility", "example")

        if import_error is not None:
            any_failed = True
            payload = {{
                "case_id": case_id,
                "status": "error",
                "actual": None,
                "printed": "",
                "error": import_error,
                "elapsed_ms": 0,
                "category": category,
                "args": args_raw,
                "expected": expected_raw,
                "visibility": visibility,
            }}
            print(f"{{NONCE}}{{json.dumps(payload)}}", flush=True)
            continue

        try:
            evaluated_args = [_safe_eval(a) for a in args_raw]
        except Exception as e:
            any_failed = True
            payload = {{
                "case_id": case_id,
                "status": "error",
                "actual": None,
                "printed": "",
                "error": f"Failed to evaluate arguments: {{type(e).__name__}}: {{e}}",
                "elapsed_ms": 0,
                "category": category,
                "args": args_raw,
                "expected": expected_raw,
                "visibility": visibility,
            }}
            print(f"{{NONCE}}{{json.dumps(payload)}}", flush=True)
            continue

        captured_stdout = io.StringIO()
        old_stdout = sys.stdout
        sys.stdout = captured_stdout

        actual = None
        error_msg = None
        status = "passed"
        start_t = time.perf_counter()

        try:
            call_args = copy.deepcopy(evaluated_args)
            actual = target_fn(*call_args)
        except BaseException as e:
            error_msg = f"{{type(e).__name__}}: {{e}}" if str(e) else type(e).__name__
            status = "error"
            any_failed = True
        finally:
            end_t = time.perf_counter()
            sys.stdout = old_stdout

        printed = captured_stdout.getvalue().rstrip("\r\n")
        elapsed_ms = max(0, int((end_t - start_t) * 1000))

        if status != "error":
            if expected_raw is None:
                status = "ran"
            else:
                try:
                    expected_val = _safe_eval(expected_raw)
                    cmp_kind = compare_spec.get("kind", "exact") if isinstance(compare_spec, dict) else compare_spec
                    if cmp_kind == "exact":
                        passed = (actual == expected_val)
                    elif cmp_kind == "unordered":
                        passed = _compare_unordered(actual, expected_val)
                    elif cmp_kind == "float_within":
                        tol = compare_spec.get("tolerance", 1e-6) if isinstance(compare_spec, dict) else 1e-6
                        passed = _compare_float_within(actual, expected_val, tol)
                    else:
                        passed = (actual == expected_val)
                    if passed:
                        status = "passed"
                    else:
                        status = "failed"
                        any_failed = True
                except Exception as e:
                    status = "error"
                    error_msg = f"Expected value evaluation error: {{type(e).__name__}}: {{e}}"
                    any_failed = True

        payload = {{
            "case_id": case_id,
            "status": status,
            "actual": repr(actual) if status != "error" else None,
            "printed": printed,
            "error": error_msg,
            "elapsed_ms": elapsed_ms,
            "category": category,
            "args": args_raw,
            "expected": expected_raw,
            "visibility": visibility,
        }}
        print(f"{{NONCE}}{{json.dumps(payload)}}", flush=True)

    if any_failed:
        sys.exit(1)

if __name__ == "__main__":
    main()
"#
        );

        vec![
            LearningLabFile {
                path: "cases.json".to_owned(),
                content: cases_json,
            },
            LearningLabFile {
                path: "checks.py".to_owned(),
                content: checks_py,
            },
        ]
    }

    fn results(&self, stdout: &str, nonce: &str) -> Vec<CaseResult> {
        let mut results = Vec::new();
        for line in stdout.lines() {
            if let Some(idx) = line.find(nonce) {
                if let Some(json_str) = line.get(idx + nonce.len()..) {
                    if let Ok(raw) = serde_json::from_str::<RawCaseOutput>(json_str.trim()) {
                        let status = match raw.status.to_lowercase().as_str() {
                            "passed" => CaseStatus::Passed,
                            "failed" => CaseStatus::Failed,
                            "error" => CaseStatus::Error,
                            "ran" => CaseStatus::Ran,
                            "timed_out" => CaseStatus::TimedOut,
                            "not_run" => CaseStatus::NotRun,
                            _ => CaseStatus::Error,
                        };
                        results.push(CaseResult {
                            case_id: raw.case_id,
                            status,
                            actual: raw.actual,
                            printed: raw.printed.unwrap_or_default(),
                            error: raw.error,
                            elapsed_ms: raw.elapsed_ms,
                        });
                    }
                }
            }
        }
        results
    }
}
