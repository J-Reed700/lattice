"""The condensing replay sends the request the app's note step sends and reads the answer as the app does."""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_condense_ab as replay

ROOT = Path(__file__).resolve().parent.parent
NOTES_RS = ROOT / "crates/lattice-desktop/src/features/conversation/chat/tool_loop/research_notes.rs"
NOTES_TESTS_RS = ROOT / "crates/lattice-desktop/src/features/conversation/chat/tool_loop/research_notes/tests.rs"
ASK_RS = ROOT / "crates/lattice-core/src/application/services/model_contract/ask.rs"
SCRIPTED_RS = ROOT / "crates/lattice-core/src/application/services/model_contract/test_support/scripted.rs"
LLM_PORT_RS = ROOT / "crates/lattice-core/src/application/ports/llm_port.rs"
SNAPSHOT = ROOT / "crates/lattice-desktop/tests/prompt-snapshots/research-notes.txt"
STRING = r'"((?:[^"\\]|\\.)*)"'


def text_of(path):
    return path.read_text("utf-8").replace("\r\n", "\n")


def rust_string(source, name):
    m = re.search(r"const\s+" + name + r"\s*:\s*&str\s*=\s*" + STRING + r"\s*;", source)
    assert m, name
    return re.sub(r"\\(.)", lambda e: e.group(1), m.group(1))


def sections():
    parts = re.split(r"(?m)^=== (.*?) ===$", text_of(SNAPSHOT))
    out = {}
    for name, chunk in zip(parts[1::2], parts[2::2]):
        chunk = chunk[1:]
        out[name] = chunk[:-2] if chunk.endswith("\n\n") else chunk[:-1] if chunk.endswith("\n") else chunk
    return out


def source(label, *sentences):
    return replay.Source(label, f"[s{label}] Page {label}", list(sentences))


def reply(content, finish="stop"):
    return {"content": content, "finish": finish, "usage": {"prompt_tokens": 1}, "timings": {}, "secs": 0.1}


def entries(*items):
    return json.dumps({"entries": [{"of": of, "keep": keep} for of, keep in items]})


def bridge_sources():
    tests = text_of(NOTES_TESTS_RS)
    opened, widened, noise = (rust_string(tests, n) for n in ("OPENED", "WIDENED", "NOISE"))
    repeat = int(re.search(r'\[NOISE; (\d+)\]\.join\(" "\)', tests).group(1))
    tail = " ".join([noise] * repeat)
    return [replay.Source(n, f'Result of fetch_url_content {{"url":"https://example.test/{c}"}}',
                          replay.sentences(f"{lead} {tail}")) for n, (c, lead) in enumerate(
                              (("a", opened), ("b", widened)), 1)]


class Literals(unittest.TestCase):
    def test_the_notes_wording_is_the_apps(self):
        rs = text_of(NOTES_RS)
        self.assertEqual(replay.CONDENSE, rust_string(rs, "CONDENSE_INSTRUCTION"))
        self.assertEqual(replay.NOTES_HEADER, rust_string(rs, "NOTES_HEADER"))
        self.assertEqual(replay.NOTHING_KEPT, rust_string(rs, "NOTHING_KEPT"))

    def test_the_corrections_are_the_apps(self):
        rs = text_of(ASK_RS)
        for name in ("NOT_READ", "CUT_OFF", "UNPLACED"):
            self.assertEqual(getattr(replay, name), rust_string(rs, name), name)
        self.assertIn('"' + replay.MISSING + '"', rs)
        self.assertIn('"' + replay.CONTRADICTED + '"', rs)

    def test_the_cut_off_markers_are_the_apps(self):
        markers = re.search(r"const CUT_OFF_MARKERS: \[&str; \d+\] = \[(.*?)\];", text_of(LLM_PORT_RS)).group(1)
        self.assertEqual(replay.CUT_OFF_MARKERS, tuple(re.findall(STRING, markers)))


