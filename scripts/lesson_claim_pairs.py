"""Build claim/passage pairs from the claims real lessons were checked on.

`extract` reads the stored verification report of every prepared lesson in a
Lattice database. Each checked claim kept the passages it was judged against
and the passage the judge cited. Two kinds of pair come out of it:

    approved  the claim with the passage the lesson judge cited as support
    related   the claim with the retrieved passage it did not cite that shares
              the most words with it, a realistic near miss

`--calls` adds a third kind from a recorded lesson run (`call-N.json` files):

    rejected  a claim the judge answered unsupported, with the permitted
              passage that shares the most words with it

The judge's verdict is kept in `pipeline_verdict` for analysis and is never
shown to a labeller. Every passage must be a verbatim span of the stored source
text it came from; a pair that is not is dropped and counted.

`add-perturbed` adds minimally edited claims written against pool passages.

`blind` writes the labelling view: id, claim and passage only, shuffled, as
readable text parts.

`assemble` keeps a label two independent labellers agree on, or an
adjudicator's decision where they differ, reports their agreement (Cohen's
kappa, three-way and supported-versus-not), and writes the gold dataset.

`spot-check` writes a page for a person to relabel a gold sample blind, and
`compare` reports their agreement with the gold labels.
"""
import argparse
import hashlib
import json
import random
import re
import sqlite3
import sys
from pathlib import Path

WORD = re.compile(r"[a-z0-9][a-z0-9.+#-]*")
# Lessons cite with inline markers; they carry no factual content, and only
# the lesson judge was trained on seeing them, so every checker gets the bare claim.
MARKER = re.compile(r'\s*\[sourceIndex:\s*\d+\]')
STOP = frozenset(
    'a an and are as at be by can for from has have in into is it its of on or that the this to '
    'was were which with you your their they these those not no do does more most than when how'.split())


def clean(claim):
    return ' '.join(MARKER.sub('', claim).split())


def words(text):
    return {w for w in WORD.findall(text.lower()) if w not in STOP and len(w) > 1}


def overlap(claim, passage):
    claim_words = words(claim)
    return len(claim_words & words(passage)) / max(1, len(claim_words))


def pair_id(subject, claim, passage):
    digest = hashlib.sha256(f'{claim}\n{passage}'.encode()).hexdigest()[:10]
    return f'{subject}-{digest}'


def sources(db):
    """Source id -> url, title and every stored text of it.

    A lesson's evidence names the program's source; its saved text can sit on
    a version row keyed to the library copy of the same page, so versions are
    matched by URL as well as by id.
    """
    by_key = {}
    for source_id, requested, resolved, text in db.execute(
            'select source_id, requested_url, resolved_url, full_text from learning_source_versions'):
        for key in {source_id, requested, resolved} - {None, ''}:
            by_key.setdefault(key, []).append(text or '')
    found = {}
    for source_id, url, title in db.execute('select id, url, title from learning_sources'):
        texts = by_key.get(source_id, []) + by_key.get(url, [])
        found[source_id] = {'url': url or source_id, 'title': title or '', 'texts': texts}
    return found


def verbatim(source, passage):
    return source is not None and any(passage in text for text in source['texts'])


