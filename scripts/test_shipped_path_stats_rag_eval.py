import unittest
from math import comb

import exact_stats as es
import shipped_path_stats as sps


def scored(query_id, judge, mechanical=True, turns=1, graded=True):
    return {"query_id": query_id, "judge_pass": judge, "mechanical_checks_passed": mechanical,
            "answer_sha256": f"answer-{query_id}",
            "turn_scores": [{"turn": n, "judge": {"pass": judge, "reason": "r", "graded": graded}}
                            for n in range(turns)]}


class SignTest(unittest.TestCase):
    def test_known_exact_values(self):
        self.assertEqual(es.sign_test_p(0, 0), 1.0)
        self.assertEqual(es.sign_test_p(6, 0), 2 / 64)
        self.assertEqual(es.sign_test_p(0, 5), 2 / 32)
        self.assertEqual(es.sign_test_p(7, 1), 2 * 9 / 256)
        self.assertEqual(es.sign_test_p(8, 1), 2 * 10 / 512)
        self.assertEqual(es.sign_test_p(3, 3), 1.0)

    def test_thresholds_come_from_the_test(self):
        self.assertEqual(es.fewest_changed_to_call(0.05), 6)
        self.assertEqual(es.fewest_to_call(10, 0.05), 9)
        self.assertIsNone(es.fewest_to_call(5, 0.05))
        self.assertEqual(es.fewest_changed_to_call(0.01), 8)


class HypergeometricInterval(unittest.TestCase):
    def test_edges(self):
        self.assertEqual(es.hypergeometric_interval(5, 10, 10), (5, 5))
        self.assertEqual(es.hypergeometric_interval(0, 0, 20), (0, 20))
        self.assertEqual(es.hypergeometric_interval(2, 2, 4, alpha=0.5), (3, 4))
        self.assertEqual(es.hypergeometric_interval(2, 2, 4, alpha=0.05), (2, 4))
        with self.assertRaises(ValueError):
            es.hypergeometric_interval(3, 2, 4)

    def test_coverage_holds_for_every_true_count(self):
        population, sample, alpha = 12, 5, 0.05
        for marked in range(population + 1):
            covered = 0.0
            for x in range(max(0, sample - population + marked), min(sample, marked) + 1):
                low, high = es.hypergeometric_interval(x, sample, population, alpha)
                if low <= marked <= high:
                    covered += comb(marked, x) * comb(population - marked, sample - x) / comb(population, sample)
            self.assertGreaterEqual(covered, 1 - alpha, marked)


class Compare(unittest.TestCase):
    def test_six_one_way_is_a_call_and_five_is_not(self):
        base = [scored(f"q{i}", False) for i in range(6)] + [scored("same", True)]
        better = [scored(f"q{i}", True) for i in range(6)] + [scored("same", True)]
        report = sps.compare([base], [better])
        self.assertEqual(report["discordant"], {"better": 6, "worse": 0})
        self.assertEqual(report["concordant"], {"both pass": 1, "both fail": 0})
        self.assertEqual(report["verdict"], "better")
        self.assertEqual(sps.compare([better], [base])["verdict"], "worse")
        five = sps.compare([base[1:]], [better[1:]])
        self.assertEqual(five["verdict"], "can't tell from noise")
        self.assertIsNone(five["fewest_one_way_to_call"])

    def test_unpaired_and_changed_items_are_listed_not_scored(self):
        base = [scored("a", True), scored("b", True, turns=2), scored("only-base", True)]
        cand = [scored("a", False), scored("b", False, turns=3)]
        report = sps.compare([base], [cand])
        self.assertEqual(report["paired"], 1)
        self.assertEqual(report["unpaired"], ["only-base"])
        self.assertEqual(report["changed_items"], ["b"])

    def test_repeated_runs_are_averaged_within_the_question(self):
        base = [[scored("a", False)], [scored("a", True)]]
        cand = [[scored("a", True)], [scored("a", True)]]
        report = sps.compare(base, cand)
        self.assertEqual(report["questions"][0]["baseline"], 0.5)
        self.assertEqual(report["discordant"], {"better": 1, "worse": 0})
        self.assertEqual([r["passed"] for r in report["per_run_pass_rates"]["baseline"]], [0, 1])

    def test_outcome_choice_and_legacy_rows(self):
        legacy = {"query_id": "a", "judge_pass_k": True, "mechanical_checks_passed": False,
                  "turn_scores": [{"judge": {"verdicts": [{"pass": True, "reason": "r"}] * 3, "pass_k": True}}]}
        self.assertTrue(sps.outcome(legacy, "judge"))
        self.assertFalse(sps.outcome(legacy, "mechanical"))
        self.assertFalse(sps.outcome(legacy, "both"))
        with self.assertRaises(ValueError):
            sps.compare([[legacy]], [[legacy]], kind="vibes")


