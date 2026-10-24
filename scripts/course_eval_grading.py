#!/usr/bin/env python3
"""The quiz grading set of the course eval: is the app's grader right, and does it say why?

Design: docs/design/2026-10-23-quizzes-and-tests.md sections 4.3a and 7.2; protocol: evals/courses/README.md
("Grading set"). A set is a fixed bank (`bank-v1.json`) and learner answers to it (`quiz-grading-v1.jsonl`), each
row one answer by one persona. Rows the deterministic grader decides carry their expected points (checked here
against the reference arithmetic of section 3); rows that go to the model (explanations, short answers that match
no variant) are scored against a person's blind labels. The grades come from the app's grader run on the set
(`quiz-grading-grades/1`, written by the live hook).

  validate [--set NAME]                    the registry, every set and bank, and the frozen hashes
  score NAME GRADES --out RESULTS          items.json, answers.jsonl, scored.jsonl (+ manifest) in the chat eval's
                                           shape, rationale.jsonl, deterministic.json; a results dir is never
                                           overwritten
  label RESULTS [--seed N]                 the blind labelling file: every model-graded row, through
                                           `calibration-sample` unchanged, plus human_points
  reasons RESULTS [--labels FILE]          the reason-labelling file: rows the grader marked down that a person has
                                           already labelled blind; kept labels survive a redraw
  report RESULTS [--labels FILE]           summary.json and summary.md: agreement, false-pass and false-fail rates,
                                           the PPI-corrected pass rate, point error, rationale quality, reasons,
                                           contest stability, deterministic exactness
  freeze NAME --baseline RESULTS           record the set's hashes once its baseline is committed

`shipped_path_stats.py calibration-score RESULTS/calibration-labels.json` reads the label file unchanged.
"""
import argparse
import hashlib
import json
import math
import re
import statistics
from datetime import date
from pathlib import Path

import course_eval_quiz as cq
import shipped_path_stats as sps
from chat_rag_eval import load_jsonl
from eval_integrity import SCHEMA_VERSION, digest, unique_records, wilson

ROOT = Path(__file__).resolve().parent.parent
GRADING_DIR = ROOT / "evals" / "courses" / "grading"
REGISTRY = GRADING_DIR / "sets.json"
REGISTRY_SCHEMA = "quiz-grading-sets/1"
BANK_SCHEMA = "quiz-grading-bank/1"
GRADES_SCHEMA = "quiz-grading-grades/1"
ITEMS_SCHEMA = "quiz-grading-items/1"
REASONS_SCHEMA = "quiz-grading-reasons/1"
PERSONAS = ("correct", "near_miss", "partial", "wrong", "off_topic", "empty", "contesting")
REASON_LABELS = ("right", "verdict_right_reason_wrong", "reason_fabricated")
VERDICTS = ("met", "partly_met", "missed", "uncertain")
AUTHOR = re.compile(r"hand|model:\S+")
STABILITY_LIMIT = 0.10  # section 7.2: a regrade changes its verdict on an unchanged answer at most 10 % of the time
SEEDS = 3               # section 7.2: across three seeds
# `full_credit_includes` must name what is missing, never be a model answer to copy. It reproduces the reference
# answer when it holds the whole of it, or COPY_SHARE of its word trigrams: half the reference, word for word, is
# enough to copy its substance into a retake.
COPY_NGRAM, COPY_SHARE = 3, 0.5
EPSILON = 1e-6
# Section 7.2's predeclared bar on reasons (set by the owner on 2026-10-24, a B or better): no reason a person
# labels fabricated, and at least 83 % of marked-down answers' reasons accepted. A set may declare its own
# in sets.json ("reason_bar"), and `score` may override it before any label exists; either way the bar a run is
# judged by is fixed in its manifest when it is scored, never chosen after the labels are in.
REASON_BAR = {"max_fabricated": 0, "min_accepted_share": 0.83}
ALPHA = 0.05
LABEL_RULE = ("Pass when the answer earns every point it can earn; fail when it should lose any. Set human_points to "
              "the points you would award.")
SHORT_ANSWER_RULE = ("Pass when the answer means the same as an accepted answer; fail when it does not. Set "
                     "human_points to the item's points for a pass and 0 for a fail.")


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_json(path, default=None):
    return json.loads(Path(path).read_text()) if Path(path).exists() else default


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def write_jsonl(path, rows):
    with Path(path).open("x") as out:
        for row in rows:
            out.write(json.dumps(row, ensure_ascii=False) + "\n")


def relative(path):
    path = Path(path).resolve()
    return str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)


def ws(text):
    """Whitespace-normalised text: the comparison section 4.3a uses for verbatim spans and quotes."""
    return " ".join(str(text or "").split())


# --- the set: rows, bank, registry -------------------------------------------

def response_text(response):
    """A response as one line of text, for the labeller and the verbatim span check."""
    if "text" in response:
        return response["text"] or ""
    return json.dumps(response, ensure_ascii=False)


def is_empty(response):
    values = list(response.values())
    return all(v in ("", None) or (isinstance(v, list) and all(x in ("", None) for x in v)) for v in values)


