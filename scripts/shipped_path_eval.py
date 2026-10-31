#!/usr/bin/env python3
"""Score answers from the shipped desktop chat path.

`crates/lattice-desktop/examples/shipped_path_eval` asks each question of the desktop's own
orchestrator and writes one row per item. This script grades those rows two
ways and reports both, so each can be checked against the other:

* mechanical checks against the item's rubric: required facts present
  (case-insensitive substring, any listed alternative), forbidden claims
  absent, abstention matching what the corpus supports, citation numbers that
  exist, and the expected documents among those cited and among the evidence
  the model was given;
* a rubric judge: a model behind `--endpoint` grades each answered turn once
  with a fixed prompt, greedy sampling and thinking off. Greedy decoding gives
  the same grade for the same input, so a repeated grade is the same verdict
  again, not a second trial: the 2026-10-13 runs graded every turn three times
  and no turn ever split. One call is therefore one verdict.

The judge's endpoint and model are independent of the model that wrote the
answers, and the scored manifest records which model judged. A judge grading
its own answers is the weakest setup: its verdict is a smoke-level signal until
`shipped_path_stats.py calibration-score` has measured it against human labels.
Disagreements between the judge and the mechanical checks are listed in the
report for the same reason.

Commands:
  validate QUESTIONS CORPUS        check a question set against its corpus
  score QUESTIONS ANSWERS OUT      grade a run (add --endpoint to run the judge)
  score-dataset DATASET EXPECTATIONS ANSWERS OUT
                                   grade a run of a labelled dataset with the
                                   answer harness's own regex checks
"""
import argparse
import json
import os
import re
import statistics
import unicodedata
import urllib.error
import urllib.request
from pathlib import Path

from chat_rag_eval import ABSTENTION_RE, CITATION_RE, aggregate, evaluate_answer
from eval_integrity import SCHEMA_VERSION, digest, unique_records, validate_expectations, wilson
from rag_eval import validate_dataset

ROW_SCHEMA = "lattice-shipped-path-answers/1"
JUDGE_RUBRIC = "lattice-shipped-path-judge/1"

# Phrases a desktop answer uses to decline that the answer harness's pattern
# does not list.
EXTRA_ABSTENTION_RE = re.compile(
    r"\b(?:no (?:mention|record|reference|document|entry|passage)|there (?:is|are) no|found no|"
    r"no (?:\w+ ){1,6}appears|"
    r"isn't (?:mentioned|recorded|in)|is not (?:mentioned|recorded|in the)|"
    r"(?:couldn't|could not|cannot|can't) (?:find|identify|tell|give|supply|confirm)|"
    r"none of the (?:documents|sources|notes|passages)|"
    r"not (?:mentioned|recorded|listed|included|stated|given|named)|"
    r"do(?:es)? not (?:contain|appear|mention|state|give|name|list|cover)|"
    r"cannot be determined)\b",
    re.IGNORECASE,
)

# Words too common in a question to say what it asks about.
QUESTION_WORDS = frozenset(
    "what which when where does with that this have from their there would about much "
    "whose whom will should could".split()
)

TURN_FIELDS = {
    "index": int, "question": str, "answer": (str, type(None)), "error": (str, type(None)),
    "answered_ms": int, "evidence": list, "citation_map": dict, "ranked_ids": list,
    "model_calls": list, "prompt_tokens": dict,
}
ROW_FIELDS = {
    "schema": str, "query_id": str, "tags": list, "answer": (str, type(None)),
    "ranked_ids": list, "citation_map": dict, "latency_ms": int, "model": str,
    "error": (str, type(None)), "turns": list,
}

JUDGE_SYSTEM = (
    "You grade one answer from a document assistant against a rubric written by the "
    "person who built the test. The documents are fictional, so use no outside "
    "knowledge: judge only whether the answer meets the rubric. Paraphrases, "
    "equivalent number and date formats, and extra correct detail are fine. Citation "
    "markers such as [1] are not part of the content. Return JSON with verdict "
    '"pass" or "fail" and a one-sentence reason.'
)
JUDGE_SCHEMA = {
    "type": "object",
    "properties": {
        "verdict": {"type": "string", "enum": ["pass", "fail"]},
        "reason": {"type": "string"},
    },
    "required": ["verdict", "reason"],
}


