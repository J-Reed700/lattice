"""Applies the integrated tree's compile fixes (5e1c685c8) to a tests-first tree that lacks them.

The review branches' tests-first commits were never compiled before CI, and four slips stop their
test builds: a json! value that is a method call on an array, a struct field added without the test
literals that build the struct, and a trait method called where the trait is not imported. Each fix
here touches only what the compiler refused, and prints what it changed.

Run from the root of the checked-out tree.
"""

import pathlib
import re

SRC = pathlib.Path("crates/lattice-learning/src")
LESSONS = SRC / "lessons"
applied = []


def bind_json_array_values():
    path = LESSONS / "content_verification/review_pass/put_back_tests.rs"
    if not path.exists():
        return
    lines = path.read_text(encoding="utf-8").split("\n")
    value = '[ALPHA, ECHO].join(" ")'
    while True:
        hit = next((i for i, line in enumerate(lines) if f'"body": {value}' in line), None)
        if hit is None:
            break
        start = next(j for j in range(hit, -1, -1) if "json!(" in lines[j] and lines[j].lstrip().startswith("let "))
        indent = lines[start][: len(lines[start]) - len(lines[start].lstrip())]
        lines[hit] = lines[hit].replace(value, "body_a")
        lines.insert(start, f"{indent}let body_a = {value};")
        applied.append(f"{path}:{hit + 1}: bound the json! array value before the macro")
    path.write_text("\n".join(lines), encoding="utf-8")


def literal_ends(text, open_brace):
    depth = 0
    for at in range(open_brace, len(text)):
        if text[at] == "{":
            depth += 1
        elif text[at] == "}":
            depth -= 1
            if depth == 0:
                return at
    raise ValueError("unbalanced braces")


def add_missing_field(struct, field, value, declared_in, declaration):
    if not declared_in.exists() or declaration not in declared_in.read_text(encoding="utf-8"):
        return
    for path in sorted(SRC.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        changed = False
        search_from = 0
        while True:
            match = re.compile(rf"\b{struct}\s*\{{").search(text, search_from)
            if match is None:
                break
            line_start = text.rfind("\n", 0, match.start()) + 1
            before = text[line_start : match.start()].rstrip()
            open_brace = match.end() - 1
            close = literal_ends(text, open_brace)
            search_from = match.end()
            if before.endswith(("->", "struct", "impl", "for", "enum")):
                continue
            body = text[open_brace + 1 : close]
            if re.search(rf"\b{field}\b", body) or re.search(r"^\s*\.\.", body, re.MULTILINE):
                continue
            field_lines = [line for line in body.split("\n") if line.strip()]
            indent = field_lines[-1][: len(field_lines[-1]) - len(field_lines[-1].lstrip())]
            last = body.rstrip()
            separator = "" if last.endswith(",") else ","
            insert_at = open_brace + 1 + len(last)
            text = f"{text[:insert_at]}{separator}\n{indent}{field}: {value},{text[insert_at:]}"
            line_no = text.count("\n", 0, match.start()) + 1
            applied.append(f"{path}:{line_no}: set {struct}.{field} to {value}")
            changed = True
        if changed:
            path.write_text(text, encoding="utf-8")


def import_claim_review_store():
    path = LESSONS / "claim_reviews/service/section_edit.rs"
    if not path.exists():
        return
    text = path.read_text(encoding="utf-8")
    if ".load(" not in text or "ClaimReviewStore" in text:
        return
    line = "use lattice_core::application::ports::claim_review::ClaimReviewStore;\n"
    at = text.index("use lattice_core::")
    path.write_text(text[:at] + line + text[at:], encoding="utf-8")
    applied.append(f"{path}: imported ClaimReviewStore for its load calls")


bind_json_array_values()
add_missing_field(
    "LessonVerificationReport",
    "replaced_findings",
    "Vec::new()",
    LESSONS / "content_verification/mod.rs",
    "replaced_findings: Vec<",
)
add_missing_field(
    "TeachingFindingDto",
    "part_changed",
    "false",
    LESSONS / "claim_reviews/teaching_findings.rs",
    "pub part_changed: bool",
)
import_claim_review_store()
print("\n".join(applied) if applied else "no compile fixes needed")
