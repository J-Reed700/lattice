"""The floors are chosen from the rows by the rule, and rows that name no embedding model, or two, are refused."""
import json
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import analyse_memory_floors as floors  # noqa: E402

MODEL = "sha256-0f0f+prefix-qwen3-instruct"


def recall_row(probe, kind, candidate, relevant, share=None, similarity=None, exact=False, found=True, model=MODEL):
    return {
        "probe": probe, "kind": kind, "gate": "recall", "candidate": candidate, "relevant": relevant,
        "found": found, "lexical_share": share, "shares_word": None, "similarity": similarity,
        "exact_identifier": exact, "embedding_model": model,
    }


def shared_row(probe, kind, candidate, relevant, shares_word=False, similarity=None, model=MODEL):
    return {
        "probe": probe, "kind": kind, "gate": "shared", "candidate": candidate, "relevant": relevant,
        "found": None, "lexical_share": None, "shares_word": shares_word, "similarity": similarity,
        "exact_identifier": False, "embedding_model": model,
    }


def loaded(*rows):
    return floors.load_rows(json.dumps(row) for row in rows)


def unrelated_items(similarities, shares_word=()):
    """One unrelated pool item per similarity; the first len(shares_word) of them share a word as flagged."""
    return [
        shared_row("p", "shared_unrelated", f"i{n}", False, shares_word=n < len(shares_word) and shares_word[n], similarity=s)
        for n, s in enumerate(similarities)
    ]


class TheGates(unittest.TestCase):
    def test_the_recall_gate_lets_through_on_any_one_signal(self):
        gate = floors.lets_through_recall
        row = recall_row("p", "paraphrase", "m1", False)
        self.assertFalse(gate(row, 0.25, 0.6))
        self.assertTrue(gate(dict(row, exact_identifier=True), 0.25, 0.6))
        self.assertTrue(gate(dict(row, lexical_share=0.25), 0.25, 0.6))
        self.assertFalse(gate(dict(row, lexical_share=0.2499), 0.25, 0.6))
        self.assertTrue(gate(dict(row, similarity=0.6), 0.25, 0.6))
        self.assertFalse(gate(dict(row, similarity=0.59), 0.25, 0.6))

    def test_a_candidate_the_search_did_not_return_is_never_recalled(self):
        row = recall_row("p", "paraphrase", "m1", False, share=2.0, similarity=0.9, exact=True, found=False)
        self.assertFalse(floors.lets_through_recall(row, 0.0, 0.0))

    def test_recall_has_no_score_where_the_code_has_none(self):
        row = recall_row("p", "paraphrase", "m1", False)
        self.assertFalse(floors.lets_through_recall(row, 0.0, 0.0))

    def test_the_shared_gate_counts_an_unscored_item_as_zero(self):
        row = shared_row("p", "shared_unrelated", "i", False)
        self.assertTrue(floors.offers_shared(row, 0.0))
        self.assertFalse(floors.offers_shared(row, 0.1))
        self.assertTrue(floors.offers_shared(dict(row, shares_word=True), float("inf")))

    def test_rows_are_read_as_the_32_bit_floats_the_code_compared(self):
        row = loaded(shared_row("p", "shared_bearing", "i", True, similarity=0.1))[0]
        self.assertEqual(row["similarity"], struct.unpack("f", struct.pack("f", 0.1))[0])
        self.assertNotEqual(row["similarity"], 0.1)


class TheRows(unittest.TestCase):
    def test_rows_must_name_one_embedding_model(self):
        one = loaded(shared_row("a", "shared_bearing", "x", True, model="m1"), shared_row("b", "shared_bearing", "y", True, model="m1"))
        self.assertEqual(floors.embedding_model_of(one), "m1")
        two = loaded(shared_row("a", "shared_bearing", "x", True, model="m1"), shared_row("b", "shared_bearing", "y", True, model="m2"))
        with self.assertRaises(SystemExit):
            floors.embedding_model_of(two)
        with self.assertRaises(SystemExit):
            floors.embedding_model_of([{"probe": "a"}])

    def test_a_row_that_lacks_a_signal_is_refused(self):
        row = shared_row("p", "shared_bearing", "i", True)
        del row["shares_word"]
        with self.assertRaises(SystemExit):
            floors.load_rows([json.dumps(row)])
        with self.assertRaises(SystemExit):
            floors.load_rows([""])