def normalize(text):
    """Casefolded, NFKC, one space, ASCII hyphens, no Markdown emphasis."""
    text = unicodedata.normalize("NFKC", text or "")
    text = re.sub(r"[‐-―−]", "-", text)
    text = re.sub(r"[*_`]", "", text)
    return re.sub(r"\s+", " ", text).casefold().strip()


def contains(haystack, needle):
    return normalize(needle) in normalize(haystack)


def load_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def validate_row(row):
    """Reject a row whose shape the scorer does not understand."""
    for field, kind in ROW_FIELDS.items():
        if field not in row or not isinstance(row[field], kind):
            raise ValueError(f"row {row.get('query_id')!r}: field {field} missing or mistyped")
    if row["schema"] != ROW_SCHEMA:
        raise ValueError(f"row {row['query_id']!r}: schema {row['schema']!r} is not {ROW_SCHEMA}")
    if not row["turns"]:
        raise ValueError(f"row {row['query_id']!r}: no turns")
    for turn in row["turns"]:
        for field, kind in TURN_FIELDS.items():
            if field not in turn or not isinstance(turn[field], kind):
                raise ValueError(f"row {row['query_id']!r} turn: field {field} missing or mistyped")
    return row


def validate_questions(questions, corpus):
    """Problems with a question set; empty when it is sound."""
    problems = []
    documents = {doc["id"]: doc for doc in corpus["documents"]}
    vocabulary = set(questions.get("tags", {}))
    items = questions.get("items")
    if not isinstance(items, list) or not items:
        return ["question set has no items"]
    try:
        unique_records(items, "id")
    except ValueError as error:
        problems.append(str(error))
    for item in items:
        where = item.get("id")
        tags = item.get("tags") or []
        if not tags or not set(tags) <= vocabulary:
            problems.append(f"{where}: tags {tags} are empty or outside the vocabulary")
        turns = item.get("turns") or []
        if not turns:
            problems.append(f"{where}: no turns")
        for number, turn in enumerate(turns, 1):
            rubric = turn.get("rubric")
            if not isinstance(turn.get("ask"), str) or not turn["ask"].strip() or not isinstance(rubric, dict):
                problems.append(f"{where} turn {number}: needs ask and rubric")
                continue
            if not isinstance(rubric.get("notes"), str) or not rubric["notes"].strip():
                problems.append(f"{where} turn {number}: rubric needs notes for the judge")
            expected = rubric.get("expected_citations", [])
            unknown = [doc for doc in expected if doc not in documents]
            if unknown:
                problems.append(f"{where} turn {number}: unknown documents {unknown}")
            groups = rubric.get("must_mention", [])
            if rubric.get("abstain"):
                if groups or expected:
                    problems.append(f"{where} turn {number}: an abstention has no facts or citations")
                for claim in rubric.get("must_not_claim", []):
                    if any(contains(doc["text"], claim) for doc in documents.values()):
                        problems.append(f"{where} turn {number}: {claim!r} occurs in the corpus")
                continue
            if not groups or not expected:
                problems.append(f"{where} turn {number}: an answerable turn needs facts and citations")
            source = " ".join(documents[doc]["text"] for doc in expected if doc in documents)
            for group in groups:
                if not isinstance(group, list) or not group:
                    problems.append(f"{where} turn {number}: must_mention groups are nonempty lists")
                elif not any(contains(source, option) for option in group):
                    problems.append(f"{where} turn {number}: none of {group} occurs in {expected}")
    return problems


def says_unavailable(text):
    """Whether any of `text` says something is unavailable."""
    return ABSTENTION_RE.search(text) or EXTRA_ABSTENTION_RE.search(text)


def words_of(text):
    """The longer words of `text`, lower-cased, hyphenated codes whole."""
    return {word.lower() for word in re.findall(r"[\w-]{4,}", text)}


