import unittest

from claim_checker_bakeoff import lettuce_support
from claim_checker_bakeoff_score import (
    best_threshold, binary_metrics, both_approve, score_checker, validate_dataset)


def pair(pair_id, label, tags=('paraphrase',), passage='The pump is on shelf R-14.'):
    return {'id': pair_id, 'claim': 'The pump is on shelf R-14.', 'passage': passage,
            'document_id': 'doc', 'label': label, 'difficulty': list(tags), 'note': 'n'}


class DatasetValidationTests(unittest.TestCase):
    def test_a_passage_must_be_a_verbatim_span_of_its_document(self):
        pairs = [pair('a', 'supported')]
        validate_dataset(pairs, {'doc': 'Spares. The pump is on shelf R-14. Seals are next door.'})
        with self.assertRaises(ValueError):
            validate_dataset(pairs, {'doc': 'The pump is on shelf R-41.'})

    def test_partial_cannot_label_a_contradiction(self):
        validate_dataset([pair('a', 'unsupported', ('partial',))])
        with self.assertRaises(ValueError):
            validate_dataset([pair('a', 'contradicted', ('partial',))])

    def test_malformed_pairs_are_rejected(self):
        broken = [
            [pair('a', 'nonfactual')],
            [pair('a', 'supported', ('made_up_tag',))],
            [pair('a', 'supported'), pair('a', 'supported')],
            [{**pair('a', 'supported'), 'claim': 'one line\nand another'}],
            [{**pair('a', 'supported'), 'difficulty': []}],
        ]
        for pairs in broken:
            with self.assertRaises(ValueError):
                validate_dataset(pairs)


