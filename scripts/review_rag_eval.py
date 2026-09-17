#!/usr/bin/env python3
"""Export blinded review packets and score evidence-bound human judgments.

This validates review provenance and evidence locations, not semantic entailment.
Reviewers must check every claim, including statements without citations.
"""
import argparse
import json
from pathlib import Path

from chat_rag_eval import aggregate, evaluate_answer, load_jsonl
from eval_integrity import digest, finite_number, unique_records, validate_expectations
from rag_eval import validate_dataset, query_dimensions

RUBRIC = 'lattice-claim-review-v1'


def packets(dataset, rows):
    validate_dataset(dataset)
    queries = unique_records(dataset['queries'], 'id')
    documents = unique_records(dataset['documents'], 'id')
    unique_records(rows, 'query_id')
    result = []
    for row in rows:
        qid = row['query_id']
        if qid not in queries or row.get('answer_sha256') != digest(row['answer']):
            raise ValueError('Unknown query or answer hash mismatch')
        if not isinstance(row.get('run_id'), str) or not row['run_id']:
            raise ValueError('Review requires a manifested run')
        selected = row['ranked_ids']
        if len(set(selected)) != len(selected) or not set(selected).issubset(documents):
            raise ValueError('Invalid context document IDs')
        packet = {'rubric': RUBRIC, 'query_id': qid, 'query': queries[qid],
                  'answer': row['answer'], 'answer_sha256': row['answer_sha256'],
                  'run_id': row['run_id'],
                  'sources': [{'citation': i, **documents[doc]} for i, doc in enumerate(selected, 1)]}
        packet['packet_sha256'] = digest(packet)
        result.append(packet)
    return result


def apply_review(packet, review):
    if packet.get('answer_sha256') != digest(packet['answer']) or packet.get('packet_sha256') != digest({
            k: v for k, v in packet.items() if k != 'packet_sha256'}):
        raise ValueError('Evidence packet was modified')
    for field in ('rubric', 'query_id', 'answer_sha256', 'run_id', 'packet_sha256'):
        if review.get(field) != packet[field]:
            raise ValueError(f'Review {field} does not match its evidence packet')
    if review.get('reviewer_type') != 'human' or not str(review.get('reviewer', '')).strip():
        raise ValueError('A named human reviewer is required for verified correctness')
    for field in ('complete', 'correct', 'abstention_appropriate'):
        if type(review.get(field)) is not bool:
            raise ValueError(f'Review requires boolean {field}')
    if not isinstance(review.get('rationale'), str) or not review['rationale'].strip():
        raise ValueError('Review requires a rationale')
    answer = packet['answer']
    spans = review.get('spans')
    if not isinstance(spans, list) or not spans:
        raise ValueError('Review must classify the entire answer using spans')
    covered = set()
    supported = 0
    factual = 0
    sources = {source['id']: source['text'] for source in packet['sources']}
    for span in spans:
        start, end = span.get('start'), span.get('end')
        if type(start) is not int or type(end) is not int or not 0 <= start < end <= len(answer):
            raise ValueError('Invalid answer span offsets')
        positions = set(range(start, end))
        if covered & positions:
            raise ValueError('Review spans overlap')
        covered |= positions
        verdict = span.get('verdict')
        if verdict not in ('supported', 'unsupported', 'contradicted', 'nonfactual'):
            raise ValueError('Unknown claim verdict')
        if not isinstance(span.get('rationale'), str) or not span['rationale'].strip():
            raise ValueError('Every span needs a rationale')
        factual += verdict != 'nonfactual'
        supported += verdict == 'supported'
        evidence = span.get('evidence', [])
        if not isinstance(evidence, list) or (verdict == 'supported' and not evidence):
            raise ValueError('Supported claims require exact source evidence')
        for item in evidence:
            doc, quote = item.get('document_id'), item.get('quote')
            if doc not in sources or not isinstance(quote, str) or not quote.strip() or quote not in sources[doc]:
                raise ValueError('Evidence quote must occur verbatim in a provided source')
        if type(span.get('citations_support_claim')) is not bool:
            raise ValueError('Each span needs a citation support judgment')
    if any(i not in covered for i, char in enumerate(answer) if not char.isspace()):
        raise ValueError('Review leaves answer text unexamined')
    grounded = supported == factual
    citations = all(s['citations_support_claim'] for s in spans if s['verdict'] != 'nonfactual')
    answerable = any(g > 0 for g in packet['query']['relevance'].values())
    passed = (review['complete'] and review['correct'] and review['abstention_appropriate']
              and grounded and citations and (not answerable or factual > 0))
    return {'answer_correct': passed, 'review_status': 'reviewed',
            'supported_claims': supported, 'total_claims': factual,
            'review_sha256': digest(review), 'reviewer': review['reviewer']}


