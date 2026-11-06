"""The memory reviewer replay asks the model for what the app asks for."""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_memory_review_ab as replay

PROMPTS = (
    Path(__file__).resolve().parent.parent
    / "crates/lattice-desktop/src/features/conversation/memory/prompts.rs"
)


def shipped_verdict_schema():
    source = PROMPTS.read_text(encoding="utf-8")
    body = re.search(r"pub fn verdict_schema\(\).*?json!\((\{.*?\})\)\)\s*\n\}", source, re.S).group(1)
    return json.loads(body)


class ReviewerSchemaTests(unittest.TestCase):
    def test_the_replay_sends_the_schema_the_app_sends(self):
        self.assertEqual(replay.SCHEMA, shipped_verdict_schema())


if __name__ == "__main__":
    unittest.main()