class TheSelection(unittest.TestCase):
    def test_a_rate_is_the_share_of_each_label_let_through(self):
        rows = loaded(
            recall_row("p", "paraphrase", "a", True, share=0.5),
            recall_row("p", "paraphrase", "b", True),
            recall_row("p", "paraphrase", "c", False, share=0.3),
            recall_row("p", "paraphrase", "d", False),
        )
        t = floors.tally(rows, floors.recall_gate(0.4, 0.6))
        self.assertEqual((t.relevant, t.kept, t.irrelevant, t.let_through), (2, 1, 2, 0))
        self.assertEqual((t.kept_share(), t.let_through_share()), (0.5, 0.0))

    def test_a_higher_floor_never_lets_more_through(self):
        shares = [None, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]
        rows = loaded(*[recall_row("p", "distractor", f"m{n}", False, share=s) for n, s in enumerate(shares)])
        counts = [floors.tally(rows, floors.recall_gate(f, 0.6)).let_through for f in floors.grid(1.0, 0.05)]
        self.assertEqual(counts, sorted(counts, reverse=True))
        self.assertEqual(counts[0], 9)
        self.assertEqual(counts[-1], 0)

    def test_the_shared_floor_is_the_lowest_value_offering_at_most_five_percent(self):
        rows = loaded(*unrelated_items([0.9, 0.5] + [0.1] * 18))
        self.assertEqual(floors.choose_shared_floor(rows), (0.501, 0.5))

    def test_the_report_shows_what_rounding_down_does_to_the_cap(self):
        rows = loaded(
            *unrelated_items([0.9, 0.5] + [0.1] * 18),
            shared_row("q", "shared_bearing", "r1", True, similarity=0.8),
            shared_row("q", "shared_bearing", "r2", True, similarity=0.3),
        )
        text = floors.report(rows)
        self.assertIn("Embedding model: " + MODEL, text)
        self.assertIn("is 0.501, rounded down to 0.50", text)
        self.assertIn("at 0.50: 10.0% of unrelated pool items let through (n=20), 50.0% of relevant kept (n=2)", text)
        self.assertIn("Rounding down put 0.50 above the 5% cap; 0.501 meets it.", text)

    def test_no_shared_floor_meets_the_rule_when_shared_words_alone_offer_too_much(self):
        rows = loaded(*unrelated_items([0.0] * 10, shares_word=[True, True, True]))
        self.assertEqual(floors.choose_shared_floor(rows), (None, None))
        text = floors.report(rows)
        self.assertIn("no value up to 1.00 offers at most 5%", text)
        self.assertIn("a shared word alone offers 30.0% (n=10)", text)

    def test_the_recall_floor_counts_only_instruction_and_distractor_candidates(self):
        counted = [recall_row("p", "instruction_only", f"a{n}", False, share=0.3) for n in range(10)]
        counted += [recall_row("q", "distractor", f"b{n}", False, share=0.1) for n in range(10)]
        not_counted = [recall_row("r", "paraphrase", f"c{n}", False, share=0.9) for n in range(10)]
        rows = loaded(*counted, *not_counted)
        self.assertEqual(floors.choose_recall_floor(rows, 0.6), (0.301, 0.3))

    def test_the_recall_floor_is_searched_with_the_meaning_arm_at_the_shared_floor(self):
        rows = loaded(
            *[recall_row("p", "instruction_only", f"a{n}", False, similarity=0.7) for n in range(3)],
            *[recall_row("p", "instruction_only", f"b{n}", False) for n in range(17)],
        )
        self.assertEqual(floors.choose_recall_floor(rows, 0.6), (None, None))
        self.assertEqual(floors.choose_recall_floor(rows, 0.8), (0.0, 0.0))

    def test_rounding_down_never_rounds_up(self):
        for value, expected in [(0.499, 0.49), (0.5, 0.5), (0.47, 0.47), (0.29, 0.29), (0.301, 0.3), (0.0, 0.0)]:
            self.assertEqual(floors.round_down(value), expected, value)

    def test_the_report_covers_both_gates_and_each_kind(self):
        rows = loaded(
            recall_row("e", "exact_restatement", "m1", True, share=0.8),
            recall_row("e", "exact_restatement", "m2", False, share=0.1),
            recall_row("a", "paraphrase", "m1", True, similarity=0.7),
            recall_row("a", "paraphrase", "m2", False),
            recall_row("f", "follow_up", "m1", True, share=0.3),
            recall_row("f", "follow_up", "m2", False),
            recall_row("i", "instruction_only", "m1", False, share=0.3),
            recall_row("d", "distractor", "m1", False),
            shared_row("s", "shared_bearing", "i1", True, similarity=0.7),
            shared_row("s", "shared_bearing", "i2", False, similarity=0.2),
            shared_row("u", "shared_unrelated", "i1", False, similarity=0.1),
        )
        text = floors.report(rows)
        for expected in ["Shared gate:", "Recall gate:", "Selection", "Shipped floors", "exact_restatement", "shared_unrelated"]:
            self.assertIn(expected, text)
        self.assertIn("Rows: 11 (8 recall, 3 shared)", text)


if __name__ == "__main__":
    unittest.main()