def score_reviews(dataset, expectations, rows, reviews):
    labels = validate_expectations(dataset, expectations)
    by_id = unique_records(reviews, 'query_id')
    packet_list = packets(dataset, rows)
    if not set(by_id).issubset(p['query_id'] for p in packet_list):
        raise ValueError('Review refers to an absent answer')
    scored = []
    for packet, row in zip(packet_list, rows):
        # Never trust cached PASS flags in imported artifacts.
        fresh = evaluate_answer(packet['query'], labels[row['query_id']], row['answer'],
                                row['ranked_ids'], row['latency_ms'], row.get('model'))
        if row['query_id'] in by_id:
            fresh.update(apply_review(packet, by_id[row['query_id']]))
            fresh['answer_correct'] &= fresh['citation_structure_passed']
        scored.append(fresh)
    report = aggregate(scored, len(dataset['queries']))
    report['dataset_sha256'] = digest(dataset)
    report['review_grader_sha256'] = digest(Path(__file__).read_text())
    report['reviews_sha256'] = digest(reviews)
    report['by_dimension'] = {
        tag: aggregate([r for r in scored if tag in r['dimensions']],
                       sum(tag in query_dimensions(q) for q in dataset['queries']))
        for tag in sorted({tag for q in dataset['queries'] for tag in query_dimensions(q)})}
    return report


def gate(report, policy):
    """Fail closed on missing evidence. Policy is chosen before examining results."""
    if type(policy.get('min_queries')) is not int or policy['min_queries'] < 2:
        raise ValueError('Policy min_queries must be an integer >= 2')
    finite_number(policy.get('min_accuracy_lower_bound'), 'min_accuracy_lower_bound')
    if policy['min_accuracy_lower_bound'] > 1:
        raise ValueError('Accuracy threshold cannot exceed 1')
    latency_budget = policy.get('max_p95_latency_ms')
    if latency_budget is not None:
        finite_number(latency_budget, 'max_p95_latency_ms')
    failures = []
    if report.get('coverage') != 1 or report.get('review_coverage') != 1:
        failures.append('Full answer and human review coverage required')
    for name, bucket in [('overall', report)] + list(report.get('by_dimension', {}).items()):
        if bucket.get('reviewed_queries', 0) < policy['min_queries']:
            failures.append(f'{name}: insufficient sample size')
        ci = bucket.get('answer_accuracy_95ci')
        if ci is None or ci[0] < policy['min_accuracy_lower_bound']:
            failures.append(f'{name}: accuracy lower confidence bound below policy')
        latency = bucket.get('p95_latency_ms')
        if latency_budget is not None and (latency is None or latency > latency_budget):
            failures.append(f'{name}: latency budget exceeded or unavailable')
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=Path)
    parser.add_argument('run', type=Path)
    parser.add_argument('--export', type=Path)
    parser.add_argument('--expectations', type=Path)
    parser.add_argument('--reviews', type=Path)
    parser.add_argument('--policy', type=Path)
    args = parser.parse_args()
    dataset, rows = json.loads(args.dataset.read_text()), load_jsonl(args.run)
    sidecar = args.run.with_suffix(args.run.suffix + '.manifest.json')
    provenance = json.loads(sidecar.read_text())
    identity = {key: provenance[key] for key in ('schema_version', 'dataset_sha256',
                'expectations_sha256', 'retrieval_sha256', 'settings', 'harness_sha256')}
    if provenance['run_id'] != digest(identity) or provenance['dataset_sha256'] != digest(dataset):
        raise ValueError('Manifest identity or dataset mismatch')
    if any(row.get('run_id') != provenance['run_id'] for row in rows):
        raise ValueError('Mixed or incompatible runs')
    if args.expectations and provenance['expectations_sha256'] != digest(json.loads(args.expectations.read_text())):
        raise ValueError('Expectation labels differ from the run manifest')
    if args.export:
        with args.export.open('x') as out:
            for packet in packets(dataset, rows):
                out.write(json.dumps(packet, ensure_ascii=False) + '\n')
        return
    if not args.expectations or not args.reviews:
        parser.error('Scoring requires --expectations and --reviews')
    report = score_reviews(dataset, json.loads(args.expectations.read_text()), rows, load_jsonl(args.reviews))
    failures = gate(report, json.loads(args.policy.read_text())) if args.policy else []
    report['gate_failures'] = failures
    print(json.dumps(report, indent=2, allow_nan=False))
    raise SystemExit(1 if failures else 0)


if __name__ == '__main__':
    main()
