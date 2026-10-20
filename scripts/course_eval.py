#!/usr/bin/env python3
"""Score course and lesson generation runs of the course eval.

The Rust harness (`live_course_eval`, run by `scripts/course-eval.sh run`) writes one raw directory per request.
This script turns it into a committed result in the shapes `shipped_path_stats.py compare`, `calibration-sample`
and `calibration-score` read. Design: docs/design/2026-10-20-course-eval.md.

* `collect` copies the small files, sums each stage's tokens from its call files, renders every artifact as one
  deterministic markdown text, and writes one item per artifact and criterion (and per claim) with its
  "answers". It never overwrites a results directory.
* `score` grades each item once with the chat scorer's judge request. A grade takes as long as the server
  takes; a prompt over the judge's window is refused, never truncated. It resumes past scored items.
* `summarize` writes the headline, the criterion and request tables and the `compare` commands.

Commands:
  collect RUN_DIR --out RESULTS [--requests SET]
  score RESULTS --endpoint URL --model NAME [--window N] [--claims]
  summarize RESULTS
"""
import argparse
import hashlib
import json
import math
import re
import statistics
import string
from datetime import datetime, timezone
from pathlib import Path

import shipped_path_stats as sps
from chat_rag_eval import load_jsonl
from eval_integrity import SCHEMA_VERSION, digest, unique_records, wilson
from shipped_path_eval import http_judge, judge_identity, judge_request

ROOT = Path(__file__).resolve().parent.parent
RUBRIC = "lattice-course-judge/1"
SET_SCHEMA = "course-eval-requests/1"
ITEMS_SCHEMA = "course-eval-items/1"
RESULTS_SCHEMA = "course-eval-results/1"
CHARS_PER_TOKEN = 4.0  # the app's own default, `DEFAULT_CHARS_PER_TOKEN` in lattice-core's llm_port
TOO_LONG = "too long for the judge window"
NO_ARTIFACT = "no artifact to grade"
LESSONS_DIR = "crates/lattice-desktop/src/features/learning/lessons/"
HARNESS_FILES = (*(LESSONS_DIR + name for name in ("live_course_eval.rs", "live_web_trace.rs", "live_small_lesson.rs")),
                 "scripts/course-eval.sh")
COPIED = ("request.json", "outline.json", "web-trace-report.json", "phases.jsonl")
CALL_FILE = re.compile(r"call-(\d+)\.json")
LESSON_FILE = re.compile(r"lesson-(.+)\.json")

# --- rubric texts, Appendix A (`lattice-course-judge/1`), verbatim ----------

COURSE_JUDGE_SYSTEM = (
    "You grade one artifact of a learning app, an outline or a lesson, against one criterion written by the "
    "person who built the test. The learner's request is given: their goal, what they already know, and the "
    "minutes they have per session. Judge only the criterion you are given, from the artifact and the request. "
    "Do not reward length. Do not penalise a lesson for what a later lesson in the outline will cover. Return "
    "JSON with verdict \"pass\" or \"fail\" and a one-sentence reason that names the part of the artifact that "
    "decided it."
)
CLAIM_JUDGE_SYSTEM = (
    "You judge one factual statement taken from a lesson. Use your own knowledge of the subject. Return pass "
    "when the statement is true as written, allowing ordinary simplification for a learner. Return fail when it "
    "is false, misleadingly overstated, or when you do not know; begin the reason with FALSE: or UNSURE: "
    "accordingly."
)
# What a person labelling a claim reads: the variant's rule, one sentence per line.
CLAIM_LINES = re.split(r"(?<=\.) ", CLAIM_JUDGE_SYSTEM)[1:]
O1_INTRO = ("Each of the following topics must be the stated objective or outcome of at least one module or "
            "lesson, not merely mentioned in passing:")
O1_CLOSE = ("Pass only if every topic is covered. If the list is empty, pass only if the modules and lessons "
            "together plainly achieve the learner's goal as written.")
O2_INTRO = ("The learner already knows the following; no lesson may teach any of it from scratch (a short "
            "recap inside a lesson is fine, a lesson devoted to it is not):")
