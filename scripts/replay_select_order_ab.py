#!/usr/bin/env python3
"""Does the lesson's reference selector pick better when it gives its reason before its decision?

`research::selection_schema` asks for `useForGap` before `reason`. The selector then commits to a page before saying what
the page establishes, and its prompt carries a patch against the result: "A rejection rationale must never accompany
useForGap=true". This replays saved selection calls with the two fields in each order and everything else the same.

  python -I -u scripts/replay_select_order_ab.py --calls DIR --llama URL --out OUT.jsonl [--seeds 3]
  python -I scripts/replay_select_order_ab.py --summary OUT.jsonl

DIR holds `call-N.json` files from a run with LATTICE_LESSON_CALLS set. An auth header comes from LLAMA_AUTH_HEADER_NAME
and LLAMA_AUTH_HEADER_VALUE, never from a command line.
"""
import argparse
import json
import os
import sys
import urllib.request
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from replay_model import one_model, served_model

ARMS = ("decision_first", "reason_first")
SELECTION_ANSWER_TOKENS = 2000
DESCRIPTION = (
    "True only when this result is directly relevant and credible for the scoped evidence gap. False for a rejected "
    "candidate, even if its rejection is explained in reason."
)


def schema(arm, ids):
    fields = {
        "id": {"type": "string", "enum": ids},
        "useForGap": {"type": "boolean", "description": DESCRIPTION},
        "reason": {"type": "string", "minLength": 1, "maxLength": 600},
    }
    order = ["id", "useForGap", "reason"] if arm == "decision_first" else ["id", "reason", "useForGap"]
    decision = {
        "type": "object",
        "additionalProperties": False,
        "required": order,
        "properties": {name: fields[name] for name in order},
    }
    return {
        "type": "object",
        "additionalProperties": False,
        "required": ["decisions"],
        "properties": {"decisions": {"type": "array", "maxItems": len(ids), "items": decision}},
    }


def selection_calls(directory):
    for path in sorted(Path(directory).glob("call-*.json"), key=lambda p: int(p.stem.split("-")[1])):
        text = path.read_text("utf-8")
        if "Select trustworthy references" in text:
            call = json.loads(text)
            yield path.stem, call["input"][0]["content"], call["input"][1]["content"]


def ask(url, arm, system, user, seed):
    ids = [result["id"] for result in json.loads(user)["results"]]
    body = {
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "max_tokens": SELECTION_ANSWER_TOKENS,
        "temperature": 0.2,
        "seed": seed,
        "cache_prompt": False,
        "chat_template_kwargs": {"enable_thinking": False},
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "response", "strict": True, "schema": schema(arm, ids)},
        },
    }
    request = urllib.request.Request(
        url.rstrip("/") + "/v1/chat/completions",
        json.dumps(body).encode("utf-8"),
        {"Content-Type": "application/json"},
    )
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        request.add_header(name, value)
    with urllib.request.urlopen(request, timeout=900) as response:
        return json.load(response)["choices"][0]["message"]["content"], len(ids)


def run(args):
    model = served_model(args.llama)
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["call"], r["arm"], r["seed"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    calls = list(selection_calls(args.calls))
    print(f"{model}: {len(calls)} selection calls", flush=True)
    with out.open("a", encoding="utf-8") as sink:
        for seed in range(1, args.seeds + 1):
            for name, system, user in calls:
                for arm in ARMS:
                    if (name, arm, seed) in done:
                        continue
                    raw, shown = ask(args.llama, arm, system, user, seed)
                    sink.write(json.dumps({"model": model, "call": name, "arm": arm, "seed": seed, "shown": shown,
                                           "raw": raw}, ensure_ascii=False) + "\n")
                    sink.flush()
            print(f"seed {seed} done", flush=True)


def summary(path):
    totals = {arm: Counter() for arm in ARMS}
    records = [json.loads(line) for line in Path(path).read_text("utf-8").splitlines()]
    print("Model:", one_model(records))
    for record in records:
        count = totals[record["arm"]]
        count["results shown"] += record["shown"]
        try:
            decisions = json.loads(record["raw"])["decisions"]
        except (ValueError, KeyError):
            count["unparsed"] += 1
            continue
        count["selected"] += sum(1 for decision in decisions if decision["useForGap"])
        count["rejected out loud"] += sum(1 for decision in decisions if not decision["useForGap"])
    for arm in ARMS:
        print(arm, dict(totals[arm]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--calls")
    parser.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    parser.add_argument("--out")
    parser.add_argument("--seeds", type=int, default=3)
    parser.add_argument("--summary")
    args = parser.parse_args()
    if args.summary:
        summary(args.summary)
    elif args.out and args.calls:
        run(args)
    else:
        sys.exit("give --calls and --out, or --summary")


if __name__ == "__main__":
    main()
