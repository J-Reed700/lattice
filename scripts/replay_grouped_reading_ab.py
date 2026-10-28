"""Offline A/B: the reading as shipped (one reading of every passage, sweeps, then the close read of the
passages it marked with a hyphen) against a grouped reading (every passage shown again in small groups, each
group read on its own, and no close read).

The shipped reading decides each passage thousands of tokens after the passage itself, and the 9B marks
passages that bear on the question with a hyphen; the close read then shows those again, one request per
group that fits. In the 2026-10-24 close-read A/B every fact the close read recovered sat first or second in
the group shown. A grouped reading shows every passage that way from the start, so no decision is far from its
passage and nothing is read twice to repair the first pass.

Arms, all live on one server, each on the same saved reading prompts:
  SHIPPED  main's reading: READING_INSTRUCTION, sweeps while one accounts for a label, attribution, then the
           close read as built (CLOSE_C1, one showing, room = window - prompt - notes - instruction - reservation).
  G<k>     grouped: the message's reading instruction replaced by GROUP_INSTRUCTION with k passages shown again,
           in label order; one sweep per group for labels it left out; attribution over all groups' lines.

Each arm is timed on a warm prefix: before it, one token is asked on the message without its instruction, so
the first arm does not pay the cold prompt alone. The parsing, the copy check (`locate`), attribution and the
fact scoring are scripts/replay_close_read_ab.py's own, imported, so every arm is judged the same way.

  python -I -u scripts/replay_grouped_reading_ab.py --out OUT --llama http://127.0.0.1:8089 \\
    --rubric R1 --rubric R2 ... --calls A.calls.jsonl --calls B.calls.jsonl ... [--arms SHIPPED,G1,G2,G4]
  python -I -u scripts/replay_grouped_reading_ab.py --out OUT --claude claude-haiku-5-5 --budget 10 ...
  python -I scripts/replay_grouped_reading_ab.py --out OUT --summary

On Claude (scripts/replay_claude.py) the message every group shares is cached, so a group pays for its own
passages and instruction only; nothing else about the request differs.
"""
import argparse, collections, json, os, re, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402

# main's READING_INSTRUCTION, SWEEP_INSTRUCTION and CLOSE_READ_INSTRUCTION (reading.rs, close_read.rs).
READING = base.READ_OLD
SWEEP = "You did not account for {labels}. Write one line for each of them, in the same form."
CLOSE = base.CLOSE_C1
# The close read's measured criterion and placement (after the passages), said of passages not yet read.
GROUP = ("Before answering, read these passages from above again, each on its own.\n\n{passages}\n\n"
         "Write one line for each of these passages: its label, then the sentences of that passage that bear on my "
         "question, copied exactly as written and in the passage's own language — or its label and a single hyphen "
         "when it says nothing bearing on my question, however near its subject. Write only these lines.")
INSTRUCTIONS = (base.READ_OLD, base.READ_NEW)


def bare_message(read_user):
    """The turn's message without its reading instruction: what every arm's request shares."""
    for said in INSTRUCTIONS:
        if read_user.endswith(said):
            return read_user[: -len(said)].rstrip()
    sys.exit("a reading request whose message does not end with a reading instruction")


def with_message(read_req, text):
    messages = [dict(m) for m in read_req["messages"]]
    messages[-1]["content"] = text
    return messages


def warm(ctx):
    base.call(ctx["read_req"], "warmup", max_tokens=1, messages=with_message(ctx["read_req"], ctx["bare"]))


def ask(read_req, kind, max_tokens, **extra):
    """base.call, sending max_tokens only when there is a ceiling."""
    if max_tokens is not None:
        extra["max_tokens"] = max_tokens
    return base.call(read_req, kind, **extra)


def parse_into(lines, reply, wanted):
    """The labels in `wanted` this reply accounts for, taken into `lines`."""
    got = {l: n for l, n in base.parse_notes(base.whole_lines(reply["content"], reply["finish"])).items() if l in wanted}
    lines.update(got)
    return set(got)


