#!/usr/bin/env python3
"""Does the reading of a follow-up read the passages found for this question, or only the one carried over from the last?

A follow-up's context holds the earlier turn, so its reading request can be taken for a repeat of the earlier question: the
reading quotes only the passage that answered it and writes a hyphen for every passage found now, and the answer repeats the
last one. `tool_loop/reading/groups.rs` now says the question again after the passages. This takes the logged reading
requests of every follow-up turn of a shipped-path run and sends them exactly as the app sent them, under each arm:

  without   the question not said again, as before
  restated  `My question is: "..."` after the passages, as groups.rs now writes it

A reply counts as `fresh` when it quotes a passage found for this question, `only carried` when it quotes only passages
carried over, and `nothing` when every line is a hyphen. Labels past `--fresh-limit` are the carried ones: a turn numbers the
passages it found first.

  python -X utf8 -I -u scripts/replay_followup_reading_ab.py --run RUN.jsonl --out OUT.jsonl [--llama URL] [--repeats 10]
  python -X utf8 -I scripts/replay_followup_reading_ab.py --summary OUT.jsonl

`RUN.jsonl` is a shipped_path_eval run, with its `.calls.jsonl` beside it. Without `--llama` the requests go to Claude. An auth
header comes from LLAMA_AUTH_HEADER_NAME and LLAMA_AUTH_HEADER_VALUE, never from a command line.
"""
import argparse
import concurrent.futures as futures
import copy
import json
import os
import re
import sys
import threading
import time
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

ARMS = ("without", "restated")
INSTRUCTION = re.compile(r"Write one line for each of these passages")
SAID_AGAIN = re.compile(r'My question is: ".*"\n\n(?=Write one line for each of these passages)', re.S)
LINE = re.compile(r"^\s*\[(\d+)\]\s*(.*)$", re.M)
WORKERS = 5


def without_question(text):
    """The reading's closing message with the question not said again."""
    return SAID_AGAIN.sub("", text)


def restated(text, question):
    """The reading's closing message as groups.rs writes it."""
    text = without_question(text)
    at = list(INSTRUCTION.finditer(text))[-1].start()
    return f'{text[:at]}My question is: "{question}"\n\n{text[at:]}'


def current_question(questions, messages):
    """The turn's own question: the one the closing message holds and no earlier message does."""
    earlier = json.dumps(messages[:-1], ensure_ascii=False)
    last = messages[-1]["content"]
    current = [q for q in questions if q in last and json.dumps(q, ensure_ascii=False)[1:-1] not in earlier]
    if not current:
        raise SystemExit("a reading request holds no question of its turn")
    return current[-1]


def reading_requests(run):
    """The logged reading requests of follow-up turns: a follow-up carries the earlier turn before its own message."""
    run = Path(run)
    questions = {}
    for line in run.read_text("utf-8").splitlines():
        row = json.loads(line)
        questions[row["query_id"]] = [turn["question"] for turn in row["turns"]]
    items = []
    for line in Path(f"{run}.calls.jsonl").read_text("utf-8").splitlines():
        entry = json.loads(line)
        call = entry["call"]
        if not isinstance(call, dict) or "request" not in call:
            continue
        messages = call["request"].get("messages") or []
        if len(messages) < 3 or not isinstance(messages[-1]["content"], str) or not INSTRUCTION.search(messages[-1]["content"]):
            continue
        items.append({
            "query": entry["query_id"],
            "seq": call["seq"],
            "request": call["request"],
            "question": current_question(questions[entry["query_id"]], messages),
        })
    return items


def arm_request(item, arm):
    request = copy.deepcopy(item["request"])
    last = request["messages"][-1]
    last["content"] = restated(last["content"], item["question"]) if arm == "restated" else without_question(last["content"])
    return request


def ask_claude(client, request):
    body = copy.deepcopy(request)
    body.pop("stream", None)
    extra = {key: body.pop(key) for key in ("cache_control", "output_config") if key in body}
    with client.messages.stream(**body, extra_body=extra or None) as stream:
        message = stream.get_final_message()
    return "".join(block.text for block in message.content if block.type == "text"), message.usage.output_tokens


def ask_llama(url, request):
    body = copy.deepcopy(request)
    body["stream"] = False
    # Greedy decoding gives one answer to one prompt, so a repeat is only worth asking with a seed of its own.
    outgoing = urllib.request.Request(
        url.rstrip("/") + "/v1/chat/completions", json.dumps(body).encode("utf-8"), {"Content-Type": "application/json"}
    )
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        outgoing.add_header(name, value)
    with urllib.request.urlopen(outgoing, timeout=1800) as response:
        reply = json.load(response)
    return reply["choices"][0]["message"]["content"] or "", reply["usage"]["completion_tokens"]


def run(args):
    items = reading_requests(args.run)
    print("follow-up readings", len(items), Counter(item["query"] for item in items).most_common(40), flush=True)
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["arm"], r["seq"], r["repeat"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    if args.llama:
        import replay_model

        model = replay_model.served_model(args.llama)
        ask = lambda request: ask_llama(args.llama, request)  # noqa: E731
        workers = 1
    else:
        import anthropic

        import replay_claude as claude

        client = anthropic.Anthropic(api_key=claude.api_key(), max_retries=10, timeout=900)
        ask = lambda request: ask_claude(client, request)  # noqa: E731
        model = None
        workers = WORKERS
    lock = threading.Lock()

    def one(task):
        item, arm, repeat = task
        request = arm_request(item, arm)
        text, output = ask(request)
        row = {"arm": arm, "query": item["query"], "seq": item["seq"], "repeat": repeat, "text": text, "output": output}
        with lock, out.open("a", encoding="utf-8") as sink:
            sink.write(json.dumps({"model": model or request["model"], **row}, ensure_ascii=False) + "\n")

    tasks = [
        (item, arm, repeat)
        for item in items
        for arm in args.arms.split(",")
        for repeat in range(args.repeats)
        if (arm, item["seq"], repeat) not in done
    ]
    with futures.ThreadPoolExecutor(workers) as pool:
        list(pool.map(one, tasks))
    print("done", time.strftime("%H:%M"), flush=True)


def kind_of_reply(text, fresh_limit):
    kept = [int(label) for label, body in LINE.findall(text) if body.strip() and not re.fullmatch(r"[-–—]\s*", body.strip())]
    if any(label <= fresh_limit for label in kept):
        return "fresh"
    return "only carried" if kept else "nothing"


def summary(path, fresh_limit):
    rows = [json.loads(line) for line in Path(path).read_text("utf-8").splitlines() if line.strip()]
    print("model", sorted({row["model"] for row in rows}))
    totals = defaultdict(Counter)
    per_question = defaultdict(lambda: defaultdict(Counter))
    for row in rows:
        kind = kind_of_reply(row["text"], fresh_limit)
        totals[row["arm"]][kind] += 1
        per_question[row["query"], row["seq"]][row["arm"]][kind] += 1
    for arm, counts in totals.items():
        print(arm, dict(counts), "of", sum(counts.values()))
    for (query, seq), arms in per_question.items():
        print(query[:34].ljust(35), seq, {arm: dict(counts) for arm, counts in arms.items()})


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--run")
    ap.add_argument("--out")
    ap.add_argument("--summary")
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--repeats", type=int, default=10)
    ap.add_argument("--fresh-limit", type=int, default=40)
    ap.add_argument("--llama")
    args = ap.parse_args()
    if args.summary:
        summary(args.summary, args.fresh_limit)
    elif args.run and args.out:
        run(args)
    else:
        sys.exit("give --summary, or --run and --out")


if __name__ == "__main__":
    main()