def declines(answer, question):
    """Whether some sentence says what the question asks about is unavailable.

    An answer can refuse after first saying what it searched, so no one place
    will do; the sentence must share a word with the question, so an aside
    about something else does not pass off a guess as a refusal.
    """
    asked = words_of(question or "") - QUESTION_WORDS
    return any(
        says_unavailable(sentence) and asked & words_of(sentence)
        for sentence in re.split(r"(?<=[.!?])\s+", answer or "")
    )


def abstained(answer):
    """Whether the answer's disposition is that the requested fact is unavailable.

    Missing-information language can also qualify one source, reject a premise,
    or contrast an incorrect option with the supported answer. Treating any such
    phrase anywhere as a whole-answer abstention makes a decisive answer fail
    merely because it includes a useful caveat.
    """
    text = (answer or "").strip()
    if not text:
        return False

    if not says_unavailable(text):
        return False

    first_breaks = [index for mark in ".!?\n" if (index := text.find(mark)) >= 0]
    first_end = min(first_breaks) + 1 if first_breaks else min(len(text), 400)
    lead = text[:first_end]
    lead_match = says_unavailable(lead)
    if lead_match:
        # "There is no X; instead use Y [1]" is a correction, not a refusal.
        rest = text[lead_match.end():]
        correction = re.search(r"\b(?:instead|rather|however)\b", rest, re.I)
        if correction:
            corrected = rest[correction.end():]
            if CITATION_RE.search(corrected) and not says_unavailable(corrected):
                return False
        return True

    sentences = re.split(r"(?<=[.!?])\s+(?=[A-Z])", text)
    return bool(says_unavailable(sentences[-1]))


def mechanical(rubric, turn):
    """The rubric's checkable parts, applied to one turn."""
    answer = turn.get("answer") or ""
    groups = rubric.get("must_mention", [])
    missing = [group for group in groups if not any(contains(answer, option) for option in group)]
    forbidden = [claim for claim in rubric.get("must_not_claim", []) if contains(answer, claim)]
    cited = list(dict.fromkeys(int(n) for n in CITATION_RE.findall(answer)))
    citation_map = turn.get("citation_map") or {}
    invalid = [n for n in cited if str(n) not in citation_map]
    cited_docs = list(dict.fromkeys(citation_map[str(n)] for n in cited if str(n) in citation_map))
    expected = rubric.get("expected_citations", [])
    supplied = {entry.get("document_id") for entry in turn.get("evidence") or []}
    expect_abstain = bool(rubric.get("abstain"))
    did_abstain = abstained(answer)
    # The rubric knows what the phrases cannot: stating every required fact is
    # an answer whatever caveats follow it.
    if expect_abstain:
        abstention_ok = did_abstain or declines(answer, turn.get("question"))
    else:
        abstention_ok = not did_abstain or (bool(groups) and not missing)
    expected_cited = [doc for doc in expected if doc in cited_docs]
    passed = (
        turn.get("error") is None
        and bool(answer.strip())
        and not missing
        and not forbidden
        and abstention_ok
        and not invalid
        and len(expected_cited) == len(expected)
    )
    return {
        "missing_facts": missing,
        "forbidden_claims": forbidden,
        "cited_ids": cited,
        "invalid_citations": invalid,
        "cited_document_ids": cited_docs,
        "expected_cited": expected_cited,
        "expected_in_evidence": [doc for doc in expected if doc in supplied],
        "abstained": did_abstain,
        "expected_abstain": expect_abstain,
        "abstention_ok": abstention_ok,
        "passed": passed,
    }


def rubric_lines(rubric):
    """The rubric as the judge reads it, one requirement per line."""
    if "lines" in rubric:
        return list(rubric["lines"])
    lines = []
    if rubric.get("abstain"):
        lines.append("- The documents do not contain this answer. A correct answer says so and "
                     "does not invent one.")
    else:
        lines.append("- The documents contain the answer. Declining to answer fails.")
        for group in rubric.get("must_mention", []):
            lines.append("- Must state: " + " (or) ".join(json.dumps(option) for option in group))
    for claim in rubric.get("must_not_claim", []):
        lines.append(f"- Must not claim: {json.dumps(claim)}")
    lines.append(f"- What a correct answer says: {rubric['notes']}")
    return lines


