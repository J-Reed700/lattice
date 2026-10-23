import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import course_eval as ce
import course_eval_quiz as cq
import shipped_path_eval as spe
import shipped_path_stats as sps
from chat_rag_eval import load_jsonl

REQUEST = {"goal": "Tune Postgres queries", "priorKnowledge": "Knows SQL, never tuned a database",
           "minutesPerSession": 45, "documentIds": [], "sourceUrls": [], "courseDepth": "course"}
OUTLINE = {
    "status": "passed", "issues": [], "repair_passes": 0,
    "sources": [{"id": "s1", "title": "Postgres docs", "url": "https://example.org/docs"}],
    "modules": [{"title": "Plans", "summary": "Reading what the planner chose.", "outcomes": ["Read a plan"],
                 "lessons": [{"id": "l1", "title": "EXPLAIN", "objective": "Read EXPLAIN output",
                              "estimated_minutes": 40},
                             {"id": "l2", "title": "Indexes", "objective": "Choose an index for a query",
                              "estimated_minutes": 45}],
                 "project": {"title": "Tune a query", "brief": "Fix one slow query.", "deliverables": ["a plan"],
                             "successCriteria": ["it runs faster"]}}],
}
LESSON = {
    "id": "l1", "title": "EXPLAIN", "objective": "Read EXPLAIN output", "estimatedMinutes": 40,
    "preparation": "ready", "completed": False,
    "blocks": [
        {"kind": "explanation", "title": "What a plan is", "body": "A plan is a tree.\n\nEach node feeds its parent.",
         "sourceIds": ["s1"], "rubric": []},
        {"kind": "worked_example", "title": "A slow join", "body": "Step 1: run EXPLAIN ANALYZE.", "sourceIds": [],
         "rubric": []},
        {"kind": "guided_practice", "title": "Your turn", "body": "Read this plan.", "sourceIds": [],
         "rubric": [{"id": "c1", "dimension": "accuracy", "title": "Names the scan",
                     "description": "Identifies the sequential scan", "maxPoints": 2}]},
    ],
    "questions": [{"id": "q1", "kind": "quiz", "prompt": "Which node reads every row?",
                   "options": ["Index Scan", "Seq Scan", "Hash"], "sourceIds": ["s1"], "correctIndex": 1,
                   "explanation": "A sequential scan reads the whole table."}],
}
REPORT = {"findings": [
    {"unit": 0, "quote": "A plan is a tree.", "statement": "A query plan is a tree of nodes.", "verdict": "supported",
     "resolution": None},
    {"unit": 0, "quote": "x", "statement": "Postgres caches every plan forever.", "verdict": "unsupported",
     "resolution": {"kind": "general_knowledge"}},
    {"unit": 3, "quote": "y", "statement": "A sequential scan reads every row.", "verdict": "supported",
     "resolution": None},
]}
METRICS = {
    "outline": {"review": {"status": "passed", "issues": 0, "repair_passes": 0},
                "quotes": {"checked": 3, "unmatched": 0}, "cost": {"seconds": 600, "calls": 2}},
    "lessons": {
        "m1.l1": {"outcome": "ready", "citations": {"source_ids": 2, "unresolved": 0},
                  "claims": {"total": 3, "supported": 2, "unsupported": 1, "contradicted": 0, "unverified": 0,
                             "general_knowledge": 1, "quotes_unmatched": 0},
                  "cost": {"seconds": 7200, "calls": 3}},
        "m1.l2": {"outcome": "failed", "error": "evidence ran out", "cost": {"seconds": 3600, "calls": 1}},
    },
}
SET = {"schema": "course-eval-requests/1", "name": "course-eval-test", "items": [
    {"id": "pg", "request": "requests/pg.json", "shape": "full",
     "outline_must_cover": ["reading EXPLAIN output", "choosing indexes"],
     "prior_knowledge_excludes": ["everyday SQL"]},
    {"id": "dotnet", "request": "requests/dotnet.json", "shape": "full", "optional": True,
     "outline_must_cover": [], "prior_knowledge_excludes": []},
]}
# Appendix A, copied by hand: a drift in the scorer's constants fails here.
O1_INTRO = ("Each of the following topics must be the stated objective or outcome of at least one module or lesson, "
            "not merely mentioned in passing:")
# Appendix C (`lattice-course-judge/2`), copied by hand.
Q1_TEXT = ("A learner who read only this lesson can answer the item; it needs no outside fact the lesson does not "
           "teach.")
