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

    def test_no_thinking_is_each_models_off_form_and_a_model_without_one_thinks_at_low_effort(self):
        reading = {"max_tokens": 900, "messages": [{"role": "user", "content": "x" * 4000}]}
        for model, thinking, max_tokens in (("claude-haiku-5-5", {"type": "disabled"}, 900),
                                            ("claude-sonnet-5-5", {"type": "between_tools"}, 900),
                                            ("claude-opus-5-5", {"type": "adaptive"}, claude.MAX_TOKENS)):
            claude.use(model, None, thinking="off")
            body = claude.body_of(reading)
            self.assertEqual(body["thinking"], thinking, model)
            self.assertEqual(body["max_tokens"], max_tokens, model)
            self.assertEqual(body["output_config"], {"effort": "low"}, model)
        claude.use("claude-opus-5-5", None)
        self.assertNotIn("output_config", claude.body_of(reading))

    def test_a_json_schema_is_the_output_format_shaped_as_the_app_shapes_it(self):
        schema = {"type": "object", "properties": {
            "items": {"type": "array", "minItems": 2, "maxItems": 4, "items": {"type": "string", "maxLength": 80}},
            "score": {"type": "integer", "minimum": 0, "description": "How sure."}}}
        body = claude.body_of({"messages": [{"role": "user", "content": "Q"}],
                               "response_format": {"type": "json_schema",
                                                   "json_schema": {"name": "response", "strict": True, "schema": schema}}})
        self.assertEqual(body["output_config"], {"format": {"type": "json_schema", "schema": {
            "type": "object", "additionalProperties": False, "properties": {
                "items": {"type": "array", "minItems": 1, "items": {"type": "string", "maxLength": 80},
                          "description": "minItems: 2, maxItems: 4"},
                "score": {"type": "integer", "description": "How sure. (minimum: 0)"}}}}})
        self.assertEqual(body["thinking"], {"type": "adaptive"})
        claude.use("claude-haiku-5-5", None, thinking="off")
        body = claude.body_of({"messages": [{"role": "user", "content": "Q"}],
                               "response_format": {"type": "json_schema", "json_schema": {"schema": {"type": "string"}}}})
        self.assertEqual(body["output_config"], {"effort": "low",
                                                 "format": {"type": "json_schema", "schema": {"type": "string"}}})

    def test_a_warmup_is_not_sent(self):
        self.assertEqual(claude.call({"messages": []}, "warmup")["content"], "")


if __name__ == "__main__":
    unittest.main()
