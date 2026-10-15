"""Score claim checkers against the labelled claim/passage pairs.

Standard library only. Reads the dataset and any number of result files (one
JSON line per pair, each with `id`, `checker`, `verdict` and optionally
`score_supported` and `latency_ms`) and reports, per checker:

* binary metrics, with "supported" as the positive class: accuracy, balanced
  accuracy, precision and recall on "supported", recall on each negative gold
  class (an unsupported or contradicted claim counts as caught when the
  checker does not call it supported), and macro-F1;
* three-way accuracy for checkers that separate "unsupported" from
  "contradicted" (Lattice's does; the external models are binary);
* a confusion matrix of gold label against verdict, "unverified" included;
* latency per pair;
* the best macro-F1 threshold on `score_supported`, which is chosen on the
  same pairs it is scored on and is therefore optimistic.

A verdict of "unverified" (Lattice's "not checked") is not a "supported"
verdict, so in the binary view it falls on the negative side. Its count is
reported separately so that a cautious checker is not mistaken for an
accurate one.

    python3 scripts/claim_checker_bakeoff_score.py \
        --library evals/retrieval/synthetic-library-v3.json \
        evals/grounding/results/2026-10-12-*.jsonl
"""
import argparse
import json
import random
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DATASET = ROOT / 'evals' / 'grounding' / 'claim-pairs-v1.jsonl'

LABELS = ('supported', 'unsupported', 'contradicted')
TAGS = frozenset({
    'verbatim', 'paraphrase', 'number_drift', 'date_drift', 'unit_drift', 'negation',
    'reversal', 'wrong_entity', 'partial', 'multi_clause', 'multi_sentence',
    'world_knowledge', 'distractor_numbers', 'lexical_trap', 'causal_addition', 'scope',
    'hedge', 'table', 'arithmetic', 'noisy_passage', 'cross_lingual',
})
# "partial" means one clause is supported and another is missing, which makes
# the claim unsupported. A clause the passage rules out makes it contradicted
# instead, and that case is tagged "multi_clause".
PARTIAL_LABELS = frozenset({'supported', 'unsupported'})
# Optional pair fields reported separately when a dataset has them.
BREAKDOWN_FIELDS = ('subject', 'origin')
BOOTSTRAP_RESAMPLES = 2000
BOOTSTRAP_SEED = 20261012


def load_jsonl(path):
    rows = []
    for number, line in enumerate(Path(path).read_text(encoding='utf-8').splitlines(), start=1):
        if line.strip():
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise ValueError(f'{path}:{number}: {error}') from error
    return rows


def validate_dataset(pairs, documents=None):
    """Reject a malformed dataset; return the pairs keyed by id.

    With `documents` (id -> text), every passage must also be a verbatim span
    of the document it names, so a label can always be checked at the source.
    """
    by_id = {}
    for pair in pairs:
        pair_id = pair.get('id')
        if not isinstance(pair_id, str) or not pair_id:
            raise ValueError('every pair needs a nonempty id')
        if pair_id in by_id:
            raise ValueError(f'duplicate id {pair_id}')
        for field in ('claim', 'passage', 'document_id', 'note'):
            if not isinstance(pair.get(field), str) or not pair[field].strip():
                raise ValueError(f'{pair_id}: {field} must be a nonempty string')
        if '\n' in pair['claim']:
            raise ValueError(f'{pair_id}: a claim must be one line')
        if pair.get('label') not in LABELS:
            raise ValueError(f'{pair_id}: label must be one of {LABELS}')
        tags = pair.get('difficulty')
        if not isinstance(tags, list) or not tags:
            raise ValueError(f'{pair_id}: difficulty must be a nonempty list of tags')
        unknown = set(tags) - TAGS
        if unknown:
            raise ValueError(f'{pair_id}: unknown difficulty tags {sorted(unknown)}')
        if 'partial' in tags and pair['label'] not in PARTIAL_LABELS:
            raise ValueError(f'{pair_id}: "partial" applies to supported or unsupported pairs only')
        if documents is not None:
            text = documents.get(pair['document_id'])
            if text is None:
                raise ValueError(f'{pair_id}: unknown document {pair["document_id"]}')
            if pair['passage'] not in text:
                raise ValueError(f'{pair_id}: passage is not a verbatim span of {pair["document_id"]}')
        by_id[pair_id] = pair
    return by_id


