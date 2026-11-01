"""Offline A/B of the journal synthesis prompts (conversation/synthesis/prompts.rs), as the app sends them today
against the same prompts sent on their own, with their rules before and after the material.

A synthesis maps parts of its entries to chunk notes, may merge groups of notes, then reduces them to one brief. Each
prompt is a closed-book chat turn: no evidence, so chat/prompting.rs renders it in the no-context template, and the
model reads the whole prompt as "The user asked", then that no passages were available, then "Answer from general
knowledge". Inside it, every prompt lists its sections and rules first and ends on the material, the shape the handoff
note had: see evals/conversation-memory/2026-10-30-HANDOFF-RULES-LAST.md.

Entries are the conversations scripts/replay_handoff_ab.py builds from desktop-path-v1. A map reads two; a merge or a
reduce reads three chunk notes, the same notes in every arm, written by the SENT arm's map. An output is scored on its
sections, in order, with nothing before the first, on the rubric facts it keeps of those its material stated, on
whether it says it is not backed by the user's documents, and on its length.

Arms:
  SENT    the prompts as the app sends them: rules, then material, inside the no-context template.
  BEFORE  the same prompts on their own.
  AFTER   the job in one sentence, then the material, then "Now write…" and every rule, on their own.
  KEEP, KEEP_AFTER  (reduce only) BEFORE and AFTER with the rule to stay concise replaced by one to keep the detail.

--system is the app's default chat system prompt, taken from a recorded call.

  python -X utf8 -I -u scripts/replay_synthesis_ab.py --out OUT --answers A.jsonl --system S.txt
      [--llama URL | --claude MODEL ...]
  python -X utf8 -I scripts/replay_synthesis_ab.py --out OUT --answers A.jsonl --summary
"""
import argparse, json, os, random, statistics, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402
import replay_handoff_ab as handoff  # noqa: E402

ARMS = ("SENT", "BEFORE", "AFTER", "KEEP", "KEEP_AFTER")
STEPS = ("map", "merge", "reduce")
MAPS, ROLLUPS, NOTES_PER_ROLLUP = 8, 4, 3
SEPARATOR = "\n\n---\n\n"
NO_CONTEXT = ("The user asked: \"{question}\"\n\nNo passages from their documents were available for this turn.\n\n"
              "Answer from general knowledge where you can, and say plainly that this answer is not backed by their own "
              "documents. If they want sourced evidence, offer a web search or adding documents to their lattice.")
DISCLAIMERS = ("not backed", "general knowledge", "web search")
MAP_SECTIONS = ("### Entry Highlights", "### Practical Details", "### Decisions & Constraints",
                "### Open Questions & Risks", "### Evidence Notes")
REDUCE_SECTIONS = ("## Executive Summary", "## Detailed Synthesis", "## Action Plan", "## Decisions and Assumptions",
                   "## Open Questions and Risks", "## Evidence Map")
MAP_RULES = [
    "Goal: capture practical details so the final output can replace re-reading every entry.",
    "Output markdown with exactly these sections:",
    *MAP_SECTIONS,
    "Rules:",
    "- Write concise but information-dense bullets (not generic summaries).",
    "- Include concrete values when present (counts, ranges, timing, limits, caveats).",
    "- Prefix each bullet with [Entry: <title>].",
    "- In Evidence Notes, map each important claim to one or more supporting entry titles.",
    "- Preserve source citations as numeric markers [n] from the supplied source lists; never renumber, invent, or "
    "cite an unavailable number.",
    "- If a section has no data, write one bullet: - None identified.",
    "",
    "- An entry marked (part i of n) continues across chunks; describe the part you have.",
]
REDUCE_RULES = [
    "Audience: someone who wants to understand and act quickly without reading every entry.",
    "Output markdown with exactly these sections (in this order):",
    *REDUCE_SECTIONS,
    "Formatting rules:",
    "- Executive Summary: 1 short paragraph (4-6 sentences), plain language.",
    "- Detailed Synthesis: 3-6 subsections with '### <Theme>' headers. Under each theme, write one short paragraph "
    "plus bullets for critical specifics.",
    "- Action Plan: numbered list (3-7 steps) with enough detail to execute.",
    "- Decisions and Assumptions: bullets with explicit rationale when available.",
    "- Open Questions and Risks: bullets with impact noted briefly.",
    "- Evidence Map: markdown table with columns `Claim`, `Supporting entries`, `Confidence`.",
    "- Preserve numeric source citations [n] verbatim from the chunk syntheses; never renumber or invent citations.",
    "- Keep the brief concise but comprehensive; avoid fluff and repetition.",
    "- If data is missing in a section, write `None identified.`",
]
CONCISE = "- Keep the brief concise but comprehensive; avoid fluff and repetition."
KEEP_DETAIL = "- Keep every name, date, figure and limit the chunk syntheses state; cut repetition, not detail."
MERGE_RULES = [
    "Output markdown with exactly these sections:",
    *MAP_SECTIONS,
    "Rules:",
    "- Keep the [Entry: <title>] prefix on each bullet.",
    "- Preserve numeric source citations [n] verbatim; never renumber or invent citations.",
    "- Merge duplicates; drop nothing else.",
    "- If a section has no data, write one bullet: - None identified.",
]


