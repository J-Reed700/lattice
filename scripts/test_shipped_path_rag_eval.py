import json
import os
import unittest
from unittest import mock
from pathlib import Path

import shipped_path_eval as spe

ROOT = Path(__file__).resolve().parent.parent


def turn(answer, citation_map=None, evidence=None, **extra):
    return {
        "index": 0, "question": "q", "answer": answer, "error": None, "answered_ms": 10,
        "evidence": evidence if evidence is not None else [{"document_id": "a"}],
        "citation_map": citation_map if citation_map is not None else {"1": "a"},
        "ranked_ids": ["a"], "model_calls": [], "prompt_tokens": {"cached": 0, "evaluated": 5, "completion": 3},
        **extra,
    }


def row(query_id, turns):
    return {
        "schema": spe.ROW_SCHEMA, "query_id": query_id, "tags": ["t"],
        "answer": turns[-1]["answer"], "ranked_ids": ["a"], "citation_map": {"1": "a"},
        "latency_ms": 10, "model": "m", "error": None, "turns": turns,
    }


def rubric(**fields):
    base = {"must_mention": [["14 days", "fourteen days"]], "must_not_claim": [],
            "expected_citations": ["a"], "abstain": False, "notes": "Fourteen days."}
    base.update(fields)
    return base


QUESTIONS = {
    "tags": {"t": "a tag", "u": "another"},
    "items": [
        {"id": "one", "tags": ["t"], "turns": [{"ask": "How long?", "rubric": rubric()}]},
        {"id": "two", "tags": ["t", "u"], "turns": [
            {"ask": "How long?", "rubric": rubric()},
            {"ask": "And the phone?", "rubric": rubric(must_mention=[], expected_citations=[], abstain=True)},
        ]},
    ],
}


