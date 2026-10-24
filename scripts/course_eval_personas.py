#!/usr/bin/env python3
"""Draft learner answers for the quiz grading set from a model playing learner personas.

Design: docs/design/2026-10-23-quizzes-and-tests.md section 7.2; protocol: evals/courses/README.md ("Grading set").
The hand-written seed rows cover every format; persona rows bring the set to its full size against the eval
lessons' real banks once they are in the set's bank file. A drafted row is never part of the set until a person
reads it, corrects its persona or answer where the model missed, and sets "reviewed": true; `merge` then appends
the reviewed rows, recomputing each row's expected grade, and refuses a frozen set.

  generate NAME --endpoint URL --model NAME --out DRAFT.jsonl [--items ID,...] [--personas P,...] [--per-item N]
           [--seed S]
  merge DRAFT.jsonl NAME

Empty answers need no model. The model call has no time limit: a shared server may queue it.
"""
import argparse
import json
import re
import urllib.request
from pathlib import Path

import course_eval_grading as grading
import course_eval_quiz as cq
from chat_rag_eval import load_jsonl
from eval_integrity import digest
from shipped_path_eval import judge_headers

SYSTEM = ("You write test data for a quiz grader. You play one learner and answer one quiz item the way that "
          "learner would type it: plain words, no headings, no mention of the persona or the key. Return JSON with "
          "answer and note; note is the learner's argument for more credit when the persona contests, else empty.")
BRIEFS = {
    "correct": "You understood the lesson. Answer correctly and completely, in your own words, not the textbook's.",
    "near_miss": "You nearly understood. Give an answer that is close but wrong or incomplete in one specific way "
                 "a learner plausibly gets wrong.",
    "partial": "You understood about half. Get part of the answer right and clearly miss or get wrong the rest.",
    "wrong": "You misunderstood. Answer confidently from a plausible misconception.",
    "off_topic": "You answer a different question about the same subject and never address what was asked.",
    "contesting": "Give an answer that is arguably right but unusual in wording or approach, then argue in the note, "
                  "in one or two sentences, that it deserves full credit.",
}
SCHEMA = {"type": "object", "properties": {"answer": {"type": "string"}, "note": {"type": "string"}},
          "required": ["answer", "note"]}
TEMPERATURE = 0.8  # persona answers need variety; the seed and the prompt hash are recorded on every row


def personas_for(item):
    """The personas that mean something for a format: partial credit needs parts, a contest needs a model grade,
    and off-topic needs room to write."""
    fmt = item["format"]
    if fmt == "explanation":
        return list(grading.PERSONAS)
    if fmt == "short_answer":
        return ["correct", "near_miss", "wrong", "off_topic", "empty", "contesting"]
    if fmt in ("cloze", "matching", "ordering", "multi_select"):
        return ["correct", "near_miss", "partial", "wrong", "empty"]
    return ["correct", "wrong", "empty"]


def empty_response(item):
    field = cq.RESPONSE_FIELD[item["format"]]
    if field == "text":
        return {"text": ""}
    if field == "blank_responses":
        return {field: [""] * len(item.get("blanks") or [])}
    if field == "matches":
        return {field: [None] * len(item.get("left") or [])}
    return {field: []}


def messages(item, persona, subject):
    user = [f"Subject: {subject}", f"Persona: {BRIEFS[persona]}", "", "Item (with its key, so you know what to "
            "get right or wrong):", "<<<", cq.render_item(item, key=True), ">>>", "",
            f"Write the answer as {cq.answer_format(item)}."]
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": "\n".join(user)}]


def http_writer(endpoint, model, seed):
    url, headers = endpoint.rstrip("/") + "/v1/chat/completions", judge_headers()

    def call(prompt):
        body = {"model": model, "messages": prompt, "stream": False, "temperature": TEMPERATURE, "seed": seed,
                "chat_template_kwargs": {"enable_thinking": False},
                "response_format": {"type": "json_schema", "json_schema": {"name": "answer", "schema": SCHEMA}}}
        request = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST", headers=headers)
        with urllib.request.urlopen(request) as reply:
            content = json.load(reply)["choices"][0]["message"]["content"]
        return json.loads(re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL).strip())
    return call