O2_CLOSE = "No lesson may assume knowledge the learner says they lack. Pass only if both hold."
OUTLINE_CRITERIA = {
    "O1": ("Goal coverage", None),
    "O2": ("Fits prior knowledge", None),
    "O3": ("Progression", "Lessons are ordered so that each one uses only what earlier lessons or the learner's "
           "prior knowledge provide. Fail if any lesson relies on a concept first taught in a later lesson, or if "
           "modules are an unordered list of topics with no build-up."),
    "O4": ("Session fit", "Each lesson's objective is achievable, by this learner, in one session of the stated "
           "minutes, given its estimated minutes. Fail if any lesson's objective bundles several outcomes that "
           "could not fit, or if a lesson's estimate exceeds the session length."),
    "O5": ("Specificity", "Every lesson objective names a concrete thing the learner will be able to do or "
           "explain, stated so that a quiz could test it. Fail if any objective is a topic label (\"Indexes\") or "
           "a vague aim (\"understand performance\")."),
}
LESSON_CRITERIA = {
    "L1": ("Teaches rather than lists", "The explanation blocks explain how and why: mechanisms, causes, "
           "consequences, with at least one concrete example per main idea. Fail if the lesson is mostly bullet "
           "lists of facts, definitions without reasons, or restated headings."),
    "L2": ("Objective met", "After reading the lesson, the learner could do what the objective states. Fail if a "
           "part of the objective is never taught or only named."),
    "L3": ("Worked example", "The worked-example block walks through a complete, specific case step by step, and "
           "each step is correct for the lesson's subject as far as you can tell. Fail if the example is missing, "
           "generic, skips the step the objective is about, or contains an error you can identify."),
    "L4": ("Practice", "The practice blocks can be completed using only what this lesson and the learner's prior "
           "knowledge provide, and completing them exercises the objective. Fail if practice needs tools or facts "
           "the lesson never gave, or practises something else."),
    "L5": ("Questions", "Each question tests the objective; exactly one option is defensibly correct given the "
           "lesson; the other options are plausible to a learner who missed the point, not obviously wrong or "
           "jokes; the marked correct answer is the correct one. Fail if any question breaks any of these."),
    "L6": ("Level", "The lesson is pitched to the learner's stated prior knowledge: it does not re-teach what they "
           "know, and it does not assume vocabulary or skills they said they lack without introducing them. Fail "
           "if either happens in more than a passing sentence."),
    "L7": ("Coherence", "Sections follow from one another, the recap matches what was taught, and no section "
           "contradicts another. Fail on a contradiction, a recap that lists things not taught, or a section that "
           "belongs to a different lesson."),
}
TITLES = {key: value[0] for key, value in {**OUTLINE_CRITERIA, **LESSON_CRITERIA}.items()}


def outline_lines(criterion, set_item):
    """Rubric lines for one outline criterion; O1 and O2 carry the request's own lists."""
    if criterion == "O1":
        return [O1_INTRO, *[f"- {topic}" for topic in set_item.get("outline_must_cover") or []], O1_CLOSE]
    if criterion == "O2":
        known = [f"- {topic}" for topic in set_item.get("prior_knowledge_excludes") or []]
        return [O2_INTRO, *(known or ["- (none listed)"]), O2_CLOSE]
    return [OUTLINE_CRITERIA[criterion][1]]


# --- reading a raw run -----------------------------------------------------

def read_json(path, default=None):
    return json.loads(Path(path).read_text()) if Path(path).exists() else default


def load_optional(path):
    return load_jsonl(path) if Path(path).exists() else []


def field(record, *names, default=None):
    """The first present field: the harness writes snake_case, the app's DTOs camelCase."""
    for name in names:
        if isinstance(record, dict) and record.get(name) is not None:
            return record[name]
    return default


def dig(record, *path):
    for key in path:
        record = record.get(key) if isinstance(record, dict) else None
    return record


def count(value):
    """A metric that is either a count or the list it counts."""
    return len(value) if isinstance(value, (list, dict)) else int(value or 0)


def zero(value):
    """A check passes only on a recorded zero: a missing metric is not a clean one."""
    return value is not None and count(value) == 0


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def relative(path):
    path = Path(path).resolve()
    return str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)


def call_files(directory):
    """{N: {"input", "output", "sha256"}} for every `call-N.json` in a request directory."""
    calls = {}
    for path in Path(directory).iterdir():
        if match := CALL_FILE.fullmatch(path.name):
            raw = path.read_bytes()
            record = json.loads(raw)
            calls[int(match[1])] = {"input": int(record.get("inputTokens") or 0),
                                    "output": int(record.get("outputTokens") or 0),
                                    "sha256": hashlib.sha256(raw).hexdigest()}
    return calls


def stages(metrics):
    """(name, stage metrics) in run order: the outline, then lessons as the harness ran them."""
    out = [("outline", metrics["outline"])] if isinstance(metrics.get("outline"), dict) else []
    return out + [(sel, stage) for sel, stage in (metrics.get("lessons") or {}).items() if isinstance(stage, dict)]


def call_ranges(metrics, numbers):
    """Call-file numbers per stage. `cost.first_call`/`cost.last_call` (inclusive) name a stage's range when
    the harness records them; otherwise a stage takes the next `cost.calls` files in number order, since
    stages run one at a time."""
    ordered, ranges, position = sorted(numbers), {}, 0
    for name, stage in stages(metrics):
        cost = stage.get("cost") or {}
        if cost.get("first_call") is not None and cost.get("last_call") is not None:
            taken = [n for n in ordered if cost["first_call"] <= n <= cost["last_call"]]
            position = ordered.index(taken[-1]) + 1 if taken else position
        else:
            taken = ordered[position:position + count(cost.get("calls"))]
            position += len(taken)
        ranges[name] = taken
    return ranges


def fill_costs(metrics, calls):
    """Write each stage's token sums from its call files into its `cost` block."""
    for name, numbers in call_ranges(metrics, calls).items():
        stage = metrics["outline"] if name == "outline" else metrics["lessons"][name]
        inputs = [calls[n]["input"] for n in numbers]
        stage.setdefault("cost", {}).update(
            input_tokens=sum(inputs), output_tokens=sum(calls[n]["output"] for n in numbers),
            peak_input_tokens=max(inputs, default=0), call_files=len(numbers))
    return metrics


def lesson_report(document):
    report = (document or {}).get("report")
    return json.loads(report) if isinstance(report, str) else (report or {})


# --- rendering (section 6.3) -----------------------------------------------

def request_lines(request):
    return [f"Goal: {field(request, 'goal', default='')}",
            f"Prior knowledge: {field(request, 'priorKnowledge', 'prior_knowledge', default='')}",
            f"Minutes per session: {field(request, 'minutesPerSession', 'minutes_per_session', default='')}",
            f"Depth: {field(request, 'courseDepth', 'course_depth', default='unspecified')}"]