def shipped(ctx):
    read_req, texts = ctx["read_req"], ctx["texts"]
    reserved = read_req.get("max_tokens") or 0
    stop = base.stops_for(texts)
    message = f"{ctx['bare']}\n\n{READING}"
    reqs, lines, text, written = [], {}, "", 0
    while True:
        missing = sorted(set(texts) - set(lines))
        ceiling = reserved - written if reserved else None
        if ceiling is not None and ceiling <= 0:
            break
        messages = with_message(read_req, message)
        if text:
            if not missing:
                break
            messages += [{"role": "assistant", "content": text},
                         {"role": "user", "content": SWEEP.format(labels=", ".join(f"[{l}]" for l in missing))}]
        r = ask(read_req, "read", ceiling, stop=stop, messages=messages)
        reqs.append(base.req_record(sorted(texts), r, ceiling))
        whole = base.whole_lines(r["content"], r["finish"])
        accounted = parse_into(lines, r, set(texts) if not text else set(missing))
        if not (accounted & set(missing)):
            break
        text = (text + "\n" if text else "") + whole.strip()
        written = base.tokens(text)
    lines, noted, _ = base.attribute(lines, texts)
    if not noted:
        return lines, reqs, {"withdrawn": True}
    ctx2 = dict(ctx, lines0=lines, text0=base.canonical_text(text, lines),
                prompt_tokens=reqs[0].get("prompt_tokens") or base.tokens(message))
    ctx2["read_req"] = dict(read_req, messages=with_message(read_req, message))
    after, _, close_reqs, _, extra = base.close_read_once(ctx2, CLOSE, False, "close")
    return after, reqs + close_reqs, {"close_requests": len(close_reqs), **extra}


def grouped(ctx, k):
    read_req, texts = ctx["read_req"], ctx["texts"]
    reserved = read_req.get("max_tokens")
    labels = sorted(texts)
    reqs, lines, unaccounted = [], {}, []
    for i in range(0, len(labels), k):
        group = labels[i:i + k]
        block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in group)
        rest = f"\n\n{GROUP.format(passages=block)}"
        messages = with_message(read_req, claude.cached(ctx["bare"], rest) if base.CLAUDE else ctx["bare"] + rest)
        stop = [f"[{group[-1] + 1}]", f"\n{group[-1] + 1}."]
        r = ask(read_req, f"G{k}", reserved, stop=stop, messages=messages)
        reqs.append(base.req_record(group, r, reserved))
        got = parse_into(lines, r, set(group))
        left = [l for l in group if l not in got]
        if left and got:
            whole = base.whole_lines(r["content"], r["finish"]).strip()
            sweep = messages + [{"role": "assistant", "content": whole},
                                {"role": "user", "content": SWEEP.format(labels=", ".join(f"[{l}]" for l in left))}]
            r2 = ask(read_req, f"G{k}-sweep", reserved, stop=stop, messages=sweep)
            reqs.append(base.req_record(left, r2, reserved))
            got |= parse_into(lines, r2, set(left))
        unaccounted += [l for l in group if l not in got]
    lines, noted, _ = base.attribute(lines, texts)
    return lines, reqs, {"unaccounted": unaccounted, "withdrawn": not noted}


def record(ctx, arm, out):
    lines, reqs, extra = out
    texts, fl = ctx["texts"], ctx["fl"]
    noted = {l for l, n in lines.items() if n and l in base.locate(n, texts)}
    s = lambda key: sum((q.get(key) or 0) for q in reqs)
    return {"key": ctx["key"], "run": ctx["run"], "query_id": ctx["qid"], "set": ctx["set"], "arm": arm,
            "passages": len(texts), "fact_labels": sorted(fl), "facts_noted": sorted(fl & noted),
            "missed": sorted(fl - noted), "nonfact_lines": sorted(noted - fl),
            "whole_passages": sorted(l for l in noted - fl if base.is_whole(lines[l], texts[l])),
            "requests": len(reqs), "prompt_new": s("prompt_new"), "output_tokens": s("output_tokens"),
            "secs": round(s("secs"), 1), "extra": extra, "reqs": reqs,
            "lines": {str(l): n for l, n in sorted(lines.items())}}