def generate(name, call, writer, out, items=None, personas=None, per_item=1, seed=2026,
             registry=grading.REGISTRY):
    """Draft rows into `out` (never overwritten) and return (written, skipped with reasons)."""
    out = Path(out)
    if out.exists():
        raise SystemExit(f"{out} exists; a draft is never overwritten")
    rows, bank, entry, problems = grading.load_set(name, registry)
    if entry.get("frozen"):
        raise SystemExit(f"{name} is frozen; draft rows for a new version instead")
    taken = {row["id"] for row in rows}
    subjects = bank.get("subjects") or {}
    drafted, skipped = [], []
    for item in bank["items"]:
        if items and item["id"] not in items:
            continue
        for persona in personas_for(item):
            if personas and persona not in personas:
                continue
            for n in range(1, per_item + 1):
                row_id = re.sub(r"[^A-Za-z0-9_.-]+", "-", f"{item['id']}-{persona}-m{n}")
                if row_id in taken:
                    skipped.append((row_id, "id already in the set"))
                    continue
                note, prompt = "", None
                if persona == "empty":
                    response = empty_response(item)
                else:
                    prompt = messages(item, persona, (subjects.get(item.get("subject")) or {}).get("title", ""))
                    written = call(prompt)
                    response = cq.parse_answer(item, written.get("answer"))
                    note = str(written.get("note") or "").strip()
                    if response is None or grading.response_problems(item, response):
                        skipped.append((row_id, f"unreadable answer {written.get('answer')!r}"))
                        continue
                    if grading.is_empty(response):
                        skipped.append((row_id, "the model wrote nothing for a non-empty persona"))
                        continue
                expected = grading.expected_for(item, response)
                if persona == "contesting" and (expected["path"] != "model" or not note):
                    skipped.append((row_id, "a contest needs a model-graded answer and a note"))
                    continue
                row = {"id": row_id, "item": item["id"], "persona": persona, "author": f"model:{writer}",
                       "reviewed": False, "response": response}
                row |= {"contest_note": note} if persona == "contesting" else {}
                row |= {"expected": expected,
                        "generator": {"model": writer, "seed": seed, "temperature": TEMPERATURE,
                                      "prompt_sha256": digest(prompt) if prompt else None}}
                drafted.append(row)
                taken.add(row_id)
    grading.write_jsonl(out, drafted)
    return drafted, skipped


def merge(draft, name, registry=grading.REGISTRY):
    """Append the draft's reviewed rows to the set; the whole set must still validate."""
    rows, bank, entry, problems = grading.load_set(name, registry)
    if entry.get("frozen"):
        raise SystemExit(f"{name} is frozen; register a new version and merge into that")
    if problems:
        raise SystemExit(f"{name} is not valid before the merge: " + "; ".join(problems))
    drafted = load_jsonl(draft)
    reviewed = [row for row in drafted if row.get("reviewed") is True]
    items = {item["id"]: item for item in bank["items"]}
    for row in reviewed:
        if row.get("item") in items and not grading.response_problems(items[row["item"]], row.get("response")):
            row["expected"] = grading.expected_for(items[row["item"]], row["response"])
    problems = grading.set_problems(rows + reviewed, bank)
    if problems:
        raise SystemExit("the merged set would not be valid: " + "; ".join(problems))
    with Path(entry["rows_path"]).open("a") as out:
        for row in reviewed:
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
    return len(reviewed), len(drafted) - len(reviewed)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--registry", type=Path, default=grading.REGISTRY)
    sub = parser.add_subparsers(dest="command", required=True)
    draft = sub.add_parser("generate")
    draft.add_argument("name")
    draft.add_argument("--endpoint", required=True)
    draft.add_argument("--model", required=True)
    draft.add_argument("--out", type=Path, required=True)
    draft.add_argument("--items", help="comma-separated item ids (default: every item)")
    draft.add_argument("--personas", help="comma-separated personas (default: every one that fits the format)")
    draft.add_argument("--per-item", type=int, default=1, help="answers per item and persona")
    draft.add_argument("--seed", type=int, default=2026)
    join = sub.add_parser("merge")
    join.add_argument("draft", type=Path)
    join.add_argument("name")
    args = parser.parse_args(argv)
    if args.command == "generate":
        split = (lambda text: set(text.split(",")) if text else None)
        drafted, skipped = generate(args.name, http_writer(args.endpoint, args.model, args.seed), args.model,
                                    args.out, split(args.items), split(args.personas), args.per_item, args.seed,
                                    args.registry)
        print(f"{args.out}: {len(drafted)} rows drafted for review; {len(skipped)} skipped")
        for row_id, why in skipped:
            print(f"  {row_id}: {why}")
    else:
        merged, left = merge(args.draft, args.name, args.registry)
        print(f"{merged} reviewed rows merged into {args.name}; {left} left in the draft unreviewed")


if __name__ == "__main__":
    main()
