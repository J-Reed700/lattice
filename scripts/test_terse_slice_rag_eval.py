import unittest

import terse_slice as ts

SLICE = ts.ROOT / "evals/retrieval/synthetic-library-v3-terse.json"


def tiny():
    source = {
        "documents": [{"id": "a", "text": "The boiler restart procedure"}, {"id": "b", "text": "Bread recipe"}],
        "queries": [
            {"id": "q1", "category": "c", "text": "how is the boiler restart procedure done today please",
             "relevance": {"a": 3}},
            {"id": "q2", "category": "c", "text": "bread", "relevance": {"b": 3}},
            {"id": "q3", "category": "same-language", "text": "Brot", "relevance": {"b": 3}},
            {"id": "q4", "category": "c", "text": "nothing here", "relevance": {}},
        ],
    }
    terse = {"name": "t", "description": "d", "length_reference": {"word_counts": [2, 3, 4, 50, 60]},
             "queries": [{"source_id": "q1", "text": "heater reset how"}, {"source_id": "q2", "text": "loaf"}]}
    return terse, source


class Slice(unittest.TestCase):
    def test_reference_is_the_short_half(self):
        terse, _ = tiny()
        self.assertEqual(ts.reference_lengths(terse), [2, 3, 4])

    def test_targets_follow_the_reference_and_keep_order(self):
        terse, source = tiny()
        targets = ts.length_targets(terse, source)
        self.assertEqual(set(targets), {"q1", "q2"})
        self.assertLessEqual(targets["q2"], targets["q1"])
        self.assertTrue(set(targets.values()) <= {2, 3, 4})

    def test_overlap_weighs_shared_words(self):
        _, source = tiny()
        idf = ts.idf_table(source)
        self.assertEqual(ts.keyword_overlap("boiler restart", ["The boiler restart procedure"], idf), 1.0)
        self.assertEqual(ts.keyword_overlap("heater reset", ["The boiler restart procedure"], idf), 0.0)

    def test_check_flags_gaps_copies_and_strays(self):
        terse, source = tiny()
        self.assertEqual(ts.check(terse, source)["problems"], [])
        terse["queries"] = [{"source_id": "q1", "text": "How is the boiler restart procedure done today please"},
                            {"source_id": "q3", "text": "bread?"}]
        problems = ts.check(terse, source)["problems"]
        self.assertTrue(any("identical" in p for p in problems))
        self.assertTrue(any("q3" in p for p in problems))
        self.assertTrue(any("q2" in p and "no terse" in p for p in problems))

    def test_build_keeps_the_source_judgements(self):
        terse, source = tiny()
        built = ts.build(terse, source)
        self.assertEqual([(q["id"], q["text"], q["relevance"]) for q in built["queries"]],
                         [("q1", "heater reset how", {"a": 3}), ("q2", "loaf", {"b": 3})])
        self.assertEqual(built["documents"], source["documents"])

    def test_compare_pairs_each_query(self):
        terse, source = tiny()
        row = lambda qid, ranked: {"query_id": qid, "ranked_ids": ranked, "latency_ms": 1}
        v3 = [row("q1", ["a"]), row("q2", ["b"]), row("q4", [])]
        worse = [row("q1", ["b"]), row("q2", ["b"])]
        report = ts.compare(terse, source, v3, worse)
        self.assertEqual(report["recall_changes"], {"better": 0, "worse": 1, "same": 1})
        self.assertEqual(report["lost"], ["q1"])
        self.assertEqual(report["verdict"], "can't tell from noise")
        self.assertEqual((report["v3"]["recall_at_k"], report["terse"]["recall_at_k"]), (1.0, 0.5))

    def test_committed_slice_is_sound(self):
        terse, source = ts.load(SLICE)
        report = ts.check(terse, source)
        self.assertEqual(report["problems"], [])
        self.assertLessEqual(report["mean_abs_words_off_target"], 1)
        self.assertLess(report["keyword_overlap"]["terse"], report["keyword_overlap"]["v3"])


if __name__ == "__main__":
    unittest.main()