def extract(args):
    db = sqlite3.connect(f'file:{args.db}?mode=ro', uri=True)
    known = sources(db)
    rng = random.Random(args.seed)
    approved, related, dropped = [], [], 0
    for lesson_id, report in db.execute('select lesson_id, report_json from learning_lesson_verifications'):
        for finding in json.loads(report).get('findings', []):
            claim = clean(finding['statement'])
            cited = finding.get('supporting_quote')
            evidence = finding.get('evidence', [])
            base = {'subject': args.subject, 'lesson_id': lesson_id,
                    'pipeline_verdict': finding['verdict'], 'pipeline_reason': finding.get('reason', '')}
            if cited and finding['verdict'] == 'supported':
                origin = next((e for e in evidence if e['text'] == cited), None)
                source = known.get(origin['source_id']) if origin else None
                if verbatim(source, cited):
                    approved.append({**base, 'origin': 'approved', 'claim': claim, 'passage': cited,
                                     'document_id': source['url'], 'source_title': source['title']})
                else:
                    dropped += 1
            others = [e for e in evidence if e['text'] != cited]
            if others:
                best = max(others, key=lambda e: overlap(claim, e['text']))
                source = known.get(best['source_id'])
                if verbatim(source, best['text']):
                    related.append({**base, 'origin': 'related', 'claim': claim, 'passage': best['text'],
                                    'document_id': source['url'], 'source_title': source['title']})
                else:
                    dropped += 1
    rejected = list({(row['claim'], row['passage']): row
                     for row in rejected_pairs(args, known)}.values()) if args.calls else []
    rejected = rng.sample(rejected, min(args.rejected, len(rejected)))
    picked = rng.sample(approved, min(args.approved, len(approved)))
    chosen = picked + rng.sample(related, min(args.related, len(related))) + rejected
    if args.pool_out:
        # Approved pairs not used as-is are the material for perturbed claims,
        # so no claim appears both untouched and edited against one passage.
        rest = [row for row in approved if row not in picked]
        with open(args.pool_out, 'a', encoding='utf-8') as pool:
            for row in rng.sample(rest, min(args.pool, len(rest))):
                pool.write(json.dumps({'pool_id': pair_id(f'{args.subject}-pool', row['claim'], row['passage']),
                                       **row}, ensure_ascii=False) + '\n')
    out_path = Path(args.out)
    seen = {json.loads(line)['id'] for line in out_path.read_text().splitlines() if line.strip()} \
        if out_path.exists() else set()
    written = 0
    with out_path.open('a', encoding='utf-8') as out:
        for row in chosen:
            row = {'id': pair_id(args.subject, row['claim'], row['passage']), **row}
            if row['id'] in seen:
                continue
            seen.add(row['id'])
            out.write(json.dumps(row, ensure_ascii=False) + '\n')
            written += 1
    print(f'{args.subject}: {len(approved)} approved, {len(related)} related, {len(rejected)} rejected '
          f'available; wrote {written}; dropped {dropped} non-verbatim', file=sys.stderr)


def rejected_pairs(args, known):
    """Claims a recorded batch call answered unsupported, with their closest permitted passage."""
    texts = {source_id: source for source_id, source in known.items()}
    rows = []
    for path in sorted(Path(args.calls).glob('call-*.json')):
        call = json.loads(path.read_text())
        system = call['input'][0]['content']
        if not system.startswith('You check one claim') or 'inventory-fidelity' in system:
            continue
        try:
            bank = json.loads(call['input'][1]['content'])
            checks = {check['id']: check['response'] for check in json.loads(call['answer'])['checks']}
        except (ValueError, KeyError, TypeError):
            continue
        passages = {p['id']: p['text'] for p in bank.get('sourcePassages', [])}
        for claim in bank.get('claims', []):
            verdict = re.findall(r'Verdict:\s*(\w+)', checks.get(claim['id'], ''))
            if not verdict or verdict[-1] != 'unsupported':
                continue
            permitted = [passages[i] for i in claim.get('evidenceIds', []) if i in passages]
            if not permitted:
                continue
            passage = max(permitted, key=lambda text: overlap(claim['claim'], text))
            source = next((s for s in texts.values() if verbatim(s, passage)), None)
            if source is None:
                continue
            text = clean(claim['claim'])
            rows.append({'subject': args.subject, 'lesson_id': '', 'pipeline_verdict': 'unsupported',
                         'pipeline_reason': checks[claim['id']], 'origin': 'rejected', 'claim': text,
                         'passage': passage, 'document_id': source['url'], 'source_title': source['title']})
    return rows