def render_project(project):
    if not isinstance(project, dict):
        return [f"Project milestone: {project}", ""] if isinstance(project, str) else []
    lines = [f"Project milestone: {project.get('title', '')}", *([project["brief"]] if project.get("brief") else [])]
    for label, names in (("Deliverables", ("deliverables",)), ("Success criteria", ("successCriteria",
                                                                                     "success_criteria"))):
        entries = field(project, *names, default=[])
        lines += [f"{label}:", *[f"- {entry}" for entry in entries]] if entries else []
    return lines + [""]


def render_outline(outline, request):
    """The outline as the judge and the labeller read it."""
    lines = [f"# {field(outline, 'title', default=field(request, 'goal', default='Outline'))}", "",
             *request_lines(request), ""]
    for n, module in enumerate(outline.get("modules") or [], 1):
        lines += [f"## Module {n}: {module.get('title', '')}", ""]
        lines += [module["summary"], ""] if module.get("summary") else []
        lines += ["Outcomes:", *[f"- {o}" for o in module["outcomes"]], ""] if module.get("outcomes") else []
        lessons = module.get("lessons") or []
        for m, lesson in enumerate(lessons, 1):
            minutes = field(lesson, "estimated_minutes", "estimatedMinutes", default="?")
            lines.append(f"- Lesson {n}.{m}: {lesson.get('title', '')} ({minutes} min) — "
                         f"{lesson.get('objective', '')}")
        lines += [""] if lessons else []
        lines += render_project(module.get("project"))
    lines += render_project(outline.get("project"))
    return "\n".join(lines).rstrip() + "\n"


def answer_key(question, document):
    """(correct index, explanation): on the question when the harness kept the key there, else in the lesson
    file's `answer_key` by question id (the learner DTO carries no key)."""
    keyed = (document.get("answer_key") or {}).get(question.get("id")) or {}
    index = field(question, "correctIndex", "correct_index", default=field(keyed, "correctIndex", "correct_index"))
    return index, field(question, "explanation", default=field(keyed, "explanation"))


def render_lesson(document, request):
    """A prepared lesson as the judge and the labeller read it, blocks and answer key included."""
    lesson, asked = document["lesson"], request_lines(request)
    lines = [f"# {lesson.get('title', '')}", "", f"Objective: {lesson.get('objective', '')}", asked[1], asked[2], ""]
    for block in lesson.get("blocks") or []:
        lines += [f"## [{block.get('kind', '')}] {block.get('title', '')}", "", block.get("body", ""), ""]
        if block.get("rubric"):
            lines += ["Rubric:", *[f"- {c.get('title', '')}: {c.get('description', '')} "
                                   f"({field(c, 'maxPoints', 'max_points', default='?')} points)"
                                   for c in block["rubric"]], ""]
    questions = lesson.get("questions") or []
    lines += ["## Questions", ""] if questions else []
    for n, question in enumerate(questions, 1):
        options = question.get("options") or []
        lines += [f"{n}. {question.get('prompt', '')}",
                  *[f"   {letter}. {option}" for letter, option in zip(string.ascii_uppercase, options)]]
        index, explanation = answer_key(question, document)
        correct = string.ascii_uppercase[index] if isinstance(index, int) and 0 <= index < len(options) else None
        lines.append(f"   Correct: {correct or '(not recorded)'}")
        lines += [f"   Explanation: {explanation}"] if explanation else []
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


# --- collect ---------------------------------------------------------------

def load_set(path):
    data = json.loads(Path(path).read_text())
    if data.get("schema") != SET_SCHEMA:
        raise SystemExit(f"{path} is not {SET_SCHEMA}")
    items = unique_records(data["items"], "id")
    if any("/" in item_id for item_id in items):
        raise SystemExit(f"request ids must not contain '/': {sorted(i for i in items if '/' in i)}")
    return data, items


def selected_lessons(source, run_meta, metrics):
    """The lesson slots of a request: what the run selected, then any other lesson it recorded."""
    chosen = run_meta.get("lessons")
    chosen = list((run_meta.get("recipe") or {}).get("lessons", ["m1.l1"]) if chosen is None else chosen)
    seen = set(metrics.get("lessons") or {}) | set((read_json(source / "done.json") or {}).get("lessons") or {})
    seen |= {m[1] for m in (LESSON_FILE.fullmatch(p.name) for p in source.iterdir()) if m}
    return chosen + sorted(seen - set(chosen))


def outline_slot(rid, set_item, metrics, outline, request):
    stage = metrics.get("outline") or {}
    status = str(dig(stage, "review", "status") or (outline or {}).get("status")).lower()
    return {"slot": f"{rid}/outline", "request": rid, "shape": set_item.get("shape"), "stage": "outline",
            "lesson": None, "outcome": status, "error": None,
            "mechanical_checks_passed": status == "passed" and zero(dig(stage, "quotes", "unmatched")),
            "resumed": bool(stage.get("resumed")), "answer": render_outline(outline, request) if outline else None}


