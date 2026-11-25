"""The condensing replay sends the request the app's note step sends and reads the answer as the app does."""
import json
import re
import sys
import time
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
        self.assertEqual(self.messages[0]["content"], self.sections["input 1: message (system)"])
        self.assertEqual(self.messages[1]["content"], self.sections["input 2: message (user)"])

    def test_the_snapshot_has_no_schema(self):
        self.assertNotIn("schema", self.sections)

    def test_the_sampling_and_the_reservation_are_the_snapshots(self):
        sent = json.loads(self.sections["sent with"])
        self.assertEqual(replay.SAMPLING, sent["sampling"])
        window = int(re.search(r"pub const SCRIPTED_WINDOW: usize = ([\d_]+);", text_of(SCRIPTED_RS))
                     .group(1).replace("_", ""))
        self.assertEqual(replay.max_tokens_for(window), sent["max_output_tokens"])

    def test_the_body_is_what_the_snapshot_says_was_sent(self):
        sent = json.loads(self.sections["sent with"])
        window = int(re.search(r"pub const SCRIPTED_WINDOW: usize = ([\d_]+);", text_of(SCRIPTED_RS))
                     .group(1).replace("_", ""))
        body = replay.body("m", self.messages, window)
        self.assertNotIn("response_format", body)
        self.assertEqual(sent["reasoning_effort"], "none")
        self.assertNotIn("reasoning_effort", body)
        self.assertEqual(sent["thinking"], "off")
        self.assertEqual(body["chat_template_kwargs"], {"enable_thinking": False})
        for key, value in sent["sampling"].items():
            self.assertEqual(body[key], value, key)
        self.assertEqual(sent["stop"], [])
        self.assertNotIn("stop", body)
        self.assertEqual(body["max_tokens"], sent["max_output_tokens"])
        self.assertEqual(body["messages"], self.messages)


class Reservation(unittest.TestCase):
    def test_a_quarter_of_the_window_at_every_size(self):
        self.assertEqual(replay.max_tokens_for(16384), 4096)
        self.assertEqual(replay.max_tokens_for(65536), 16384)
        self.assertEqual(replay.max_tokens_for(196608), 49152)
        self.assertEqual(replay.max_tokens_for(1048576), 262144)

    def test_the_doubled_room_is_the_default_times_two_held_to_half_the_window(self):
        for window, doubled in ((4096, 2048), (16384, 8192), (65536, 32768), (131072, 65536),
                                (196608, 98304), (262144, 131072), (1048576, 524288)):
            self.assertEqual(replay.max_tokens_for(window, 2), doubled, window)

    def test_the_doubled_room_is_larger_than_the_first_at_every_window(self):
        for window in (4096, 16384, 65536, 131072, 196608, 262144, 1048576):
            self.assertGreater(replay.max_tokens_for(window, 2), replay.max_tokens_for(window), window)

    def test_the_body_carries_the_scale(self):
        body = replay.body("m", [], 16384, 2)
        self.assertEqual(body["max_tokens"], 8192)


class Splitter(unittest.TestCase):
    def test_sentences(self):
        for text, count in (("Dr. Smith arrived. He left.", 2), ("Line one\nLine two", 2),
                            ("It cost 3.5 million. Then it fell.", 2), ("He said “Stop.” Then he left.", 2),
                            ("See e.g. this one.", 1), ("J. Smith wrote it.", 1)):
            self.assertEqual(len(replay.sentences(text)), count, text)


COMPLETE, CUT_OFF_FINISH, DECLINED_FINISH = "stop", "length", "refusal"


def answered(*numbers):
    return ("answered", list(numbers))


def unanswered(why):
    return ("unanswered", why)


def contradicted(text):
    return ("contradicted", text)


def read_keeps(text, finish, items, own):
    """What the answer says of each of `items` results that each show `own` sentences, in shown order."""
    shown = [(i, source(i + 1, *[f"Sentence {k}." for k in range(1, own + 1)])) for i in range(items)]
    got = replay.read(reply(text, finish), shown)
    assert list(got) == list(range(items))
    return [got[i] for i in range(items)]


