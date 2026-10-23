"""Offline tests for the quiz grading set: set validation and freezing, the scorer's chat-shaped rows, the
rationale checks of section 4.3a, the blind labels, reasons, contest stability and the persona generator. The
fixture bank is biology, history and chemistry; no model is called."""
import copy
import json
import tempfile
import unittest
from pathlib import Path

import course_eval_grading as g
import course_eval_personas as personas
import course_eval_quiz as cq
import shipped_path_stats as sps
from chat_rag_eval import load_jsonl
from test_course_eval_quiz_rag_eval import CLOZE, MATCHING, MULTI, ORDERING, SHORT, SINGLE, TRUE_FALSE

SOURCES = {"bio-v1": "A competitive inhibitor binds the active site, so it competes with the substrate. It raises the "
                     "apparent Km while Vmax is unchanged, because enough substrate still outcompetes it.",
           "chem-v1": "The pH of a strong acid solution is minus the log of its hydrogen ion concentration.",
           "hist-v1": "The Magna Carta was sealed in 1215."}
EXPLAIN = {"id": "bio.expl", "subject": "biology", "format": "explanation",
           "prompt": "Explain how a competitive inhibitor changes an enzyme's Km and Vmax.",
           "rubric": [{"id": "c1", "title": "Binding site", "max_points": 2,
                       "levels": {"incomplete": "no site", "adequate": "binds the enzyme",
                                  "strong": "binds the active site, competing with substrate"}},
                      {"id": "c2", "title": "Kinetics", "max_points": 2,
                       "levels": {"incomplete": "no effect", "adequate": "Km rises",
                                  "strong": "Km rises and Vmax is unchanged, with the reason"}}],
           "key_points": ["competes for the active site", "apparent Km rises, Vmax unchanged"],
           "reference_answer": "It binds the active site and competes with the substrate, so the apparent Km rises "
                               "while Vmax is unchanged because enough substrate outcompetes it.",
           "source_version_id": "bio-v1", "source_quote": "A competitive inhibitor binds the active site"}
SUBJECT = {"bio.sc": "biology", "hist.tf": "history", "chem.ms": "chemistry", "chem.sa": "chemistry",
           "lang.cloze": "history", "hist.match": "history", "hist.order": "history"}
BANK = {"schema": "quiz-grading-bank/1", "name": "bank-test",
        "subjects": {"biology": {"title": "Enzymes", "software": False}, "history": {"title": "Medieval England",
                                                                                      "software": False},
                     "chemistry": {"title": "Acids", "software": False}},
        "sources": SOURCES,
        "items": [EXPLAIN] + [{**item, "subject": SUBJECT[item["id"]], "lesson_block_id": "b1"}
                              for item in (SINGLE, TRUE_FALSE, MULTI, SHORT, CLOZE, MATCHING, ORDERING)]}


def row(rid, item, persona, response, expected, note=None, **extra):
    out = {"id": rid, "item": item, "persona": persona, "author": "hand", "response": response, "expected": expected,
           **extra}
    return out | ({"contest_note": note} if note else {})


MODEL = {"path": "model"}
ROWS = [
    row("bio-correct", "bio.expl", "correct", {"text": "It sits in the active site and competes with the substrate, so "
                                                       "Km goes up but Vmax stays the same."}, MODEL),
    row("bio-partial", "bio.expl", "partial", {"text": "It binds the active site of the enzyme."}, MODEL),
    row("bio-wrong", "bio.expl", "wrong", {"text": "It destroys the enzyme so Vmax drops to zero."}, MODEL),
    row("bio-empty", "bio.expl", "empty", {"text": ""}, MODEL),
    row("bio-copy", "bio.expl", "near_miss", {"text": "It binds the enzyme and Km rises."}, MODEL),
    row("bio-contest", "bio.expl", "contesting", {"text": "It blocks where the substrate goes, so you need more "
                                                          "substrate for half speed; top speed is the same."}, MODEL,
        "Half speed is Km and top speed is Vmax, so I covered both."),
    row("chem-paraphrase", "chem.sa", "correct", {"text": "pH of two"}, MODEL),
    row("chem-contest", "chem.sa", "contesting", {"text": "around 2-ish"}, MODEL, "2-ish is 2."),
    row("chem-variant", "chem.sa", "correct", {"text": "2"}, {"path": "deterministic", "points": 1.0}),
    row("bio-sc-right", "bio.sc", "correct", {"selected_indices": [1]}, {"path": "deterministic", "points": 1.0}),
    row("hist-tf-wrong", "hist.tf", "wrong", {"selected_indices": [1]}, {"path": "deterministic", "points": 0.0}),
    row("chem-ms-partial", "chem.ms", "partial", {"selected_indices": [0, 1, 2]},
        {"path": "deterministic", "points": 0.333333}),
    row("lang-cloze-half", "lang.cloze", "partial", {"blank_responses": ["soy", "es"]},
        {"path": "deterministic", "points": 0.5}),
    row("hist-match-two", "hist.match", "near_miss", {"matches": [0, 1, 3]}, {"path": "deterministic", "points": 0.666667}),
    row("hist-order-right", "hist.order", "correct", {"ordered_values": ORDERING["values"]},
        {"path": "deterministic", "points": 1.0}),
]


