#!/usr/bin/env python3
"""Replay requests a shipped-path run recorded, with one thing changed.

    wording  dp-mt-plant-room-key's writing round with the sentence about language,
             and every continuation round with the sentence about notes.
    markers  every follow-up turn with the old answers' [n] markers removed.

The requests must come from a run made before the change, so `wording` needs a run
from before 2026-11-01. `RUN.jsonl` is the eval's output, and its calls sit beside it
in `RUN.jsonl.calls.jsonl`. Those hold whole prompts, so they are never committed.

    ANTHROPIC_API_KEY=... python -X utf8 scripts/replay_recorded_turns.py wording haiku \
        --repeats 5 --out out.jsonl RUN.jsonl
    LLAMA_REPLAY_URL=... python -X utf8 scripts/replay_recorded_turns.py markers 27b \
        --out out.jsonl RUN.jsonl

The 27B reads LLAMA_AUTH_HEADER_NAME and LLAMA_AUTH_HEADER_VALUE when they are set.
`--dry-run` prints the jobs and sends nothing. The flags in a record are coarse:
`leak` matches any "Note:", and `german` any quoted German, so read the answers.
"""
import argparse
import concurrent.futures
import copy
import json
import os
import re
import time
import urllib.error
import urllib.request

from replay_model import one_model, served_model

LANGUAGE_ADD = " Write in the language of my question, translating any passage in another."
NOTES_OLD = "If your answer already covers my question, reply with a single hyphen."
NOTES_NEW = NOTES_OLD + " Do not mention your notes or correct them."
GERMAN = re.compile(r"\b(der|die|das|liegt|Tresor|nur gegen|Ersatzschl)\b", re.I)
LEAK = re.compile(r"\bnotes?\b|hyphen", re.I)
CORRECTION = re.compile(
    r"correct(s|ing)? my (earlier|previous)|correction to my|cited the wrong|wrong passage"
    r"|my (earlier|previous) (answer|reply) (was|cited|said|had)", re.I)
MARKER = re.compile(r"\s*\[\d+(?:,\s*\d+)*\]")


def text_of(message):
    content = message.get("content")
    if isinstance(content, str):
        return content
    return " ".join(block.get("text", "") for block in content or [] if block.get("type") == "text")


def set_last(request, change):
    copied = copy.deepcopy(request)
    message = copied["messages"][-1]
    if isinstance(message["content"], str):
        message["content"] = change(message["content"])
        return copied
    for block in reversed(message["content"]):
        if block.get("type") == "text":
            block["text"] = change(block["text"])
            break
    return copied


def calls_in(run):
    with open(run + ".calls.jsonl", encoding="utf-8") as file:
        return [json.loads(line) for line in file]


def wording_jobs(runs, repeats):
    jobs, seen = [], set()
    for run in runs:
        for row in calls_in(run):
            request = row["call"].get("request") or {}
            messages = request.get("messages") or []
            if not messages or messages[-1].get("role") != "user":
                continue
            last = text_of(messages[-1])
            query = row.get("query_id")
            key = (query, last[:80])
            if key in seen:
                continue
            if query == "dp-mt-plant-room-key" and "own words" in last and "Is there anything" not in json.dumps(messages):
                seen.add(key)
                for repeat in range(repeats):
                    jobs.append(("language", query, "old", repeat, request))
                    jobs.append(("language", query, "new", repeat, set_last(request, lambda text: text + LANGUAGE_ADD)))
            if NOTES_OLD in last:
                seen.add(key)
                jobs.append(("notes", query, "old", 0, request))
                jobs.append(("notes", query, "new", 0, set_last(request, lambda text: text.replace(NOTES_OLD, NOTES_NEW))))
    return jobs


def without_markers(request, turn):
    """Only the finished answers of earlier turns, which sit before this turn's passages."""
    copied = copy.deepcopy(request)
    done = 0
    for message in copied["messages"]:
        if message["role"] != "assistant" or done >= turn:
            continue
        done += 1
        if isinstance(message["content"], str):
            message["content"] = MARKER.sub("", message["content"])
        else:
            message["content"] = [
                {**block, "text": MARKER.sub("", block["text"])} if block.get("type") == "text" else block
                for block in message["content"]]
    return copied


