#!/usr/bin/env python3
"""Score a long-conversation memory run of the shipped desktop path.

`crates/lattice-desktop/examples/shipped_path_eval` plays each item's turns in
one conversation and writes one row per item. Some turns carry a probe: a
question whose answer depends on something said earlier. This script checks a
dataset's probes against its own turns, and grades a run's answers mechanically
(required facts present, forbidden claims absent, abstention where the
conversation never said). Where the run's request log is beside the rows, each
probe also records which messages of the answering request held the fact.

Commands:
  validate DATASET                 check a dataset's probes against its turns
  score DATASET ROWS OUT_JSONL     grade a run, one JSON line per probe
"""
import argparse
import json
import re
import sys
from pathlib import Path

from shipped_path_eval import QUESTION_WORDS, abstained, contains, declines, load_jsonl, words_of

KINDS = ["early_fact", "corrected_fact", "list_state", "pasted_detail", "constraint",
         "preference", "absent", "assistant_detail"]
PROBE_FIELDS = {"kind", "source_turns", "must_mention", "must_not_claim", "abstain", "by_hand", "notes"}
LABEL_CHARS = 80
SHOWN_ANSWER_CHARS = 300
SHOWN_ERROR_CHARS = 120
NEVER_SAID_RE = re.compile(
    r"\b(?:never|haven['’]t|have not|didn['’]t|did not)\s+"
    r"(?:told|tell|mention(?:ed)?|say|said|share(?:d)?|give|given|specif(?:y|ied)|name(?:d)?)\b",
    re.IGNORECASE,
)


def is_int(value):
    return isinstance(value, int) and not isinstance(value, bool)


def is_text_list(value):
    return isinstance(value, list) and all(isinstance(entry, str) for entry in value)


def probe_shape_problems(where, probe):
    problems = []
    if not isinstance(probe, dict):
        return [f"{where}: probe is not an object"]
    if set(probe) != PROBE_FIELDS:
        problems.append(f"{where}: probe fields are {sorted(probe)}, not {sorted(PROBE_FIELDS)}")
        return problems
    if probe["kind"] not in KINDS:
        problems.append(f"{where}: kind {probe['kind']!r} is not one of {KINDS}")
    if not isinstance(probe["source_turns"], list) or not all(is_int(n) for n in probe["source_turns"]):
        problems.append(f"{where}: source_turns is not a list of integers")
    groups = probe["must_mention"]
    if not isinstance(groups, list) or not all(isinstance(g, list) for g in groups):
        problems.append(f"{where}: must_mention is not a list of lists")
    if not is_text_list(probe["must_not_claim"]):
        problems.append(f"{where}: must_not_claim is not a list of strings")
    for field in ("abstain", "by_hand"):
        if not isinstance(probe[field], bool):
            problems.append(f"{where}: {field} is not a boolean")
    if not isinstance(probe["notes"], str) or not probe["notes"].strip():
        problems.append(f"{where}: notes is empty")
    return problems


def probe_meaning_problems(where, probe, asks, number):
    problems = []
    absent = probe["kind"] == "absent"
    sources = probe["source_turns"]
    for n in sources:
        if not 1 <= n < number:
            problems.append(f"{where}: source turn {n} is not before the probe's own turn {number}")
    if bool(sources) != (not absent):
        problems.append(f"{where}: source_turns must be empty exactly for kind absent")
    if probe["abstain"] != absent:
        problems.append(f"{where}: abstain must be true exactly for kind absent")
    groups = probe["must_mention"]
    if probe["abstain"] and groups:
        problems.append(f"{where}: an abstaining probe has no must_mention")
    if probe["by_hand"] and groups:
        problems.append(f"{where}: a by_hand probe has no must_mention")
    if probe["abstain"] or probe["by_hand"]:
        return problems
    if not groups:
        problems.append(f"{where}: must_mention is empty")
    well_formed = [g for g in groups if g and all(isinstance(a, str) and a.strip() for a in g)]
    if len(well_formed) != len(groups):
        problems.append(f"{where}: every must_mention group is a nonempty list of nonempty strings")
    valid_sources = [n for n in sources if 1 <= n < number]
    if not valid_sources or len(valid_sources) != len(sources):
        return problems
    source_text = " ".join(asks[n - 1] for n in valid_sources)
    later = asks[max(valid_sources):number]
    for group in well_formed:
        if not any(contains(source_text, option) for option in group):
            problems.append(f"{where}: none of {group} occurs in source turns {valid_sources}")
        for option in group:
            if any(contains(ask, option) for ask in later):
                problems.append(f"{where}: {option!r} is restated by the user before the probe")
    return problems


