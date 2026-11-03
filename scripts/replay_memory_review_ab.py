#!/usr/bin/env python3
"""Does the memory reviewer keep a conflict visible, without losing the changes it should accept?

On `conflicting_facts` the 9B answered "supported" for a transition whose later passage leaves two dates open, 6 runs of
6, with reasons such as "introducing uncertainty and a new date, supporting the supersession". The rule against that is
two-thirds of the way through one long paragraph. This puts the reviewer's own request to the model for the conflict and
for changes that must still pass or fail, under the shipped prompt, an added question, the rules as ordered lists, and both.

  python -I -u scripts/replay_memory_review_ab.py --shipped A/prompts.rs --candidate B/prompts.rs \
      --llama URL --out OUT.jsonl [--seeds 5]
  python -I -u scripts/replay_memory_review_ab.py --shipped A/prompts.rs --candidate B/prompts.rs \
      --claude claude-haiku-5-5 --budget 1 --out OUT.jsonl [--seeds 5]
  python -I scripts/replay_memory_review_ab.py --summary OUT.jsonl

`--shipped` and `--candidate` are `conversation/memory/prompts.rs` from two checkouts; the candidate holds the lists. An
auth header comes from LLAMA_AUTH_HEADER_NAME and LLAMA_AUTH_HEADER_VALUE, never from a command line. On Claude
(scripts/replay_claude.py) the request is sent as the app's Claude adapter sends the reviewer's: adaptive thinking, the
schema as its output format, no sampling settings; every call's price goes to OUT.jsonl.spend.jsonl.
"""
import argparse
import json
import os
import re
import sys
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_claude as claude
from replay_model import one_model, served_model

ARMS = ("shipped", "question", "lists", "lists_question")
VERIFIER_OUTPUT_TOKENS = 1500
QUESTIONS = [
    "Is the subject of the newer passage the same subject as the item it concerns, or merely a related topic?",
    "Does the passage state this as the user's own requirement, goal or decision, rather than a question, an example, a quotation of someone else, or an assistant suggestion?",
    "Is the scope preserved: the same action, object, environment, quantity and time window, including any exception?",
    "Is the negation preserved: does the passage forbid what the proposal says it forbids, or permit what it says it permits?",
    "For a retirement or replacement: does the later passage actually revoke or replace the earlier requirement, or does it leave that requirement standing?",
]
OPEN_QUESTION = (
    "For a retirement or replacement: does the later passage settle on one value, or does it name alternatives, say the "
    'earlier value may still apply, or say the matter is not confirmed? When it leaves the matter open the transition\'s '
    'verdict is "ambiguous": doubt about the old value does not replace it.'
)
# Each scenario: the older statement, the later one, the labels, and the verdict the transition must get.
SCENARIOS = [
    {
        "name": "conflict dates", "expect": "ambiguous", "kind": "user_fact",
        "earlier": "The Aldersgate handover is on 12 August.",
        "later": "Actually the Aldersgate handover is on 19 August. Or it may still be the 12th — the client has "
                 "sent both dates and has not confirmed which.",
        "earlier_label": "Aldersgate handover date: 12 August",
        "later_label": "Aldersgate handover date: 19 August or possibly 12 August (client sent both, unconfirmed)",
    },
    {
        "name": "conflict budget", "expect": "ambiguous", "kind": "user_fact",
        "earlier": "The Kestrel budget is 40,000.",
        "later": "The Kestrel budget might be 55,000 now, or it could still be 40,000. Finance has not confirmed either.",
        "earlier_label": "Kestrel budget: 40,000",
        "later_label": "Kestrel budget: 55,000 or 40,000, unconfirmed by finance",
    },
    {
        "name": "clean correction", "expect": "supported", "kind": "user_fact",
        "earlier": "The Aldersgate handover is on 12 August.",
        "later": "Correction: the Aldersgate handover has moved to 19 August.",
        "earlier_label": "Aldersgate handover date: 12 August",
        "later_label": "Aldersgate handover date: 19 August",
    },
    {
        "name": "restatement", "expect": "supported", "kind": "constraint",
        "earlier": "Keep the output reproducible from a seed.",
        "later": "Remember: any output you generate must be re-creatable from the seed value, exactly.",
        "earlier_label": "Output reproducible from a seed",
        "later_label": "Any generated output must be exactly re-creatable from the seed value",
    },
    {
        "name": "partial revocation", "expect": "supported", "kind": "constraint",
        "earlier": "Do not deploy anywhere.",
        "later": "Update: deploy only to staging, after approval. Production stays off limits.",
        "earlier_label": "Do not deploy anywhere",
        "later_label": "Deploy only to staging after approval; production stays off limits",
    },
    {
        "name": "moved on", "expect": "unsupported", "kind": "constraint",
        "earlier": "Keep the Sandpiper launch budget under 40,000.",
        "later": "Let's draft the Sandpiper launch announcement now.",
        "earlier_label": "Sandpiper launch budget under 40,000",
        "later_label": "Draft the Sandpiper launch announcement",
    },
]
SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "verdicts": {
            "type": "array",
            "items": {
                "type": "object",
                "additionalProperties": False,
                "properties": {
                    "id": {"type": "string"},
                    "reason": {"type": "string", "maxLength": 512},
                    "verdict": {"type": "string", "enum": ["supported", "ambiguous", "unsupported"]},
                },
                "required": ["id", "verdict", "reason"],
            },
        }
    },
    "required": ["verdicts"],
}