def criterion(cid, verdict, points, top=2, missing="", words=(), full="", quote=None):
    return {"criterion_id": cid, "title": cid, "verdict": verdict, "points": points, "max_points": top,
            "what_was_missing": missing, "your_words": list(words), "full_credit_includes": full,
            "source_quote": quote, "source_version_id": "bio-v1" if quote else None}


def rationale(summary, *criteria):
    return {"summary": summary, "criteria": list(criteria), "grader_model": "fake-9b", "rubric_version": "r1",
            "graded_at": 1}


def grade(row_id, earned, possible, why, raw=None, status="graded", **extra):
    return {"row_id": row_id, "pass": "first", "seed": 0, "status": status, "points_earned": earned,
            "points_possible": possible, "grade_status": "provisional", "rationale": why, "raw": raw, **extra}


GOOD_FULL = rationale("Full marks.", criterion("c1", "met", 2, words=["sits in the active site"]),
                      criterion("c2", "met", 2, words=["Km goes up but Vmax stays the same"]))
PARTIAL = rationale("You named the site but not the kinetics.",
                    criterion("c1", "met", 2, words=["binds the active site"],
                              quote="A competitive inhibitor binds the active site"),
                    criterion("c2", "missed", 0, missing="The effect on Km and Vmax.",
                              full="Says the apparent Km rises and Vmax is unchanged."))
WRONG_SHOWN = rationale("The inhibitor does not destroy the enzyme.",
                        criterion("c1", "missed", 0, missing="Where it binds."),
                        criterion("c2", "missed", 0, missing="Km rises and Vmax is unchanged."))
WRONG_RAW = rationale("The inhibitor does not destroy the enzyme.",
                      criterion("c1", "partly_met", 0, missing="Where it binds.", words=["it ruins the enzyme"]),
                      criterion("c2", "missed", 0, missing="Km rises and Vmax is unchanged."))
COPYING = rationale("Partly right.", criterion("c1", "partly_met", 1, missing="The active site.",
                                               words=["It binds the enzyme"],
                                               full=EXPLAIN["reference_answer"]),
                    criterion("c2", "partly_met", 1, missing="Vmax.", words=["Km rises"]))
CONTEST = rationale("Km covered in other words; Vmax implied.", criterion("c1", "met", 2, words=["blocks where the substrate goes"]),
                    criterion("c2", "missed", 0, missing="Vmax named as unchanged."))
EQUIVALENT = rationale("'pH of two' means 2.", criterion("equivalent", "met", 1, top=1, words=["pH of two"]))
NOT_EQUIVALENT = rationale("'around 2-ish' is not a value.", criterion("equivalent", "missed", 0, top=1,
                                                                        missing="A value of 2."))