Q3_TEXT = ("Each wrong option is one a learner who missed the point could pick; none is a joke, a category error, or "
           "trivially excluded by grammar or length.")
Q5_EXPLANATION = ("Each rubric criterion names an observable thing in the answer at incomplete, adequate and strong "
                  "levels, without stating the answer.")
SETTINGS = {"model": "fake-judge", "rubric": ce.RUBRIC}
# A recorded bank for m1.l1 (`quiz-m1.l1.json`): one item per kind of case the stage must handle.
BANK = {"schema": "course-eval-quiz-bank/1", "selector": "m1.l1", "sources": {"s1-v1": "A plan is a tree of nodes."},
        "metrics": {"target": 6, "bank_size": 5, "validated": {"passed": 5, "total": 7},
                    "key_review": {"agreed_first_pass": 4, "agreed_after_repair": 1, "dropped": 1},
                    "calls": 12, "seconds": 600},
        "items": [
            {"id": "c1", "format": "single_choice", "prompt": "Which node reads every row of a table?",
             "options": ["Index Scan", "Seq Scan", "Bitmap Heap Scan"], "key": [1],
             "explanation": "A sequential scan visits each row.", "source_version_id": "s1-v1",
             "source_quote": "A plan is a tree of nodes."},
            {"id": "t1", "format": "true_false", "prompt": "Each node of a plan feeds its parent.",
             "options": ["True", "False"], "key": [0], "explanation": "The lesson says so.", "lesson_block_id": "b1"},
            {"id": "m1", "format": "matching", "prompt": "Match each node to what it does.",
             "left": ["Seq Scan", "Sort", "Hash"], "right": ["reads every row", "orders rows", "builds a hash table",
                                                              "joins two inputs"], "key": [0, 1, 2],
             "explanation": "Each node does one job.", "lesson_block_id": "b1"},
            {"id": "e1", "format": "explanation", "prompt": "Explain why a plan is read from the leaves up.",
             "rubric": [{"id": "r1", "title": "Data flow", "max_points": 2,
                         "levels": {"incomplete": "no direction", "adequate": "children first",
                                    "strong": "children feed parents"}},
                        {"id": "r2", "title": "Example", "max_points": 2,
                         "levels": {"incomplete": "none", "adequate": "names a node", "strong": "walks a plan"}}],
             "key_points": ["children produce rows for parents"], "lesson_block_id": "b1"},
            {"id": "bad", "format": "single_choice", "prompt": "Broken", "options": ["only"], "key": [3]},
        ]}


class FakeSolver:
    """Answers every item with its key unless told otherwise; records every prompt."""

    def __init__(self, answers=None, other=()):
        self.calls, self.answers, self.other = [], answers or {}, other

    def __call__(self, messages):
        self.calls.append(messages)
        item = next(i for i in BANK["items"] if f"Question: {i['prompt']}\n" in messages[1]["content"])
        return {"answer": self.answers.get(item["id"], cq.key_text(item)), "other_defensible": item["id"] in self.other,
                "reason": "from the lesson"}


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value if isinstance(value, str) else json.dumps(value))


def build_run(root):
    """A raw run directory as section 6.2 describes it: one request, one Ready and one failed lesson."""
    request = root / "run" / "pg"
    write(request / "run.json", {"schema": "course-eval-run/1", "request_id": "pg", "recipe_sha256": "r1",
                                 "recipe": {"requests": "evals/courses/course-eval-v1.json",
                                            "lessons": ["m1.l1", "m1.l2"], "server": {"model": "answer-model"}},
                                 "commit": "abc123", "server": {"thinking": "off", "gguf_sha256": "g1"},
                                 "web": {"mode": "replay", "trace_index_sha256": "t1"}})
    write(request / "request.json", REQUEST)
    write(request / "recipe.json", {})
    write(request / "outline.json", OUTLINE)
    write(request / "metrics.json", METRICS)
    write(request / "lesson-m1.l1.json", {"selector": "m1.l1", "lesson_id": "l1", "outcome": "ready", "error": None,
                                          "lesson": LESSON, "report": json.dumps(REPORT), "sources": []})
    write(request / "lesson-m1.l2.json", {"selector": "m1.l2", "lesson_id": "l2", "outcome": "failed",
                                          "error": "evidence ran out", "lesson": None, "report": None, "sources": []})
    write(request / "web-trace-report.json", {"mode": "replay", "hits": 40, "misses_filled": 2, "misses_failed": 0,
                                              "untraced_calls": 0})
    write(request / "phases.jsonl", json.dumps({"at": 1, "selector": "m1.l1", "status": "running"}) + "\n")
    write(request / "done.json", {"outline": "passed", "lessons": {"m1.l1": "ready", "m1.l2": "failed"}})
    for n in range(1, 7):
        write(request / f"call-{n}.json", {"input": "x", "answer": "y", "inputTokens": 100 * n,
                                           "outputTokens": 10 * n})
    write(root / "set.json", SET)
    return root / "run"


