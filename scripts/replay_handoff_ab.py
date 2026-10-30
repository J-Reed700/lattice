"""Offline A/B of the handoff note's prompt (conversation/handoff.rs): its rules before the transcript, as sent
today, against the same rules after it.

"Continue in new chat" writes a note that becomes the new conversation's only memory of the old one. The request
is GroundedRequest's task, then the transcript (one `USER:`/`ASSISTANT:` passage per message), then `closing`,
which handoff.rs leaves empty: the last thing the model reads is the conversation's last message, often a
question.

Conversations are built from desktop-path-v1: each strings together the exchanges of 10 to 20 of its items with
the answers a recorded shipped-path run gave them, then ends on another question with no answer. A note is scored
on its four headings, the rubric facts it keeps of those the answers stated, whether that last question is left
under Open threads, and its length.

Arms:
  BEFORE  handoff_task before this change: every rule before the transcript, nothing after it.
  AFTER   the purpose before the transcript; "Now write the handoff note for the conversation above…" and every
          rule after it.
  AFTER_Q AFTER, saying that a last question nobody answered goes under Open threads, not answered (shipped).

  python -X utf8 -I -u scripts/replay_handoff_ab.py --out OUT --answers A.jsonl [--llama URL | --claude MODEL ...]
  python -X utf8 -I scripts/replay_handoff_ab.py --out OUT --answers A.jsonl --summary
"""
import argparse, json, os, random, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QUESTIONS = os.path.join(ROOT, "evals", "retrieval", "desktop-path-v1.json")
SYSTEM = "You summarize conversations faithfully and concisely. You never add facts the material does not contain."
HEADINGS = ("## Where things stand", "## Established", "## Decisions", "## Open threads")
RULES = [
    "Output markdown with exactly these sections:",
    *HEADINGS,
    "Rules:",
    "- Where things stand: 2-4 sentences on the goal and how far it got.",
    "- Established: bullets of findings, with concrete values, and the source named when the conversation names one.",
    "- Decisions: bullets, each with its reason when one was given.",
    "- Open threads: unanswered questions and next steps.",
    "- If a section has nothing, write `None.`",
    "- No preamble, no closing remarks. Stay under 450 words.",
]
LAST_QUESTION = ("- Its last message may be a question nobody has answered yet. Do not answer it: put it under Open "
                 "threads.")
ARMS = ("BEFORE", "AFTER", "AFTER_Q")
CONVERSATIONS, SHORTEST, LONGEST = 20, 10, 20
# What names each possible last question in a note, however it is paraphrased.
LAST_TERMS = {
    "dp-buried-agreement-meeting": ("tessivar",), "dp-conflict-receipt-threshold": ("receipt",),
    "dp-conflict-ring-threshold": ("cold shard", "shard promotion"), "dp-long-apiary-log": ("slantgate",),
    "dp-long-canal-log": ("dreithworth",), "dp-long-choir-minutes": ("holdmere",), "dp-long-cookbook": ("brearnby",),
    "dp-long-lab-notebook": ("throassmere",), "dp-mt-ring-thresholds": ("bucket contention", "tenant bucket"),
    "dp-multihop-dunlark-notice": ("dunlark",), "dp-opening-agreement": ("brackleton",),
    "dp-paraphrase-monitor": ("monitor",), "dp-table-calibration": ("gauge", "calibration record 03"),
    "dp-table-order-line": ("71486",), "dp-table-spend-csv": ("selkirk",),
    "dp-transcript-rollback": ("roll back", "rollback"),
}


def handoff_task(title):
    return "\n".join([
        f'Write a handoff note for the conversation "{title}" so a new conversation can pick up where it left off.',
        "The note will be the new conversation's only memory of this one.",
        "Use only the conversation transcript below. Do not add anything it does not say.",
        *RULES,
    ])


def prompt(arm, title, transcript):
    """handoff.rs summary_request rendered as GroundedRequest renders it: task, evidence, closing."""
    if arm == "BEFORE":
        return f"{handoff_task(title)}\n\n{transcript}"
    task = (f'Below is the transcript of the conversation "{title}". You will write a handoff note from it so a new '
            "conversation can pick up where it left off.")
    rules = RULES + ([LAST_QUESTION] if arm == "AFTER_Q" else [])
    closing = "\n".join([
        "Now write the handoff note for the conversation above. The note will be the new conversation's only memory "
        "of this one.",
        "Use only the conversation transcript above. Do not add anything it does not say.",
        *rules,
    ])
    return f"{task}\n\n{transcript}\n\n{closing}"


def items():
    return {i["id"]: i["turns"] for i in json.load(open(QUESTIONS, encoding="utf-8"))["items"]}


def states(text, group):
    low = text.lower()
    return any(alt.lower() in low for alt in group)


