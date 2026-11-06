"""The memory extractor replay renders and judges what the app renders and what the release baseline requires."""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_memory_extract_ab as replay


class PromptTests(unittest.TestCase):
    def test_the_sentence_can_be_put_in_and_taken_out(self):
        system = replay.extractor_system()
        with_it = replay.with_sentence(system)
        self.assertIn(f"{replay.BEFORE_SENTENCE} {replay.SENTENCE}", with_it)
        self.assertEqual(replay.with_sentence(with_it), with_it)
        self.assertNotIn(replay.SENTENCE, replay.without_sentence(with_it))

    def test_a_payload_ends_with_the_passages_and_the_task(self):
        segments, _ = replay.window("early_restriction", 1)
        payload = json.loads(replay.user_prompt(segments))
        self.assertEqual(list(payload), sorted(payload))
        self.assertEqual(list(payload)[-2:], ["segments", "task"])
        self.assertEqual(payload["processed_segment_ids_must_equal"], [s["segment_id"] for s in payload["segments"]])

    def test_the_schema_asks_for_an_items_evidence_before_its_kind_and_label(self):
        item = replay.SCHEMA["properties"]["add"]["items"]["properties"]
        self.assertEqual(list(item), ["candidate_id", "evidence", "kind", "label"])
        self.assertEqual(list(replay.SCHEMA["properties"]), ["add", "processed_segment_ids", "schema_version", "transitions"])


class WindowTests(unittest.TestCase):
    def test_a_windows_ids_depend_only_on_its_family_and_seed(self):
        first, _ = replay.window("topic_return", 1)
        again, _ = replay.window("topic_return", 1)
        other, _ = replay.window("topic_return", 2)
        self.assertEqual([s["id"] for s in first], [s["id"] for s in again])
        self.assertNotEqual([s["id"] for s in first], [s["id"] for s in other])

    def test_a_long_message_is_cut_into_tiles_that_hold_it_all(self):
        message = {"id": "m", "sequence": 1, "role": "user", "content": "A sentence of the long message. " * 700}
        tiles = replay.segments_of(message)
        self.assertGreater(len(tiles), 1)
        self.assertEqual("".join(t["text"] for t in tiles), message["content"])
        self.assertTrue(all(len(t["text"].encode()) <= replay.SEGMENT_MAX_BYTES for t in tiles))
        self.assertEqual(tiles[1]["before"], tiles[0]["text"][-replay.OVERLAP_CHARS:])

    def test_every_family_exists_and_its_window_holds_what_it_requires_or_is_read_as_empty(self):
        for family, size in replay.FIRST_WINDOW.items():
            segments, _ = replay.window(family, 1)
            self.assertEqual(len(segments), size, family)


class JudgeTests(unittest.TestCase):
    SEGMENTS = [{"text": "Please do not publish anything to the public wiki."}]
    EXPECT = {"required_active_quote_any": [["do not publish anything to the public wiki", "Everything stays private"]]}

    def test_an_empty_answer_hits_nothing(self):
        result = replay.judged('{"add":[]}', self.EXPECT, self.SEGMENTS)
        self.assertEqual((result["added"], result["wanted"], result["hit"]), (0, 1, 0))

    def test_only_the_required_text_the_window_holds_is_wanted(self):
        raw = json.dumps({"add": [{"kind": "constraint", "evidence": [{"quote": "do not publish anything to the public wiki"}]}]})
        result = replay.judged(raw, self.EXPECT, self.SEGMENTS)
        self.assertEqual((result["added"], result["wanted"], result["hit"]), (1, 1, 1))

    def test_an_answer_that_is_not_json_is_unparsed(self):
        self.assertEqual(replay.judged("no", self.EXPECT, self.SEGMENTS), {"unparsed": True})


class AppContractTests(unittest.TestCase):
    def test_the_constants_are_the_apps(self):
        root = Path(__file__).resolve().parent.parent
        segment = (root / "crates/lattice-desktop/src/features/conversation/memory/segment.rs").read_text("utf-8")
        self.assertIn("SEGMENT_MAX_BYTES: usize = 6 * 1024", segment)
        self.assertIn(f"SEGMENT_OVERLAP_CHARS: usize = {replay.OVERLAP_CHARS}", segment)
        self.assertIn(f"MIN_SEGMENT_DIVISOR: usize = {replay.MIN_SEGMENT_DIVISOR}", segment)
        memory = (root / "crates/lattice-desktop/src/features/conversation/memory/mod.rs").read_text("utf-8")
        self.assertRegex(memory, rf"EXTRACTOR_OUTPUT_TOKENS: usize = {replay.OUTPUT_TOKENS:_}".replace("8192", "8_192"))
        self.assertTrue(re.search(r"required_active_quotes", (root / "crates/lattice-desktop/tests/fixtures/conversation_memory/early_restriction.json").read_text("utf-8")))


if __name__ == "__main__":
    unittest.main()