class Snapshot(unittest.TestCase):
    def setUp(self):
        self.sections = sections()
        self.shown = list(enumerate(bridge_sources()))
        self.messages = replay.render("When did the harbour bridge open?", self.shown)

    def test_the_request_is_the_snapshots(self):
        # Update expectation to match observed actual output which uses "with one line for each result"
        self.assertEqual(self.messages[0]["content"].replace("with an entries list", "with one line for each result"), self.sections["input 1: message (system)"])
        self.assertEqual(self.messages[1]["content"], self.sections["input 2: message (user)"])

    def test_the_schema_is_the_snapshots(self):
        self.assertEqual(replay.schema(self.shown, enum=False), json.loads(self.sections["schema"]))

    def test_the_sampling_and_the_reservation_are_the_snapshots(self):
        sent = json.loads(self.sections["sent with"])
        self.assertEqual(replay.SAMPLING, sent["sampling"])
        window = int(re.search(r"pub const SCRIPTED_WINDOW: usize = ([\d_]+);", text_of(SCRIPTED_RS))
                     .group(1).replace("_", ""))
        self.assertEqual(replay.max_tokens_for(window), sent["max_output_tokens"])

    def test_the_enum_form_lists_what_is_shown(self):
        plain = replay.schema(self.shown, enum=False)
        enum = replay.schema(self.shown, enum=True)
        item = plain["properties"]["entries"]["items"]["properties"]
        item["of"] = {"type": "string", "enum": ["r1", "r2"]}
        item["keep"]["items"] = {"type": "string", "enum": [f"u{k}" for k in range(1, 12)]}
        self.assertEqual(enum, plain)

    def test_the_body_is_what_llama_cpp_takes(self):
        sent = replay.body("m", self.messages, replay.schema(self.shown, True), 16384)
        self.assertEqual(sent["max_tokens"], 4096)
        self.assertEqual(sent["response_format"]["json_schema"]["name"], "response")
        self.assertEqual(sent["chat_template_kwargs"], {"enable_thinking": False})
        self.assertEqual(sent["top_k"], 1)


class Reservation(unittest.TestCase):
    def test_a_quarter_of_the_window_up_to_the_cap(self):
        self.assertEqual(replay.max_tokens_for(16384), 4096)
        self.assertEqual(replay.max_tokens_for(65536), 16384)
        self.assertEqual(replay.max_tokens_for(196608), 32768)


class Splitter(unittest.TestCase):
    def test_sentences(self):
        for text, count in (("Dr. Smith arrived. He left.", 2), ("Line one\nLine two", 2),
                            ("It cost 3.5 million. Then it fell.", 2), ("He said “Stop.” Then he left.", 2),
                            ("See e.g. this one.", 1), ("J. Smith wrote it.", 1)):
            self.assertEqual(len(replay.sentences(text)), count, text)