def is_supported(verdict):
    return verdict == 'supported'


def _ratio(numerator, denominator):
    return numerator / denominator if denominator else None


def _f1(true_positive, false_positive, false_negative):
    denominator = 2 * true_positive + false_positive + false_negative
    return 2 * true_positive / denominator if denominator else None


def binary_metrics(gold_supported, predicted_supported):
    """Metrics for parallel lists of booleans, "supported" being positive."""
    pairs = list(zip(gold_supported, predicted_supported))
    tp = sum(1 for gold, pred in pairs if gold and pred)
    fp = sum(1 for gold, pred in pairs if not gold and pred)
    fn = sum(1 for gold, pred in pairs if gold and not pred)
    tn = sum(1 for gold, pred in pairs if not gold and not pred)
    recall_supported = _ratio(tp, tp + fn)
    recall_flagged = _ratio(tn, tn + fp)
    f1_supported = _f1(tp, fp, fn)
    f1_flagged = _f1(tn, fn, fp)
    recalls = [value for value in (recall_supported, recall_flagged) if value is not None]
    f1s = [value for value in (f1_supported, f1_flagged) if value is not None]
    return {
        'accuracy': _ratio(tp + tn, len(pairs)),
        'balanced_accuracy': sum(recalls) / len(recalls) if recalls else None,
        'precision_supported': _ratio(tp, tp + fp),
        'recall_supported': recall_supported,
        'f1_supported': f1_supported,
        'macro_f1': sum(f1s) / len(f1s) if f1s else None,
        # The two errors a gate makes: passing a claim the passage does not
        # back, and stopping one it does.
        'false_approvals': fp, 'false_approval_rate': _ratio(fp, fp + tn),
        'false_blocks': fn, 'false_block_rate': _ratio(fn, tp + fn),
    }


def best_threshold(gold_supported, scores):
    """The P(supported) cut that maximises macro-F1 on these same pairs.

    A missing score never clears a threshold. Ties keep the lowest cut.
    """
    candidates = sorted({score for score in scores if score is not None})
    best = None
    for cut in candidates:
        predicted = [score is not None and score >= cut for score in scores]
        metrics = binary_metrics(gold_supported, predicted)
        if metrics['macro_f1'] is None:
            continue
        if best is None or metrics['macro_f1'] > best['macro_f1']:
            best = {'threshold': cut, **metrics}
    return best


def bootstrap_interval(gold_supported, predicted_supported, resamples=BOOTSTRAP_RESAMPLES,
                       seed=BOOTSTRAP_SEED):
    """95% percentile interval for balanced accuracy, resampling pairs."""
    rng = random.Random(seed)
    indices = range(len(gold_supported))
    values = []
    for _ in range(resamples):
        sample = [rng.choice(indices) for _ in indices]
        metric = binary_metrics([gold_supported[i] for i in sample],
                                [predicted_supported[i] for i in sample])['balanced_accuracy']
        if metric is not None:
            values.append(metric)
    if not values:
        return None
    values.sort()
    return values[int(0.025 * (len(values) - 1))], values[int(0.975 * (len(values) - 1))]


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(round(fraction * (len(ordered) - 1))))]