def response_problems(item, response):
    field = cq.RESPONSE_FIELD[item["format"]]
    if not isinstance(response, dict) or set(response) != {field}:
        return [f"response must hold exactly {field!r}"]
    value = response[field]
    if field == "text":
        return [] if isinstance(value, str) else ["text must be a string"]
    if not isinstance(value, list):
        return [f"{field} must be a list"]
    if field == "selected_indices" and not all(isinstance(v, int) and 0 <= v < len(item["options"]) for v in value):
        return ["selected_indices out of range"]
    if field == "matches" and not all(v is None or (isinstance(v, int) and 0 <= v < len(item["right"])) for v in value):
        return ["matches out of range"]
    if field in ("blank_responses", "ordered_values") and not all(isinstance(v, str) for v in value):
        return [f"{field} must hold strings"]
    return []


def expected_for(item, response):
    graded = cq.grade_keyed(item, response)
    return ({"path": "model"} if graded["path"] == "model"
            else {"path": "deterministic", "points": graded["earned"]})


def bank_problems(bank):
    problems = []
    if bank.get("schema") != BANK_SCHEMA:
        return [f"bank is not {BANK_SCHEMA}"]
    subjects, sources = bank.get("subjects") or {}, bank.get("sources") or {}
    if len(subjects) < 3 or all(s.get("software", True) for s in subjects.values()):
        problems.append("the bank needs three subjects, at least one not about software")
    try:
        items = unique_records(bank.get("items") or [], "id")
    except ValueError as error:
        return problems + [str(error)]
    for item_id, item in items.items():
        problems += [f"{item_id}: {p}" for p in cq.format_problems(item)]
        if item.get("subject") not in subjects:
            problems.append(f"{item_id}: unknown subject {item.get('subject')!r}")
        if item.get("format") == "explanation" and not str(item.get("reference_answer") or "").strip():
            problems.append(f"{item_id}: an explanation item needs a reference_answer for the copy check")
        has_anchor, verbatim = cq.anchored(item, sources)
        if not has_anchor:
            problems.append(f"{item_id}: no lesson block or source quote")
        if verbatim is False or (item.get("source_quote") and item.get("source_version_id") not in sources):
            problems.append(f"{item_id}: source quote is not verbatim in its source")
    return problems


def set_problems(rows, bank):
    """Everything wrong with a set; empty when it is valid. Expected points are recomputed, never trusted."""
    problems = bank_problems(bank)
    items = {item.get("id"): item for item in bank.get("items") or []}
    try:
        unique_records(rows, "id")
    except ValueError as error:
        return problems + [str(error)]
    formats, subjects = set(), set()
    for row in rows:
        rid = row["id"]
        item = items.get(row.get("item"))
        if "/" in rid:
            problems.append(f"{rid}: ids must not contain '/'")
        if item is None:
            problems.append(f"{rid}: unknown item {row.get('item')!r}")
            continue
        formats.add(item["format"])
        subjects.add(item.get("subject"))
        if row.get("persona") not in PERSONAS:
            problems.append(f"{rid}: persona must be one of {', '.join(PERSONAS)}")
        if not isinstance(row.get("author"), str) or not AUTHOR.fullmatch(row["author"]):
            problems.append(f"{rid}: author must be 'hand' or 'model:<name>'")
        elif row["author"].startswith("model:") and row.get("reviewed") is not True:
            problems.append(f"{rid}: a model-written row enters the set only after a person reviews it")
        response = row.get("response")
        found = response_problems(item, response)
        problems += [f"{rid}: {p}" for p in found]
        if found:
            continue
        note = str(row.get("contest_note") or "").strip()
        if (row.get("persona") == "contesting") != bool(note):
            problems.append(f"{rid}: a contest_note belongs on contesting rows, and every contesting row has one")
        if row.get("persona") == "contesting" and expected_for(item, response)["path"] != "model":
            problems.append(f"{rid}: only model-graded answers can be contested (section 4.4)")
        if (row.get("persona") == "empty") != is_empty(response):
            problems.append(f"{rid}: an empty persona has an empty response, and only it does")
        expected, recomputed = row.get("expected") or {}, expected_for(item, response)
        if expected.get("path") != recomputed["path"] or abs((expected.get("points") or 0)
                                                             - (recomputed.get("points") or 0)) > EPSILON:
            problems.append(f"{rid}: expected {expected} but section 3's arithmetic gives {recomputed}")
    missing = [f for f in cq.FORMATS if f not in formats]
    if missing:
        problems.append(f"no rows for {', '.join(missing)}")
    if len(subjects) < 3:
        problems.append("rows must cover three subjects")
    return problems


def load_registry(path=REGISTRY):
    registry = read_json(path)
    if not registry or registry.get("schema") != REGISTRY_SCHEMA:
        raise SystemExit(f"{path} is not {REGISTRY_SCHEMA}")
    return registry


def load_set(name, registry_path=REGISTRY):
    """(rows, bank, entry, problems) for one named set, frozen hashes included in the problems."""
    registry_path = Path(registry_path)
    entry = load_registry(registry_path)["sets"].get(name)
    if entry is None:
        raise SystemExit(f"no set {name!r} in {registry_path}")
    rows_path, bank_path = registry_path.parent / entry["rows"], registry_path.parent / entry["bank"]
    rows, bank = load_jsonl(rows_path), read_json(bank_path)
    problems = set_problems(rows, bank)
    frozen = entry.get("frozen")
    if frozen:
        for label, path, key in (("rows", rows_path, "rows_sha256"), ("bank", bank_path, "bank_sha256")):
            if sha256_file(path) != frozen.get(key):
                problems.append(f"{path.name} changed after {name} was frozen; make a new version instead")
    return rows, bank, entry | {"rows_path": rows_path, "bank_path": bank_path}, problems