def judge_messages(item, turn_number, answers):
    """The fixed judge prompt for one turn; earlier turns are context only."""
    turns = item["turns"]
    lines = []
    if turn_number:
        lines.append("Earlier in the conversation (context only, not graded):")
        for earlier in range(turn_number):
            lines.append(f"Q{earlier + 1}: {turns[earlier]['ask']}")
            lines.append(f"A{earlier + 1}: {answers[earlier] or '(no answer)'}")
        lines.append("")
    lines.append(f"Question being graded: {turns[turn_number]['ask']}")
    lines.append("")
    lines.append("Rubric:")
    lines.extend(rubric_lines(turns[turn_number]["rubric"]))
    lines.append("")
    lines.append("Answer to grade:")
    lines.append("<<<")
    lines.append(answers[turn_number] or "(no answer)")
    lines.append(">>>")
    return [{"role": "system", "content": JUDGE_SYSTEM},
            {"role": "user", "content": "\n".join(lines)}]


def judge_request(model, messages):
    """Greedy, thinking off, schema-bound: the same input gives the same grade."""
    return {
        "model": model,
        "messages": messages,
        "stream": False,
        "temperature": 0,
        "top_p": 1,
        "top_k": 1,
        "seed": 2026,
        "max_tokens": 300,
        "chat_template_kwargs": {"enable_thinking": False},
        "response_format": {"type": "json_schema",
                            "json_schema": {"name": "verdict", "schema": JUDGE_SCHEMA}},
    }


def parse_verdict(payload):
    content = payload["choices"][0]["message"]["content"]
    verdict = json.loads(re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL).strip())
    if verdict.get("verdict") not in ("pass", "fail"):
        raise ValueError(f"judge returned no verdict: {content!r}")
    return verdict["verdict"] == "pass", str(verdict.get("reason", ""))


# A judge server that wants a key reads it from here as `Header-Name: value`.
# It is only ever sent to the judge: never printed, logged or written to a result.
JUDGE_AUTH_ENV = "LATTICE_JUDGE_AUTH"


def judge_headers():
    headers = {"Content-Type": "application/json"}
    auth = os.environ.get(JUDGE_AUTH_ENV, "")
    if auth:
        name, separator, value = auth.partition(":")
        if not separator or not name.strip() or not value.strip():
            raise SystemExit(f"{JUDGE_AUTH_ENV} must read `Header-Name: value`")
        headers[name.strip()] = value.strip()
    return headers


ANTHROPIC_API = "https://api.anthropic.com"
ANTHROPIC_VERSION = "2023-06-01"


def anthropic_headers():
    key = os.environ.get("ANTHROPIC_API_KEY", "").strip()
    if not key:
        raise SystemExit("an Anthropic judge needs ANTHROPIC_API_KEY")
    return {"Content-Type": "application/json", "x-api-key": key,
            "anthropic-version": ANTHROPIC_VERSION}


def anthropic_judge_request(model, messages):
    """The judge request in the Messages API's shape. Claude rejects the
    sampling knobs greedy grading uses, so none is sent: a Claude judge is
    not deterministic, and its manifest says so. The verdict is bound to the
    same schema through structured output."""
    system = "\n\n".join(m["content"] for m in messages if m.get("role") == "system")
    turns = [{"role": m["role"], "content": m["content"]}
             for m in messages if m.get("role") in ("user", "assistant")]
    request = {
        "model": model,
        "max_tokens": 4096,
        "messages": turns,
        "output_config": {"effort": "low",
                          "format": {"type": "json_schema",
                                     "schema": JUDGE_SCHEMA | {"additionalProperties": False}}},
    }
    if system:
        request["system"] = system
    return request


def parse_anthropic_verdict(payload):
    content = "".join(block.get("text", "") for block in payload.get("content") or []
                      if block.get("type") == "text")
    verdict = json.loads(content.strip())
    if verdict.get("verdict") not in ("pass", "fail"):
        raise ValueError(f"judge returned no verdict: {content!r}")
    return verdict["verdict"] == "pass", str(verdict.get("reason", ""))