def grades_doc(rows_path, bank_path, extra=()):
    entries = [
        grade("bio-correct", 4, 4, GOOD_FULL), grade("bio-partial", 2, 4, PARTIAL),
        grade("bio-wrong", 0, 4, WRONG_SHOWN, raw=WRONG_RAW),
        grade("bio-empty", None, 4, {}, status="couldnt_grade", error="the model didn't explain this grade"),
        grade("bio-copy", 2, 4, COPYING), grade("bio-contest", 2, 4, CONTEST),
        grade("chem-paraphrase", 1, 1, EQUIVALENT), grade("chem-contest", 0, 1, NOT_EQUIVALENT),
        grade("chem-variant", 1.0, 1, {}), grade("hist-tf-wrong", 0.0, 1, {}), grade("chem-ms-partial", 0.5, 1, {}),
    ]
    for seed, points in enumerate((2, 2, 4)):
        entries.append({**grade("bio-contest", points, 4, CONTEST, response_to_note="Vmax was not named."),
                        "pass": "regrade", "seed": seed})
    for seed in range(2):
        entries.append({**grade("chem-contest", 0, 1, NOT_EQUIVALENT), "pass": "regrade", "seed": seed})
    return {"schema": "quiz-grading-grades/1", "set": "test-v1", "rows_sha256": g.sha256_file(rows_path),
            "bank_sha256": g.sha256_file(bank_path), "grader": {"model": "fake-9b", "rubric_version": "r1"},
            "grades": entries + list(extra)}


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.registry = self.root / "sets.json"
        self.rows_path, self.bank_path = self.root / "test-v1.jsonl", self.root / "bank.json"
        self.write_set(ROWS, BANK)
        g.write_json(self.registry, {"schema": "quiz-grading-sets/1",
                                     "sets": {"test-v1": {"rows": "test-v1.jsonl", "bank": "bank.json", "frozen": None}}})

    def tearDown(self):
        self.tmp.cleanup()

    def write_set(self, rows, bank):
        self.rows_path.write_text("".join(json.dumps(r) + "\n" for r in rows))
        g.write_json(self.bank_path, bank)

    def scored(self, extra=()):
        grades = self.root / "grades.json"
        g.write_json(grades, grades_doc(self.rows_path, self.bank_path, extra))
        out = self.root / "results" / "2026-10-23-test-v1"
        g.score("test-v1", grades, out, self.registry)
        return out


class SetValidation(Fixture):
    def test_the_fixture_and_the_committed_set_are_valid(self):
        self.assertEqual(g.validate(self.registry), {"test-v1": []})
        committed = g.validate()
        self.assertEqual(committed, {name: [] for name in committed})
        self.assertIn("quiz-grading-v1", committed)

    def test_every_rule(self):
        def problems(rows=ROWS, bank=BANK):
            return "; ".join(g.set_problems(rows, bank))
        bad = copy.deepcopy(ROWS)
        bad[9]["expected"]["points"] = 0.5
        self.assertIn("bio-sc-right: expected", problems(bad))
        cases = [
            ({"persona": "angry"}, "persona must be one of"),
            ({"author": "someone"}, "author must be 'hand' or 'model:<name>'"),
            ({"author": "model:qwen"}, "only after a person reviews it"),
            ({"response": {"selected_indices": [0]}}, "response must hold exactly 'text'"),
            ({"persona": "contesting"}, "every contesting row has one"),
            ({"persona": "empty"}, "an empty persona has an empty response"),
            ({"id": "a/b"}, "ids must not contain '/'"),
            ({"item": "nope"}, "unknown item 'nope'"),
        ]
        for change, problem in cases:
            self.assertIn(problem, problems([{**ROWS[0], **change}] + ROWS[1:]), change)
        contested = [row("x", "chem.sa", "contesting", {"text": "2"}, {"path": "deterministic", "points": 1.0}, "n")]
        self.assertIn("only model-graded answers can be contested", problems(ROWS + contested))
        self.assertIn("no rows for ordering", problems([r for r in ROWS if r["item"] != "hist.order"]))
        software = {**BANK, "subjects": {k: {**v, "software": True} for k, v in BANK["subjects"].items()}}
        self.assertIn("at least one not about software", problems(bank=software))
        unquoted = {**BANK, "items": [{**EXPLAIN, "source_quote": "not in the source"}] + BANK["items"][1:]}
        self.assertIn("bio.expl: source quote is not verbatim", problems(bank=unquoted))
        no_reference = {**BANK, "items": [{**EXPLAIN, "reference_answer": ""}] + BANK["items"][1:]}
        self.assertIn("needs a reference_answer", problems(bank=no_reference))
        self.assertIn("Duplicate id", problems(ROWS + ROWS[:1]))

    def test_freezing_locks_the_set(self):
        with self.assertRaises(SystemExit):
            g.freeze("test-v1", self.root / "no-baseline", self.registry)
        baseline = self.scored()
        frozen = g.freeze("test-v1", baseline, self.registry)
        self.assertEqual(frozen["rows_sha256"], g.sha256_file(self.rows_path))
        self.assertEqual(g.validate(self.registry), {"test-v1": []})
        with self.assertRaises(SystemExit):
            g.freeze("test-v1", baseline, self.registry)
        self.write_set(ROWS + [row("late", "bio.sc", "wrong", {"selected_indices": [0]},
                                   {"path": "deterministic", "points": 0.0})], BANK)
        self.assertEqual(g.validate(self.registry)["test-v1"],
                         ["test-v1.jsonl changed after test-v1 was frozen; make a new version instead"])
        with self.assertRaises(SystemExit):
            personas.merge(self.root / "draft.jsonl", "test-v1", self.registry)


