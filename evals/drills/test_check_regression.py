"""Tests for check_regression.py. Run: python -X utf8 -I evals/drills/test_check_regression.py"""
import importlib.util
import pathlib
import unittest

_spec = importlib.util.spec_from_file_location(
    "check_regression", pathlib.Path(__file__).with_name("check_regression.py")
)
check_regression = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check_regression)


def run(issues=(36, 40), findings=(45, 50), failed=0, wrong=(), model="claude-haiku-5-5", cases=40):
    """A grading report as the live eval writes it, holding only what the check reads."""
    return {
        "model": {"name": model, "provider": "anthropic"},
        "cases": cases,
        "failed": failed,
        "issues": {"agree": issues[0], "total": issues[1]},
        "findings": {"agree": findings[0], "total": findings[1]},
        "disagreements": [
            {"case": case, "kind": kind, "index": index} for case, kind, index in wrong
        ],
    }


class BaselineTests(unittest.TestCase):
    def test_keeps_the_worst_run_and_what_every_run_got_wrong_or_right(self):
        baseline = check_regression.make_baseline(
            [
                run(issues=(36, 40), wrong=[("a", "issue", 0), ("b", "issue", 1)]),
                run(issues=(38, 40), wrong=[("a", "issue", 0)]),
            ]
        )
        self.assertEqual(baseline["model"], "claude-haiku-5-5")
        self.assertEqual(baseline["runs"], 2)
        self.assertEqual(baseline["issues"]["worst_agree"], 36)
        self.assertEqual(baseline["issues"]["spread"], 2)
        self.assertEqual(
            baseline["ever_wrong"], ["a/issue/0", "b/issue/1"]
        )

    def test_a_case_that_could_not_be_graded_does_not_widen_the_spread(self):
        baseline = check_regression.make_baseline(
            [run(issues=(37, 40)), run(issues=(34, 37), failed=1)]
        )
        self.assertEqual(baseline["issues"]["spread"], 0)

    def test_refuses_runs_of_different_models(self):
        with self.assertRaises(ValueError):
            check_regression.make_baseline([run(), run(model="other")])

    def test_refuses_a_run_with_no_items(self):
        with self.assertRaises(ValueError):
            check_regression.make_baseline([run(issues=(0, 0))])


class CheckTests(unittest.TestCase):
    def baseline(self):
        return check_regression.make_baseline([run(), run()])

    def test_a_run_like_the_baseline_passes(self):
        self.assertEqual(check_regression.check(self.baseline(), run()).failures, [])

    def test_two_items_down_is_noise(self):
        self.assertEqual(
            check_regression.check(self.baseline(), run(issues=(34, 40))).failures, []
        )

    def test_three_items_down_is_a_regression(self):
        failures = check_regression.check(self.baseline(), run(issues=(33, 40))).failures
        self.assertEqual(len(failures), 1)
        self.assertIn("issues", failures[0])

    def test_a_wider_baseline_spread_widens_what_counts_as_noise(self):
        baseline = check_regression.make_baseline(
            [run(findings=(40, 50)), run(findings=(45, 50))]
        )
        self.assertEqual(
            check_regression.check(baseline, run(findings=(35, 50))).failures, []
        )
        self.assertTrue(check_regression.check(baseline, run(findings=(34, 50))).failures)

    def test_a_change_in_the_number_of_items_compares_rates_not_counts(self):
        # Half the items at the same rate is not a drop of 18.
        self.assertEqual(
            check_regression.check(self.baseline(), run(issues=(18, 20))).failures, []
        )

    def test_a_case_that_could_not_be_graded_is_a_regression(self):
        failures = check_regression.check(self.baseline(), run(failed=1)).failures
        self.assertTrue(any("not graded" in failure for failure in failures))

    def test_another_model_is_refused_not_compared(self):
        with self.assertRaises(ValueError):
            check_regression.check(self.baseline(), run(model="other"))

    def test_an_item_every_baseline_run_got_right_is_listed_to_read(self):
        result = check_regression.check(
            self.baseline(), run(wrong=[("c", "finding", 2)])
        )
        self.assertEqual(result.failures, [])
        self.assertEqual(result.to_read, ["c/finding/2"])

    def test_an_item_a_baseline_run_also_got_wrong_is_not_listed(self):
        baseline = check_regression.make_baseline([run(wrong=[("c", "finding", 2)]), run()])
        self.assertEqual(
            check_regression.check(baseline, run(wrong=[("c", "finding", 2)])).to_read, []
        )


if __name__ == "__main__":
    unittest.main()