def validate(registry_path=REGISTRY, only=None):
    """{set name: problems} for every set in the registry."""
    registry = load_registry(registry_path)
    return {name: load_set(name, registry_path)[3] for name in registry["sets"] if only in (None, name)}


def freeze(name, baseline, registry_path=REGISTRY):
    registry_path = Path(registry_path)
    registry = load_registry(registry_path)
    entry = registry["sets"].get(name)
    if entry is None:
        raise SystemExit(f"no set {name!r}")
    if entry.get("frozen"):
        raise SystemExit(f"{name} is already frozen; copy it to a new version and register that")
    _, _, loaded, problems = load_set(name, registry_path)
    if problems:
        raise SystemExit(f"{name} is not valid: " + "; ".join(problems))
    if not (Path(baseline) / "scored.jsonl").exists():
        raise SystemExit(f"{baseline} holds no scored.jsonl; freeze after the baseline is scored")
    entry["frozen"] = {"rows_sha256": sha256_file(loaded["rows_path"]), "bank_sha256": sha256_file(loaded["bank_path"]),
                       "baseline": relative(baseline), "frozen_at": date.today().isoformat()}
    write_json(registry_path, registry)
    return entry["frozen"]


# --- rationale quality (section 4.3a) ------------------------------------------

def reference_containment(text, reference):
    """The share of the reference answer's word trigrams found in `text` (1.0 when the whole of it is there)."""
    ref, said = cq.words(reference), cq.words(text)
    if not ref:
        return None
    if f" {' '.join(ref)} " in f" {' '.join(said)} ":
        return 1.0
    grams = cq.ngrams(ref, COPY_NGRAM)
    return len(grams & cq.ngrams(said, COPY_NGRAM)) / len(grams) if grams else 0.0


def rationale_checks(rationale, answer, sources, reference, earned, possible):
    """Section 4.3a's checks on one rationale, counted rather than repaired, so the eval sees what the model did."""
    criteria = rationale.get("criteria") or []
    spans = [w for c in criteria for w in c.get("your_words") or []]
    spans_ok = [bool(ws(w)) and ws(w) in ws(answer) for w in spans]
    quotes = [c for c in criteria if str(c.get("source_quote") or "").strip()]
    quotes_ok = [ws(c["source_quote"]) in ws(sources.get(c.get("source_version_id"), "\0")) for c in quotes]
    consistent, lost, unknown = [], [], []
    for c in criteria:
        verdict, points, top = c.get("verdict"), c.get("points"), c.get("max_points")
        if verdict not in VERDICTS:
            unknown.append(c.get("criterion_id"))
            continue
        if verdict != "uncertain":
            consistent.append(points is not None and top is not None and (
                (verdict == "missed" and abs(points) < EPSILON) or (verdict == "met" and abs(points - top) < EPSILON)
                or (verdict == "partly_met" and EPSILON < points < top - EPSILON)))
        if points is not None and top is not None and points < top - EPSILON:
            lost.append(bool(str(c.get("what_was_missing") or "").strip()))
    containment = reference_containment(" ".join(str(c.get("full_credit_includes") or "") for c in criteria), reference)
    copies = None if containment is None else containment >= COPY_SHARE
    summary = bool(str(rationale.get("summary") or "").strip())
    marked_down = earned is not None and earned < possible - EPSILON
    unexplained = marked_down and (not summary or not any(lost))
    total = sum(c["points"] for c in criteria if c.get("points") is not None)
    sum_ok = earned is None or not criteria or abs(total - earned) < EPSILON
    problems = ([f"your_words not verbatim: {w!r}" for w, ok in zip(spans, spans_ok) if not ok]
                + [f"source_quote not verbatim: {c['source_quote']!r}" for c, ok in zip(quotes, quotes_ok) if not ok]
                + (["points disagree with a verdict"] if not all(consistent) else [])
                + (["lost points without what_was_missing"] if not all(lost) else [])
                + ([f"unknown verdict on {unknown}"] if unknown else [])
                + (["full_credit_includes reproduces the reference answer"] if copies else [])
                + (["marked down without a reason"] if unexplained else [])
                + (["criterion points do not add up to the grade"] if not sum_ok else []))
    return {"spans": len(spans), "spans_verbatim": sum(spans_ok), "quotes": len(quotes),
            "quotes_verbatim": sum(quotes_ok), "criteria": len(consistent), "consistent": sum(consistent),
            "lost": len(lost), "lost_with_missing": sum(lost), "summary": summary, "containment": containment,
            "copies_reference": copies, "unexplained": unexplained, "problems": problems, "passed": not problems}


# --- score -------------------------------------------------------------------

def load_grades(path, rows_path, bank_path):
    grades = read_json(path)
    if not grades or grades.get("schema") != GRADES_SCHEMA:
        raise SystemExit(f"{path} is not {GRADES_SCHEMA}")
    for key, source in (("rows_sha256", rows_path), ("bank_sha256", bank_path)):
        if grades.get(key) != sha256_file(source):
            raise SystemExit(f"{path} graded another version of {Path(source).name}")
    first, regrades = {}, {}
    for grade in grades.get("grades") or []:
        if grade.get("pass", "first") == "first":
            if grade["row_id"] in first:
                raise SystemExit(f"{path}: two first grades for {grade['row_id']}")
            first[grade["row_id"]] = grade
        else:
            regrades.setdefault(grade["row_id"], []).append(grade)
    return grades, first, regrades