def anthropic_judge(model):
    """Grades one turn with a Claude model through the Messages API. No time
    limit cuts a grade off, as for any judge."""
    url = ANTHROPIC_API + "/v1/messages"
    headers = anthropic_headers()

    def call(messages):
        body = json.dumps(anthropic_judge_request(model, messages)).encode()
        request = urllib.request.Request(url, data=body, method="POST", headers=headers)
        with urllib.request.urlopen(request) as response:
            return parse_anthropic_verdict(json.load(response))
    return call


def http_judge(endpoint, model):
    """Grades one turn. A grade takes as long as the judge's server takes: a
    shared remote may queue it behind other work, so no time limit cuts it off."""
    url = endpoint.rstrip("/") + "/v1/chat/completions"
    headers = judge_headers()

    def call(messages):
        body = json.dumps(judge_request(model, messages)).encode()
        request = urllib.request.Request(url, data=body, method="POST", headers=headers)
        with urllib.request.urlopen(request) as response:
            return parse_verdict(json.load(response))
    return call


def answering_model(run_meta):
    """The model that wrote a run's answers, and the revision it was measured
    at: a local model's weights hash, or the names a cloud model's replies
    were served under. Older runs carry only the recipe's server."""
    answering = run_meta.get("answering_model")
    if answering:
        served = answering.get("served_as") or []
        return answering.get("model"), ",".join(served) or None
    return (run_meta.get("recipe", {}).get("server", {}).get("model"),
            run_meta.get("gguf_sha256"))


def judge_identity(endpoint, model, answer_model, timeout=30, headers=None):
    """Which model judged: the request's model name and what the server lists.

    `/v1/models` is the OpenAI-compatible listing every supported server
    answers; when it does not, the identity says so rather than guessing. The
    endpoint URL itself is not recorded, because a remote judge's address is
    not something a committed result should carry.
    """
    served = None
    try:
        listing_request = urllib.request.Request(endpoint.rstrip("/") + "/v1/models",
                                                 headers=headers or judge_headers())
        with urllib.request.urlopen(listing_request, timeout=timeout) as response:
            listing = json.load(response)
        served = [{"id": entry.get("id"), "meta": entry.get("meta")}
                  for entry in listing.get("data") or [] if isinstance(entry, dict)]
    except (urllib.error.URLError, OSError, ValueError) as error:
        served = {"unavailable": str(error)}
    served_ids = {entry["id"] for entry in served} if isinstance(served, list) else set()
    known = {model} | served_ids
    return {
        "requested_model": model,
        "served_models": served,
        "answer_model": answer_model,
        "judges_own_answers": (answer_model in known) if answer_model and served_ids else None,
    }


def score(questions, rows, judge=None):
    """Scored rows, one per item, in question-set order."""
    items = unique_records(questions["items"], "id")
    by_id = unique_records([validate_row(row) for row in rows], "query_id")
    unknown = set(by_id) - set(items)
    if unknown:
        raise ValueError(f"rows for items the question set does not have: {sorted(unknown)}")
    scored = []
    for item_id, item in items.items():
        row = by_id.get(item_id)
        if row is None:
            continue
        answers = [turn.get("answer") for turn in row["turns"]]
        turn_scores = []
        for number, spec in enumerate(item["turns"]):
            turn = row["turns"][number] if number < len(row["turns"]) else {"answer": None, "error": "turn not run"}
            checks = mechanical(spec["rubric"], turn)
            result = {"turn": number, "mechanical": checks}
            if judge is not None:
                graded = bool(turn.get("answer"))
                if graded:
                    passed, reason = judge(judge_messages(item, number, answers + [None] * len(item["turns"])))
                else:
                    passed, reason = False, "no answer to grade"
                result["judge"] = {"pass": passed, "reason": reason, "graded": graded}
            turn_scores.append(result)
        last = row["turns"][-1]
        mechanical_pass = all(t["mechanical"]["passed"] for t in turn_scores)
        record = {
            "query_id": item_id,
            "tags": item["tags"],
            "dimensions": item["tags"],
            "answer": row["answer"],
            "answer_sha256": digest(row["answer"]),
            "ranked_ids": row["ranked_ids"],
            "citation_map": row["citation_map"],
            "cited_ids": turn_scores[-1]["mechanical"]["cited_ids"],
            "cited_document_ids": turn_scores[-1]["mechanical"]["cited_document_ids"],
            "invalid_citations": turn_scores[-1]["mechanical"]["invalid_citations"],
            "abstained": turn_scores[-1]["mechanical"]["abstained"],
            "expected_abstain": turn_scores[-1]["mechanical"]["expected_abstain"],
            "mechanical_checks_passed": mechanical_pass,
            "answer_correct": None,
            "review_status": "unreviewed",
            "latency_ms": row["latency_ms"],
            "model": row["model"],
            "error": row["error"],
            "turn_scores": turn_scores,
            "observed": [observed(turn) for turn in row["turns"]],
            "verification": [verdict_counts(turn) for turn in row["turns"]],
            "timing": [turn_timing(turn) for turn in row["turns"]],
            "last_turn_evidence_docs": [e.get("document_id") for e in last.get("evidence", [])],
        }
        if judge is not None:
            record["judge_pass"] = all(t["judge"]["pass"] for t in turn_scores)
        scored.append(record)
    return scored


