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

Rubric `lattice-course-judge/2` (design Appendix C) adds the `quiz` stage of docs/design/2026-10-23-quizzes-and-tests.md
section 7.1: each Ready lesson's quiz bank (the harness's `quiz-<sel>.json`, or the lesson's own six questions when no
bank was recorded) gets mechanical item checks in `metrics.json` and judged criteria Q1-Q5 per item, one call each;
its slot `<request>/<lesson>/quiz` passes when the mechanical checks pass and at least 90 % of items pass every judged
criterion. L5 is retired into Q2-Q4. The quiz judge must not be the model that wrote the bank (`--allow-self-judge`
records an exception in the manifest).

Commands:
  collect RUN_DIR --out RESULTS [--requests SET]
  score RESULTS --endpoint URL --model NAME [--window N] [--claims] [--allow-self-judge]
  summarize RESULTS
"""
import argparse
import hashlib
import json
import math
import re
import statistics
import urllib.request
from datetime import datetime, timezone
from fractions import Fraction
from pathlib import Path

import course_eval_quiz as cq
import shipped_path_stats as sps
from chat_rag_eval import load_jsonl
from eval_integrity import SCHEMA_VERSION, digest, unique_records, wilson
from shipped_path_eval import http_judge, judge_headers, judge_identity, judge_request

ROOT = Path(__file__).resolve().parent.parent
RUBRIC = "lattice-course-judge/2"
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
QUIZ_FILE = re.compile(r"quiz-(.+)\.json")
QUIZ_ITEM_PASS_SHARE = Fraction(9, 10)  # design 2026-10-23 section 7.1: a bank, unlike a lesson, tolerates a few drops

# --- rubric texts, Appendix A (`lattice-course-judge/1`) and C (`/2`), verbatim ----------

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
    "L6": ("Level", "The lesson is pitched to the learner's stated prior knowledge: it does not re-teach what they "
           "know, and it does not assume vocabulary or skills they said they lack without introducing them. Fail "
           "if either happens in more than a passing sentence."),
    "L7": ("Coherence", "Sections follow from one another, the recap matches what was taught, and no section "
           "contradicts another. Fail on a contradiction, a recap that lists things not taught, or a section that "
           "belongs to a different lesson."),
}
# `lattice-course-judge/2`: L5 (questions) is retired; the quiz stage's Q2-Q4 judge every question instead.
RETIRED = {"L5": "Questions"}
QUIZ_JUDGE_SYSTEM = (
    "You grade one quiz item of a learning app against one criterion written by the person who built the test. "
    "The lesson the item belongs to is given. Judge only the criterion you are given, from the lesson and the "
    "item. Return JSON with verdict \"pass\" or \"fail\" and a one-sentence reason that names the part of the "
    "item that decided it."
)
QUIZ_SOLVE_SYSTEM = (
    "You are a careful learner who has read only the lesson given. Answer the quiz item from the lesson alone. "
    "Return JSON with answer in the format asked for, other_defensible true when a different answer would also be "
    "defensible given the lesson, and a one-sentence reason."
)
Q2_LINES = ("Solve the item from the lesson alone, without the key. Pass when your answer is the key and no "
            "other answer is defensible given the lesson.")
Q5_TEXT = {
    "typed": ("The accepted variants cover the reasonable ways a correct learner would phrase this; the numeric "
              "tolerance is neither trivial nor forgiving of a wrong answer."),
    "explanation": ("Each rubric criterion names an observable thing in the answer at incomplete, adequate and "
                    "strong levels, without stating the answer."),
}
QUIZ_CRITERIA = {
    "Q1": ("Answerable from the lesson", "A learner who read only this lesson can answer the item; it needs no "
           "outside fact the lesson does not teach."),
    "Q2": ("Exactly one correct key", Q2_LINES),
    "Q3": ("Plausible distractors", "Each wrong option is one a learner who missed the point could pick; none is "
           "a joke, a category error, or trivially excluded by grammar or length."),
    "Q4": ("No leak", "The stem, the other options, and the lesson's own phrasing of the item do not give the "
           "answer away."),
    "Q5": ("Keyed fairly", None),
}
QUIZ_KEY_SHOWN = ("Q3", "Q4", "Q5")  # Q1 and Q2 read the item as a learner does; the others need to know the key
TITLES = {key: value[0] for key, value in {**OUTLINE_CRITERIA, **LESSON_CRITERIA, **RETIRED, **QUIZ_CRITERIA}.items()}


def quiz_criteria(item):
    """The criteria that apply to an item's format: Q2 needs a key to solve for, Q3 distractors, Q5 typed
    variants or a rubric."""
    fmt = item.get("format")
    distractors = fmt in ("single_choice", "multi_select") or (
        fmt == "matching" and len(item.get("right") or []) > len(item.get("left") or []))
    return (["Q1"] + (["Q2"] if fmt != "explanation" else []) + (["Q3"] if distractors else []) + ["Q4"]
            + (["Q5"] if fmt in ("short_answer", "cloze", "explanation") else []))


def quiz_lines(criterion, item):
    if criterion == "Q5":
        return [Q5_TEXT["explanation" if item.get("format") == "explanation" else "typed"]]
    return [QUIZ_CRITERIA[criterion][1]]


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
    """A prepared lesson as the judge and the labeller read it. Its questions are not part of it: under
    `lattice-course-judge/2` they are quiz items with their own criteria, and a quiz judge must not see their keys
    in the lesson text."""
    lesson, asked = document["lesson"], request_lines(request)
    lines = [f"# {lesson.get('title', '')}", "", f"Objective: {lesson.get('objective', '')}", asked[1], asked[2], ""]
    for block in lesson.get("blocks") or []:
        lines += [f"## [{block.get('kind', '')}] {block.get('title', '')}", "", block.get("body", ""), ""]
        if block.get("rubric"):
            lines += ["Rubric:", *[f"- {c.get('title', '')}: {c.get('description', '')} "
                                   f"({field(c, 'maxPoints', 'max_points', default='?')} points)"
                                   for c in block["rubric"]], ""]
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


def load_bank(source, sel, document):
    """(origin, bank) for a prepared lesson: the bank the harness recorded with its keys (`quiz-<sel>.json`), else
    the lesson's own reviewed questions as single-choice items, else (None, None)."""
    path = source / f"quiz-{sel}.json"
    if path.exists():
        bank = read_json(path)
        if bank.get("schema") != cq.BANK_SCHEMA:
            raise SystemExit(f"{path} is not {cq.BANK_SCHEMA}")
        return "bank", bank
    items = []
    for question in ((document or {}).get("lesson") or {}).get("questions") or []:
        index, explanation = answer_key(question, document)
        items.append({"id": question.get("id"), "format": "single_choice", "prompt": question.get("prompt"),
                      "options": question.get("options") or [], "key": [index] if isinstance(index, int) else [],
                      "explanation": explanation, "origin": "prose_step",
                      "source_ids": field(question, "sourceIds", "source_ids", default=[])})
    return ("lesson_questions", {"schema": cq.BANK_SCHEMA, "selector": sel, "items": items}) if items else (None, None)


