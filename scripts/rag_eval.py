#!/usr/bin/env python3
"""Score a labeled retrieval run. Missing queries fail instead of inflating averages."""
import argparse
import json
import math
from pathlib import Path


def query_dimensions(query):
    """Dimension tags for one query.

    `dimensions` is the fixture field; `tags` is accepted so runs exported from other
    harnesses do not have to be rewritten. A query without either is only counted in the
    overall metrics.
    """
    for field in ('dimensions', 'tags'):
        value = query.get(field)
        if value is not None:
            return list(value)
    return []


def validate_dataset(dataset):
    if not isinstance(dataset, dict):
        raise ValueError('Dataset must be a JSON object')
    documents = dataset.get('documents')
    queries = dataset.get('queries')
    if not isinstance(documents, list) or not documents:
        raise ValueError('Dataset must contain a nonempty documents list')
    if not isinstance(queries, list) or not queries:
        raise ValueError('Dataset must contain a nonempty queries list')

    def validate_records(records, label):
        ids = []
        for index, record in enumerate(records):
            if not isinstance(record, dict):
                raise ValueError(f'{label} {index} must be an object')
            record_id = record.get('id')
            text = record.get('text')
            if not isinstance(record_id, str) or not record_id.strip():
                raise ValueError(f'{label} {index} must have a nonempty string ID')
            if not isinstance(text, str) or not text.strip():
                raise ValueError(f'{label} {record_id} must have nonempty text')
            ids.append(record_id)
        if len(ids) != len(set(ids)):
            raise ValueError(f'{label} IDs must be unique')
        return set(ids)

    document_ids = validate_records(documents, 'Document')
    validate_records(queries, 'Query')
    for query in queries:
        relevance = query.get('relevance')
        if not isinstance(relevance, dict):
            raise ValueError(f"Query {query['id']} must have a relevance object")
        if not set(relevance).issubset(document_ids):
            raise ValueError(f"Query {query['id']} labels reference unknown documents")
        for document_id, grade in relevance.items():
            if isinstance(grade, bool) or not isinstance(grade, (int, float)):
                raise ValueError(
                    f"Query {query['id']} has a nonnumeric grade for {document_id}")
            if not math.isfinite(grade) or grade < 0:
                raise ValueError(
                    f"Query {query['id']} has an invalid grade for {document_id}")
        for field in ('dimensions', 'tags'):
            tags = query.get(field)
            if tags is None:
                continue
            if not isinstance(tags, list) or not tags:
                raise ValueError(f"Query {query['id']} must have a nonempty {field} list")
            for tag in tags:
                if not isinstance(tag, str) or not tag.strip():
                    raise ValueError(f"Query {query['id']} has an empty {field} entry")
            if len(tags) != len(set(tags)):
                raise ValueError(f"Query {query['id']} repeats a {field} entry")


def _mean(values):
    return sum(values) / len(values) if values else None


def _summarize(records, k):
    """Aggregate one bucket of per-query outcomes."""
    recall = [r['recall'] for r in records if r['recall'] is not None]
    return {
        'queries': len(records),
        'retrieval_queries': len(recall),
        'k': k,
        'recall_at_k': _mean(recall),
        'ndcg_at_k': _mean([r['ndcg'] for r in records if r['ndcg'] is not None]),
        'mrr_at_k': _mean([r['mrr'] for r in records if r['mrr'] is not None]),
        'retrieval_abstention_accuracy': _mean(
            [r['retrieval_abstention'] for r in records
             if r['retrieval_abstention'] is not None]),
    }


def _retrieval_top_score(row, qid):
    """The run's retrieval confidence, falling back to the first ranked score.

    Rows written before `top_score` existed still carry `scores`, so an older run can be
    scored for abstention without being re-run.
    """
    value = row.get('top_score')
    if value is None:
        scores = row.get('scores')
        if scores is None:
            return None
        value = scores[0] if scores else 0.0
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f'{qid}: top_score must be a number')
    if not math.isfinite(value):
        raise ValueError(f'{qid}: top_score must be finite')
    return value


def _retrieval_abstained(row, qid, field):
    """The run's own abstention prediction: the named field being false.

    Unlike `top_score`, this is a judgment the run already made, so there is nothing to
    fall back to. A row missing the field is a mismatched run file, not an older one.
    """
    if field not in row:
        raise ValueError(f'{qid}: row has no {field} field')
    value = row[field]
    if not isinstance(value, bool):
        raise ValueError(f'{qid}: {field} must be true or false')
    return not value


