"""The hollow-review replay counts a reply as verify.rs does, and can still run against the shipped prompt."""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_memory_review_ab as ab
import replay_memory_review_hollow as replay

ROOT = Path(__file__).resolve().parent.parent
PROMPTS = ROOT / "crates/lattice-desktop/src/features/conversation/memory/prompts.rs"
VERIFY = ROOT / "crates/lattice-desktop/src/features/conversation/memory/verify.rs"


class ReasonTests(unittest.TestCase):
    def test_a_reason_is_what_verify_rs_calls_one(self):
        for hollow in ("", "   ", "x", "placeholder", "unused"):
            self.assertFalse(replay.gives_a_reason(hollow), hollow)
        for reason in ("The quoted passage says this.", "two words", "文档中明确说明了这一点"):
            self.assertTrue(replay.gives_a_reason(reason), reason)

    def test_a_reply_gives_only_the_ids_it_was_asked_with_a_reason(self):
        raw = json.dumps({"verdicts": [
            {"id": "addition:c1", "verdict": "supported", "reason": ""},
            {"id": "addition:c2", "verdict": "unsupported", "reason": "The passage is about something else."},
            {"id": "addition:c9", "verdict": "supported", "reason": "This id was never asked about."},
        ]})
        self.assertEqual(list(replay.read_reply(raw, {"addition:c1", "addition:c2"})), ["addition:c2"])
        self.assertEqual(replay.read_reply("not json", {"addition:c1"}), {})

    def test_a_keyed_reply_is_read_by_its_keys(self):
        raw = json.dumps({"verdicts": {"addition:c1": {"verdict": "supported", "reason": "The passage says so."}}})
        self.assertEqual(list(replay.read_reply(raw, {"addition:c1"})), ["addition:c1"])


class RequestTests(unittest.TestCase):
    def test_the_sentence_arm_finds_its_place_in_the_shipped_prompt(self):
        system = ab.verifier_system(PROMPTS)
        self.assertNotIn(replay.SENTENCE, system)
        self.assertIn(replay.SENTENCE, replay.with_sentence(system))

    def test_a_request_can_be_narrowed_to_the_reviews_left_unread(self):
        user = ab.payload(ab.SCENARIOS[0], ab.QUESTIONS)
        self.assertEqual({review["id"] for review in json.loads(user)["reviews"]}, set(replay.IDS))
        narrowed = json.loads(replay.only(user, {"transition:c1"}))
        self.assertEqual([review["id"] for review in narrowed["reviews"]], ["transition:c1"])

    def test_the_keyed_schema_requires_every_id(self):
        schema = replay.keyed_schema()["properties"]["verdicts"]
        self.assertEqual(schema["required"], replay.IDS)

    def test_every_scenario_has_a_verdict_it_needs(self):
        self.assertEqual({scenario["name"] for scenario in ab.SCENARIOS}, set(replay.NEED))

    def test_asks_per_batch_match_the_app(self):
        source = VERIFY.read_text("utf-8")
        found = re.search(r"const ASKS_PER_BATCH: usize = (\d+);", source)
        if found is None:
            self.skipTest("this checkout predates asking again")
        self.assertEqual(replay.ASKS, int(found.group(1)))


if __name__ == "__main__":
    unittest.main()
