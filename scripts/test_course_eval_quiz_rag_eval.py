"""Offline tests for the quiz items of the course eval: normalisation, the reference grader, format validity, the
mechanical checks and the renderings. Fixtures span biology, history, chemistry and languages."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import course_eval_quiz as cq

SINGLE = {"id": "bio.sc", "format": "single_choice", "prompt": "Which organelle makes most of a cell's ATP?",
          "options": ["Ribosome", "Mitochondrion", "Golgi apparatus", "Lysosome"], "key": [1],
          "explanation": "Oxidative phosphorylation happens in the mitochondrion.", "lesson_block_id": "b1"}
TRUE_FALSE = {"id": "hist.tf", "format": "true_false", "prompt": "The Magna Carta was sealed in 1215.",
              "options": ["True", "False"], "key": [0], "explanation": "It was sealed at Runnymede in 1215."}
MULTI = {"id": "chem.ms", "format": "multi_select", "prompt": "Which of these are noble gases?",
         "options": ["Neon", "Argon", "Nitrogen", "Helium", "Chlorine"], "key": [0, 1, 3],
         "explanation": "Neon, argon and helium have full outer shells."}
SHORT = {"id": "chem.sa", "format": "short_answer", "prompt": "What is the pH of a 0.01 M solution of HCl?",
         "variants": ["2", "two", "pH 2"], "numeric": {"value": 2, "tolerance": 0.05, "unit_variants": []},
         "explanation": "pH is minus the log of 0.01, which is 2."}
WORD = {"id": "lang.sa", "format": "short_answer", "prompt": "What is the French word for 'library'?",
        "variants": ["bibliothèque", "la bibliothèque", "une bibliothèque"], "numeric": None}
CLOZE = {"id": "lang.cloze", "format": "cloze", "prompt": "In Spanish, 'I am' is {{1}} for a trait and {{2}} for a state.",
         "blanks": [["soy"], ["estoy"]], "explanation": "Ser for traits, estar for states."}
MATCHING = {"id": "hist.match", "format": "matching", "prompt": "Match each treaty to its year.",
            "left": ["Treaty of Westphalia", "Treaty of Utrecht", "Treaty of Versailles"],
            "right": ["1648", "1713", "1919", "1815"], "key": [0, 1, 2], "explanation": "Westphalia ended the Thirty Years War."}
ORDERING = {"id": "hist.order", "format": "ordering", "prompt": "Put these events in order.",
            "values": ["Magna Carta is sealed", "The Black Death reaches England", "The Wars of the Roses begin",
                       "The Act of Union joins England and Scotland"]}
EXPLAIN = {"id": "bio.expl", "format": "explanation", "prompt": "Explain how a competitive inhibitor changes Km and Vmax.",
           "rubric": [{"id": "c1", "title": "Binding site", "max_points": 2,
                       "levels": {"incomplete": "no site", "adequate": "binds the enzyme", "strong": "binds the active site"}},
                      {"id": "c2", "title": "Kinetics", "max_points": 3,
                       "levels": {"incomplete": "no effect", "adequate": "Km rises", "strong": "Km rises, Vmax unchanged"}}],
           "key_points": ["competes for the active site", "apparent Km rises, Vmax unchanged"]}
ALL = [SINGLE, TRUE_FALSE, MULTI, SHORT, WORD, CLOZE, MATCHING, ORDERING, EXPLAIN]


def with_(item, **changes):
    return {**item, **changes}


class Normalisation(unittest.TestCase):
    def test_the_apps_rules(self):
        self.assertEqual(cq.normalise("  The   Mitochondrión! "), "mitochondrion")
        self.assertEqual(cq.normalise("1,200,000 cells"), "1200000 cells")
        self.assertEqual(cq.normalise("pH 3.5."), "ph 3.5")
        self.assertEqual(cq.normalise("A"), "a")  # a lone article is the answer, not an article
        self.assertEqual(cq.normalise("la bibliothèque"), "la bibliotheque")  # only English articles are dropped

    def test_numbers_units_and_tolerance(self):
        self.assertEqual(cq.parse_number("1,200 workers", ["workers"]), 1200.0)
        self.assertEqual(cq.parse_number("37 ℃", ["°C"]), 37.0)
        self.assertEqual(cq.parse_number("−4.5"), -4.5)
        self.assertIsNone(cq.parse_number("about 37"))
        self.assertTrue(cq.text_matches("2.05", [], SHORT["numeric"]))
        self.assertFalse(cq.text_matches("2.06", [], SHORT["numeric"]))
        self.assertTrue(cq.text_matches("PH 2", SHORT["variants"]))
        self.assertTrue(cq.text_matches("Bibliotheque", WORD["variants"]))
        self.assertFalse(cq.text_matches("", ["x"]))
        # As the engine: any comma between digits is a separator, fractions count, spaces group thousands.
        self.assertEqual(cq.normalise("3,5"), "35")
        self.assertEqual(cq.parse_number("1/2"), 0.5)
        self.assertEqual(cq.parse_number("1 200"), 1200.0)
        self.assertIsNone(cq.parse_number("1/0"))
        self.assertIsNone(cq.parse_number("inf"))
        # A number is always decided in code: against the numeric key, else against the variants.
        self.assertIs(cq.text_match("5", ["7", "seven"], {"value": 7, "tolerance": 0.5}), False)
        self.assertIs(cq.text_match("7.0", ["7", "seven"]), True)
        self.assertIs(cq.text_match("8", ["7", "seven"]), False)
        self.assertIsNone(cq.text_match("seven-ish", ["7", "seven"]))


class ReferenceGrader(unittest.TestCase):
    def points(self, item, response):
        return cq.grade_keyed(item, response)["earned"]

    def test_choice_formats(self):
        self.assertEqual(self.points(SINGLE, {"selected_indices": [1]}), 1.0)
        self.assertEqual(self.points(SINGLE, {"selected_indices": [0, 1]}), 0.0)
        self.assertEqual(self.points(TRUE_FALSE, {"selected_indices": [1]}), 0.0)
        self.assertEqual(self.points(with_(SINGLE, points=2), {"selected_indices": [1]}), 2.0)

    def test_multi_select_subtracts_wrong_picks_and_floors_at_zero(self):
        self.assertEqual(self.points(MULTI, {"selected_indices": [0, 1, 3]}), 1.0)
        self.assertEqual(self.points(MULTI, {"selected_indices": [0, 1, 2]}), round(1 / 3, 6))
        self.assertEqual(self.points(MULTI, {"selected_indices": [0, 2, 4]}), 0.0)
        self.assertEqual(self.points(MULTI, {"selected_indices": [0, 1, 2, 3, 4]}), round(1 / 3, 6))

    def test_short_answer_goes_to_the_model_only_on_a_non_empty_miss(self):
        self.assertEqual(cq.grade_keyed(SHORT, {"text": "2.0"})["path"], "deterministic")
        self.assertEqual(cq.grade_keyed(SHORT, {"text": "two units"})["path"], "model")
        self.assertEqual(cq.grade_keyed(SHORT, {"text": "3"}), {"path": "deterministic", "earned": 0.0, "possible": 1.0,
                                                               "share": 0.0})  # a wrong number is wrong in code
        self.assertEqual(cq.grade_keyed(SHORT, {"text": "  "}), {"path": "deterministic", "earned": 0.0,
                                                                  "possible": 1.0, "share": 0.0})

    def test_partial_credit_formats(self):
        self.assertEqual(self.points(CLOZE, {"blank_responses": ["Soy", "es"]}), 0.5)
        self.assertEqual(self.points(CLOZE, {"blank_responses": ["soy"]}), 0.5)
        self.assertEqual(self.points(MATCHING, {"matches": [0, 3, None]}), round(1 / 3, 6))
        swapped = [ORDERING["values"][i] for i in (1, 0, 2, 3)]
        self.assertEqual(self.points(ORDERING, {"ordered_values": swapped}), round(5 / 6, 6))
        self.assertEqual(self.points(ORDERING, {"ordered_values": list(reversed(ORDERING["values"]))}), 0.0)
        self.assertEqual(self.points(ORDERING, {"ordered_values": ORDERING["values"][:3]}), 0.5)

    def test_explanations_are_model_graded_and_worth_their_rubric(self):
        self.assertEqual(cq.grade_keyed(EXPLAIN, {"text": "anything"}),
                         {"path": "model", "earned": None, "possible": 5.0, "share": None})
        self.assertEqual(cq.grade_keyed(EXPLAIN, {"text": "  "})["path"], "deterministic")  # "No answer given"


class FormatValidity(unittest.TestCase):
    def test_valid_items_of_every_format(self):
        for item in ALL:
            self.assertEqual(cq.format_problems(item), [], item["id"])
        self.assertEqual({i["format"] for i in ALL}, set(cq.FORMATS))

    def test_an_engine_rubric_with_descriptions_is_valid(self):
        recorded = with_(EXPLAIN, rubric=[{"id": "c1", "title": "Binding site", "max_points": 2,
                                           "description": "Names the active site"}])
        self.assertEqual(cq.format_problems(recorded), [])
        self.assertIn("- Binding site: Names the active site", cq.render_item(recorded, key=True))

    def test_each_format_rule(self):
        cases = [
            (with_(SINGLE, key=[1, 2]), "single choice needs exactly one key"),
            (with_(SINGLE, key=[9]), "key indices must be distinct and in range"),
            (with_(TRUE_FALSE, options=["Yes", "No"]), "true/false options must be exactly True, False"),
            (with_(MULTI, key=[0]), "multi-select needs 2-3 keys"),
            (with_(MULTI, options=["Neon", "Argon", "Helium"], key=[0, 1]), "multi-select needs 4-6 options"),
            (with_(SHORT, variants=[]), "short answer needs 1-12 accepted variants"),
            (with_(SINGLE, options=[f"o{n}" for n in range(9)]), "single choice needs 2-8 options"),
            (with_(CLOZE, prompt="{{2}} then {{1}}"), "cloze blanks must be {{1}}..{{n}}, each once and in order, at most 8"),
            (with_(SHORT, numeric={"value": float("nan"), "tolerance": 0}),
             "numeric key needs a finite value and a tolerance >= 0"),
            (with_(CLOZE, prompt="{{1}} and {{3}}"), "cloze blanks must be {{1}}..{{n}}, each once and in order, at most 8"),
            (with_(CLOZE, blanks=[["soy"]]), "cloze needs one non-empty variant list per blank"),
            (with_(MATCHING, right=["1648", "1713", "1919", "1815", "1789"]),
             "matching's right side holds the pairs and at most one distractor"),
            (with_(MATCHING, key=[0, 0, 2]), "matching key repeats a right item"),
            (with_(ORDERING, values=["a", "A", "b"]), "ordering values must be distinct"),
            (with_(ORDERING, values=["a", "b"]), "ordering needs 3-12 values"),
            (with_(EXPLAIN, rubric=[]), "explanation needs 1-12 rubric criteria"),
            (with_(EXPLAIN, rubric=[EXPLAIN["rubric"][0], {**EXPLAIN["rubric"][1], "levels": {"strong": "x"}}]),
             "criterion c2: needs a description, or incomplete, adequate and strong levels"),
            (with_(SINGLE, prompt=" "), "empty prompt"),
            (with_(SINGLE, format="essay"), "unknown format 'essay'"),
        ]
        for item, problem in cases:
            self.assertIn(problem, cq.format_problems(item), item)


class MechanicalChecks(unittest.TestCase):
    def test_leaks_are_the_key_in_the_stem(self):
        self.assertEqual(cq.leak_hits(SINGLE), [])
        leaked = with_(SINGLE, prompt="The mitochondrion makes ATP. Which organelle makes most of a cell's ATP?")
        self.assertEqual(cq.leak_hits(leaked), ["the stem contains the key 'Mitochondrion'"])
        # A comparison question names the key beside another option; an option alone in the stem is fine.
        self.assertEqual(cq.leak_hits(with_(SINGLE, prompt="Which makes ATP: the ribosome or the mitochondrion?")), [])
        self.assertEqual(cq.leak_hits(with_(SINGLE, prompt="Is it the ribosome that makes most ATP?")), [])
        long_key = with_(SINGLE, options=["It binds the active site of the enzyme", "It denatures it", "It adds heat"],
                         key=[0], prompt="Why does it bind the active site of an enzyme so tightly?")
        self.assertEqual(cq.leak_hits(long_key), ["the stem contains the key 'It binds the active site of the enzyme'"])
        self.assertEqual(cq.leak_hits(with_(CLOZE, prompt="Soy means 'I am': {{1}} for a trait and {{2}} for a state.")),
                         ["the stem contains the key 'soy'"])
        self.assertEqual(cq.leak_hits(with_(SHORT, prompt="Is the pH of 0.01 M HCl equal to pH 2?")),
                         ["the stem contains the key 'pH 2'"])
        self.assertEqual(cq.leak_hits(with_(SHORT, prompt="Is the pH of 0.01 M HCl 2?")), [])  # too short to check
        self.assertEqual(cq.leak_hits(with_(MATCHING, left=["Treaty of 1648", "Treaty of Utrecht", "Versailles"])), [])
        self.assertEqual(cq.leak_hits(with_(MULTI, prompt="Neon and which others are noble gases?")),
                         ["the stem contains the key 'Neon'"])

    def test_key_positions_flag_a_bank_keyed_at_one_place(self):
        all_b = [with_(SINGLE, id=f"q{n}", key=[1]) for n in range(6)]
        report = cq.key_positions(all_b)["choice"]
        self.assertEqual(report["counts"], {"A": 0, "B": 6, "C": 0, "D": 0})
        self.assertAlmostEqual(report["p_value"], 4 * 0.25 ** 6)
        self.assertTrue(report["skewed"])
        self.assertFalse(cq.bank_checks(all_b + [ORDERING])["passed"])
        spread = [with_(SINGLE, id=f"q{n}", key=[n % 4]) for n in range(8)]
        self.assertFalse(cq.key_positions(spread)["choice"]["skewed"])
        self.assertFalse(cq.key_positions(all_b[:2])["choice"]["skewed"])  # two items cannot show a skew
        self.assertEqual(cq.key_positions([MULTI])["choice"]["shapes"], [[5, 3]])
        true_only = [with_(TRUE_FALSE, id=f"t{n}") for n in range(9)]
        truth = cq.key_positions(true_only)["true_false"]
        self.assertEqual((truth["true"], truth["false"], truth["skewed"]), (9, 0, True))
        self.assertAlmostEqual(cq.upper_tail([0.5, 0.5], 1), 0.75)
        self.assertIsNone(cq.key_positions([ORDERING])["choice"]["p_value"])

    def test_length_bias_needs_a_margin(self):
        self.assertEqual(cq.length_bias(SINGLE), (False, False))  # the longest option is a distractor
        long = with_(SINGLE, options=["Ribosome", "The mitochondrion, through oxidative phosphorylation", "Golgi",
                                      "Lysosome"])
        self.assertEqual(cq.length_bias(long), (True, True))
        close = with_(SINGLE, options=["Ribosome", "Mitochondrion", "Golgi body", "Lysosome"])
        self.assertEqual(cq.length_bias(close), (True, False))
        self.assertEqual(cq.length_bias(TRUE_FALSE), (False, False))

    def test_duplicates_within_an_item(self):
        self.assertEqual(cq.duplicate_options(with_(SINGLE, options=["Ribosome", "ribosome!", "Golgi", "Lysosome"])),
                         ["options repeat 'ribosome'"])
        self.assertEqual(cq.duplicate_options(MATCHING), [])

    def test_key_and_explanation_disagree(self):
        self.assertEqual(cq.key_explanation_conflicts(SINGLE), [])
        self.assertEqual(cq.key_explanation_conflicts(with_(SINGLE, explanation="The correct answer is C.")),
                         ["names option C as the answer"])
        self.assertEqual(cq.key_explanation_conflicts(with_(SINGLE, explanation="The answer is Golgi apparatus.")),
                         ["names 'Golgi apparatus' as the answer"])
        self.assertEqual(cq.key_explanation_conflicts(with_(SINGLE, explanation="The answer is a mitochondrion.")), [])
        self.assertEqual(cq.key_explanation_conflicts(with_(MULTI, explanation="The correct answers are A, B and C.")),
                         ["names option C as the answer"])
        self.assertEqual(cq.key_explanation_conflicts(with_(TRUE_FALSE, explanation="False: it was 1225.")),
                         ["says the statement is false"])
        self.assertEqual(cq.key_explanation_conflicts(with_(SHORT, explanation="The answer is 3 because...")),
                         ["states the answer as '3'"])
        self.assertEqual(cq.letter_references(with_(SINGLE, explanation="Option B is right; (C) is a trap.")),
                         ["(C)", "Option B"])

    def test_absolutes_must_come_from_the_lesson(self):
        always = with_(TRUE_FALSE, prompt="Treaties always end wars.")
        self.assertEqual(cq.absolute_unstated(always, "Treaties end some wars."), ["always"])
        self.assertEqual(cq.absolute_unstated(always, "Treaties always end wars, the lesson claims."), [])
        self.assertEqual(cq.absolute_unstated(always, None), [])

    def test_bank_report(self):
        twin = with_(SINGLE, id="bio.sc2", prompt="Which organelle makes most of the cell's ATP?")
        bank = [SINGLE, twin, with_(MULTI, explanation="The correct answer is C."), ORDERING, EXPLAIN,
                with_(SINGLE, id="bad", key=[], prompt="Name a stage of mitosis.")]
        report = cq.bank_checks(bank, "lesson text", {"v1": "x"}, {"target": 9})
        self.assertFalse(report["passed"])
        self.assertEqual(report["near_duplicate_items"], [["bio.sc", "bio.sc2"]])
        self.assertEqual(report["duplicate_items"], [])
        self.assertEqual(report["hits"]["key_explanation_conflict"], ["chem.ms"])
        self.assertEqual(report["hits"]["format_invalid"], ["bad"])
        self.assertEqual(report["format_mix"], {"explanation": 1, "multi_select": 1, "ordering": 1, "single_choice": 3})
        self.assertEqual(report["key_longest"]["of"], 3)
        self.assertAlmostEqual(report["key_longest"]["chance"], (0.25 + 0.25 + 0.6) / 3)
        self.assertEqual(report["harness"], {"target": 9})
        exact = cq.bank_checks([SINGLE, with_(SINGLE, id="copy")])
        self.assertEqual(exact["duplicate_items"], [["bio.sc", "copy"]])
        self.assertTrue(cq.bank_checks([SINGLE, ORDERING])["passed"])
        self.assertFalse(cq.bank_checks([])["passed"])

    def test_anchors_and_verbatim_quotes(self):
        quoted = with_(SINGLE, source_version_id="v1", source_quote="makes   most ATP", lesson_block_id=None)
        self.assertEqual(cq.anchored(quoted, {"v1": "The mitochondrion makes most ATP."}), (True, True))
        self.assertEqual(cq.anchored(quoted, {"v1": "Something else."}), (True, False))
        self.assertEqual(cq.anchored(quoted, {}), (True, None))
        self.assertEqual(cq.anchored(with_(ORDERING), {}), (False, None))


class Renderings(unittest.TestCase):
    def test_the_key_shows_only_when_asked(self):
        for item in ALL:
            hidden, shown = cq.render_item(item), cq.render_item(item, key=True)
            self.assertTrue(shown.startswith(hidden.rstrip("\n")), item["id"])
            self.assertNotIn("Key:", hidden)
            self.assertNotIn("Explanation:", hidden)
        self.assertIn("Key: B", cq.render_item(SINGLE, key=True))
        self.assertIn("strong: Km rises, Vmax unchanged", cq.render_item(EXPLAIN, key=True))
        self.assertIn("- Kinetics (3 points)", cq.render_item(EXPLAIN))

    def test_ordering_is_never_shown_in_its_answer_order(self):
        for n in range(3, 8):
            item = {"id": f"o{n}", "format": "ordering", "values": [f"step {i}" for i in range(n)]}
            self.assertNotEqual(cq.shown_order(item), item["values"])
            self.assertEqual(sorted(cq.shown_order(item)), sorted(item["values"]))

    def test_answers_in_the_solve_format_round_trip(self):
        for item in ALL[:-1]:
            response = cq.parse_answer(item, cq.key_text(item).split(" (numeric")[0].split(" | ")[0]
                                       if item["format"] == "short_answer" else cq.key_text(item))
            self.assertTrue(cq.agrees_with_key(item, response), item["id"])
        self.assertEqual(cq.parse_answer(TRUE_FALSE, "False"), {"selected_indices": [1]})
        self.assertEqual(cq.parse_answer(MATCHING, "1-A, 3-C"), {"matches": [0, None, 2]})
        self.assertEqual(cq.parse_answer(CLOZE, "1: soy | 2: estoy"), {"blank_responses": ["soy", "estoy"]})
        self.assertIsNone(cq.parse_answer(SINGLE, "none of them"))
        self.assertFalse(cq.agrees_with_key(EXPLAIN, {"text": "x"}))
        self.assertFalse(cq.agrees_with_key(SINGLE, None))


if __name__ == "__main__":
    unittest.main()
