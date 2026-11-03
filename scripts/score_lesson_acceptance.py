#!/usr/bin/env python3
"""Score the acceptance run of docs/design/2026-11-01-lesson-claim-classes.md section 3.11.

Input: a copy of the app's `lattice.db` after the app's own job prepared the lesson (default `f36400bc`). The
database is opened read-only. The report names ids, classes, states, counts, titles and addresses of saved pages
and the model each record names, and never the text of a page or of a statement.

  python scripts/score_lesson_acceptance.py PATH/TO/lattice.db [--lesson f36400bc]

Exit status 0 when every hard check passes, 1 otherwise; the report says which failed.

Where each number comes from, in the app's own tables:
  end state    learning_lessons.preparation, learning_lesson_reviews.state, learning_lesson_verifications
  statements   learning_lesson_claims (state_json) and learning_lesson_claim_events (classified, checked, ...)
  calls made   jobs.activity_json: modelCalls; the most they can come to: modelCallsCeiling; how often that most
               went up: modelCallsCeilingRises (the app counts it as it publishes the ceiling)
  terms        S, R, C: activity_json.statements; F, G, V, D, U: the claims' check events; K: the job's
               `claim-ledger-v1:cuts` checkpoint; M: the claims' places in their sections
  gate         learning_source_versions.not_a_source and admission_policy, learning_blocks.source_ids_json,
               the report's findings
  models       the checker of each check event, learning_lesson_reviews.checker_model, the report's checker_model,
               jobs.activity_json.modelName. A classification, rewrite or flow-repair call records no model in the
               database: read the llama-server's log for those.
"""
import argparse
import json
import math
import sqlite3
import sys
from collections import Counter

# The lesson design 3.11 was written for, its planted statement, and the statements its text makes (105 less the 32
# coverage injections). Other lessons have no such count; how long a run takes is reported, the calls bound it.
LESSON = "f36400bc"
PLANTED = "Microsoft's grounding feature returns bounding box coordinates in millimetres."
STATEMENT_LIMIT = 73
SHORT_PAGE_WORDS = 400
SORT_BATCH = 20
# The authority of a class, weakest first (design 3.1): the ledger keeps the strongest authority's, and of one
# authority's the strictest.
AUTHORITY = {"model": 0, "rule": 1, "writer": 2}
STRICTNESS = {"internal": 0, "common": 1, "reasoning": 2, "sourced": 3}
# What a class may end in (design 3.11); any class may also end removed by the machine.
TERMINAL = {
    "sourced": {"resolved_by_evidence"},
    "reasoning": {"consistent"},
    "common": {"general_knowledge"},
}
# Calls a second check of a statement sharing a sentence can make, by class (design 3.11, M).
RECHECK = {"sourced": 1, "reasoning": 5, "common": 4}
STRICT_RESULTS = {"supported", "supported_elsewhere"}


def connect(path):
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    return db


def loads(text):
    try:
        return json.loads(text) if text else None
    except (TypeError, ValueError):
        return None


def tag(value):
    """The variant of a serde value: a string, or the one key of an object."""
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and value:
        return next(iter(value))
    return None


def result_of(event):
    """(result, failure): a check event's result variant, and the verdict kind of a failed one."""
    result = event.get("result")
    kind = tag(result)
    failure = None
    if kind == "failed" and isinstance(result, dict):
        failure = tag((result.get("failed") or {}).get("kind"))
    return kind, failure