def observed(turn):
    """What the shipped path actually did on this turn, from its own record."""
    trace = turn.get("retrieval_trace") or {}
    steps = (turn.get("turn_record") or {}).get("steps") or []
    calls = turn.get("model_calls") or []
    return {
        "kb_searched": bool(trace) and not trace.get("unavailableReason"),
        "passages": trace.get("passages"),
        "kb_sufficient": trace.get("kbSufficient"),
        "corrective_retries": trace.get("kbCorrectiveRetries"),
        "planner_skipped": trace.get("kbPlannerSkipped"),
        "sufficiency_reasons": trace.get("sufficiencyReasons", []),
        "step_kinds": [step.get("kind") for step in steps],
        "tool_calls": [call.get("name") for c in calls for call in c.get("tool_calls") or []],
        "model_calls": len(calls),
    }


def verdict_counts(turn):
    verification = turn.get("verification") or {}
    return verification.get("verdictCounts")


def turn_timing(turn):
    flow = turn.get("flow_timing") or {}
    tokens = turn.get("prompt_tokens") or {}
    return {
        "answered_ms": turn.get("answered_ms"),
        "retrieval_ms": flow.get("retrievalPipelineMs"),
        "generation_ms": flow.get("generationMs"),
        "router_ms": flow.get("routerMs"),
        "verification_wait_ms": turn.get("verification_wait_ms"),
        "cached_prompt_tokens": tokens.get("cached"),
        "evaluated_prompt_tokens": tokens.get("evaluated"),
        "completion_tokens": tokens.get("completion"),
    }


def rate(passes, total):
    return {"passed": passes, "total": total, "rate": passes / total if total else None,
            "wilson95": wilson(passes, total)}


def median(values):
    values = [v for v in values if isinstance(v, (int, float))]
    return statistics.median(values) if values else None


