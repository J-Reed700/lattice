"""Offline A/B of how a grounded answer is written, scored on the answers themselves.

On the saved reading prompts of scripts/replay_grouped_reading_ab.py (the same questions, passages and rubric
facts), four ways to write the answer, every arm live on one server:

  DIRECT    the answer from the passages alone: the reading instruction taken back, as before reading existed.
  NOTES     the grouped reading (groups of four), then the answer written from the notes as kept, then the
            coverage continuation for passages the notes copy and the answer does not cite (reading.rs as built).
  NOTESV    as NOTES, but the writer is given only the notes that are a verified copy of their own passage; any
            other note is replayed as a hyphen.
  CHECK     DIRECT's answer, then the grouped reading used only as a check: the coverage continuation for the
            passages its notes copy and the answer does not cite.

A reading with no verified copy is withdrawn, as the app does: NOTES and NOTESV then answer as DIRECT. Writes
forbid tool calls in every arm (a replay cannot run them), so the arms differ only in how the answer is written.
A fact counts when the answer states it in one of the rubric's accepted forms (replay_close_read_ab.missing).

  python -X utf8 -I -u scripts/replay_answer_ab.py --out OUT --llama URL --rubric R ... --calls C ... [--arms ...]
  python -X utf8 -I scripts/replay_answer_ab.py --out OUT --summary
"""
import argparse, json, os, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_close_read_ab as base  # noqa: E402
import replay_grouped_reading_ab as reading  # noqa: E402

WRITE = base.WRITE_NEW
ARMS = ("DIRECT", "NOTES", "NOTESV", "CHECK")
SEED = 1


def write_args(ctx, messages):
    """The shipped answer's sampling, a fixed seed, and calls forbidden."""
    extra = dict(ctx["write_sampling"])
    extra.update(messages=messages, seed=SEED, tool_choice="none")
    extra.pop("max_tokens", None)
    return extra


def answer(ctx, messages, kind):
    r = reading.ask(ctx["read_req"], kind, ctx["read_req"].get("max_tokens"), **write_args(ctx, messages))
    return (r["content"] or "").strip(), r


def continue_coverage(ctx, messages, text, notes, noted):
    """reading.rs `add_what_was_left_out`: while a continuation cites one of the left-out passages."""
    reqs = []
    left = sorted(noted - base.cited(text))
    while left:
        quotes = " ".join(f'[{l}] "{notes[l]}"' for l in left if notes.get(l))
        messages = messages + [{"role": "assistant", "content": text},
                               {"role": "user", "content": base.CONTINUE.format(quotes=quotes)}]
        added, r = answer(ctx, messages, "continue")
        reqs.append(base.req_record(left, r))
        if added:
            text = f"{text}\n\n{added}"
        if not (base.cited(added) & set(left)):
            break
        left = sorted(noted - base.cited(text))
    return text, reqs


def direct(ctx):
    messages = reading.with_message(ctx["read_req"], ctx["bare"])
    text, r = answer(ctx, messages, "direct")
    return text, messages, [base.req_record(None, r)]


def grouped_notes(ctx):
    lines, reqs, extra = reading.grouped(ctx, 4)
    texts = ctx["texts"]
    noted = {l for l, n in lines.items() if n and l in base.locate(n, texts)}
    return lines, noted, reqs


def from_notes(ctx, lines, noted, verified_only):
    shown = {l: (n if (not verified_only or l in noted) else None) for l, n in lines.items()}
    notes_text = base.render(shown)
    messages = reading.with_message(ctx["read_req"], f"{ctx['bare']}\n\n{reading.READING}") + [
        {"role": "assistant", "content": notes_text}, {"role": "user", "content": WRITE}]
    text, r = answer(ctx, messages, "write")
    text, more = continue_coverage(ctx, messages, text, {l: n for l, n in lines.items() if n}, noted)
    return text, [base.req_record(None, r)] + more