class Claim:
    def __init__(self, row, events):
        self.id = row["claim_id"]
        self.unit = row["unit_index"]
        self.field = row["field"]
        self.quote = row["quote"] or ""
        self.statement = row["statement"] or ""
        self.state = loads(row["state_json"]) or {}
        self.events = events
        self.cls = None
        held = (-1, -1)
        for event in events:
            if event.get("kind") == "classified":
                rank = (AUTHORITY.get(event.get("by"), -1), STRICTNESS.get(event.get("class"), -1))
                if rank > held:
                    self.cls, held = event.get("class"), rank

    @property
    def kind(self):
        return self.state.get("state")

    def checks(self, who="local"):
        return [e for e in self.events if e.get("kind") == "checked" and tag(e.get("checker")) == who]

    def failed_for_real(self):
        """Some local check failed it with an answer (an unreadable reply is no failure)."""
        return any(f not in (None, "could_not_judge") for _, f in map(result_of, self.checks()))

    def unjudged(self):
        return sum(1 for _, f in map(result_of, self.checks()) if f == "could_not_judge")

    def results(self):
        return [r for r, _ in map(result_of, self.checks())]


def load_claims(db, lesson_id):
    events = {}
    for row in db.execute(
        "SELECT e.claim_id, e.payload_json FROM learning_lesson_claim_events e "
        "JOIN learning_lesson_claims c ON c.claim_id = e.claim_id WHERE c.lesson_id = ? "
        "ORDER BY e.claim_id, e.ordinal",
        (lesson_id,),
    ):
        stored = loads(row["payload_json"]) or {}
        events.setdefault(row["claim_id"], []).append(stored.get("event") or {})
    rows = db.execute(
        "SELECT claim_id, unit_index, field, quote, statement, state_json FROM learning_lesson_claims "
        "WHERE lesson_id = ? ORDER BY unit_index, claim_id",
        (lesson_id,),
    ).fetchall()
    return [Claim(row, events.get(row["claim_id"], [])) for row in rows]


def terminal(claim):
    """Whether a claim ends in one of the states its class allows (design 3.11)."""
    if claim.kind == "removed":
        return claim.state.get("by") == "machine"
    if claim.kind == "edited":
        # A repair's successor answers for it; the successor is a claim of its own.
        return claim.state.get("successor") is not None and claim.state.get("by", "machine") == "machine"
    return claim.kind in TERMINAL.get(claim.cls, set())


def stand_together(a, b):
    return a.unit == b.unit and a.field == b.field and a.quote and b.quote and (a.quote in b.quote or b.quote in a.quote)


def recheck_calls(claims):
    """M: the most calls re-checking the statements that share a sentence can make."""
    live = [c for c in claims if c.kind not in ("removed", "edited")]
    return sum(
        RECHECK.get(c.cls, 0)
        for c in live
        if any(o is not c and stand_together(c, o) for o in live)
    )


def end_state(db, lesson, claims, failures):
    review = db.execute(
        "SELECT state, job_id, checker_model FROM learning_lesson_reviews WHERE lesson_id = ?", (lesson["id"],)
    ).fetchone()
    verification = db.execute(
        "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id = ?", (lesson["id"],)
    ).fetchone()
    out = {
        "preparation": lesson["preparation"],
        "review": review["state"] if review else None,
        "verification_row": verification is not None,
    }
    if lesson["preparation"] != "ready":
        failures.append(f"the lesson is {lesson['preparation']!r}, not 'ready'")
    if out["review"] != "ready":
        failures.append(f"the review is {out['review']!r}, not 'ready'")
    if not out["verification_row"]:
        failures.append("no learning_lesson_verifications row")
    pairs = Counter()
    for claim in claims:
        pairs[(claim.cls or "none", claim.kind)] += 1
        if claim.cls in (None, "internal"):
            failures.append(f"claim {claim.id[:8]} has class {claim.cls!r}: the ledger holds no Internal or unsorted statement")
        elif not terminal(claim):
            failures.append(f"claim {claim.id[:8]}: {claim.cls} ended {claim.kind!r}, a pairing outside the five of 3.11")
    live = [c for c in claims if c.kind != "edited"]
    limit = STATEMENT_LIMIT if lesson["id"].startswith(LESSON) else None
    out["pairs"] = dict(sorted(pairs.items()))
    out["statements"] = {"ledger_rows": len(claims), "live": len(live), "limit": limit}
    if limit is not None and len(live) > limit:
        failures.append(f"{len(live)} statements in the ledger, more than {limit}")
    return out, (verification and loads(verification["report_json"])) or {}, review