def validate_dataset(dataset):
    """Problems with a dataset; empty when it is sound."""
    items = dataset.get("items") if isinstance(dataset, dict) else None
    if not isinstance(items, list) or not items:
        return ["dataset has no items"]
    problems = []
    seen = set()
    for position, item in enumerate(items, 1):
        item_id = item.get("id") if isinstance(item, dict) else None
        if not isinstance(item_id, str) or not item_id.strip():
            problems.append(f"item {position}: id is not a nonempty string")
            item_id = f"item {position}"
        elif item_id in seen:
            problems.append(f"{item_id}: duplicate id")
        seen.add(item_id)
        turns = item.get("turns") if isinstance(item, dict) else None
        if not isinstance(turns, list) or len(turns) < 2:
            problems.append(f"{item_id}: needs at least two turns")
            continue
        asks = []
        for number, turn in enumerate(turns, 1):
            ask = turn.get("ask") if isinstance(turn, dict) else None
            if not isinstance(ask, str) or not ask.strip():
                problems.append(f"{item_id} turn {number}: ask is not a nonempty string")
                ask = ""
            asks.append(ask)
            if isinstance(turn, dict) and set(turn) - {"ask", "probe"}:
                problems.append(f"{item_id} turn {number}: unknown keys {sorted(set(turn) - {'ask', 'probe'})}")
        for number, turn in enumerate(turns, 1):
            if not isinstance(turn, dict) or "probe" not in turn:
                continue
            where = f"{item_id} turn {number}"
            shape = probe_shape_problems(where, turn["probe"])
            problems.extend(shape)
            if not shape:
                problems.extend(probe_meaning_problems(where, turn["probe"], asks, number))
    return problems


def load_calls(path):
    """(query_id, seq) -> request, from a run's `.calls.jsonl`; empty when absent."""
    path = Path(path)
    if not path.exists():
        return {}
    calls = {}
    for entry in load_jsonl(path):
        call = entry.get("call") or {}
        if "seq" in call and "request" in call:
            calls[(entry.get("query_id"), call["seq"])] = call["request"]
    return calls


def message_text(message):
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(part.get("text") or "" for part in content
                         if isinstance(part, dict) and part.get("type") == "text")
    return ""


def message_label(text, group):
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if any(contains(line, option) for option in group):
            for earlier in reversed(lines[:index + 1]):
                if earlier.strip().startswith("["):
                    return earlier.strip()[:LABEL_CHARS]
            break
    return text[:LABEL_CHARS].replace("\r", " ").replace("\n", " ")


def found_in(request, groups):
    """For each group, the messages of the answering request that hold any alternative."""
    result = []
    for group in groups:
        holders = []
        for index, message in enumerate(request.get("messages") or []):
            text = message_text(message)
            if any(contains(text, option) for option in group):
                holders.append({"index": index, "role": message.get("role"),
                                "label": message_label(text, group)})
        result.append({"group": group, "messages": holders})
    return result


def answer_seq(turn):
    seqs = [call.get("seq") for call in turn.get("model_calls") or [] if call.get("purpose") == "answer"]
    return seqs[-1] if seqs else None


def never_said(answer, question):
    """Whether a sentence says the user never told what the question asks about."""
    asked = words_of(question or "") - QUESTION_WORDS
    return any(
        NEVER_SAID_RE.search(sentence) and asked & words_of(sentence)
        for sentence in re.split(r"(?<=[.!?])\s+", answer or "")
    )


def grade(probe, ask, turn):
    answer = turn.get("answer") or ""
    groups = probe["must_mention"]
    missing = [g for g in groups if not any(contains(answer, option) for option in g)]
    forbidden = [claim for claim in probe["must_not_claim"] if contains(answer, claim)]
    did_abstain = abstained(answer) or never_said(answer, ask)
    if probe["abstain"]:
        ok = did_abstain or declines(answer, ask)
    else:
        ok = not did_abstain or (bool(groups) and not missing)
    passed = turn.get("error") is None and bool(answer.strip()) and not missing and not forbidden and ok
    return {"passed": passed, "missing": missing, "forbidden": forbidden, "abstained": did_abstain}


