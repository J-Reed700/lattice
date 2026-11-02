import copy
import json
import unittest
from pathlib import Path

import replay_session_talk as talk
import tutor_eval

ROOT = Path(__file__).resolve().parents[1]


def load_course():
    return json.loads((ROOT / "evals/tutor/course-v1.json").read_text(encoding="utf-8"))


def load_sessions():
    return json.loads((ROOT / "evals/tutor/sessions-v1.json").read_text(encoding="utf-8"))


def card(question, review_count=0, stability=None, due_now=True, ratings=(), key="c.x", interval_days=0):
    return {
        "key": key,
        "question": question,
        "review_count": review_count,
        "stability": stability,
        "due_now": due_now,
        "ratings": list(ratings),
        "interval_days": interval_days,
    }


def turn(index, answer, cards=(), tools=(), citations=None, verdicts=None, notes=()):
    return {
        "index": index,
        "answer": answer,
        "citation_map": citations or {},
        "tool_calls": list(tools),
        "verification": {"verdictCounts": verdicts or {}},
        "course_after": {"cards": list(cards), "notes": list(notes)},
    }


def row(turns, query_id="s", tags=(), expect=None, model="model-a"):
    return {
        "schema": tutor_eval.ROW_SCHEMA,
        "query_id": query_id,
        "tags": list(tags),
        "turns": list(turns),
        "expect": expect or {},
        "model": model,
        "error": None,
    }


class FixtureTests(unittest.TestCase):
    def test_the_committed_course_and_sessions_are_valid(self):
        course = load_course()
        self.assertEqual(tutor_eval.validate_course(course), [])
        self.assertEqual(tutor_eval.validate_sessions(load_sessions(), course), [])

    def test_a_card_stating_a_number_its_source_lacks_is_refused(self):
        course = load_course()
        course["cards"][0]["answer"] += " It takes 999 seconds."
        self.assertTrue(any("999" in problem for problem in tutor_eval.validate_course(course)))

    def test_cards_from_one_module_are_refused(self):
        course = load_course()
        for item in course["cards"]:
            item["lesson"] = "m1.l1"
        self.assertTrue(any("span modules" in problem for problem in tutor_eval.validate_course(course)))

    def test_a_session_that_opens_on_a_play_turn_is_refused(self):
        sessions = copy.deepcopy(load_sessions())
        sessions["sessions"][0]["turns"].insert(0, {"play": "right"})
        problems = tutor_eval.validate_sessions(sessions, load_course())
        self.assertTrue(any("opens on a play turn" in problem for problem in problems))


class ModelTests(unittest.TestCase):
    def test_a_summary_refuses_rows_with_no_model(self):
        with self.assertRaises(SystemExit):
            tutor_eval.summarize([row([], model=None)], load_course())

    def test_a_summary_refuses_rows_from_two_models(self):
        rows = [row([], "a", model="model-a"), row([], "b", model="model-b")]
        with self.assertRaises(SystemExit):
            tutor_eval.summarize(rows, load_course())

    def test_a_summary_names_the_rows_model(self):
        self.assertEqual(tutor_eval.summarize([row([])], load_course())["model"], "model-a")

    def test_a_summary_says_how_the_course_was_searched(self):
        searched = row([])
        searched["index_mode"] = "hybrid"
        self.assertEqual(tutor_eval.summarize([searched], load_course())["index_modes"], ["hybrid"])