def rubric_lines(item):
    if item["format"] == "explanation":
        return [LABEL_RULE, *[f"- {c['title']} ({c['max_points']} points): " + "; ".join(
            f"{level}: {c['levels'][level]}" for level in reversed(cq.LEVELS)) for c in item["rubric"]]]
    return [SHORT_ANSWER_RULE, "Accepted: " + cq.key_text(item)]


def chat_row(row, item, grade, checks, run_id):
    """One first-pass grade in the chat eval's row shape: the grader is the judge, full credit is a pass."""
    possible = cq.possible_points(item)
    graded = grade.get("status") == "graded" and grade.get("points_earned") is not None
    earned = grade.get("points_earned") if graded else None
    passed = graded and abs(earned - possible) < EPSILON
    reason = (grade.get("rationale") or {}).get("summary") if graded else f"couldn't grade: {grade.get('error')}"
    return {"query_id": row["id"], "item": row["item"], "subject": item.get("subject"), "format": item["format"],
            "persona": row["persona"], "judge_pass": passed,
            "mechanical_checks_passed": graded and checks["passed"],
            "turn_scores": [{"turn": 0, "judge": {"pass": passed, "reason": reason or "", "graded": graded}}],
            "points_earned": earned, "points_possible": possible, "grade_status": grade.get("grade_status"),
            "answer_sha256": digest([row["item"], row["response"]]), "run_id": run_id}


def reason_bar(entry, override=None):
    bar = {**REASON_BAR, **(entry.get("reason_bar") or {}), **{k: v for k, v in (override or {}).items()
                                                                if v is not None}}
    if set(bar) != set(REASON_BAR) or not isinstance(bar["max_fabricated"], int) or bar["max_fabricated"] < 0 \
            or not 0 <= bar["min_accepted_share"] <= 1:
        raise SystemExit(f"a reason bar holds max_fabricated (an integer >= 0) and min_accepted_share (0 to 1): {bar}")
    return bar


def score(name, grades_path, out, registry_path=REGISTRY, bar=None):
    out = Path(out)
    if out.exists():
        raise SystemExit(f"{out} exists; a results directory is never overwritten")
    rows, bank, entry, problems = load_set(name, registry_path)
    if problems:
        raise SystemExit(f"{name} is not valid: " + "; ".join(problems))
    grades, first, regrades = load_grades(grades_path, entry["rows_path"], entry["bank_path"])
    items, sources = {i["id"]: i for i in bank["items"]}, bank.get("sources") or {}
    model_rows = [r for r in rows if r["expected"]["path"] == "model"]
    missing = [r["id"] for r in model_rows if r["id"] not in first]
    if missing:
        raise SystemExit(f"{grades_path} has no first grade for {missing}")
    identity = {"schema_version": SCHEMA_VERSION, "dataset_sha256": digest({"rows": rows, "bank": bank}),
                "expectations_sha256": digest(None), "retrieval_sha256": digest(None),
                "settings": {"harness": "quiz_grading", "set": name, "frozen": bool(entry.get("frozen")),
                             "grader": grades.get("grader"), "reason_bar": reason_bar(entry, bar)},
                "harness_sha256": digest({"scorer": sha256_file(__file__), "items": sha256_file(cq.__file__)})}
    manifest = {**identity, "run_id": digest(identity)}
    scored, checked, chat_items, answers = [], [], [], []
    for row in model_rows:
        item, grade = items[row["item"]], first[row["id"]]
        answer = response_text(row["response"])
        rationale = grade.get("raw") or grade.get("rationale") or {}
        checks = rationale_checks(rationale, answer, sources, item.get("reference_answer"), grade.get("points_earned"),
                                  cq.possible_points(item))
        shown = rationale_checks(grade.get("rationale") or {}, answer, sources, item.get("reference_answer"),
                                 grade.get("points_earned"), cq.possible_points(item))
        checks |= {"checked_on": "raw" if grade.get("raw") else "shown", "shown_unexplained": shown["unexplained"]}
        if shown["unexplained"] and grade.get("status") == "graded":
            checks["problems"].append("the app showed a markdown without a reason")
            checks["passed"] = False
        scored.append(chat_row(row, item, grade, checks, manifest["run_id"]))
        checked.append({"query_id": row["id"], "graded": scored[-1]["turn_scores"][0]["judge"]["graded"], **checks})
        chat_items.append({"id": row["id"], "item": row["item"], "subject": item.get("subject"),
                           "format": item["format"], "persona": row["persona"], "max_points": cq.possible_points(item),
                           "turns": [{"ask": cq.render_item(item, key=True), "rubric": {"lines": rubric_lines(item)}}]})
        answers.append({"query_id": row["id"], "turns": [{"answer": answer}]})
    exact = []
    for row in (r for r in rows if r["expected"]["path"] == "deterministic"):
        grade = first.get(row["id"])
        got = None if grade is None else grade.get("points_earned")
        exact.append({"query_id": row["id"], "expected": row["expected"]["points"], "app": got,
                      "match": got is not None and abs(got - row["expected"]["points"]) < EPSILON})
    out.mkdir(parents=True)
    write_json(out / "items.json", {"schema": ITEMS_SCHEMA, "set": name, "frozen": bool(entry.get("frozen")),
                                    "items": chat_items})
    write_jsonl(out / "answers.jsonl", answers)
    write_json(out / "scored.jsonl.manifest.json", manifest)
    write_jsonl(out / "scored.jsonl", scored)
    write_jsonl(out / "rationale.jsonl", checked)
    write_json(out / "deterministic.json", {"rows": exact, "recorded": sum(e["app"] is not None for e in exact),
                                            "mismatches": [e["query_id"] for e in exact
                                                           if e["app"] is not None and not e["match"]]})
    write_json(out / "grades.json", grades)
    write_json(out / "run.json", {"schema": "quiz-grading-results/1", "set": name, "frozen": entry.get("frozen"),
                                  "rows_sha256": sha256_file(entry["rows_path"]),
                                  "bank_sha256": sha256_file(entry["bank_path"]), "grades": relative(grades_path),
                                  "grades_sha256": sha256_file(grades_path), "grader": grades.get("grader"),
                                  "reason_bar": manifest["settings"]["reason_bar"],
                                  "model_rows": len(model_rows), "deterministic_rows": len(exact),
                                  "regraded_rows": len(regrades)})
    return {"model_rows": len(model_rows), "deterministic_rows": len(exact), "regraded_rows": len(regrades)}


