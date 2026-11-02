#!/usr/bin/env python3
"""Replay the check that sorts a tutor's uncited sentences, on sentences a reader labelled.

The grounding check marks a sentence that cites nothing as not found in the sources, which is right for an uncited
claim about the subject and wrong for "Good, next card". `ClaimJudge::about_the_subject` asks the judge which is
which, one call a turn, as the Rust constant `SESSION_TALK_SYSTEM` says. This sends that prompt the uncited sentences
of each turn of a run from before that check existed, with one invented false claim mixed in, because the failure
that matters is a false claim dropped as talk.

Labels and false claims: `evals/tutor/session-talk-v1.json`. Each saved row names the model the server says it serves.

Usage, with LLAMA_AUTH_HEADER_NAME and LLAMA_AUTH_HEADER_VALUE set for a server that wants them:
  python -X utf8 scripts/replay_session_talk.py URL OUT.jsonl RUN.jsonl[.gz] ...
"""
import argparse
import gzip
import json
import os
import random
import re
import urllib.request
from pathlib import Path

from replay_model import one_model, served_model

ROOT = Path(__file__).resolve().parents[1]
JUDGE = ROOT / "crates/lattice-desktop/src/features/conversation/chat/verification/judge.rs"
LABELS = ROOT / "evals/tutor/session-talk-v1.json"


def shipped_prompt():
    """The prompt the app sends, read from its source so a replay cannot drift from it."""
    return re.search(r'const SESSION_TALK_SYSTEM: &str = "(.*?)";', JUDGE.read_text(encoding="utf-8")).group(1)


def read_rows(path):
    opened = gzip.open(path, "rt", encoding="utf-8") if str(path).endswith(".gz") else open(path, encoding="utf-8")
    with opened as lines:
        return [json.loads(line) for line in lines if line.strip()]


def uncited_flagged(row):
    """Each turn's sentences the check flagged that cite nothing, as (session, turn, sentences)."""
    for turn in row["turns"]:
        claims = (turn.get("verification") or {}).get("claimVerdicts") or []
        sentences = [c["sentence"] for c in claims if c["verdict"] == "unsupported" and not c.get("citationIds")]
        if sentences:
            yield row["query_id"], turn["index"], sentences


def batches(rows, false_claims, seed=7):
    """Each turn's sentences with a false claim put in at a place the seed picks."""
    picker = random.Random(seed)
    turns = [turn for row in rows for turn in uncited_flagged(row)]
    for number, (session, index, sentences) in enumerate(turns):
        at = picker.randint(0, len(sentences))
        planted = false_claims[number % len(false_claims)]
        yield {"session": session, "turn": index, "sentences": sentences[:at] + [planted] + sentences[at:], "planted": at}


def listed(sentences):
    return "\n".join(f"{number + 1}. {sentence}" for number, sentence in enumerate(sentences))


def positions(reply):
    """The numbers of `reply`, less one, when it is one JSON array of numbers and nothing else, as the app reads it
    (`listed_positions`); None otherwise."""
    text = (reply or "").strip()
    for fence in ("```json", "```"):
        if text.startswith(fence) and text.endswith("```") and len(text) >= len(fence) + 3:
            text = text[len(fence):-3]
            break
    try:
        numbers = json.loads(text.strip())
    except ValueError:
        return None
    if not isinstance(numbers, list) or not all(isinstance(number, int) and number >= 0 for number in numbers):
        return None
    return {number - 1 for number in numbers if number >= 1}


def is_subject(sentence, openings):
    return any(sentence.startswith(opening) for opening in openings)


def score(rows, openings):
    """What the check would have done across `rows`, against the reader's labels."""
    totals = {"turns": len(rows), "unreadable": 0, "false_claims_dropped": [], "subject": 0, "subject_dropped": [],
              "talk": 0, "talk_kept": 0}
    for row in rows:
        picked = None if row["picked"] is None else set(row["picked"])
        if picked is None:
            totals["unreadable"] += 1
            continue
        for position, sentence in enumerate(row["sentences"]):
            if position == row["planted"]:
                if position not in picked:
                    totals["false_claims_dropped"].append(sentence)
            elif is_subject(sentence, openings):
                totals["subject"] += 1
                if position not in picked:
                    totals["subject_dropped"].append(sentence)
            else:
                totals["talk"] += 1
                totals["talk_kept"] += position in picked
    return totals


def ask(url, model, system, sentences):
    body = {"model": model, "messages": [{"role": "system", "content": system}, {"role": "user", "content": listed(sentences)}],
            "max_tokens": 300, "temperature": 0, "stream": False, "chat_template_kwargs": {"enable_thinking": False}}
    request = urllib.request.Request(url.rstrip("/") + "/v1/chat/completions", data=json.dumps(body).encode(),
                                     headers={"content-type": "application/json"}, method="POST")
    if os.environ.get("LLAMA_AUTH_HEADER_NAME"):
        request.add_header(os.environ["LLAMA_AUTH_HEADER_NAME"], os.environ["LLAMA_AUTH_HEADER_VALUE"])
    with urllib.request.urlopen(request, timeout=300) as response:
        return json.load(response)["choices"][0]["message"]["content"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("url")
    parser.add_argument("out")
    parser.add_argument("runs", nargs="+")
    args = parser.parse_args()
    labels = json.loads(LABELS.read_text(encoding="utf-8"))
    runs = [row for path in args.runs for row in read_rows(path)]
    system, model = shipped_prompt(), served_model(args.url)
    rows = []
    with open(args.out, "w", encoding="utf-8") as out:
        for batch in batches(runs, labels["false_claims"]):
            reply = ask(args.url, model, system, batch["sentences"])
            picked = positions(reply)
            row = {**batch, "reply": reply[:300], "picked": None if picked is None else sorted(picked)}
            rows.append({"model": model, **row})
            out.write(json.dumps({"model": model, **row}, ensure_ascii=False) + "\n")
    print(json.dumps({"model": one_model(rows), **score(rows, labels["subject_openings"])}, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