class Scoring(Fixture):
    def test_rows_in_the_chat_eval_shape(self):
        out = self.scored()
        self.assertEqual({p.name for p in out.iterdir()},
                         {"items.json", "answers.jsonl", "scored.jsonl", "scored.jsonl.manifest.json", "rationale.jsonl",
                          "deterministic.json", "grades.json", "run.json"})
        scored = {r["query_id"]: r for r in load_jsonl(out / "scored.jsonl")}
        self.assertEqual(sorted(scored), sorted(r["id"] for r in ROWS if r["expected"]["path"] == "model"))
        self.assertTrue(scored["bio-correct"]["judge_pass"])
        self.assertFalse(scored["bio-partial"]["judge_pass"])
        self.assertEqual(scored["bio-empty"]["turn_scores"][0]["judge"],
                         {"pass": False, "reason": "couldn't grade: the model didn't explain this grade", "graded": False})
        self.assertEqual((scored["bio-partial"]["points_earned"], scored["bio-partial"]["points_possible"]), (2, 4.0))
        report = sps.compare([list(scored.values())], [list(scored.values())], kind="both")
        self.assertEqual(report["paired"], 8)
        manifest = json.loads((out / "scored.jsonl.manifest.json").read_text())
        self.assertEqual(manifest["settings"]["grader"]["model"], "fake-9b")
        self.assertFalse(manifest["settings"]["frozen"])
        exact = json.loads((out / "deterministic.json").read_text())
        self.assertEqual((exact["recorded"], exact["mismatches"]), (3, ["chem-ms-partial"]))
        with self.assertRaises(SystemExit):
            g.score("test-v1", self.root / "grades.json", out, self.registry)

    def test_grades_must_match_the_set_and_cover_it(self):
        grades = self.root / "grades.json"
        document = grades_doc(self.rows_path, self.bank_path)
        g.write_json(grades, {**document, "rows_sha256": "0" * 64})
        with self.assertRaises(SystemExit):
            g.score("test-v1", grades, self.root / "a", self.registry)
        g.write_json(grades, {**document, "grades": [x for x in document["grades"] if x["row_id"] != "bio-wrong"]})
        with self.assertRaises(SystemExit):
            g.score("test-v1", grades, self.root / "b", self.registry)
        self.assertFalse((self.root / "b").exists())

    def test_rationale_checks_read_the_raw_output(self):
        out = self.scored()
        checks = {r["query_id"]: r for r in load_jsonl(out / "rationale.jsonl")}
        self.assertTrue(checks["bio-partial"]["passed"])
        self.assertEqual(checks["bio-partial"]["quotes_verbatim"], 1)
        self.assertEqual(checks["bio-wrong"]["checked_on"], "raw")
        self.assertEqual(checks["bio-wrong"]["problems"], ["your_words not verbatim: 'it ruins the enzyme'",
                                                           "points disagree with a verdict"])
        self.assertTrue(checks["bio-copy"]["copies_reference"])
        self.assertIn("full_credit_includes reproduces the reference answer", checks["bio-copy"]["problems"])
        self.assertIsNone(checks["chem-paraphrase"]["copies_reference"])
        scored = {r["query_id"]: r for r in load_jsonl(out / "scored.jsonl")}
        self.assertFalse(scored["bio-wrong"]["mechanical_checks_passed"])
        self.assertTrue(scored["bio-partial"]["mechanical_checks_passed"])