# --- labels: verdict and points blind, then reasons -----------------------------

def label(results, seed=sps.DEFAULT_SEED):
    """Every model-graded row the grader graded, blind and in random order, through `calibration_sample`
    unchanged; each row also asks for human_points."""
    results = Path(results)
    out = results / "calibration-labels.json"
    if out.exists():
        raise SystemExit(f"{out} exists; a labelling file is never overwritten")
    items = read_json(results / "items.json")
    labels = sps.calibration_sample(items, load_jsonl(results / "answers.jsonl"), load_jsonl(results / "scored.jsonl"),
                                    half_width=0.0, seed=seed)
    top = {item["id"]: item["max_points"] for item in items["items"]}
    for row in labels["items"]:
        row |= {"max_points": top[row["query_id"]], "human_points": None}
    labels["instructions"] += (" Also set human_points to the points you would award by the rubric alone, from 0 "
                               "to max_points; a pass is full points.")
    labels["scored_run"] = relative(results / "scored.jsonl")
    labels["scored_sha256"] = sha256_file(results / "scored.jsonl")
    write_json(out, labels)
    return labels


def shown_rationale(rationale):
    """A rationale in the words the result screen shows (section 4.3a), one line per part."""
    lines = [f"Summary: {rationale.get('summary', '')}"]
    for c in rationale.get("criteria") or []:
        lines.append(f"- {c.get('title', c.get('criterion_id', ''))} · {c.get('verdict')} · "
                     f"{c.get('points')}/{c.get('max_points')}")
        lines += [f"  What was missing: {c['what_was_missing']}"] if c.get("what_was_missing") else []
        lines += [f"  Your words: {json.dumps(w, ensure_ascii=False)}" for w in c.get("your_words") or []]
        lines += [f"  Full credit includes: {c['full_credit_includes']}"] if c.get("full_credit_includes") else []
        lines += [f"  Source: {c['source_quote']}"] if c.get("source_quote") else []
    lines += [f"Response to the learner's note: {rationale['response_to_note']}"] if rationale.get(
        "response_to_note") else []
    return lines


def labels_path(results, labels=None):
    return Path(labels) if labels else Path(results) / "calibration-labels.json"


def reasons(results, labels=None):
    """The reason-labelling file for rows the grader marked down that already carry a blind verdict, so the
    verdict is never labelled with the grader's reason in view. Existing reason labels are kept."""
    results = Path(results)
    blind = read_json(labels_path(results, labels))
    if blind is None:
        raise SystemExit("label the verdicts first: run `label`, then fill human_verdict and human_points")
    scored = unique_records(load_jsonl(results / "scored.jsonl"), "query_id")
    first = {g["row_id"]: g for g in read_json(results / "grades.json")["grades"] if g.get("pass", "first") == "first"}
    questions = {i["id"]: i for i in read_json(results / "items.json")["items"]}
    out = results / "reason-labels.json"
    kept = {row["query_id"]: row for row in (read_json(out) or {}).get("items", [])}
    items, waiting = [], 0
    for row in blind["items"]:
        if row.get("human_verdict") is None:
            waiting += 1
            continue
        mine = scored.get(row["query_id"])
        if not mine or not mine["turn_scores"][0]["judge"]["graded"] or mine["judge_pass"]:
            continue
        previous = kept.get(row["query_id"], {})
        items.append({"query_id": row["query_id"],
                      "question": questions[row["query_id"]]["turns"][0]["ask"].splitlines(),
                      "answer": row["turns"][0]["answer"],
                      "grade": f"{mine['points_earned']} of {mine['points_possible']}",
                      "rationale": shown_rationale(first[row["query_id"]].get("rationale") or {}),
                      "human_reason": previous.get("human_reason"), "human_note": previous.get("human_note", "")})
    document = {"schema": REASONS_SCHEMA, "values": list(REASON_LABELS), "waiting_for_blind_labels": waiting,
                "instructions": ("Each row is an answer the grader marked down, with the reason it showed. Set "
                                 "human_reason to \"right\" when the reason is true of the answer and is why it "
                                 "loses points, \"verdict_right_reason_wrong\" when the answer does deserve to lose "
                                 "points but not for this reason, or \"reason_fabricated\" when the reason claims "
                                 "something about the answer or the source that is not there."),
                "items": items}
    write_json(out, document)
    return document


# --- report ------------------------------------------------------------------

def share(part, whole):
    return part / whole if whole else None