def entry_block(number, conv):
    transcript = "\n".join(f"USER: {turn['ask']}\nASSISTANT: {answer}" for turn, answer in conv["exchanges"])
    return "\n".join([f"ENTRY {number}", f"Title: {handoff.title_of(conv)}", "Updated: 2026-10-29",
                      f"Message Count: {2 * len(conv['exchanges'])}",
                      "Transcript (citation markers resolve to the source list below):", transcript])


def map_prompt(arm, part, parts, entries):
    material = "Entries:\n" + SEPARATOR.join(entries)
    if arm != "AFTER":
        return "\n".join([f"You are synthesizing journal entry chunk {part} of {parts}.",
                          "Use only the provided entry transcripts.", *MAP_RULES, "", material])
    closing = "\n".join([f"Now write the synthesis of these entries (chunk {part} of {parts}).",
                         "Use only the entry transcripts above.", *MAP_RULES])
    return f"Below are journal entries, chunk {part} of {parts}. You will synthesize them.\n\n{material}\n\n{closing}"


def chunks_text(notes):
    return SEPARATOR.join(f"CHUNK SYNTHESIS {i + 1}\n{n}" for i, n in enumerate(notes))


def reduce_prompt(arm, notes):
    rules = [KEEP_DETAIL if arm.startswith("KEEP") and r == CONCISE else r for r in REDUCE_RULES]
    if not arm.endswith("AFTER"):
        return "\n".join(["Merge the chunk syntheses into one final journal brief.",
                          "Use only the chunk syntheses below.", *rules, "", chunks_text(notes)])
    closing = "\n".join(["Now merge the chunk syntheses above into one final journal brief.",
                         "Use only the chunk syntheses above.", *rules])
    return (f"Below are chunk syntheses of a journal. You will merge them into one final brief.\n\n"
            f"{chunks_text(notes)}\n\n{closing}")


def merge_prompt(arm, notes):
    keep = "keep every concrete detail, decision, open question and evidence note."
    if arm != "AFTER":
        return "\n".join(["Merge these chunk syntheses into one chunk synthesis (group 1 of 2).",
                          f"Use only the chunk syntheses below; {keep}", *MERGE_RULES, "", chunks_text(notes)])
    closing = "\n".join(["Now merge the chunk syntheses above into one chunk synthesis (group 1 of 2).",
                         f"Use only the chunk syntheses above; {keep}", *MERGE_RULES])
    return f"Below are chunk syntheses of a journal. You will merge them into one.\n\n{chunks_text(notes)}\n\n{closing}"


def in_order(text, heads):
    at = [text.find(h) for h in heads]
    return all(i >= 0 for i in at) and at == sorted(at) and not text[:at[0]].strip()


def facts_in(convs, within=None):
    """The rubric facts the conversations' answers state, once each, and of those the ones `within` keeps."""
    facts = {tuple(g): g for c in convs for turn, answer in c["exchanges"] for g in turn["rubric"]["must_mention"]
             if handoff.states(answer, g)}
    return [g for g in facts.values() if within is None or handoff.states(within, g)]


def score(text, facts, heads):
    low = text.lower()
    return {"sections": in_order(text, heads), "facts": len(facts), "kept": sum(handoff.states(text, g) for g in facts),
            "disclaims": any(d in low for d in DISCLAIMERS), "words": len(text.split())}


def request(system, prompt, max_tokens):
    return {"messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
            "temperature": 0.0, "top_p": 1.0, "top_k": 1, "presence_penalty": 0.0, "repeat_penalty": 1.0,
            "max_tokens": max_tokens, "chat_template_kwargs": {"enable_thinking": False}, "cache_prompt": True}


def plan(convs):
    """(id, step, conversations, what the prompt is built from, sections). Rollups read the SENT arm's map notes."""
    pairs = [convs[i:i + 2] for i in range(0, 2 * MAPS, 2)]
    steps = []
    for n, pair in enumerate(pairs):
        entries = [entry_block(i + 1, c) for i, c in enumerate(pair)]
        steps.append((f"map-{n:02d}", "map", pair, (n, entries), MAP_SECTIONS))
    for n in range(ROLLUPS):
        picks = random.Random(n).sample(range(MAPS), NOTES_PER_ROLLUP)
        for step, heads in (("reduce", REDUCE_SECTIONS), ("merge", MAP_SECTIONS)):
            steps.append((f"{step}-{n:02d}", step, [c for p in picks for c in pairs[p]], picks, heads))
    return steps


def shown_notes(arg, notes):
    return [notes[f"map-{p:02d}"] for p in arg]


