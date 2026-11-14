import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import lesson_trace as trace  # noqa: E402

D, R = "\x1b[2m", "\x1b[0m"
HEAD = D + "2026-11-13T01:45:05.669928Z" + R + " \x1b[32m INFO" + R + " "
LESSON_SPAN = (
    "\x1b[1mlesson" + R + "\x1b[1m{" + R + "\x1b[3mlesson" + R + D + "=" + R
    + '"5e21c107-7fdc"' + "\x1b[1m}" + R + D + ":" + R + " "
)
COURSE_SPAN = (
    "\x1b[1mcourse" + R + "\x1b[1m{" + R + "\x1b[3mcourse" + R + D + "=" + R
    + '"c-1"' + "\x1b[1m}" + R + D + ":" + R + " "
)
WHERE = (
    D + trace.TARGET + R + D + ":" + R + " " + D
    + "crates/lattice-learning/src/lessons/trace.rs" + R + D + ":" + R + D + "215:" + R + " "
)


def field(key, value):
    return " \x1b[3m" + key + R + D + "=" + R + value


CHANGED = (
    HEAD + LESSON_SPAN + WHERE + "changed"
    + field("stage", '"teaching_repair"')
    + field("cause_findings", '["key/question-3"]')
    + field("cause_claims", "[]")
    + field("removed", '["The \\"old\\" way.\\nSecond line."]')
    + field("added", '["The new way.", "Caf\\u{e9} \\\\ done."]')
)
REFUSED = (
    HEAD + LESSON_SPAN + WHERE + "refused"
    + field("stage", '"claim_repair"')
    + field("subject", '"the correction of \\"Rust is fast.\\""')
    + field("reason", '"it changes a checked sentence"')
    + field("proposed_removed", '["Rust is fast."]')
    + field("proposed_added", '["Rust is very fast."]')
)
COURSE_DECIDED = (
    HEAD + COURSE_SPAN + WHERE + "decided"
    + field("stage", '"course_setup"')
    + field("subject", '"page A (s1)"')
    + field("verdict", '"left out of retrieval"')
    + field("why", '"a login page"')
)
PUBLISHED = (
    HEAD + LESSON_SPAN + WHERE + "published"
    + field("stage", '"publishing"')
    + field("subject", '"lesson"')
    + field("verdict", '"published"')
    + field("sections", "5")
    + field("questions", "4")
    + field("unverified", "0")
)
OTHER = HEAD + D + "lattice_learning::model_call" + R + ": other line"


class ParseTests(unittest.TestCase):
    def test_a_line_with_ansi_parses(self):
        record = trace.parse_line(CHANGED)
        self.assertEqual(record["time"], "2026-11-13T01:45:05.669928Z")
        self.assertEqual(record["kind"], "changed")
        self.assertEqual(record["spans"], {"lesson": {"lesson": "5e21c107-7fdc"}})
        self.assertEqual(record["fields"]["stage"], "teaching_repair")
        self.assertEqual(record["fields"]["cause_findings"], ["key/question-3"])
        self.assertEqual(record["fields"]["cause_claims"], [])

    def test_the_same_line_without_ansi_parses_the_same(self):
        coloured = trace.parse_line(CHANGED)
        plain = trace.parse_line(trace.strip_ansi(CHANGED))
        self.assertEqual(plain["fields"], coloured["fields"])
        self.assertEqual(plain["spans"], coloured["spans"])

    def test_a_non_trace_line_and_an_empty_line_give_none(self):
        self.assertIsNone(trace.parse_line(OTHER))
        self.assertIsNone(trace.parse_line(""))

    def test_debug_escapes_decode(self):
        fields = trace.parse_line(CHANGED)["fields"]
        self.assertEqual(fields["removed"], ['The "old" way.\nSecond line.'])
        self.assertEqual(fields["added"], ["The new way.", "Caf\u00e9 \\ done."])

    def test_a_refused_subject_keeps_its_quotes(self):
        record = trace.parse_line(REFUSED)
        self.assertEqual(record["kind"], "refused")
        self.assertEqual(record["fields"]["subject"], 'the correction of "Rust is fast."')

    def test_a_course_span_gives_the_course_owner(self):
        record = trace.parse_line(COURSE_DECIDED)
        self.assertEqual(trace.owner_of(record), "course c-1")
        self.assertEqual(record["fields"]["verdict"], "left out of retrieval")

    def test_a_published_record_keeps_its_counts(self):
        fields = trace.parse_line(PUBLISHED)["fields"]
        self.assertEqual(fields["sections"], "5")
        self.assertEqual(fields["questions"], "4")
        self.assertEqual(fields["unverified"], "0")


class TimelineTests(unittest.TestCase):
    def test_records_group_by_owner_in_first_seen_order(self):
        lines = trace.timeline([CHANGED, OTHER, REFUSED, COURSE_DECIDED, PUBLISHED])
        text = "\n".join(lines)
        self.assertLess(
            text.index("== lesson 5e21c107-7fdc (3 records) =="),
            text.index("== course c-1 (1 records) =="),
        )
        self.assertIn('      - The "old" way.\nSecond line.', text)
        self.assertIn("      + The new way.", text)

    def test_the_substring_filter_keeps_matching_owners(self):
        lines = trace.timeline([CHANGED, COURSE_DECIDED], "c-1")
        self.assertTrue(lines[0].startswith("== course c-1"))
        self.assertFalse(any("lesson 5e21" in line for line in lines))

    def test_a_filter_with_no_match_gives_nothing(self):
        self.assertEqual(trace.timeline([CHANGED], "nomatch"), [])


if __name__ == "__main__":
    unittest.main()