def binomial_tail(k, n, p, upper):
    """P(X >= k) when upper, else P(X <= k), for X ~ Binomial(n, p)."""
    terms = range(k, n + 1) if upper else range(0, k + 1)
    return sum(math.comb(n, i) * p ** i * (1 - p) ** (n - i) for i in terms)


def exact_interval(k, n, alpha=ALPHA):
    """The Clopper-Pearson interval for k successes in n, by bisection; None for n = 0."""
    if not n:
        return None

    def solve(target, upper):
        low, high = 0.0, 1.0
        for _ in range(60):
            middle = (low + high) / 2
            tail = binomial_tail(k, n, middle, upper)
            # upper tail rises with p; lower tail falls with p
            if (tail < target) == upper:
                low = middle
            else:
                high = middle
        return (low + high) / 2
    lower = 0.0 if k == 0 else solve(alpha / 2, True)
    upper = 1.0 if k == n else solve(alpha / 2, False)
    return [lower, upper]


def calibration(results, scored, labels_file):
    """calibration_score unchanged; the labels may have been drawn from another run of the same set."""
    labels = read_json(labels_file)
    if labels is None:
        return None
    design_path = Path(labels["scored_run"])
    design_path = design_path if design_path.is_absolute() else ROOT / design_path
    own = design_path.resolve() == (Path(results) / "scored.jsonl").resolve()
    design = scored if own else load_jsonl(design_path)
    if labels.get("scored_sha256") != sha256_file(design_path):
        raise SystemExit(f"{design_path} has changed since {labels_file} was drawn from it")
    report = sps.calibration_score(design, labels, None if own else scored)
    return {key: report[key] for key in ("labelled", "unlabelled", "items", "unanswered", "judge_accuracy",
                                         "false_pass_rate", "false_fail_rate", "judge_pass_rate",
                                         "corrected_pass_rate", "disagreements")}, labels


def point_error(scored, labels):
    rows = unique_records(scored, "query_id")
    errors, by_persona, inconsistent, ungraded = [], {}, [], 0
    for label_row in (labels or {}).get("items", []):
        human, verdict = label_row.get("human_points"), label_row.get("human_verdict")
        top = label_row.get("max_points")
        if human is not None and verdict is not None and (verdict == "pass") != (abs(human - top) < EPSILON):
            inconsistent.append(label_row["query_id"])
        row = rows.get(label_row["query_id"])
        if human is None or row is None:
            continue
        if row["points_earned"] is None:
            ungraded += 1
            continue
        error = abs(row["points_earned"] - human)
        errors.append((error, error / row["points_possible"]))
        by_persona.setdefault(row["persona"], []).append(error)
    return {"labelled": len(errors), "ungraded": ungraded,
            "mean_abs_points": statistics.mean(e for e, _ in errors) if errors else None,
            "mean_abs_share": statistics.mean(s for _, s in errors) if errors else None,
            "max_abs_points": max((e for e, _ in errors), default=None),
            "by_persona": {p: statistics.mean(v) for p, v in sorted(by_persona.items())},
            "labels_inconsistent": inconsistent}


def rationale_quality(checked):
    graded = [c for c in checked if c["graded"]]
    total = {key: sum(c[key] for c in graded) for key in ("spans", "spans_verbatim", "quotes", "quotes_verbatim",
                                                          "criteria", "consistent", "lost", "lost_with_missing")}
    applicable = [c for c in graded if c["copies_reference"] is not None]
    containment = [c["containment"] for c in applicable]
    return {"rows": len(graded), "rows_passing": sum(c["passed"] for c in graded),
            "checked_on_raw": sum(c["checked_on"] == "raw" for c in graded),
            "verbatim_spans": share(total["spans_verbatim"], total["spans"]),
            "verbatim_quotes": share(total["quotes_verbatim"], total["quotes"]),
            "points_agree_with_verdict": share(total["consistent"], total["criteria"]),
            "lost_points_with_missing": share(total["lost_with_missing"], total["lost"]),
            "copies_reference": {"rows": sum(c["copies_reference"] for c in applicable), "of": len(applicable),
                                 "median_containment": statistics.median(containment) if containment else None,
                                 "max_containment": max(containment, default=None)},
            "unexplained_markdowns": sum(c["unexplained"] for c in graded),
            "shown_unexplained": sum(c["shown_unexplained"] for c in graded), "counts": total}


def reason_quality(results, bar=None):
    """The person's reason labels against the run's predeclared bar."""
    bar = bar or REASON_BAR
    document = read_json(Path(results) / "reason-labels.json")
    if document is None:
        return None
    counts = {value: 0 for value in REASON_LABELS}
    for row in document["items"]:
        if row.get("human_reason") is not None:
            if row["human_reason"] not in REASON_LABELS:
                raise SystemExit(f"{row['query_id']}: human_reason must be one of {', '.join(REASON_LABELS)}")
            counts[row["human_reason"]] += 1
    labelled = sum(counts.values())
    accepted = share(counts["right"], labelled)
    fabricated = counts["reason_fabricated"] > bar["max_fabricated"]
    short = accepted is not None and accepted < bar["min_accepted_share"]
    return {"marked_down": len(document["items"]), "labelled": labelled, "counts": counts,
            "accepted_share": accepted, "accepted_exact95": exact_interval(counts["right"], labelled),
            "waiting_for_blind_labels": document.get("waiting_for_blind_labels", 0), "bar": bar,
            "fails": fabricated or short, "fails_because": [why for why, hit in (
                (f"{counts['reason_fabricated']} fabricated (bar: at most {bar['max_fabricated']})", fabricated),
                (f"accepted {accepted:.0%} (bar: at least {bar['min_accepted_share']:.0%})" if accepted is not None
                 else "", short)) if hit]}


