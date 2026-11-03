"""The eval scripts read and write text as UTF-8, whatever the platform's default encoding is."""
import ast
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_close_read_ab as base

SCRIPTS = Path(__file__).resolve().parent
NOT_FILES = {"os", "tarfile", "zipfile", "webbrowser", "shelve", "dbm"}
COMPRESSED = {"gzip", "bz2", "lzma"}


def mode_of(call, position):
    if len(call.args) > position:
        mode = call.args[position]
    else:
        mode = next((k.value for k in call.keywords if k.arg == "mode"), None)
    return mode.value if isinstance(mode, ast.Constant) and isinstance(mode.value, str) else None


def text_without_an_encoding(call):
    """Whether `call` opens, reads or writes a text file at the platform's default encoding."""
    func = call.func
    if any(k.arg == "encoding" for k in call.keywords):
        return False
    if isinstance(func, ast.Name) and func.id == "open":
        mode = mode_of(call, 1)
        return len(call.args) < 4 and "b" not in (mode or "")
    if not isinstance(func, ast.Attribute):
        return False
    owner = func.value.id if isinstance(func.value, ast.Name) else None
    if func.attr == "open" and owner in COMPRESSED:
        return "t" in (mode_of(call, 1) or "")
    if func.attr == "open" and owner not in NOT_FILES:
        return "b" not in (mode_of(call, 0) or "")
    if func.attr == "read_text":
        return not call.args
    if func.attr == "write_text":
        return len(call.args) < 2
    return False


def text_without_an_encoding_in(source):
    """The lines of `source` that touch a text file with no `encoding`."""
    return sorted(
        node.lineno
        for node in ast.walk(ast.parse(source))
        if isinstance(node, ast.Call) and text_without_an_encoding(node)
    )


def cp1252_by_default(module):
    """`open` in `module` as it behaves on Windows: text at cp1252 unless told otherwise."""
    real = open

    def windows_open(file, mode="r", *args, **kwargs):
        if "b" not in mode and len(args) < 2:
            kwargs.setdefault("encoding", "cp1252")
        return real(file, mode, *args, **kwargs)

    return mock.patch.object(module, "open", windows_open, create=True)


class EveryScript(unittest.TestCase):
    def test_the_check_tells_a_pinned_encoding_from_the_default(self):
        flagged = text_without_an_encoding_in
        self.assertEqual(flagged("open(p)"), [1])
        self.assertEqual(flagged('open(p, "a")'), [1])
        self.assertEqual(flagged('json.load(open(p))'), [1])
        self.assertEqual(flagged('gzip.open(p, "rt")'), [1])
        self.assertEqual(flagged('Path(p).open()'), [1])
        self.assertEqual(flagged('Path(p).read_text()'), [1])
        self.assertEqual(flagged('Path(p).write_text(s)'), [1])
        self.assertEqual(flagged('open(p, encoding="utf-8")'), [])
        self.assertEqual(flagged('open(p, "rb")'), [])
        self.assertEqual(flagged('gzip.open(p)'), [])
        self.assertEqual(flagged('gzip.open(p, "rt", encoding="utf-8")'), [])
        self.assertEqual(flagged('Path(p).read_text("utf-8")'), [])
        self.assertEqual(flagged('Path(p).write_text(s, "utf-8")'), [])
        self.assertEqual(flagged('os.open(p, os.O_RDONLY)'), [])
        self.assertEqual(flagged('urllib.request.urlopen(req)'), [])

    def test_no_script_touches_a_text_file_at_the_default_encoding(self):
        found = {
            path.name: text_without_an_encoding_in(path.read_text("utf-8"))
            for path in sorted(SCRIPTS.rglob("*.py"))
        }
        self.assertEqual({name: lines for name, lines in found.items() if lines}, {})


class RecordedCalls(unittest.TestCase):
    def test_a_recorded_log_reads_back_as_written_where_the_default_is_not_utf8(self):
        said = "Probate: Edith Fleastmere left £4,680.83 — divided equally."
        row = {"query_id": "q", "call": {"path": "/v1/chat/completions",
                                         "request": {"messages": [{"role": "user", "content": said}]}}}
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "run.calls.jsonl"
            path.write_text(json.dumps(row, ensure_ascii=False) + "\n", "utf-8")
            with cp1252_by_default(base):
                rows = list(base.load_rows(str(path)))
        self.assertEqual(rows[0]["call"]["request"]["messages"][0]["content"], said)


if __name__ == "__main__":
    unittest.main()