def contexts(args):
    ctxs, seen = [], set()
    for path in args.calls:
        run = os.path.basename(path).split(".")[0]
        for (run_, qid), g in base.group_calls(path, run).items():
            if "read" not in g or qid not in base.items or (run, qid) in seen:
                continue
            seen.add((run, qid))
            ctx = base.base_ctx(run, qid, "replay", g)
            if not ctx["fl"]:
                continue  # no passage holds a fact: nothing to measure
            ctx["bare"] = bare_message(ctx["read_user"])
            ctxs.append(ctx)
    return ctxs


def run(args):
    path = os.path.join(args.out, "records.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a")
    ctxs = contexts(args)
    print(f"{len(ctxs)} readings, {sum(len(c['fl']) for c in ctxs)} fact passages", flush=True)
    for ctx in ctxs:
        line = []
        for arm in args.arms:
            if (ctx["key"], arm) in done:
                continue
            warm(ctx)
            result = shipped(ctx) if arm == "SHIPPED" else grouped(ctx, int(arm[1:]))
            rec = record(ctx, arm, result)
            out.write(json.dumps(rec) + "\n")
            out.flush()
            line.append(f"{arm} {len(rec['facts_noted'])}/{len(rec['fact_labels'])} nf{len(rec['nonfact_lines'])} "
                        f"req{rec['requests']} {rec['secs']}s")
        if line:
            print(ctx["key"], " | ".join(line), flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "records.jsonl"))
    arms = list(dict.fromkeys(r["arm"] for r in recs))
    keys = set.intersection(*({r["key"] for r in recs if r["arm"] == a} for a in arms)) if arms else set()
    print(f"{len(keys)} readings every arm ran\n")
    print("| Arm | Facts noted | Missed readings | Non-fact lines | of them whole passages | Requests | "
          "Prompt tokens processed | Output tokens | Seconds | Median s/reading |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for subset in ("all", "long", "short"):
        for arm in arms:
            rs = [r for r in recs if r["arm"] == arm and r["key"] in keys and subset in ("all", r["set"])]
            if not rs:
                continue
            f = sum(len(r["facts_noted"]) for r in rs)
            of = sum(len(r["fact_labels"]) for r in rs)
            name = arm if subset == "all" else f"{arm} ({subset})"
            print(f"| {name} | {f}/{of} | {sum(1 for r in rs if r['missed'])} | "
                  f"{sum(len(r['nonfact_lines']) for r in rs)} | {sum(len(r['whole_passages']) for r in rs)} | "
                  f"{sum(r['requests'] for r in rs)} | {sum(r['prompt_new'] for r in rs):,} | "
                  f"{sum(r['output_tokens'] for r in rs):,} | {sum(r['secs'] for r in rs):,.0f} | "
                  f"{statistics.median(r['secs'] for r in rs):.1f} |")
    misses = collections.defaultdict(list)
    for r in recs:
        if r["key"] in keys and r["missed"]:
            misses[r["arm"]].append(f"{r['key']} {r['missed']}")
    for arm in arms:
        print(f"\n{arm} missed: {misses[arm] or 'none'}")
    claude.print_spend(os.path.join(args.out, "claude-spend.jsonl"))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--arms", default="SHIPPED,G1,G2,G4")
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    ap.add_argument("--claude", help="a Claude model to run on instead, sent as the app sends it")
    ap.add_argument("--budget", type=float, help="the dollars a Claude run stops at")
    args = ap.parse_args()
    args.arms = [a for a in args.arms.split(",") if a]
    bad = [a for a in args.arms if a != "SHIPPED" and not re.fullmatch(r"G\d+", a)]
    if bad:
        sys.exit(f"unknown arms {bad}")
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    if args.claude:
        claude.use(args.claude, os.path.join(args.out, "claude-spend.jsonl"), args.budget)
        base.CLAUDE = claude
    else:
        base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    run(args)


if __name__ == "__main__":
    main()