def full_credit(grade, possible):
    earned = grade.get("points_earned")
    return grade.get("status") == "graded" and earned is not None and abs(earned - possible) < EPSILON


def contest_stability(results, labels=None):
    """Section 7.2: across SEEDS regrades of an unchanged answer, the verdict changes at most STABILITY_LIMIT of
    the time. Flips toward a person's label are counted too: they are corrections, not noise, but still flips."""
    grades = read_json(Path(results) / "grades.json")["grades"]
    possible = {row["query_id"]: row["points_possible"] for row in load_jsonl(Path(results) / "scored.jsonl")}
    first = {g["row_id"]: g for g in grades if g.get("pass", "first") == "first"}
    human = {row["query_id"]: row.get("human_verdict") for row in (labels or {}).get("items", [])}
    regrades, flips, toward, short, split, answered, rows = 0, 0, 0, [], 0, 0, 0
    by_row = {}
    for grade in (g for g in grades if g.get("pass") == "regrade"):
        by_row.setdefault(grade["row_id"], []).append(grade)
    for row_id, own in sorted(by_row.items()):
        if row_id not in first or row_id not in possible:
            continue
        rows += 1
        before = full_credit(first[row_id], possible[row_id])
        after = [full_credit(g, possible[row_id]) for g in own if g.get("status") == "graded"]
        short += [row_id] if len({g.get("seed") for g in own}) < SEEDS else []
        split += len(set(after)) > 1
        regrades += len(after)
        changed = [a for a in after if a != before]
        flips += len(changed)
        toward += sum(human.get(row_id) == ("pass" if a else "fail") for a in changed)
        answered += sum(bool(str(g.get("response_to_note") or "").strip()) for g in own)
    rate = share(flips, regrades)
    return {"rows": rows, "regrades": regrades, "flips": flips, "flip_rate": rate, "wilson95": wilson(flips, regrades),
            "stable": None if rate is None else rate <= STABILITY_LIMIT, "limit": STABILITY_LIMIT,
            "flips_toward_human": toward, "rows_with_split_seeds": split, "rows_short_of_seeds": short,
            "responses_to_note": share(answered, sum(len(v) for k, v in by_row.items() if k in first))}


def report(results, labels=None):
    results = Path(results)
    scored = load_jsonl(results / "scored.jsonl")
    labelled = calibration(results, scored, labels_path(results, labels))
    calibrated, label_doc = labelled if labelled else (None, None)
    exact = read_json(results / "deterministic.json")
    summary = {
        "run": read_json(results / "run.json"),
        "rows": len(scored), "couldnt_grade": sum(not r["turn_scores"][0]["judge"]["graded"] for r in scored),
        "grader_pass_rate": share(sum(r["judge_pass"] for r in scored), len(scored)),
        "calibration": calibrated, "points": point_error(scored, label_doc),
        "rationale": rationale_quality(load_jsonl(results / "rationale.jsonl")),
        "reasons": reason_quality(results, read_json(results / "scored.jsonl.manifest.json")["settings"]["reason_bar"]),
        "contest": contest_stability(results, label_doc),
        "deterministic": {k: exact[k] for k in ("recorded", "mismatches")} | {"rows": len(exact["rows"])},
    }
    write_json(results / "summary.json", summary)
    (results / "summary.md").write_text(markdown(summary))
    return summary


def pct(value):
    return "—" if value is None else f"{value:.1%}"


def span(result):
    if not result:
        return "—"
    low, high = result["interval"]
    return f"{pct(result['estimate'])} [{low:.1%}, {high:.1%}]"


