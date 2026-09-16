import unittest
from rag_eval import query_dimensions, score, validate_dataset


class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.dataset = {'documents': [
            {'id': 'a', 'text': 'Alpha'}, {'id': 'b', 'text': 'Beta'},
            {'id': 'c', 'text': 'Gamma'}], 'queries': [
            {'id': 'q', 'text': 'Question', 'relevance': {'a': 2, 'b': 1}},
            {'id': 'unknown', 'text': 'Unknown', 'relevance': {}}]}
        self.rows = [{'query_id': 'q', 'ranked_ids': ['a', 'b'], 'latency_ms': 10},
                     {'query_id': 'unknown', 'ranked_ids': [], 'latency_ms': 30}]

    def test_perfect_retrieval_does_not_invent_answer_quality(self):
        result = score(self.dataset, self.rows, 2)
        self.assertEqual(result['recall_at_k'], 1)
        self.assertEqual(result['ndcg_at_k'], 1)
        self.assertEqual(result['p95_latency_ms'], 30)
        self.assertIsNone(result['citation_precision'])
        self.assertIsNone(result['abstention_accuracy'])
        self.assertIsNone(result['retrieval_abstention_accuracy'])
        self.assertEqual(result['by_dimension'], {})

    def test_ranking_and_cutoff_matter(self):
        self.rows[0]['ranked_ids'] = ['c', 'b', 'a']
        result = score(self.dataset, self.rows, 2)
        self.assertEqual(result['recall_at_k'], .5)
        self.assertEqual(result['mrr_at_k'], .5)
        self.assertLess(result['ndcg_at_k'], .2)

    def test_missing_and_duplicate_queries_fail(self):
        for rows in [self.rows[:1], self.rows + self.rows[:1]]:
            with self.assertRaises(ValueError):
                score(self.dataset, rows)

    def test_grounding_and_abstention_require_judgments(self):
        self.rows[0].update(supported_claims=3, total_claims=4, correct_citations=1, total_citations=2, abstained=False)
        self.rows[1]['abstained'] = True
        result = score(self.dataset, self.rows)
        self.assertEqual(result['grounded_claim_fraction'], .75)
        self.assertEqual(result['citation_precision'], .5)
        self.assertEqual(result['abstention_accuracy'], 1)

    def test_dataset_validation_rejects_bad_structure(self):
        validate_dataset(self.dataset)

        duplicate = {'documents': self.dataset['documents'] + [
            {'id': 'a', 'text': 'Duplicate'}], 'queries': self.dataset['queries']}
        with self.assertRaisesRegex(ValueError, 'IDs must be unique'):
            validate_dataset(duplicate)

        unknown_label = {'documents': self.dataset['documents'], 'queries': [
            {'id': 'q', 'text': 'Question', 'relevance': {'missing': 1}}]}
        with self.assertRaisesRegex(ValueError, 'unknown documents'):
            validate_dataset(unknown_label)

        empty_text = {'documents': [{'id': 'a', 'text': ' '}],
                      'queries': self.dataset['queries']}
        with self.assertRaisesRegex(ValueError, 'nonempty text'):
            validate_dataset(empty_text)

    def test_production_mode_fields_do_not_disturb_older_rows(self):
        self.rows[0].update(
            mode='production', top_score=.031, score_spread=.012,
            chunk_ranked_ids=['a#0', 'b#1'],
            branch_ranked_ids={'vector': ['a', 'b'], 'bm25': ['b', 'a'], 'sparse': ['a']},
            embedding_strategy='late_chunking', compression='mrl256i8', sparse=True,
            sufficient=True, sufficiency_reasons=[], term_coverage=1.0)
        result = score(self.dataset, self.rows, 2)
        self.assertEqual(result['recall_at_k'], 1)
        self.assertEqual(result['queries'], 2)

    def test_dimension_breakdown_uses_dimensions_or_tags(self):
        self.dataset['queries'][0]['dimensions'] = ['paraphrase', 'hard negative']
        self.dataset['queries'][1]['tags'] = ['unanswerable']
        result = score(self.dataset, self.rows, 2)
        self.assertEqual(sorted(result['by_dimension']),
                         ['hard negative', 'paraphrase', 'unanswerable'])
        self.assertEqual(result['by_dimension']['paraphrase']['recall_at_k'], 1)
        self.assertEqual(result['by_dimension']['paraphrase']['queries'], 1)
        # An unanswerable query contributes no retrieval metrics, only a query count.
        self.assertEqual(result['by_dimension']['unanswerable']['retrieval_queries'], 0)
        self.assertIsNone(result['by_dimension']['unanswerable']['recall_at_k'])

    def test_dimension_lists_must_be_wellformed(self):
        for bad in [[], 'paraphrase', ['paraphrase', ''], ['same', 'same']]:
            self.dataset['queries'][0]['dimensions'] = bad
            with self.assertRaises(ValueError):
                validate_dataset(self.dataset)

    def test_dimensions_field_wins_over_tags(self):
        self.assertEqual(
            query_dimensions({'dimensions': ['a'], 'tags': ['b']}), ['a'])
        self.assertEqual(query_dimensions({'tags': ['b']}), ['b'])
        self.assertEqual(query_dimensions({}), [])

    def test_retrieval_abstention_compares_top_score_with_threshold(self):
        self.rows[0]['top_score'] = .04
        self.rows[1]['top_score'] = .005
        result = score(self.dataset, self.rows, 2, abstain_threshold=.02)
        self.assertEqual(result['retrieval_abstention_accuracy'], 1)
        self.assertEqual(result['abstain_threshold'], .02)

        # A confident top hit on a question the corpus cannot answer is a false answer.
        self.rows[1]['top_score'] = .09
        self.assertEqual(
            score(self.dataset, self.rows, 2, abstain_threshold=.02)['retrieval_abstention_accuracy'], .5)

        # Without a threshold the metric is withheld rather than guessed.
        self.assertIsNone(score(self.dataset, self.rows, 2)['retrieval_abstention_accuracy'])

    def test_retrieval_abstention_falls_back_to_the_first_ranked_score(self):
        self.rows[0]['scores'] = [.9, .4]
        self.rows[1]['scores'] = []
        result = score(self.dataset, self.rows, 2, abstain_threshold=.5)
        self.assertEqual(result['retrieval_abstention_accuracy'], 1)

    def test_retrieval_abstention_is_skipped_when_a_row_carries_no_scores(self):
        result = score(self.dataset, self.rows, 2, abstain_threshold=.5)
        self.assertIsNone(result['retrieval_abstention_accuracy'])

    def test_nonfinite_top_score_fails(self):
        self.rows[0]['top_score'] = float('nan')
        with self.assertRaisesRegex(ValueError, 'top_score'):
            score(self.dataset, self.rows, 2, abstain_threshold=.02)

    def test_abstain_field_reads_the_runs_own_verdict(self):
        self.rows[0]['sufficient'] = True
        self.rows[1]['sufficient'] = False
        result = score(self.dataset, self.rows, 2, abstain_field='sufficient')
        self.assertEqual(result['retrieval_abstention_accuracy'], 1)
        self.assertEqual(result['abstention_predictor'], 'field:sufficient')
        # The field predictor leaves the threshold unset rather than implying one.
        self.assertIsNone(result['abstain_threshold'])

        # Calling a corpus miss sufficient is a false answer, exactly as a confident
        # top score would be.
        self.rows[1]['sufficient'] = True
        self.assertEqual(
            score(self.dataset, self.rows, 2,
                  abstain_field='sufficient')['retrieval_abstention_accuracy'], .5)

    def test_abstain_field_must_be_present_and_boolean_on_every_row(self):
        self.rows[0]['sufficient'] = True
        with self.assertRaisesRegex(ValueError, 'no sufficient field'):
            score(self.dataset, self.rows, 2, abstain_field='sufficient')

        self.rows[1]['sufficient'] = 0.4
        with self.assertRaisesRegex(ValueError, 'sufficient must be true or false'):
            score(self.dataset, self.rows, 2, abstain_field='sufficient')

    def test_the_two_abstention_predictors_are_mutually_exclusive(self):
        self.rows[0].update(top_score=.04, sufficient=True)
        self.rows[1].update(top_score=.005, sufficient=False)
        with self.assertRaisesRegex(ValueError, 'one retrieval abstention predictor'):
            score(self.dataset, self.rows, 2,
                  abstain_threshold=.02, abstain_field='sufficient')

    def test_abstention_predictor_is_null_when_the_metric_is_withheld(self):
        result = score(self.dataset, self.rows, 2)
        self.assertIsNone(result['abstention_predictor'])
        self.assertIsNone(result['retrieval_abstention_accuracy'])
        self.assertEqual(
            score(self.dataset, self.rows, 2,
                  abstain_threshold=.5)['abstention_predictor'], 'threshold')


if __name__ == '__main__':
    unittest.main()