class MechanicalChecks(unittest.TestCase):
    def test_any_alternative_satisfies_a_group_regardless_of_case_and_emphasis(self):
        checks = spe.mechanical(rubric(), turn("It is **Fourteen Days** [1]."))
        self.assertEqual(checks["missing_facts"], [])
        self.assertTrue(checks["passed"])

    def test_unicode_hyphen_and_spacing_still_match_an_identifier(self):
        self.assertTrue(spe.contains("Quote SIG‑84122  please", "SIG-84122"))

    def test_missing_fact_fails(self):
        checks = spe.mechanical(rubric(), turn("It is a fortnight [1]."))
        self.assertEqual(checks["missing_facts"], [["14 days", "fourteen days"]])
        self.assertFalse(checks["passed"])

    def test_forbidden_claim_fails(self):
        checks = spe.mechanical(rubric(must_not_claim=["42 hours"]), turn("14 days, not 42 hours [1]."))
        self.assertEqual(checks["forbidden_claims"], ["42 hours"])
        self.assertFalse(checks["passed"])

    def test_invented_citation_number_fails(self):
        checks = spe.mechanical(rubric(), turn("14 days [1][3]."))
        self.assertEqual(checks["invalid_citations"], [3])
        self.assertFalse(checks["passed"])

    def test_expected_document_must_be_cited_and_is_traced_to_evidence(self):
        checks = spe.mechanical(rubric(expected_citations=["a", "b"]),
                                turn("14 days [1].", evidence=[{"document_id": "a"}]))
        self.assertEqual(checks["expected_cited"], ["a"])
        self.assertEqual(checks["expected_in_evidence"], ["a"])
        self.assertFalse(checks["passed"])

    def test_abstention_passes_only_where_the_corpus_is_silent(self):
        silent = rubric(must_mention=[], expected_citations=[], abstain=True)
        declined = turn("That is not recorded in your documents.", citation_map={})
        self.assertTrue(spe.mechanical(silent, declined)["passed"])
        self.assertFalse(spe.mechanical(rubric(), declined)["passed"])
        self.assertFalse(spe.mechanical(silent, turn("It is 555-0100."))["passed"])

    def test_incidental_missing_information_does_not_turn_a_conclusion_into_abstention(self):
        answer = (
            "The applicable policy requires 14 days [1]. "
            "A separate schedule is not provided [2]. "
            "Therefore the applicable period is 14 days [1]."
        )
        self.assertFalse(spe.abstained(answer))

    def test_a_negative_premise_followed_by_a_cited_correction_is_not_abstention(self):
        answer = "There is no mention of using the prior release; instead use v5.7.3 [1]."
        self.assertFalse(spe.abstained(answer))

    def test_a_global_or_concluding_refusal_is_still_abstention(self):
        long_decline = (
            "The answer is not available in the provided documents. "
            + "Several unrelated records are listed. " * 40
        )
        self.assertTrue(spe.abstained(long_decline))
        self.assertTrue(spe.abstained("The records discuss timing, but it cannot be determined."))

    def test_a_caveat_after_the_required_facts_does_not_fail_an_answer(self):
        answer = (
            "Fourteen days [1]. Two caveats follow. "
            "The schedule does not state a separate period for contractors."
        )
        self.assertTrue(spe.abstained(answer))
        self.assertTrue(spe.mechanical(rubric(), turn(answer))["passed"])

    def test_a_caveat_does_not_save_an_answer_missing_its_facts(self):
        answer = "It varies [1]. The schedule does not state a separate period."
        self.assertFalse(spe.mechanical(rubric(), turn(answer))["passed"])

    def test_a_refusal_about_the_question_counts_wherever_the_answer_says_it(self):
        silent = rubric(must_mention=[], expected_citations=[], abstain=True)
        for question, answer in (
            ("What does the Marbleau undertaking oblige?",
             "I searched the library. No passage in the documents mentions the Marbleau undertaking."),
            ("How much did the boiler repair cost?",
             "The cost of the boiler repair is not stated in the excerpts searched."),
            ("Which runbook covers ERR-0001?",
             "The provided excerpts do not contain any runbook covering ERR-0001."),
            ("What was the unit price on order PO-00001?",
             "I searched both, but there is no purchase order numbered PO-00001."),
            ("What's her mobile number?",
             "I found no phone number for her in the passages."),
            ("What's her mobile number?",
             "The pager goes to the bench [1]. No mobile number for her appears in the passages."),
        ):
            declined = turn(answer)
            declined["question"] = question
            self.assertTrue(spe.mechanical(silent, declined)["passed"], answer)

    def test_a_guess_with_an_aside_about_something_else_is_no_refusal(self):
        silent = rubric(must_mention=[], expected_citations=[], abstain=True)
        guess = turn("Her mobile is 555-0100. The schedule is not stated. Call after nine.")
        guess["question"] = "What's her mobile number?"
        self.assertTrue(spe.says_unavailable(guess["answer"]))
        self.assertFalse(spe.mechanical(silent, guess)["passed"])

    def test_a_failed_turn_never_passes(self):
        failed = turn("14 days [1].")
        failed["error"] = "timed out"
        self.assertFalse(spe.mechanical(rubric(), failed)["passed"])


class RowShape(unittest.TestCase):
    def test_runner_row_shape_is_accepted(self):
        self.assertEqual(spe.validate_row(row("one", [turn("x")]))["query_id"], "one")

    def test_missing_or_mistyped_fields_are_rejected(self):
        broken = row("one", [turn("x")])
        del broken["turns"][0]["evidence"]
        with self.assertRaises(ValueError):
            spe.validate_row(broken)
        wrong = row("one", [turn("x")])
        wrong["latency_ms"] = "10"
        with self.assertRaises(ValueError):
            spe.validate_row(wrong)
        other = row("one", [turn("x")])
        other["schema"] = "something-else"
        with self.assertRaises(ValueError):
            spe.validate_row(other)