def render(step, arm, arg, notes):
    if step == "map":
        prompt = map_prompt(arm, arg[0] + 1, MAPS, arg[1])
    else:
        prompt = (reduce_prompt if step == "reduce" else merge_prompt)(arm, shown_notes(arg, notes))
    return NO_CONTEXT.format(question=prompt) if arm == "SENT" else prompt


def facts_of(step, group, arg, notes):
    return facts_in(group) if step == "map" else facts_in(group, "\n".join(shown_notes(arg, notes)))


def sent_notes(recs):
    return {r["id"]: r["text"] for r in recs if r["arm"] == "SENT" and r["step"] == "map"}


def run(args):
    convs = handoff.conversations(args.answers)
    path = os.path.join(args.out, "outputs.jsonl")
    recs = base.read_out(path)
    done = {(r["id"], r["arm"]) for r in recs}
    notes = sent_notes(recs)
    system = open(args.system, encoding="utf-8").read()
    with open(path, "a", encoding="utf-8") as out:
        for sid, step, group, arg, heads in plan(convs):
            for arm in args.arms:
                if (sid, arm) in done or step not in args.steps:
                    continue
                r = base.call(request(system, render(step, arm, arg, notes), args.max_tokens), "synthesis")
                text = r["content"].strip()
                if arm == "SENT" and step == "map":
                    notes[sid] = text
                out.write(json.dumps({"id": sid, "step": step, "arm": arm, "finish": r["finish"], "secs": r["secs"],
                                      "prompt_tokens": (r.get("usage") or {}).get("prompt_tokens"),
                                      **score(text, facts_of(step, group, arg, notes), heads), "text": text}) + "\n")
                out.flush()
                print(sid, arm, flush=True)


def summary(args):
    """Every output scored again from its step, so a change to the scoring needs no new run."""
    steps = {s[0]: s for s in plan(handoff.conversations(args.answers))}
    recs = base.read_out(os.path.join(args.out, "outputs.jsonl"))
    notes = sent_notes(recs)
    scored = []
    for r in recs:
        _, step, group, arg, heads = steps[r["id"]]
        scored.append({**r, **score(r["text"], facts_of(step, group, arg, notes), heads)})
    print("| Step | Arm | Outputs | Cut off | All sections in order | Says it is not backed | Facts kept | "
          "Median words | Median seconds |")
    print("|---|---|---|---|---|---|---|---|---|")
    for step in STEPS:
        for arm in ARMS:
            rs = [r for r in scored if r["step"] == step and r["arm"] == arm]
            if rs:
                print(f"| {step} | {arm} | {len(rs)} | {sum(r['finish'] == 'length' for r in rs)} | "
                      f"{sum(r['sections'] for r in rs)} | {sum(r['disclaims'] for r in rs)} | "
                      f"{sum(r['kept'] for r in rs)}/{sum(r['facts'] for r in rs)} | "
                      f"{statistics.median(r['words'] for r in rs):.0f} | "
                      f"{statistics.median(r['secs'] for r in rs):.0f} |")
    by = {(r["id"], r["arm"]): r["kept"] for r in scored}
    for arm, against in (("BEFORE", "SENT"), ("AFTER", "SENT"), ("AFTER", "BEFORE"), ("KEEP", "SENT"),
                         ("KEEP_AFTER", "SENT"), ("KEEP", "BEFORE")):
        diffs = [by[i, arm] - by[i, against] for i in steps if (i, arm) in by and (i, against) in by]
        if diffs:
            rng = random.Random(0)
            boots = sorted(sum(rng.choices(diffs, k=len(diffs))) for _ in range(10000))
            print(f"{arm} - {against}: {sum(diffs):+d} [{boots[250]:+d}, {boots[9749]:+d}] over {len(diffs)} pairs; "
                  f"{sum(d > 0 for d in diffs)} better, {sum(d < 0 for d in diffs)} worse")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--answers", required=True, help="a shipped-path answers file on desktop-path-v1")
    ap.add_argument("--system", help="the app's default chat system prompt")
    ap.add_argument("--max-tokens", type=int, default=8000, help="the app's own cap is what is left of the window")
    ap.add_argument("--arms", default="SENT,BEFORE,AFTER")
    ap.add_argument("--steps", default=",".join(STEPS))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    args.arms, args.steps = args.arms.split(","), args.steps.split(",")
    if bad := [a for a in args.arms if a not in ARMS] + [s for s in args.steps if s not in STEPS]:
        sys.exit(f"unknown arms or steps: {bad}")
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    if not args.system:
        sys.exit("--system is required for a run")
    if args.claude:
        claude.use(args.claude, os.path.join(args.out, "claude-spend.jsonl"), args.budget, thinking=args.thinking)
        base.CLAUDE = claude
    else:
        base.URL = args.llama
    run(args)


if __name__ == "__main__":
    main()
