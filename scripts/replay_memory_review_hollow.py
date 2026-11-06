#!/usr/bin/env python3
"""How often Claude answers a memory review without reading it, which request changes that, and what asking again leaves.

Some of Haiku's replies to the reviewer's request are one short object: the first id, "supported", and a reason that is
empty or a filler word. This sends the reviewer's own request, as the app's Claude adapter sends it, for the scenarios in
replay_memory_review_ab.py, under each arm:

  now, bound, min     the schema as shipped; the reason's maxLength 512 as before #301; its minLength 1
  one, one_min        the request holding one addition, as shipped and with minLength 1
  adaptive            as shipped
  disabled            thinking disabled
  disabled_low        thinking disabled, effort low
  adaptive_medium     effort medium
  adaptive_high       effort high
  keyed               verdicts as an object with one required property per id
  sentence            "Return only JSON..." back in the prompt

`--reask` replays verify.rs instead: a verdict counts only with a reason, and the reviews a reply did not read are asked for
once more, on their own. Its arms are `three` (two additions and a transition) and `one` (one addition).

  python -X utf8 -I -u scripts/replay_memory_review_hollow.py --prompts A/prompts.rs --out OUT.jsonl --arms now,bound,min,one,one_min
  python -X utf8 -I -u scripts/replay_memory_review_hollow.py --prompts A/prompts.rs --out OUT.jsonl --reask
  python -X utf8 -I scripts/replay_memory_review_hollow.py --summary OUT.jsonl

`--prompts` is conversation/memory/prompts.rs from a checkout. Arms are shuffled together, so time of day cannot tell them
apart, and a run resumes by skipping rows already written. The API key is never printed.
"""
import argparse
import concurrent.futures as futures
import copy
import json
import random
import statistics
import sys
import threading
import time
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_claude as claude  # noqa: E402
import replay_memory_review_ab as ab  # noqa: E402

IDS = ["addition:c1", "addition:c2", "transition:c1"]
WORKERS = 6
ADAPTIVE = {"type": "adaptive"}
DISABLED = {"type": "disabled"}
SENTENCE = "Return only JSON matching the requested schema, with no commentary, explanation, or markdown outside it."
AFTER_THE_SENTENCE_ASKING_FOR_VERDICTS = "with a reason of one short sentence."
# What each scenario's transition needs, from the scenarios' own reading in 2026-11-02-REVIEWER-ON-HAIKU.md.
NEED = {
    "conflict dates": "ambiguous",
    "conflict budget": "ambiguous",
    "clean correction": "supported",
    "restatement": "supported",
    "partial revocation": "supported",
    "moved on": "unsupported",
}


def schema_with_reason(**bounds):
    schema = copy.deepcopy(ab.SCHEMA)
    schema["properties"]["verdicts"]["items"]["properties"]["reason"].update(bounds)
    return schema


def keyed_schema():
    item = copy.deepcopy(ab.SCHEMA["properties"]["verdicts"]["items"])
    item["properties"].pop("id")
    item["required"] = ["verdict", "reason"]
    return {
        "type": "object",
        "additionalProperties": False,
        "required": ["verdicts"],
        "properties": {"verdicts": {"type": "object", "additionalProperties": False, "required": IDS, "properties": {i: item for i in IDS}}},
    }


ARMS = {
    "now": {"seeds": 100},
    "bound": {"seeds": 100, "reason": {"maxLength": 512}},
    "min": {"seeds": 100, "reason": {"minLength": 1}},
    "one": {"seeds": 50, "ids": ["addition:c1"]},
    "one_min": {"seeds": 50, "ids": ["addition:c1"], "reason": {"minLength": 1}},
    "adaptive": {"seeds": 40},
    "disabled": {"seeds": 40, "thinking": DISABLED},
    "disabled_low": {"seeds": 40, "thinking": DISABLED, "effort": "low"},
    "adaptive_medium": {"seeds": 40, "effort": "medium"},
    "adaptive_high": {"seeds": 40, "effort": "high"},
    "keyed": {"seeds": 40, "keyed": True},
    "sentence": {"seeds": 40, "sentence": True},
}
REASK_ARMS = {"three": IDS, "one": ["addition:c1"]}
REASK_SEEDS = 50
ASKS = 2


def gives_a_reason(reason):
    """`gives_a_reason` in conversation/memory/verify.rs."""
    reason = str(reason).strip()
    return any(c.isspace() for c in reason) or (bool(reason) and not reason.isascii())


def with_sentence(system):
    assert system.count(AFTER_THE_SENTENCE_ASKING_FOR_VERDICTS) == 1
    return system.replace(AFTER_THE_SENTENCE_ASKING_FOR_VERDICTS, f"{AFTER_THE_SENTENCE_ASKING_FOR_VERDICTS} {SENTENCE}")


def only(user, ids):
    body = json.loads(user)
    body["reviews"] = [review for review in body["reviews"] if review["id"] in ids]
    return json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def verdicts_of(raw):
    verdicts = json.loads(raw)["verdicts"]
    if isinstance(verdicts, dict):
        return [{"id": name, **verdict} for name, verdict in verdicts.items()]
    return verdicts


def read_reply(raw, asked):
    """What verify.rs takes from a reply: verdicts with a reason, for ids this call asked about."""
    try:
        verdicts = verdicts_of(raw)
    except Exception:
        return {}
    return {v["id"]: v for v in verdicts if v.get("id") in asked and gives_a_reason(v.get("reason", ""))}


