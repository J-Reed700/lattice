"""Offline A/B of the memory extractor's request (conversation/memory/extract.rs, prompts.rs).

The release baseline in tests/conversation_memory_evals.rs takes about two hours on a remote server. This asks the question that
failed it directly: is a window holding the user's rule answered with that rule, or with `add: []`? It replays each family's
first extraction window, rendered as `render_extractor_prompt` renders it and sent as the app sends it, under each arm. A
window's message ids depend only on the family and the seed, so every arm sees the same prompt but for its own change.

An answer is `hit` for each text the family requires that an item's evidence quotes, and `empty` when `add` is empty.

Arms:
  with_sentence     the extractor's prompt holding "Return only JSON matching the requested schema..."
  without_sentence  the same prompt without it
  repeat_1.1        without the sentence, repeat_penalty 1.1 as before #43
  effort_none       without the sentence, reasoning_effort "none" as before #43
  bounded_label     without the sentence, the label's maxLength 200 as before #301

  python -X utf8 -I -u scripts/replay_memory_extract_ab.py --out OUT [--llama URL | --claude MODEL] [--arms A,B] [--families F,G]
  python -X utf8 -I scripts/replay_memory_extract_ab.py --summary OUT
"""
import argparse
import json
import os
import re
import sys
import time
import urllib.request
import uuid
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates/lattice-desktop/tests/fixtures/conversation_memory"
PROMPTS = ROOT / "crates/lattice-desktop/src/features/conversation/memory/prompts.rs"
SEGMENT_MAX_BYTES = 6 * 1024
OVERLAP_CHARS = 256
MIN_SEGMENT_DIVISOR = 4
OUTPUT_TOKENS = 8192
SENTENCE = "Return only JSON matching the requested schema, with no commentary, explanation, or markdown outside it."
BEFORE_SENTENCE = "Never classify an action request as a decision."
# How many segments the release baseline's first extraction batch holds, read from its traces.
FIRST_WINDOW = {
    "early_restriction": 8,
    "topic_return": 10,
    "assistant_hallucination": 6,
    "quoted_adversarial_text": 4,
    "long_user_messages": 5,
    "missing_fact": 6,
    "storage_lifecycle": 6,
    "utility_failure": 6,
    "multi_turn_tool_work": 6,
    "partial_revocation": 8,
    "exact_identifiers": 6,
    "narrowed_constraint": 6,
}
ARMS = {
    "with_sentence": {"_sentence": True},
    "without_sentence": {"_sentence": False},
    "repeat_1.1": {"_sentence": False, "repeat_penalty": 1.1},
    "effort_none": {"_sentence": False, "reasoning_effort": "none"},
    "bounded_label": {"_sentence": False, "_bounded_label": True},
}


def extractor_system(path=PROMPTS):
    source = Path(path).read_text("utf-8")
    literal = re.search(r'pub const EXTRACTOR_SYSTEM: &str = "(.*?)";\r?\n', source, re.S).group(1)
    return literal.replace('\\"', '"').replace("\\n", "\n")


def with_sentence(system):
    if SENTENCE in system:
        return system
    assert system.count(BEFORE_SENTENCE) == 1
    return system.replace(BEFORE_SENTENCE, f"{BEFORE_SENTENCE} {SENTENCE}")


def without_sentence(system):
    return system.replace(f" {SENTENCE}", "")


def sorted_keys(value):
    if isinstance(value, dict):
        return {key: sorted_keys(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return [sorted_keys(item) for item in value]
    return value


def evidence(purposes):
    return {
        "type": "array",
        "maxItems": 4,
        "items": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "message_id": {"type": "string"},
                "quote": {"type": "string"},
                "occurrence": {"type": ["integer", "null"], "minimum": 0},
                "purpose": {"type": "string", "enum": purposes},
            },
            "required": ["message_id", "quote", "occurrence", "purpose"],
        },
    }


SCHEMA = sorted_keys({
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "schema_version": {"type": "integer", "enum": [1]},
        "add": {
            "type": "array",
            "maxItems": 32,
            "items": {
                "type": "object",
                "additionalProperties": False,
                "properties": {
                    "candidate_id": {"type": "string"},
                    "kind": {
                        "type": "string",
                        "enum": ["constraint", "goal", "decision", "user_fact", "preference", "open_question", "established_fact"],
                    },
                    "label": {"type": "string"},
                    "evidence": evidence(["assertion", "antecedent"]),
                },
                "required": ["candidate_id", "kind", "label", "evidence"],
            },
        },
        "transitions": {
            "type": "array",
            "maxItems": 32,
            "items": {
                "type": "object",
                "additionalProperties": False,
                "properties": {
                    "item_id": {"type": "string", "description": "Existing item_id or candidate_id of an addition in this response"},
                    "state": {"type": "string", "enum": ["superseded", "resolved"]},
                    "replacement_candidate_id": {"type": ["string", "null"]},
                    "evidence": evidence(["transition"]),
                },
                "required": ["item_id", "state", "replacement_candidate_id", "evidence"],
            },
        },
        "processed_segment_ids": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["schema_version", "add", "transitions", "processed_segment_ids"],
})