class ScoreTests(unittest.TestCase):
    def test_a_citation_the_turn_does_not_hold_is_invalid(self):
        rows = [row([turn(0, "Leases last 45 seconds [1], renewals 15 minutes [2].", citations={"1": "x"})])]
        summary = tutor_eval.summarize(rows, load_course())
        self.assertEqual(summary["citations"]["invalid"], {"s": [2]})

    def test_the_supported_share_counts_every_verdict(self):
        verdicts = {"supported": 3, "unsupported": 1}
        summary = tutor_eval.summarize([row([turn(0, "x", verdicts=verdicts)])], load_course())
        self.assertEqual(summary["citations"]["supported_share"], 0.75)

    def test_a_check_counts_only_in_the_closing_paragraph_and_within_the_limit(self):
        early = row([turn(0, "Why? Because of leases.\n\nNext, the ladder."), turn(1, "So.\n\nWhat comes next?")],
                    tags=["retrieval-question"])
        late = [turn(index, "Lecture.") for index in range(tutor_eval.MAX_TURNS_TO_A_CHECK)]
        late.append(turn(tutor_eval.MAX_TURNS_TO_A_CHECK, "Done?"))
        scored = tutor_eval.summarize([early, row(late, "late", ["retrieval-question"])], load_course())
        self.assertEqual(scored["reached_a_check"], {"passed": 1, "of": 2})
        self.assertEqual(scored["by_session"][0]["first_check_turn"], 1)

    def test_a_review_counts_as_moved_only_when_the_schedule_moved(self):
        before = [card("q1"), card("q2")]
        after = [
            card("q1", review_count=1, stability=2.5, due_now=False, ratings=["good"], interval_days=3),
            card("q2", review_count=1, stability=None, due_now=True, ratings=["again"]),
        ]
        rows = [row([turn(0, "Ask", before), turn(1, "Rated", after)])]
        summary = tutor_eval.summarize(rows, load_course())
        self.assertEqual(summary["reviews_moved"], {"passed": 1, "of": 2})

    def test_opening_with_due_needs_the_tool_in_the_first_turn_and_modules_are_read_from_its_result(self):
        course = load_course()
        questions = [item["question"] for item in course["cards"]]
        shown = {"name": "due_reviews", "result": f"2 due:\n- {questions[0]}\n- {questions[2]}"}
        opened = row([turn(0, "First card?", tools=[shown])], "opened", ["due-opening"])
        missed = row([turn(0, "Hello."), turn(1, "Card?", tools=[shown])], "missed", ["due-opening"])
        summary = tutor_eval.summarize([opened, missed], course)
        self.assertEqual(summary["opened_with_due"], {"passed": 1, "of": 2})
        self.assertEqual(summary["due_span_modules"], {"passed": 1, "of": 2})
        self.assertEqual(summary["by_session"][0]["modules_due"], ["Delivery", "Ordering"])

    def test_cards_shown_in_the_facts_or_asked_in_their_own_words_open_a_session(self):
        course = load_course()
        questions = [item["question"] for item in course["cards"]]
        shown = turn(0, "Hello.")
        shown["cards_shown"] = [questions[0], questions[4]]
        asked = turn(0, f"First one: {questions[1]}")
        summary = tutor_eval.summarize(
            [row([shown], "shown", ["due-opening"]), row([asked], "asked", ["due-opening"])], course
        )
        self.assertEqual(summary["opened_with_due"], {"passed": 1, "of": 2})
        self.assertEqual(summary["by_session"][0]["modules_due"], ["Delivery", "Operations"])

    def test_a_first_message_that_asks_is_answered_before_the_cards(self):
        answered = row([turn(0, "A lease lasts 45 seconds [1].")], "answered", ["ask-first"])
        deferred = row([turn(0, "First, your cards.")], "deferred", ["ask-first"])
        summary = tutor_eval.summarize([answered, deferred], load_course())
        self.assertEqual(summary["answered_first"], {"passed": 1, "of": 2})

    def test_saying_the_course_does_not_cover_it_counts_only_in_the_turn_that_says_so(self):
        declined = turn(0, "None of the sources mention which cipher jobs use.")
        declined["question"] = "Which cipher do jobs use?"
        guessed = turn(1, "Jobs use AES.")
        guessed["question"] = "Which cipher do jobs use?"
        summary = tutor_eval.summarize([row([declined, guessed], tags=["honesty"])], load_course())
        self.assertEqual(summary["said_not_covered"], {"passed": 1, "of": 2})

    def test_a_refused_tool_call_is_reported_with_its_arguments(self):
        call = {"name": "record_recall", "arguments": '{"card_id":"c1"}', "result": "Error: Invalid input"}
        fine = {"name": "due_reviews", "arguments": "{}", "result": "6 due:"}
        summary = tutor_eval.summarize([row([turn(0, "x", tools=[call, fine])])], load_course())
        self.assertEqual(summary["refused_calls"], {"s": ['record_recall {"card_id":"c1"}']})

    def test_each_expectation_fails_on_what_the_course_holds(self):
        expect = {"reviews_at_least": 1, "cards_added_at_least": 1, "notes_containing": ["holding pen"]}
        empty = row([turn(0, "x")], expect=expect)
        self.assertEqual(len(tutor_eval.summarize([empty], load_course())["expect_failed"]["s"]), 3)
        saved = [card("q", key=None)]
        notes = [{"content": "The Holding Pen keeps it."}]
        rated = [card("q", key=None, review_count=1, stability=1.0, due_now=False, ratings=["good"])]
        full = row([turn(0, "x", saved), turn(1, "y", rated, notes=notes)], expect=expect)
        self.assertEqual(tutor_eval.summarize([full], load_course())["expect_failed"], {})


class SessionTalkTests(unittest.TestCase):
    def run_row(self):
        claims = [
            {"sentence": "Good, next card.", "verdict": "unsupported", "citationIds": []},
            {"sentence": "A lease lasts 45 seconds [1].", "verdict": "unsupported", "citationIds": [1]},
            {"sentence": "Well done, that was the hard one.", "verdict": "supported", "citationIds": []},
            {"sentence": "The pen keeps jobs.", "verdict": "unsupported", "citationIds": []},
        ]
        return {"query_id": "s", "turns": [{"index": 0, "verification": {"claimVerdicts": claims}}, {"index": 1}]}

    def test_a_batch_is_a_turn_s_flagged_uncited_sentences_with_one_false_claim_in_it(self):
        [batch] = list(talk.batches([self.run_row()], ["A lease lasts 90 seconds."]))
        self.assertEqual(batch["sentences"][batch["planted"]], "A lease lasts 90 seconds.")
        kept = [s for i, s in enumerate(batch["sentences"]) if i != batch["planted"]]
        self.assertEqual(kept, ["Good, next card.", "The pen keeps jobs."])

    def test_a_reply_is_read_as_the_app_reads_it(self):
        self.assertEqual(talk.positions("Here: [1, 3]"), {0, 2})
        self.assertEqual(talk.positions("[]"), set())
        self.assertIsNone(talk.positions("I could not tell."))

    def test_the_score_counts_a_dropped_false_claim_and_a_dropped_subject_sentence(self):
        rows = [
            {"sentences": ["False.", "The pen keeps jobs.", "Good."], "planted": 0, "picked": [2]},
            {"sentences": ["False.", "Good."], "planted": 0, "picked": None},
        ]
        totals = talk.score(rows, ["The pen"])
        self.assertEqual(totals["false_claims_dropped"], ["False."])
        self.assertEqual(totals["subject_dropped"], ["The pen keeps jobs."])
        self.assertEqual((totals["talk"], totals["talk_kept"], totals["unreadable"]), (1, 1, 1))

    def test_the_labelled_openings_and_false_claims_are_there(self):
        labels = json.loads((ROOT / "evals/tutor/session-talk-v1.json").read_text(encoding="utf-8"))
        self.assertGreaterEqual(len(labels["false_claims"]), 10)
        self.assertGreaterEqual(len(labels["subject_openings"]), 20)


if __name__ == "__main__":
    unittest.main()