def quiz_stage(source, rid, set_item, lesson_slot_row, document, request, metrics):
    """The quiz slot of one lesson, its items (one per bank item, one turn per criterion that applies), their
    answers (the item with its key, as a labeller reads it), and the lesson text the quiz judge reads."""
    sel = lesson_slot_row["lesson"]
    prepared = lesson_slot_row["answer"] is not None
    origin, bank = load_bank(source, sel, document) if prepared else (None, None)
    items = list((bank or {}).get("items") or [])
    checks = cq.bank_checks(items, cq.lesson_plain(document), bank.get("sources"), bank.get("metrics")) if bank else None
    if prepared:
        metrics.setdefault("lessons", {}).setdefault(sel, {})["quiz"] = (
            {"origin": origin, **checks} if checks else {"origin": None, "items": 0, "passed": False})
    slot_id = f"{rid}/{sel}/quiz"
    slot = {"slot": slot_id, "request": rid, "shape": set_item.get("shape"), "stage": "quiz", "lesson": sel,
            "outcome": origin or ("no bank" if prepared else "no lesson"), "error": None,
            "mechanical_checks_passed": bool(checks and checks["passed"]), "resumed": False,
            "answer": lesson_slot_row["answer"] if items else None}
    ask = "\n".join([*request_lines(request)[:2], f"Lesson: requests/{rid}/lesson-{sel}.md (read it once)"])
    entries, answers, used = [], [], set()
    for item in items:
        name = re.sub(r"[/\s]+", "_", str(item.get("id") or "item"))
        name = name if name not in used else f"{name}-{len(used)}"
        used.add(name)
        problems = checks["problems"].get(item.get("id"), [])
        invalid = [p for p in problems if p.startswith("format_invalid")]
        criteria = ["format"] if invalid else quiz_criteria(item)
        turns = [{"criterion": c, "ask": ask,
                  "rubric": {"lines": ["The item is valid for its format."] if c == "format" else quiz_lines(c, item)}}
                 for c in criteria]
        entries.append({"id": f"{slot_id}/{name}", "request": rid, "shape": set_item.get("shape"), "stage": "quiz",
                        "lesson": sel, "slot": slot_id, "criterion": "quiz-item", "title": item.get("format"),
                        "format": item.get("format"), "origin": item.get("origin", origin),
                        "mechanical_checks_passed": not problems, "mechanical_problems": problems,
                        "item": item, "lesson_ref": f"{rid}/{sel}", "turns": turns})
        shown = json.dumps(item, indent=2, ensure_ascii=False) if invalid else cq.render_item(item, key=True)
        answers.append({"query_id": entries[-1]["id"], "turns": [{"answer": shown} for _ in turns]})
    return slot, entries, answers


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
    gathered = {"id": rid, "slots": slots, "items": [], "answers": [], "claims": [], "claim_answers": [],
                "quiz": [], "quiz_answers": [], "lessons": {}}
    copies = {}
    for slot in list(slots):
        more, said = slot_items(slot, set_item, request)
        gathered["items"] += more
        gathered["answers"] += said
        if slot["stage"] == "lesson":
            if slot["answer"]:
                more, said = claim_items(slot, lessons[slot["lesson"]], request)
                gathered["claims"] += more
                gathered["claim_answers"] += said
                gathered["lessons"][f"{rid}/{slot['lesson']}"] = slot["answer"]
                copies[f"lesson-{slot['lesson']}.md"] = slot["answer"].encode()
            quiz, more, said = quiz_stage(source, rid, set_item, slot, lessons[slot["lesson"]], request, metrics)
            slots.append(quiz)
            gathered["quiz"] += more
            gathered["quiz_answers"] += said
            if (source / f"quiz-{slot['lesson']}.json").exists():
                copies[f"quiz-{slot['lesson']}.json"] = (source / f"quiz-{slot['lesson']}.json").read_bytes()
    copies |= {name: (source / name).read_bytes() for name in COPIED if (source / name).exists()}
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
    write_json(out / "quiz.items.json", {**head, "stage": "quiz",
                                         "lessons": {k: v for r in gathered for k, v in r["lessons"].items()},
                                         "items": [i for r in gathered for i in r["quiz"]]})
    write_jsonl(out / "quiz.jsonl", [row for r in gathered for row in r["quiz_answers"]])
    manifest = {
        "schema": RESULTS_SCHEMA, "set": request_set.get("name"), "set_path": relative(set_path),
        "set_sha256": sha256_file(set_path), "scorer_sha256": sha256_file(__file__),
        "quiz_scorer_sha256": sha256_file(cq.__file__),
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


SOLVE_SCHEMA = {
    "type": "object",
    "properties": {"answer": {"type": "string"}, "other_defensible": {"type": "boolean"}, "reason": {"type": "string"}},
    "required": ["answer", "other_defensible", "reason"],
}


def solve_request(model, messages):
    """The judge request (greedy, thinking off) bound to the blind-solve schema."""
    return judge_request(model, messages) | {
        "response_format": {"type": "json_schema", "json_schema": {"name": "solve", "schema": SOLVE_SCHEMA}}}


def parse_solve(payload):
    content = payload["choices"][0]["message"]["content"]
    solved = json.loads(re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL).strip())
    if not isinstance(solved.get("answer"), str) or not isinstance(solved.get("other_defensible"), bool):
        raise ValueError(f"solver returned no answer: {content!r}")
    return solved