class Calibration(unittest.TestCase):
    def test_allocation(self):
        self.assertEqual(sps.allocate([20, 14], 10), [6, 4])
        self.assertEqual(sps.allocate([20, 14], 34), [20, 14])
        self.assertEqual(sps.allocate([20, 1], 3), [2, 1])
        self.assertEqual(sum(sps.allocate([7, 5, 0], 6)), 6)

    def test_a_stratum_sampled_whole_is_exact_and_spends_no_alpha(self):
        whole = sps.stratified_interval([(4, 4, 3), (6, 6, 1)])
        self.assertEqual(whole, {"estimate": 0.4, "interval": [0.4, 0.4]})
        alone = sps.stratified_interval([(10, 5, 2)])
        with_known = sps.stratified_interval([(10, 5, 2), (10, 10, 0)])
        self.assertEqual([round(v * 20) for v in with_known["interval"]],
                         [round(v * 10) for v in alone["interval"]])

    def test_plan_is_the_fewest_labels_meeting_the_target(self):
        sizes, target = [20, 14], 0.1
        n = sps.plan_sample(sizes, target)
        self.assertLessEqual(sps.planned_half_width(sizes, n), target)
        self.assertGreater(sps.planned_half_width(sizes, n - 1), target)
        self.assertEqual(sps.plan_sample(sizes, 0.0), 34)

    def _run(self):
        questions = {"items": [{"id": f"q{i}", "turns": [{"ask": f"question {i}?", "rubric": {
            "must_mention": [["x"]], "must_not_claim": [], "expected_citations": [], "abstain": False,
            "notes": "x"}}]} for i in range(10)]}
        answers = [{"query_id": f"q{i}", "turns": [{"answer": None if i == 9 else f"answer {i}\nline two"}]}
                   for i in range(10)]
        rows = [scored(f"q{i}", i < 6, graded=i != 9) for i in range(9)] + [scored("q9", False, graded=False)]
        return questions, answers, rows

    def test_sample_then_score(self):
        questions, answers, rows = self._run()
        labels = sps.calibration_sample(questions, answers, rows, half_width=0.0)
        self.assertEqual(labels["frame"], {"judge pass": {"population": 6, "sample": 6},
                                           "judge fail": {"population": 3, "sample": 3},
                                           "unanswered": {"population": 1, "sample": 0}})
        first = labels["items"][0]
        self.assertEqual(set(first), {"query_id", "turns", "human_verdict", "human_note"})
        self.assertEqual(set(first["turns"][0]), {"turn", "question", "rubric", "answer"})
        self.assertEqual(first["turns"][0]["answer"][1], "line two")
        order = [int(item["query_id"][1:]) < 6 for item in labels["items"]]
        self.assertNotEqual(order, sorted(order, reverse=True))
        report = sps.calibration_score(rows, labels)
        self.assertEqual((report["labelled"], report["unlabelled"]), (0, 9))
        self.assertIsNone(report["corrected_pass_rate"]["estimate"])
        judge = {r["query_id"]: r["judge_pass"] for r in rows}
        for item in labels["items"]:
            item["human_verdict"] = "pass" if judge[item["query_id"]] else "fail"
        labels["items"][[judge[i["query_id"]] for i in labels["items"]].index(True)]["human_verdict"] = "fail"
        report = sps.calibration_score(rows, labels)
        self.assertEqual(report["corrected_pass_rate"], {"estimate": 0.5, "interval": [0.5, 0.5]})
        self.assertAlmostEqual(report["correction"], -0.1)
        self.assertAlmostEqual(report["false_pass_rate"]["estimate"], 1 / 6)
        self.assertEqual(report["false_fail_rate"]["estimate"], 0)
        self.assertEqual(len(report["disagreements"]), 1)

    def test_one_labelled_file_scores_another_judge_of_the_same_answers(self):
        questions, answers, rows = self._run()
        labels = sps.calibration_sample(questions, answers, rows, half_width=0.0)
        truth = {f"q{i}": i % 2 == 0 for i in range(9)}
        for item in labels["items"]:
            item["human_verdict"] = "pass" if truth[item["query_id"]] else "fail"
        other = [dict(r, judge_pass=truth.get(r["query_id"], False)) for r in rows]
        report = sps.calibration_score(rows, labels, other)
        self.assertEqual(report["judge_accuracy"], {"estimate": 1.0, "interval": [1.0, 1.0]})
        self.assertEqual(report["corrected_pass_rate"]["estimate"], 0.5)
        self.assertEqual({(c["sampled_as"], c["judge"]) for c in report["cells"]},
                         {("judge pass", "pass"), ("judge pass", "fail"),
                          ("judge fail", "pass"), ("judge fail", "fail")})
        self.assertLess(sps.calibration_score(rows, labels)["judge_accuracy"]["estimate"], 1.0)

    def test_labels_must_match_the_run(self):
        questions, answers, rows = self._run()
        labels = sps.calibration_sample(questions, answers, rows, half_width=0.0)
        labels["items"][0]["human_verdict"] = "maybe"
        with self.assertRaises(ValueError):
            sps.calibration_score(rows, labels)
        labels["items"][0]["human_verdict"] = None
        rewritten = [dict(r, answer_sha256="other") if r["query_id"] == "q0" else r for r in rows]
        with self.assertRaises(ValueError):
            sps.calibration_score(rows, labels, rewritten)
        with self.assertRaises(ValueError):
            sps.calibration_score(rows, labels, rows[:-1])


if __name__ == "__main__":
    unittest.main()