def score_checker(pairs_by_id, rows):
    """Every metric for one checker's rows, which must cover the dataset once."""
    seen = {}
    for row in rows:
        if row.get('id') not in pairs_by_id:
            raise ValueError(f'result for unknown pair {row.get("id")}')
        if row['id'] in seen:
            raise ValueError(f'duplicate result for {row["id"]}')
        if not isinstance(row.get('verdict'), str):
            raise ValueError(f'{row["id"]}: verdict missing')
        seen[row['id']] = row
    missing = sorted(set(pairs_by_id) - set(seen))
    if missing:
        raise ValueError(f'no result for {len(missing)} pairs, first {missing[0]}')

    ids = sorted(pairs_by_id)
    gold = [pairs_by_id[pair_id]['label'] for pair_id in ids]
    verdicts = [seen[pair_id]['verdict'] for pair_id in ids]
    gold_supported = [label == 'supported' for label in gold]
    predicted_supported = [is_supported(verdict) for verdict in verdicts]

    result = {'pairs': len(ids), **binary_metrics(gold_supported, predicted_supported)}
    result['balanced_accuracy_95ci'] = bootstrap_interval(gold_supported, predicted_supported)
    for label in LABELS:
        caught = [is_supported(v) if label == 'supported' else not is_supported(v)
                  for g, v in zip(gold, verdicts) if g == label]
        result[f'recall_{label}'] = _ratio(sum(caught), len(caught))

    three_way = any(verdict in ('unsupported', 'contradicted') for verdict in verdicts)
    result['three_way'] = three_way
    if three_way:
        result['three_way_accuracy'] = _ratio(sum(g == v for g, v in zip(gold, verdicts)), len(ids))
        for label in LABELS:
            exact = [v == label for g, v in zip(gold, verdicts) if g == label]
            result[f'exact_recall_{label}'] = _ratio(sum(exact), len(exact))

    result['unverified'] = sum(verdict == 'unverified' for verdict in verdicts)
    confusion = {label: {} for label in LABELS}
    for g, v in zip(gold, verdicts):
        confusion[g][v] = confusion[g].get(v, 0) + 1
    result['confusion'] = confusion

    latencies = [seen[pair_id]['latency_ms'] for pair_id in ids
                 if isinstance(seen[pair_id].get('latency_ms'), (int, float))]
    if latencies:
        result['latency_ms'] = {'median': statistics.median(latencies),
                                'p95': percentile(latencies, 0.95),
                                'mean': statistics.fmean(latencies), 'max': max(latencies)}
    if any('judge_calls' in seen[pair_id] for pair_id in ids):
        reached = sum(1 for pair_id in ids if seen[pair_id].get('judge_calls', 0) > 0)
        result['reached_judge'] = reached
        result['never_reached_judge'] = len(ids) - reached

    scores = [seen[pair_id].get('score_supported') for pair_id in ids]
    if any(isinstance(score, (int, float)) for score in scores):
        result['best_threshold'] = best_threshold(gold_supported, scores)

    for field in BREAKDOWN_FIELDS:
        groups = {}
        for pair_id, g, pred in zip(ids, gold_supported, predicted_supported):
            value = pairs_by_id[pair_id].get(field)
            if value is not None:
                groups.setdefault(value, ([], []))
                groups[value][0].append(g)
                groups[value][1].append(pred)
        if groups:
            result[f'by_{field}'] = {
                value: {'pairs': len(golds), **{key: metric for key, metric in
                                                binary_metrics(golds, preds).items()
                                                if key.startswith(('false_', 'accuracy'))}}
                for value, (golds, preds) in sorted(groups.items())}

    by_tag = {}
    for pair_id, g, pred in zip(ids, gold_supported, predicted_supported):
        for tag in pairs_by_id[pair_id]['difficulty']:
            bucket = by_tag.setdefault(tag, [0, 0])
            bucket[0] += int(g == pred)
            bucket[1] += 1
    result['by_tag'] = {tag: {'correct': c, 'pairs': n, 'accuracy': c / n}
                        for tag, (c, n) in sorted(by_tag.items())}
    return result


def both_approve(name, first, second):
    """Rows for a gate that passes a claim only when both checkers support it."""
    second_by_id = {row['id']: row for row in second}
    combined = []
    for row in first:
        other = second_by_id.get(row['id'])
        if other is None:
            continue
        verdict = 'supported' if is_supported(row['verdict']) and is_supported(other['verdict']) \
            else 'not_supported'
        latencies = [r.get('latency_ms') for r in (row, other) if isinstance(r.get('latency_ms'), (int, float))]
        combined.append({'id': row['id'], 'checker': name, 'verdict': verdict,
                         'latency_ms': sum(latencies) if latencies else None})
    return combined