def markdown(summary):
    run, cal, points, rat, why, contest = (summary["run"], summary["calibration"], summary["points"],
                                           summary["rationale"], summary["reasons"], summary["contest"])
    grader = run.get("grader") or {}
    lines = [f"# Quiz grading: {run['set']}" + ("" if run.get("frozen") else " (draft set: not frozen, not quotable)"),
             "", f"Grader: {grader.get('model', 'unknown')} · rubric {grader.get('rubric_version', '?')} · "
             f"{summary['rows']} model-graded rows, {summary['couldnt_grade']} couldn't grade", ""]
    if cal and cal["labelled"]:
        lines.append(f"1. **Agreement with a person on pass/fail**: {span(cal['judge_accuracy'])}; false-pass "
                     f"{span(cal['false_pass_rate'])}, false-fail {span(cal['false_fail_rate'])}; pass rate "
                     f"{pct(cal['judge_pass_rate'])} by the grader, {span(cal['corrected_pass_rate'])} corrected (PPI). "
                     f"{cal['labelled']} labelled.")
    else:
        lines.append("1. **Agreement with a person**: not labelled yet (run `label`, fill human_verdict).")
    lines.append(f"2. **Point error**: mean {points['mean_abs_points']:.2f} points "
                 f"({pct(points['mean_abs_share'])} of the item) over {points['labelled']} labelled rows."
                 if points["labelled"] else "2. **Point error**: no human_points labelled yet.")
    copies = rat["copies_reference"]
    lines.append(f"3. **Rationale checks** ({rat['checked_on_raw']} of {rat['rows']} on the model's raw output): "
                 f"verbatim spans {pct(rat['verbatim_spans'])}, verbatim quotes {pct(rat['verbatim_quotes'])}, points "
                 f"agree with the verdict {pct(rat['points_agree_with_verdict'])}, lost points with what was missing "
                 f"{pct(rat['lost_points_with_missing'])}, full credit reproduces the reference {copies['rows']} of "
                 f"{copies['of']}, marked down without a reason {rat['unexplained_markdowns']} (shown to the learner: "
                 f"{rat['shown_unexplained']}); {rat['rows_passing']} of {rat['rows']} rows pass every check.")
    if why and why["labelled"]:
        counts, bar, interval = why["counts"], why["bar"], why["accepted_exact95"]
        lines.append(f"4. **Reasons a person accepts**: {counts['right']}/{why['labelled']} ({pct(why['accepted_share'])}, "
                     f"exact 95 % [{interval[0]:.1%}, {interval[1]:.1%}]) of marked-down answers; "
                     f"{counts['verdict_right_reason_wrong']} verdict right but reason wrong, "
                     f"{counts['reason_fabricated']} fabricated. Bar: at most {bar['max_fabricated']} fabricated, at "
                     f"least {bar['min_accepted_share']:.0%} accepted: "
                     + ("**fails** (" + "; ".join(why["fails_because"]) + ")." if why["fails"] else "meets it."))
    else:
        lines.append("4. **Reasons a person accepts**: not labelled yet (run `reasons` after the blind labels).")
    if contest["regrades"]:
        lines.append(f"5. **Contest stability**: {contest['flips']}/{contest['regrades']} regrades changed the verdict "
                     f"({pct(contest['flip_rate'])}, limit {contest['limit']:.0%}): "
                     f"{'stable' if contest['stable'] else 'unstable'}; {contest['flips_toward_human']} flips toward "
                     f"the person's label, {contest['rows_with_split_seeds']} rows whose seeds disagree, replies to the "
                     f"note {pct(contest['responses_to_note'])}."
                     + (f" Short of {SEEDS} seeds: {', '.join(contest['rows_short_of_seeds'])}."
                        if contest["rows_short_of_seeds"] else ""))
    else:
        lines.append("5. **Contest stability**: no regrades recorded.")
    exact = summary["deterministic"]
    lines.append(f"6. **Deterministic rows**: {exact['recorded']} of {exact['rows']} graded by the app, "
                 f"{len(exact['mismatches'])} differ from section 3's arithmetic"
                 + (f" ({', '.join(exact['mismatches'])})" if exact["mismatches"] else "") + ".")
    if points["labels_inconsistent"]:
        lines.append(f"\nLabels whose verdict and points disagree: {', '.join(points['labels_inconsistent'])}.")
    if cal and cal["disagreements"]:
        lines.append("\nDisagreements: " + ", ".join(f"{d['query_id']} (grader {d['judge']}, person {d['human']})"
                                                    for d in cal["disagreements"]))
    return "\n".join(lines) + "\n"


# --- command line ----------------------------------------------------------

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--registry", type=Path, default=REGISTRY)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("validate")
    check.add_argument("--set")
    run = sub.add_parser("score")
    run.add_argument("name")
    run.add_argument("grades", type=Path)
    run.add_argument("--out", type=Path, required=True)
    run.add_argument("--max-fabricated", type=int, help=f"override the set's reason bar (default "
                     f"{REASON_BAR['max_fabricated']}); recorded in the manifest")
    run.add_argument("--min-accepted-share", type=float, help=f"override the set's reason bar (default "
                     f"{REASON_BAR['min_accepted_share']}); recorded in the manifest")
    draw = sub.add_parser("label")
    draw.add_argument("results", type=Path)
    draw.add_argument("--seed", type=int, default=sps.DEFAULT_SEED)
    why = sub.add_parser("reasons")
    why.add_argument("results", type=Path)
    why.add_argument("--labels", type=Path, help="a labelling file drawn from another run of the same set")
    summary = sub.add_parser("report")
    summary.add_argument("results", type=Path)
    summary.add_argument("--labels", type=Path, help="a labelling file drawn from another run of the same set")
    lock = sub.add_parser("freeze")
    lock.add_argument("name")
    lock.add_argument("--baseline", type=Path, required=True)
    args = parser.parse_args(argv)

    if args.command == "validate":
        problems = validate(args.registry, args.set)
        for name, found in problems.items():
            print(f"{name}: " + ("valid" if not found else f"{len(found)} problems\n  " + "\n  ".join(found)))
        if any(problems.values()):
            raise SystemExit(1)
    elif args.command == "score":
        bar = {"max_fabricated": args.max_fabricated, "min_accepted_share": args.min_accepted_share}
        print(json.dumps(score(args.name, args.grades, args.out, args.registry, bar)))
    elif args.command == "label":
        labels = label(args.results, args.seed)
        print(f"{len(labels['items'])} rows to label, about {labels['estimated_minutes']} minutes")
    elif args.command == "reasons":
        document = reasons(args.results, args.labels)
        print(f"{len(document['items'])} marked-down rows; {document['waiting_for_blind_labels']} rows still need "
              f"a blind verdict first")
    elif args.command == "report":
        report(args.results, args.labels)
        print((args.results / "summary.md").read_text())
    else:
        print(json.dumps(freeze(args.name, args.baseline, args.registry)))


if __name__ == "__main__":
    main()
