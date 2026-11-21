"""Whether the note step (chat/tool_loop/research_notes.rs) could have fired in a recorded shipped-path run at
another context window.

Each turn's tool rounds, as `shipped_path_eval` recorded them, go through the loop's own arithmetic at every window
named. The prompt may fill the input budget (the window less the output reservation and the safety margin) at the
turn's measured characters per token, under-filled by a quarter. Each call of a round is given its share of the
room left, one share kept back. At the start of every round after the first, the results are condensed once they
pass 60% of the window, and only then does the round ask whether the window has room for another result. The
planned request keeps its recorded size unless the window's input budget is smaller, when the turn's passages
would have been cut to fill it.

Approximations: the model is taken to make the same calls when its results are cut, and to answer where it
answered; the first round's window uses the measured rate, where the app starts from the backend's calibrated one.

  python -I scripts/condense_reach.py RUN.jsonl.calls.jsonl 16384 32768 65536 131072 196608
"""
import json
import math
import sys

CONDENSE_AT_WINDOW_SHARE = 0.6
MAX_CONDENSES_PER_TURN = 2
CONDENSE_INSTRUCTION_START = "Your context window is nearly full."
MIN_TOOL_RESULT_CHARS = 1500
UNDER_FILL = 0.75
MAX_TOOL_ROUNDS = 5
OUTPUT_RESERVATION_CAP = 32768
OUTPUT_RESERVATION_DIVISOR = 4
SAFETY_MARGIN_SHARE = 0.05
SAFETY_MARGIN_FLOOR = 256
CHAT_SYSTEM_START = "You are Lattice"
# How a reading request looked in recorded runs before the reading step was removed.
READ_INSTRUCTION = "Do not answer my question yet. First read these passages, each on its own."
# How a reading request ended before the reading became a contract.
OLD_READING_CLOSE = "Write only these lines"
PLANNED_MARK = "[current message]"


def text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return ""


def chars(message):
    """A message as `chars_in_flight` counts it: its text, and each call's name and arguments."""
    n = len(text(message.get("content")))
    for call in message.get("tool_calls") or []:
        f = call.get("function") or call
        arguments = f.get("arguments", "")
        n += len(f.get("name") or "") + len(arguments if isinstance(arguments, str) else json.dumps(arguments))
    return n


