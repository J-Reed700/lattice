"""Run Granite Guardian 3.3's groundedness check over claim/passage pairs.

The model is served by a llama-server (`--jinja` not required) and prompted
with the exact text IBM's chat template renders for `criteria_id:
groundedness`, sent raw to `/completion`. llama-server's own chat endpoint
cannot be used: it treats the final assistant message, which is the text under
judgement, as a prefill of the judge's answer.

Granite answers "yes" when the text is ungrounded. The decision rule is the
publisher's: the claim is supported when the generated score is "no".
`score_supported` is P(no) / (P(yes) + P(no)) at the score token, read from
the top log-probabilities, for threshold analysis.

    python3 scripts/claim_checker_granite.py --url http://127.0.0.1:8094 \
        --dataset evals/grounding/lesson-claim-pairs-v1.jsonl \
        --out evals/grounding/results/<date>-granite-guardian-3.3-8b.jsonl
"""
import argparse
import json
import math
import time
import urllib.request
from pathlib import Path

SYSTEM = ('As a judge agent, your role is to help assess whether the provided text meets the given judging '
          'criteria, utilizing all available information, including conversations, documents, and tools.')
CRITERIA = ('A text is considered ungrounded or unfaithful if it includes information lacking support from, or '
            'directly contradicting, the provided document(s). This risk arises when the text fabricates details, '
            'misinterprets the content, or makes unsupported extrapolations beyond what is explicitly stated in '
            'the document(s).')
SCHEMA = "If the last assistant's text meets the criteria, return 'yes'; otherwise, return 'no'."


def prompt(passage, claim, think=False):
    return (f'<|start_of_role|>system<|end_of_role|>{SYSTEM}<|end_of_text|>\n'
            f'<|start_of_role|>document {{"document_id" :"0"}}<|end_of_role|>\n{passage}<|end_of_text|>\n'
            f'<|start_of_role|>assistant<|end_of_role|>{claim}<|end_of_text|>\n'
            f'<|start_of_role|>judge_protocol<|end_of_role|>### Criteria: {CRITERIA}\n\n'
            f'### Scoring Schema: {SCHEMA}<|end_of_text|>\n'
            f'<|start_of_role|>judge{{{"think" if think else "no_think"}}}<|end_of_role|>')


def complete(url, text, think):
    body = {'prompt': text, 'temperature': 0, 'n_predict': 2048 if think else 24, 'n_probs': 10,
            'cache_prompt': False}
    request = urllib.request.Request(f'{url}/completion', json.dumps(body).encode(),
                                     {'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=600) as response:
        return json.load(response)


def judge(reply):
    """(verdict, score_supported) from one completion."""
    content = reply['content']
    score = content.rsplit('<score>', 1)[-1].split('</score>', 1)[0].strip().lower() \
        if '<score>' in content else ''
    verdict = {'no': 'supported', 'yes': 'not_supported'}.get(score, 'unverified')
    probability = None
    for token in reply.get('completion_probabilities', []):
        if token.get('token', '').strip().lower() in ('yes', 'no'):
            mass = {'yes': 0.0, 'no': 0.0}
            for candidate in token.get('top_logprobs', token.get('top_probs', [])):
                word = candidate.get('token', '').strip().lower()
                if word in mass:
                    value = candidate.get('logprob')
                    mass[word] += math.exp(value) if value is not None else candidate.get('prob', 0.0)
            if mass['yes'] + mass['no'] > 0:
                probability = mass['no'] / (mass['yes'] + mass['no'])
            break
    return verdict, probability


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--url', default='http://127.0.0.1:8094')
    parser.add_argument('--dataset', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--think', action='store_true', help="the model's thinking mode")
    parser.add_argument('--checker', default=None)
    args = parser.parse_args(argv)
    name = args.checker or f'granite-guardian-3.3-8b{"-think" if args.think else ""}'
    out = Path(args.out)
    done = {json.loads(line)['id'] for line in out.read_text().splitlines() if line.strip()} \
        if out.exists() else set()
    pairs = [json.loads(line) for line in Path(args.dataset).read_text().splitlines() if line.strip()]
    with out.open('a', encoding='utf-8') as results:
        for pair in pairs:
            if pair['id'] in done:
                continue
            started = time.perf_counter()
            reply = complete(args.url, prompt(pair['passage'], pair['claim'], args.think), args.think)
            verdict, probability = judge(reply)
            results.write(json.dumps({
                'id': pair['id'], 'checker': name, 'label': pair.get('label'),
                'difficulty': pair.get('difficulty'), 'verdict': verdict, 'score_supported': probability,
                'latency_ms': (time.perf_counter() - started) * 1000, 'reply': reply['content'][-2000:],
            }) + '\n')
            results.flush()
    print(f'{name}: {len(pairs) - len(done)} pairs scored')


if __name__ == '__main__':
    main()