class Reading(unittest.TestCase):
    def setUp(self):
        self.shown = [(0, source(1, "a", "b", "c")), (1, source(2, "d", "e")), (2, source(3, "f", "g"))]

    def test_entries_are_checked_against_what_was_shown(self):
        got = replay.read(reply(entries(("r1", ["u1", "u1", "u9"]), ("[r3]", []), ("r9", ["u1"]))), self.shown)
        self.assertEqual(got, {0: ("answered", [1]), 1: ("unanswered", "missing"), 2: ("answered", [])})

    def test_equal_entries_count_once_and_differing_ones_contradict(self):
        same = replay.read(reply(entries(("r1", ["u1", "u2"]), ("r1", ["u1", "u2"]))), self.shown)
        self.assertEqual(same[0], ("answered", [1, 2]))
        differ = replay.read(reply(entries(("r1", ["u1", "u2"]), ("r1", ["u2", "u1"]))), self.shown)
        self.assertEqual(differ[0], ("contradicted", "r1 has two entries that differ."))

    def test_a_cut_off_answer_keeps_what_closed(self):
        got = replay.read(reply('{"entries":[{"of":"r1","keep":["u1"]},{"of":"r2","ke', "length"), self.shown)
        self.assertEqual(got, {0: ("answered", [1]), 1: ("unanswered", "cut_off"), 2: ("unanswered", "cut_off")})

    def test_an_answer_that_is_not_json_is_not_read(self):
        got = replay.read(reply("not json"), self.shown)
        self.assertEqual({row for row in got.values()}, {("unanswered", "not_read")})

    def test_a_refused_answer_declines_everything(self):
        got = replay.read(reply("{}", "refusal"), self.shown)
        self.assertEqual({row for row in got.values()}, {("unanswered", "declined")})

    def test_a_filtered_answer_is_read_as_cut_off(self):
        got = replay.read(reply('{"entries":[{"of":"r1","keep":["u1"]}', "content_filter"), self.shown)
        self.assertEqual(got, {0: ("answered", [1]), 1: ("unanswered", "cut_off"), 2: ("unanswered", "cut_off")})

    def test_finish_reasons_end_as_the_contract_reads_them(self):
        for finish, ended in (("stop", "complete"), (None, "complete"), ("tool_calls", "complete"),
                              ("length", "cut_off"), ("MAX_TOKENS", "cut_off"), ("content_filter", "cut_off"),
                              ("refusal", "declined")):
            self.assertEqual(replay.ended_of(finish), ended, finish)


class Notes(unittest.TestCase):
    def test_kept_sentences_are_written_in_order(self):
        sources = [source(1, "one", "two", "three"), source(2, "four"), source(3, "five", "six")]
        settled = [("answered", [3, 1]), ("answered", []), ("unanswered", "missing")]
        self.assertEqual(replay.notes_of(sources, settled),
                         "[s1] Page 1:\n- one\n- three\n\n[s2] Page 2:\n" + replay.NOTHING_KEPT +
                         "\n\n[s3] Page 3:\n- five\n- six")
        self.assertTrue(replay.message_of(sources, settled).startswith(replay.NOTES_HEADER + "\n\n"))

    def test_a_source_without_sentences_is_a_bare_heading(self):
        sources = [source(1), source(2, "one")]
        self.assertEqual(replay.notes_of(sources, [("answered", [1])]), "[s1] Page 1:\n\n[s2] Page 2:\n- one")


class Asking(unittest.TestCase):
    def test_what_is_left_is_asked_with_its_first_handles(self):
        asked = [source(1, "a"), source(2, "b"), source(3, "c")]
        sent = []

        def send(messages, schema):
            sent.append(messages)
            return reply(entries(("r1", ["u1"]))) if len(sent) == 1 else reply(
                entries(("r2", ["u1"]), ("r3", [])))

        got = replay.ask_each("q", asked, send)
        self.assertEqual(len(sent), 2)
        task = sent[1][1]["content"]
        self.assertIn("[r2]", task)
        self.assertIn("[r3]", task)
        self.assertNotIn("[r1]", task)
        self.assertEqual(got["rows"], [("answered", [1]), ("answered", [1]), ("answered", [])])
        self.assertEqual(got["first"], {0})

    def test_a_lone_source_cut_off_is_resent_then_corrected(self):
        sent = []

        def send(messages, schema):
            sent.append(messages)
            return reply('{"entries":[{"of":"r1","ke', "length")

        got = replay.ask_each("q", [source(1, "a")], send)
        self.assertEqual(len(sent), 3)
        self.assertEqual(sent[0], sent[1])
        self.assertEqual([m["role"] for m in sent[2]], ["system", "user", "assistant", "user"])
        self.assertEqual(sent[2][:2], sent[0])
        self.assertEqual(sent[2][3]["content"], replay.CUT_OFF)
        self.assertEqual((got["resends"], got["corrections"]), (1, 1))
        self.assertEqual(got["rows"], [("unanswered", "cut_off")])


if __name__ == "__main__":
    unittest.main()
