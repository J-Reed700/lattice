"""Offline A/B for condensing a turn's tool results (chat/tool_loop/research_notes.rs): one request that rewrites
every result into notes against one request per result, and where its instruction is said.

The saved reading prompts' passages stand in for what a research turn has read: the message keeps its
instructions and question with the Knowledge Base Results block taken out, and the passages come back as the
results of the turn's calls (`semantic_search` for the question, then `get_document` for the section a result
opens with), RESULT_SIZE passages to a result, in label order, while they stay within RESULTS_CHARS: the share
of a 16k window the loop's results reach before they are condensed, less the room the message and the
reservation take. Every request is the shipped condensing request: the transcript, CONDENSE_INSTRUCTION said last,
the same tools forbidden, deterministic sampling, thinking off, the round's reservation.

  ONE    every result rewritten in one request, the instruction after the last result (`say_last`), as
         shipped until 2026-10-28.
  EACH   one request per result: CONDENSE_EACH names the result's call and shows the result again after it.
  NAMED  one request per result, naming its call only: the transcript's cached prompt and a line more.
  ONE-USER  ONE with the instruction as a user message of its own after the last result, as shipped.
  ONE-BARE  ONE sent without the tools, so the model has no call to make (the prompt is read again from its head).

Each reply is taken as `usable_notes` takes it: the text before any call written into it, and nothing when it
was cut off or holds nothing else; an unusable reply leaves its results as they were, as the app carries on
without condensing them. A fact is kept when the transcript after condensing holds one of the rubric's spellings
for it (case-insensitive), of the facts whose passage is among the results; it keeps its citation when the
passage's label is on the same line or the last one cited before it.

  python -I -u scripts/replay_condense_ab.py --out OUT --llama http://127.0.0.1:8089 --rubric R ... \\
    --calls A.calls.jsonl ... [--arms ONE,EACH,NAMED,ONE-USER,ONE-BARE]
  python -I scripts/replay_condense_ab.py --out OUT --summary
"""
import argparse, json, os, re, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_close_read_ab as base  # noqa: E402
import replay_cut_evidence_ab as cut  # noqa: E402
import replay_grouped_reading_ab as grouped  # noqa: E402

RESULT_SIZE = 4
RESULTS_CHARS = 26_000
ARMS = ("ONE", "ONE-USER", "ONE-BARE", "EACH", "NAMED")
CALL_MARKERS = ("<tool_call>", "<function=", "[TOOL_CALLS]", "<|tool_call")  # final_round.rs LEAKED_CALL_MARKERS
SEPARATOR = "\n\n---\nFrom me, not part of the result above: "
# research_notes.rs CONDENSE_INSTRUCTION.
CONDENSE = ("Your context window is nearly full. Before you continue, rewrite all the tool results you have so far into "
            "compact research notes. Keep every fact, number, date, name, short quotation and URL that could matter to my "
            "question, and keep each citation number exactly as it is written, for example [3], next to the facts it "
            "supports. Drop navigation text, boilerplate, repetition and anything that does not bear on my question. "
            "Group the notes by source, one short line per fact. Do not answer my question yet and do not call any "
            "tools: write only the notes.")
CONDENSE_EACH = ("Your context window is nearly full. Before you continue, rewrite the result of your call {call} into "
                 "compact research notes. Here it is again:\n\n{result}\n\nKeep every fact, number, date, name, short "
                 "quotation and URL in it that could matter to my question, and keep each citation number exactly as it "
                 "is written, for example [3], next to the facts it supports. Drop navigation text, boilerplate, "
                 "repetition and anything that does not bear on my question. Group the notes by source, one short line "
                 "per fact. Do not answer my question yet and do not call any tools: write only the notes.")
CONDENSE_NAMED = ("Your context window is nearly full. Before you continue, rewrite the result of your call {call} into "
                  "compact research notes. Keep every fact, number, date, name, short quotation and URL in it that could "
                  "matter to my question, and keep each citation number exactly as it is written, for example [3], next "
                  "to the facts it supports. Drop navigation text, boilerplate, repetition and anything that does not "
                  "bear on my question. Group the notes by source, one short line per fact. Do not answer my question "
                  "yet and do not call any tools: write only the notes.")
DETERMINISTIC = {"temperature": 0.0, "top_p": 1.0, "top_k": 1, "presence_penalty": 0.0,
                 "chat_template_kwargs": {"enable_thinking": False}}


