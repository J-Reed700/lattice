#!/usr/bin/env python3
"""Should a lesson block that asserts nothing, or a topic with no evidence, have to cite a passage?

The lesson contract says every block cites a verbatim quote, and its schema does not allow null. That covers
reflection, recap and practice blocks, and blocks on a topic the same prompt says has no evidence. This replays a saved
lesson draft through its two steps with an added topic that no source covers, under the shipped contract and one that
lets those blocks carry no citation, and counts what each writes. A third arm keeps the shipped citation rule and says how
long a quote must be: `validate_excerpt_quote` refuses one under 25 characters or five words, and neither the prompt nor
the schema says so.

  python -I -u scripts/replay_lesson_citation_ab.py --calls DIR --llama URL --out OUT.jsonl [--seeds 3]
  python -I scripts/replay_lesson_citation_ab.py --calls DIR --summary OUT.jsonl

DIR holds `call-N.json` files from a run with LATTICE_LESSON_CALLS set; `--draft N` names the first step of the lesson
(default 2). An auth header comes from LLAMA_AUTH_HEADER_NAME and LLAMA_AUTH_HEADER_VALUE, never from a command line.
"""
import argparse
import json
import os
import re
import sys
import unicodedata
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from replay_model import one_model, served_model

ARMS = ("must_cite", "may_omit", "whole_sentence")
UNCITED_KINDS = {"reflection", "recap", "guided_practice", "independent_practice"}
UNCOVERED_TOPIC = "Undoing a Done card by supervisor override"
UNCOVERED_WORDS = ("supervisor", "override")
STEP_TWO = (
    "Step 2: give the final answer from your draft. Keep its content, wording, numbers, citations and answers. Copy "
    "quotations and identifiers exactly as the task supplies them. Where the draft left something out or stopped "
    "early, complete it from the task."
)
EVERY_PART = "Every block, outcome, summary and question must cite a supplied sourceIndex with a verbatim quote."
TEACHING_PARTS = (
    "Every explanation, worked_example, contrast and check block, and every outcome, summary and question, must cite "
    "a supplied sourceIndex with a verbatim quote. A reflection, recap, guided_practice or independent_practice block "
    "that states no new fact sets sourceIndex to null and quote to an empty string."
)
NO_EVIDENCE = "For a topic without evidence, say what the learner should find out and why, asserting no facts about it."
NO_EVIDENCE_NEW = (
    "For a topic without evidence, write a reflection block saying what the learner should find out and why, "
    "asserting no facts about it, with no citation."
)
WHOLE_SENTENCE = (
    "Every block, outcome, summary and question must cite a supplied sourceIndex with a verbatim quote: at least one "
    "whole sentence of five words or more, copied exactly, with no ellipsis."
)
RUBRIC = {
    "type": "array", "maxItems": 4,
    "items": {
        "type": "object", "additionalProperties": False, "required": ["title", "description", "dimension"],
        "properties": {
            "title": {"type": "string", "minLength": 1},
            "description": {"type": "string", "minLength": 1},
            "dimension": {"enum": ["recall", "explanation", "application", "transfer"]},
        },
    },
}


def schema(arm, sources):
    required_index = {"type": "integer", "minimum": 0, "maximum": sources - 1}
    block_index = (
        {"type": ["integer", "null"], "minimum": 0, "maximum": sources - 1} if arm == "may_omit" else required_index
    )
    quote = {"type": "string"}
    if arm == "whole_sentence":
        quote = {"type": "string", "minLength": 25}
    return {
        "type": "object", "additionalProperties": False, "required": ["blocks", "questions"],
        "properties": {
            "blocks": {"type": "array", "minItems": 10, "maxItems": 16, "items": {
                "type": "object", "additionalProperties": False,
                "required": ["kind", "title", "body", "rubric", "sourceIndex", "quote"],
                "properties": {
                    "kind": {"type": "string", "enum": ["explanation", "worked_example", "contrast", "check",
                                                         "guided_practice", "independent_practice", "reflection",
                                                         "recap"]},
                    "title": {"type": "string"}, "body": {"type": "string", "minLength": 1},
                    "rubric": RUBRIC, "sourceIndex": block_index, "quote": quote,
                },
            }},
            "questions": {"type": "array", "minItems": 6, "maxItems": 6, "items": {
                "type": "object", "additionalProperties": False,
                "required": ["kind", "prompt", "options", "correctIndex", "explanation", "sourceIndex", "quote"],
                "properties": {
                    "kind": {"type": "string", "enum": ["practice", "quiz", "test"]},
                    "prompt": {"type": "string"},
                    "options": {"type": "array", "minItems": 4, "maxItems": 4,
                                "items": {"type": "string"}},
                    "correctIndex": {"type": "integer", "minimum": 0, "maximum": 3},
                    "explanation": {"type": "string"}, "sourceIndex": required_index, "quote": quote,
                },
            }},
        },
    }