def cuts_of(db, job_id):
    row = db.execute(
        "SELECT data_json FROM job_checkpoints WHERE job_id = ? AND key = 'claim-ledger-v1:cuts'", (job_id,)
    ).fetchone()
    return loads(row["data_json"]) if row else None


def calls(db, lesson, claims, review, failures):
    job = db.execute(
        "SELECT status, activity_json, started_at, finished_at FROM jobs WHERE id = ?",
        (review["job_id"] if review else None,),
    ).fetchone()
    activity = (loads(job["activity_json"]) if job else None) or {}
    plan = activity.get("statements") or {}
    s, r, c = (int(plan.get(k) or 0) for k in ("sourced", "reasoning", "common"))
    sections = db.execute("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id = ?", (lesson["id"],)).fetchone()[0]
    cuts = (cuts_of(db, review["job_id"]) if review else None) or {}
    by_class = lambda cls: [x for x in claims if x.cls == cls]
    f = sum(1 for x in claims if x.cls in ("sourced", "common") and x.failed_for_real())
    g = sum(1 for x in by_class("reasoning") if x.failed_for_real())
    v = sum(1 for x in by_class("reasoning") if STRICT_RESULTS & set(x.results()))
    d = sum(1 for x in by_class("common") if STRICT_RESULTS & set(x.results()))
    u = sum(x.unjudged() for x in claims)
    k = len(cuts.get("taken") or {})
    m = recheck_calls(claims)
    sorting = math.ceil((s + r + c) / SORT_BATCH)
    worst = sorting + s + r + 3 * r + 3 * c + 4 * (s + c) + 6 * r + sections + r + c + m + (s + 3 * r + 3 * c)
    bound = sorting + s + r + 3 * r + 3 * c + 4 * f + 6 * g + k + v + d + m + u
    made, ceiling = activity.get("modelCalls"), activity.get("modelCallsCeiling")
    rises = activity.get("modelCallsCeilingRises")
    recorded = Counter()
    for claim in claims:
        for event in claim.events:
            if event.get("kind") == "checked" and tag(event.get("checker")) == "local":
                recorded[f"checks of {claim.cls or 'unsorted'} statements"] += 1
            elif event.get("kind") == "repair_attempted" and tag(event.get("outcome")) == "rewritten":
                recorded["rewrites"] += 1
    out = {
        "made": made,
        "ceiling_at_the_end": ceiling,
        "ceiling_rises": rises,
        "terms": {"S": s, "R": r, "C": c, "F": f, "G": g, "K": k, "V": v, "D": d, "M": m, "U": u},
        "sort_calls": sorting,
        "bound_with_the_terms_found": bound,
        "bound_with_every_term_at_most": worst,
        "recorded_in_the_ledger": dict(sorted(recorded.items())),
        "sections": sections,
        "flow_repairs": len(cuts.get("mended") or {}),
        "sections_that_go": len(cuts.get("going") or {}),
    }
    if made is None or ceiling is None:
        failures.append("the job records no calls made or no ceiling (jobs.activity_json)")
    else:
        if made > ceiling:
            failures.append(f"{made} calls made, more than the {ceiling} the panel ended at")
        if made > worst:
            failures.append(f"{made} calls made, more than the {worst} the plan allowed at its start")
    if rises is None:
        failures.append("the job does not record whether its ceiling went up")
    elif rises > 0:
        failures.append(f"the ceiling went up {rises} time(s): it may only come down")
    out["minutes"] = out["seconds_per_call"] = None
    if job and job["started_at"] is not None and job["finished_at"] is not None:
        minutes = (job["finished_at"] - job["started_at"]) / 60000
        out["minutes"] = round(minutes, 1)
        if made:
            out["seconds_per_call"] = round(minutes * 60 / made, 1)
    else:
        failures.append("the job's start and finish are not recorded")
    out["model_name"] = activity.get("modelName")
    return out


