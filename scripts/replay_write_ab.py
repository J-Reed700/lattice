"""Offline A/B of the instruction that turns the reading's notes into the answer (reading.rs WRITE_INSTRUCTION and
CONTINUE_INSTRUCTION), with the answer free to call the turn's tools, as the first round after a reading is.

On the saved reading prompts of scripts/replay_grouped_reading_ab.py, each question in two conditions:
  complete  the question as saved: its passages hold every rubric fact, so a search is a wasted round.
  gap       the question with another document's question added ("… Also, <other question>"). No passage shown
            bears on the added part, so the turn should search for it: the answer template says to search before
            concluding evidence is missing.

Arms, each on the same reading (every passage in one group, as shipped):
  DIRECT  no reading: the answer asked for on the passages and question alone, as when a reading is withdrawn.
  W0      WRITE_INSTRUCTION and CONTINUE_INSTRUCTION as built.
  W6      W0 without "directly", ending: if the passages leave part of my question unanswered, search for that
          part first, then answer.
  W16     W0 with "use every passage your notes quote that answers part of my question", and the continuation
          offered rather than insisted on.
  W6_16   both.
  W6F     searching put first: if the passages leave any part of my question unanswered, do not answer yet but
          search for that part now; otherwise W0's answer, without "Now … directly".

The message is the answer template new installs get (DEFAULT_RAG_PROMPT_TEMPLATE), without its closing "Answer:".
An arm's first answer round may call tools (tool_choice auto). A call is recorded and ends the arm, since a replay
cannot run it; `on_target` says whether its arguments name the part no passage covers. An answer gets its arm's
continuation, calls forbidden, and is scored on the rubric's facts.

  python -X utf8 -I -u scripts/replay_write_ab.py --out OUT --llama URL --rubric R ... --calls C ... [--arms ...]
  python -X utf8 -I -u scripts/replay_write_ab.py --out OUT --claude claude-haiku-5-5 --budget 3 ...
  python -X utf8 -I scripts/replay_write_ab.py --out OUT --summary
"""
import argparse, json, os, re, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402
import replay_grouped_reading_ab as reading  # noqa: E402

OLD_HEAD = ("If the context does not contain the answer, say that the answer is not available in the provided documents "
            "and do not guess. When the answer is unavailable, respond concisely without summarizing or citing "
            "unrelated context.")
# llm.rs DEFAULT_RAG_PROMPT_TEMPLATE, in place of the sentences it replaced.
NEW_HEAD = ("If the excerpts are insufficient, use available document search/read tools before concluding that evidence "
            "is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim "
            "the entire collection lacks it. Do not cite unrelated context.")

W0 = base.WRITE_NEW
C0 = base.CONTINUE
SEARCH = " If the passages leave part of my question unanswered, search for that part first, then answer."
W6 = W0.replace("Now answer my question directly,", "Now answer my question,") + SEARCH
W16 = ("Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. "
       "Use every passage your notes quote that answers part of my question, citing each with its label, and leave out "
       "any that turns out not to bear on it. Do not describe the notes or the passages.")
C16 = ("Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. If any of "
       "them answers part of my question, continue your answer with what it says, in the same form as above, citing it "
       "with its label; do not repeat what you have written. Write nothing for one that does not.")
W6_16 = W16.replace("Now answer my question directly,", "Now answer my question,") + SEARCH
W6F = ("If the passages leave any part of my question unanswered, do not answer yet: search for that part now. "
       "Otherwise, answer my question in the passages' own words, keeping their dates, figures and names as given. "
       "Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, "
       "and say nothing about passages that do not bear on my question.")
ARMS = {"DIRECT": (None, None), "W0": (W0, C0), "W6": (W6, C0), "W16": (W16, C16), "W6_16": (W6_16, C16),
        "W6F": (W6F, C0)}
CONDITIONS = ("complete", "gap")
SEED = 1
SAYS_MISSING = re.compile(r"not (?:found|available|mentioned|covered|included)|no (?:information|mention|passage|record)|"
                          r"(?:do|does|did)(?: not|n't) (?:contain|mention|cover|include|say)|couldn't find|could not find",
                          re.IGNORECASE)


def rewrite_template(bare):
    if OLD_HEAD not in bare:
        sys.exit("a saved message without the old answer template")
    return bare.replace(OLD_HEAD, NEW_HEAD, 1)


def ask_of(qid):
    return base.items[qid]["turns"][-1]["ask"]


def with_gap(bare, qid, other):
    """The message with `other`'s question added to its own."""
    said = f"Question: {ask_of(qid)}"
    if said not in bare:
        sys.exit(f"{qid}: the question is not in its message")
    added = ask_of(other)
    return bare.replace(said, f"{said} Also, {added[0].lower()}{added[1:]}", 1)