def lesson_slot(rid, set_item, sel, metrics, document, request):
    stage = (metrics.get("lessons") or {}).get(sel) or {}
    outcome = str(field(stage, "outcome", default=field(document, "outcome", default="not run"))).lower()
    prepared = outcome == "ready" and isinstance((document or {}).get("lesson"), dict)
    return {"slot": f"{rid}/{sel}", "request": rid, "shape": set_item.get("shape"), "stage": "lesson",
            "lesson": sel, "outcome": outcome, "error": field(stage, "error", default=field(document, "error")),
            "mechanical_checks_passed": outcome == "ready" and zero(dig(stage, "citations", "unresolved"))
            and zero(dig(stage, "claims", "quotes_unmatched")) and zero(dig(stage, "claims", "unverified")),
            "resumed": False, "answer": render_lesson(document, request) if prepared else None}


def item(slot, query_id, criterion, title, ask, lines, **extra):
    return {"id": query_id, "request": slot["request"], "shape": slot["shape"], "stage": extra.pop("stage",
            slot["stage"]), "lesson": slot["lesson"], "slot": slot["slot"], "criterion": criterion, "title": title,
            "mechanical_checks_passed": extra.pop("mechanical", slot["mechanical_checks_passed"]), **extra,
            "turns": [{"ask": ask, "rubric": {"lines": lines}}]}


def slot_items(slot, set_item, request):
    """One item per rubric criterion of a slot, all sharing the slot's rendered artifact."""
    criteria = OUTLINE_CRITERIA if slot["stage"] == "outline" else LESSON_CRITERIA
    items = [item(slot, f"{slot['slot']}/{criterion}", criterion, title, "\n".join(request_lines(request)),
                  outline_lines(criterion, set_item) if slot["stage"] == "outline" else [text])
             for criterion, (title, text) in criteria.items()]
    return items, [{"query_id": i["id"], "turns": [{"answer": slot["answer"]}]} for i in items]


def claim_items(slot, document, request):
    """One item per finding of a prepared lesson's stored verification report."""
    items, answers, per_unit = [], [], {}
    for finding in lesson_report(document).get("findings") or []:
        unit = finding.get("unit")
        per_unit[unit] = per_unit.get(unit, 0) + 1
        verdict = str(finding.get("verdict", "")).lower()
        items.append(item(slot, f"{slot['slot']}/claim/{unit}-{per_unit[unit]}", "C1", "True statement",
                          f"Subject: {field(request, 'goal', default='')}", list(CLAIM_LINES), stage="claim",
                          mechanical=verdict == "supported", pipeline_verdict=verdict,
                          general_knowledge=verdict == "unsupported" and finding.get("resolution") is not None))
        answers.append({"query_id": items[-1]["id"],
                        "turns": [{"answer": field(finding, "statement", "quote", default="")}]})
    return items, answers


def gather_request(source, set_item):
    """Everything collect writes for one request, read before anything is written."""
    rid, run_meta = source.name, read_json(source / "run.json")
    if run_meta.get("request_id", rid) != rid:
        raise SystemExit(f"{source}: run.json names request {run_meta['request_id']!r}")
    request = read_json(source / "request.json", {})
    metrics = read_json(source / "metrics.json", {}) or {}
    calls = call_files(source)
    fill_costs(metrics, calls)
    outline = read_json(source / "outline.json")
    slots, lessons = [outline_slot(rid, set_item, metrics, outline, request)], {}
    for sel in selected_lessons(source, run_meta, metrics):
        lessons[sel] = read_json(source / f"lesson-{sel}.json")
        slots.append(lesson_slot(rid, set_item, sel, metrics, lessons[sel], request))
    gathered = {"id": rid, "slots": slots, "items": [], "answers": [], "claims": [], "claim_answers": []}
    for slot in slots:
        more, said = slot_items(slot, set_item, request)
        gathered["items"] += more
        gathered["answers"] += said
        if slot["stage"] == "lesson" and slot["answer"]:
            more, said = claim_items(slot, lessons[slot["lesson"]], request)
            gathered["claims"] += more
            gathered["claim_answers"] += said
    copies = {name: (source / name).read_bytes() for name in COPIED if (source / name).exists()}
    copies |= {f"lesson-{sel}.json": (source / f"lesson-{sel}.json").read_bytes() for sel in lessons if lessons[sel]}
    copies["metrics.json"] = (json.dumps(metrics, indent=2, ensure_ascii=False) + "\n").encode()
    index = hashlib.sha256("\n".join(sorted(c["sha256"] for c in calls.values())).encode()).hexdigest()
    gathered["copies"] = copies
    gathered["summary"] = {"run": run_meta, "done": read_json(source / "done.json"),
                           "exit": read_json(source / "exit.json"), "calls": len(calls), "calls_index_sha256": index}
    return gathered


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def write_jsonl(path, rows, mode="x"):
    with Path(path).open(mode) as out:
        for row in rows:
            out.write(json.dumps(row, ensure_ascii=False) + "\n")