def conversations(answers_path):
    """Each conversation: some items' exchanges in a random order (a multi-turn item's turns together), then the
    first question of an item not shown, unanswered, whose rubric facts the transcript does not state."""
    asks = items()
    answered = {}
    for line in open(answers_path, encoding="utf-8"):
        r = json.loads(line)
        turns = [(t.get("answer") or "").strip() for t in r.get("turns") or []]
        if r["query_id"] in asks and len(turns) == len(asks[r["query_id"]]) and all(turns):
            answered[r["query_id"]] = turns
    ids = sorted(answered)
    out = []
    for n in range(CONVERSATIONS):
        rng = random.Random(n)
        shown = rng.sample(ids, rng.randint(SHORTEST, LONGEST))
        exchanges = [(asks[q][i], answered[q][i]) for q in shown for i in range(len(asks[q]))]
        text = " ".join(f"{turn['ask']} {answer}" for turn, answer in exchanges)
        last = [q for q in sorted(asks) if q not in shown and not asks[q][0]["rubric"]["abstain"]
                and not any(states(text, g) for g in asks[q][0]["rubric"]["must_mention"])]
        q = rng.choice(last)
        out.append({"id": f"handoff-{n:02d}", "shown": shown, "exchanges": exchanges, "last": q,
                    "last_ask": asks[q][0]["ask"]})
    return out


def transcript_of(conv):
    lines = []
    for turn, answer in conv["exchanges"]:
        lines += [f"USER: {turn['ask']}", f"ASSISTANT: {answer}"]
    lines.append(f"USER: {conv['last_ask']}")
    return "\n\n".join(lines)


def title_of(conv):
    ask = conv["exchanges"][0][0]["ask"]
    return ask if len(ask) <= 60 else ask[:59] + "…"


def sections(note):
    """Each heading's text, when the note has all four in order with nothing but whitespace before the first."""
    at = [note.find(h) for h in HEADINGS]
    if any(i < 0 for i in at) or at != sorted(at) or note[:at[0]].strip():
        return None
    ends = at[1:] + [len(note)]
    return {h: note[a + len(h):e].strip() for h, a, e in zip(HEADINGS, at, ends)}


def score(conv, note):
    parts = sections(note)
    facts = [g for turn, answer in conv["exchanges"] for g in turn["rubric"]["must_mention"] if states(answer, g)]
    return {"headings": parts is not None,
            "facts": len(facts), "kept": sum(states(note, g) for g in facts),
            "last_open": states((parts or {}).get("## Open threads", ""), LAST_TERMS[conv["last"]]),
            "words": len(note.split())}


def run(args):
    convs = conversations(args.answers)
    path = os.path.join(args.out, "notes.jsonl")
    done = {(r["id"], r["arm"]) for r in base.read_out(path)}
    with open(path, "a", encoding="utf-8") as out:
        for conv in convs:
            transcript, title = transcript_of(conv), title_of(conv)
            for arm in args.arms:
                if (conv["id"], arm) in done:
                    continue
                req = {"messages": [{"role": "system", "content": SYSTEM},
                                    {"role": "user", "content": prompt(arm, title, transcript)}],
                       "temperature": 0.0, "top_p": 1.0, "top_k": 1, "presence_penalty": 0.0, "repeat_penalty": 1.0,
                       "max_tokens": 2048, "chat_template_kwargs": {"enable_thinking": False}, "cache_prompt": True}
                r = base.call(req, "handoff")
                note = r["content"].strip()
                out.write(json.dumps({"id": conv["id"], "arm": arm, "shown": conv["shown"], "last": conv["last"],
                                      **score(conv, note), "finish": r["finish"], "secs": r["secs"],
                                      "prompt_tokens": (r.get("usage") or {}).get("prompt_tokens"), "note": note})
                          + "\n")
                out.flush()
                print(conv["id"], arm, flush=True)


def summary(args):
    """Every note scored again from its conversation, so a change to the scoring needs no new run."""
    convs = {c["id"]: c for c in conversations(args.answers)}
    recs = [{**r, **score(convs[r["id"]], r["note"])} for r in base.read_out(os.path.join(args.out, "notes.jsonl"))]
    print("| Arm | Notes | All four headings | Last question under Open threads | Facts kept | Median words | "
          "Over 450 words |")
    print("|---|---|---|---|---|---|---|")
    for arm in ARMS:
        rs = [r for r in recs if r["arm"] == arm]
        if not rs:
            continue
        print(f"| {arm} | {len(rs)} | {sum(r['headings'] for r in rs)} | {sum(r['last_open'] for r in rs)} | "
              f"{sum(r['kept'] for r in rs)}/{sum(r['facts'] for r in rs)} | "
              f"{statistics.median(r['words'] for r in rs):.0f} | {sum(r['words'] > 450 for r in rs)} |")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--answers", required=True, help="a shipped-path answers file on desktop-path-v1")
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    args.arms = args.arms.split(",")
    if bad := [a for a in args.arms if a not in ARMS]:
        sys.exit(f"unknown arms: {bad}")
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
