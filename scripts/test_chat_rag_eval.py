import unittest

import chat_rag_eval


class ChatRagEvalTests(unittest.TestCase):
    def test_base_v1_is_not_duplicated(self):
        self.assertEqual(
            chat_rag_eval.chat_completions_url("https://example.test/v1"),
            "https://example.test/v1/chat/completions",
        )

    def test_context_numbers_match_ranked_documents(self):
        documents = {
            "a": {"title": "Alpha", "text": "one"},
            "b": {"title": "Beta", "text": "two"},
        }
        selected, context = chat_rag_eval.render_context(["b", "a", "b"], documents, 5)
        self.assertEqual(selected, ["b", "a"])
        self.assertIn("[1] Document: Beta", context)
        self.assertIn("[2] Document: Alpha", context)

    def test_answer_requires_supported_valid_citation(self):
        query = {"id": "q", "relevance": {"a": 3}}
        expectation = {
            "required_patterns": [[r"fourteen|14"]],
            "required_citation_docs": ["a"],
        }
        passing = chat_rag_eval.evaluate_answer(
            query, expectation, "The period is 14 days [1].", ["a", "b"], 10, "m"
        )
        wrong_source = chat_rag_eval.evaluate_answer(
            query, expectation, "The period is 14 days [2].", ["a", "b"], 10, "m"
        )
        self.assertTrue(passing["answer_correct"])
        self.assertFalse(wrong_source["answer_correct"])

    def test_unanswerable_requires_abstention_without_citation(self):
        query = {"id": "q", "relevance": {}}
        expectation = {"expected_abstain": True}
        row = chat_rag_eval.evaluate_answer(
            query,
            expectation,
            "That information is not provided in the documents.",
            ["a"],
            10,
            "m",
        )
        self.assertTrue(row["answer_correct"])


if __name__ == "__main__":
    unittest.main()