def gate(db, lesson, report, failures):
    refused = db.execute(
        "SELECT v.id, v.title, v.not_a_source AS reason, v.word_count, COALESCE(v.resolved_url, v.requested_url) AS url "
        "FROM learning_source_library s JOIN learning_source_versions v "
        "ON v.program_id = s.program_id AND v.id = s.active_version_id "
        "WHERE s.program_id = ? AND s.deleted_at IS NULL AND v.not_a_source IS NOT NULL ORDER BY s.created_at, s.id",
        (lesson["program_id"],),
    ).fetchall()
    short = db.execute(
        "SELECT v.id, v.title, v.word_count, COALESCE(v.resolved_url, v.requested_url) AS url "
        "FROM learning_source_library s JOIN learning_source_versions v "
        "ON v.program_id = s.program_id AND v.id = s.active_version_id "
        "WHERE s.program_id = ? AND s.deleted_at IS NULL AND v.not_a_source IS NULL "
        "AND v.admission_policy IS NOT NULL AND v.word_count < ? ORDER BY v.word_count, s.created_at, s.id",
        (lesson["program_id"], SHORT_PAGE_WORDS),
    ).fetchall()
    refused_ids = {row["id"] for row in refused}
    citing = []
    for block in db.execute("SELECT ordinal, source_ids_json FROM learning_blocks WHERE lesson_id = ?", (lesson["id"],)):
        cited = set(loads(block["source_ids_json"]) or [])
        if cited & refused_ids:
            citing.append(("block", block["ordinal"], sorted(cited & refused_ids)))
    for index, finding in enumerate(report.get("findings") or []):
        used = {p.get("source_id") for p in finding.get("evidence") or []} & refused_ids
        if used:
            citing.append(("finding", index, sorted(used)))
    if citing:
        failures.append(f"{len(citing)} place(s) of the lesson cite a page the admission gate refused")
    return {
        "refused": [dict(row) for row in refused],
        "admitted_under": [dict(row) for row in short],
        "words_floor": SHORT_PAGE_WORDS,
        "citing_a_refused_page": [{"in": kind, "at": at, "versions": ids} for kind, at, ids in citing],
    }


def planted(claims, report):
    found = [c for c in claims if c.statement == PLANTED]
    in_report = any(f.get("statement") == PLANTED for f in report.get("findings") or [])
    out = {"in_the_ledger": bool(found), "in_the_lesson": in_report}
    if found:
        claim = found[0]
        out |= {"class": claim.cls, "ended": claim.kind, "checks_in_order": claim.results()}
        out["note"] = (
            "the falsity veto's own verdict is not stored: its effect is how the statement ended"
        )
    return out


def sent_to_source(claims):
    """Common statements a check against a source decided, which only a lone dissent of the knowledge judge asks for."""
    return [c.id[:8] for c in claims if c.cls == "common" and STRICT_RESULTS & set(c.results())]


def models(claims, review, report, activity_model):
    counts = Counter()
    for claim in claims:
        for event in claim.events:
            if event.get("kind") == "checked":
                who = tag(event.get("checker"))
                model = (event.get("checker") or {}).get(who, {}).get("model") if who else None
                counts[f"{who or '?'} check: {model}"] += 1
    return {
        "checks": dict(sorted(counts.items())),
        "review_checker_model": review["checker_model"] if review else None,
        "report_checker_model": report.get("checker_model"),
        "job_model_name": activity_model,
        "not_recorded": "classification, rewrite and flow-repair calls name no model in the database",
    }


def score(path, lesson_prefix=LESSON):
    db = connect(path)
    try:
        return scored(db, lesson_prefix)
    finally:
        db.close()