def transcript(ctx):
    """The messages up to the last result, and each call's id and result text."""
    every, head, tail = cut.blocks(ctx["bare"])
    head = head.replace("Knowledge Base Results:\n", "").rstrip()
    question = re.search(r"(?m)^Question: (.*)$", tail).group(1)
    messages = [dict(ctx["read_req"]["messages"][0]), {"role": "user", "content": f"{head}\n\n{tail.strip()}"}]
    labels, results, total = sorted(every), [], 0
    for i in range(0, len(labels), RESULT_SIZE):
        call_id = f"call_{i // RESULT_SIZE + 1}"
        first = every[labels[i]]
        document = re.search(r"(?m)^Document ID: (.*)$", first)
        heading = re.search(r"(?m)^Source heading: (.*)$", first)
        name, args = (("semantic_search", {"query": question}) if not i or not document else
                      ("get_document", {"document_id": document.group(1).strip(),
                                        **({"section": heading.group(1).strip()} if heading else {})}))
        text = "\n\n\n\n".join(every[l] for l in labels[i:i + RESULT_SIZE])
        total += len(text)
        if total > RESULTS_CHARS and results:
            break
        messages += [{"role": "assistant", "content": "", "tool_calls": [
                         {"id": call_id, "type": "function", "function": {"name": name, "arguments": json.dumps(args)}}]},
                     {"role": "tool", "tool_call_id": call_id, "content": text}]
        results.append((call_id, text, labels[i:i + RESULT_SIZE]))
    return messages, results


def said_last(messages, instruction):
    out = [dict(m) for m in messages]
    out[-1]["content"] += SEPARATOR + instruction
    return out


def condense(ctx, messages, instruction, kind, how="said"):
    req = ctx["read_req"]
    if how == "user":
        return base.call(req, kind, messages=[dict(m) for m in messages] + [{"role": "user", "content": instruction}],
                         tool_choice="none", max_tokens=req.get("max_tokens"), **DETERMINISTIC)
    if how == "bare":
        bare = {k: v for k, v in req.items() if k not in ("tools", "tool_choice")}
        return base.call(bare, kind, messages=said_last(messages, instruction), max_tokens=req.get("max_tokens"),
                         **DETERMINISTIC)
    return base.call(req, kind, messages=said_last(messages, instruction), tool_choice="none",
                     max_tokens=req.get("max_tokens"), **DETERMINISTIC)


def arm(ctx, name):
    messages, results = transcript(ctx)
    base.call(ctx["read_req"], "warmup", messages=messages, tool_choice="none", max_tokens=1, **DETERMINISTIC)
    if name.startswith("ONE"):
        how = {"ONE": "said", "ONE-USER": "user", "ONE-BARE": "bare"}[name]
        replies = [condense(ctx, messages, CONDENSE, name, how)]
    else:
        template = CONDENSE_EACH if name == "EACH" else CONDENSE_NAMED
        replies = [condense(ctx, messages, template.format(call=call_id, result=text), name)
                   for call_id, text, _ in results]
    return replies, results


def packed(ctxs, k):
    """The long questions of each run, `k` at a time, as one turn that asks all of them in turn: their passages
    one after another, labelled again from [1], so one condensing holds every question's facts."""
    by_run = {}
    for c in ctxs:
        if c["set"] == "long":
            by_run.setdefault(c["run"], []).append(c)
    out = []
    for run, cs in by_run.items():
        for i in range(0, len(cs) - k + 1, k):
            group, blocks = cs[i:i + k], []
            for c in group:
                every, _, _ = cut.blocks(c["bare"])
                blocks += [re.sub(r"^\[\d+\]", f"[{len(blocks) + n + 1}]", every[l], count=1)
                           for n, l in enumerate(sorted(every))]
            _, head, _ = cut.blocks(group[0]["bare"])
            asks = " ".join(f"({n}) {re.search(r'(?m)^Question: (.*)$', c['bare']).group(1)}"
                            for n, c in enumerate(group, 1))
            bare = (head + "\n\n\n\n".join(blocks)
                    + f"\n\n\n\nQuestion: Answer each of these in turn. {asks}\n\nAnswer:")
            qids = [c["qid"] for c in group]
            out.append(dict(group[0], key=f"{run}|pack:{'+'.join(qids)}", qid=None, qids=qids, set="pack",
                            bare=bare, texts=base.passages(bare)))
    return out


def facts_of(ctx):
    return [f for q in ctx.get("qids") or [ctx["qid"]] for f in base.facts(q)]


def kept_facts(facts, texts, notes):
    """fact -> (kept, cited): of `facts`, the ones whose passage is among `texts`."""
    out = {}
    lines = notes.splitlines()
    for spellings in facts:
        label = next((l for l, (t, _) in texts.items() if any(v in t for v in spellings)), None)
        if label is None:
            continue
        hit = next((i for i, line in enumerate(lines) if any(v.lower() in line.lower() for v in spellings)), None)
        cited = False
        if hit is not None:
            near = next((base.cited(lines[j]) for j in range(hit, -1, -1) if base.cited(lines[j])), set())
            cited = label in near
        out[spellings[0]] = (hit is not None, cited)
    return out


def usable(reply):
    """research_notes.rs `usable_notes`: the text before a call written into the reply, unless the reply was cut
    off or nothing is left. (The app also refuses a reply that is wholly a JSON call; the 9B writes its calls in
    its own markup, which the markers catch.)"""
    if reply["finish"] in ("length", "max_tokens", "incomplete", "failed"):
        return None
    text = reply["content"].strip()
    starts = [text.find(m) for m in CALL_MARKERS if m in text]
    if starts:
        text = text[:min(starts)].strip()
    return text or None


