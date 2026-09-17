import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import chat_rag_eval as chat
import compare_rag_eval as comparison
import eval_integrity as integrity
import rag_eval
import review_rag_eval as review


class IntegrityTests(unittest.TestCase):
    def setUp(self):
        self.dataset = {'documents': [{'id': 'a', 'text': 'The period is 14 days.'},
                                      {'id': 'b', 'text': 'An unrelated document.'}],
                        'queries': [{'id': 'q', 'text': 'How long?', 'relevance': {'a': 3},
                                     'dimensions': ['numeric']}]}
        self.labels = {'queries': [{'id': 'q', 'required_patterns': [['14']],
                                   'required_citation_docs': ['a']}]}
        self.row = chat.evaluate_answer(self.dataset['queries'][0], self.labels['queries'][0],
                                       'The period is 14 days [1].', ['a'], 10, 'hidden-model')
        self.row['run_id'] = 'run'
        self.packet = review.packets(self.dataset, [self.row])[0]
        self.judgment = {k: self.packet[k] for k in
                         ('rubric', 'query_id', 'answer_sha256', 'run_id', 'packet_sha256')}
        self.judgment.update({'reviewer_type': 'human', 'reviewer': 'reviewer-1',
                             'complete': True, 'correct': True, 'abstention_appropriate': True,
                             'rationale': 'Answers the duration question exactly.',
                             'spans': [{'start': 0, 'end': len(self.row['answer']),
                                        'verdict': 'supported', 'rationale': 'Exact duration in source.',
                                        'citations_support_claim': True,
                                        'evidence': [{'document_id': 'a', 'quote': '14 days'}]}]})

    def test_patterns_never_certify_semantics(self):
        for answer in ['The period is NOT 14 days [1].',
                       'The period is 14 days [1]. The moon is made of cheese.']:
            row = chat.evaluate_answer(self.dataset['queries'][0], self.labels['queries'][0],
                                       answer, ['a'], 1, 'm')
            self.assertIsNone(row['answer_correct'])
            self.assertIsNone(chat.aggregate([row])['answer_accuracy'])

    def test_empty_expectations_do_not_vacuously_pass(self):
        row = chat.evaluate_answer(self.dataset['queries'][0], {}, 'Cheese [1].', ['a'], 1, 'm')
        self.assertFalse(row['mechanical_checks_passed'])
        with self.assertRaises(ValueError):
            integrity.validate_expectations(self.dataset, {'queries': [{'id': 'q'}]})

    def test_abstention_with_hallucination_is_not_correct(self):
        row = chat.evaluate_answer({'id': 'u', 'relevance': {}}, {},
                                   'Not provided. The secret code is 12345.', ['a'], 1, 'm')
        self.assertIsNone(row['answer_correct'])

    def test_blind_packet_hides_model_and_scores(self):
        self.assertNotIn('model', self.packet)
        self.assertNotIn('mechanical_checks_passed', self.packet)

    def test_review_can_establish_correctness(self):
        result = review.score_reviews(self.dataset, self.labels, [self.row], [self.judgment])
        self.assertEqual(result['answer_accuracy'], 1)
        self.assertEqual(result['review_coverage'], 1)
        self.assertLess(result['answer_accuracy_95ci'][0], .5)

    def test_reject_stale_answer_review(self):
        self.packet['answer_sha256'] = integrity.digest('changed answer')
        with self.assertRaises(ValueError):
            review.apply_review(self.packet, self.judgment)

    def test_reject_fabricated_evidence(self):
        self.judgment['spans'][0]['evidence'][0]['quote'] = '30 days'
        with self.assertRaises(ValueError):
            review.apply_review(self.packet, self.judgment)

    def test_reject_evidence_from_unseen_document(self):
        self.judgment['spans'][0]['evidence'][0] = {'document_id': 'b', 'quote': 'unrelated'}
        with self.assertRaises(ValueError):
            review.apply_review(self.packet, self.judgment)

    def test_reject_unreviewed_suffix(self):
        self.packet['answer'] += ' The moon is cheese.'
        self.packet['answer_sha256'] = integrity.digest(self.packet['answer'])
        self.packet['packet_sha256'] = integrity.digest({k: v for k, v in self.packet.items() if k != 'packet_sha256'})
        for key in ('answer_sha256', 'packet_sha256'):
            self.judgment[key] = self.packet[key]
        with self.assertRaisesRegex(ValueError, 'unexamined'):
            review.apply_review(self.packet, self.judgment)

    def test_unsupported_and_contradicted_claims_fail(self):
        for verdict in ('unsupported', 'contradicted'):
            self.judgment['spans'][0]['verdict'] = verdict
            self.assertFalse(review.apply_review(self.packet, self.judgment)['answer_correct'])

    def test_model_judge_cannot_masquerade_as_human(self):
        self.judgment['reviewer_type'] = 'model'
        with self.assertRaises(ValueError):
            review.apply_review(self.packet, self.judgment)

    def test_nonfactual_only_answer_cannot_pass_answerable_query(self):
        self.judgment['spans'][0]['verdict'] = 'nonfactual'
        self.assertFalse(review.apply_review(self.packet, self.judgment)['answer_correct'])

    def test_missing_reviews_are_not_zeros_or_passes(self):
        report = review.score_reviews(self.dataset, self.labels, [self.row], [])
        self.assertIsNone(report['answer_accuracy'])
        self.assertTrue(review.gate(report, {'min_queries': 2, 'min_accuracy_lower_bound': .8,
                                            'max_p95_latency_ms': 1000}))

    def test_latency_is_informational_without_an_explicit_budget(self):
        report = {"coverage": 1, "review_coverage": 1, "reviewed_queries": 100,
                  "answer_accuracy_95ci": [.95, 1], "p95_latency_ms": 999999}
        policy = {"min_queries": 30, "min_accuracy_lower_bound": .9}
        self.assertEqual(review.gate(report, policy), [])
        self.assertTrue(review.gate(report, {**policy, "max_p95_latency_ms": 1000}))

    def test_gate_rejects_nan_threshold(self):
        with self.assertRaises(ValueError):
            review.gate({}, {'min_queries': 2, 'min_accuracy_lower_bound': float('nan'),
                             'max_p95_latency_ms': 1000})

    def test_cached_correctness_is_not_trusted(self):
        self.row.update({'answer_correct': True, 'review_status': 'reviewed'})
        self.assertIsNone(review.score_reviews(self.dataset, self.labels, [self.row], [])['answer_accuracy'])

    def test_manifest_resume_and_overwrite_protection(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'run.jsonl'
            current = {'run_id': 'one'}
            self.assertEqual(integrity.prepare_output(output, current, False), [])
            output.write_text(json.dumps({'query_id': 'q', 'run_id': 'one'}) + '\n')
            self.assertEqual(len(integrity.prepare_output(output, current, True)), 1)
            for resume, identity in [(False, current), (True, {'run_id': 'two'})]:
                with self.assertRaises(ValueError):
                    integrity.prepare_output(output, identity, resume)

    def test_manifest_identity_changes_with_prompt_inputs(self):
        first = integrity.manifest(self.dataset, self.labels, [], {'model': 'a'})
        second = integrity.manifest(self.dataset, self.labels, [], {'model': 'b'})
        self.assertNotEqual(first['run_id'], second['run_id'])
        self.assertNotIn('headers', first['settings'])

    def test_duplicate_records_fail_before_dictionary_conversion(self):
        with self.assertRaises(ValueError):
            integrity.unique_records([self.row, self.row], 'query_id')

    def test_unknown_documents_beyond_cutoff_are_rejected(self):
        with self.assertRaises(ValueError):
            rag_eval.score(self.dataset, [{'query_id': 'q', 'ranked_ids': ['a', 'bad'], 'latency_ms': 1}], 1)

    def test_boolean_latency_rejected(self):
        with self.assertRaises(ValueError):
            rag_eval.score(self.dataset, [{'query_id': 'q', 'ranked_ids': ['a'], 'latency_ms': True}])

    def test_truncated_generation_is_rejected(self):
        response = unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value = json.dumps(
            {'choices': [{'message': {'content': '14 days'}, 'finish_reason': 'length'}]}).encode()
        provider = {'model': 'm', 'temperature': 0, 'top_p': 1, 'top_k': 1,
                    'headers': {}, 'base_url': 'http://localhost:1'}
        with patch('urllib.request.urlopen', return_value=response):
            with self.assertRaisesRegex(RuntimeError, 'truncated'):
                chat.request_answer(provider, 'q', 'context', 1, 10)

    def test_cli_generation_resume_review_export_and_scoring(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dataset, labels, retrieval, output, config = [root / name for name in
                ('dataset.json', 'labels.json', 'retrieval.jsonl', 'answers.jsonl', 'config.json')]
            dataset.write_text(json.dumps(self.dataset))
            labels.write_text(json.dumps(self.labels))
            retrieval.write_text(json.dumps({'query_id': 'q', 'ranked_ids': ['a'], 'latency_ms': 1}) + '\n')
            config.write_text(json.dumps({'model': 'local/m', 'provider': {'local': {
                'options': {'baseURL': 'http://localhost:1', 'headers': {'Authorization': 'SECRET'}}}}}))
            argv = ['chat', str(dataset), str(retrieval), str(labels), str(output),
                    '--config', str(config), '--model-revision', 'weights-sha', '--delay', '0']
            with patch('sys.argv', argv), patch.object(chat, 'request_answer', return_value=(
                    self.row['answer'], 10, {'finish_reason': 'stop', 'usage': None, 'attempts': 1})) as request:
                with contextlib.redirect_stdout(io.StringIO()):
                    chat.main()
                request.assert_called_once()
            with patch('sys.argv', argv + ['--resume']), patch.object(chat, 'request_answer') as request:
                with contextlib.redirect_stdout(io.StringIO()):
                    chat.main()
                request.assert_not_called()
            self.assertNotIn('SECRET', output.with_suffix('.jsonl.manifest.json').read_text())
            packet_path = root / 'packets.jsonl'
            with patch('sys.argv', ['review', str(dataset), str(output), '--export', str(packet_path)]):
                review.main()
            packet = json.loads(packet_path.read_text())
            judgment = dict(self.judgment)
            for key in ('rubric', 'query_id', 'answer_sha256', 'run_id', 'packet_sha256'):
                judgment[key] = packet[key]
            reviews = root / 'reviews.jsonl'
            reviews.write_text(json.dumps(judgment) + '\n')
            with patch('sys.argv', ['review', str(dataset), str(output), '--expectations', str(labels),
                                    '--reviews', str(reviews)]):
                with contextlib.redirect_stdout(io.StringIO()) as stdout, self.assertRaises(SystemExit) as result:
                    review.main()
                self.assertEqual(result.exception.code, 0)
                self.assertEqual(json.loads(stdout.getvalue())['answer_accuracy'], 1)

    def test_paired_comparison_detects_regression(self):
        baseline = [{'query_id': 'q', 'ranked_ids': ['a'], 'latency_ms': 1}]
        candidate = [{'query_id': 'q', 'ranked_ids': ['b'], 'latency_ms': 1}]
        report = comparison.compare(self.dataset, [baseline], [candidate], samples=100)
        self.assertEqual(report['metrics']['recall_at_k']['paired_delta_95ci'], [-1, -1])

    def test_repeated_trials_do_not_inflate_query_count(self):
        run = [{'query_id': 'q', 'ranked_ids': ['a'], 'latency_ms': 1}]
        report = comparison.compare(self.dataset, [run]*3, [run]*3, samples=100)
        self.assertEqual(report['queries'], 1)
        self.assertEqual(report['trials_per_arm'], 3)
        self.assertEqual(report['metrics']['recall_at_k']['paired_delta_95ci'], [0, 0])

    def test_comparison_rejects_incomplete_runs(self):
        with self.assertRaises(ValueError):
            comparison.compare(self.dataset, [[]], [[]], samples=100)


if __name__ == '__main__':
    unittest.main()