def http_solver(endpoint, model):
    """Solves one item blind (Q2). Like the judge, it takes as long as the server takes."""
    url, headers = endpoint.rstrip("/") + "/v1/chat/completions", judge_headers()

    def call(messages):
        body = json.dumps(solve_request(model, messages)).encode()
        with urllib.request.urlopen(urllib.request.Request(url, data=body, method="POST", headers=headers)) as reply:
            return parse_solve(json.load(reply))
    return call


def quiz_messages(lesson, item, criterion, lines):
    """One quiz criterion's prompt: the lesson and the item between markers; Q2 asks for a blind solve."""
    if criterion == "Q2":
        user = ["Lesson:", "<<<", lesson, ">>>", "", "Item:", "<<<", cq.render_item(item), ">>>", "",
                f"Answer with {cq.answer_format(item)}."]
        return [{"role": "system", "content": QUIZ_SOLVE_SYSTEM}, {"role": "user", "content": "\n".join(user)}]
    user = ["Criterion:", *lines, "", "Lesson:", "<<<", lesson, ">>>", "", "Item:", "<<<",
            cq.render_item(item, key=criterion in QUIZ_KEY_SHOWN), ">>>"]
    return [{"role": "system", "content": QUIZ_JUDGE_SYSTEM}, {"role": "user", "content": "\n".join(user)}]