def input_budget(window_tokens):
    reserved = min(window_tokens // OUTPUT_RESERVATION_DIVISOR, OUTPUT_RESERVATION_CAP)
    margin = max(math.ceil(window_tokens * SAFETY_MARGIN_SHARE), SAFETY_MARGIN_FLOOR)
    return max(window_tokens - reserved - margin, 0)


def allowance(window, in_flight, calls_left):
    room = max(window - in_flight, 0)
    share = room // max(calls_left + 1, 2)
    return share if room < MIN_TOOL_RESULT_CHARS else max(share, MIN_TOOL_RESULT_CHARS)


def turns_of(lines):
    """The chat turns of a calls file: each turn's tool-round requests in order, and how often it condensed."""
    turns = []
    for line in lines:
        row = json.loads(line)
        call = row["call"]
        request = call.get("request")
        if call.get("method") != "POST" or not isinstance(request, dict) or not request.get("messages"):
            continue
        if (request.get("max_tokens") or 0) <= 1:
            continue
        messages = request["messages"]
        system = text(messages[0].get("content"))
        if system.startswith(CONDENSE_INSTRUCTION_START):
            if turns and turns[-1]["qid"] == row["query_id"]:
                turns[-1]["condensed"] += 1
            continue
        if not system.startswith(CHAT_SYSTEM_START):
            continue
        if OLD_READING_CLOSE in text(messages[-1].get("content")) or any(
                m.get("role") == "user" and READ_INSTRUCTION in text(m.get("content")) for m in messages):
            continue
        planned = max((i for i, m in enumerate(messages)
                       if m.get("role") == "user" and PLANNED_MARK in text(m.get("content"))), default=None)
        key = text(messages[planned].get("content")) if planned is not None else None
        if not turns or turns[-1]["qid"] != row["query_id"] or (key is not None and turns[-1]["key"] != key):
            turns.append({"qid": row["query_id"], "key": key, "planned": planned, "requests": [], "condensed": 0})
        turns[-1]["requests"].append({
            "messages": messages,
            "schema": len(json.dumps(request.get("tools"), ensure_ascii=False)) if request.get("tools") else 0,
            "prompt_tokens": ((call.get("reply") or {}).get("usage") or {}).get("prompt_tokens"),
        })
    return turns


def shape(turn):
    """What the loop holds before its first round, the rounds the model made with what each call returned, and
    the turn's characters per prompt token. `None` for a turn whose first request shows no planned message."""
    first = turn["requests"][0]["messages"]
    if turn["planned"] is None:
        return None
    planned = sum(chars(m) for m in first[:turn["planned"] + 1])
    after = sum(chars(m) for m in first[turn["planned"] + 1:])
    rounds, previous = [], first
    for request in turn["requests"][1:]:
        messages = request["messages"]
        if messages[:len(previous)] != previous:
            break
        added = messages[len(previous):]
        calls = [m for m in added if m.get("tool_calls")]
        if calls:
            rounds.append({"call_chars": sum(chars(m) for m in calls),
                           "results": [len(text(m.get("content"))) for m in added if m.get("role") == "tool"]})
        previous = messages
    schema = turn["requests"][0]["schema"]
    measured = [(sum(chars(m) for m in r["messages"]) + r["schema"]) / r["prompt_tokens"]
                for r in turn["requests"] if r["prompt_tokens"]]
    rate = measured[-1] if measured else 4.0
    return {"planned": planned, "after": after, "schema": schema, "rounds": rounds, "rate": rate}


def reach(turn, window_tokens):
    """How the turn's rounds end at `window_tokens`: condensed at a round, the window full at a round, out of
    rounds, or answered."""
    budget = input_budget(window_tokens)
    window = int(budget * turn["rate"] * UNDER_FILL)
    in_flight = min(turn["planned"], int(budget * turn["rate"])) + turn["after"] + turn["schema"]
    results = 0
    for round_ in range(MAX_TOOL_ROUNDS):
        last = round_ + 1 >= MAX_TOOL_ROUNDS
        if round_ > 0:
            if not last and results > window * CONDENSE_AT_WINDOW_SHARE:
                return f"condensed at round {round_}", window
            if window - in_flight < MIN_TOOL_RESULT_CHARS:
                return f"window full at round {round_}", window
        if last:
            return "out of rounds", window
        if round_ >= len(turn["rounds"]):
            return "answered", window
        made = turn["rounds"][round_]
        in_flight += made["call_chars"]
        reserved, got = 0, 0
        for index, size in enumerate(made["results"]):
            share = allowance(window, in_flight + reserved, len(made["results"]) - index)
            got += min(size, share)
            reserved += share
        in_flight += got
        results += got
    return "out of rounds", window


def main():
    path, windows = sys.argv[1], [int(w) for w in sys.argv[2:]]
    with open(path, encoding="utf-8") as f:
        turns = turns_of(f)
    shaped = [(t, shape(t)) for t in turns]
    reading = [(t, s) for t, s in shaped if s and s["rounds"]]
    print(f"{len(turns)} turns, {len(reading)} with a tool call, {sum(t['condensed'] for t in turns)} condensings "
          f"recorded")
    tally = {w: {} for w in windows}
    for t, s in reading:
        print(f"\n{t['qid']}: planned {s['planned']} chars, then {s['after']}, tools {s['schema']}, "
              f"{s['rate']:.2f} chars a token; results by round {[sum(r['results']) for r in s['rounds']]}")
        for w in windows:
            outcome, window = reach(s, w)
            tally[w][outcome] = tally[w].get(outcome, 0) + 1
            print(f"  {w:>7} tokens: window {window:>7} chars, condensing past {int(window * CONDENSE_AT_WINDOW_SHARE):>7}"
                  f" -> {outcome}")
    print("\n| Window (tokens) | " + " | ".join(str(w) for w in windows) + " |")
    print("|---|" + "---|" * len(windows))
    for outcome in sorted({o for counts in tally.values() for o in counts}):
        print(f"| {outcome} | " + " | ".join(str(tally[w].get(outcome, 0)) for w in windows) + " |")


if __name__ == "__main__":
    main()