def collect(run_dir, out, set_path=None):
    run_dir, out = Path(run_dir), Path(out)
    if out.exists():
        raise SystemExit(f"{out} exists; a results directory is never overwritten")
    sources = {p.name: p for p in sorted(run_dir.iterdir()) if p.is_dir() and (p / "run.json").exists()}
    if not sources:
        raise SystemExit(f"{run_dir} holds no request directory with a run.json")
    if set_path is None:
        named = (read_json(next(iter(sources.values())) / "run.json").get("recipe") or {}).get("requests")
        if not named:
            raise SystemExit("the run's recipe names no request set; pass --requests")
        set_path = Path(named).expanduser() if Path(named).expanduser().is_absolute() else ROOT / named
    request_set, set_items = load_set(set_path)
    unknown = sorted(set(sources) - set(set_items))
    if unknown:
        raise SystemExit(f"requests not in {set_path}: {unknown}")
    gathered = [gather_request(sources[rid], entry) for rid, entry in set_items.items() if rid in sources]
    absent = [rid for rid in set_items if rid not in sources]

    out.mkdir(parents=True)
    for request in gathered:
        (out / "requests" / request["id"]).mkdir(parents=True)
        for name, data in request["copies"].items():
            (out / "requests" / request["id"] / name).write_bytes(data)
    head = {"schema": ITEMS_SCHEMA, "rubric": RUBRIC, "set": request_set.get("name")}
    write_json(out / "items.json", {**head, "items": [i for r in gathered for i in r["items"]]})
    write_json(out / "claims.items.json", {**head, "items": [i for r in gathered for i in r["claims"]]})
    write_jsonl(out / "answers.jsonl", [row for r in gathered for row in r["answers"]])
    write_jsonl(out / "claims.jsonl", [row for r in gathered for row in r["claim_answers"]])
    manifest = {
        "schema": RESULTS_SCHEMA, "set": request_set.get("name"), "set_path": relative(set_path),
        "set_sha256": sha256_file(set_path), "scorer_sha256": sha256_file(__file__),
        "harness_sha256": {name: sha256_file(ROOT / name) if (ROOT / name).exists() else None
                           for name in HARNESS_FILES},
        "artifacts_dir": relative(run_dir),
        "collected_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "calls_index_sha256": {r["id"]: r["summary"]["calls_index_sha256"] for r in gathered},
        "requests": {r["id"]: r["summary"] for r in gathered},
        "missing_requests": [rid for rid in absent if not set_items[rid].get("optional")],
        "skipped_optional": [rid for rid in absent if set_items[rid].get("optional")],
        "slots": [{**{k: v for k, v in slot.items() if k != "answer"}, "artifact": slot["answer"] is not None}
                  for r in gathered for slot in r["slots"]],
    }
    write_json(out / "run.json", manifest)
    return manifest


# --- score -----------------------------------------------------------------

def judge_messages(entry, answer):
    """The fixed judge prompt for one item: request, criterion lines, artifact between markers."""
    turn = entry["turns"][0]
    if entry["criterion"] == "C1":
        return [{"role": "system", "content": CLAIM_JUDGE_SYSTEM},
                {"role": "user", "content": f"{turn['ask']}\nStatement: <<<\n{answer}\n>>>"}]
    user = ["Request:", turn["ask"], "", "Criterion:", *turn["rubric"]["lines"], "", "Artifact:", "<<<", answer, ">>>"]
    return [{"role": "system", "content": COURSE_JUDGE_SYSTEM}, {"role": "user", "content": "\n".join(user)}]


def prompt_tokens(messages):
    """A character-based estimate, rounded up, at the app's default pace."""
    return math.ceil(sum(len(m["content"]) for m in messages) / CHARS_PER_TOKEN)


def grade(entry, answer, judge, window):
    if not answer:
        return {"pass": False, "reason": NO_ARTIFACT, "graded": False}
    messages = judge_messages(entry, answer)
    if window and prompt_tokens(messages) + judge_request("", messages)["max_tokens"] > window:
        return {"pass": False, "reason": TOO_LONG, "graded": False}
    passed, reason = judge(messages)
    return {"pass": passed, "reason": reason, "graded": True}


def course_manifest(items_doc, run, judge_settings):
    """Identity in the chat scorer's manifest shape, so review tooling can read it."""
    runs = {rid: r.get("run") or {} for rid, r in (run.get("requests") or {}).items()}

    def one(path):
        values = sorted({json.dumps(dig(meta, *path)) for meta in runs.values()} - {"null"})
        return json.loads(values[0]) if len(values) == 1 else [json.loads(v) for v in values] or None
    identity = {
        "schema_version": SCHEMA_VERSION, "dataset_sha256": digest(items_doc),
        "expectations_sha256": digest(None), "retrieval_sha256": digest(None),
        "settings": {
            "harness": "course_eval", "model": one(("recipe", "server", "model")),
            "model_revision": one(("server", "gguf_sha256")) or one(("server", "model_revision")),
            "recipe_sha256": one(("recipe_sha256",)), "set_sha256": run.get("set_sha256"),
            "thinking": one(("server", "thinking")), "commit": one(("commit",)),
            "web_trace_index_sha256": {rid: dig(meta, "web", "trace_index_sha256") for rid, meta in runs.items()},
            "judge": judge_settings,
        },
        "harness_sha256": digest({"runner": run.get("harness_sha256"), "scorer": run.get("scorer_sha256"),
                                  "binary": one(("binary_sha256",))}),
    }
    return {**identity, "run_id": digest(identity)}


def stable(manifest):
    """What must agree for a resumed score to append to an earlier one."""
    judge = {k: v for k, v in (manifest["settings"]["judge"] or {}).items() if k != "identity"}
    return manifest["dataset_sha256"], {**manifest["settings"], "judge": judge}