def run_arm(ctx, arm, cache):
    """One arm's answer. The grouped notes and the direct answer are computed once per question and shared by
    the arms that use them, so the arms differ only where their flows do."""
    if arm in ("NOTES", "NOTESV", "CHECK") and "notes" not in cache:
        cache["notes"] = grouped_notes(ctx)
    if arm in ("DIRECT", "CHECK") and "direct" not in cache:
        cache["direct"] = direct(ctx)
    if arm == "DIRECT":
        text, _, reqs = cache["direct"]
        return text, reqs, {}
    lines, noted, read_reqs = cache["notes"]
    if arm == "CHECK":
        text, messages, reqs = cache["direct"]
        text, more = continue_coverage(ctx, messages, text, {l: n for l, n in lines.items() if n}, noted)
        return text, reqs + read_reqs + more, {"noted": sorted(noted)}
    if not noted:
        if "direct" not in cache:
            cache["direct"] = direct(ctx)
        text, _, reqs = cache["direct"]
        return text, read_reqs + reqs, {"withdrawn": True}
    text, reqs = from_notes(ctx, lines, noted, arm == "NOTESV")
    return text, read_reqs + reqs, {"noted": sorted(noted)}


def record(ctx, arm, out):
    text, reqs, extra = out
    facts = base.facts(ctx["qid"])
    missing = base.missing(ctx["qid"], text)
    s = lambda key: sum((q.get(key) or 0) for q in reqs)
    return {"key": ctx["key"], "run": ctx["run"], "query_id": ctx["qid"], "set": ctx["set"], "arm": arm,
            "facts": len(facts), "stated": len(facts) - len(missing), "missing": missing,
            "abstains": base.abstains(text), "cited": sorted(base.cited(text)), "chars": len(text),
            "requests": len(reqs), "prompt_new": s("prompt_new"), "output_tokens": s("output_tokens"),
            "secs": round(s("secs"), 1), "extra": extra, "answer": text}


def run(args):
    path = os.path.join(args.out, "answers.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a", encoding="utf-8")
    ctxs = reading.contexts(args)
    print(f"{len(ctxs)} questions", flush=True)
    for ctx in ctxs:
        todo = [a for a in args.arms if (ctx["key"], a) not in done]
        if not todo:
            continue
        reading.warm(ctx)
        cache, line = {}, []
        for arm in todo:
            rec = record(ctx, arm, run_arm(ctx, arm, cache))
            out.write(json.dumps(rec) + "\n")
            out.flush()
            line.append(f"{arm} {rec['stated']}/{rec['facts']} {rec['secs']}s")
        print(ctx["key"], " | ".join(line), flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "answers.jsonl"))
    arms = [a for a in ARMS if any(r["arm"] == a for r in recs)]
    keys = set.intersection(*({r["key"] for r in recs if r["arm"] == a} for a in arms)) if arms else set()
    print(f"{len(keys)} questions every arm answered (times include the reading where the arm reads)\n")
    print("| Arm | Facts stated | Answers with every fact | Long: facts | Short: answers right | Requests | Median s |")
    print("|---|---|---|---|---|---|---|")
    for arm in arms:
        rs = [r for r in recs if r["arm"] == arm and r["key"] in keys]
        long_ = [r for r in rs if r["set"] == "long"]
        short = [r for r in rs if r["set"] == "short"]
        print(f"| {arm} | {sum(r['stated'] for r in rs)}/{sum(r['facts'] for r in rs)} | "
              f"{sum(1 for r in rs if not r['missing'])}/{len(rs)} | "
              f"{sum(r['stated'] for r in long_)}/{sum(r['facts'] for r in long_)} | "
              f"{sum(1 for r in short if not r['missing'] and not r['abstains'])}/{len(short)} | "
              f"{sum(r['requests'] for r in rs)} | {statistics.median(r['secs'] for r in rs):.1f} |")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    args = ap.parse_args()
    args.arms = [a for a in args.arms.split(",") if a]
    if bad := [a for a in args.arms if a not in ARMS]:
        sys.exit(f"unknown arms {bad}")
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    run(args)


if __name__ == "__main__":
    main()
