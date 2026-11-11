import ast
import contextlib
import copy
import hashlib
import io
import json
import os
import sys
import time

BASE = os.path.dirname(globals().get("__file__") or "")
if BASE and BASE not in sys.path:
    sys.path.insert(0, BASE)

SCALE_NAMES = {
    "range": range,
    "list": list,
    "tuple": tuple,
    "dict": dict,
    "set": set,
    "str": str,
    "int": int,
    "len": len,
    "sum": sum,
    "min": min,
    "max": max,
    "sorted": sorted,
    "reversed": reversed,
    "zip": zip,
    "enumerate": enumerate,
}

SCALE_NODES = (
    ast.Expression,
    ast.Constant,
    ast.Name,
    ast.Load,
    ast.Call,
    ast.keyword,
    ast.BinOp,
    ast.UnaryOp,
    ast.operator,
    ast.unaryop,
    ast.List,
    ast.Tuple,
    ast.Dict,
    ast.Set,
    ast.ListComp,
    ast.SetComp,
    ast.DictComp,
    ast.GeneratorExp,
    ast.comprehension,
    ast.Store,
    ast.Compare,
    ast.cmpop,
    ast.BoolOp,
    ast.boolop,
    ast.IfExp,
    ast.Subscript,
    ast.Slice,
)


def describe(error):
    text = str(error)
    name = type(error).__name__
    return name + ": " + text if text else name


def scale_value(source):
    tree = ast.parse(source, mode="eval")
    bound = {
        name.id
        for node in ast.walk(tree)
        if isinstance(node, ast.comprehension)
        for name in ast.walk(node.target)
        if isinstance(name, ast.Name)
    }
    for node in ast.walk(tree):
        if not isinstance(node, SCALE_NODES):
            raise ValueError("not allowed in a scale input: " + type(node).__name__)
        if isinstance(node, ast.Call) and not (
            isinstance(node.func, ast.Name)
            and node.func.id in SCALE_NAMES
            and node.func.id not in bound
        ):
            raise ValueError("not allowed in a scale input: a call of something else")
        if (
            isinstance(node, ast.Name)
            and node.id not in SCALE_NAMES
            and node.id not in bound
        ):
            raise ValueError("not allowed in a scale input: " + node.id)
    scope = {"__builtins__": {}}
    scope.update(SCALE_NAMES)
    return eval(compile(tree, "<scale input>", "eval"), scope)


def argument(source, scale):
    return scale_value(source) if scale else ast.literal_eval(source)


def close(left, right, tolerance):
    if isinstance(left, (list, tuple)) and isinstance(right, (list, tuple)):
        return len(left) == len(right) and all(
            close(a, b, tolerance) for a, b in zip(left, right)
        )
    if isinstance(left, bool) or isinstance(right, bool):
        return left == right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return left == right or abs(left - right) <= tolerance
    return left == right


def same_elements(left, right):
    try:
        return sorted(map(repr, left)) == sorted(map(repr, right))
    except TypeError:
        return left == right


def matches(kind, result, expected):
    name = kind.get("kind", "exact")
    if name == "unordered":
        return same_elements(result, expected)
    if name == "float_within":
        return close(result, expected, kind.get("tolerance", 0.0))
    return result == expected


def canonical(kind, result):
    if kind.get("kind", "exact") == "unordered":
        try:
            return repr(sorted(map(repr, result)))
        except TypeError:
            return repr(result)
    return repr(result)


def emit(nonce, case_id, status, actual=None, printed="", error=None, elapsed=0):
    line = json.dumps(
        {
            "caseId": case_id,
            "status": status,
            "actual": actual,
            "printed": printed,
            "error": error,
            "elapsedMs": elapsed,
        }
    )
    sys.stdout.write(nonce + line + "\n")
    sys.stdout.flush()


def main():
    with open(os.path.join(BASE, "cases.json"), encoding="utf-8") as handle:
        spec = json.load(handle)
    nonce = spec["nonce"]
    function = spec["function"]
    cases = spec["cases"]

    target = None
    load_error = None
    imported = io.StringIO()
    try:
        with contextlib.redirect_stdout(imported):
            import solution
        target = getattr(solution, function, None)
        if target is None:
            load_error = "NameError: " + function + " is not defined in solution.py"
    except BaseException as error:
        load_error = describe(error)
    text = imported.getvalue()
    if text:
        sys.stdout.write(text if text.endswith("\n") else text + "\n")
        sys.stdout.flush()

    failing = False
    for case in cases:
        case_id = case["id"]
        scale = bool(case.get("scale"))
        if load_error is not None:
            emit(nonce, case_id, "error", error=load_error)
            failing = True
            continue
        try:
            arguments = [argument(source, scale) for source in case["args"]]
            call_arguments = copy.deepcopy(arguments)
        except Exception as error:
            emit(nonce, case_id, "error", error="bad arguments: " + describe(error))
            failing = True
            continue
        sink = io.StringIO()
        failure = None
        result = None
        started = time.perf_counter()
        try:
            with contextlib.redirect_stdout(sink):
                result = target(*call_arguments)
        except BaseException as error:
            failure = describe(error)
        elapsed = int((time.perf_counter() - started) * 1000)
        printed = sink.getvalue().rstrip("\r\n")
        if failure is not None:
            emit(nonce, case_id, "error", printed=printed, error=failure, elapsed=elapsed)
            failing = True
            continue
        kind = case.get("compare") or {}
        digest = case.get("digest")
        expected_source = case.get("expected")
        try:
            if scale:
                actual = hashlib.sha256(canonical(kind, result).encode("utf-8")).hexdigest()
                status = "ran" if digest is None else ("passed" if actual == digest else "failed")
            else:
                actual = repr(result)
                if expected_source is None:
                    status = "ran"
                else:
                    expected = ast.literal_eval(expected_source)
                    status = "passed" if matches(kind, result, expected) else "failed"
        except Exception as error:
            emit(nonce, case_id, "error", printed=printed, error=describe(error), elapsed=elapsed)
            failing = True
            continue
        if status == "failed":
            failing = True
        emit(nonce, case_id, status, actual=actual, printed=printed, elapsed=elapsed)
    if failing:
        sys.exit(1)


main()