class FakeJudge:
    """Passes every criterion except the named lesson criteria; records every prompt it was sent."""

    def __init__(self, failing=()):
        self.calls, self.failing = [], failing

    def __call__(self, messages):
        self.calls.append(messages)
        texts = {**{k: v[1] for k, v in ce.LESSON_CRITERIA.items()}, **{k: v[1] for k, v in ce.QUIZ_CRITERIA.items()}}
        failed = any(texts[c] in messages[1]["content"] for c in self.failing)
        return (not failed, "FALSE: it is wrong" if failed else "the named part meets it")


class Rendering(unittest.TestCase):
    def test_lesson_rendering_is_deterministic_and_complete(self):
        document = {"lesson": LESSON}
        text = ce.render_lesson(document, REQUEST)
        self.assertEqual(text, ce.render_lesson(json.loads(json.dumps(document)), REQUEST))
        for block in LESSON["blocks"]:
            self.assertIn(f"## [{block['kind']}] {block['title']}\n\n{block['body']}\n", text)
        self.assertIn("- Names the scan: Identifies the sequential scan (2 points)", text)
        self.assertIn("Prior knowledge: Knows SQL, never tuned a database\nMinutes per session: 45", text)
        # Under lattice-course-judge/2 the questions are quiz items: the lesson text never carries their keys.
        self.assertNotIn("Which node reads every row?", text)
        self.assertNotIn("Correct:", text)

    def test_lesson_questions_become_items_with_the_lesson_files_key(self):
        bare = {**LESSON, "questions": [{k: v for k, v in LESSON["questions"][0].items()
                                         if k not in ("correctIndex", "explanation")}]}
        with tempfile.TemporaryDirectory() as tmp:
            origin, bank = ce.load_bank(Path(tmp), "m1.l1", {"lesson": bare, "answer_key": {"q1": {"correct_index": 2}}})
            self.assertEqual(origin, "lesson_questions")
            self.assertEqual({k: bank["items"][0][k] for k in ("format", "key", "origin")},
                             {"format": "single_choice", "key": [2], "origin": "prose_step"})
            _, unkeyed = ce.load_bank(Path(tmp), "m1.l1", {"lesson": bare})
            self.assertEqual(cq.format_problems(unkeyed["items"][0]), ["single choice needs exactly one key"])
            self.assertEqual(ce.load_bank(Path(tmp), "m1.l1", {"lesson": {**LESSON, "questions": []}}), (None, None))
            write(Path(tmp) / "quiz-m1.l1.json", {"schema": "other"})
            with self.assertRaises(SystemExit):
                ce.load_bank(Path(tmp), "m1.l1", {"lesson": LESSON})

    def test_outline_rendering_lists_modules_lessons_and_the_project(self):
        text = ce.render_outline(OUTLINE, REQUEST)
        self.assertEqual(text, ce.render_outline(OUTLINE, REQUEST))
        for line in ("# Tune Postgres queries", "Goal: Tune Postgres queries", "Depth: course",
                     "## Module 1: Plans", "Reading what the planner chose.", "Outcomes:\n- Read a plan",
                     "- Lesson 1.1: EXPLAIN (40 min) — Read EXPLAIN output",
                     "- Lesson 1.2: Indexes (45 min) — Choose an index for a query",
                     "Project milestone: Tune a query\nFix one slow query.\nDeliverables:\n- a plan\n"
                     "Success criteria:\n- it runs faster"):
            self.assertIn(line, text)


class RubricLines(unittest.TestCase):
    def test_lines_are_returned_and_chat_rubrics_are_unchanged(self):
        rubric = {"lines": ["first", "second"]}
        lines = spe.rubric_lines(rubric)
        self.assertEqual(lines, ["first", "second"])
        lines.append("third")
        self.assertEqual(rubric["lines"], ["first", "second"])
        chat = {"abstain": False, "must_mention": [["x", "y"]], "must_not_claim": ["z"], "notes": "n"}
        self.assertEqual(spe.rubric_lines(chat), [
            "- The documents contain the answer. Declining to answer fails.", '- Must state: "x" (or) "y"',
            '- Must not claim: "z"', "- What a correct answer says: n"])
        self.assertEqual(spe.rubric_lines({"abstain": True, "notes": "n"})[0],
                         "- The documents do not contain this answer. A correct answer says so and "
                         "does not invent one.")


