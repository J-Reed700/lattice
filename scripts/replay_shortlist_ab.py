"""Offline A/B of reading a ranked shortlist instead of the whole window, on the 27B's saved shipped-path turns.

Each saved reading prompt is rebuilt with only the arm's passages, renumbered 1..K in the arm's order as the app
numbers what it sends, then read and answered as shipped (replay_write_ab.py's W0: every passage in one group,
the write instruction, the continuation). The answer is scored on the question's rubric facts. The baseline is
the same turn's answer from the shipped-path run, which read every passage the window held.

Arms:
  FIRST<K>  the first stage's top K, in its order.
  FUSED<K>  the top K by reciprocal-rank fusion (k = 60) of the first stage's order and Qwen3-Reranker-0.6B's,
            over the first POOL passages; scores from rerank_blend.py, keyed "query_id|turn".

  python -X utf8 -I -u scripts/replay_shortlist_ab.py --out OUT --calls RUN.calls.jsonl --rubric Q.json \\
    --answers RUN.jsonl --scores SCORES.json [--arms FIRST40,FUSED40] [--llama URL]
  python -X utf8 -I scripts/replay_shortlist_ab.py --out OUT --answers RUN.jsonl --rubric Q.json --summary
"""
import argparse, json, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402
import replay_cut_evidence_ab as cut  # noqa: E402
import replay_grouped_reading_ab as reading  # noqa: E402
import replay_write_ab as write  # noqa: E402

POOL = 64
RRF_K = 60


def order_of(arm, n, scores):
    k = int(re.sub(r"\D", "", arm))
    if arm.startswith("FIRST"):
        return list(range(1, min(k, n) + 1))
    pool = list(range(1, min(POOL, n, len(scores)) + 1))
    by_score = sorted(pool, key=lambda l: -scores[l - 1])
    rr = {l: i for i, l in enumerate(by_score)}
    fused = sorted(pool, key=lambda l: -(1 / (RRF_K + l) + 1 / (RRF_K + rr[l] + 1)))
    return fused[:k]


def contexts(paths):
    """One replay context per question's first turn, from a shipped-path calls log in today's reading form: the
    read-ahead request (the message alone, one token) is the message every arm shares, the first group request
    gives the reading's sampling, and the first write request the answer's."""
    out = []
    for path in paths:
        run = os.path.basename(path).split(".")[0]
        by = {}
        for row in base.load_rows(path):
            c = row.get("call") or {}
            q = c.get("request") or {}
            m = q.get("messages") or []
            if c.get("path") != "/v1/chat/completions" or len(m) < 2 or not base.text_of(m[0]).startswith("You are Lattice"):
                continue
            g = by.setdefault(row["query_id"], {})
            last = base.text_of(m[-1])
            if len(m) == 2 and q.get("max_tokens") == 1 and "[1] Document:" in last:
                g.setdefault("bare", last)
            elif len(m) == 2 and re.search(r"your reply ends with the line for \[\d+\]\.$", last):
                g.setdefault("read", q)
            elif last.endswith(base.WRITE_ENDINGS):
                g.setdefault("write", q)
        for qid, g in by.items():
            if not {"bare", "read"} <= set(g) or qid not in base.items:
                continue
            texts = base.passages(g["bare"])
            w = g.get("write") or {}
            out.append({"key": f"{run}|{qid}", "run": run, "qid": qid, "set": "long", "read_req": g["read"],
                        "read_user": g["bare"], "bare": g["bare"], "texts": texts,
                        "fl": base.fact_labels(qid, texts),
                        "write_sampling": {k: w[k] for k in base.SAMPLING_KEYS if k in w}})
    return out


def shortlisted(ctx, labels):
    """`ctx` with only `labels` shown, renumbered 1..len(labels) in that order."""
    every, head, tail = cut.blocks(ctx["bare"])
    blocks = [re.sub(r"^\[\d+\]", f"[{i}]", every[l], count=1) for i, l in enumerate(labels, 1)]
    bare = head + "\n\n\n\n".join(blocks) + "\n\n\n\n" + tail
    texts = base.passages(bare)
    return dict(ctx, bare=bare, texts=texts, fl=base.fact_labels(ctx["qid"], texts), shown=labels)


def baseline(path):
    return {r["query_id"]: r["answer"] for r in map(json.loads, open(path, encoding="utf-8"))
            if len(r.get("turns") or []) == 1 and r.get("answer")}


def run(args):
    scores = json.load(open(args.scores, encoding="utf-8"))
    single = baseline(args.answers)
    path = os.path.join(args.out, "answers.jsonl")
    done = {(r["query_id"], r["arm"]) for r in base.read_out(path)}
    with open(path, "a", encoding="utf-8") as out:
        for ctx in contexts(args.calls):
            if ctx["qid"] not in single or f"{ctx['qid']}|0" not in scores:
                continue
            for arm in args.arms:
                if (ctx["qid"], arm) in done:
                    continue
                labels = order_of(arm, len(ctx["texts"]), scores[f"{ctx['qid']}|0"])
                short = shortlisted(ctx, labels)
                rec = write.record(dict(short, cond="complete"), "W0", write.run_arm(short, "W0", {}))
                rec.update(arm=arm, shown=labels, fact_passages=sorted(short["fl"]))
                out.write(json.dumps(rec) + "\n")
                out.flush()
                print(ctx["qid"], arm, rec["stated"], "/", rec["facts"], f"{rec['secs']}s", flush=True)


def summary(args):
    recs = list(base.read_out(os.path.join(args.out, "answers.jsonl")))
    single = baseline(args.answers)
    qids = sorted({r["query_id"] for r in recs})
    print("| Arm | Questions | Facts stated | Median seconds |")
    print("|---|---|---|---|")
    total = sum(len(base.facts(q)) for q in qids)
    stated = sum(len(base.facts(q)) - len(base.missing(q, single[q])) for q in qids)
    print(f"| whole window (shipped-path run) | {len(qids)} | {stated}/{total} | |")
    for arm in dict.fromkeys(r["arm"] for r in recs):
        rs = [r for r in recs if r["arm"] == arm]
        secs = sorted(r["secs"] for r in rs)
        print(f"| {arm} | {len(rs)} | {sum(r['stated'] for r in rs)}/{sum(r['facts'] for r in rs)} | "
              f"{secs[len(secs) // 2]} |")
    for q in qids:
        print(q, "window:", base.missing(q, single[q]),
              {r["arm"]: r["missing"] for r in recs if r["query_id"] == q})


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--answers", required=True)
    ap.add_argument("--scores")
    ap.add_argument("--arms", default="FIRST40,FUSED40")
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    args.arms = args.arms.split(",")
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    base.URL = args.llama
    run(args)


if __name__ == "__main__":
    main()