def verifier_system(path):
    source = Path(path).read_text("utf-8")
    literal = re.search(r'pub const VERIFIER_SYSTEM: &str = "(.*)";', source).group(1)
    return literal.replace('\\"', '"').replace("\\n", "\n")


def passage(message_id, sequence, text):
    return {"message_id": message_id, "resolves": True, "role": "user", "sequence": sequence, "text": text}


def surrounding(message_id, sequence, text):
    return {"message_id": message_id, "role": "user", "sequence": sequence, "text": text, "truncated": False}


def payload(scenario, questions):
    old = passage("m1", 1, scenario["earlier"])
    new = passage("m2", 5, scenario["later"])
    around = [surrounding("m1", 1, scenario["earlier"]), surrounding("m2", 5, scenario["later"])]
    addition = {
        "change": "add_mandatory_item", "existing_item_evidence": [], "existing_item_label": None,
        "kind": scenario["kind"],
    }
    reviews = [
        {**addition, "id": "addition:c1", "proposed_evidence": [old], "proposed_label": scenario["earlier_label"],
         "surrounding_original_text": around[:1]},
        {**addition, "id": "addition:c2", "proposed_evidence": [new], "proposed_label": scenario["later_label"],
         "surrounding_original_text": around[1:]},
        {"change": "supersede_item", "existing_item_evidence": [old], "existing_item_label": scenario["earlier_label"],
         "id": "transition:c1", "kind": scenario["kind"], "proposed_evidence": [new],
         "proposed_label": scenario["earlier_label"], "surrounding_original_text": around},
    ]
    body = {"questions": questions, "reviews": reviews, "task": "review_memory_changes",
            "verdicts_required_for_every_id": True}
    return json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def ask(args, system, user, seed):
    body = {
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "max_tokens": VERIFIER_OUTPUT_TOKENS,
        "temperature": 0.2,
        "seed": seed,
        "cache_prompt": False,
        "chat_template_kwargs": {"enable_thinking": False},
        "response_format": {"type": "json_schema", "json_schema": {"name": "response", "strict": True, "schema": SCHEMA}},
    }
    if args.claude:
        return claude.call(body, "review")["content"]
    request = urllib.request.Request(
        args.llama.rstrip("/") + "/v1/chat/completions", json.dumps(body).encode("utf-8"),
        {"Content-Type": "application/json"},
    )
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        request.add_header(name, value)
    with urllib.request.urlopen(request, timeout=900) as response:
        return json.load(response)["choices"][0]["message"]["content"]


def run(args):
    if args.claude:
        model = args.claude
        claude.use(model, args.out + ".spend.jsonl", args.budget)
    else:
        model = served_model(args.llama)
    print(model, flush=True)
    shipped = verifier_system(args.shipped)
    lists = verifier_system(args.candidate)
    assert shipped != lists, "the candidate must hold the lists"
    arms = {
        "shipped": (shipped, QUESTIONS),
        "question": (shipped, QUESTIONS + [OPEN_QUESTION]),
        "lists": (lists, QUESTIONS),
        "lists_question": (lists, QUESTIONS + [OPEN_QUESTION]),
    }
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["arm"], r["scenario"], r["seed"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    with out.open("a", encoding="utf-8") as sink:
        for seed in range(1, args.seeds + 1):
            for scenario in SCENARIOS:
                for arm in ARMS:
                    if (arm, scenario["name"], seed) in done:
                        continue
                    system, questions = arms[arm]
                    raw = ask(args, system, payload(scenario, questions), seed)
                    sink.write(json.dumps({"model": model, "arm": arm, "scenario": scenario["name"], "seed": seed,
                                           "raw": raw}, ensure_ascii=False) + "\n")
                    sink.flush()
            print(f"seed {seed} done", flush=True)
    if args.claude:
        claude.print_spend(args.out + ".spend.jsonl")


def summary(path):
    expected = {scenario["name"]: scenario["expect"] for scenario in SCENARIOS}
    verdicts = defaultdict(Counter)
    additions = defaultdict(Counter)
    records = [json.loads(line) for line in Path(path).read_text("utf-8").splitlines()]
    print("Model:", one_model(records))
    for record in records:
        by_id = {v["id"]: v["verdict"] for v in json.loads(record["raw"])["verdicts"]}
        verdicts[record["arm"], record["scenario"]][by_id.get("transition:c1", "missing")] += 1
        additions[record["arm"], record["scenario"]][by_id.get("addition:c2", "missing")] += 1
    for name, expect in expected.items():
        print(f"{name} (transition should be {expect})")
        for arm in ARMS:
            print(f"  {arm:<16}{dict(verdicts[arm, name])}   later item: {dict(additions[arm, name])}")
    print("transitions right:", {
        arm: f"{sum(verdicts[arm, n][e] for n, e in expected.items())}/{sum(sum(verdicts[arm, n].values()) for n in expected)}"
        for arm in ARMS
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shipped")
    parser.add_argument("--candidate")
    parser.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    parser.add_argument("--out")
    parser.add_argument("--seeds", type=int, default=5)
    parser.add_argument("--summary")
    claude.add_arguments(parser)
    args = parser.parse_args()
    if args.summary:
        summary(args.summary)
    elif args.out and args.shipped and args.candidate:
        run(args)
    else:
        sys.exit("give --shipped, --candidate and --out, or --summary")


if __name__ == "__main__":
    main()
