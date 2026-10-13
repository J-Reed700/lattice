"""Run one external claim checker over the labelled claim/passage pairs.

Each invocation loads exactly one model, scores every pair one at a time, and
exits, so the model's memory is returned before the next checker starts. The
script needs torch and transformers plus the checker's own package; it does
not assume where they are installed. Run it with whichever interpreter has
them, for example:

    "$EVAL_PYTHON" scripts/claim_checker_bakeoff.py --checker hhem-2.1-open

Checkers (each at its publisher's recommended decision rule):

    minicheck-roberta-large   lytang/MiniCheck-RoBERTa-Large      P(supported) > 0.5
    minicheck-flan-t5-large   lytang/MiniCheck-Flan-T5-Large      P(supported) > 0.5
    hhem-2.1-open             vectara/hallucination_evaluation_model   score >= 0.5
    lettucedetect-base        KRLabsOrg/lettucedect-base-modernbert-en-v1
    lettucedetect-large       KRLabsOrg/lettucedect-large-modernbert-en-v1
                              (both: unsupported when any claim token is
                              classified hallucinated, i.e. P > 0.5)

Every checker is binary: it says whether the passage supports the claim and
has no "contradicted" class. Output, one JSON line per pair:
evals/grounding/results/<date>-<checker>.jsonl, plus a .meta.json with the
model's revision, parameter count, device and load time.
"""
import argparse
import json
import platform
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DATASET = ROOT / 'evals' / 'grounding' / 'claim-pairs-v1.jsonl'
DEFAULT_OUT_DIR = ROOT / 'evals' / 'grounding' / 'results'
RUN_DATE = '2026-10-12'

# Revisions are pinned where the loader accepts one. MiniCheck's package loads
# its own checkpoints at `main`, so the revision it resolved is recorded instead.
CHECKERS = {
    'minicheck-roberta-large': {
        'family': 'MiniCheck', 'repo': 'lytang/MiniCheck-RoBERTa-Large',
        'architecture': 'RoBERTa-large sequence classifier', 'loader': 'minicheck',
        'minicheck_name': 'roberta-large'},
    'minicheck-flan-t5-large': {
        'family': 'MiniCheck', 'repo': 'lytang/MiniCheck-Flan-T5-Large',
        'architecture': 'Flan-T5-large encoder-decoder, first decoded token', 'loader': 'minicheck',
        'minicheck_name': 'flan-t5-large'},
    'hhem-2.1-open': {
        'family': 'HHEM', 'repo': 'vectara/hallucination_evaluation_model',
        'revision': 'de9a5e6ba9ba49531a022dba17e35ffeb2a6fad8',
        'architecture': 'Flan-T5-base encoder, token-classification head read at the first token',
        'loader': 'hhem'},
    'lettucedetect-base': {
        'family': 'LettuceDetect', 'repo': 'KRLabsOrg/lettucedect-base-modernbert-en-v1',
        'architecture': 'ModernBERT-base token classifier', 'loader': 'lettucedetect'},
    'lettucedetect-large': {
        'family': 'LettuceDetect', 'repo': 'KRLabsOrg/lettucedect-large-modernbert-en-v1',
        'architecture': 'ModernBERT-large token classifier', 'loader': 'lettucedetect'},
}


def load_pairs(path):
    pairs = []
    for line in Path(path).read_text(encoding='utf-8').splitlines():
        if line.strip():
            pairs.append(json.loads(line))
    return pairs


def lettuce_support(token_predictions):
    """Score and verdict for one claim from LettuceDetect's token output.

    LettuceDetect labels each answer token; the claim is unsupported as soon
    as one token is labelled hallucinated, which is the detector's own span
    rule. P(supported) is one minus the most confident hallucinated token.
    """
    if not token_predictions:
        return 1.0, 'supported'
    worst = max(float(token['prob']) for token in token_predictions)
    flagged = any(int(token['pred']) == 1 for token in token_predictions)
    return 1.0 - worst, 'not_supported' if flagged else 'supported'


def resolved_revision(repo, cache_dir):
    """The commit a `main` download resolved to, read from the hub cache."""
    ref = Path(cache_dir) / f"models--{repo.replace('/', '--')}" / 'refs' / 'main'
    return ref.read_text().strip() if ref.exists() else None


def parameter_count(model):
    return sum(parameter.numel() for parameter in model.parameters())