def solve_verdict(item, solved):
    """Q2 is decided by code: the blind answer must earn the key's full credit and the solver must call no other
    answer defensible."""
    response = cq.parse_answer(item, solved.get("answer"))
    agrees = cq.agrees_with_key(item, response)
    other = bool(solved.get("other_defensible"))
    reason = (f"solved {solved.get('answer')!r}, key {cq.key_text(item)}: "
              f"{'agrees' if agrees else 'disagrees'}{'; another answer is defensible' if other else ''}. "
              f"{solved.get('reason', '')}").strip()
    return {"pass": agrees and not other, "reason": reason, "graded": True, "solved": solved}


def grade_quiz_item(entry, lesson, judge, solver, window):
    """One verdict per criterion turn; an invalid item fails ungraded without a call."""
    item, turns = entry["item"], []
    for number, turn in enumerate(entry["turns"]):
        criterion = turn["criterion"]
        if criterion == "format":
            verdict = {"pass": False, "reason": "invalid item: " + "; ".join(entry["mechanical_problems"]),
                       "graded": False}
        else:
            messages = quiz_messages(lesson, item, criterion, turn["rubric"]["lines"])
            if window and prompt_tokens(messages) + judge_request("", messages)["max_tokens"] > window:
                verdict = {"pass": False, "reason": TOO_LONG, "graded": False}
            elif criterion == "Q2":
                verdict = (solve_verdict(item, solver(messages)) if solver else
                           {"pass": False, "reason": "no solver was given", "graded": False})
            else:
                passed, reason = judge(messages)
                verdict = {"pass": passed, "reason": reason, "graded": True}
        turns.append({"turn": number, "criterion": criterion, "judge": verdict})
    return turns


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


def scored_output(results, items_doc, out_name, judge_settings):
    """(manifest, output path, query ids already scored): resume only with the same items and judge."""
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
    return manifest, out_path, {row["query_id"] for row in load_optional(out_path)}


def score_file(results, items_name, answers_name, out_name, judge, judge_settings, window):
    """Grade one question set into `out_name`, appending to what an earlier run of this score graded."""
    items_doc = read_json(results / items_name)
    answers = unique_records(load_jsonl(results / answers_name), "query_id")
    manifest, out_path, done = scored_output(results, items_doc, out_name, judge_settings)
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


def score_quiz(results, judge, solver, judge_settings, window):
    """Grade every quiz item into `quiz.scored.jsonl`: one row per item, one turn score per criterion; the item
    passes when every criterion does."""
    items_doc = read_json(results / "quiz.items.json")
    manifest, out_path, done = scored_output(results, items_doc, "quiz.scored.jsonl", judge_settings)
    counts = {"graded": 0, "skipped": len(done), "ungraded": 0}
    with out_path.open("a") as out:
        for entry in items_doc["items"]:
            if entry["id"] in done:
                continue
            turns = grade_quiz_item(entry, items_doc["lessons"].get(entry["lesson_ref"], ""), judge, solver, window)
            counts["graded" if all(t["judge"]["graded"] for t in turns) else "ungraded"] += 1
            row = {"query_id": entry["id"], **{k: entry[k] for k in ("request", "shape", "stage", "lesson", "slot",
                                                                      "criterion", "format", "origin")},
                   "judge_pass": bool(turns) and all(t["judge"]["pass"] for t in turns),
                   "mechanical_checks_passed": entry["mechanical_checks_passed"], "turn_scores": turns,
                   "answer_sha256": digest(entry["item"]), "run_id": manifest["run_id"]}
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
            out.flush()
    return counts