def bounded_label_schema():
    schema = json.loads(json.dumps(SCHEMA))
    schema["properties"]["add"]["items"]["properties"]["label"] = {"maxLength": 200, "type": "string"}
    return schema


def next_cut(content, start, max_bytes):
    """segment.rs's next_cut, on bytes."""
    hard = start + max_bytes
    if hard >= len(content):
        return len(content)
    window = content[start:hard]
    floor = min(max_bytes // MIN_SEGMENT_DIVISOR, len(window))
    for delimiters in ((b"\n\n", b"\n"), (b". ", b".\n", b"! ", b"? ")):
        for delimiter in delimiters:
            found = window.rfind(delimiter)
            if found >= 0:
                cut = found + len(delimiter)
                if cut >= floor and cut > 0:
                    return start + cut
    return hard


def segments_of(message):
    content = message["content"].encode("utf-8")
    if len(content) <= SEGMENT_MAX_BYTES:
        return [{"id": f"{message['id']}:s0", "message": message, "text": message["content"], "before": "", "after": "", "whole": True}]
    out = []
    start = 0
    while start < len(content):
        end = next_cut(content, start, SEGMENT_MAX_BYTES)
        out.append({
            "id": f"{message['id']}:s{len(out)}",
            "message": message,
            "text": content[start:end].decode("utf-8"),
            "before": content[:start].decode("utf-8")[-OVERLAP_CHARS:],
            "after": content[end:].decode("utf-8")[:OVERLAP_CHARS],
            "whole": False,
        })
        start = end
    return out


def window(family, seed):
    """The family's first extraction window, with message ids fixed by family and seed, and what it requires."""
    fixture = json.loads((FIXTURES / f"{family}.json").read_text("utf-8"))
    messages = [
        {"id": str(uuid.uuid5(uuid.NAMESPACE_URL, f"{family}/{seed}/{index}")), "sequence": index + 1, "role": turn["role"], "content": turn["content"]}
        for index, turn in enumerate(fixture["turns"])
    ]
    segments = []
    for message in messages:
        if len(segments) >= FIRST_WINDOW[family]:
            break
        segments.extend(segments_of(message))
    return segments[: FIRST_WINDOW[family]], fixture["expect"]


def user_prompt(segments):
    rendered = [
        {
            "segment_id": s["id"],
            "message_id": s["message"]["id"],
            "sequence": s["message"]["sequence"],
            "role": s["message"]["role"],
            "whole_message": s["whole"],
            "text": s["text"],
            "context_before": s["before"],
            "context_after": s["after"],
        }
        for s in segments
    ]
    payload = {
        "task": "extract_memory",
        "schema_version": 1,
        "segments": rendered,
        "processed_segment_ids_must_equal": [s["id"] for s in segments],
        "context_passages": [],
        "existing_items": [],
        "rules": {
            "max_operations": 32,
            "max_evidence_per_item": 4,
            "max_quote_bytes": 2048,
            "quote_must_be_exact_substring_of_its_message": True,
            "evidence_message_id_uses_message_id_not_segment_id": True,
            "every_item_needs_at_least_one_user_assertion": True,
            "context_passages_are_context_only_report_segments_only": True,
            "unresolved_change_is_host_owned_and_must_not_be_proposed": True,
            "transition_item_id_may_reference_same_response_candidate_id": True,
            "same_subject_later_updates_require_candidate_transition": True,
        },
    }
    return json.dumps(sorted_keys(payload), separators=(",", ":"), ensure_ascii=False)


def required_quotes(expect):
    quotes = list(expect.get("required_active_quotes") or [])
    for group in expect.get("required_active_quote_any") or []:
        quotes.extend(group)
    return quotes


def judged(raw, expect, segments):
    try:
        reply = json.loads(raw)
    except ValueError:
        return {"unparsed": True}
    added = reply.get("add", [])
    wanted = [q for q in required_quotes(expect) if any(q.lower() in s["text"].lower() for s in segments)]
    said = " \n ".join(e.get("quote", "") for item in added for e in item.get("evidence", [])).lower()
    return {"added": len(added), "wanted": len(wanted), "hit": sum(q.lower() in said for q in wanted), "kinds": [item.get("kind") for item in added]}


def llama_body(model, system, user, arm):
    body = {
        "model": model,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "stream": False,
        "temperature": 0.0,
        "top_p": 1.0,
        "top_k": 1,
        "presence_penalty": 0.0,
        "repeat_penalty": 1.0,
        "max_tokens": OUTPUT_TOKENS,
        "chat_template_kwargs": {"enable_thinking": False},
        "response_format": {"type": "json_schema", "json_schema": {"name": "response", "schema": bounded_label_schema() if arm.get("_bounded_label") else SCHEMA}},
    }
    body.update({key: value for key, value in arm.items() if not key.startswith("_")})
    return body


def authorised(request):
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        request.add_header(name, value)
    return request


def ask_llama(url, body):
    request = authorised(urllib.request.Request(url.rstrip("/") + "/v1/chat/completions", json.dumps(body).encode("utf-8"), {"Content-Type": "application/json"}))
    with urllib.request.urlopen(request, timeout=900) as response:
        reply = json.load(response)
    return reply["choices"][0]["message"]["content"], reply.get("usage", {}).get("completion_tokens")


def run(args):
    if args.claude:
        import replay_claude as claude

        claude.use(args.claude, args.out + ".spend.jsonl", args.budget)
        model = args.claude
    else:
        with urllib.request.urlopen(authorised(urllib.request.Request(args.llama.rstrip("/") + "/v1/models")), timeout=60) as response:
            model = json.load(response)["data"][0]["id"]
    print(model, time.strftime("%H:%M"), flush=True)
    system = extractor_system()
    out = Path(args.out)
    done = set()
    if out.exists():
        done = {(r["arm"], r["family"], r["seed"]) for r in map(json.loads, out.read_text("utf-8").splitlines())}
    with out.open("a", encoding="utf-8") as sink:
        for seed in range(1, args.seeds + 1):
            for family in args.families.split(","):
                segments, expect = window(family, seed)
                user = user_prompt(segments)
                for name in args.arms.split(","):
                    if (name, family, seed) in done:
                        continue
                    arm = ARMS[name]
                    prompt = with_sentence(system) if arm["_sentence"] else without_sentence(system)
                    body = llama_body(model, prompt, user, arm)
                    if args.claude:
                        answer = claude.call(body, "extract")
                        raw, tokens = answer["content"], answer["usage"]["completion_tokens"]
                    else:
                        raw, tokens = ask_llama(args.llama, body)
                    sink.write(json.dumps({"model": model, "arm": name, "family": family, "seed": seed, "tokens": tokens, "result": judged(raw, expect, segments), "raw": raw}, ensure_ascii=False) + "\n")
                    sink.flush()
            print(f"seed {seed} done {time.strftime('%H:%M')}", flush=True)
    if args.claude:
        claude.print_spend(args.out + ".spend.jsonl")


def summary(path):
    rows = [json.loads(line) for line in Path(path).read_text("utf-8").splitlines() if line.strip()]
    print("model", sorted({row["model"] for row in rows}))
    totals = defaultdict(Counter)
    per = defaultdict(lambda: defaultdict(Counter))
    for row in rows:
        result = row["result"]
        arm = row["arm"]
        totals[arm]["answers"] += 1
        if result.get("unparsed"):
            totals[arm]["unparsed"] += 1
            continue
        totals[arm]["empty"] += result["added"] == 0
        totals[arm]["wanted quotes"] += result["wanted"]
        totals[arm]["quotes hit"] += result["hit"]
        for name, value in (("answers", 1), ("empty", result["added"] == 0), ("hit", result["hit"]), ("wanted", result["wanted"])):
            per[row["family"]][arm][name] += value
    for arm, counts in totals.items():
        print(arm, dict(counts))
    for family, arms in per.items():
        print(family, {arm: f"{c['empty']}/{c['answers']} empty, {c['hit']}/{c['wanted']} quotes" for arm, c in arms.items()})


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out")
    ap.add_argument("--summary")
    ap.add_argument("--seeds", type=int, default=6)
    ap.add_argument("--arms", default="with_sentence,without_sentence")
    ap.add_argument("--families", default=",".join(FIRST_WINDOW))
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    ap.add_argument("--claude", help="ask this Claude model, as cloud.rs sends it, instead of the llama-server")
    ap.add_argument("--budget", type=float, default=1.0)
    args = ap.parse_args()
    if args.summary:
        summary(args.summary)
    elif args.out:
        run(args)
    else:
        sys.exit("give --out or --summary")


if __name__ == "__main__":
    main()
