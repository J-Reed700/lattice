"""Guards on the synthetic-library-v3 generator and the fixture it commits."""
import json
import unittest
from pathlib import Path

import build_synthetic_library_v3 as generator
from rag_eval import validate_dataset

FIXTURE = Path(__file__).resolve().parents[1] / 'evals/retrieval/synthetic-library-v3.json'


class SyntheticLibraryV3Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dataset = generator.build()
        cls.text = generator.render(cls.dataset)

    def test_the_committed_fixture_is_what_the_generator_produces(self):
        self.assertTrue(FIXTURE.exists(), 'run scripts/build_synthetic_library_v3.py')
        self.assertEqual(FIXTURE.read_text(encoding='utf-8'), self.text,
                         'the fixture is stale or was hand-edited; regenerate it')

    def test_the_same_seed_produces_the_same_bytes(self):
        self.assertEqual(generator.render(generator.build()), self.text)

    def test_the_fixture_stays_within_its_size_and_coverage_budget(self):
        summary = generator.summary(self.dataset)
        self.assertTrue(300 <= summary['documents'] <= 400, summary['documents'])
        self.assertTrue(150 <= summary['queries'] <= 200, summary['queries'])
        self.assertGreaterEqual(summary['unanswerable_queries'], 20)
        self.assertTrue(1_500_000 <= summary['total_characters'] <= 3_000_000,
                        summary['total_characters'])
        # Comfortably under the point where a JSON fixture becomes a nuisance in git.
        self.assertLess(len(self.text.encode('utf-8')), 5_000_000)

    def test_every_dimension_carries_enough_queries_to_slice(self):
        counts = generator.summary(self.dataset)['queries_per_dimension']
        self.assertEqual(sorted(counts), sorted(generator.DIMENSIONS))
        for dimension, count in counts.items():
            self.assertGreaterEqual(count, generator.MIN_PER_DIMENSION, dimension)

    def test_the_long_documents_are_actually_long(self):
        lengths = sorted(len(doc['text']) for doc in self.dataset['documents']
                         if doc['collection'] == 'work/long-form')
        self.assertGreaterEqual(len(lengths), 12)
        # 5k tokens is roughly 20k characters of English.
        self.assertGreaterEqual(lengths[0], 20_000)
        self.assertGreaterEqual(lengths[-1], 60_000)

    def test_answer_spans_point_at_the_passage_that_answers_the_query(self):
        bodies = {doc['id']: doc['text'].encode('utf-8') for doc in self.dataset['documents']}
        queries = {query['id']: query for query in self.dataset['queries']}
        answerable = [q for q in self.dataset['queries'] if q['relevance']]
        self.assertEqual(len(answerable),
                         sum(1 for q in answerable if q.get('answer_spans')))
        for query in answerable:
            for document_id, spans in query['answer_spans'].items():
                self.assertGreater(query['relevance'][document_id], 0)
                for start, end in spans:
                    self.assertTrue(bodies[document_id][start:end].decode('utf-8').strip())

        # A bare identifier query must land on the passage that carries it.
        identifier = next(q for q in self.dataset['queries']
                          if q['category'] == 'bare identifier')
        document_id, spans = next(iter(identifier['answer_spans'].items()))
        start, end = spans[0]
        self.assertIn(identifier['text'], bodies[document_id][start:end].decode('utf-8'))

        # A multi-hop query needs both halves labelled.
        multihop = queries['v3-multihop-renewal-00']
        self.assertEqual(len(multihop['answer_spans']), 2)

    def test_the_validators_the_harness_uses_accept_the_fixture(self):
        validate_dataset(json.loads(self.text))
        generator.check_spans(self.dataset)


class IntegrityCheckTests(unittest.TestCase):
    """The build's own checks have to fail on the mistakes they exist to catch."""

    def library(self):
        lib = generator.Library()
        for name in ('a', 'b'):
            doc = generator.Doc(id=name, title=name, kind='note', collection='c')
            doc.mark('fact', f'Document {name} says something unremarkable.')
            lib.add(doc)
        lib.needle(['unremarkable'], ['a', 'b'])
        lib.ask('q', 'Is there anything notable here?', 'test', [generator.BURIED],
                {'a': 3}, {'a': ['fact']})
        lib.finalize()
        return lib

    def test_a_needle_that_leaked_into_another_document_fails_the_build(self):
        lib = self.library()
        lib.needles = [{'tokens': ['unremarkable'], 'docs': ['a']}]
        with self.assertRaisesRegex(ValueError, "'unremarkable' appears in"):
            generator.check_integrity(lib)

    def test_an_unanswerable_token_present_in_the_corpus_fails_the_build(self):
        lib = self.library()
        lib.absent.append('unremarkable')
        with self.assertRaisesRegex(ValueError, 'leaked into'):
            generator.check_integrity(lib)

    def test_a_query_quoting_its_own_answer_fails_the_build(self):
        lib = self.library()
        lib.queries[0]['text'] = 'Document a says something unremarkable, does it not?'
        with self.assertRaisesRegex(ValueError, 'quotes its own answer'):
            generator.check_integrity(lib)

    def test_an_answerable_query_without_a_passage_label_fails_the_build(self):
        lib = self.library()
        lib.queries[0]['spans'] = {}
        with self.assertRaisesRegex(ValueError, 'without an answer span'):
            generator.check_integrity(lib)

    def test_minted_tokens_are_never_substrings_of_one_another(self):
        tokens = generator.Tokens(generator.rng_for('test'))
        issued = [getattr(tokens, name)() for name in
                  ('error_code', 'purchase_order', 'ticket', 'signal', 'part', 'release',
                   'date', 'amount', 'percent', 'duration', 'measure', 'anchor')] * 1
        issued += [tokens.anchor() for _ in range(200)]
        for token in issued:
            self.assertEqual([other for other in issued if token in other], [token], token)


if __name__ == '__main__':
    unittest.main()