def scored(db, lesson_prefix):
    failures = []
    lesson = db.execute(
        "SELECT id, program_id, preparation FROM learning_lessons WHERE id LIKE ? || '%'", (lesson_prefix,)
    ).fetchone()
    if lesson is None:
        return {"failures": [f"no lesson starts with {lesson_prefix!r}"]}
    claims = load_claims(db, lesson["id"])
    state, report, review = end_state(db, lesson, claims, failures)
    made = calls(db, lesson, claims, review, failures)
    result = {
        "lesson": lesson["id"][:8],
        "end_state": state,
        "calls": made,
        "gate": gate(db, lesson, report, failures),
        "planted_statement": planted(claims, report),
        "common_statements_sent_to_a_source_check": sent_to_source(claims),
        "models": models(claims, review, report, made.get("model_name")),
        "failures": failures,
    }
    return result


def render(result):
    lines = []
    if "end_state" not in result:
        return "\n".join(["FAILED"] + [f"  - {f}" for f in result["failures"]])
    state, made, gate_ = result["end_state"], result["calls"], result["gate"]
    lines += [f"Lesson {result['lesson']}"]
    lines += [
        "",
        "End state",
        f"  preparation {state['preparation']} | review {state['review']} | verification row {state['verification_row']}",
        f"  statements: {state['statements']['live']} live of {state['statements']['ledger_rows']} ledger rows"
        + (f" (at most {state['statements']['limit']})" if state["statements"]["limit"] is not None else ""),
    ]
    lines += [f"  {cls} -> {kind}: {n}" for (cls, kind), n in state["pairs"].items()]
    t = made["terms"]
    lines += [
        "",
        "Calls (the app's own count, jobs.activity_json)",
        f"  made {made['made']} | at most, at the end {made['ceiling_at_the_end']} | "
        f"the ceiling went up {made['ceiling_rises']} time(s) | {made['minutes']} minutes, "
        f"{made['seconds_per_call']} s a call",
        "  terms: " + " ".join(f"{k}={v}" for k, v in t.items()) + f" | sorting {made['sort_calls']}",
        f"  bound with the terms found {made['bound_with_the_terms_found']} | "
        f"with every term at most {made['bound_with_every_term_at_most']}",
        f"  recorded in the ledger: {made['recorded_in_the_ledger']}",
        f"  sections {made['sections']}, flow repairs {made['flow_repairs']}, sections that go {made['sections_that_go']}",
        "",
        f"Gate: {len(gate_['refused'])} page(s) refused",
    ]
    lines += [
        f"  refused {row['id'][:8]} | {row['reason']} | {row['word_count']} words | {row['title']} | {row['url']}"
        for row in gate_["refused"]
    ]
    lines += [f"Gate: {len(gate_['admitted_under'])} admitted page(s) under {gate_['words_floor']} words"]
    lines += [
        f"  admitted {row['id'][:8]} | {row['word_count']} words | {row['title']} | {row['url']}"
        for row in gate_["admitted_under"]
    ]
    lines += [f"  cites a refused page: {c}" for c in gate_["citing_a_refused_page"]]
    p = result["planted_statement"]
    lines += ["", "The planted statement " + (f"is in the ledger: {p}" if p["in_the_ledger"] else "is not in this lesson")]
    lines += [f"Common statements sent to a source check: {result['common_statements_sent_to_a_source_check']}"]
    lines += ["", f"Models: {json.dumps(result['models'], indent=2)}"]
    lines += ["", "FAILED" if result["failures"] else "PASSED"]
    lines += [f"  - {f}" for f in result["failures"]]
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("database")
    parser.add_argument("--lesson", default=LESSON, help="the start of the lesson's id")
    args = parser.parse_args(argv)
    result = score(args.database, args.lesson)
    print(render(result))
    return 1 if result["failures"] else 0


if __name__ == "__main__":
    sys.exit(main())
