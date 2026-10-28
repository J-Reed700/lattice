"""Offline A/B for evidence cut to fit the window (turn/assemble.rs `keep_evidence`): what the grouped reading
would note if the passages cut from the prompt were read in groups of their own, shown only there.

The cut keeps the first passages of the message and drops the rest, as `keep_sources` does. Each saved reading
prompt is cut to its first ceil(n * (1 - CUT)) passages; the arm then reads the cut passages, in groups of
four, after the cut message, each group shown in full (with its document heading) under CUT_GROUP. Its notes
are judged as replay_grouped_reading_ab.py judges a group's: a verified copy from a passage that states a fact.
The comparison is G4 on the uncut prompt (that script's records), where the same passages sat in the window.

  python -I -u scripts/replay_cut_evidence_ab.py --out OUT --llama http://127.0.0.1:8089 --rubric R ... \\
    --calls A.calls.jsonl ... [--cut 0.5]
  python -I scripts/replay_cut_evidence_ab.py --out OUT --summary --uncut G4_RECORDS.jsonl
"""
import argparse, json, math, os, re, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_close_read_ab as base  # noqa: E402
import replay_grouped_reading_ab as grouped  # noqa: E402

GROUP_SIZE = 4
CUT_GROUP = ("Before answering, read these further passages, which did not fit above, each on its own.\n\n{passages}\n\n"
             "Write one line for each of these passages: its label, then the sentences of that passage that bear on my "
             "question, copied exactly as written and in the passage's own language — or its label and a single hyphen "
             "when it says nothing bearing on my question, however near its subject. Write only these lines.")


def blocks(message):
    """label -> the passage's whole block (heading lines and content), and the message with no passage in it."""
    st = re.search(r"(?m)^\[1\] Document:", message).start()
    en = re.search(r"(?m)^Question: ", message).start()
    out = {}
    for b in re.split(r"\n(?=\[\d+\] Document:)", message[st:en]):
        m = re.match(r"\[(\d+)\]", b)
        if m:
            out[int(m.group(1))] = b.strip()
    return out, message[:st], message[en:]


def cut_message(bare, kept):
    every, head, tail = blocks(bare)
    return head + "\n\n\n\n".join(every[l] for l in sorted(every) if l in kept) + "\n\n\n\n" + tail, every


def cut_reading(ctx, cut):
    read_req, texts = ctx["read_req"], ctx["texts"]
    labels = sorted(texts)
    keep = math.ceil(len(labels) * (1 - cut))
    kept, dropped = set(labels[:keep]), labels[keep:]
    message, every = cut_message(ctx["bare"], kept)
    reserved = read_req.get("max_tokens")
    base.call(read_req, "warmup", max_tokens=1, messages=grouped.with_message(read_req, message))
    reqs, lines, unaccounted = [], {}, []
    for i in range(0, len(dropped), GROUP_SIZE):
        group = dropped[i:i + GROUP_SIZE]
        block = "\n\n".join(every[l] for l in group)
        messages = grouped.with_message(read_req, f"{message}\n\n{CUT_GROUP.format(passages=block)}")
        stop = [f"[{group[-1] + 1}]", f"\n{group[-1] + 1}."]
        r = grouped.ask(read_req, "cut", reserved, stop=stop, messages=messages)
        reqs.append(base.req_record(group, r, reserved))
        got = grouped.parse_into(lines, r, set(group))
        left = [l for l in group if l not in got]
        if left and got:
            whole = base.whole_lines(r["content"], r["finish"]).strip()
            sweep = messages + [{"role": "assistant", "content": whole},
                                {"role": "user", "content": grouped.SWEEP.format(labels=", ".join(f"[{l}]" for l in left))}]
            r2 = grouped.ask(read_req, "cut-sweep", reserved, stop=stop, messages=sweep)
            reqs.append(base.req_record(left, r2, reserved))
            got |= grouped.parse_into(lines, r2, set(left))
        unaccounted += [l for l in group if l not in got]
    cut_texts = {l: texts[l] for l in dropped}
    lines, _, _ = base.attribute(lines, cut_texts)
    rec = grouped.record(dict(ctx, texts=cut_texts, fl=ctx["fl"] & set(dropped)), f"CUT{cut}", (lines, reqs, {"unaccounted": unaccounted}))
    rec["kept"] = keep
    return rec


def run(args):
    path = os.path.join(args.out, "records.jsonl")
    done = {r["key"] for r in base.read_out(path)}
    out = open(path, "a")
    ctxs = grouped.contexts(args)
    print(f"{len(ctxs)} readings", flush=True)
    for ctx in ctxs:
        if ctx["key"] in done:
            continue
        rec = cut_reading(ctx, args.cut)
        out.write(json.dumps(rec) + "\n")
        out.flush()
        print(f"{ctx['key']} cut {rec['passages']}: facts {len(rec['facts_noted'])}/{len(rec['fact_labels'])} "
              f"nf{len(rec['nonfact_lines'])} req{rec['requests']} {rec['secs']}s", flush=True)


def summary(args):
    recs = base.read_out(os.path.join(args.out, "records.jsonl"))
    uncut = {r["key"]: r for r in base.read_out(args.uncut) if r["arm"] == "G4"} if args.uncut else {}
    keys = [r["key"] for r in recs if r["key"] in uncut] if uncut else [r["key"] for r in recs]
    rs = [r for r in recs if r["key"] in keys]
    cut_labels = {r["key"]: set(range(r["kept"] + 1, r["kept"] + r["passages"] + 1)) for r in rs}
    f = sum(len(r["facts_noted"]) for r in rs)
    of = sum(len(r["fact_labels"]) for r in rs)
    print(f"{len(rs)} readings, {sum(r['passages'] for r in rs)} passages cut, {of} of them state a fact "
          f"({sum(1 for r in rs if r['fact_labels'])} readings)\n")
    print(f"Read only in their own groups: {f}/{of} fact passages noted, {sum(len(r['nonfact_lines']) for r in rs)} "
          f"non-fact lines, {sum(r['requests'] for r in rs)} requests, {sum(r['secs'] for r in rs):,.0f} s "
          f"(median {statistics.median(r['secs'] for r in rs):.1f} s a reading)")
    if uncut:
        u = [uncut[k] for k in keys]
        uf = sum(len(set(x["facts_noted"]) & cut_labels[x["key"]]) for x in u)
        unf = sum(len(set(x["nonfact_lines"]) & cut_labels[x["key"]]) for x in u)
        print(f"The same passages read in the window (G4, uncut): {uf}/{of} fact passages noted, {unf} non-fact lines")
    for r in rs:
        if r["missed"]:
            print(f"  missed {r['key']} {r['missed']}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--cut", type=float, default=0.5)
    ap.add_argument("--uncut")
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    run(args)


if __name__ == "__main__":
    main()