class Keeps(unittest.TestCase):
    def test_keeps_reads_every_spelling_of_a_line(self):
        text = "r1: u1 u2\n[r2] [u1], [u3]\n  r3: u2,u3  \n\nr4\tu3 u1\n"
        self.assertEqual(read_keeps(text, COMPLETE, 4, 3),
                         [answered(1, 2), answered(1, 3), answered(2, 3), answered(3, 1)])

    def test_keeps_counts_a_handle_once_in_written_order(self):
        self.assertEqual(read_keeps("r1: u3 u1 u3 u2 u1", COMPLETE, 2, 3), [answered(3, 1, 2), answered()])

    def test_an_item_line_with_no_own_handles_keeps_nothing(self):
        self.assertEqual(read_keeps("r1\nr2: u1\nr3:", COMPLETE, 3, 2), [answered(), answered(1), answered()])

    def test_the_word_none_says_every_item_keeps_nothing(self):
        for text in ["none", "  none \n", "\n\nnone"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 2), [answered(), answered(), answered()], repr(text))
        self.assertEqual(read_keeps("r1: u1\nnone", COMPLETE, 3, 2), [answered(1), answered(), answered()])
        self.assertEqual(read_keeps("none", CUT_OFF_FINISH, 3, 2), [unanswered("cut_off")] * 3)

    def test_an_empty_answer_is_not_read_and_a_declined_one_is_declined(self):
        for text in ["", " \n\n  "]:
            self.assertEqual(read_keeps(text, COMPLETE, 2, 2), [unanswered("not_read")] * 2, repr(text))
            self.assertEqual(read_keeps(text, CUT_OFF_FINISH, 2, 2), [unanswered("cut_off")] * 2, repr(text))
        self.assertEqual(read_keeps("r1: u1", DECLINED_FINISH, 2, 2), [unanswered("declined")] * 2)

    def test_a_cut_off_answer_drops_its_last_line_and_leaves_the_rest_cut_off(self):
        self.assertEqual(read_keeps("r1: u1\nr2: u2", CUT_OFF_FINISH, 3, 2),
                         [answered(1), unanswered("cut_off"), unanswered("cut_off")])
        self.assertEqual(read_keeps("r1: u1\nr2: u2\n\n", CUT_OFF_FINISH, 3, 2),
                         [answered(1), unanswered("cut_off"), unanswered("cut_off")])
        self.assertEqual(read_keeps("r1: u1\nr2: u2\n", COMPLETE, 3, 2), [answered(1), answered(2), answered()])

    def test_an_own_handle_that_does_not_resolve_leaves_only_its_item_unplaced(self):
        for line in ["r1: u9", "r1: u1 u9"]:
            text = f"{line}\nr2: u1"
            self.assertEqual(read_keeps(text, COMPLETE, 3, 2),
                             [unanswered("unplaced"), answered(1), answered()], repr(text))

    def test_an_item_handle_that_is_not_shown_leaves_the_unnamed_items_unplaced(self):
        for line in ["r9: u1", "u1: u1", "[r9]: u1"]:
            text = f"r2: u1\n{line}"
            self.assertEqual(read_keeps(text, COMPLETE, 3, 2),
                             [unanswered("unplaced"), answered(1), unanswered("unplaced")], repr(text))

    def test_words_that_are_not_handles_are_passed_over(self):
        self.assertEqual(read_keeps("Here is my answer:\nr1: u1", COMPLETE, 3, 3),
                         [answered(1), answered(), answered()])
        self.assertEqual(read_keeps("- r1: u1\n* r2: u2\n1. r3: u3", COMPLETE, 3, 3),
                         [answered(1), answered(2), answered(3)])
        self.assertEqual(read_keeps("**r1**: `u1`", COMPLETE, 3, 3), [answered(1), answered(), answered()])

    def test_items_on_one_line_each_keep_their_own(self):
        for text in ["r1: u1 u2; r2: u3", "r1: u1, u2, r2: u3"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 3), [answered(1, 2), answered(3), answered()], repr(text))

    def test_handles_are_read_whatever_punctuation_surrounds_them(self):
        self.assertEqual(read_keeps("r1:u1,u2.\n[r2]: [u3].", COMPLETE, 3, 3),
                         [answered(1, 2), answered(3), answered()])

    def test_an_answer_naming_no_item_is_not_read_unless_it_says_none(self):
        for text in ["None", "none.", "**NONE**", chr(0xE9) + "none", "none" + chr(0xB2)]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 2), [answered(), answered(), answered()], repr(text))
        for text in ["I found nothing that bears on it.", "1none", "none 2"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 2), [unanswered("not_read")] * 3, repr(text))

    def test_an_own_handle_before_any_item_leaves_the_unnamed_items_unplaced(self):
        self.assertEqual(read_keeps("u1\nr2: u1", COMPLETE, 3, 2),
                         [unanswered("unplaced"), answered(1), unanswered("unplaced")])

    def test_an_own_handle_on_a_line_after_an_unknown_word_is_not_given_to_the_line_above(self):
        self.assertEqual(read_keeps("r2: u1\nx1: u2", COMPLETE, 3, 2),
                         [unanswered("unplaced"), answered(1), unanswered("unplaced")])

    def test_equal_lines_for_one_item_count_once_and_differing_lines_contradict(self):
        self.assertEqual(read_keeps("r1: u1 u2\nr2: u1\nr1: u1 u2", COMPLETE, 3, 2),
                         [answered(1, 2), answered(1), answered()])
        self.assertEqual(read_keeps("r1: u1\nr1: u2", COMPLETE, 3, 2),
                         [contradicted("r1 has two entries that differ."), answered(), answered()])

    def test_a_range_of_own_handles_keeps_every_handle_from_the_first_through_the_second(self):
        for text in ["r1: u1-u4", "r1: u1–u4", "r1: u1 - u4", "r1: u1..u4", "r1: u1...u4", "r1: u1…u4",
                     "r1: u1—u4", "r1: u1-4", "r1: u1 – 4", "r1: u1 -- u4"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 4), [answered(1, 2, 3, 4), answered(), answered()],
                             repr(text))

    def test_a_range_counts_a_handle_already_kept_once(self):
        self.assertEqual(read_keeps("r1: u3 u1-u3", COMPLETE, 2, 4), [answered(3, 1, 2), answered()])

    def test_a_reversed_range_or_one_past_the_last_own_handle_leaves_its_item_unplaced(self):
        for line in ["r1: u3-u1", "r1: u2-u5", "r1: u2-5", "r1: u3-2"]:
            text = f"{line}\nr2: u1"
            self.assertEqual(read_keeps(text, COMPLETE, 3, 4),
                             [unanswered("unplaced"), answered(1), answered()], repr(text))

    def test_a_range_of_items_is_not_an_entry(self):
        for text in ["r1-r3: u1", "r1-3: u1", "r1 – r3: u1"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 4), [unanswered("unplaced")] * 3, repr(text))

    def test_handles_are_read_in_any_case(self):
        self.assertEqual(read_keeps("R1: U1 U2\nr2: U3", COMPLETE, 3, 3), [answered(1, 2), answered(3), answered()])

    def test_an_item_named_above_its_sentences_keeps_the_sentences_listed_under_it(self):
        self.assertEqual(read_keeps("r1:\n- u1\n- u2\nr2:\n- u3", COMPLETE, 3, 3),
                         [answered(1, 2), answered(3), answered()])
        self.assertEqual(read_keeps("r1\nu1 u2", COMPLETE, 3, 3), [answered(1, 2), answered(), answered()])
        self.assertEqual(read_keeps("r1:\n- u1-u3", COMPLETE, 3, 3), [answered(1, 2, 3), answered(), answered()])

    def test_an_item_named_above_a_line_that_says_none_keeps_nothing(self):
        self.assertEqual(read_keeps("r1:\nnone\nr2: u1", COMPLETE, 3, 3), [answered(), answered(1), answered()])

    def test_an_entry_holding_a_word_or_number_it_cannot_read_is_not_read(self):
        for text in ["r1: 2 3\nr2: u1", "r1: sentence two\nr2: u1"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 3),
                             [unanswered("not_read"), answered(1), answered()], repr(text))

    def test_an_entry_of_only_punctuation_or_none_keeps_nothing(self):
        for text in ["r1: -\nr2: u1", "r1: none\nr2: u1", "r1:\nr2: u1"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 3), [answered(), answered(1), answered()], repr(text))

    def test_an_entry_that_says_none_and_names_what_it_keeps_contradicts_itself(self):
        said = contradicted("r1 both says none and names what it keeps.")
        for text in ["r1: none u1\nr2: u2", "r1:\nnone\nu1\nr2: u2"]:
            self.assertEqual(read_keeps(text, COMPLETE, 3, 3), [said, answered(2), answered()], repr(text))

    def test_a_cut_off_answer_with_an_item_still_open_leaves_every_item_cut_off(self):
        self.assertEqual(read_keeps("r1:\n- u1\n- u2", CUT_OFF_FINISH, 3, 3), [unanswered("cut_off")] * 3)

    def test_lines_are_trimmed_and_ranges_read_with_white_space_only_as_rust_does(self):
        for separator in "\x1c\x1d\x1e\x1f":
            self.assertEqual(read_keeps(f"r1: u1\n{separator}", CUT_OFF_FINISH, 2, 2),
                             [answered(1), unanswered("cut_off")], repr(separator))
            self.assertEqual(read_keeps(f"r1: u1{separator}-{separator}u3", COMPLETE, 2, 3),
                             [answered(1, 3), answered()], repr(separator))
            self.assertIsNone(replay.pointed(f"[r1{separator}]"), repr(separator))
            self.assertIsNone(replay.pointed(f"{separator}r1"), repr(separator))
        self.assertEqual(read_keeps("r1: u1\u3000-\u2028u3", COMPLETE, 2, 3), [answered(1, 2, 3), answered()])
        self.assertEqual(replay.pointed("\u3000[ r1 ]\u2029"), ("r", 1))

    def test_white_space_is_python_s_less_the_four_information_separators(self):
        spaces = {chr(code) for code in range(0x110000) if chr(code).isspace()}
        self.assertEqual(set(replay.WHITE_SPACE), spaces - set("\x1c\x1d\x1e\x1f"))

    def test_a_handle_or_range_end_too_long_for_u32_reads_as_rust_parses_it(self):
        self.assertIsNone(replay.pointed("u" + "1" * 4301))
        self.assertIsNone(replay.pointed("u4294967296"))
        self.assertEqual(replay.pointed("u4294967295"), ("u", 4294967295))
        self.assertEqual(read_keeps("r1: u1 u" + "1" * 4301, COMPLETE, 2, 2), [answered(1), answered()])
        for end in ["4294967296", "9" * 4301]:
            self.assertEqual(read_keeps(f"r1: u1-{end}\nr2: u1", COMPLETE, 3, 4),
                             [unanswered("unplaced"), answered(1), answered()], f"{len(end)} digits")
        self.assertEqual(read_keeps("r1: u1-" + "0" * 5000 + "3", COMPLETE, 2, 4), [answered(1, 2, 3), answered()])

    def test_says_none_reads_a_long_run_of_punctuation_in_linear_time(self):
        line = "r1: u1" + " -" * 32_000 + " u2"
        started = time.perf_counter()
        self.assertFalse(replay.says_none(line))
        self.assertLess(time.perf_counter() - started, 2.0)


