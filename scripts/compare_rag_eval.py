#!/usr/bin/env python3
"""Paired, query-cluster bootstrap comparison of complete retrieval runs.

Repeated runs are averaged within query before resampling; trials are not treated
as independent new queries. Intervals describe this corpus, not all users.
"""
import argparse
import json
import random
import statistics
from pathlib import Path

from chat_rag_eval import load_jsonl
from eval_integrity import digest, finite_number
from rag_eval import score, query_dimensions


def interval(values, samples=5000, seed=2026):
    if not values:
        return None
    rng = random.Random(seed)
    draws = sorted(statistics.fmean(rng.choices(values, k=len(values))) for _ in range(samples))
    return [draws[int(.025 * (samples-1))], draws[int(.975 * (samples-1))]]


def compare(dataset, baseline, candidate, k=5, samples=5000, seed=2026):
    if not baseline or len(baseline) != len(candidate):
        raise ValueError('Provide equal, nonzero numbers of baseline and candidate trials')
    if samples < 100:
        raise ValueError('Use at least 100 bootstrap samples')
    for run in baseline + candidate:
        score(dataset, run, k)
    indexed = [[{r['query_id']: r for r in run} for run in arm] for arm in (baseline, candidate)]
    outcomes = []
    for query in dataset['queries']:
        if not any(g > 0 for g in query['relevance'].values()):
            continue
        arm_scores = []
        for arm in indexed:
            arm_scores.append([score({'documents': dataset['documents'], 'queries': [query]},
                                     [run[query['id']]], k) for run in arm])
        row = {'query_id': query['id'], 'dimensions': query_dimensions(query)}
        for metric in ('recall_at_k', 'ndcg_at_k', 'mrr_at_k', 'p95_latency_ms'):
            before, after = [statistics.fmean(r[metric] for r in arm) for arm in arm_scores]
            row[metric] = {'baseline': before, 'candidate': after, 'delta': after-before}
        row['candidate_full_recall_every_trial'] = all(r['recall_at_k'] == 1 for r in arm_scores[1])
        outcomes.append(row)

    def summarize(rows):
        return {'queries': len(rows), 'metrics': {
            metric: {'baseline': statistics.fmean(r[metric]['baseline'] for r in rows) if rows else None,
                     'candidate': statistics.fmean(r[metric]['candidate'] for r in rows) if rows else None,
                     'delta': statistics.fmean(r[metric]['delta'] for r in rows) if rows else None,
                     'paired_delta_95ci': interval([r[metric]['delta'] for r in rows], samples, seed)}
            for metric in ('recall_at_k', 'ndcg_at_k', 'mrr_at_k')},
            'full_recall_every_trial_fraction': statistics.fmean(r['candidate_full_recall_every_trial'] for r in rows) if rows else None}

    result = summarize(outcomes)
    def p95(arm):
        values = sorted(row['latency_ms'] for run in arm for row in run)
        return values[max(0, (95 * len(values) + 99) // 100 - 1)]
    result['p95_latency_ms'] = {'baseline': p95(baseline), 'candidate': p95(candidate)}
    result.update({'dataset_sha256': digest(dataset), 'baseline_sha256': digest(baseline),
                   'candidate_sha256': digest(candidate), 'k': k, 'trials_per_arm': len(baseline),
                   'bootstrap_samples': samples, 'seed': seed,
                   'unanswerable_queries_excluded': len(dataset['queries'])-len(outcomes),
                   'by_dimension': {tag: summarize([r for r in outcomes if tag in r['dimensions']])
                                    for tag in sorted({t for r in outcomes for t in r['dimensions']})},
                   'query_deltas': outcomes})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=Path)
    parser.add_argument('--baseline', type=Path, nargs='+', required=True)
    parser.add_argument('--candidate', type=Path, nargs='+', required=True)
    parser.add_argument('--k', type=int, default=5)
    parser.add_argument('--samples', type=int, default=5000)
    parser.add_argument('--seed', type=int, default=2026)
    parser.add_argument('--max-regression', type=float, default=0.02)
    parser.add_argument('--max-latency-ratio', type=float, default=None,
                        help='Optional latency gate; latency is informational by default')
    parser.add_argument('--min-queries', type=int, default=5)
    args = parser.parse_args()
    finite_number(args.max_regression, 'max-regression')
    if args.max_latency_ratio is not None:
        finite_number(args.max_latency_ratio, 'max-latency-ratio', minimum=1)
    if args.min_queries < 2:
        parser.error('min-queries must be >= 2')
    report = compare(json.loads(args.dataset.read_text()),
                     [load_jsonl(p) for p in args.baseline], [load_jsonl(p) for p in args.candidate],
                     args.k, args.samples, args.seed)
    failures = []
    latency = report['p95_latency_ms']
    if args.max_latency_ratio is not None and latency['candidate'] > latency['baseline'] * args.max_latency_ratio:
        failures.append('p95 latency exceeds allowed baseline ratio')
    for name, bucket in [('overall', report)] + list(report['by_dimension'].items()):
        if bucket['queries'] < args.min_queries:
            failures.append(f'{name}: too few queries for a release claim')
        for metric, values in bucket['metrics'].items():
            ci = values['paired_delta_95ci']
            if ci is None or ci[0] < -args.max_regression:
                failures.append(f'{name}: {metric} regression cannot be ruled out')
    report['gate_failures'] = failures
    print(json.dumps(report, indent=2, allow_nan=False))
    raise SystemExit(1 if failures else 0)


if __name__ == '__main__':
    main()