# What a search for each question would name.
KEY_TERMS = {
    "dp-long-household-log": ("peallby", "saltings"), "dp-long-hall-transcript": ("reallhollow", "trustees"),
    "dp-long-chronicle": ("greassby", "mill lane"), "dp-long-lab-notebook": ("throassmere", "soil lab"),
    "dp-long-choir-minutes": ("holdmere", "choir"), "dp-long-conservation-record": ("nouftley", "tapestry"),
    "dp-long-family-history": ("fleastmere",), "dp-long-apiary-log": ("slantgate", "apiary"),
    "dp-paraphrase-clock": ("clock", "agree", "time"), "dp-buried-handbook-ticket": ("ticket", "handbook"),
    "dp-buried-agreement-meeting": ("tessivar", "meeting", "court"),
    "dp-section-after-clause": ("tessivar", "24.111", "section 25"), "dp-opening-agreement": ("brackleton",),
    "dp-transcript-rollback": ("roll back", "rollback", "quarterly"),
}


def key_terms(qid):
    return KEY_TERMS[qid]


def partner(qid, qids):
    """Another question about another document: halfway round the list."""
    i = qids.index(qid)
    return qids[(i + len(qids) // 2) % len(qids)]


def call_of(r):
    """The first tool call of a reply, structured or written as text."""
    if r["tool_calls"]:
        f = r["tool_calls"][0]["function"]
        return {"name": f["name"], "arguments": f.get("arguments")}
    start = (r["content"] or "").find("<tool_call>")
    return {"name": "written", "arguments": r["content"][start:start + 400]} if start >= 0 else None


def answer(ctx, messages, kind, tools):
    extra = dict(ctx["write_sampling"])
    extra.update(messages=messages, seed=SEED, tool_choice="auto" if tools else "none", stop=[])
    extra.pop("max_tokens", None)
    return reading.ask(ctx["read_req"], kind, ctx["read_req"].get("max_tokens"), **extra)


def continued(ctx, messages, text, notes, noted, instruction):
    """reading.rs `add_what_was_left_out`, with `instruction` as the continuation."""
    reqs = []
    left = sorted(noted - base.cited(text))
    while left:
        quotes = " ".join(f'[{l}] "{notes[l]}"' for l in left if notes.get(l))
        messages = messages + [{"role": "assistant", "content": text},
                               {"role": "user", "content": instruction.format(quotes=quotes)}]
        r = answer(ctx, messages, "continue", False)
        reqs.append(base.req_record(left, r))
        added = (r["content"] or "").strip()
        if added:
            text = f"{text}\n\n{added}"
        if not (base.cited(added) & set(left)):
            break
        left = sorted(noted - base.cited(text))
    return text, reqs


def first_round(ctx, messages, kind):
    r = answer(ctx, messages, kind, True)
    return r, call_of(r)


def planned(ctx):
    """groups.rs `planned`: every passage in one group, split in two while the window cannot hold it (the room left
    after the group's prompt is less than the passages shown again)."""
    texts, req = ctx["texts"], ctx["read_req"]
    fixed = base.tokens("\n".join(base.text_of(m) for m in req["messages"][:-1])
                        + json.dumps(req.get("tools") or [])) + 8 * len(req["messages"])
    pending, groups = [sorted(texts)], []
    while pending:
        group = pending.pop(0)
        block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in group)
        prompt = fixed + base.tokens(f"{ctx['bare']}\n\n{reading.GROUP.format(passages=block, last=group[-1])}")
        if base.n_ctx() - prompt >= base.tokens(block) or len(group) == 1:
            groups.append(group)
        else:
            pending[:0] = [group[:len(group) // 2], group[len(group) // 2:]]
    return groups


def run_arm(ctx, arm, cache):
    write, cont = ARMS[arm]
    if arm != "DIRECT" and "notes" not in cache:
        groups = planned(ctx)
        lines, reqs, extra = reading.grouped(ctx, 0, groups)
        noted = {l for l, n in lines.items() if n and l in base.locate(n, ctx["texts"])}
        cache["notes"] = (lines, noted, reqs, dict(extra, groups=[len(g) for g in groups]))
    if arm == "DIRECT" or not cache["notes"][1]:
        if "direct" not in cache:
            messages = reading.with_message(ctx["read_req"], ctx["bare"])
            r, called = first_round(ctx, messages, "direct")
            cache["direct"] = (r, called, messages)
        r, called, messages = cache["direct"]
        reqs = [base.req_record(None, r)] + ([] if arm == "DIRECT" else cache["notes"][2])
        return (r["content"] or "").strip(), called, reqs, {"withdrawn": arm != "DIRECT"}
    lines, noted, read_reqs, read = cache["notes"]
    shown = {l: (n if l in noted else None) for l, n in lines.items()}
    messages = reading.with_message(ctx["read_req"], f"{ctx['bare']}\n\n{reading.READING}") + [
        {"role": "assistant", "content": base.render(shown)}, {"role": "user", "content": write}]
    r, called = first_round(ctx, messages, "write")
    reqs = read_reqs + [base.req_record(None, r)]
    if called:
        return (r["content"] or "").strip(), called, reqs, {"noted": sorted(noted), "groups": read["groups"]}
    text, more = continued(ctx, messages, (r["content"] or "").strip(), {l: n for l, n in lines.items() if n},
                           noted, cont)
    return text, None, reqs + more, {"noted": sorted(noted), "groups": read["groups"]}


def record(ctx, arm, out):
    text, called, reqs, extra = out
    facts = base.facts(ctx["qid"])
    missing = base.missing(ctx["qid"], text)
    other = ctx.get("other")
    args = json.dumps(called["arguments"]).lower() if called else ""
    s = lambda key: sum((q.get(key) or 0) for q in reqs)
    return {"key": ctx["key"], "run": ctx["run"], "query_id": ctx["qid"], "set": ctx["set"], "cond": ctx["cond"],
            "other": other, "arm": arm, "called": called,
            "on_target": bool(called and other and any(t in args for t in key_terms(other))),
            "own_target": bool(called and any(t in args for t in key_terms(ctx["qid"]))),
            "facts": len(facts), "stated": 0 if called else len(facts) - len(missing),
            "missing": [] if called else missing, "says_missing": bool(not called and SAYS_MISSING.search(text)),
            "invented": [f[0] for f in base.facts(other) if any(a.lower() in text.lower() for a in f)] if other and not called else [],
            "cited": sorted(base.cited(text)), "cited_without_fact": sorted(base.cited(text) - ctx["fl"]),
            "chars": len(text), "requests": len(reqs), "output_tokens": s("output_tokens"),
            "secs": round(s("secs"), 1), "extra": extra, "answer": text}


def contexts(args):
    ctxs = reading.contexts(args)
    qids = sorted({c["qid"] for c in ctxs}, key=list(base.items).index)
    out = []
    for c in ctxs:
        c["bare"] = rewrite_template(c["bare"])
        for cond in CONDITIONS:
            ctx = dict(c, cond=cond, key=f"{c['key']}|{cond}")
            if cond == "gap":
                ctx["other"] = partner(c["qid"], qids)
                ctx["bare"] = with_gap(c["bare"], c["qid"], ctx["other"])
            out.append(ctx)
    return out


def run(args):
    path = os.path.join(args.out, "answers.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a", encoding="utf-8")
    ctxs = contexts(args)
    print(f"{len(ctxs)} questions and conditions", flush=True)
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
            line.append(f"{arm} " + (f"call {rec['called']['name']}{'*' if rec['on_target'] else ''}" if rec["called"]
                                     else f"{rec['stated']}/{rec['facts']}") + f" {rec['secs']}s")
        print(ctx["key"], " | ".join(line), flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "answers.jsonl"))
    arms = [a for a in ARMS if any(r["arm"] == a for r in recs)]
    print("Calls: answer rounds that called a tool (* = naming the part no passage covers). Facts, length and "
          "citations are over the rounds that answered.\n")
    print("| Condition | Arm | Questions | Calls (on target) | Facts stated | Says something is missing | "
          "Cited, holding no fact | Median chars | Median s |")
    print("|---|---|---|---|---|---|---|---|---|")
    for cond in CONDITIONS:
        keys = set.intersection(*({r["key"] for r in recs if r["arm"] == a and r["cond"] == cond} for a in arms)) \
            if arms else set()
        for arm in arms:
            rs = [r for r in recs if r["arm"] == arm and r["key"] in keys]
            if not rs:
                continue
            said = [r for r in rs if not r["called"]]
            print(f"| {cond} | {arm} | {len(rs)} | {sum(1 for r in rs if r['called'])} "
                  f"({sum(1 for r in rs if r['on_target'])}) | {sum(r['stated'] for r in said)}/"
                  f"{sum(r['facts'] for r in said)} | {sum(1 for r in said if r['says_missing'])}/{len(said)} | "
                  f"{sum(len(r['cited_without_fact']) for r in said)} | "
                  f"{statistics.median(r['chars'] for r in said) if said else 0:.0f} | "
                  f"{statistics.median(r['secs'] for r in rs):.1f} |")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    args.arms = [a for a in args.arms.split(",") if a]
    if bad := [a for a in args.arms if a not in ARMS]:
        sys.exit(f"unknown arms {bad}")
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    if args.claude:
        claude.use(args.claude, os.path.join(args.out, "claude-spend.jsonl"), args.budget, thinking=args.thinking)
        base.CLAUDE = claude
    else:
        base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    run(args)


if __name__ == "__main__":
    main()
