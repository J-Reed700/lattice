import unittest
from pathlib import Path

import long_chat_eval as lce

ROOT = Path(__file__).resolve().parent.parent
DATASET_PATH = ROOT / "evals" / "conversation-memory" / "long-chats-v1.json"


def probe(kind="early_fact", source_turns=(1,), must_mention=(("9107",),), **extra):
    base = {
        "kind": kind, "source_turns": list(source_turns),
        "must_mention": [list(g) for g in must_mention], "must_not_claim": [],
        "abstain": kind == "absent", "by_hand": False, "notes": "The port is 9107.",
    }
    base.update(extra)
    return base


def dataset(turns=None):
    turns = turns or [
        {"ask": "The proxy listens on port 9107."},
        {"ask": "Unrelated: what is a mutex?"},
        {"ask": "Which port does the proxy use?", "probe": probe()},
        {"ask": "What was the hotel's name?",
         "probe": probe("absent", (), (), must_not_claim=["Grand Plaza"])},
    ]
    return {"name": "t", "description": "d", "items": [{"id": "one", "tags": ["t"], "turns": turns}]}


def turn_row(index, answer, error=None, calls=None):
    return {"index": index, "question": "q", "answer": answer, "error": error, "answered_ms": 5,
            "model_calls": calls if calls is not None else [{"seq": index, "purpose": "answer"}],
            "prompt_tokens": {}}


def run_row(turns, error=None):
    return {"query_id": "one", "tags": ["t"], "model": "m", "error": error, "turns": turns}


def by_turn(records):
    return {r["turn"]: r for r in records}


class Validate(unittest.TestCase):
    def test_sound_dataset_is_accepted(self):
        self.assertEqual(lce.validate_dataset(dataset()), [])

    def test_group_missing_from_source_asks_is_reported(self):
        data = dataset()
        data["items"][0]["turns"][2]["probe"]["must_mention"] = [["4455"]]
        problems = lce.validate_dataset(data)
        self.assertTrue(any("one turn 3" in p and "4455" in p for p in problems), problems)

    def test_answer_restated_before_the_probe_is_reported(self):
        data = dataset()
        data["items"][0]["turns"][1]["ask"] = "Remind me later: 9107 is the port, right?"
        problems = lce.validate_dataset(data)
        self.assertTrue(any("restated" in p and "turn 3" in p for p in problems), problems)

    def test_bad_source_turns_and_absent_with_must_mention_are_reported(self):
        data = dataset()
        turns = data["items"][0]["turns"]
        turns[2]["probe"]["source_turns"] = [3]
        turns[3]["probe"]["must_mention"] = [["Grand Plaza"]]
        problems = lce.validate_dataset(data)
        self.assertTrue(any("turn 3" in p and "source turn 3" in p for p in problems), problems)
        self.assertTrue(any("turn 4" in p and "abstaining" in p for p in problems), problems)

    def test_shipped_dataset_is_sound(self):
        if not DATASET_PATH.exists():
            self.skipTest("long-chats-v1.json is not present")
        import json
        data = json.loads(DATASET_PATH.read_text(encoding="utf-8"))
        self.assertEqual(lce.validate_dataset(data), [])


class Score(unittest.TestCase):
    def test_answer_with_the_fact_passes_and_without_it_fails(self):
        turns = [turn_row(0, "ok"), turn_row(1, "ok"), turn_row(2, "Port 9107."),
                 turn_row(3, "You never told me the hotel's name.")]
        records = by_turn(lce.score_run(dataset(), [run_row(turns)], {}))
        self.assertTrue(records[3]["passed"])
        self.assertTrue(records[4]["passed"])
        self.assertEqual(records[3]["depth"], 2)
        self.assertIsNone(records[4]["depth"])

        turns[2] = turn_row(2, "Port 8080.")
        failed = by_turn(lce.score_run(dataset(), [run_row(turns)], {}))[3]
        self.assertFalse(failed["passed"])
        self.assertEqual(failed["missing"], [["9107"]])

    def test_stopped_row_leaves_later_probes_unreached(self):
        row = run_row([turn_row(0, "ok"), turn_row(1, None, error="boom")], error="boom")
        records = by_turn(lce.score_run(dataset(), [row], {}))
        for number in (3, 4):
            self.assertFalse(records[number]["reached"])
            self.assertEqual(records[number]["error"], "boom")
            self.assertIsNone(records[number]["passed"])
            self.assertIsNone(records[number]["answer"])

    def test_missing_row_is_not_reached(self):
        records = lce.score_run(dataset(), [], {})
        self.assertTrue(all(not r["reached"] and r["error"] == "no row" for r in records))


class Attribution(unittest.TestCase):
    def rows(self):
        turns = [turn_row(0, "ok"), turn_row(1, "ok"), turn_row(2, "Port 9107."),
                 turn_row(3, "You never told me the hotel's name.")]
        return [run_row(turns)]

    def test_found_in_lists_the_holding_message_with_its_label(self):
        request = {"messages": [
            {"role": "system", "content": "policy"},
            {"role": "user", "content": "[recorded requirement]\nproxy_pass http://127.0.0.1:9107;\n"
                                        "[current message]\nWhich port?"},
        ]}
        records = by_turn(lce.score_run(dataset(), self.rows(), {("one", 2): request}))
        record = records[3]
        self.assertEqual(record["answer_call_seq"], 2)
        self.assertEqual(record["prompt_messages"], 2)
        self.assertEqual(record["found_in"], [{"group": ["9107"], "messages": [
            {"index": 1, "role": "user", "label": "[recorded requirement]"}]}])

    def test_list_content_parts_are_searched(self):
        request = {"messages": [
            {"role": "user", "content": [{"type": "image_url", "image_url": {}},
                                         {"type": "text", "text": "The proxy listens on port 9107."}]},
        ]}
        record = by_turn(lce.score_run(dataset(), self.rows(), {("one", 2): request}))[3]
        messages = record["found_in"][0]["messages"]
        self.assertEqual([m["index"] for m in messages], [0])
        self.assertEqual(messages[0]["label"], "The proxy listens on port 9107.")

    def test_without_calls_attribution_is_null(self):
        record = by_turn(lce.score_run(dataset(), self.rows(), {}))[3]
        self.assertIsNone(record["found_in"])
        self.assertIsNone(record["prompt_messages"])


class Report(unittest.TestCase):
    def test_report_lists_kinds_and_failed_probes(self):
        turns = [turn_row(0, "ok"), turn_row(1, "ok"), turn_row(2, "Port 8080."),
                 turn_row(3, "You never told me the hotel's name.")]
        rows = [run_row(turns)]
        text = lce.report(lce.score_run(dataset(), rows, {}), rows)
        self.assertIn("model: m", text)
        self.assertIn("early_fact: 0/1 (by hand: 0, not reached: 0)", text)
        self.assertIn("absent: 1/1 (by hand: 0, not reached: 0)", text)
        self.assertIn("one T3 early_fact", text)
        self.assertIn("Port 8080.", text)


if __name__ == "__main__":
    unittest.main()