def request_for(calls, draft, arm):
    call = json.loads((Path(calls) / f"call-{draft}.json").read_text("utf-8"))
    system = call["input"][0]["content"]
    user = json.loads(call["input"][1]["content"])
    user["evidenceByTopic"].insert(-1, {"topic": UNCOVERED_TOPIC, "coveringSourceIndexes": []})
    if arm == "may_omit":
        assert system.count(EVERY_PART) == 1 and system.count(NO_EVIDENCE) == 1
        system = system.replace(EVERY_PART, TEACHING_PARTS).replace(NO_EVIDENCE, NO_EVIDENCE_NEW)
    if arm == "whole_sentence":
        assert system.count(EVERY_PART) == 1
        system = system.replace(EVERY_PART, WHOLE_SENTENCE)
    return system, json.dumps(user, ensure_ascii=False), user["sources"]


def post(url, body):
    request = urllib.request.Request(
        url.rstrip("/") + "/v1/chat/completions", json.dumps(body).encode("utf-8"),
        {"Content-Type": "application/json"},
    )
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        request.add_header(name, value)
    with urllib.request.urlopen(request, timeout=3000) as response:
        choice = json.load(response)["choices"][0]
    return choice["message"]["content"], choice.get("finish_reason")


def run(args):
    model = served_model(args.llama)
    print(model, flush=True)
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["arm"], r["seed"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    with out.open("a", encoding="utf-8") as sink:
        for seed in range(1, args.seeds + 1):
            for arm in ARMS:
                if (arm, seed) in done:
                    continue
                system, user, sources = request_for(args.calls, args.draft, arm)
                messages = [{"role": "system", "content": system}, {"role": "user", "content": user}]
                common = {"temperature": 0.2, "seed": seed, "cache_prompt": False,
                          "chat_template_kwargs": {"enable_thinking": False}}
                draft, _ = post(args.llama, {"messages": messages, "max_tokens": 9000, **common})
                messages += [{"role": "assistant", "content": draft}, {"role": "user", "content": STEP_TWO}]
                final, finish = post(args.llama, {
                    "messages": messages, "max_tokens": 12000, **common,
                    "response_format": {"type": "json_schema", "json_schema": {
                        "name": "response", "strict": True, "schema": schema(arm, len(sources))}},
                })
                sink.write(json.dumps({"model": model, "arm": arm, "seed": seed, "finish": finish, "final": final},
                                      ensure_ascii=False) + "\n")
                sink.flush()
                print(arm, seed, finish, flush=True)


def normalize(text):
    text = unicodedata.normalize("NFKC", text).lower()
    return re.sub(r"\s+", " ", text).strip()


def quote_holds(quote, excerpt):
    quote = quote.strip()
    return 25 <= len(quote) <= 1200 and len(quote.split()) >= 5 and normalize(quote) in normalize(excerpt)


def summary(args):
    records = [json.loads(line) for line in Path(args.summary).read_text("utf-8").splitlines()]
    print("Model:", one_model(records))
    _, _, sources = request_for(args.calls, args.draft, "must_cite")
    totals = defaultdict(Counter)
    for record in records:
        arm = record["arm"]
        totals[arm]["lessons"] += 1
        try:
            parsed = json.loads(record["final"])
        except ValueError:
            totals[arm]["unparsed"] += 1
            continue
        for block in parsed["blocks"]:
            index = block["sourceIndex"]
            uncited = block["kind"] in UNCITED_KINDS
            on_topic = any(word in (block["title"] + block["body"]).lower() for word in UNCOVERED_WORDS)
            totals[arm]["blocks"] += 1
            totals[arm]["reflective blocks"] += uncited
            totals[arm]["reflective blocks citing"] += uncited and index is not None
            totals[arm]["blocks on the uncovered topic"] += on_topic
            totals[arm]["uncovered-topic blocks citing"] += on_topic and index is not None
            if index is None:
                totals[arm]["teaching blocks left uncited"] += not uncited and not on_topic
            elif not quote_holds(block["quote"], sources[index]["excerpt"]):
                totals[arm]["citations a review would flag"] += 1
        for question in parsed["questions"]:
            totals[arm]["questions"] += 1
            if not quote_holds(question["quote"], sources[question["sourceIndex"]]["excerpt"]):
                totals[arm]["question citations a review would flag"] += 1
    for arm in ARMS:
        print(arm, dict(totals[arm]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--calls", required=True)
    parser.add_argument("--draft", type=int, default=2)
    parser.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    parser.add_argument("--out")
    parser.add_argument("--seeds", type=int, default=3)
    parser.add_argument("--summary")
    args = parser.parse_args()
    if args.summary:
        summary(args)
    elif args.out:
        run(args)
    else:
        sys.exit("give --out or --summary")


if __name__ == "__main__":
    main()