class Finish(unittest.TestCase):
    def test_finish_reasons_end_as_the_contract_reads_them(self):
        for finish, ended in (("stop", "complete"), (None, "complete"), ("tool_calls", "complete"),
                              ("length", "cut_off"), ("MAX_TOKENS", "cut_off"), ("content_filter", "cut_off"),
                              ("refusal", "declined")):
            self.assertEqual(replay.ended_of(finish), ended, finish)

    def test_a_filtered_answer_is_read_as_cut_off(self):
        got = replay.read(reply("r1: u1\nr2: u", "content_filter"), [(0, source(1, "a")), (1, source(2, "b"))])
        self.assertEqual(got, {0: ("answered", [1]), 1: ("unanswered", "cut_off")})


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

        def send(messages, scale=1):
            sent.append(messages)
            return reply("r1: u1\nr9: u1") if len(sent) == 1 else reply("r2: u1\nr3:")

        got = replay.ask_each("q", asked, send)
        self.assertEqual(len(sent), 2)
        task = sent[1][1]["content"]
        self.assertIn("[r2]", task)
        self.assertIn("[r3]", task)
        self.assertNotIn("[r1]", task)
        self.assertEqual(got["rows"], [("answered", [1]), ("answered", [1]), ("answered", [])])
        self.assertEqual(got["first"], {0})

    def test_a_lone_source_cut_off_is_resent_with_twice_the_room_then_corrected_with_it(self):
        answers = [reply("r1: u", "length"), reply("r1:\n- u", "length"), reply("r1: u1")]
        sent = []

        def send(messages, scale=1):
            sent.append((messages, scale))
            return answers[len(sent) - 1]

        got = replay.ask_each("q", [source(1, "a")], send)
        self.assertEqual([scale for _, scale in sent], [1, 2, 2])
        self.assertEqual(sent[1][0], sent[0][0])
        self.assertEqual(sent[2][0], sent[0][0] + [{"role": "assistant", "content": "r1:\n- u"},
                                                   {"role": "user", "content": replay.CUT_OFF}])
        self.assertEqual((got["resends"], got["corrections"]), (1, 1))
        self.assertEqual(got["rows"], [("answered", [1])])

    def test_a_lone_source_not_cut_off_is_corrected_at_the_first_room(self):
        sent = []

        def send(messages, scale=1):
            sent.append(scale)
            return reply("I cannot say.")

        got = replay.ask_each("q", [source(1, "a")], send)
        self.assertEqual(sent, [1, 1])
        self.assertEqual((got["resends"], got["corrections"]), (0, 1))

    def test_an_unplaced_answer_asked_alone_is_corrected_with_unplaced(self):
        sent = []

        def send(messages, scale=1):
            sent.append(messages)
            return reply("r1: u9") if len(sent) == 1 else reply("r1: u1")

        got = replay.ask_each("q", [source(1, "a")], send)
        self.assertEqual(len(sent), 2)
        self.assertEqual(sent[1][3]["content"], replay.UNPLACED)
        self.assertEqual(sent[1][2], {"role": "assistant", "content": "r1: u9"})
        self.assertEqual((got["resends"], got["corrections"]), (0, 1))
        self.assertEqual(got["rows"], [("answered", [1])])


if __name__ == "__main__":
    unittest.main()