def score(dataset, runs, k=5, abstain_threshold=None, abstain_field=None):
    if k < 1:
        raise ValueError('k must be positive')
    if abstain_threshold is not None and abstain_field is not None:
        raise ValueError('Choose one retrieval abstention predictor, a threshold or a field')
    if abstain_threshold is not None and not math.isfinite(abstain_threshold):
        raise ValueError('Abstention threshold must be finite')
    validate_dataset(dataset)
    expected = {q['id']: q for q in dataset['queries']}
    actual = {r['query_id']: r for r in runs}
    if len(actual) != len(runs) or set(actual) != set(expected):
        raise ValueError('Run must contain exactly one result per dataset query')
    doc_ids = {d['id'] for d in dataset['documents']}
    latency = []
    totals = {'supported_claims': 0, 'total_claims': 0, 'correct_citations': 0, 'total_citations': 0}
    judged = 0
    abstentions = []
    records = []
    for qid, query in expected.items():
        row = actual[qid]
        all_ranked = row['ranked_ids']
        if not isinstance(all_ranked, list) or any(not isinstance(doc, str) for doc in all_ranked):
            raise ValueError(f'{qid}: ranked_ids must be a list of document IDs')
        ranked = all_ranked[:k]
        if len(set(all_ranked)) != len(all_ranked) or not set(all_ranked).issubset(doc_ids):
            raise ValueError(f'{qid}: duplicate or unknown document ID')
        relevance = query['relevance']
        positives = {doc for doc, grade in relevance.items() if grade > 0}
        if not set(relevance).issubset(doc_ids):
            raise ValueError(f'{qid}: labels reference unknown documents')
        record = {'dimensions': query_dimensions(query), 'recall': None, 'ndcg': None,
                  'mrr': None, 'retrieval_abstention': None}
        if positives:
            record['recall'] = len(set(ranked) & positives) / len(positives)
            gains = [2 ** relevance.get(doc, 0) - 1 for doc in ranked]
            dcg = sum(g / math.log2(i + 2) for i, g in enumerate(gains))
            ideal = sorted((2 ** g - 1 for g in relevance.values() if g > 0), reverse=True)[:k]
            record['ndcg'] = dcg / sum(g / math.log2(i + 2) for i, g in enumerate(ideal))
            record['mrr'] = next(
                (1 / (i + 1) for i, doc in enumerate(ranked) if doc in positives), 0)
        value = row['latency_ms']
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError('Latency must be finite and nonnegative')
        latency.append(value)
        if abstain_threshold is not None:
            top_score = _retrieval_top_score(row, qid)
            if top_score is not None:
                # A retrieval-side abstention is correct when a weak top hit coincides with
                # a query the corpus genuinely cannot answer.
                record['retrieval_abstention'] = (top_score < abstain_threshold) == (not positives)
        elif abstain_field is not None:
            record['retrieval_abstention'] = (
                _retrieval_abstained(row, qid, abstain_field) == (not positives))
        records.append(record)
        if 'abstained' in row:
            if type(row['abstained']) is not bool:
                raise ValueError('abstained must be boolean')
            abstentions.append(row['abstained'] == (not positives))
        if all(field in row for field in totals):
            for numerator, denominator in [('supported_claims', 'total_claims'), ('correct_citations', 'total_citations')]:
                if not 0 <= row[numerator] <= row[denominator]:
                    raise ValueError('Invalid human judgment counts')
            if any(type(row[field]) is not int for field in totals):
                raise ValueError('Human judgment counts must be integers')
            judged += 1
            for field in totals:
                totals[field] += row[field]
    latency.sort()
    dimensions = sorted({tag for record in records for tag in record['dimensions']})
    result = _summarize(records, k)
    result.update({
        'p95_latency_ms': latency[max(0, math.ceil(len(latency) * .95) - 1)] if latency else None,
        'answer_judgments': judged,
        'grounded_claim_fraction': totals['supported_claims'] / totals['total_claims'] if totals['total_claims'] else None,
        'citation_precision': totals['correct_citations'] / totals['total_citations'] if totals['total_citations'] else None,
        'abstention_accuracy': _mean(abstentions),
        'abstain_threshold': abstain_threshold,
        # Which rule produced `retrieval_abstention_accuracy`, so two runs are never
        # compared across predictors by accident. Null when the metric was withheld.
        'abstention_predictor': (
            'threshold' if abstain_threshold is not None
            else f'field:{abstain_field}' if abstain_field is not None
            else None),
        'by_dimension': {
            dimension: _summarize(
                [r for r in records if dimension in r['dimensions']], k)
            for dimension in dimensions},
    })
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=Path)
    parser.add_argument('run', type=Path, nargs='?')
    parser.add_argument('--k', type=int, default=5)
    predictor = parser.add_mutually_exclusive_group()
    predictor.add_argument(
        '--abstain-threshold', type=float, default=None,
        help='predict "unanswerable" when a run row\'s top_score falls below this value')
    predictor.add_argument(
        '--abstain-field', default=None, metavar='NAME',
        help='predict "unanswerable" when a run row\'s NAME field is false, e.g. the '
             'production-mode "sufficient" verdict; every row must carry it')
    parser.add_argument(
        '--validate-only', action='store_true',
        help='validate the dataset without requiring a model run')
    args = parser.parse_args()
    dataset = json.loads(args.dataset.read_text())
    validate_dataset(dataset)
    if args.validate_only:
        if args.run is not None:
            parser.error('a run file cannot be supplied with --validate-only')
        answerable = sum(bool(query['relevance']) for query in dataset['queries'])
        multi_relevant = sum(
            sum(grade > 0 for grade in query['relevance'].values()) > 1
            for query in dataset['queries'])
        counts = {}
        for query in dataset['queries']:
            for dimension in query_dimensions(query):
                counts[dimension] = counts.get(dimension, 0) + 1
        print(json.dumps({
            'name': dataset.get('name'),
            'documents': len(dataset['documents']),
            'queries': len(dataset['queries']),
            'answerable_queries': answerable,
            'unanswerable_queries': len(dataset['queries']) - answerable,
            'multi_relevant_queries': multi_relevant,
            'queries_without_dimensions': sum(
                not query_dimensions(query) for query in dataset['queries']),
            'queries_per_dimension': dict(sorted(counts.items())),
        }, indent=2, allow_nan=False))
        return
    if args.run is None:
        parser.error('run is required unless --validate-only is used')
    runs = [json.loads(line) for line in args.run.read_text().splitlines() if line.strip()]
    print(json.dumps(
        score(dataset, runs, args.k, args.abstain_threshold, args.abstain_field),
        indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