def add_perturbed(args):
    """Append edited claims, each against the pool passage it was written for."""
    pool = {row['pool_id']: row for row in map(json.loads, Path(args.pool).read_text().splitlines()) if row}
    out_path = Path(args.out)
    seen = {json.loads(line)['id'] for line in out_path.read_text().splitlines() if line.strip()}
    written = 0
    with out_path.open('a', encoding='utf-8') as out:
        for edit in map(json.loads, filter(str.strip, Path(args.edits).read_text().splitlines())):
            base = pool[edit['pool_id']]
            claim = ' '.join(edit['claim'].split())
            row = {'id': pair_id(base['subject'], claim, base['passage']), 'subject': base['subject'],
                   'lesson_id': base['lesson_id'], 'pipeline_verdict': None, 'pipeline_reason': '',
                   'origin': 'perturbed', 'claim': claim, 'passage': base['passage'],
                   'document_id': base['document_id'], 'source_title': base['source_title'],
                   'original_claim': base['claim'], 'intended_label': edit['intended_label'],
                   'perturbation': edit['difficulty'], 'perturbation_note': edit['note']}
            if row['id'] not in seen:
                seen.add(row['id'])
                out.write(json.dumps(row, ensure_ascii=False) + '\n')
                written += 1
    print(f'wrote {written} perturbed pairs', file=sys.stderr)


def wrap(text, width=1500):
    lines = []
    for line in text.splitlines():
        while len(line) > width:
            cut = line.rfind(' ', 0, width)
            cut = cut if cut > 0 else width
            lines.append(line[:cut])
            line = line[cut:].lstrip()
        lines.append(line)
    return '\n'.join(lines)


def blind(args):
    """The labelling view: id, claim and passage only, shuffled, in readable parts."""
    rows = [json.loads(line) for line in Path(args.pairs).read_text().splitlines() if line.strip()]
    if args.only:
        wanted = set(Path(args.only).read_text().split())
        rows = [row for row in rows if row['id'] in wanted]
    random.Random(args.seed).shuffle(rows)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    for part, start in enumerate(range(0, len(rows), args.per_part), start=1):
        chunk = rows[start:start + args.per_part]
        text = [f'{len(chunk)} items. Label each one by the rule; the passage is the only evidence.\n']
        for number, row in enumerate(chunk, start=start + 1):
            text.append(f'=== ITEM {number}  id: {row["id"]}\nCLAIM: {row["claim"]}\nPASSAGE:\n'
                        f'{wrap(row["passage"])}\n=== END ITEM {number}\n')
        (out_dir / f'part-{part:02d}.txt').write_text('\n'.join(text), encoding='utf-8')
    print(f'wrote {len(rows)} blind pairs in {part} parts', file=sys.stderr)


def kappa(first, second):
    """Cohen's kappa for two parallel label lists."""
    total = len(first)
    if not total:
        return None
    observed = sum(a == b for a, b in zip(first, second)) / total
    labels = set(first) | set(second)
    expected = sum((first.count(label) / total) * (second.count(label) / total) for label in labels)
    return 1.0 if expected == 1 else (observed - expected) / (1 - expected)