def probe_record(item, number, row, calls):
    probe = item["turns"][number - 1]["probe"]
    ask = item["turns"][number - 1]["ask"]
    sources = probe["source_turns"]
    record = {
        "query_id": item["id"], "turn": number, "kind": probe["kind"], "question": ask,
        "notes": probe["notes"], "source_turns": sources,
        "depth": number - max(sources) if sources else None,
        "reached": False, "answer": None, "error": None, "answered_ms": None,
        "passed": None, "missing": None, "forbidden": None, "abstained": None,
        "answer_call_seq": None, "found_in": None, "prompt_messages": None,
    }
    turn = next((t for t in (row or {}).get("turns") or [] if t.get("index") == number - 1), None)
    if turn is None:
        record["error"] = "no row" if row is None else (row.get("error") or "turn not run")
        return record
    record.update(reached=True, answer=turn.get("answer"), error=turn.get("error"),
                  answered_ms=turn.get("answered_ms"))
    if not probe["by_hand"]:
        record.update(grade(probe, ask, turn))
    seq = answer_seq(turn)
    record["answer_call_seq"] = seq
    request = calls.get((item["id"], seq)) if seq is not None else None
    if request is not None:
        record["prompt_messages"] = len(request.get("messages") or [])
        if probe["must_mention"]:
            record["found_in"] = found_in(request, probe["must_mention"])
    return record


def score_run(dataset, rows, calls):
    """One record per probe, in dataset order."""
    by_id = {row.get("query_id"): row for row in rows}
    records = []
    for item in dataset["items"]:
        for number, turn in enumerate(item["turns"], 1):
            if "probe" in turn:
                records.append(probe_record(item, number, by_id.get(item["id"]), calls))
    return records


def shown(text, limit):
    return (text or "").replace("\n", " ")[:limit]


def tally(records):
    graded = [r for r in records if r["reached"] and r["passed"] is not None]
    return sum(bool(r["passed"]) for r in graded), len(graded)


def report(records, rows):
    models = list(dict.fromkeys(row.get("model") for row in rows if row.get("model")))
    reached = [r for r in records if r["reached"]]
    graded = [r for r in reached if r["passed"] is not None]
    lines = [f"model: {', '.join(models)}",
             f"probes: {len(records)}, reached: {len(reached)}, graded: {len(graded)}, "
             f"passed: {sum(bool(r['passed']) for r in graded)}"]
    for kind in KINDS:
        of_kind = [r for r in records if r["kind"] == kind]
        if not of_kind:
            continue
        passed, total = tally(of_kind)
        by_hand = sum(1 for r in of_kind if r["reached"] and r["passed"] is None)
        unreached = sum(1 for r in of_kind if not r["reached"])
        lines.append(f"{kind}: {passed}/{total} (by hand: {by_hand}, not reached: {unreached})")
    by_id = {row.get("query_id"): row for row in rows}
    for item_id in dict.fromkeys(r["query_id"] for r in records):
        mine = [r for r in records if r["query_id"] == item_id]
        passed, total = tally(mine)
        line = f"{item_id}: {passed}/{total}"
        row = by_id.get(item_id)
        stopped = next((r for r in mine if not r["reached"]), None)
        if row is None or row.get("error") or stopped:
            turns = (row or {}).get("turns") or []
            at = (len(turns) if turns and turns[-1].get("error") else len(turns) + 1)
            error = (row or {}).get("error") or (stopped or {}).get("error") or "no row"
            line += f", stopped at turn {at}: {shown(error, SHOWN_ERROR_CHARS)}"
        lines.append(line)
    for r in graded:
        if not r["passed"]:
            lines.append(f"{r['query_id']} T{r['turn']} {r['kind']} missing={r['missing']} "
                         f"forbidden={r['forbidden']} abstained={r['abstained']}")
            lines.append("    " + shown(r["answer"], SHOWN_ANSWER_CHARS))
    for r in reached:
        if r["passed"] is None:
            lines.append(f"{r['query_id']} T{r['turn']} {r['kind']} (by hand): {r['notes']}")
            lines.append("    " + shown(r["answer"], SHOWN_ANSWER_CHARS))
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("validate")
    check.add_argument("dataset", type=Path)
    run = sub.add_parser("score")
    run.add_argument("dataset", type=Path)
    run.add_argument("rows", type=Path)
    run.add_argument("output", type=Path)
    args = parser.parse_args(argv)

    dataset = json.loads(args.dataset.read_text(encoding="utf-8"))
    if args.command == "validate":
        problems = validate_dataset(dataset)
        if problems:
            print("\n".join(problems))
            return 1
        items = dataset["items"]
        turns = sum(len(item["turns"]) for item in items)
        probes = sum("probe" in turn for item in items for turn in item["turns"])
        print(f"ok: {len(items)} items, {turns} turns, {probes} probes")
        return 0

    rows = load_jsonl(args.rows)
    calls = load_calls(str(args.rows) + ".calls.jsonl")
    records = score_run(dataset, rows, calls)
    with args.output.open("w", encoding="utf-8", newline="\n") as out:
        for record in records:
            out.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(report(records, rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