def marker_jobs(runs, repeats):
    jobs = []
    for run in runs:
        by_query = {}
        for row in calls_in(run):
            by_query.setdefault(row.get("query_id"), []).append(row["call"])
        with open(run, encoding="utf-8") as file:
            items = [json.loads(line) for line in file]
        for item in items:
            for index, turn in enumerate(item["turns"]):
                answer = (turn.get("answer") or "").strip()
                if index == 0 or not answer:
                    continue
                matching = [
                    (call.get("request") or {}) for call in by_query.get(item["query_id"], [])
                    if (call.get("request") or {}).get("messages")
                    and ((call.get("reply") or {}).get("content") or "").strip()[:60] == answer[:60]]
                if matching:
                    for repeat in range(repeats):
                        jobs.append(("markers", item["query_id"], "as-sent", repeat, matching[-1], index))
                        jobs.append(("markers", item["query_id"], "stripped", repeat, without_markers(matching[-1], index), index))
    return jobs


def post(url, body, headers, timeout):
    data = json.dumps(body).encode("utf-8")
    for attempt in range(4):
        try:
            request = urllib.request.Request(url, data=data, method="POST", headers=headers)
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 503, 529) or attempt == 3:
                return {"error": error.code, "detail": error.read().decode("utf-8", "replace")[:200]}
        except OSError as error:
            if attempt == 3:
                return {"error": type(error).__name__}
        time.sleep(6 * (attempt + 1))
    return {"error": "gave up"}


def sender(model):
    if model == "haiku":
        headers = {"x-api-key": os.environ["ANTHROPIC_API_KEY"], "anthropic-version": "2023-06-01",
                   "content-type": "application/json"}

        def send(request):
            keep = ("model", "system", "messages", "tools", "thinking", "output_config")
            body = {**{k: v for k, v in request.items() if k in keep}, "max_tokens": 8192, "stream": False}
            return post("https://api.anthropic.com/v1/messages", body, headers, 180)

        return send
    url = os.environ["LLAMA_REPLAY_URL"].rstrip("/") + "/v1/chat/completions"
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    headers = {"content-type": "application/json", **({name: value} if name and value else {})}

    def send(request):
        body = {**request, "stream": False}
        body.pop("stream_options", None)
        return post(url, body, headers, 600)

    return send


def text_from(reply):
    if "choices" in reply:
        return (reply["choices"][0].get("message") or {}).get("content") or ""
    return "".join(block.get("text", "") for block in reply.get("content", []) if block.get("type") == "text")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("experiment", choices=("wording", "markers"))
    parser.add_argument("model", choices=("haiku", "27b"))
    parser.add_argument("runs", nargs="+")
    parser.add_argument("--out", required=True)
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    jobs = (wording_jobs if args.experiment == "wording" else marker_jobs)(args.runs, args.repeats)
    print("jobs", len(jobs), {kind: sum(1 for job in jobs if job[0] == kind) for kind in {job[0] for job in jobs}}, flush=True)
    if args.dry_run:
        return
    send = sender(args.model)
    model = one_model([job[4] for job in jobs]) if args.model == "haiku" else served_model(os.environ["LLAMA_REPLAY_URL"])

    def one(job):
        kind, query, arm, repeat, request = job[:5]
        reply = send(request)
        text = text_from(reply)
        row = {"kind": kind, "query": query, "arm": arm, "repeat": repeat, "text": text, "error": reply.get("error")}
        if kind == "markers":
            return {**row, "turn": job[5], "corrected": bool(CORRECTION.search(text))}
        return {**row, "german": bool(GERMAN.search(text)), "leak": bool(LEAK.search(text)), "hyphen": text.strip() == "-"}

    with concurrent.futures.ThreadPoolExecutor(max_workers=4 if args.model == "haiku" else 1) as pool:
        for result in pool.map(one, jobs):
            with open(args.out, "a", encoding="utf-8") as out:
                out.write(json.dumps({"model": model, **result}, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