def record(ctx, name, replies, results):
    """What the transcript holds after condensing: each usable reply's notes in place of the results it covers,
    and the results themselves wherever the reply was unusable (the app carries on without condensing them)."""
    notes = [usable(r) for r in replies]
    covered = [[text for _, text, _ in results]] if name.startswith("ONE") else [[text] for _, text, _ in results]
    after = "\n".join(n if n else "\n".join(texts) for n, texts in zip(notes, covered))
    shown = {l for _, _, ls in results for l in ls}
    facts = kept_facts(facts_of(ctx), {l: t for l, t in ctx["texts"].items() if l in shown}, after)
    results_chars = sum(len(t) for _, t, _ in results)
    written = "\n".join(n for n in notes if n)
    s = lambda key: sum(((r.get("timings") or {}).get(key) or 0) for r in replies)
    return {"key": ctx["key"], "set": ctx["set"], "arm": name, "results": len(results), "passages": len(shown),
            "results_chars": results_chars, "after_chars": len(after), "facts": len(facts),
            "kept": sum(k for k, _ in facts.values()), "cited": sum(c for _, c in facts.values()),
            "lost": [f for f, (k, _) in facts.items() if not k],
            "unknown_citations": sorted(base.cited(written) - set(ctx["texts"])),
            "usable": sum(1 for n in notes if n), "calls_written": sum(1 for r in replies if any(m in r["content"] for m in CALL_MARKERS)),
            "cut_off": sum(1 for r in replies if r["finish"] == "length"), "requests": len(replies),
            "prompt_new": s("prompt_n"), "output_tokens": s("predicted_n"),
            "secs": round(sum(r["secs"] for r in replies), 1),
            "replies": [{"content": r["content"], "finish": r["finish"]} for r in replies]}


def run(args):
    path = os.path.join(args.out, "records.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a")
    ctxs = grouped.contexts(args)
    if args.pack > 1:
        ctxs = packed(ctxs, args.pack)
    print(f"{len(ctxs)} turns", flush=True)
    for i, ctx in enumerate(ctxs):
        line = []
        order = args.arms if i % 2 == 0 else list(reversed(args.arms))
        for name in order:
            if (ctx["key"], name) in done:
                continue
            rec = record(ctx, name, *arm(ctx, name))
            out.write(json.dumps(rec) + "\n")
            out.flush()
            line.append(f"{name} {rec['kept']}/{rec['facts']} cited {rec['cited']} usable {rec['usable']}/"
                        f"{rec['requests']} calls {rec['calls_written']} {rec['results_chars']}->{rec['after_chars']} "
                        f"{rec['secs']}s")
        if line:
            print(ctx["key"], " | ".join(line), flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "records.jsonl"))
    arms = list(dict.fromkeys(r["arm"] for r in recs))
    keys = set.intersection(*({r["key"] for r in recs if r["arm"] == a} for a in arms)) if arms else set()
    print(f"{len(keys)} turns every arm ran\n")
    print("| Arm | Facts kept | with citation | Turns losing a fact | Usable replies | Replies writing a call | "
          "After / before chars | Cut off | Unknown citations | Requests | Prompt tokens processed | Output tokens | "
          "Seconds | Median s/turn |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for subset in ("all", "long", "short", "pack"):
        for a in arms:
            rs = [r for r in recs if r["arm"] == a and r["key"] in keys and subset in ("all", r["set"])]
            if not rs:
                continue
            name = a if subset == "all" else f"{a} ({subset})"
            print(f"| {name} | {sum(r['kept'] for r in rs)}/{sum(r['facts'] for r in rs)} | {sum(r['cited'] for r in rs)} | "
                  f"{sum(1 for r in rs if r['lost'])} | {sum(r['usable'] for r in rs)}/{sum(r['requests'] for r in rs)} | "
                  f"{sum(r['calls_written'] for r in rs)} | "
                  f"{sum(r['after_chars'] for r in rs) / sum(r['results_chars'] for r in rs):.2f} | "
                  f"{sum(r['cut_off'] for r in rs)} | {sum(len(r['unknown_citations']) for r in rs)} | "
                  f"{sum(r['requests'] for r in rs)} | {sum(r['prompt_new'] for r in rs):,} | "
                  f"{sum(r['output_tokens'] for r in rs):,} | {sum(r['secs'] for r in rs):,.0f} | "
                  f"{statistics.median(r['secs'] for r in rs):.1f} |")
    for a in arms:
        lost = [f"{r['key']} {r['lost']}" for r in recs if r["arm"] == a and r["key"] in keys and r["lost"]]
        print(f"\n{a} lost: {lost or 'none'}")


def main():
    global RESULT_SIZE, RESULTS_CHARS
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--arms", default="ONE,EACH,NAMED")
    ap.add_argument("--pack", type=int, default=1, help="long questions asked together in one turn")
    ap.add_argument("--result-size", type=int, default=RESULT_SIZE)
    ap.add_argument("--results-chars", type=int, default=RESULTS_CHARS)
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    args = ap.parse_args()
    RESULT_SIZE, RESULTS_CHARS = args.result_size, args.results_chars
    args.arms = [a for a in args.arms.split(",") if a]
    bad = [a for a in args.arms if a not in ARMS]
    if bad:
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