class Judge(unittest.TestCase):
    def test_request_is_greedy_with_thinking_off_and_no_reasoning_effort(self):
        body = spe.judge_request("m", [])
        self.assertEqual((body["temperature"], body["top_k"]), (0, 1))
        self.assertEqual(body["chat_template_kwargs"], {"enable_thinking": False})
        self.assertNotIn("reasoning_effort", body)

    def test_verdict_parsing(self):
        payload = {"choices": [{"message": {"content": '{"verdict": "pass", "reason": "ok"}'}}]}
        self.assertEqual(spe.parse_verdict(payload), (True, "ok"))
        bad = {"choices": [{"message": {"content": '{"verdict": "maybe"}'}}]}
        with self.assertRaises(ValueError):
            spe.parse_verdict(bad)

    def test_follow_up_prompt_carries_earlier_turns_as_context(self):
        messages = spe.judge_messages(QUESTIONS["items"][1], 1, ["Fourteen days.", "I don't know."])
        user = messages[1]["content"]
        self.assertIn("Q1: How long?", user)
        self.assertIn("A1: Fourteen days.", user)
        self.assertIn("do not contain this answer", user)

    def test_scoring_grades_each_answered_turn_once(self):
        calls = []

        def judge(messages):
            calls.append(messages)
            return (len(calls) != 2, "graded")

        rows = [row("one", [turn("14 days [1].")]),
                row("two", [turn("14 days [1]."), turn("Not recorded in the documents.", citation_map={})])]
        scored = spe.score(QUESTIONS, rows, judge)
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(r["mechanical_checks_passed"] for r in scored))
        self.assertTrue(scored[0]["judge_pass"])
        self.assertFalse(scored[1]["judge_pass"])
        self.assertEqual(scored[1]["turn_scores"][0]["judge"], {"pass": False, "reason": "graded", "graded": True})
        summary = spe.summarize(scored, QUESTIONS)
        self.assertEqual(summary["overall"]["judge"]["passed"], 1)
        self.assertEqual(summary["by_tag"]["u"]["items"], 1)
        self.assertEqual(len(summary["disagreements"]), 1)

    def test_an_unanswered_turn_fails_without_a_judge_call(self):
        calls = []
        scored = spe.score(QUESTIONS, [row("one", [turn(None)])], lambda m: calls.append(m) or (True, ""))
        self.assertEqual(calls, [])
        self.assertFalse(scored[0]["judge_pass"])

    def test_identity_says_when_the_judge_cannot_be_listed(self):
        identity = spe.judge_identity("http://127.0.0.1:9", "judge", "answerer.gguf", timeout=1)
        self.assertIn("unavailable", identity["served_models"])
        self.assertIsNone(identity["judges_own_answers"])
        self.assertEqual(identity["answer_model"], "answerer.gguf")

    def test_rubric_lines_are_what_the_judge_reads(self):
        messages = spe.judge_messages(QUESTIONS["items"][0], 0, ["x"])
        for line in spe.rubric_lines(QUESTIONS["items"][0]["turns"][0]["rubric"]):
            self.assertIn(line, messages[1]["content"])

    def test_rows_for_unknown_items_are_rejected(self):
        with self.assertRaises(ValueError):
            spe.score(QUESTIONS, [row("three", [turn("x")])])


