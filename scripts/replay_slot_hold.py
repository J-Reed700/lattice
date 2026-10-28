"""Timing for the chat slot hold on a one-slot llama-server: a chat turn's requests back to back on its cached
prompt (background work waits for the turn), against the same requests with a background request run in every
gap between them (-FREE: a course build or a check queued throughout the turn takes the slot each time the
turn's last request finishes and before its next one is sent).

  COLD   the reading's groups as main sends them, the first one on a cold prompt.
  WARM   one token asked on the message without its instruction first, so a hybrid model's server keeps a
         checkpoint where every group's prompt diverges.

The turn is main's grouped reading of a saved reading prompt (scripts/replay_grouped_reading_ab.py's G4); the
background request is another question's first group request, cut at BACKGROUND_TOKENS. Every message opens with
a fresh nonce, so no arm or background request finds its prompt in the server's RAM cache, and each arm starts
after a background request. Arm order alternates.

  python -I -u scripts/replay_slot_hold.py --out OUT --llama http://127.0.0.1:8089 --rubric R ... \\
    --calls A.calls.jsonl ... [--turns 12] [--arms COLD,WARM,COLD-FREE,WARM-FREE]
  python -I scripts/replay_slot_hold.py --out OUT --summary
"""
import argparse, json, os, statistics, sys, time, uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_close_read_ab as base  # noqa: E402
import replay_grouped_reading_ab as grouped  # noqa: E402

GROUP_SIZE = 4
BACKGROUND_TOKENS = 256
ARMS = ("COLD", "WARM", "COLD-FREE", "WARM-FREE")


def group_requests(ctx):
    texts, labels = ctx["texts"], sorted(ctx["texts"])
    out = []
    for i in range(0, len(labels), GROUP_SIZE):
        group = labels[i:i + GROUP_SIZE]
        block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in group)
        out.append({"messages": grouped.with_message(ctx["read_req"], f"{ctx['bare']}\n\n{grouped.GROUP.format(passages=block)}"),
                    "stop": [f"[{group[-1] + 1}]", f"\n{group[-1] + 1}."]})
    return out


def timed(req, kind, **extra):
    t = time.time()
    r = base.call(req, kind, **extra)
    return {"kind": kind, "wall": round(time.time() - t, 2), "prompt_new": r["timings"].get("prompt_n"),
            "cache_n": r["timings"].get("cache_n"), "output": (r.get("usage") or {}).get("completion_tokens")}


def unseen(ctx):
    """`ctx` with a message no request has sent before: llama-server keeps recent prompts in RAM (--cache-ram),
    so a prompt an earlier arm sent would be restored, not read."""
    return dict(ctx, bare=f"(Turn {uuid.uuid4()}.)\n\n{ctx['bare']}")


def background(other):
    first = group_requests(unseen(other))[0]
    return timed(other["read_req"], "background", max_tokens=BACKGROUND_TOKENS, **first)


def turn(ctx, other, arm):
    ctx = unseen(ctx)
    reserved = ctx["read_req"].get("max_tokens")
    requests = [dict(max_tokens=reserved, **req) for req in group_requests(ctx)]
    if arm.startswith("WARM"):
        requests.insert(0, {"max_tokens": 1, "messages": grouped.with_message(ctx["read_req"], ctx["bare"])})
    calls = [background(other)]
    start = time.time()
    for i, req in enumerate(requests):
        if arm.endswith("-FREE") and i:
            calls.append(background(other))
        calls.append(timed(ctx["read_req"], "chat", **req))
    turn_secs = time.time() - start
    if not arm.endswith("-FREE"):
        calls.append(background(other))
    return {"turn_secs": round(turn_secs, 1), "calls": calls}


def run(args):
    path = os.path.join(args.out, "records.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a")
    seen, chosen = set(), []
    for ctx in grouped.contexts(args):
        if ctx["qid"] not in seen:
            seen.add(ctx["qid"])
            chosen.append(ctx)
    sets = [[c for c in chosen if c["set"] == s] for s in ("long", "short")]
    chosen = [c for pair in zip(*sets) for c in pair][:args.turns]
    for i, ctx in enumerate(chosen):
        other = chosen[(i + 1) % len(chosen)]
        line = []
        for arm in (args.arms if i % 2 == 0 else list(reversed(args.arms))):
            if (ctx["key"], arm) in done:
                continue
            rec = {"key": ctx["key"], "arm": arm, "background_of": other["key"], **turn(ctx, other, arm)}
            out.write(json.dumps(rec) + "\n")
            out.flush()
            chat = [c for c in rec["calls"] if c["kind"] == "chat"]
            line.append(f"{arm} {rec['turn_secs']}s reprocessed {sum(c['prompt_new'] or 0 for c in chat):,}")
        if line:
            print(ctx["key"], " | ".join(line), flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "records.jsonl"))
    by = {}
    for r in recs:
        by.setdefault(r["key"], {})[r["arm"]] = r
    arms = list(dict.fromkeys(r["arm"] for r in recs))
    turns = [v for v in by.values() if all(a in v for a in arms)]
    print(f"{len(turns)} turns, every arm\n")
    print("| Arm | Median turn s | Total turn s | Chat prompt tokens processed | Chat requests | Background s in the turn |")
    print("|---|---|---|---|---|---|")
    for arm in arms:
        rs = [t[arm] for t in turns]
        chat = [c for r in rs for c in r["calls"] if c["kind"] == "chat"]
        inside = [c for r in rs for c in r["calls"][1:] if c["kind"] == "background"] if arm.endswith("-FREE") else []
        print(f"| {arm} | {statistics.median(r['turn_secs'] for r in rs):.1f} | {sum(r['turn_secs'] for r in rs):,.0f} | "
              f"{sum(c['prompt_new'] or 0 for c in chat):,} | {len(chat)} | {sum(c['wall'] for c in inside):,.0f} |")
    print()
    for a, b in (("COLD-FREE", "COLD"), ("WARM-FREE", "WARM"), ("COLD", "WARM"), ("COLD-FREE", "WARM")):
        ratios = [t[a]["turn_secs"] / t[b]["turn_secs"] for t in turns if a in t and b in t and t[b]["turn_secs"]]
        if ratios:
            print(f"{a}/{b} turn time: median {statistics.median(ratios):.2f}x, range {min(ratios):.2f}-{max(ratios):.2f}x")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--turns", type=int, default=12)
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    args = ap.parse_args()
    args.arms = [a for a in args.arms.split(",") if a]
    if any(a not in ARMS for a in args.arms):
        sys.exit(f"arms are {ARMS}")
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    run(args)


if __name__ == "__main__":
    main()
