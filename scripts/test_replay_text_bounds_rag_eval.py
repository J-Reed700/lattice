"""A replay sends the answer schema the app sends: no length rule on text a model writes."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_grade_order_ab as grade
import replay_lesson_citation_ab as lesson
import replay_select_order_ab as select

QUOTE_FLOOR = 25
RUBRIC = [{"id": "r1", "max_points": 2}, {"id": "r2", "max_points": 4}]


def schemas():
    for arm in select.ARMS:
        yield f"select/{arm}", select.schema(arm, ["a", "b"])
    for arm in grade.ARMS:
        yield f"grade/{arm}", grade.schema(arm, RUBRIC)
        yield f"diagnostic/{arm}", grade.diagnostic_schema(arm)
    for arm in lesson.ARMS:
        yield f"lesson/{arm}", lesson.schema(arm, 3)


def length_rules(node, path=""):
    """Every maxLength, and every minLength above one, as (pointer, key, value)."""
    found = []
    if isinstance(node, dict):
        for key, child in node.items():
            here = f"{path}/{key}"
            if key in ("maxLength", "minLength") and isinstance(child, int):
                if key == "maxLength" or child > 1:
                    found.append((here, key, child))
            else:
                found.extend(length_rules(child, here))
    elif isinstance(node, list):
        for index, child in enumerate(node):
            found.extend(length_rules(child, f"{path}/{index}"))
    return found


class ReplaySchemas(unittest.TestCase):
    def test_no_arm_bounds_the_length_of_what_the_model_writes(self):
        for name, schema in schemas():
            with self.subTest(name):
                allowed = [
                    rule for rule in length_rules(schema)
                    if rule[1] == "minLength" and rule[2] == QUOTE_FLOOR and rule[0].endswith("/quote/minLength")
                ]
                self.assertEqual([r for r in length_rules(schema) if r not in allowed], [])

    def test_the_whole_sentence_arm_keeps_its_quote_floor(self):
        rules = [
            rule for rule in length_rules(lesson.schema("whole_sentence", 3)) if rule[0].endswith("/quote/minLength")
        ]
        self.assertEqual(
            sorted(rule[0] for rule in rules),
            ["/properties/blocks/items/properties/quote/minLength",
             "/properties/questions/items/properties/quote/minLength"],
        )
        self.assertEqual({rule[2] for rule in rules}, {QUOTE_FLOOR})


if __name__ == "__main__":
    unittest.main()