class Pipeline(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.run_dir = build_run(self.root)
        self.out = self.root / "results" / "model" / "2026-10-22-course-eval-v1-base"
        self.manifest = ce.collect(self.run_dir, self.out, self.root / "set.json")

    def tearDown(self):
        self.tmp.cleanup()

    def items(self, name="items.json"):
        return {item["id"]: item for item in json.loads((self.out / name).read_text())["items"]}

    def test_collect_writes_every_results_file(self):
        names = {p.relative_to(self.out).as_posix() for p in self.out.rglob("*") if p.is_file()}
        self.assertEqual(names, {"run.json", "items.json", "answers.jsonl", "claims.items.json", "claims.jsonl",
                                 "quiz.items.json", "quiz.jsonl",
                                 *(f"requests/pg/{n}" for n in ("request.json", "outline.json", "metrics.json",
                                                                "web-trace-report.json", "phases.jsonl",
                                                                "lesson-m1.l1.json", "lesson-m1.l2.json",
                                                                "lesson-m1.l1.md"))})
        self.assertEqual(self.manifest["skipped_optional"], ["dotnet"])
        self.assertEqual(self.manifest["missing_requests"], [])
        self.assertEqual(len(self.manifest["calls_index_sha256"]["pg"]), 64)
        self.assertEqual([s["slot"] for s in self.manifest["slots"]],
                         ["pg/outline", "pg/m1.l1", "pg/m1.l2", "pg/m1.l1/quiz", "pg/m1.l2/quiz"])
        self.assertEqual([s["mechanical_checks_passed"] for s in self.manifest["slots"]], [True, True, False, True, False])
        self.assertEqual([s["outcome"] for s in self.manifest["slots"][3:]], ["lesson_questions", "no lesson"])

    def test_items_carry_appendix_lines_and_the_requests_lists(self):
        items = self.items()
        self.assertEqual(len(items), 5 + 2 * 6)
        self.assertNotIn("pg/m1.l1/L5", items)
        self.assertEqual(items["pg/outline/O1"]["turns"][0]["rubric"]["lines"],
                         [O1_INTRO, "- reading EXPLAIN output", "- choosing indexes", ce.O1_CLOSE])
        self.assertIn("- everyday SQL", items["pg/outline/O2"]["turns"][0]["rubric"]["lines"])
        for criterion, (_, text) in ce.LESSON_CRITERIA.items():
            self.assertEqual(spe.rubric_lines(items[f"pg/m1.l2/{criterion}"]["turns"][0]["rubric"]), [text])
        self.assertTrue(items["pg/outline/O3"]["turns"][0]["ask"].startswith("Goal: Tune Postgres queries\n"))
        claims = self.items("claims.items.json")
        self.assertEqual(sorted(claims), ["pg/m1.l1/claim/0-1", "pg/m1.l1/claim/0-2", "pg/m1.l1/claim/3-1"])
        self.assertTrue(claims["pg/m1.l1/claim/0-2"]["general_knowledge"])
        answers = {row["query_id"]: row for row in load_jsonl(self.out / "claims.jsonl")}
        self.assertEqual(answers["pg/m1.l1/claim/3-1"]["turns"][0]["answer"], "A sequential scan reads every row.")

    def test_unprepared_lesson_is_ungraded_and_never_judged(self):
        judge = FakeJudge()
        report = ce.score(self.out, judge, SETTINGS, claims=True)
        self.assertEqual(len(judge.calls), 5 + 6 + 3 + 3)  # outline, lesson, claims, quiz Q1/Q3/Q4 of one question
        self.assertEqual(report["items"], {"graded": 11, "skipped": 0, "no_artifact": 6, "too_long": 0})
        self.assertEqual(report["quiz"], {"graded": 0, "skipped": 0, "ungraded": 1})  # Q2 had no solver
        rows = {row["query_id"]: row for row in load_jsonl(self.out / "scored.jsonl")}
        for criterion in ce.LESSON_CRITERIA:
            row = rows[f"pg/m1.l2/{criterion}"]
            self.assertFalse(row["judge_pass"])
            self.assertEqual(row["turn_scores"][0]["judge"], {"pass": False, "reason": ce.NO_ARTIFACT, "graded": False})
        claims = load_jsonl(self.out / "claims.scored.jsonl")
        self.assertEqual({row["pipeline_verdict"] for row in claims}, {"supported", "unsupported"})

    def test_token_sums_follow_call_ranges(self):
        metrics = json.loads((self.out / "requests/pg/metrics.json").read_text())
        self.assertEqual({k: metrics["outline"]["cost"][k] for k in ("input_tokens", "output_tokens",
                                                                     "peak_input_tokens", "call_files")},
                         {"input_tokens": 300, "output_tokens": 30, "peak_input_tokens": 200, "call_files": 2})
        self.assertEqual(metrics["lessons"]["m1.l1"]["cost"]["input_tokens"], 300 + 400 + 500)
        self.assertEqual(metrics["lessons"]["m1.l1"]["cost"]["peak_input_tokens"], 500)
        self.assertEqual(metrics["lessons"]["m1.l2"]["cost"]["input_tokens"], 600)
        explicit = {"outline": {"cost": {"first_call": 2, "last_call": 3}},
                    "lessons": {"m1.l1": {"cost": {"calls": 2}}}}
        self.assertEqual(ce.call_ranges(explicit, [5, 1, 4, 3, 2]), {"outline": [2, 3], "m1.l1": [4, 5]})

    def test_slot_rows_are_the_conjunction_of_their_items(self):
        ce.score(self.out, FakeJudge(failing=("L3",)), SETTINGS)
        rows = load_jsonl(self.out / "scored.jsonl")
        slots = {row["query_id"]: row for row in load_jsonl(self.out / "slots.scored.jsonl")}
        self.assertEqual(sorted(slots), ["pg/m1.l1", "pg/m1.l1/quiz", "pg/m1.l2", "pg/m1.l2/quiz", "pg/outline"])
        for slot, row in ((k, v) for k, v in slots.items() if not k.endswith("/quiz")):
            members = [r for r in rows if r["slot"] == slot]
            self.assertEqual(row["judge_pass"], all(r["judge_pass"] for r in members))
            self.assertEqual(row["turn_scores"][0]["judge"]["graded"],
                             all(r["turn_scores"][0]["judge"]["graded"] for r in members))
        self.assertTrue(slots["pg/outline"]["judge_pass"])
        self.assertIn("L3: FALSE: it is wrong", slots["pg/m1.l1"]["turn_scores"][0]["judge"]["reason"])
        self.assertFalse(slots["pg/m1.l2"]["mechanical_checks_passed"])

    def test_rows_satisfy_compare_and_calibration_sample(self):
        ce.score(self.out, FakeJudge(failing=("L3",)), SETTINGS)
        rows = load_jsonl(self.out / "scored.jsonl")
        report = sps.compare([rows], [rows], kind="both")
        self.assertEqual((report["paired"], report["verdict"]), (17, "can't tell from noise"))
        self.assertIn("verdict", sps.compare_markdown(report))
        slots = load_jsonl(self.out / "slots.scored.jsonl")
        self.assertEqual(sps.compare([slots], [slots], kind="both")["paired"], 5)
        questions = json.loads((self.out / "items.json").read_text())
        labels = sps.calibration_sample(questions, load_jsonl(self.out / "answers.jsonl"), rows, half_width=0.0)
        self.assertEqual(labels["frame"], {"judge pass": {"population": 10, "sample": 10},
                                           "judge fail": {"population": 1, "sample": 1},
                                           "unanswered": {"population": 6, "sample": 0}})
        first = labels["items"][0]
        self.assertEqual(first["turns"][0]["rubric"], self.items()[first["query_id"]]["turns"][0]["rubric"]["lines"])
        self.assertEqual(sps.calibration_score(rows, labels)["labelled"], 0)

    def test_a_prompt_over_the_window_is_refused_not_truncated(self):
        item = self.items()["pg/m1.l1/L1"]
        answer = load_jsonl(self.out / "answers.jsonl")[5]["turns"][0]["answer"]
        needed = ce.prompt_tokens(ce.judge_messages(item, answer)) + spe.judge_request("", [])["max_tokens"]
        judge = FakeJudge()
        self.assertEqual(ce.grade(item, answer, judge, needed - 1),
                         {"pass": False, "reason": ce.TOO_LONG, "graded": False})
        self.assertEqual(judge.calls, [])
        self.assertTrue(ce.grade(item, answer, judge, needed)["graded"])
        self.assertIn("<<<\n" + answer + "\n>>>", judge.calls[0][1]["content"])
        report = ce.score(self.out, FakeJudge(), SETTINGS, window=50)
        self.assertEqual(report["items"], {"graded": 0, "skipped": 0, "no_artifact": 6, "too_long": 11})
        quiz = load_jsonl(self.out / "quiz.scored.jsonl")[0]
        self.assertEqual({t["judge"]["reason"] for t in quiz["turn_scores"]}, {ce.TOO_LONG})

    def test_score_resumes_and_refuses_another_judge(self):
        ce.score(self.out, FakeJudge(), SETTINGS)
        judge = FakeJudge()
        report = ce.score(self.out, judge, SETTINGS)
        self.assertEqual((judge.calls, report["items"]["skipped"], report["quiz"]["skipped"]), ([], 17, 1))
        self.assertEqual(len(load_jsonl(self.out / "scored.jsonl")), 17)
        with self.assertRaises(SystemExit):
            ce.score(self.out, judge, {**SETTINGS, "model": "another-judge"})

    def test_collect_refuses_an_existing_output(self):
        before = (self.out / "items.json").read_bytes()
        with self.assertRaises(SystemExit):
            ce.collect(self.run_dir, self.out, self.root / "set.json")
        self.assertEqual((self.out / "items.json").read_bytes(), before)
        empty = self.root / "empty"
        empty.mkdir()
        with self.assertRaises(SystemExit):
            ce.collect(self.run_dir, empty, self.root / "set.json")
        self.assertEqual(list(empty.iterdir()), [])

    def test_summary_headline_tables_and_compare_line(self):
        earlier = self.out.parent / "2026-10-20-course-eval-v1-base"
        ce.collect(self.run_dir, earlier, self.root / "set.json")
        ce.score(earlier, FakeJudge(), SETTINGS)
        ce.score(self.out, FakeJudge(failing=("L3",)), SETTINGS, claims=True)
        summary = ce.summarize(self.out)
        self.assertEqual(summary["slots"]["outline"], {"passed": 1, "mechanical": 1, "total": 1})
        self.assertEqual(summary["slots"]["lesson"], {"passed": 0, "mechanical": 1, "total": 2})
        self.assertEqual(summary["slots"]["quiz"], {"passed": 0, "mechanical": 1, "total": 2})
        self.assertEqual(summary["quiz"]["banks"][0]["origin"], "lesson_questions")
        self.assertEqual(summary["lessons_ready"]["failures"],
                         [{"slot": "pg/m1.l2", "outcome": "failed", "error": "evidence ran out"}])
        self.assertEqual((summary["grounding"]["supported"], summary["grounding"]["total"],
                          summary["grounding"]["general_knowledge"]), (2, 3, 1))
        self.assertEqual(summary["cost"]["lesson_minutes"], {"median": 90.0, "max": 120.0})
        self.assertEqual(summary["criteria"]["L3"], {"passed": 0, "graded": 1, "total": 2, "too_long": 0})
        page = (self.out / "summary.md").read_text()
        self.assertIn("web: 40 hits, 2 misses filled, 0 failed", page)
        self.assertIn(f"--baseline {ce.relative(earlier)}/slots.scored.jsonl", page)
        self.assertTrue((self.out / "summary.json").exists())



class QuizStage(unittest.TestCase):
    """A run whose m1.l1 has a recorded bank (`quiz-m1.l1.json`)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.run_dir = build_run(self.root)
        write(self.run_dir / "pg" / "quiz-m1.l1.json", BANK)
        self.out = self.root / "results" / "model" / "2026-10-23-course-eval-v1-base"
        ce.collect(self.run_dir, self.out, self.root / "set.json")
        self.doc = json.loads((self.out / "quiz.items.json").read_text())
        self.items = {item["id"]: item for item in self.doc["items"]}

    def tearDown(self):
        self.tmp.cleanup()

    def test_criteria_follow_the_format(self):
        turns = {i.split("/")[-1]: [t["criterion"] for t in item["turns"]] for i, item in self.items.items()}
        self.assertEqual(turns, {"c1": ["Q1", "Q2", "Q3", "Q4"], "t1": ["Q1", "Q2", "Q4"],
                                 "m1": ["Q1", "Q2", "Q3", "Q4"], "e1": ["Q1", "Q4", "Q5"], "bad": ["format"]})
        self.assertEqual(self.items["pg/m1.l1/quiz/e1"]["turns"][2]["rubric"]["lines"], [Q5_EXPLANATION])
        self.assertEqual(self.items["pg/m1.l1/quiz/c1"]["turns"][0]["rubric"]["lines"], [Q1_TEXT])
        self.assertEqual(ce.quiz_lines("Q5", {"format": "cloze"}), [ce.Q5_TEXT["typed"]])
        self.assertTrue((self.out / "requests/pg/quiz-m1.l1.json").exists())

    def test_mechanical_checks_go_into_metrics(self):
        quiz = json.loads((self.out / "requests/pg/metrics.json").read_text())["lessons"]["m1.l1"]["quiz"]
        self.assertEqual((quiz["origin"], quiz["items"], quiz["passed"]), ("bank", 5, False))
        self.assertEqual(quiz["hits"]["format_invalid"], ["bad"])
        self.assertEqual(quiz["harness"]["target"], 6)
        self.assertEqual(quiz["quotes_verbatim"], {"verbatim": 1, "checked": 1})
        self.assertFalse(self.items["pg/m1.l1/quiz/bad"]["mechanical_checks_passed"])
        self.assertTrue(self.items["pg/m1.l1/quiz/c1"]["mechanical_checks_passed"])

    def test_judges_see_the_key_only_where_the_criterion_needs_it(self):
        judge, solver = FakeJudge(), FakeSolver()
        ce.score(self.out, judge, SETTINGS, solver=solver)
        prompts = {}
        for call in judge.calls + solver.calls:
            content = call[1]["content"]
            if "Question: Which node reads every row of a table?" in content:
                criterion = next((c for c, (_, text) in ce.QUIZ_CRITERIA.items() if text and text in content), "Q2")
                prompts[criterion] = (call[0]["content"], content)
        self.assertEqual(sorted(prompts), ["Q1", "Q2", "Q3", "Q4"])
        self.assertNotIn("Key: B", prompts["Q1"][1])
        self.assertNotIn("Key: B", prompts["Q2"][1])
        self.assertIn("Key: B", prompts["Q3"][1])
        self.assertIn("Key: B", prompts["Q4"][1])
        self.assertEqual(prompts["Q2"][0], ce.QUIZ_SOLVE_SYSTEM)
        self.assertEqual(prompts["Q1"][0], ce.QUIZ_JUDGE_SYSTEM)
        self.assertIn("## [explanation] What a plan is", prompts["Q1"][1])
        self.assertNotIn("Correct:", prompts["Q1"][1])
        self.assertFalse(any("Question: Broken" in c[1]["content"] for c in judge.calls + solver.calls))

    def test_q2_is_decided_by_code(self):
        item = BANK["items"][0]
        self.assertTrue(ce.solve_verdict(item, {"answer": "B", "other_defensible": False, "reason": "r"})["pass"])
        wrong = ce.solve_verdict(item, {"answer": "A", "other_defensible": False, "reason": "r"})
        self.assertFalse(wrong["pass"])
        self.assertIn("disagrees", wrong["reason"])
        both = ce.solve_verdict(item, {"answer": "B", "other_defensible": True, "reason": "r"})
        self.assertFalse(both["pass"])
        self.assertIn("another answer is defensible", both["reason"])
        matching = BANK["items"][2]
        self.assertTrue(ce.solve_verdict(matching, {"answer": "1-A, 2-B, 3-C", "other_defensible": False})["pass"])
        self.assertFalse(ce.solve_verdict(matching, {"answer": "1-A, 2-C, 3-B", "other_defensible": False})["pass"])
        self.assertFalse(ce.solve_verdict(item, {"answer": "no idea", "other_defensible": False})["pass"])

    def test_quiz_slot_needs_ninety_percent_of_items(self):
        slot = {"slot": "r/m1.l1/quiz", "request": "r", "shape": "full", "stage": "quiz", "lesson": "m1.l1",
                "outcome": "bank", "mechanical_checks_passed": True}
        entries = [{"id": f"r/m1.l1/quiz/{n}", "slot": slot["slot"]} for n in range(10)]

        def rows(passing):
            return [{"query_id": e["id"], "judge_pass": n < passing, "answer_sha256": str(n), "run_id": "x",
                     "turn_scores": [{"turn": 0, "criterion": "Q1", "judge": {"pass": n < passing, "reason": "",
                                                                           "graded": True}}]}
                    for n, e in enumerate(entries)]
        for passing, expected in ((10, True), (9, True), (8, False)):
            out, incomplete = ce.quiz_slot_rows([slot], {"items": entries}, rows(passing))
            self.assertEqual((out[0]["judge_pass"], incomplete), (expected, []))
            self.assertEqual(out[0]["turn_scores"][0]["judge"]["reason"],
                             f"{passing} of 10 items pass every judged criterion")
        out, incomplete = ce.quiz_slot_rows([slot], {"items": entries}, rows(10)[:9])
        self.assertEqual((out, incomplete), ([], ["r/m1.l1/quiz"]))
        empty, _ = ce.quiz_slot_rows([{**slot, "outcome": "no bank"}], {"items": []}, [])
        self.assertEqual(empty[0]["turn_scores"][0]["judge"], {"pass": False, "reason": "no quiz items (no bank)",
                                                               "graded": False})

    def test_the_judge_must_not_be_the_bank_author(self):
        judge = FakeJudge()
        report = ce.score(self.out, judge, {**SETTINGS, "independent": False}, solver=FakeSolver())
        self.assertIn("refused", report["quiz"])
        self.assertFalse((self.out / "quiz.scored.jsonl").exists())
        self.assertFalse(any("Question:" in c[1]["content"] for c in judge.calls))
        allowed = {**SETTINGS, "independent": False, "self_judge_allowed": True}
        other = self.root / "results" / "model" / "2026-10-24-course-eval-v1-self"
        ce.collect(self.run_dir, other, self.root / "set.json")
        report = ce.score(other, FakeJudge(), allowed, solver=FakeSolver(), allow_self_judge=True)
        self.assertEqual(report["quiz"]["graded"], 4)
        manifest = json.loads((other / "quiz.scored.jsonl.manifest.json").read_text())
        self.assertEqual((manifest["settings"]["judge"]["independent"],
                          manifest["settings"]["judge"]["self_judge_allowed"]), (False, True))

    def test_rows_feed_compare_calibration_and_the_summary(self):
        ce.score(self.out, FakeJudge(failing=("Q3",)), SETTINGS, solver=FakeSolver(answers={"t1": "B"}))
        rows = {row["query_id"]: row for row in load_jsonl(self.out / "quiz.scored.jsonl")}
        self.assertFalse(rows["pg/m1.l1/quiz/c1"]["judge_pass"])  # Q3 failed
        self.assertFalse(rows["pg/m1.l1/quiz/t1"]["judge_pass"])  # the blind solve disagreed
        self.assertFalse(rows["pg/m1.l1/quiz/m1"]["judge_pass"])  # Q3 applies: the right side has a distractor
        self.assertTrue(rows["pg/m1.l1/quiz/e1"]["judge_pass"])
        self.assertFalse(rows["pg/m1.l1/quiz/bad"]["turn_scores"][0]["judge"]["graded"])
        scored = list(rows.values())
        self.assertEqual(sps.compare([scored], [scored], kind="both")["paired"], 5)
        labels = sps.calibration_sample(self.doc, load_jsonl(self.out / "quiz.jsonl"), scored, half_width=0.0)
        self.assertEqual(labels["frame"]["unanswered"]["population"], 1)
        labelled = next(i for i in labels["items"] if i["query_id"] == "pg/m1.l1/quiz/c1")
        self.assertEqual([t["rubric"] for t in labelled["turns"]][2], [Q3_TEXT])
        self.assertIn("Key: B", "\n".join(labelled["turns"][0]["answer"]))
        slot = {row["query_id"]: row for row in load_jsonl(self.out / "slots.scored.jsonl")}["pg/m1.l1/quiz"]
        self.assertEqual(slot["turn_scores"][0]["judge"]["reason"], "1 of 5 items pass every judged criterion")
        summary = ce.summarize(self.out)
        self.assertEqual(summary["quiz"]["items"], {"passing": 1, "total": 5, "ungraded": 1})
        self.assertEqual(summary["quiz"]["criteria"]["Q3"], {"passed": 0, "graded": 2, "total": 2, "too_long": 0})
        self.assertEqual(summary["quiz"]["mechanical"]["format_invalid"], 1)
        page = (self.out / "summary.md").read_text()
        self.assertIn("5. **Quizzes**: 1/5 items pass every judged criterion", page)
        self.assertIn("| Q3 Plausible distractors | 0 | 2 | 2 | 0 |", page)
        self.assertIn("| pg/m1.l1/quiz | bank | 5 | 6 | fail |", page)
        positions = summary["quiz"]["key_positions"]
        self.assertEqual((positions["counts"], positions["true"], positions["skewed_banks"]),
                         ({"A": 0, "B": 1, "C": 0}, 1, 0))
        self.assertIn("key positions A 0, B 1, C 0 (pooled exact p 1;", page)


if __name__ == "__main__":
    unittest.main()