def load_checker(name, device):
    """Return (predict(passage, claim) -> (score, verdict), torch module, notes)."""
    spec = CHECKERS[name]
    import torch

    if spec['loader'] == 'minicheck':
        from minicheck.minicheck import MiniCheck

        scorer = MiniCheck(model_name=spec['minicheck_name'], batch_size=1)
        # The package places the model with device_map="auto"; move it so
        # every checker runs on the device asked for.
        scorer.model.model.to(device)

        def predict(passage, claim):
            labels, probs, _, _ = scorer.score(docs=[passage], claims=[claim])
            return float(probs[0]), 'supported' if int(labels[0]) == 1 else 'not_supported'

        return predict, scorer.model.model, {'decision_rule': 'P(supported) > 0.5 (package pred_label)'}

    if spec['loader'] == 'hhem':
        from transformers import AutoModelForSequenceClassification

        model = AutoModelForSequenceClassification.from_pretrained(
            spec['repo'], revision=spec['revision'], trust_remote_code=True)
        model.to(device).eval()

        def predict(passage, claim):
            with torch.no_grad():
                score = float(model.predict([(passage, claim)])[0])
            return score, 'supported' if score >= 0.5 else 'not_supported'

        return predict, model, {'decision_rule': 'consistency score >= 0.5'}

    if spec['loader'] == 'lettucedetect':
        from lettucedetect.models.inference import HallucinationDetector

        detector = HallucinationDetector(method='transformer', model_path=spec['repo'], device=device)

        def predict(passage, claim):
            # No question: the claim is checked as a statement about the
            # passage, which LettuceDetect frames with its summary prompt.
            tokens = detector.predict(context=[passage], answer=claim, question=None,
                                      output_format='tokens')
            return lettuce_support(tokens)

        return predict, detector.detector.model, {
            'decision_rule': 'any claim token with P(hallucinated) > 0.5 makes it unsupported'}

    raise ValueError(f'unknown loader {spec["loader"]}')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--checker', required=True, choices=sorted(CHECKERS))
    parser.add_argument('--dataset', default=str(DEFAULT_DATASET))
    parser.add_argument('--out-dir', default=str(DEFAULT_OUT_DIR))
    parser.add_argument('--device', default='cpu',
                        help='cpu (default; what a Candle port would run on) or mps')
    parser.add_argument('--threads', type=int, default=4)
    parser.add_argument('--limit', type=int, default=None, help='score only the first N pairs (smoke test)')
    args = parser.parse_args(argv)

    import torch
    import transformers
    from huggingface_hub import constants as hub_constants

    torch.set_num_threads(args.threads)
    pairs = load_pairs(args.dataset)[:args.limit]
    spec = CHECKERS[args.checker]

    started = time.perf_counter()
    predict, module, notes = load_checker(args.checker, args.device)
    load_seconds = time.perf_counter() - started
    # One untimed call so lazy initialisation is not charged to the first pair.
    predict('The kettle is in the kitchen.', 'The kettle is in the kitchen.')

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f'{RUN_DATE}-{args.checker}.jsonl'
    latencies = []
    with out_path.open('w', encoding='utf-8') as out:
        for position, pair in enumerate(pairs, start=1):
            started = time.perf_counter()
            score, verdict = predict(pair['passage'], pair['claim'])
            latency_ms = (time.perf_counter() - started) * 1000
            latencies.append(latency_ms)
            out.write(json.dumps({
                'id': pair['id'], 'checker': args.checker, 'label': pair['label'],
                'difficulty': pair.get('difficulty', []), 'verdict': verdict,
                'score_supported': score, 'latency_ms': latency_ms}) + '\n')
            print(f"[{position}/{len(pairs)}] {pair['id']} gold={pair['label']} "
                  f"verdict={verdict} p={score:.3f} {latency_ms:.0f} ms", file=sys.stderr)

    meta = {
        'checker': args.checker, 'family': spec['family'], 'repo': spec['repo'],
        'revision': spec.get('revision') or resolved_revision(spec['repo'], hub_constants.HF_HUB_CACHE),
        'architecture': spec['architecture'], 'parameters': parameter_count(module),
        'device': args.device, 'threads': args.threads, 'load_seconds': load_seconds,
        'pairs': len(pairs), 'median_latency_ms': statistics.median(latencies) if latencies else None,
        'torch': torch.__version__, 'transformers': transformers.__version__,
        'python': platform.python_version(), 'machine': platform.machine(), **notes,
    }
    (out_dir / f'{RUN_DATE}-{args.checker}.meta.json').write_text(json.dumps(meta, indent=2) + '\n')
    print(json.dumps(meta, indent=2), file=sys.stderr)


if __name__ == '__main__':
    main()