def score_file(results, items_name, answers_name, out_name, judge, judge_settings, window):
    """Grade one question set into `out_name`, appending to what an earlier run of this score graded."""
    items_doc = read_json(results / items_name)
    answers = unique_records(load_jsonl(results / answers_name), "query_id")
    manifest = course_manifest(items_doc, read_json(results / "run.json"), judge_settings)
    manifest_path, out_path = results / f"{out_name}.manifest.json", results / out_name
    earlier = read_json(manifest_path)
    if earlier is not None:
        if stable(earlier) != stable(manifest):
            raise SystemExit(f"{out_path} was scored with other items or another judge; use a fresh results dir")
        manifest = earlier
    elif out_path.exists():
        raise SystemExit(f"{out_path} has no manifest; it was not written by this scorer")
    else:
        write_json(manifest_path, manifest)
    done = {row["query_id"] for row in load_optional(out_path)}
    counts = {"graded": 0, "skipped": len(done), "no_artifact": 0, "too_long": 0}
    with out_path.open("a") as out:
        for entry in items_doc["items"]:
            if entry["id"] in done:
                continue
            answer = answers[entry["id"]]["turns"][0].get("answer")
            verdict = grade(entry, answer, judge, window)
            counts["graded" if verdict["graded"] else "too_long" if verdict["reason"] == TOO_LONG
                   else "no_artifact"] += 1
            row = {"query_id": entry["id"], **{k: entry[k] for k in ("request", "shape", "stage", "lesson", "slot",
                                                                      "criterion")},
                   "judge_pass": verdict["pass"], "mechanical_checks_passed": entry["mechanical_checks_passed"],
                   "turn_scores": [{"turn": 0, "judge": verdict}], "answer_sha256": digest(answer),
                   "run_id": manifest["run_id"]}
            row |= {k: entry[k] for k in ("pipeline_verdict", "general_knowledge") if k in entry}
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
            out.flush()
    return counts


def slot_rows(items_doc, scored):
    """One row per slot: it passes only when every one of its criteria passes."""
    rows, by_slot, out, incomplete = unique_records(scored, "query_id"), {}, [], []
    for entry in items_doc["items"]:
        by_slot.setdefault(entry["slot"], []).append(entry)
    for slot, members in by_slot.items():
        graded = [rows.get(entry["id"]) for entry in members]
        if any(row is None for row in graded):
            incomplete.append(slot)
            continue
        verdicts = [row["turn_scores"][0]["judge"] for row in graded]
        failed = [f"{m['criterion']}: {v['reason']}" for m, v in zip(members, verdicts) if not v["pass"]]
        first = members[0]
        out.append({"query_id": slot, **{k: first[k] for k in ("request", "shape", "stage", "lesson")},
                    "criterion": "all", "judge_pass": not failed,
                    "mechanical_checks_passed": first["mechanical_checks_passed"],
                    "turn_scores": [{"turn": 0, "judge": {"pass": not failed,
                                                          "reason": "; ".join(failed) or "every criterion passed",
                                                          "graded": all(v["graded"] for v in verdicts)}}],
                    "answer_sha256": graded[0]["answer_sha256"], "run_id": graded[0]["run_id"]})
    return out, incomplete


def score(results, judge, judge_settings, window=None, claims=False):
    results = Path(results)
    if not (results / "items.json").exists():
        raise SystemExit(f"{results} has no items.json; run collect first")
    report = {"items": score_file(results, "items.json", "answers.jsonl", "scored.jsonl", judge, judge_settings,
                                  window)}
    if claims:
        report["claims"] = score_file(results, "claims.items.json", "claims.jsonl", "claims.scored.jsonl", judge,
                                      judge_settings, window)
    rows, incomplete = slot_rows(read_json(results / "items.json"), load_jsonl(results / "scored.jsonl"))
    partial = results / "slots.scored.jsonl.partial"
    write_jsonl(partial, rows, mode="w")
    partial.replace(results / "slots.scored.jsonl")
    report["slots"] = {"written": len(rows), "incomplete": incomplete}
    return report


def served_window(identity, model):
    """`n_ctx` from the server's model listing when it lists one; nothing is probed for it."""
    served = identity.get("served_models") if isinstance(identity.get("served_models"), list) else []
    entries = [e for e in served if e.get("id") == model] or (served if len(served) == 1 else [])
    windows = [(e.get("meta") or {}).get("n_ctx") for e in entries]
    return next((w for w in windows if isinstance(w, int) and w > 0), None)


# --- summarize -------------------------------------------------------------

def spread(values, scale=1):
    values = [v / scale for v in values if v is not None]
    return {"median": statistics.median(values) if values else None, "max": max(values, default=None)}


def calibrated(results, labels_name, rows):
    """The judge's corrected pass rate once a labelled calibration slice exists."""
    labels = read_json(results / labels_name)
    if labels is None or not rows:
        return None
    try:
        report = sps.calibration_score(rows, labels)
    except (ValueError, KeyError) as error:
        return {"error": str(error)}
    return {key: report[key] for key in ("labelled", "corrected_pass_rate", "false_pass_rate", "false_fail_rate")}


def previous_results(results):
    """The most recent earlier scored results directory in the same model folder."""
    earlier = [p for p in results.parent.iterdir()
               if p.is_dir() and p.name < results.name and (p / "slots.scored.jsonl").exists()]
    return max(earlier, key=lambda p: p.name, default=None)


