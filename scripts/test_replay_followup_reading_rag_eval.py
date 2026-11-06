"""The follow-up reading replay sends what the app sends, under each arm, and counts a reply by what it quoted."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_followup_reading_ab as replay

CLOSING = 'Do not answer my question yet. First read these passages from above again, each on its own.\n\n[1] First.\n\n[41] Carried.\n\nWrite one line for each of these passages, starting with its label.'
QUESTION = 'And who owns it "now"?'


class ArmTests(unittest.TestCase):
    def test_the_question_goes_after_the_passages_as_groups_writes_it(self):
        text = replay.restated(CLOSING, QUESTION)
        self.assertIn(f'[41] Carried.\n\nMy question is: "{QUESTION}"\n\nWrite one line', text)

    def test_either_arm_gives_the_same_message_whichever_tree_logged_it(self):
        with_it = replay.restated(CLOSING, QUESTION)
        self.assertEqual(replay.restated(with_it, QUESTION), with_it)
        self.assertEqual(replay.without_question(with_it), CLOSING)
        self.assertEqual(replay.without_question(CLOSING), CLOSING)

    def test_an_arm_does_not_change_the_logged_request(self):
        item = {"request": {"messages": [{"role": "user", "content": CLOSING}]}, "question": QUESTION}
        replay.arm_request(item, "restated")
        self.assertEqual(item["request"]["messages"][0]["content"], CLOSING)
        self.assertIn("My question is", replay.arm_request(item, "restated")["messages"][0]["content"])


class ReadingRequestTests(unittest.TestCase):
    def test_only_a_follow_ups_reading_is_replayed_with_its_own_question(self):
        with tempfile.TemporaryDirectory() as folder:
            run = Path(folder) / "run.jsonl"
            run.write_text(json.dumps({"query_id": "q", "turns": [{"question": "Who owned it?"}, {"question": "And now?"}]}) + "\n", "utf-8")
            reading = {"role": "user", "content": f'My question is about And now?\n\n{CLOSING}'}
            calls = [
                {"query_id": "q", "call": {"seq": 1, "request": {"messages": [reading]}}},
                {"query_id": "q", "call": {"seq": 2, "request": {"messages": [
                    {"role": "user", "content": "Who owned it?"},
                    {"role": "assistant", "content": "Tobin."},
                    reading,
                ]}}},
                {"query_id": "q", "call": {"seq": 3, "request": {"messages": [
                    {"role": "user", "content": "Who owned it?"},
                    {"role": "assistant", "content": "Tobin."},
                    {"role": "user", "content": "And now?"},
                ]}}},
            ]
            Path(f"{run}.calls.jsonl").write_text("".join(json.dumps(call) + "\n" for call in calls), "utf-8")
            items = replay.reading_requests(run)
        self.assertEqual([item["seq"] for item in items], [2])
        self.assertEqual(items[0]["question"], "And now?")


class KindTests(unittest.TestCase):
    def test_a_reply_counts_by_what_it_quoted(self):
        self.assertEqual(replay.kind_of_reply("[1] -\n[2] The date is 2025.\n[41] -\n", 40), "fresh")
        self.assertEqual(replay.kind_of_reply("[1] -\n[2] –\n[41] The owner is Tobin.\n", 40), "only carried")
        self.assertEqual(replay.kind_of_reply("[1] -\n[41] —\n", 40), "nothing")


if __name__ == "__main__":
    unittest.main()
