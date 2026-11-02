"""A replay's records say which model wrote them, or they cannot be read back as results."""
import ast
import io
import json
import sys
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_grade_order_ab as grade
import replay_lesson_citation_ab as lesson
import replay_memory_review_ab as review
import replay_model
import replay_select_order_ab as select

SCRIPTS = Path(__file__).resolve().parent
MODEL = "/models/Qwen3.5-9B-Q4_K_M.gguf"
GRADE_REPLY = json.dumps({"status": "provisional", "criteria": [{"score": 0, "evidence_quote": "x"}]})
SELECT_REPLY = json.dumps({"decisions": []})
LESSON_REPLY = json.dumps({"blocks": [], "questions": []})
GRADE_RECORD = {"id": "a", "persona": "wrong", "arm": "score_first", "seed": 1, "raw": GRADE_REPLY, "issue": None,
                "max_points": 2}
SELECT_RECORD = {"call": "call-1", "arm": "decision_first", "seed": 1, "shown": 1, "raw": SELECT_REPLY}
LESSON_RECORD = {"arm": "must_cite", "seed": 1, "finish": "stop", "final": LESSON_REPLY}
REVIEW_REPLY = json.dumps({"verdicts": []})
REVIEW_RECORD = {"arm": "shipped", "scenario": "moved on", "seed": 1, "raw": REVIEW_REPLY}


def records_file(folder, *records):
    path = Path(folder) / "records.jsonl"
    path.write_text("".join(json.dumps(record) + "\n" for record in records), "utf-8")
    return str(path)


def read_records(path):
    return [json.loads(line) for line in Path(path).read_text("utf-8").splitlines()]


def lesson_summary(path):
    lesson.summary(Namespace(summary=path, calls=None, draft=2))


def rows_without_a_model(source):
    """The lines of `source` that write a JSON row with no `model` key of its own."""
    lines = []
    for node in ast.walk(ast.parse(source)):
        if not (isinstance(node, ast.Call) and getattr(node.func, "attr", "") == "write"):
            continue
        for dumped in ast.walk(node):
            if isinstance(dumped, ast.Call) and getattr(dumped.func, "attr", "") == "dumps":
                row = dumped.args[0]
                keys = [key.value for key in getattr(row, "keys", []) if isinstance(key, ast.Constant)]
                if "model" not in keys:
                    lines.append(node.lineno)
    return lines


class EveryReplay(unittest.TestCase):
    def test_the_check_tells_a_stamped_row_from_a_bare_one(self):
        self.assertEqual(rows_without_a_model('out.write(json.dumps(rec) + "\\n")'), [1])
        self.assertEqual(rows_without_a_model('out.write(json.dumps({"id": 1}) + "\\n")'), [1])
        self.assertEqual(rows_without_a_model('out.write(json.dumps({"model": m, **rec}) + "\\n")'), [])
        self.assertEqual(rows_without_a_model('print(json.dumps(rec))'), [])

    def test_no_replay_writes_a_row_without_its_model(self):
        missing = {
            path.name: rows_without_a_model(path.read_text("utf-8"))
            for path in sorted(SCRIPTS.glob("replay_*.py"))
        }
        self.assertEqual({name: lines for name, lines in missing.items() if lines}, {})


def server_naming(*models):
    body = json.dumps({"data": [{"id": model} for model in models]}).encode("utf-8")
    return mock.patch.object(replay_model.urllib.request, "urlopen", return_value=io.BytesIO(body))


class AskingTheServer(unittest.TestCase):
    def test_the_server_says_which_model_it_serves(self):
        with server_naming(MODEL) as urlopen:
            self.assertEqual(replay_model.served_model("http://server/"), MODEL)
        self.assertEqual(urlopen.call_args.args[0].full_url, "http://server/v1/models")

    def test_a_server_naming_no_model_or_two_stops_the_run(self):
        for models in ((), (MODEL, "another")):
            with self.subTest(models=models), server_naming(*models), self.assertRaises(SystemExit):
                replay_model.served_model("http://server")

    def test_a_run_on_claude_names_claude_and_asks_no_server(self):
        with mock.patch.object(replay_model, "served_model") as asked:
            self.assertEqual(replay_model.model_of(Namespace(MODEL="claude-haiku-5-5"), "http://server"),
                             "claude-haiku-5-5")
        asked.assert_not_called()


class ReadingBack(unittest.TestCase):
    def test_records_that_name_no_model_are_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            for read, record in ((grade.summary, GRADE_RECORD), (select.summary, SELECT_RECORD),
                                 (lesson_summary, LESSON_RECORD), (review.summary, REVIEW_RECORD)):
                with self.subTest(read=read), self.assertRaises(SystemExit):
                    read(records_file(folder, record))

    def test_records_from_two_models_are_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            mixed = records_file(folder, {**GRADE_RECORD, "model": MODEL}, {**GRADE_RECORD, "model": "another"})
            with self.assertRaises(SystemExit):
                grade.summary(mixed)

    def test_the_summary_names_the_model(self):
        with tempfile.TemporaryDirectory() as folder, mock.patch("builtins.print") as printed:
            grade.summary(records_file(folder, {**GRADE_RECORD, "model": MODEL}))
        self.assertIn(mock.call("Model:", MODEL), printed.call_args_list)


class Writing(unittest.TestCase):
    def test_the_grader_replay_stamps_every_record(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "out.jsonl"
            args = Namespace(llama="http://server", out=str(out), seeds=1, diagnostic=False)
            with mock.patch.object(grade, "served_model", return_value=MODEL), \
                    mock.patch.object(grade, "ask", return_value=(GRADE_REPLY, "stop", 1.0)):
                grade.run(args)
            self.assertEqual({record["model"] for record in read_records(out)}, {MODEL})

    def test_the_selection_replay_stamps_every_record(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "out.jsonl"
            args = Namespace(llama="http://server", out=str(out), seeds=1, calls=folder)
            calls = [("call-1", "system", json.dumps({"results": [{"id": "result-0"}]}))]
            with mock.patch.object(select, "served_model", return_value=MODEL), \
                    mock.patch.object(select, "selection_calls", return_value=calls), \
                    mock.patch.object(select, "ask", return_value=(SELECT_REPLY, 1)):
                select.run(args)
            self.assertEqual({record["model"] for record in read_records(out)}, {MODEL})

    def test_the_lesson_replay_stamps_every_record(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "out.jsonl"
            args = Namespace(llama="http://server", out=str(out), seeds=1, calls=folder, draft=2)
            with mock.patch.object(lesson, "served_model", return_value=MODEL), \
                    mock.patch.object(lesson, "request_for", return_value=("system", "user", [{}])), \
                    mock.patch.object(lesson, "post", return_value=(LESSON_REPLY, "stop")):
                lesson.run(args)
            self.assertEqual({record["model"] for record in read_records(out)}, {MODEL})


    def test_the_review_replay_stamps_every_record(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "out.jsonl"
            args = Namespace(llama="http://server", out=str(out), seeds=1, shipped="a", candidate="b")
            with mock.patch.object(review, "served_model", return_value=MODEL), \
                    mock.patch.object(review, "verifier_system", side_effect=["old", "new"]), \
                    mock.patch.object(review, "ask", return_value=REVIEW_REPLY):
                review.run(args)
            self.assertEqual({record["model"] for record in read_records(out)}, {MODEL})


if __name__ == "__main__":
    unittest.main()