def summarize(results):
    results = Path(results)
    run = read_json(results / "run.json")
    slots, scored = run["slots"], load_optional(results / "scored.jsonl")
    judged = {row["query_id"]: row for row in load_optional(results / "slots.scored.jsonl")}
    claims = load_optional(results / "claims.scored.jsonl")
    metrics = {rid: read_json(results / "requests" / rid / "metrics.json", {}) for rid in run["requests"]}
    web = {rid: read_json(results / "requests" / rid / "web-trace-report.json") or {} for rid in run["requests"]}

    def stage_metrics(slot):
        own = metrics[slot["request"]]
        if slot["stage"] == "outline":
            return own.get("outline") or {}
        return (own.get("lessons") or {}).get(slot["lesson"]) or {}

    def passed(slot):
        row = judged.get(slot["slot"])
        return bool(row and row["judge_pass"] and row["mechanical_checks_passed"])

    def tally(group):
        return {"passed": sum(passed(s) for s in group) if judged else None,
                "mechanical": sum(s["mechanical_checks_passed"] for s in group), "total": len(group)}

    outlines = [s for s in slots if s["stage"] == "outline"]
    lessons = [s for s in slots if s["stage"] == "lesson"]
    ready = [s for s in lessons if s["outcome"] == "ready"]
    ground = {key: sum(count(dig(stage_metrics(s), "claims", key)) for s in ready)
              for key in ("total", "supported", "unsupported", "contradicted", "unverified", "general_knowledge")}
    total, fails = ground["total"], [r["turn_scores"][0]["judge"]["reason"] for r in claims if not r["judge_pass"]]
    ground |= {"supported_share": ground["supported"] / total if total else None,
               "supported_wilson95": wilson(ground["supported"], total),
               "general_knowledge_share": ground["general_knowledge"] / total if total else None,
               "claim_judge": {"graded": sum(r["turn_scores"][0]["judge"]["graded"] for r in claims),
                               "total": len(claims), "false": sum(f.startswith("FALSE:") for f in fails),
                               "unsure": sum(f.startswith("UNSURE:") for f in fails),
                               "calibrated": calibrated(results, "claims.calibration-labels.json", claims)}}
    criteria = {}
    for row in scored:
        entry = criteria.setdefault(row["criterion"], {"passed": 0, "graded": 0, "total": 0, "too_long": 0})
        verdict = row["turn_scores"][0]["judge"]
        entry["passed"] += bool(row["judge_pass"])
        entry["graded"] += bool(verdict["graded"])
        entry["total"] += 1
        entry["too_long"] += verdict["reason"] == TOO_LONG
    requests = []
    for rid in run["requests"]:
        own = [s for s in slots if s["request"] == rid]
        outline, own_lessons = own[0], [s for s in own if s["stage"] == "lesson"]
        requests.append({"id": rid, "shape": outline["shape"], "outline": outline["outcome"],
                         "resumed": outline["resumed"], "lessons": len(own_lessons),
                         "lessons_ready": sum(s["outcome"] == "ready" for s in own_lessons), "slots": tally(own),
                         "web": {k: web[rid].get(k) for k in ("mode", "hits", "misses_filled", "misses_failed",
                                                               "untraced_calls")}})
    costs = [stage_metrics(s).get("cost") or {} for s in lessons]
    previous = previous_results(results)
    summary = {
        "set": run.get("set"), "rubric": RUBRIC, "judged": bool(judged),
        "slots": {"outline": tally(outlines), "lesson": tally(lessons),
                  "by_shape": {shape: tally([s for s in slots if str(s["shape"]) == shape])
                               for shape in sorted({str(s["shape"]) for s in slots})}},
        "lessons_ready": {"ready": len(ready), "total": len(lessons),
                          "failures": [{k: s.get(k) for k in ("slot", "outcome", "error")}
                                       for s in lessons if s["outcome"] != "ready"]},
        "grounding": ground,
        "cost": {"outline_minutes": spread([dig(stage_metrics(s), "cost", "seconds") for s in outlines], 60),
                 "lesson_minutes": spread([c.get("seconds") for c in costs], 60),
                 "lesson_calls": spread([c.get("calls") for c in costs]),
                 "lesson_input_tokens": spread([c.get("input_tokens") for c in costs])},
        "criteria": dict(sorted(criteria.items())),
        "judge_calibrated": calibrated(results, "calibration-labels.json", scored),
        "too_long": sum(c["too_long"] for c in criteria.values()),
        "requests": requests, "missing_requests": run.get("missing_requests", []),
        "skipped_optional": run.get("skipped_optional", []),
        "previous": relative(previous) if previous else None, "metrics": metrics,
    }
    write_json(results / "summary.json", summary)
    (results / "summary.md").write_text(markdown(summary, relative(results)))
    return summary


def fraction(tally, key="passed"):
    return f"{'not judged' if tally[key] is None else tally[key]}/{tally['total']}"


def number(value):
    return "—" if value is None else f"{value:.1f}" if isinstance(value, float) else str(value)