def quiz_slot_rows(run_slots, items_doc, scored):
    """One row per quiz slot: it passes when at least QUIZ_ITEM_PASS_SHARE of its items pass every criterion; a
    lesson with no bank has no items and fails ungraded."""
    rows, members = unique_records(scored, "query_id"), {}
    for entry in items_doc["items"]:
        members.setdefault(entry["slot"], []).append(entry)
    out, incomplete = [], []
    for slot in (s for s in run_slots if s["stage"] == "quiz"):
        own = members.get(slot["slot"], [])
        graded = [rows.get(entry["id"]) for entry in own]
        if any(row is None for row in graded):
            incomplete.append(slot["slot"])
            continue
        passing = sum(bool(row["judge_pass"]) for row in graded)
        passed = bool(own) and passing >= QUIZ_ITEM_PASS_SHARE * len(own)
        reason = (f"{passing} of {len(own)} items pass every judged criterion" if own
                  else f"no quiz items ({slot['outcome']})")
        complete = bool(own) and all(t["judge"]["graded"] for row in graded for t in row["turn_scores"])
        out.append({"query_id": slot["slot"], **{k: slot[k] for k in ("request", "shape", "stage", "lesson")},
                    "criterion": "all", "judge_pass": passed,
                    "mechanical_checks_passed": slot["mechanical_checks_passed"],
                    "turn_scores": [{"turn": 0, "judge": {"pass": passed, "reason": reason, "graded": complete}}],
                    "answer_sha256": digest(sorted(row["answer_sha256"] for row in graded)),
                    "run_id": graded[0]["run_id"] if graded else None})
    return out, incomplete


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


def score(results, judge, judge_settings, window=None, claims=False, solver=None, allow_self_judge=False):
    results = Path(results)
    if not (results / "items.json").exists():
        raise SystemExit(f"{results} has no items.json; run collect first")
    report = {"items": score_file(results, "items.json", "answers.jsonl", "scored.jsonl", judge, judge_settings,
                                  window)}
    if claims:
        report["claims"] = score_file(results, "claims.items.json", "claims.jsonl", "claims.scored.jsonl", judge,
                                      judge_settings, window)
    rows, incomplete = slot_rows(read_json(results / "items.json"), load_jsonl(results / "scored.jsonl"))
    if (results / "quiz.items.json").exists():
        if judge_settings.get("independent") is False and not allow_self_judge:
            report["quiz"] = {"refused": "the judge is the model that wrote the bank; name another judge, or pass "
                                         "--allow-self-judge to record the exception"}
        else:
            report["quiz"] = score_quiz(results, judge, solver, judge_settings, window)
            more, waiting = quiz_slot_rows(read_json(results / "run.json")["slots"],
                                           read_json(results / "quiz.items.json"),
                                           load_jsonl(results / "quiz.scored.jsonl"))
            rows, incomplete = rows + more, incomplete + waiting
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


def quiz_summary(slots, scored, quiz_metrics):
    """The quiz stage: criterion pass counts across items, and per bank the mechanical checks and the harness's
    own numbers (target, validation, key review, cost) when it recorded them."""
    criteria = {}
    for row in scored:
        for turn in row["turn_scores"]:
            entry = criteria.setdefault(turn.get("criterion", "?"), {"passed": 0, "graded": 0, "total": 0,
                                                                     "too_long": 0})
            entry["passed"] += bool(turn["judge"]["pass"])
            entry["graded"] += bool(turn["judge"]["graded"])
            entry["total"] += 1
            entry["too_long"] += turn["judge"].get("reason") == TOO_LONG
    banks, totals = [], {name: 0 for name in (*cq.GATING_CHECKS, "duplicate_items", "near_duplicate_items",
                                               "letter_references")}
    longest, anchors = {"items": 0, "of": 0, "chance_sum": 0.0}, {"items": 0, "of": 0}
    for slot in slots:
        own = quiz_metrics(slot)
        if not own.get("items"):
            banks.append({"slot": slot["slot"], "origin": own.get("origin"), "items": 0, "passed": False})
            continue
        hits = {name: len(own["hits"].get(name, [])) for name in cq.GATING_CHECKS}
        hits |= {name: len(own.get(name) or []) for name in ("duplicate_items", "near_duplicate_items",
                                                             "letter_references")}
        for name, value in hits.items():
            totals[name] += value
        key = own.get("key_longest") or {}
        longest["items"] += key.get("items", 0)
        longest["of"] += key.get("of", 0)
        longest["chance_sum"] += (key.get("chance") or 0) * key.get("of", 0)
        anchors["items"] += own["anchored"]["items"]
        anchors["of"] += own["anchored"]["of"]
        banks.append({"slot": slot["slot"], "origin": own.get("origin"), "items": own["items"],
                      "passed": own["passed"], "hits": hits, "format_mix": own.get("format_mix"),
                      "key_longest": key, "anchored": own["anchored"], "harness": own.get("harness") or {}})
    return {
        "items": {"passing": sum(bool(r["judge_pass"]) for r in scored), "total": len(scored),
                  "ungraded": sum(not all(t["judge"]["graded"] for t in r["turn_scores"]) for r in scored)},
        "criteria": dict(sorted(criteria.items())), "mechanical": totals,
        "key_longest": {"items": longest["items"], "of": longest["of"],
                        "share": longest["items"] / longest["of"] if longest["of"] else None,
                        "chance": longest["chance_sum"] / longest["of"] if longest["of"] else None},
        "anchored": anchors, "banks": banks,
    }


