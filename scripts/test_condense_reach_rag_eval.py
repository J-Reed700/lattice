"""The reach of the note step is judged with the chat loop's own constants and arithmetic."""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import condense_reach as reach

ROOT = Path(__file__).resolve().parent.parent
CHAT = ROOT / "crates/lattice-desktop/src/features/conversation/chat"
NOTES_RS = CHAT / "tool_loop/research_notes.rs"
ROOM_RS = CHAT / "tool_loop/room.rs"
TOOL_LOOP_RS = CHAT / "tool_loop.rs"
CONTRACT_RS = CHAT / "tool_loop/reading/contract.rs"
PROMPT_SETTINGS_RS = CHAT / "prompt_settings.rs"
LLM_PORT_RS = ROOT / "crates/lattice-core/src/application/ports/llm_port.rs"
BUDGET_RS = ROOT / "crates/lattice-core/src/application/services/context_assembler/budget.rs"
STRING = r'"((?:[^"\\]|\\.)*)"'


def text_of(path):
    return path.read_text("utf-8").replace("\r\n", "\n")


def rust_string(source, name):
    m = re.search(r"const\s+" + name + r"\s*:\s*&str\s*=\s*" + STRING + r"\s*;", source)
    assert m, name
    return re.sub(r"\\(.)", lambda e: e.group(1), m.group(1))


def rust_number(source, name):
    m = re.search(r"const\s+" + name + r"\s*:\s*\w+\s*=\s*([\d_.]+)\s*;", source)
    assert m, name
    return float(m.group(1).replace("_", ""))


def window_of(tokens, rate=4.0):
    return int(reach.input_budget(tokens) * rate * reach.UNDER_FILL)


def call(qid, seq, messages, tools=True, prompt_tokens=None):
    request = {"messages": messages, "max_tokens": 4096}
    if tools:
        request["tools"] = [{"type": "function", "function": {"name": "semantic_search"}}]
    reply = {"usage": {"prompt_tokens": prompt_tokens}} if prompt_tokens else {}
    return json.dumps({"query_id": qid, "call": {"seq": seq, "method": "POST", "request": request, "reply": reply}})


SYSTEM = {"role": "system", "content": "You are Lattice, a precise research assistant."}


