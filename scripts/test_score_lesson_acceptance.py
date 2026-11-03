"""The acceptance scorer over a small synthetic database: what it counts, what it fails, and that it names no text."""
import json
import os
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import score_lesson_acceptance as sla  # noqa: E402

LESSON = "f36400bc-0000-0000-0000-000000000000"
PROGRAM = "program-1"
JOB = "job-1"
SCHEMA = """
CREATE TABLE learning_lessons (id TEXT PRIMARY KEY, program_id TEXT, preparation TEXT);
CREATE TABLE learning_lesson_reviews (lesson_id TEXT PRIMARY KEY, program_id TEXT, job_id TEXT, state TEXT, checker_model TEXT);
CREATE TABLE learning_lesson_verifications (lesson_id TEXT PRIMARY KEY, program_id TEXT, report_json TEXT);
CREATE TABLE learning_lesson_claims (claim_id TEXT PRIMARY KEY, lesson_id TEXT, unit_index INTEGER, field TEXT, quote TEXT,
                                     statement TEXT, state_json TEXT);
CREATE TABLE learning_lesson_claim_events (claim_id TEXT, ordinal INTEGER, payload_json TEXT);
CREATE TABLE jobs (id TEXT PRIMARY KEY, status TEXT, activity_json TEXT, started_at INTEGER, finished_at INTEGER);
CREATE TABLE job_checkpoints (job_id TEXT, key TEXT, data_json TEXT);
CREATE TABLE learning_blocks (lesson_id TEXT, ordinal INTEGER, source_ids_json TEXT);
CREATE TABLE learning_source_library (id TEXT, program_id TEXT, deleted_at INTEGER, active_version_id TEXT, created_at INTEGER);
CREATE TABLE learning_source_versions (id TEXT, program_id TEXT, title TEXT, not_a_source TEXT, admission_policy TEXT,
                                       word_count INTEGER, resolved_url TEXT, requested_url TEXT, full_text TEXT);
"""
STATEMENT = "SECRET statement {}."
PAGE = "PAGE TEXT SENTINEL"


def classified(cls, by):
    return {"kind": "classified", "class": cls, "by": by}


def checked(result, who="local", model="qwen-27b"):
    return {"kind": "checked", "checker": {who: {"model": model}}, "result": result, "reason": "r",
            "fingerprint": "f", "evidence": [], "window_tokens": 1}


def repair(outcome, attempt=1):
    return {"kind": "repair_attempted", "attempt": attempt, "escalated": attempt == 2, "before": "b", "after": None,
            "outcome": outcome}


SUPPORTED = {"supported": {"source_version_id": "v1", "supporting_quote": "q"}}
FAILED = {"failed": {"kind": "unsupported"}}
CONTRADICTED = {"failed": {"kind": "contradicted"}}
UNREADABLE = {"failed": {"kind": "could_not_judge"}}
BY_EVIDENCE = {"state": "resolved_by_evidence", "source_version_id": "v1", "supporting_quote": "q"}
REMOVED = {"state": "removed", "at": 1, "by": "machine"}

# (id, unit, class, authority, state, checks and repairs in order): six statements, two of each class.
CLAIMS = [
    ("a", 0, "sourced", "writer", BY_EVIDENCE, [checked(SUPPORTED)]),
    ("b", 1, "sourced", "rule", REMOVED,
     [checked(FAILED), repair("rewritten"), checked(FAILED), repair("removed", 2)]),
    ("c", 0, "reasoning", "model", {"state": "consistent", "at": 1}, [checked("consistent")]),
    ("d", 2, "reasoning", "model", REMOVED, [checked(CONTRADICTED), repair("removed")]),
    ("e", 0, "common", "model", {"state": "general_knowledge", "at": 1}, [checked("known_common")]),
    ("f", 2, "common", "model", {"state": "general_knowledge", "at": 1}, [checked(SUPPORTED)]),
]