def load(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def assemble(args):
    """Gold labels: two independent labellers agree, or an adjudicator decides."""
    pairs = {row['id']: row for row in load(args.pairs)}
    first = {row['id']: row for row in load(args.labels[0])}
    second = {row['id']: row for row in load(args.labels[1])}
    adjudicated = {row['id']: row for row in load(args.adjudicated)} if args.adjudicated else {}
    common = sorted(set(pairs) & set(first) & set(second))
    a = [first[i]['label'] for i in common]
    b = [second[i]['label'] for i in common]
    binary = lambda labels: ['supported' if label == 'supported' else 'not' for label in labels]
    disagreements, dataset = [], []
    for pair_id in common:
        pair, one, two = pairs[pair_id], first[pair_id], second[pair_id]
        decision = adjudicated.get(pair_id)
        if one['label'] == two['label'] and decision is None:
            label, note, tags = one['label'], one['note'], one['difficulty']
        elif decision is not None:
            if decision['label'] == 'drop':
                continue
            label, note = decision['label'], decision['note']
            tags = decision.get('difficulty') or one['difficulty']
        else:
            disagreements.append({'id': pair_id, 'claim': pair['claim'], 'first': one, 'second': two})
            continue
        tags = sorted(set(tags) | set(pair.get('perturbation') or []))
        if 'partial' in tags and label == 'contradicted':
            tags = sorted((set(tags) - {'partial'}) | {'multi_clause'})
        dataset.append({'id': pair_id, 'claim': pair['claim'], 'passage': pair['passage'],
                        'document_id': pair['document_id'], 'label': label, 'difficulty': tags,
                        'note': note, 'subject': pair['subject'], 'origin': pair['origin'],
                        'source_title': pair.get('source_title', ''),
                        'pipeline_verdict': pair.get('pipeline_verdict')})
    with open(args.out, 'w', encoding='utf-8') as out:
        for row in sorted(dataset, key=lambda row: row['id']):
            out.write(json.dumps(row, ensure_ascii=False) + '\n')
    with open(args.disagreements, 'w', encoding='utf-8') as out:
        for row in disagreements:
            out.write(json.dumps(row, ensure_ascii=False) + '\n')
    summary = {'pairs': len(pairs), 'labelled_by_both': len(common),
               'three_way_agreement': sum(x == y for x, y in zip(a, b)) / max(1, len(common)),
               'three_way_kappa': kappa(a, b), 'binary_kappa': kappa(binary(a), binary(b)),
               'adjudicated': len(adjudicated), 'unresolved': len(disagreements), 'gold': len(dataset),
               'gold_labels': {label: sum(row['label'] == label for row in dataset)
                               for label in ('supported', 'unsupported', 'contradicted')}}
    print(json.dumps(summary, indent=1))


SPOT_CHECK_PAGE = """<!doctype html><html><head><meta charset="utf-8"><title>Claim labels: blind spot-check</title>
<style>body{font:15px/1.5 -apple-system,system-ui,sans-serif;max-width:960px;margin:24px auto;padding:0 16px;color:#1d1d1f}
.item{border:1px solid #d2d2d7;border-radius:10px;padding:16px;margin:18px 0}.claim{font-weight:600;font-size:16px}
.passage{white-space:pre-wrap;background:#f5f5f7;border-radius:8px;padding:12px;max-height:420px;overflow:auto;font-size:13.5px}
mark{background:#fff3a3}button{font:inherit;padding:6px 12px;margin:8px 6px 0 0;border-radius:8px;border:1px solid #aaa;background:#fff;cursor:pointer}
button.on{background:#0071e3;color:#fff;border-color:#0071e3}#bar{position:sticky;top:0;background:#fff;padding:10px 0;border-bottom:1px solid #eee}</style></head>
<body><div id="bar"><b>Blind spot-check</b>: <span id="count"></span> labelled. Judge from the passage only (outside knowledge never counts).
<button onclick="copyResults()">Copy results</button><span id="copied"></span></div>
<p><b>supported</b>: every part of the claim is stated in the passage or follows necessarily. <b>contradicted</b>: the passage states something incompatible.
<b>unsupported</b>: the passage is silent on at least one part. Highlighted words also appear in the claim; they are a reading aid, not evidence.</p>
<div id="items"></div><script>
const ITEMS = __ITEMS__;
const KEY = "lattice-spot-check-" + ITEMS.map(i => i.id).join("").length;
const answers = JSON.parse(localStorage.getItem(KEY) || "{}");
const esc = t => t.replace(/[&<>]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));
function highlight(passage, claim) {
  const words = new Set((claim.toLowerCase().match(/[a-z0-9][a-z0-9.+#-]{3,}/g) || []));
  return esc(passage).replace(/[A-Za-z0-9][A-Za-z0-9.+#-]{3,}/g, w => words.has(w.toLowerCase()) ? "<mark>" + w + "</mark>" : w);
}
function render() {
  document.getElementById("items").innerHTML = ITEMS.map((item, n) => `<div class="item"><div>${n + 1} / ${ITEMS.length}</div>
    <p class="claim">${esc(item.claim)}</p><div class="passage">${highlight(item.passage, item.claim)}</div>
    ${["supported", "unsupported", "contradicted"].map(label => `<button class="${answers[item.id] === label ? "on" : ""}"
      onclick="pick('${item.id}','${label}')">${label}</button>`).join("")}</div>`).join("");
  document.getElementById("count").textContent = Object.keys(answers).length + " / " + ITEMS.length;
}
function pick(id, label) { answers[id] = label; localStorage.setItem(KEY, JSON.stringify(answers)); render(); }
function copyResults() { navigator.clipboard.writeText(JSON.stringify(answers)); document.getElementById("copied").textContent = " copied"; }
render();
</script></body></html>
"""


def spot_check(args):
    """A self-contained page for a person to relabel a stratified gold sample blind."""
    rows = load(args.gold)
    rng = random.Random(args.seed)
    by_label = {}
    for row in rows:
        by_label.setdefault(row['label'], []).append(row)
    share = args.count / max(1, len(rows))
    picked = []
    for label, members in sorted(by_label.items()):
        picked += rng.sample(members, min(len(members), max(1, round(len(members) * share))))
    rng.shuffle(picked)
    items = [{'id': row['id'], 'claim': row['claim'], 'passage': row['passage']} for row in picked[:args.count]]
    Path(args.out).write_text(SPOT_CHECK_PAGE.replace('__ITEMS__', json.dumps(items, ensure_ascii=False)),
                              encoding='utf-8')
    print(f'wrote {len(items)} items to {args.out}', file=sys.stderr)


def compare(args):
    """Agreement between a person's spot-check answers and the gold labels."""
    gold = {row['id']: row['label'] for row in load(args.gold)}
    answers = json.loads(Path(args.answers).read_text())
    ids = [pair_id for pair_id in answers if pair_id in gold]
    person = [answers[i] for i in ids]
    reference = [gold[i] for i in ids]
    binary = lambda labels: ['supported' if label == 'supported' else 'not' for label in labels]
    print(json.dumps({'items': len(ids),
                      'three_way_agreement': sum(a == b for a, b in zip(person, reference)) / max(1, len(ids)),
                      'three_way_kappa': kappa(person, reference),
                      'binary_agreement': sum(a == b for a, b in zip(binary(person), binary(reference)))
                      / max(1, len(ids)),
                      'binary_kappa': kappa(binary(person), binary(reference)),
                      'differences': [{'id': i, 'person': answers[i], 'gold': gold[i]}
                                      for i in ids if answers[i] != gold[i]]}, indent=1))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    commands = parser.add_subparsers(dest='command', required=True)
    ex = commands.add_parser('extract')
    ex.add_argument('--db', required=True)
    ex.add_argument('--subject', required=True)
    ex.add_argument('--out', required=True, help='candidate JSONL, appended to')
    ex.add_argument('--approved', type=int, default=35)
    ex.add_argument('--related', type=int, default=10)
    ex.add_argument('--calls', help='directory of recorded call-N.json files')
    ex.add_argument('--rejected', type=int, default=15)
    ex.add_argument('--pool', type=int, default=20, help='approved pairs offered for perturbation')
    ex.add_argument('--pool-out', help='JSONL the perturbation pool is appended to')
    ex.add_argument('--seed', type=int, default=20261014)
    ap = commands.add_parser('add-perturbed')
    ap.add_argument('--pool', required=True)
    ap.add_argument('--edits', required=True, help='JSONL of {pool_id, claim, intended_label, difficulty, note}')
    ap.add_argument('--out', required=True)
    asm = commands.add_parser('assemble')
    asm.add_argument('--pairs', required=True)
    asm.add_argument('--labels', nargs=2, required=True, metavar=('FIRST', 'SECOND'))
    asm.add_argument('--adjudicated', help='JSONL of {id, label (or "drop"), note, difficulty?}')
    asm.add_argument('--out', required=True)
    asm.add_argument('--disagreements', required=True)
    sc = commands.add_parser('spot-check')
    sc.add_argument('--gold', required=True)
    sc.add_argument('--out', required=True, help='HTML page')
    sc.add_argument('--count', type=int, default=30)
    sc.add_argument('--seed', type=int, default=20261014)
    cp = commands.add_parser('compare')
    cp.add_argument('--gold', required=True)
    cp.add_argument('--answers', required=True, help="JSON copied from the spot-check page")
    bl = commands.add_parser('blind')
    bl.add_argument('--pairs', required=True)
    bl.add_argument('--out-dir', required=True)
    bl.add_argument('--per-part', type=int, default=22)
    bl.add_argument('--only', help='file of ids to include')
    bl.add_argument('--seed', type=int, default=20261014)
    args = parser.parse_args(argv)
    {'extract': extract, 'add-perturbed': add_perturbed, 'blind': blind,
     'assemble': assemble, 'spot-check': spot_check, 'compare': compare}[args.command](args)


if __name__ == '__main__':
    main()