class RationaleChecks(unittest.TestCase):
    def check(self, why, answer="It binds the active site.", earned=2, possible=4, reference=None):
        return g.rationale_checks(why, answer, SOURCES, reference, earned, possible)

    def test_spans_are_verbatim_after_whitespace(self):
        why = rationale("s", criterion("c1", "met", 2, words=["binds   the\nactive site"]),
                        criterion("c2", "missed", 0, missing="m"))
        self.assertEqual(self.check(why)["spans_verbatim"], 1)
        self.assertFalse(self.check(rationale("s", criterion("c1", "met", 2, words=["Binds the active site"]),
                                              criterion("c2", "missed", 0, missing="m")))["passed"])

    def test_points_follow_the_verdict(self):
        for verdict, points, ok in (("met", 2, True), ("met", 1, False), ("missed", 0, True), ("missed", 1, False),
                                    ("partly_met", 1, True), ("partly_met", 2, False), ("partly_met", 0, False)):
            why = rationale("s", criterion("c1", verdict, points, missing="" if points == 2 else "m"))
            self.assertEqual(self.check(why, earned=points, possible=2)["consistent"] == 1, ok, (verdict, points))
        uncertain = rationale("s", criterion("c1", "uncertain", None))
        self.assertEqual(self.check(uncertain, earned=None, possible=2)["criteria"], 0)
        self.assertIn("unknown verdict", "; ".join(self.check(rationale("s", criterion("c1", "great", 2)),
                                                              earned=2, possible=2)["problems"]))

    def test_no_reason_no_wrong(self):
        silent = rationale("", criterion("c1", "missed", 0), criterion("c2", "missed", 0))
        result = self.check(silent, earned=0)
        self.assertTrue(result["unexplained"])
        self.assertIn("lost points without what_was_missing", result["problems"])
        self.assertFalse(self.check(rationale("", criterion("c1", "met", 2)), earned=2, possible=2)["unexplained"])
        self.assertIn("criterion points do not add up to the grade",
                      self.check(rationale("s", criterion("c1", "met", 2)), earned=1, possible=2)["problems"])

    def test_full_credit_names_points_without_copying(self):
        reference = EXPLAIN["reference_answer"]
        self.assertEqual(g.reference_containment(reference.upper(), reference), 1.0)
        self.assertLess(g.reference_containment("Says that Km rises and Vmax is unchanged.", reference), g.COPY_SHARE)
        self.assertIsNone(g.reference_containment("anything", ""))