class JudgeAuth(unittest.TestCase):
    def test_no_key_sends_only_the_content_type(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(spe.judge_headers(), {"Content-Type": "application/json"})

    def test_a_key_is_sent_under_its_own_header_name(self):
        with mock.patch.dict(os.environ, {spe.JUDGE_AUTH_ENV: "Authorization: Bearer abc:def"}):
            self.assertEqual(spe.judge_headers()["Authorization"], "Bearer abc:def")

    def test_a_key_without_a_header_name_is_refused(self):
        for value in ("Bearer abc", ": abc", "Authorization:"):
            with mock.patch.dict(os.environ, {spe.JUDGE_AUTH_ENV: value}):
                with self.assertRaises(SystemExit):
                    spe.judge_headers()


class AnsweringModel(unittest.TestCase):
    """A scored run names the model that wrote its answers, local or cloud."""

    def test_a_cloud_run_is_named_by_its_model_and_what_served_it(self):
        meta = {"answering_model": {"kind": "cloud", "model": "claude-opus-5-5",
                                    "served_as": ["claude-opus-5-5"]}}
        self.assertEqual(spe.answering_model(meta), ("claude-opus-5-5", "claude-opus-5-5"))

    def test_an_older_local_run_is_named_by_its_server_and_weights(self):
        meta = {"recipe": {"server": {"model": "Qwen3.5-9B-Q4_K_M.gguf"}}, "gguf_sha256": "abc"}
        self.assertEqual(spe.answering_model(meta), ("Qwen3.5-9B-Q4_K_M.gguf", "abc"))

    def test_an_anthropic_judge_sends_no_sampling_and_binds_the_verdict(self):
        request = spe.anthropic_judge_request("claude-opus-5-5", [
            {"role": "system", "content": "Grade."}, {"role": "user", "content": "Turn."}])
        for knob in ("temperature", "top_p", "top_k"):
            self.assertNotIn(knob, request)
        self.assertEqual(request["system"], "Grade.")
        self.assertEqual(request["messages"], [{"role": "user", "content": "Turn."}])
        self.assertFalse(request["output_config"]["format"]["schema"]["additionalProperties"])
        payload = {"content": [{"type": "text", "text": '{"verdict": "pass", "reason": "ok"}'}]}
        self.assertEqual(spe.parse_anthropic_verdict(payload), (True, "ok"))


class LabelledDataset(unittest.TestCase):
    def test_dataset_rows_are_graded_by_the_answer_harness(self):
        dataset = {"documents": [{"id": "a", "text": "Kept for 14 days."}, {"id": "b", "text": "Other."}],
                   "queries": [{"id": "one", "text": "How long?", "relevance": {"a": 3}}]}
        expectations = {"queries": [{"id": "one", "required_patterns": [["14 days"]],
                                     "required_citation_docs": ["a"]}]}
        good = row("one", [turn("Kept for 14 days [1].")])
        scored = spe.score_dataset(dataset, expectations, [good])
        self.assertTrue(scored[0]["citations_aligned"])
        self.assertTrue(scored[0]["mechanical_checks_passed"])
        shuffled = row("one", [turn("Kept for 14 days [2].")])
        shuffled["citation_map"] = {"1": "b", "2": "a"}
        shuffled["ranked_ids"] = ["a", "b"]
        scored = spe.score_dataset(dataset, expectations, [shuffled])
        self.assertFalse(scored[0]["citations_aligned"])
        self.assertEqual(scored[0]["cited_document_ids"], ["a"])


class QuestionSet(unittest.TestCase):
    def test_rubric_facts_must_occur_in_the_expected_documents(self):
        corpus = {"documents": [{"id": "a", "text": "Kept for 14 days."}]}
        self.assertEqual(spe.validate_questions(QUESTIONS, corpus), [])
        wrong = json.loads(json.dumps(QUESTIONS))
        wrong["items"][0]["turns"][0]["rubric"]["must_mention"] = [["21 days"]]
        self.assertTrue(spe.validate_questions(wrong, corpus))

    def test_shipped_question_set_is_valid_against_v3(self):
        questions = json.loads((ROOT / "evals/retrieval/desktop-path-v1.json").read_text())
        corpus = json.loads((ROOT / "evals/retrieval/synthetic-library-v3.json").read_text())
        self.assertEqual(spe.validate_questions(questions, corpus), [])
        items = questions["items"]
        self.assertTrue(20 <= len(items) <= 50)
        self.assertGreaterEqual(sum(len(item["turns"]) > 1 for item in items), 5)
        for tag in ("multi-hop", "planner-rewrite", "corrective-retry", "opening-or-section",
                    "abstain", "conflict", "numeric-precision", "follow-up-memory",
                    "pronoun-follow-up", "ellipsis-follow-up", "topic-shift", "cross-turn-evidence",
                    "long-document"):
            self.assertTrue(any(tag in item["tags"] for item in items), tag)


if __name__ == "__main__":
    unittest.main()