def markdown(summary, here):
    slots, ground, cost = summary["slots"], summary["grounding"], summary["cost"]
    lines = [f"# Course eval: {summary['set']}", "",
             f"1. **Slots passed** (mechanical checks and every judged criterion): outlines "
             f"{fraction(slots['outline'])}, lessons {fraction(slots['lesson'])} (mechanical only: "
             f"{fraction(slots['outline'], 'mechanical')}, {fraction(slots['lesson'], 'mechanical')}). By shape: "
             + ", ".join(f"{shape} {fraction(t)}" for shape, t in slots["by_shape"].items()) + ".",
             f"2. **Lessons Ready**: {summary['lessons_ready']['ready']}/{summary['lessons_ready']['total']}."]
    lines += [f"   - {f['slot']}: {f['outcome']}" + (f" — {f['error']}" if f.get("error") else "")
              for f in summary["lessons_ready"]["failures"]]
    share, rate = ground["supported_share"], (ground["claim_judge"]["calibrated"] or {}).get("corrected_pass_rate")
    correctness = (f"(judge, corrected): {rate['estimate']:.3f} [{rate['interval'][0]:.3f}, {rate['interval'][1]:.3f}]"
                   if rate and rate.get("estimate") is not None else
                   f"is quoted once claims.calibration-labels.json is labelled "
                   f"({ground['claim_judge']['graded']} claims judged)")
    lines.append(f"3. **Grounding**: {ground['supported']}/{ground['total']} claims supported"
                 + (f" ({share:.1%})" if share is not None else "") + ", general-knowledge share "
                 + ("—" if ground["general_knowledge_share"] is None else f"{ground['general_knowledge_share']:.1%}")
                 + f", {ground['contradicted']} contradicted, {ground['unverified']} unverified. "
                 f"Claim correctness {correctness}.")
    lines.append(f"4. **Cost**: outline minutes median {number(cost['outline_minutes']['median'])}, max "
                 f"{number(cost['outline_minutes']['max'])}; lesson minutes median "
                 f"{number(cost['lesson_minutes']['median'])}, max {number(cost['lesson_minutes']['max'])}; per lesson "
                 f"{number(cost['lesson_calls']['median'])} calls and {number(cost['lesson_input_tokens']['median'])} "
                 f"input tokens (median), max {number(cost['lesson_calls']['max'])} and "
                 f"{number(cost['lesson_input_tokens']['max'])}.")
    if summary["too_long"]:
        lines.append(f"\n{summary['too_long']} items were {TOO_LONG}; they count as ungraded fails.")
    if summary["missing_requests"]:
        lines.append(f"\nNot in this run: {', '.join(summary['missing_requests'])}.")
    lines += ["", "## Criteria", "", "| criterion | passed | graded | items | too long |", "|---|---:|---:|---:|---:|"]
    lines += [f"| {c} {TITLES.get(c, 'True statement')} | {e['passed']} | {e['graded']} | {e['total']} | "
              f"{e['too_long']} |" for c, e in summary["criteria"].items()]
    lines += ["", "## Requests", "", "| request | shape | outline | lessons Ready | slots passed | web |",
              "|---|---|---|---:|---:|---|"]
    for request in summary["requests"]:
        trace = request["web"]
        web_line = (f"web: {trace['hits']} hits, {trace['misses_filled']} misses filled, {trace['misses_failed']} "
                    f"failed ({trace['mode']})" if trace.get("hits") is not None else "no trace report")
        lines.append(f"| {request['id']} | {request['shape']} | {request['outline']}"
                     f"{' (resumed)' if request['resumed'] else ''} | {request['lessons_ready']}/{request['lessons']} "
                     f"| {fraction(request['slots'])} | {web_line} |")
    if summary["previous"]:
        lines += ["", "## Compare with the previous run", "", "```sh"]
        lines += [f"python3 scripts/shipped_path_stats.py compare --baseline {summary['previous']}/{name} "
                  f"--candidate {here}/{name} --outcome both" for name in ("slots.scored.jsonl", "scored.jsonl")]
        lines += ["```", "", "The slot comparison is the verdict; the criterion comparison is screening only."]
    return "\n".join(lines) + "\n"


# --- command line ----------------------------------------------------------

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    gather = sub.add_parser("collect")
    gather.add_argument("run_dir", type=Path, help="artifacts/course-eval/<date>-<label>")
    gather.add_argument("--out", type=Path, required=True, help="the results directory; must not exist")
    gather.add_argument("--requests", type=Path, help="the request set (default: the one the run's recipe names)")
    judged = sub.add_parser("score")
    judged.add_argument("results", type=Path)
    judged.add_argument("--endpoint", required=True, help="the judge's OpenAI-compatible base URL")
    judged.add_argument("--model", required=True, help="the model name the judge request carries")
    judged.add_argument("--window", type=int, help="the judge's context window in tokens "
                        "(default: n_ctx from the server's model listing, when it lists one)")
    judged.add_argument("--claims", action="store_true", help="also judge every claim (C1)")
    report = sub.add_parser("summarize")
    report.add_argument("results", type=Path)
    args = parser.parse_args(argv)

    if args.command == "collect":
        manifest = collect(args.run_dir, args.out, args.requests)
        print(f"{args.out}: {len(manifest['requests'])} requests, {len(manifest['slots'])} slots; "
              f"not in the run: {', '.join(manifest['missing_requests']) or 'none'}")
    elif args.command == "score":
        run = read_json(args.results / "run.json") or {}
        models = {dig(r, "run", "recipe", "server", "model") for r in (run.get("requests") or {}).values()}
        identity = judge_identity(args.endpoint, args.model, next(iter(models)) if len(models) == 1 else None)
        window = args.window or served_window(identity, args.model)
        if window is None:
            print("no judge window given or listed: every prompt is sent whole, none is refused for size")
        settings = judge_request(args.model, []) | {
            "messages": None, "rubric": RUBRIC, "identity": identity, "window": window,
            "chars_per_token": CHARS_PER_TOKEN, "system_sha256": digest([COURSE_JUDGE_SYSTEM, CLAIM_JUDGE_SYSTEM])}
        result = score(args.results, http_judge(args.endpoint, args.model), settings, window, args.claims)
        print(json.dumps(result, indent=2))
    else:
        summarize(args.results)
        print((args.results / "summary.md").read_text())


if __name__ == "__main__":
    main()