class Labels(Fixture):
    def label_all(self, out, verdicts, points):
        labels = json.loads((out / "calibration-labels.json").read_text())
        for item in labels["items"]:
            item["human_verdict"] = verdicts.get(item["query_id"])
            item["human_points"] = points.get(item["query_id"])
        g.write_json(out / "calibration-labels.json", labels)

    def test_blind_labels_reuse_calibration_sample_and_score(self):
        out = self.scored()
        labels = g.label(out)
        self.assertEqual(labels["schema"], sps.LABELS_SCHEMA)
        self.assertEqual(labels["frame"]["unanswered"]["population"], 1)  # bio-empty couldn't be graded
        self.assertEqual(len(labels["items"]), 7)
        first = labels["items"][0]
        self.assertEqual(set(first), {"query_id", "turns", "human_verdict", "human_note", "max_points", "human_points"})
        self.assertNotIn("judge", json.dumps(first))
        with self.assertRaises(SystemExit):
            g.label(out)
        human = {"bio-correct": "pass", "bio-partial": "fail", "bio-wrong": "fail", "bio-copy": "fail",
                 "bio-contest": "pass", "chem-paraphrase": "pass", "chem-contest": "fail"}
        self.label_all(out, human, {"bio-correct": 4, "bio-partial": 2, "bio-wrong": 0, "bio-copy": 1,
                                    "bio-contest": 4, "chem-paraphrase": 1, "chem-contest": 1})
        report = sps.calibration_score(load_jsonl(out / "scored.jsonl"), json.loads((out / "calibration-labels.json")
                                                                                   .read_text()))
        self.assertEqual(report["labelled"], 7)
        self.assertEqual(report["judge_accuracy"]["estimate"], 6 / 7)
        self.assertEqual(report["false_fail_rate"]["estimate"], 1 / 5)
        self.assertEqual(report["false_pass_rate"]["estimate"], 0.0)
        summary = g.report(out)
        self.assertEqual(summary["calibration"]["judge_accuracy"]["estimate"], 6 / 7)
        self.assertEqual(summary["points"]["labelled"], 7)
        self.assertAlmostEqual(summary["points"]["mean_abs_points"], (0 + 0 + 0 + 1 + 2 + 0 + 1) / 7)
        self.assertEqual(summary["points"]["labels_inconsistent"], ["chem-contest"])  # fail with full points

    def test_reasons_come_after_the_blind_verdicts_and_survive_a_redraw(self):
        out = self.scored()
        with self.assertRaises(SystemExit):
            g.reasons(out)
        g.label(out)
        self.label_all(out, {"bio-partial": "fail"}, {"bio-partial": 2})
        document = g.reasons(out)
        self.assertEqual([r["query_id"] for r in document["items"]], ["bio-partial"])
        self.assertEqual(document["waiting_for_blind_labels"], 6)
        self.assertIn("  What was missing: The effect on Km and Vmax.", document["items"][0]["rationale"])
        document["items"][0]["human_reason"] = "right"
        g.write_json(out / "reason-labels.json", document)
        self.label_all(out, {"bio-partial": "fail", "bio-wrong": "fail", "bio-copy": "fail", "bio-correct": "pass"},
                       {})
        redrawn = {r["query_id"]: r for r in g.reasons(out)["items"]}
        self.assertEqual(sorted(redrawn), ["bio-copy", "bio-partial", "bio-wrong"])
        self.assertEqual(redrawn["bio-partial"]["human_reason"], "right")
        redrawn["bio-wrong"]["human_reason"] = "reason_fabricated"
        redrawn["bio-copy"]["human_reason"] = "verdict_right_reason_wrong"
        g.write_json(out / "reason-labels.json", {**g.read_json(out / "reason-labels.json"),
                                                  "items": list(redrawn.values())})
        quality = g.reason_quality(out)
        self.assertEqual((quality["labelled"], quality["accepted_share"], quality["fails"]), (3, 1 / 3, True))
        redrawn["bio-wrong"]["human_reason"] = "wrong word"
        g.write_json(out / "reason-labels.json", {"items": list(redrawn.values())})
        with self.assertRaises(SystemExit):
            g.reason_quality(out)


class Contests(Fixture):
    def test_stability_across_seeds(self):
        out = self.scored()
        contest = g.contest_stability(out)
        self.assertEqual((contest["rows"], contest["regrades"], contest["flips"]), (2, 5, 1))
        self.assertEqual(contest["flip_rate"], 0.2)
        self.assertFalse(contest["stable"])
        self.assertEqual(contest["rows_with_split_seeds"], 1)
        self.assertEqual(contest["rows_short_of_seeds"], ["chem-contest"])
        self.assertEqual(contest["responses_to_note"], 3 / 5)
        human = {"items": [{"query_id": "bio-contest", "human_verdict": "pass"}]}
        self.assertEqual(g.contest_stability(out, human)["flips_toward_human"], 1)

    def test_a_steady_grader_is_stable(self):
        steady = [{**grade("chem-contest", 0, 1, NOT_EQUIVALENT), "pass": "regrade", "seed": 2}]
        out = self.scored(steady)
        contest = g.contest_stability(out)
        self.assertEqual(contest["rows_short_of_seeds"], [])
        bio = json.loads((out / "grades.json").read_text())
        for entry in bio["grades"]:
            if entry["row_id"] == "bio-contest" and entry["pass"] == "regrade":
                entry["points_earned"] = 2
        g.write_json(out / "grades.json", bio)
        self.assertEqual((g.contest_stability(out)["flips"], g.contest_stability(out)["stable"]), (0, True))