def build(path, claims=CLAIMS, preparation="ready", review="ready", ceiling=80, made=50, rises=0, minutes=20,
          block_sources=("v1",), lesson=LESSON, statements=None):
    db = sqlite3.connect(path)
    db.executescript(SCHEMA)
    db.execute("INSERT INTO learning_lessons VALUES (?,?,?)", (lesson, PROGRAM, preparation))
    db.execute("INSERT INTO learning_lesson_reviews VALUES (?,?,?,?,?)", (lesson, PROGRAM, JOB, review, "qwen-27b"))
    findings = [{"statement": STATEMENT.format(c[0]), "class": {"class": c[2], "by": c[3]}, "evidence": []}
                for c in claims if c[4]["state"] != "removed"]
    db.execute("INSERT INTO learning_lesson_verifications VALUES (?,?,?)",
               (lesson, PROGRAM, json.dumps({"checker_model": "qwen-27b", "findings": findings})))
    for claim_id, unit, cls, by, state, events in claims:
        db.execute("INSERT INTO learning_lesson_claims VALUES (?,?,?,?,?,?,?)",
                   (claim_id, lesson, unit, "body", f"quote {claim_id}", STATEMENT.format(claim_id), json.dumps(state)))
        for ordinal, event in enumerate([classified(cls, by)] + events):
            db.execute("INSERT INTO learning_lesson_claim_events VALUES (?,?,?)",
                       (claim_id, ordinal, json.dumps({"event": event, "appVersion": "1", "checkerModel": "qwen-27b"})))
    activity = {"statements": statements or {"sourced": 2, "reasoning": 2, "common": 2}, "modelCalls": made,
                "modelCallsCeiling": ceiling, "modelCallsCeilingRises": rises, "modelName": "qwen-27b"}
    db.execute("INSERT INTO jobs VALUES (?,?,?,?,?)", (JOB, "completed", json.dumps(activity), 0, minutes * 60000))
    db.execute("INSERT INTO job_checkpoints VALUES (?,?,?)",
               (JOB, "claim-ledger-v1:cuts", json.dumps({"taken": {"1": {}}, "going": {}, "mended": {"1": {}}})))
    for ordinal, version in enumerate(block_sources):
        db.execute("INSERT INTO learning_blocks VALUES (?,?,?)", (lesson, ordinal, json.dumps([version])))
    pages = [("v1", "Guide", None, "p", 500), ("v2", "Changelog", None, "p", 120), ("v3", "Sign in", "login page, 65 words", "p", 65)]
    for number, (version, title, refused, policy, words) in enumerate(pages):
        db.execute("INSERT INTO learning_source_library VALUES (?,?,?,?,?)", (f"s{number}", PROGRAM, None, version, number))
        db.execute("INSERT INTO learning_source_versions VALUES (?,?,?,?,?,?,?,?,?)",
                   (version, PROGRAM, title, refused, policy, words, f"https://example.org/{version}", None, PAGE))
    db.commit()
    db.close()