def group_by_checker(rows):
    grouped = {}
    for row in rows:
        grouped.setdefault(row.get('checker') or 'unknown', []).append(row)
    return grouped


def _fmt(value, digits=3):
    return '-' if value is None else f'{value:.{digits}f}'


def markdown(scores):
    lines = ['| checker | acc | bal. acc (95% CI) | prec. supported | recall supported | '
             'recall unsupported | recall contradicted | macro-F1 | 3-way acc | unverified | '
             'median ms | p95 ms |',
             '|' + ' --- |' * 13]
    for name, s in scores.items():
        ci = s.get('balanced_accuracy_95ci')
        ci_text = f' ({ci[0]:.2f}-{ci[1]:.2f})' if ci else ''
        latency = s.get('latency_ms') or {}
        lines.append(
            f"| {name} | {_fmt(s['accuracy'])} | {_fmt(s['balanced_accuracy'])}{ci_text} | "
            f"{_fmt(s['precision_supported'])} | {_fmt(s['recall_supported'])} | "
            f"{_fmt(s['recall_unsupported'])} | {_fmt(s['recall_contradicted'])} | "
            f"{_fmt(s['macro_f1'])} | {_fmt(s.get('three_way_accuracy'))} | {s['unverified']} | "
            f"{_fmt(latency.get('median'), 0)} | {_fmt(latency.get('p95'), 0)} |")
    lines += ['', '| checker | false approvals | false-approval rate | false blocks | false-block rate |',
              '| --- | --- | --- | --- | --- |']
    for name, s in scores.items():
        lines.append(f"| {name} | {s['false_approvals']} | {_fmt(s['false_approval_rate'])} | "
                     f"{s['false_blocks']} | {_fmt(s['false_block_rate'])} |")
    for field in BREAKDOWN_FIELDS:
        values = sorted({value for s in scores.values() for value in s.get(f'by_{field}', {})})
        if not values:
            continue
        lines += ['', f'| checker | ' + ' | '.join(f'{field}={value} (FA / FB)' for value in values) + ' |',
                  '|' + ' --- |' * (len(values) + 1)]
        for name, s in scores.items():
            cells = []
            for value in values:
                cell = s.get(f'by_{field}', {}).get(value)
                cells.append('-' if cell is None else f"{cell['false_approvals']} / {cell['false_blocks']}"
                             f" of {cell['pairs']}")
            lines.append(f'| {name} | ' + ' | '.join(cells) + ' |')
    lines += ['', '| checker | best macro-F1 threshold | macro-F1 | bal. acc | prec. supported |',
              '| --- | --- | --- | --- | --- |']
    for name, s in scores.items():
        best = s.get('best_threshold')
        if best:
            lines.append(f"| {name} | {_fmt(best['threshold'])} | {_fmt(best['macro_f1'])} | "
                         f"{_fmt(best['balanced_accuracy'])} | {_fmt(best['precision_supported'])} |")
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('results', nargs='+', help='result JSONL files')
    parser.add_argument('--dataset', default=str(DEFAULT_DATASET))
    parser.add_argument('--library', help='synthetic library JSON; checks every passage is verbatim')
    parser.add_argument('--json-out', help='write every metric here')
    parser.add_argument('--both', action='append', default=[], metavar='A,B',
                        help='also score a gate that needs checkers A and B to agree on "supported"')
    args = parser.parse_args(argv)

    documents = None
    if args.library:
        library = json.loads(Path(args.library).read_text(encoding='utf-8'))
        documents = {document['id']: document['text'] for document in library['documents']}
    pairs_by_id = validate_dataset(load_jsonl(args.dataset), documents)

    rows = [row for path in args.results for row in load_jsonl(path)]
    grouped = group_by_checker(rows)
    for spec in args.both:
        first, second = spec.split(',')
        grouped[f'{first} AND {second}'] = both_approve(f'{first} AND {second}', grouped[first], grouped[second])
    scores = {name: score_checker(pairs_by_id, checker_rows)
              for name, checker_rows in grouped.items()}
    print(markdown(scores))
    if args.json_out:
        Path(args.json_out).write_text(json.dumps(scores, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