class Report(Fixture):
    def test_summary_before_and_after_labels(self):
        out = self.scored()
        summary = g.report(out)
        self.assertIsNone(summary["calibration"])
        self.assertEqual(summary["couldnt_grade"], 1)
        self.assertEqual(summary["rationale"]["unexplained_markdowns"], 0)
        self.assertEqual(summary["rationale"]["copies_reference"]["rows"], 1)
        page = (out / "summary.md").read_text()
        self.assertIn("# Quiz grading: test-v1 (draft set: not frozen, not quotable)", page)
        self.assertIn("1. **Agreement with a person**: not labelled yet", page)
        self.assertIn("5. **Contest stability**: 1/5 regrades changed the verdict (20.0%, limit 10%): unstable", page)
        self.assertIn("6. **Deterministic rows**: 3 of 7 graded by the app, 1 differ", page)

    def test_labels_from_another_run_of_the_set(self):
        baseline = self.scored()
        g.label(baseline)
        labels = json.loads((baseline / "calibration-labels.json").read_text())
        for item in labels["items"]:
            item["human_verdict"], item["human_points"] = "fail", 0
        g.write_json(baseline / "calibration-labels.json", labels)
        labels["scored_sha256"] = g.sha256_file(baseline / "scored.jsonl")
        candidate = self.root / "results" / "2026-10-24-test-v1"
        g.score("test-v1", self.root / "grades.json", candidate, self.registry)
        summary = g.report(candidate, baseline / "calibration-labels.json")
        self.assertEqual(summary["calibration"]["labelled"], 7)
        (baseline / "scored.jsonl").write_text("")
        with self.assertRaises(SystemExit):
            g.report(candidate, baseline / "calibration-labels.json")


class Personas(Fixture):
    def writer(self, answers):
        calls = []

        def call(prompt):
            calls.append(prompt)
            item = next(i for i in BANK["items"] if f"Question: {i['prompt']}\n" in prompt[1]["content"])
            persona = next(p for p, brief in personas.BRIEFS.items() if brief in prompt[1]["content"])
            return answers.get((item["id"], persona), {"answer": cq.key_text(item).split(" | ")[0],
                                                       "note": "it is right" if persona == "contesting" else ""})
        return call, calls

    def test_drafts_follow_the_format_and_skip_what_cannot_be_used(self):
        call, calls = self.writer({("chem.sa", "contesting"): {"answer": "2", "note": "it is 2"},
                                   ("bio.sc", "wrong"): {"answer": "no idea", "note": ""}})
        out = self.root / "draft.jsonl"
        drafted, skipped = personas.generate("test-v1", call, "fake-writer", out, items={"bio.expl", "chem.sa", "bio.sc"},
                                             registry=self.registry)
        ids = [r["id"] for r in drafted]
        self.assertIn("bio.expl-empty-m1", ids)
        self.assertEqual(next(r for r in drafted if r["id"] == "bio.expl-empty-m1")["response"], {"text": ""})
        self.assertEqual(dict(skipped), {"chem.sa-contesting-m1": "a contest needs a model-graded answer and a note",
                                         "bio.sc-wrong-m1": "unreadable answer 'no idea'"})
        self.assertEqual(len(calls), len(drafted) + len(skipped) - sum(r["persona"] == "empty" for r in drafted))
        self.assertTrue(all(r["reviewed"] is False and r["author"] == "model:fake-writer" for r in drafted))
        self.assertEqual(next(r for r in drafted if r["id"] == "chem.sa-correct-m1")["expected"],
                         {"path": "deterministic", "points": 1.0})
        self.assertEqual(personas.personas_for(SINGLE), ["correct", "wrong", "empty"])
        with self.assertRaises(SystemExit):
            personas.generate("test-v1", call, "fake-writer", out, registry=self.registry)

    def test_merge_takes_only_reviewed_rows(self):
        call, _ = self.writer({})
        draft = self.root / "draft.jsonl"
        drafted, _ = personas.generate("test-v1", call, "w", draft, items={"hist.order"}, registry=self.registry)
        drafted[0]["reviewed"] = True
        drafted[0]["expected"] = {"path": "deterministic", "points": 0.0}  # a stale expectation is recomputed
        draft.write_text("".join(json.dumps(r) + "\n" for r in drafted))
        self.assertEqual(personas.merge(draft, "test-v1", self.registry), (1, len(drafted) - 1))
        self.assertEqual(g.validate(self.registry), {"test-v1": []})
        self.assertEqual(load_jsonl(self.rows_path)[-1]["id"], drafted[0]["id"])
        bad = [{**drafted[1], "reviewed": True, "persona": "angry"}]
        draft.write_text(json.dumps(bad[0]) + "\n")
        with self.assertRaises(SystemExit):
            personas.merge(draft, "test-v1", self.registry)


if __name__ == "__main__":
    unittest.main()