def tool_round(n, size):
    return [{"role": "assistant", "content": "", "tool_calls": [
        {"id": f"c{n}", "type": "function", "function": {"name": "semantic_search", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": f"c{n}", "content": "x" * size}]


def recorded_turn(planned_chars, sizes):
    planned = {"role": "user", "content": "[current message]" + "p" * planned_chars}
    messages = [SYSTEM, planned]
    lines = [call("q", 0, messages + [{"role": "user", "content": reach.READ_INSTRUCTION}])]
    for n, size in enumerate(sizes, 1):
        lines.append(call("q", n, list(messages)))
        messages = messages + tool_round(n, size)
    lines.append(call("q", len(sizes) + 1, list(messages), prompt_tokens=len(json.dumps(messages)) // 4))
    return lines


class Constants(unittest.TestCase):
    def test_the_constants_are_the_apps(self):
        notes, room, budget = text_of(NOTES_RS), text_of(ROOM_RS), text_of(BUDGET_RS)
        self.assertEqual(rust_number(notes, "CONDENSE_AT_WINDOW_SHARE"), reach.CONDENSE_AT_WINDOW_SHARE)
        self.assertEqual(rust_number(notes, "MAX_CONDENSES_PER_TURN"), reach.MAX_CONDENSES_PER_TURN)
        self.assertTrue(rust_string(notes, "CONDENSE_INSTRUCTION").startswith(reach.CONDENSE_INSTRUCTION_START))
        self.assertEqual(rust_number(room, "MIN_TOOL_RESULT_CHARS"), reach.MIN_TOOL_RESULT_CHARS)
        self.assertEqual(rust_number(text_of(LLM_PORT_RS), "UNDER_FILL"), reach.UNDER_FILL)
        self.assertIn(f"source_rounds.unwrap_or({reach.MAX_TOOL_ROUNDS})", text_of(TOOL_LOOP_RS))
        self.assertEqual(rust_number(budget, "OUTPUT_RESERVATION_CAP"), reach.OUTPUT_RESERVATION_CAP)
        self.assertIn(f"(capacity / {reach.OUTPUT_RESERVATION_DIVISOR}).min(OUTPUT_RESERVATION_CAP)", budget)
        self.assertIn(f"(capacity as f64 * {reach.SAFETY_MARGIN_SHARE}).ceil() as usize", budget)
        self.assertIn(f"proportional.max({reach.SAFETY_MARGIN_FLOOR})", budget)
        self.assertEqual(rust_string(text_of(CONTRACT_RS), "READ_INSTRUCTION"), reach.READ_INSTRUCTION)
        self.assertTrue(rust_string(text_of(PROMPT_SETTINGS_RS), "NUMERIC_BRACKETS_SYSTEM_PROMPT")
                        .startswith(reach.CHAT_SYSTEM_START))


class Arithmetic(unittest.TestCase):
    """The cases the loop's own tests pin (chat/tool_loop/tests.rs)."""

    def test_a_large_window_gives_each_result_its_share_with_no_ceiling(self):
        window = window_of(131_072)
        self.assertEqual(reach.allowance(window, 20_000, 4), (window - 20_000) // 5)
        self.assertGreater(reach.allowance(window, 20_000, 4), 50_000)

    def test_a_full_window_gives_a_result_only_what_is_left(self):
        window = window_of(8_192)
        self.assertEqual(reach.allowance(window, 1_000_000, 3), 0)
        self.assertEqual(reach.allowance(window, window - 900, 1), 450)

    def test_a_window_with_room_keeps_the_floor(self):
        window = window_of(8_192)
        self.assertEqual(reach.allowance(window, window - 4_000, 3), reach.MIN_TOOL_RESULT_CHARS)

    def test_the_input_budget_leaves_the_reservation_and_the_margin(self):
        self.assertEqual(reach.input_budget(16_384), 16_384 - 4_096 - 820)
        self.assertEqual(reach.input_budget(196_608), 196_608 - 32_768 - 9_831)
        self.assertEqual(reach.input_budget(4_096), 4_096 - 1_024 - 256)


class Turns(unittest.TestCase):
    def shaped(self, planned_chars, sizes):
        turns = reach.turns_of(recorded_turn(planned_chars, sizes))
        self.assertEqual(len(turns), 1)
        return reach.shape(turns[0])

    def test_the_reading_request_is_not_a_round(self):
        turn = self.shaped(1_000, [5_000, 5_000])
        self.assertEqual([sum(r["results"]) for r in turn["rounds"]], [5_000, 5_000])
        self.assertEqual(turn["after"], 0)

    def test_results_that_outgrow_the_window_condense_before_it_fills(self):
        turn = self.shaped(1_000, [60_000, 60_000, 60_000])
        outcome, window = reach.reach(turn, 32_768)
        self.assertEqual(outcome, "condensed at round 2", window)

    def test_a_planned_request_that_fills_the_window_leaves_no_round_to_condense(self):
        turn = self.shaped(200_000, [60_000, 60_000, 60_000])
        self.assertEqual(reach.reach(turn, 32_768)[0], "window full at round 1")

    def test_results_under_the_share_are_answered_from(self):
        turn = self.shaped(1_000, [2_000])
        self.assertEqual(reach.reach(turn, 196_608)[0], "answered")

    def test_a_turn_that_condensed_counts_its_condensing(self):
        lines = recorded_turn(1_000, [5_000])
        lines.insert(2, call("q", 99, [{"role": "system", "content": reach.CONDENSE_INSTRUCTION_START + " ..."},
                                       {"role": "user", "content": "My question: q"}], tools=False))
        self.assertEqual(reach.turns_of(lines)[0]["condensed"], 1)


if __name__ == "__main__":
    unittest.main()