class ScoringTests(unittest.TestCase):
    def setUp(self):
        self.pairs = validate_dataset([
            pair('s1', 'supported'), pair('s2', 'supported', ('table',)),
            pair('u1', 'unsupported'), pair('c1', 'contradicted', ('negation',))])

    def rows(self, verdicts, scores=None):
        return [{'id': pair_id, 'checker': 'x', 'verdict': verdict, 'latency_ms': 10.0,
                 'score_supported': None if scores is None else scores[pair_id]}
                for pair_id, verdict in verdicts.items()]

    def test_calling_everything_supported_is_chance_on_balanced_accuracy(self):
        result = score_checker(self.pairs, self.rows(dict.fromkeys(self.pairs, 'supported')))
        self.assertEqual(result['accuracy'], 0.5)
        self.assertEqual(result['balanced_accuracy'], 0.5)
        self.assertEqual(result['precision_supported'], 0.5)
        self.assertEqual(result['recall_unsupported'], 0.0)
        self.assertFalse(result['three_way'])

    def test_a_three_way_checker_is_scored_on_the_exact_label_too(self):
        verdicts = {'s1': 'supported', 's2': 'supported', 'u1': 'contradicted', 'c1': 'contradicted'}
        result = score_checker(self.pairs, self.rows(verdicts))
        self.assertEqual(result['accuracy'], 1.0)
        self.assertTrue(result['three_way'])
        self.assertEqual(result['three_way_accuracy'], 0.75)
        self.assertEqual(result['exact_recall_unsupported'], 0.0)
        self.assertEqual(result['confusion']['unsupported'], {'contradicted': 1})

    def test_unverified_is_counted_apart_and_never_as_supported(self):
        verdicts = {'s1': 'unverified', 's2': 'supported', 'u1': 'unverified', 'c1': 'contradicted'}
        result = score_checker(self.pairs, self.rows(verdicts))
        self.assertEqual(result['unverified'], 2)
        self.assertEqual(result['recall_supported'], 0.5)
        self.assertEqual(result['recall_unsupported'], 1.0)
        self.assertEqual(result['precision_supported'], 1.0)

    def test_every_pair_needs_exactly_one_result(self):
        verdicts = dict.fromkeys(self.pairs, 'supported')
        with self.assertRaises(ValueError):
            score_checker(self.pairs, self.rows(verdicts)[:-1])
        with self.assertRaises(ValueError):
            score_checker(self.pairs, self.rows(verdicts) + self.rows({'s1': 'supported'}))
        with self.assertRaises(ValueError):
            score_checker(self.pairs, self.rows({**verdicts, 'zz': 'supported'}))

    def test_by_tag_accuracy_follows_the_pairs_tags(self):
        verdicts = {'s1': 'supported', 's2': 'unsupported', 'u1': 'unsupported', 'c1': 'supported'}
        result = score_checker(self.pairs, self.rows(verdicts))
        self.assertEqual(result['by_tag']['table'], {'correct': 0, 'pairs': 1, 'accuracy': 0.0})
        self.assertEqual(result['by_tag']['negation']['accuracy'], 0.0)
        self.assertEqual(result['by_tag']['paraphrase']['accuracy'], 1.0)

    def test_the_best_threshold_separates_the_classes_when_one_exists(self):
        scores = {'s1': 0.9, 's2': 0.7, 'u1': 0.4, 'c1': 0.1}
        result = score_checker(self.pairs, self.rows(dict.fromkeys(self.pairs, 'supported'), scores))
        self.assertEqual(result['best_threshold']['threshold'], 0.7)
        self.assertEqual(result['best_threshold']['macro_f1'], 1.0)

    def test_a_missing_score_never_clears_a_threshold(self):
        best = best_threshold([True, False], [None, 0.2])
        self.assertEqual(best['recall_supported'], 0.0)

    def test_balanced_accuracy_averages_the_two_recalls(self):
        metrics = binary_metrics([True, True, True, False], [True, True, True, True])
        self.assertEqual(metrics['accuracy'], 0.75)
        self.assertEqual(metrics['balanced_accuracy'], 0.5)

    def test_a_gate_makes_two_kinds_of_error(self):
        metrics = binary_metrics([True, True, False, False], [True, False, True, False])
        self.assertEqual((metrics['false_approvals'], metrics['false_approval_rate']), (1, 0.5))
        self.assertEqual((metrics['false_blocks'], metrics['false_block_rate']), (1, 0.5))

    def test_errors_are_reported_per_subject_when_pairs_name_one(self):
        pairs = validate_dataset([{**pair('a', 'supported'), 'subject': 'music'},
                                  {**pair('b', 'unsupported'), 'subject': 'music'},
                                  {**pair('c', 'supported'), 'subject': 'sql'}])
        result = score_checker(pairs, [{'id': 'a', 'verdict': 'supported'},
                                       {'id': 'b', 'verdict': 'supported'},
                                       {'id': 'c', 'verdict': 'unsupported'}])
        self.assertEqual(result['by_subject']['music']['false_approvals'], 1)
        self.assertEqual(result['by_subject']['sql']['false_blocks'], 1)
        self.assertNotIn('by_origin', result)

    def test_an_and_gate_passes_only_what_both_checkers_support(self):
        first = [{'id': 'a', 'verdict': 'supported'}, {'id': 'b', 'verdict': 'supported'}]
        second = [{'id': 'a', 'verdict': 'supported'}, {'id': 'b', 'verdict': 'not_supported'}]
        rows = both_approve('x AND y', first, second)
        self.assertEqual([row['verdict'] for row in rows], ['supported', 'not_supported'])


class LettuceAggregationTests(unittest.TestCase):
    def test_one_hallucinated_token_makes_the_claim_unsupported(self):
        score, verdict = lettuce_support([
            {'token': 'The', 'pred': 0, 'prob': 0.02},
            {'token': ' 98', 'pred': 1, 'prob': 0.91}])
        self.assertAlmostEqual(score, 0.09)
        self.assertEqual(verdict, 'not_supported')

    def test_no_flagged_token_is_supported(self):
        score, verdict = lettuce_support([{'token': 'ok', 'pred': 0, 'prob': 0.3}])
        self.assertAlmostEqual(score, 0.7)
        self.assertEqual(verdict, 'supported')


if __name__ == '__main__':
    unittest.main()