def percentile(values, p):
    values = sorted(v for v in values if isinstance(v, (int, float)))
    if not values:
        return None
    return values[max(0, (p * len(values) + 99) // 100 - 1)]


def summarize(scored, questions):
    judged = all("judge_pass" in row for row in scored) and bool(scored)
    tags = list(questions.get("tags", {}))
    for row in scored:
        tags += [tag for tag in row["tags"] if tag not in tags]

    def bucket(rows):
        result = {"items": len(rows),
                  "mechanical": rate(sum(r["mechanical_checks_passed"] for r in rows), len(rows))}
        if judged:
            result["judge"] = rate(sum(r["judge_pass"] for r in rows), len(rows))
            result["both"] = rate(sum(r["judge_pass"] and r["mechanical_checks_passed"]
                                      for r in rows), len(rows))
        return result

    timings = [t for row in scored for t in row["timing"]]
    observations = [o for row in scored for o in row["observed"]]
    summary = {
        "items": len(scored),
        "turns": len(timings),
        "errors": [row["query_id"] for row in scored if row["error"]],
        "overall": bucket(scored),
        "by_tag": {tag: bucket([r for r in scored if tag in r["tags"]]) for tag in tags
                   if any(tag in r["tags"] for r in scored)},
        "timing_ms": {key: {"median": median([t[key] for t in timings]),
                            "p95": percentile([t[key] for t in timings], 95)}
                      for key in ("answered_ms", "retrieval_ms", "generation_ms", "verification_wait_ms")},
        "prompt_tokens": {key: sum(t[key] or 0 for t in timings)
                          for key in ("cached_prompt_tokens", "evaluated_prompt_tokens", "completion_tokens")},
        "observed": {
            "turns_with_kb_search": sum(o["kb_searched"] for o in observations),
            "turns_with_corrective_retry": sum(bool(o["corrective_retries"]) for o in observations),
            "turns_planner_skipped": sum(bool(o["planner_skipped"]) for o in observations),
            "turns_insufficient": sum(o["kb_sufficient"] is False for o in observations),
            "turns_with_tool_calls": sum(bool(o["tool_calls"]) for o in observations),
            "model_calls_per_turn_median": median([o["model_calls"] for o in observations]),
        },
    }
    if judged:
        summary["disagreements"] = [
            {"query_id": r["query_id"], "mechanical": r["mechanical_checks_passed"], "judge": r["judge_pass"]}
            for r in scored if r["mechanical_checks_passed"] != r["judge_pass"]]
    return summary


def markdown(summary):
    judged = "judge" in summary["overall"]
    head = "| tag | items | mechanical |" + (" judge | both |" if judged else "")
    rule = "|---|---:|---:|" + ("---:|---:|" if judged else "")

    def cell(r):
        return f"{r['passed']}/{r['total']} ({r['rate']:.0%})" if r["total"] else "-"
    lines = [head, rule]
    for name, bucket in [("**overall**", summary["overall"])] + list(summary["by_tag"].items()):
        line = f"| {name} | {bucket['items']} | {cell(bucket['mechanical'])} |"
        if judged:
            line += f" {cell(bucket['judge'])} | {cell(bucket['both'])} |"
        lines.append(line)
    return "\n".join(lines) + "\n"


def score_dataset(dataset, expectations, rows):
    """A run over a labelled dataset, graded by `chat_rag_eval.evaluate_answer`.

    `ranked_ids` there means "citation n is document n"; the desktop numbers
    its evidence itself, so each row says whether the two agree and the
    citation map is applied first when they do not.
    """
    validate_dataset(dataset)
    labels = validate_expectations(dataset, expectations)
    queries = unique_records(dataset["queries"], "id")
    scored = []
    for row in (validate_row(r) for r in rows):
        citation_map = row["citation_map"]
        numbered = [citation_map[str(n)] for n in range(1, len(citation_map) + 1) if str(n) in citation_map]
        aligned = numbered == row["ranked_ids"] and len(numbered) == len(citation_map)
        selected = row["ranked_ids"] if aligned else [citation_map.get(str(n), "")
                                                       for n in range(1, max(map(int, citation_map), default=0) + 1)]
        result = evaluate_answer(queries[row["query_id"]], labels[row["query_id"]], row["answer"] or "",
                                 selected, row["latency_ms"], row["model"])
        result.update({"citations_aligned": aligned, "error": row["error"],
                       "observed": [observed(t) for t in row["turns"]],
                       "timing": [turn_timing(t) for t in row["turns"]]})
        scored.append(result)
    return scored


def manifest(questions, run_meta, judge_settings):
    """Identity in the answer harness's manifest shape, so review tooling can check it."""
    scripts = Path(__file__).parent
    identity = {
        "schema_version": SCHEMA_VERSION,
        "dataset_sha256": digest(questions),
        "expectations_sha256": digest(None),
        "retrieval_sha256": digest(None),
        "settings": {
            "harness": "shipped_desktop_path",
            "model": answering_model(run_meta)[0],
            "model_revision": answering_model(run_meta)[1],
            "recipe_sha256": run_meta.get("recipe_sha256"),
            "corpus_sha256": run_meta.get("corpus_sha256"),
            "thinking": run_meta.get("thinking"),
            "commit": run_meta.get("commit"),
            "judge": judge_settings,
        },
        "harness_sha256": digest({
            "runner": run_meta.get("harness_sha256"),
            "scorer": digest((scripts / "shipped_path_eval.py").read_text()),
        }),
    }
    return {**identity, "run_id": digest(identity)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("validate")
    check.add_argument("questions", type=Path)
    check.add_argument("corpus", type=Path)
    run = sub.add_parser("score")
    run.add_argument("questions", type=Path)
    run.add_argument("answers", type=Path)
    run.add_argument("output", type=Path)
    run.add_argument("--endpoint", help="the judge's OpenAI-compatible base URL, independent of the "
                     "server that wrote the answers; omit to skip the judge")
    run.add_argument("--model", default="judge", help="the model name the judge request carries")
    run.add_argument("--judge-provider", choices=("openai-compatible", "anthropic"),
                     default="openai-compatible",
                     help="anthropic: grade with --model through the Messages API, keyed by "
                          "ANTHROPIC_API_KEY (no --endpoint)")
    labelled = sub.add_parser("score-dataset")
    labelled.add_argument("questions", type=Path, help="the labelled dataset")
    labelled.add_argument("expectations", type=Path)
    labelled.add_argument("answers", type=Path)
    labelled.add_argument("output", type=Path)
    args = parser.parse_args()

    questions = json.loads(args.questions.read_text())
    if args.command == "validate":
        problems = validate_questions(questions, json.loads(args.corpus.read_text()))
        print("\n".join(problems) or f"{len(questions['items'])} items valid")
        raise SystemExit(1 if problems else 0)

    if args.output.exists():
        raise SystemExit(f"{args.output} exists; scores are never overwritten")
    rows = load_jsonl(args.answers)
    if args.command == "score-dataset":
        expectations = json.loads(args.expectations.read_text())
        scored = score_dataset(questions, expectations, rows)
        with args.output.open("x") as out:
            for row in scored:
                out.write(json.dumps(row, ensure_ascii=False) + "\n")
        report = aggregate(scored, len(questions["queries"]))
        report["citations_aligned"] = sum(r["citations_aligned"] for r in scored)
        report["by_dimension_mechanical"] = {
            tag: rate(sum(r["mechanical_checks_passed"] for r in scored if tag in r["dimensions"]),
                      sum(tag in r["dimensions"] for r in scored))
            for tag in sorted({t for r in scored for t in r["dimensions"]})}
        report["timing_ms"] = {key: {"median": median([t[key] for r in scored for t in r["timing"]]),
                                     "p95": percentile([t[key] for r in scored for t in r["timing"]], 95)}
                               for key in ("answered_ms", "retrieval_ms", "generation_ms")}
        Path(str(args.output) + ".summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))
        return
    run_meta_path = Path(str(args.answers) + ".run.json")
    run_meta = json.loads(run_meta_path.read_text()) if run_meta_path.exists() else {}
    answer_model = answering_model(run_meta)[0]
    if args.judge_provider == "anthropic":
        judge = anthropic_judge(args.model)
        judge_settings = (anthropic_judge_request(args.model, [])
                          | {"messages": None, "rubric": JUDGE_RUBRIC, "provider": "anthropic",
                             "deterministic": False,
                             "identity": judge_identity(ANTHROPIC_API, args.model, answer_model,
                                                        headers=anthropic_headers())})
    else:
        judge = http_judge(args.endpoint, args.model) if args.endpoint else None
        judge_settings = (judge_request(args.model, [])
                          | {"messages": None, "rubric": JUDGE_RUBRIC,
                             "identity": judge_identity(args.endpoint, args.model, answer_model)}) if judge else None
    scored = score(questions, rows, judge)
    identity = manifest(questions, run_meta, judge_settings)
    with args.output.open("x") as out:
        for row in scored:
            out.write(json.dumps({**row, "run_id": identity["run_id"]}, ensure_ascii=False) + "\n")
    Path(str(args.output) + ".manifest.json").write_text(json.dumps(identity, indent=2) + "\n")
    summary = summarize(scored, questions)
    Path(str(args.output) + ".summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(markdown(summary))
    print(json.dumps({k: summary[k] for k in summary if k not in ("by_tag", "overall")}, indent=2))


if __name__ == "__main__":
    main()
