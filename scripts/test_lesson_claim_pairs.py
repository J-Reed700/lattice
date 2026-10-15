import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

import lesson_claim_pairs as pairs


def lesson_db(path, findings, sources):
    db = sqlite3.connect(path)
    db.executescript(
        'create table learning_sources (id text, program_id text, title text, url text, excerpt text, acquired_at integer);'
        'create table learning_source_versions (id text, source_id text, requested_url text, resolved_url text, full_text text);'
        'create table learning_lesson_verifications (lesson_id text, report_json text);')
    for source_id, url, text, version_source in sources:
        db.execute('insert into learning_sources values (?, ?, ?, ?, ?, ?)', (source_id, 'p', 'Title', url, '', 0))
        db.execute('insert into learning_source_versions values (?, ?, ?, ?, ?)',
                   (source_id + '-v', version_source, url, url, text))
    db.execute('insert into learning_lesson_verifications values (?, ?)',
               ('lesson-1', json.dumps({'findings': findings})))
    db.commit()
    db.close()


class ExtractTests(unittest.TestCase):
    def run_extract(self, findings, sources, **options):
        with tempfile.TemporaryDirectory() as directory:
            db = Path(directory) / 'lesson.sqlite'
            out = Path(directory) / 'pairs.jsonl'
            lesson_db(db, findings, sources)
            args = ['extract', '--db', str(db), '--subject', 'sql', '--out', str(out)]
            for key, value in options.items():
                args += [f'--{key}', str(value)]
            pairs.main(args)
            return [json.loads(line) for line in out.read_text().splitlines()]

    def test_the_cited_passage_and_the_closest_uncited_one_become_pairs(self):
        cited = 'Indexes speed up reads. A B-tree index supports equality and range queries.'
        other = 'Vacuum reclaims storage occupied by dead tuples.'
        finding = {'statement': 'A B-tree index supports range queries [sourceIndex: 2].', 'verdict': 'supported',
                   'supporting_quote': cited,
                   'evidence': [{'source_id': 's1', 'text': cited}, {'source_id': 's1', 'text': other}]}
        rows = self.run_extract([finding], [('s1', 'https://example.org/a', f'{cited}\n{other}', 's1')])
        by_origin = {row['origin']: row for row in rows}
        self.assertEqual(by_origin['approved']['passage'], cited)
        self.assertEqual(by_origin['related']['passage'], other)
        self.assertEqual(by_origin['approved']['claim'], 'A B-tree index supports range queries.')

    def test_a_version_saved_under_the_library_copy_is_found_by_url(self):
        cited = 'WAL records every change before it reaches the data files.'
        finding = {'statement': 'WAL records changes first.', 'verdict': 'supported', 'supporting_quote': cited,
                   'evidence': [{'source_id': 's1', 'text': cited}]}
        rows = self.run_extract([finding], [('s1', 'https://example.org/wal', cited, 'library-copy')])
        self.assertEqual([row['origin'] for row in rows], ['approved'])

    def test_a_passage_missing_from_every_stored_text_is_dropped(self):
        finding = {'statement': 'X.', 'verdict': 'supported', 'supporting_quote': 'Not stored anywhere.',
                   'evidence': [{'source_id': 's1', 'text': 'Not stored anywhere.'}]}
        self.assertEqual(self.run_extract([finding], [('s1', 'https://example.org/x', 'Other text.', 's1')]), [])


class AssembleTests(unittest.TestCase):
    def test_agreement_is_gold_and_a_disagreement_waits_for_adjudication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = {'passage': 'p', 'document_id': 'd', 'subject': 'sql', 'origin': 'approved'}
            (root / 'pairs.jsonl').write_text('\n'.join(json.dumps({'id': i, 'claim': i, **base})
                                                        for i in ('a', 'b', 'c')))
            label = lambda i, value: json.dumps({'id': i, 'label': value, 'difficulty': ['paraphrase'], 'note': 'n'})
            (root / 'first.jsonl').write_text('\n'.join([label('a', 'supported'), label('b', 'supported'),
                                                         label('c', 'unsupported')]))
            (root / 'second.jsonl').write_text('\n'.join([label('a', 'supported'), label('b', 'unsupported'),
                                                          label('c', 'unsupported')]))
            pairs.main(['assemble', '--pairs', str(root / 'pairs.jsonl'), '--labels', str(root / 'first.jsonl'),
                        str(root / 'second.jsonl'), '--out', str(root / 'gold.jsonl'),
                        '--disagreements', str(root / 'open.jsonl')])
            gold = [json.loads(line) for line in (root / 'gold.jsonl').read_text().splitlines()]
            self.assertEqual([(row['id'], row['label']) for row in gold], [('a', 'supported'), ('c', 'unsupported')])
            self.assertEqual(json.loads((root / 'open.jsonl').read_text())['id'], 'b')

    def test_kappa_is_one_for_identical_labels_and_zero_at_chance(self):
        self.assertEqual(pairs.kappa(['x', 'y'], ['x', 'y']), 1.0)
        self.assertAlmostEqual(pairs.kappa(['x', 'x', 'y', 'y'], ['x', 'y', 'x', 'y']), 0.0)


if __name__ == '__main__':
    unittest.main()
