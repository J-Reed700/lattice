import json, os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402

TOOLS = [{"type": "function", "function": {"name": "semantic_search", "description": "Search.",
                                           "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}}}]


class BodyOf(unittest.TestCase):
    def setUp(self):
        claude.use("claude-haiku-5-5", None)

    def test_a_reading_is_sent_as_the_app_sends_it(self):
        body = claude.body_of({"model": "qwen", "temperature": 0.0, "top_k": 1, "max_tokens": 900,
                               "chat_template_kwargs": {"enable_thinking": False}, "stop": ["[7]", "\n7."],
                               "tools": TOOLS, "tool_choice": "none",
                               "messages": [{"role": "system", "content": "Be exact."},
                                            {"role": "user", "content": "x" * 4000}]})
        self.assertEqual(body["model"], "claude-haiku-5-5")
        self.assertEqual(body["thinking"], {"type": "adaptive"})
        self.assertEqual(body["max_tokens"], claude.MAX_TOKENS)
        self.assertEqual(body["system"], [{"type": "text", "text": "Be exact.", "cache_control": {"type": "ephemeral"}}])
        self.assertEqual(body["stop_sequences"], ["[7]", "\n7."])
        self.assertEqual(body["tool_choice"], {"type": "none"})
        self.assertEqual(body["tools"][0]["name"], "semantic_search")
        self.assertIn("input_schema", body["tools"][0])
        self.assertNotIn("cache_control", body)
        for knob in ("temperature", "top_k", "chat_template_kwargs"):
            self.assertNotIn(knob, body)

    def test_output_is_cut_to_what_the_window_leaves(self):
        claude.use("claude-haiku-5-5", None, window=10_000, max_tokens=8_000)
        body = claude.body_of({"messages": [{"role": "user", "content": "x" * 16_000}]})
        self.assertLess(body["max_tokens"], 8_000)
        self.assertGreater(body["max_tokens"], 5_000)

    def test_calls_and_results_become_blocks_and_a_continued_exchange_is_cached(self):
        body = claude.body_of({"tools": TOOLS, "tool_choice": "none", "messages": [
            {"role": "system", "content": "S"},
            {"role": "user", "content": "Q"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "semantic_search",
                                                                  "arguments": json.dumps({"query": "q"})}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "result\n\n---\nFrom me: condense"}]})
        self.assertEqual(body["messages"], [
            {"role": "user", "content": "Q"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "call_1", "name": "semantic_search",
                                               "input": {"query": "q"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_1",
                                          "content": "result\n\n---\nFrom me: condense"}]}])
        self.assertEqual(body["cache_control"], {"type": "ephemeral"})

    def test_a_shared_prefix_keeps_its_breakpoint(self):
        body = claude.body_of({"messages": [{"role": "user", "content": claude.cached("message", "\n\ngroup")}]})
        self.assertEqual(body["messages"][0]["content"], [
            {"type": "text", "text": "message", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "\n\ngroup"}])

    def test_a_warmup_is_not_sent(self):
        self.assertEqual(claude.call({"messages": []}, "warmup")["content"], "")


if __name__ == "__main__":
    unittest.main()