def send(client, system, user, schema, thinking=None, effort=None):
    req = {
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "max_tokens": ab.VERIFIER_OUTPUT_TOKENS,
        "response_format": {"type": "json_schema", "json_schema": {"name": "response", "strict": True, "schema": schema}},
    }
    body = claude.body_of(req)
    extra = {key: body.pop(key) for key in ("cache_control", "output_config") if key in body}
    if thinking:
        body["thinking"] = thinking
    if effort:
        extra["output_config"]["effort"] = effort
    with client.messages.stream(**body, extra_body=extra or None) as stream:
        message = stream.get_final_message()
    return "".join(block.text for block in message.content if block.type == "text"), message


def one_arm_row(client, system, name, scenario, seed):
    arm = ARMS[name]
    schema = keyed_schema() if arm.get("keyed") else schema_with_reason(**arm.get("reason", {}))
    arm_system = with_sentence(system) if arm.get("sentence") else system
    user = only(ab.payload(scenario, ab.QUESTIONS), arm.get("ids", IDS))
    raw, message = send(client, arm_system, user, schema, arm.get("thinking"), arm.get("effort"))
    return {"raw": raw, "stop": message.stop_reason, "output": message.usage.output_tokens}


def one_reask_row(client, system, name, scenario, seed):
    user = only(ab.payload(scenario, ab.QUESTIONS), REASK_ARMS[name])
    read, left, asks = {}, set(REASK_ARMS[name]), []
    for _ in range(ASKS):
        raw, message = send(client, system, only(user, left), ab.SCHEMA)
        asks.append({"asked": sorted(left), "raw": raw, "output": message.usage.output_tokens})
        got = read_reply(raw, left)
        read.update(got)
        left -= set(got)
        if not left:
            break
    return {"asks": asks, "read": read, "left": sorted(left)}


def run(args):
    import anthropic

    claude.use(args.model, None, None)  # threads must not share a ledger file: a corrupt line stops every later call
    client = anthropic.Anthropic(api_key=claude.api_key(), max_retries=10, timeout=900)
    system = ab.verifier_system(args.prompts)
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["arm"], r["scenario"], r["seed"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    seeds = {name: REASK_SEEDS for name in REASK_ARMS} if args.reask else {name: ARMS[name]["seeds"] for name in args.arms.split(",")}
    tasks = [
        (name, scenario, seed)
        for name, count in seeds.items()
        for scenario in ab.SCENARIOS
        for seed in range(1, count + 1)
        if (name, scenario["name"], seed) not in done
    ]
    random.Random(20261106).shuffle(tasks)
    print(f"{len(tasks)} rows to run, {len(done)} done, {time.strftime('%H:%M')}", flush=True)
    lock = threading.Lock()
    one_row = one_reask_row if args.reask else one_arm_row

    def one(task):
        name, scenario, seed = task
        row = {"model": args.model, "arm": name, "scenario": scenario["name"], "seed": seed}
        try:
            row.update(one_row(client, system, name, scenario, seed))
        except Exception as error:  # a 400 on a schema is itself a result
            row.update(error=f"{type(error).__name__}: {str(error)[:300]}")
        with lock, out.open("a", encoding="utf-8") as sink:
            sink.write(json.dumps(row, ensure_ascii=False) + "\n")

    with futures.ThreadPoolExecutor(WORKERS) as pool:
        list(pool.map(one, tasks))
    print("done", time.strftime("%H:%M"), flush=True)


def arm_row_tally(row, tally, tokens):
    if "error" in row:
        tally["error"] += 1
        return
    tally["replies"] += 1
    tokens.append(row["output"])
    expected = ARMS[row["arm"]].get("ids", IDS)
    read = read_reply(row["raw"], set(expected))
    if set(expected) - set(read):
        tally["hollow"] += 1
    if "transition:c1" in expected:
        got = read.get("transition:c1")
        if got is None:
            tally["transition unread"] += 1
        else:
            tally["transition right" if got["verdict"] == NEED[row["scenario"]] else "transition WRONG"] += 1


def reask_row_tally(row, tally, tokens):
    if "error" in row:
        tally["error"] += 1
        return
    tally["requests"] += 1
    tally["calls"] += len(row["asks"])
    tokens.extend(ask["output"] for ask in row["asks"])
    if len(row["asks"]) > 1:
        tally["asked again"] += 1
    if row["left"]:
        tally["still unread after two asks"] += 1
    if row["arm"] == "three":
        got = row["read"].get("transition:c1")
        if got is None:
            tally["transition unread"] += 1
        else:
            tally["transition right" if got["verdict"] == NEED[row["scenario"]] else "transition WRONG"] += 1


def summary(path):
    rows = [json.loads(line) for line in Path(path).read_text("utf-8").splitlines() if line.strip()]
    print("model", sorted({row["model"] for row in rows}))
    tallies, tokens = defaultdict(Counter), defaultdict(list)
    for row in rows:
        counted = reask_row_tally if "asks" in row else arm_row_tally
        counted(row, tallies[row["arm"]], tokens[row["arm"]])
    for arm, tally in tallies.items():
        mean = round(statistics.mean(tokens[arm])) if tokens[arm] else None
        print(arm, dict(tally), "| mean output tokens", mean)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--prompts")
    ap.add_argument("--out")
    ap.add_argument("--summary")
    ap.add_argument("--arms", default="now,bound,min,one,one_min")
    ap.add_argument("--reask", action="store_true")
    ap.add_argument("--model", default="claude-haiku-5-5")
    args = ap.parse_args()
    if args.summary:
        summary(args.summary)
    elif args.out and args.prompts:
        run(args)
    else:
        sys.exit("give --summary, or --prompts and --out")


if __name__ == "__main__":
    main()