def summarize(results):
    results = Path(results)
    run = read_json(results / "run.json")
    slots, scored = run["slots"], load_optional(results / "scored.jsonl")
    judged = {row["query_id"]: row for row in load_optional(results / "slots.scored.jsonl")}
    claims = load_optional(results / "claims.scored.jsonl")
    quiz_scored = load_optional(results / "quiz.scored.jsonl")
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
    quizzes = [s for s in slots if s["stage"] == "quiz"]
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
    previous_rubric = (read_json(previous / "summary.json") or {}).get("rubric") if previous else None
    summary = {
        "set": run.get("set"), "rubric": RUBRIC, "judged": bool(judged),
        "slots": {"outline": tally(outlines), "lesson": tally(lessons), "quiz": tally(quizzes),
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
        "previous": relative(previous) if previous else None, "previous_rubric": previous_rubric,
        "quiz": quiz_summary(quizzes, quiz_scored, lambda slot: stage_metrics(slot).get("quiz") or {})
        | {"judged": bool(quiz_scored), "calibrated": calibrated(results, "quiz.calibration-labels.json", quiz_scored)},
        "metrics": metrics,
    }
    write_json(results / "summary.json", summary)
    (results / "summary.md").write_text(markdown(summary, relative(results)))
    return summary


def fraction(tally, key="passed"):
    return f"{'not judged' if tally[key] is None else tally[key]}/{tally['total']}"


def number(value):
    return "—" if value is None else f"{value:.1f}" if isinstance(value, float) else str(value)


def share(part, whole):
    return f"{part}/{whole}" + (f" ({part / whole:.0%})" if whole else "")


def quiz_headline(quiz):
    if not quiz["banks"]:
        return []
    hits, key = quiz["mechanical"], quiz["key_longest"]
    items = (f"{quiz['items']['passing']}/{quiz['items']['total']} items pass every judged criterion"
             if quiz["judged"] else "items not judged")
    longest = (f"the key is the longest option in {key['share']:.0%} of choice items (chance {key['chance']:.0%})"
               if key["share"] is not None else "no choice items")
    return [f"5. **Quizzes**: {items}; mechanical hits: leak {hits['leak']}, length bias {hits['length_bias']}, "
            f"duplicate options {hits['duplicate_options']}, key/explanation conflicts "
            f"{hits['key_explanation_conflict']}, invalid format {hits['format_invalid']}, unstated absolutes "
            f"{hits['absolute_unstated']}, duplicate items {hits['duplicate_items']}, near-duplicates "
            f"{hits['near_duplicate_items']}; {longest}; anchored {share(quiz['anchored']['items'], quiz['anchored']['of'])}."]


def quiz_tables(quiz):
    if not quiz["banks"]:
        return []
    lines = ["", "## Quiz criteria", "", "| criterion | passed | graded | items | too long |", "|---|---:|---:|---:|---:|"]
    lines += [f"| {c} {TITLES.get(c, c)} | {e['passed']} | {e['graded']} | {e['total']} | {e['too_long']} |"
              for c, e in quiz["criteria"].items()]
    lines += ["", "## Quiz banks", "",
              "| slot | origin | items | target | mechanical | gating hits | key longest | validated | key review "
              "(agreed first, after repair, dropped) | calls | minutes |",
              "|---|---|---:|---:|---|---:|---:|---:|---|---:|---:|"]
    for bank in quiz["banks"]:
        if not bank["items"]:
            lines.append(f"| {bank['slot']} | {bank['origin'] or 'none'} | 0 | — | fail | — | — | — | — | — | — |")
            continue
        harness, key = bank["harness"], bank["key_longest"]
        validated, review = harness.get("validated") or {}, harness.get("key_review") or {}
        gating = sum(bank["hits"][name] for name in cq.GATING_CHECKS) + bank["hits"]["duplicate_items"] \
            + bank["hits"]["near_duplicate_items"]
        lines.append(
            f"| {bank['slot']} | {bank['origin']} | {bank['items']} | {number(harness.get('target'))} | "
            f"{'pass' if bank['passed'] else 'fail'} | {gating} | {key.get('items', 0)}/{key.get('of', 0)} | "
            + (f"{validated.get('passed')}/{validated.get('total')}" if validated else "—") + " | "
            + (f"{review.get('agreed_first_pass')}, {review.get('agreed_after_repair')}, {review.get('dropped')}"
               if review else "—")
            + f" | {number(harness.get('calls'))} | "
            + (number(harness['seconds'] / 60) if harness.get("seconds") is not None else "—") + " |")
    return lines


def markdown(summary, here):
    slots, ground, cost = summary["slots"], summary["grounding"], summary["cost"]
    lines = [f"# Course eval: {summary['set']}", "",
             f"1. **Slots passed** (mechanical checks and every judged criterion): outlines "
             f"{fraction(slots['outline'])}, lessons {fraction(slots['lesson'])}, quizzes {fraction(slots['quiz'])} "
             f"(mechanical only: {fraction(slots['outline'], 'mechanical')}, {fraction(slots['lesson'], 'mechanical')}, "
             f"{fraction(slots['quiz'], 'mechanical')}). By shape: "
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
    lines += quiz_headline(summary["quiz"])
    if summary["too_long"]:
        lines.append(f"\n{summary['too_long']} items were {TOO_LONG}; they count as ungraded fails.")
    if summary["missing_requests"]:
        lines.append(f"\nNot in this run: {', '.join(summary['missing_requests'])}.")
    lines += ["", "## Criteria", "", "| criterion | passed | graded | items | too long |", "|---|---:|---:|---:|---:|"]
    lines += [f"| {c} {TITLES.get(c, 'True statement')} | {e['passed']} | {e['graded']} | {e['total']} | "
              f"{e['too_long']} |" for c, e in summary["criteria"].items()]
    lines += quiz_tables(summary["quiz"])
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
        lines += ["", "## Compare with the previous run", ""]
        if summary.get("previous_rubric") not in (None, summary["rubric"]):
            lines += [f"The previous run was judged under {summary['previous_rubric']}, this one under "
                      f"{summary['rubric']}: lesson slots no longer include L5 and quiz slots are new, so the slot "
                      f"comparison crosses rubric versions and says less.", ""]
        lines += ["```sh"]
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
    judged.add_argument("--allow-self-judge", action="store_true",
                        help="judge the quiz stage even when the judge is the model that wrote the bank; the "
                             "manifest records it")
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
        answer_model = next(iter(models)) if len(models) == 1 else None
        identity = judge_identity(args.endpoint, args.model, answer_model)
        window = args.window or served_window(identity, args.model)
        own = identity.get("judges_own_answers") or (answer_model is not None and args.model == answer_model)
        independent = False if own else (True if identity.get("judges_own_answers") is False else None)
        if window is None:
            print("no judge window given or listed: every prompt is sent whole, none is refused for size")
        settings = judge_request(args.model, []) | {
            "messages": None, "rubric": RUBRIC, "identity": identity, "window": window,
            "chars_per_token": CHARS_PER_TOKEN, "independent": independent,
            "self_judge_allowed": bool(args.allow_self_judge),
            "system_sha256": digest([COURSE_JUDGE_SYSTEM, CLAIM_JUDGE_SYSTEM, QUIZ_JUDGE_SYSTEM, QUIZ_SOLVE_SYSTEM]),
            "solve_schema": SOLVE_SCHEMA}
        if independent is None:
            print("the judge's independence from the course model could not be established from the server's "
                  "model listing; the manifest records it as unknown")
        result = score(args.results, http_judge(args.endpoint, args.model), settings, window, args.claims,
                       http_solver(args.endpoint, args.model), args.allow_self_judge)
        print(json.dumps(result, indent=2))
    else:
        summarize(args.results)
        print((args.results / "summary.md").read_text())


if __name__ == "__main__":
    main()