class ScoreLessonAcceptance(unittest.TestCase):
    def setUp(self):
        handle, self.path = tempfile.mkstemp(suffix=".db")
        os.close(handle)
        os.remove(self.path)
        self.addCleanup(self.remove)

    def remove(self):
        if os.path.exists(self.path):
            os.remove(self.path)

    def failures(self, **kwargs):
        self.remove()
        build(self.path, **kwargs)
        return sla.score(self.path)["failures"]

    def test_a_run_that_ends_as_the_design_says_passes_and_the_terms_are_read_from_the_ledger(self):
        build(self.path)
        result = sla.score(self.path)

        self.assertEqual(result["failures"], [])
        self.assertEqual(result["end_state"]["preparation"], "ready")
        self.assertEqual(result["end_state"]["statements"], {"ledger_rows": 6, "live": 6, "limit": 73})
        self.assertEqual(result["calls"]["terms"],
                         {"S": 2, "R": 2, "C": 2, "F": 1, "G": 1, "K": 1, "V": 0, "D": 1, "M": 0, "U": 0})
        self.assertEqual((result["calls"]["made"], result["calls"]["ceiling_at_the_end"]), (50, 80))
        self.assertEqual(result["calls"]["recorded_in_the_ledger"],
                         {"checks of common statements": 2, "checks of reasoning statements": 2,
                          "checks of sourced statements": 3, "rewrites": 1})
        self.assertEqual(result["common_statements_sent_to_a_source_check"], ["f"])
        self.assertEqual([page["title"] for page in result["gate"]["refused"]], ["Sign in"])
        self.assertEqual(result["gate"]["refused"][0]["reason"], "login page, 65 words")
        self.assertEqual([page["title"] for page in result["gate"]["admitted_under"]], ["Changelog"])
        self.assertEqual(result["gate"]["citing_a_refused_page"], [])
        self.assertEqual(result["models"]["checks"], {"local check: qwen-27b": 7})

    def test_what_it_prints_names_no_page_and_no_statement_text(self):
        build(self.path)
        text = sla.render(sla.score(self.path))

        self.assertNotIn("SECRET", text)
        self.assertNotIn(PAGE, text)
        self.assertIn("PASSED", text)
        self.assertIn("Sign in", text)

    def test_the_ledger_must_end_in_the_states_each_class_allows(self):
        queued = ("g", 3, "sourced", "writer", {"state": "queued", "kind": "unsupported", "reason": "r"}, [checked(FAILED)])
        internal = ("h", 3, "internal", "rule", {"state": "consistent", "at": 1}, [])
        crossed = ("i", 3, "reasoning", "model", BY_EVIDENCE, [checked(SUPPORTED)])
        failures = self.failures(claims=CLAIMS + [queued, internal, crossed])

        self.assertTrue(any("claim g" in f and "queued" in f for f in failures), failures)
        self.assertTrue(any("claim h" in f and "internal" in f for f in failures), failures)
        self.assertTrue(any("claim i" in f and "reasoning ended 'resolved_by_evidence'" in f for f in failures), failures)

    def test_a_removal_by_the_user_is_not_the_machines_to_claim(self):
        by_you = ("g", 3, "sourced", "writer", {"state": "removed", "at": 1, "by": "user"}, [])
        failures = self.failures(claims=CLAIMS + [by_you])

        self.assertTrue(any("claim g" in f for f in failures), failures)

    def test_a_lesson_not_ready_and_a_review_not_ready_fail(self):
        failures = self.failures(preparation="outline", review="needs_review")

        self.assertTrue(any("not 'ready'" in f and "lesson" in f for f in failures), failures)
        self.assertTrue(any("review" in f for f in failures), failures)

    def test_calls_past_the_ceiling_and_a_ceiling_that_went_up_fail(self):
        failures = self.failures(made=100, ceiling=80, rises=2)

        self.assertTrue(any("100 calls made, more than the 80" in f for f in failures), failures)
        self.assertTrue(any("went up 2 time(s)" in f for f in failures), failures)

    def test_how_long_a_run_took_is_reported_and_not_failed(self):
        self.assertEqual(self.failures(minutes=137), [])

        made = sla.score(self.path)["calls"]

        self.assertEqual((made["minutes"], made["seconds_per_call"]), (137.0, 164.4))

    def test_a_sentence_that_cites_a_page_the_gate_refused_fails(self):
        failures = self.failures(block_sources=("v1", "v3"))

        self.assertTrue(any("cite a page the admission gate refused" in f for f in failures), failures)

    def test_more_statements_than_the_lesson_had_fail(self):
        many = [(f"x{n}", 0, "sourced", "writer", BY_EVIDENCE, [checked(SUPPORTED)]) for n in range(sla.STATEMENT_LIMIT + 1)]

        self.assertTrue(any("more than 73" in f for f in self.failures(claims=many)))

    def test_the_statement_bound_is_the_one_lesson_the_design_counted(self):
        many = [(f"x{n}", 0, "sourced", "writer", BY_EVIDENCE, [checked(SUPPORTED)]) for n in range(sla.STATEMENT_LIMIT + 1)]
        other = "0a1b2c3d-0000-0000-0000-000000000000"
        build(self.path, claims=many, lesson=other)

        result = sla.score(self.path, other[:8])

        self.assertEqual(result["failures"], [])
        self.assertIsNone(result["end_state"]["statements"]["limit"])

    def test_statements_that_share_a_sentence_are_counted_for_their_rechecks(self):
        a = ("a", 0, "sourced", "writer", BY_EVIDENCE, [checked(SUPPORTED)])
        b = ("b", 0, "reasoning", "model", {"state": "consistent", "at": 1}, [checked("consistent")])
        build(self.path, claims=[a, b])
        db = sqlite3.connect(self.path)
        db.execute("UPDATE learning_lesson_claims SET quote = 'one sentence'")
        db.commit()
        db.close()

        made = sla.score(self.path)["calls"]

        # Each re-reads twice from its post-rewrite node, the re-ask still possible (8 calls), and takes the
        # narrowing call.
        self.assertEqual(made["terms"]["M"], (2 * 8 + 1) + (2 * 8 + 1))
        self.assertEqual(made["bound_with_every_term_at_most"], 1 + 94 + 34 + 1)

    def test_the_recheck_of_a_class_is_two_readings_from_its_post_rewrite_node_with_the_re_ask_possible_and_the_narrowing_call(self):
        self.assertEqual(sla.AFTER_REWRITE_UNSPENT, {"sourced": 8, "reasoning": 8, "common": 7, "internal": 7})
        self.assertEqual(sla.RECHECK, {"sourced": 17, "reasoning": 17, "common": 15, "internal": 15})
        self.assertEqual(sla.ENTRY, {"sourced": 16, "reasoning": 15, "common": 16, "internal": 13})

    def test_the_bound_with_every_term_at_most_is_the_sorting_each_longest_path_the_rechecks_and_a_flow_repair_a_section(self):
        build(self.path)

        made = sla.score(self.path)["calls"]

        # One sorting call for six statements; two of each class at the longest path of its graph from its entry
        # (2 * 16 + 2 * 15 + 2 * 16); no statement shares a sentence; the one section's flow repair.
        self.assertEqual(made["sort_calls"], 1)
        self.assertEqual(made["terms"]["M"], 0)
        self.assertEqual(made["bound_with_every_term_at_most"], 1 + (2 * 16 + 2 * 15 + 2 * 16) + 0 + 1)

    def test_a_statement_the_model_calls_internal_adds_the_longest_path_of_its_graph(self):
        statements = {"sourced": 1, "reasoning": 1, "common": 1, "internal": 1}
        build(self.path, statements=statements)

        made = sla.score(self.path)["calls"]

        self.assertEqual(made["bound_with_every_term_at_most"], 1 + (16 + 15 + 16 + 13) + 0 + 1)

    def test_the_planted_statement_is_reported_with_its_class_and_how_it_ended(self):
        planted = ("p", 4, "sourced", "rule", REMOVED, [checked(CONTRADICTED), repair("removed")])
        build(self.path, claims=CLAIMS + [planted])
        db = sqlite3.connect(self.path)
        db.execute("UPDATE learning_lesson_claims SET statement = ? WHERE claim_id = 'p'", (sla.PLANTED,))
        db.commit()
        db.close()

        found = sla.score(self.path)["planted_statement"]

        self.assertEqual((found["in_the_ledger"], found["in_the_lesson"]), (True, False))
        self.assertEqual((found["class"], found["ended"]), ("sourced", "removed"))
        self.assertEqual(sla.planted([], {}), {"in_the_ledger": False, "in_the_lesson": False})

    def test_a_class_is_the_strongest_authority_that_decided_it(self):
        events = [classified("sourced", "writer"), classified("reasoning", "model"), classified("common", "rule")]
        row = {"claim_id": "z", "unit_index": 0, "field": "body", "quote": "q", "statement": "s", "state_json": "{}"}

        self.assertEqual(sla.Claim(row, events).cls, "sourced")

    def test_one_authority_deciding_again_never_lightens_a_class(self):
        events = [classified("sourced", "model"), classified("reasoning", "model"), classified("common", "model")]
        row = {"claim_id": "z", "unit_index": 0, "field": "body", "quote": "q", "statement": "s", "state_json": "{}"}

        self.assertEqual(sla.Claim(row, events).cls, "sourced")
        self.assertEqual(sla.Claim(row, events[::-1]).cls, "sourced")

    def test_a_lesson_the_database_lacks_is_a_failure_not_a_crash(self):
        build(self.path)

        self.assertEqual(sla.score(self.path, "deadbeef")["failures"], ["no lesson starts with 'deadbeef'"])


if __name__ == "__main__":
    unittest.main()
