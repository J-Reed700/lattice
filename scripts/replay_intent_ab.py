"""Offline A/B of the turn-intent classifier's context (chat/intent.rs): each logged intent request sent as logged,
and with the conversation's system prompt taken out of its "Recent conversation" block, where
conversation_context.rs puts it as a `System:` turn.

Every desktop-path-v1 question is about the user's own documents, so the right flags are needsKnowledgeBase true
and needsWeb false. isFollowup is true on the later turns of its multi-turn items, except the two that change the
subject ("Unrelated question: …", "Different subject: …"). The intent request samples (temperature 0.7 in
the app), so each arm is asked with several seeds.

  python -X utf8 -I -u scripts/replay_intent_ab.py --out OUT --llama URL --calls C.calls.jsonl.gz ... [--seeds 5]
  python -X utf8 -I -u scripts/replay_intent_ab.py --out OUT --claude claude-haiku-5-5 --budget 1 --calls ...
  python -X utf8 -I scripts/replay_intent_ab.py --out OUT --summary
"""
import argparse, collections, gzip, json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402

MARK = "You classify one turn of a conversation"
HEAD, TAIL = "Recent conversation:\n", "\n\nUser message: "
ARMS = ("AS_SENT", "NO_SYSTEM")
STANDS_ALONE = {("dp-mt-topic-shift", 1), ("dp-mt-shift-and-return", 1)}


def follows(qid, turn):
    return turn > 0 and (qid, turn) not in STANDS_ALONE


def without_system(prompt):
    """The prompt with its `System:` turn removed; "(no prior turns)" when nothing else is left, as intent.rs says."""
    start = prompt.index(HEAD) + len(HEAD)
    end = prompt.index(TAIL, start)
    block = prompt[start:end]
    if not block.startswith("System: "):
        return prompt
    rest = min((i for i in (block.find("\nUser: "), block.find("\nAssistant: ")) if i >= 0), default=-1)
    kept = block[rest + 1:] if rest >= 0 else "(no prior turns)"
    return prompt[:start] + kept + prompt[end:]


def requests(paths):
    """(run, query id, turn of that item, request) for every logged intent request."""
    out = []
    for path in paths:
        run = os.path.basename(path).split(".")[0]
        turns = collections.Counter()
        opener = gzip.open if path.endswith(".gz") else open
        for line in opener(path, "rt", encoding="utf-8"):
            row = json.loads(line)
            req = (row.get("call") or {}).get("request")
            if not isinstance(req, dict) or not req.get("messages"):
                continue
            if MARK in base.text_of(req["messages"][-1]):
                out.append((run, row["query_id"], turns[row["query_id"]], req))
                turns[row["query_id"]] += 1
    return out


def ask(req, prompt, seed):
    messages = [dict(m) for m in req["messages"]]
    messages[-1]["content"] = prompt
    r = base.call(req, "intent", messages=messages, seed=seed)
    try:
        return json.loads(r["content"][r["content"].index("{"):r["content"].rindex("}") + 1]), r
    except ValueError:
        return None, r


def run(args):
    path = os.path.join(args.out, "intent.jsonl")
    done = {(r["key"], r["arm"], r["seed"]) for r in base.read_out(path)}
    out = open(path, "a", encoding="utf-8")
    reqs = requests(args.calls)
    print(f"{len(reqs)} intent requests", flush=True)
    for run_, qid, turn, req in reqs:
        key = f"{run_}|{qid}|{turn}"
        prompt = base.text_of(req["messages"][-1])
        for arm in ARMS:
            sent = prompt if arm == "AS_SENT" else without_system(prompt)
            for seed in range(1, args.seeds + 1):
                if (key, arm, seed) in done:
                    continue
                flags, r = ask(req, sent, seed)
                out.write(json.dumps({"key": key, "query_id": qid, "turn": turn, "arm": arm, "seed": seed,
                                      "flags": flags, "prompt_chars": len(sent), "secs": r["secs"]}) + "\n")
                out.flush()
        print(key, flush=True)


def summary(args):
    recs = [r for r in base.read_out(os.path.join(args.out, "intent.jsonl"))]
    print("| Arm | Answers | Unparsed | needsKnowledgeBase true | needsWeb false | isFollowup right | "
          "Median prompt chars |")
    print("|---|---|---|---|---|---|---|")
    for arm in ARMS:
        rs = [r for r in recs if r["arm"] == arm]
        ok = [r for r in rs if r["flags"]]
        chars = sorted(r["prompt_chars"] for r in rs)
        print(f"| {arm} | {len(rs)} | {len(rs) - len(ok)} | {sum(1 for r in ok if r['flags'].get('needsKnowledgeBase'))}"
              f"/{len(ok)} | {sum(1 for r in ok if not r['flags'].get('needsWeb'))}/{len(ok)} | "
              f"{sum(1 for r in ok if bool(r['flags'].get('isFollowup')) == follows(r['query_id'], r['turn']))}"
              f"/{len(ok)} | "
              f"{chars[len(chars) // 2] if chars else 0} |")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--seeds", type=int, default=5)
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    if args.claude:
        claude.use(args.claude, os.path.join(args.out, "claude-spend.jsonl"), args.budget, thinking=args.thinking)
        base.CLAUDE = claude
    else:
        base.URL = args.llama
    run(args)


if __name__ == "__main__":
    main()
