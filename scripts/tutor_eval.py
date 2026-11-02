#!/usr/bin/env python3
"""Check the tutor eval's synthetic course and score a run of it.

`crates/lattice-desktop/examples/tutor_eval` talks to a real tutor chat and writes one row per
session (`evals/tutor/README.md`). This grades what design 2026-10-28-tutor.md section 4 asks of
those rows, from what the database and the requests show rather than from the wording of a reply:

* citations: the numbers a reply cites exist, and how many claims the grounding check supported,
  beside the same count for the chat;
* a teaching stretch reaches a question within `MAX_TURNS_TO_A_CHECK` turns;
* a session with due cards opens with them, and the cards it shows span modules;
* a rated answer moved the card's FSRS schedule, read back from the database;
* each session's `expect` block.

A pass here is an upper bound: rows are still read by hand before a score is reported.

Commands:
  validate COURSE SESSIONS   check the fixtures
  score COURSE ROWS OUT      grade a run; OUT is the JSON summary
"""
import argparse
import json
import re
from collections import Counter
from pathlib import Path

from chat_rag_eval import CITATION_RE
from replay_model import one_model

ROW_SCHEMA = "lattice-shipped-path-answers/1"
SUMMARY_SCHEMA = "lattice-tutor-eval-summary/1"
COURSE_SCHEMA = "lattice-tutor-course/1"
SESSIONS_SCHEMA = "lattice-tutor-sessions/1"
VERDICTS = ("supported", "contradicted", "unsupported", "unverified")
# Design section 4: "within a few turns".
MAX_TURNS_TO_A_CHECK = 3
NUMBER_RE = re.compile(r"\b\d[\d,.]*\b")
PLAYS = {"right", "wrong"}


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def load_rows(path):
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]


def validate_course(course):
    """Problems with a course fixture: dangling keys, or a card stating a number its source does not."""
    problems = []
    if course.get("schema") != COURSE_SCHEMA:
        problems.append(f"schema is {course.get('schema')!r}")
    sources = {source["key"]: source["text"] for source in course["sources"]}
    lessons = {}
    for module in course["modules"]:
        for lesson in module["lessons"]:
            if lesson["key"] in lessons:
                problems.append(f"lesson {lesson['key']} appears twice")
            lessons[lesson["key"]] = module["title"]
            cited = [key for block in lesson["blocks"] for key in block["sources"]]
            problems += [f"lesson {lesson['key']} cites {key}" for key in cited if key not in sources]
    keys = set()
    for card in course["cards"]:
        if card["key"] in keys:
            problems.append(f"card {card['key']} appears twice")
        keys.add(card["key"])
        if card["lesson"] not in lessons:
            problems.append(f"card {card['key']} is on lesson {card['lesson']}")
        source = sources.get(card["source"])
        if source is None:
            problems.append(f"card {card['key']} cites {card['source']}")
            continue
        for number in NUMBER_RE.findall(card["answer"]):
            if number.rstrip(".,") not in source:
                problems.append(f"card {card['key']} says {number}, which its source does not")
    if len({lessons[card["lesson"]] for card in course["cards"] if card["lesson"] in lessons}) < 2:
        problems.append("the due cards must span modules")
    return problems


def validate_sessions(sessions, course):
    problems = []
    if sessions.get("schema") != SESSIONS_SCHEMA:
        problems.append(f"schema is {sessions.get('schema')!r}")
    lessons = {lesson["key"] for module in course["modules"] for lesson in module["lessons"]}
    seen = set()
    for session in sessions["sessions"]:
        name = session["id"]
        if name in seen:
            problems.append(f"session {name} appears twice")
        seen.add(name)
        if session.get("lesson") is not None and session["lesson"] not in lessons:
            problems.append(f"session {name} is on lesson {session['lesson']}")
        if not session["turns"]:
            problems.append(f"session {name} has no turns")
        for index, turn in enumerate(session["turns"]):
            if ("say" in turn) == ("play" in turn):
                problems.append(f"session {name} turn {index} is neither say nor play")
            elif "play" in turn and turn["play"] not in PLAYS:
                problems.append(f"session {name} turn {index} plays {turn['play']}")
            elif index == 0 and "play" in turn:
                problems.append(f"session {name} opens on a play turn, which has no reply to answer")
    return problems


def asks_the_learner(reply):
    """The app's own test of a reply (`tutor/facts.rs`): its closing paragraph asks something."""
    return "?" in (reply or "").rstrip().rsplit("\n\n", 1)[-1]


def cited_numbers(answer):
    return sorted({int(number) for number in CITATION_RE.findall(answer or "")})


def verdicts_of(turn):
    counts = (turn.get("verification") or {}).get("verdictCounts") or {}
    return {verdict: counts.get(verdict, 0) for verdict in VERDICTS}


def tool_names(turn):
    return [call.get("name") for call in turn.get("tool_calls") or []]


def card_index(course):
    """Each card's module, by its question: what a due list shows of it."""
    modules = {
        lesson["key"]: module["title"] for module in course["modules"] for lesson in module["lessons"]
    }
    return {card["question"]: modules[card["lesson"]] for card in course["cards"]}


def modules_shown(turn, modules_of_card):
    """The modules of the cards `due_reviews` showed this turn."""
    shown = set()
    for call in turn.get("tool_calls") or []:
        if call.get("name") != "due_reviews":
            continue
        result = call.get("result") or ""
        shown |= {module for question, module in modules_of_card.items() if question in result}
    return shown


def schedule_moves(before, after):
    """Cards rated between two read-backs, and whether each one's schedule moved."""
    earlier = {card["question"]: card for card in before}
    moves = []
    for card in after:
        was = earlier.get(card["question"])
        if was is None or card["review_count"] == was["review_count"]:
            continue
        moves.append({
            "card": card.get("key") or card["question"],
            "rating": card["ratings"][-1] if card["ratings"] else None,
            "moved": card["stability"] is not None and not card["due_now"],
            "interval_days": card["interval_days"],
        })
    return moves


def expectations(row, moves, last):
    """What the session's `expect` block asked of the course, and what it holds."""
    expect = row.get("expect") or {}
    failed = []
    if len(moves) < expect.get("reviews_at_least", 0):
        failed.append(f"{len(moves)} reviews, expected {expect['reviews_at_least']}")
    added = sum(1 for card in last.get("cards", []) if card.get("key") is None)
    if added < expect.get("cards_added_at_least", 0):
        failed.append(f"{added} cards added, expected {expect['cards_added_at_least']}")
    notes = " ".join(note.get("content") or "" for note in last.get("notes", []))
    for text in expect.get("notes_containing", []):
        if text.lower() not in notes.lower():
            failed.append(f"no note holds {text!r}")
    return failed


def score_session(row, course):
    if row.get("schema") != ROW_SCHEMA:
        raise SystemExit(f"{row.get('query_id')}: row schema is {row.get('schema')!r}")
    turns = row["turns"]
    tags = row.get("tags") or []
    modules_of_card = card_index(course)
    verdicts = Counter()
    invalid = []
    moves = []
    tools = Counter()
    previous = row.get("course_before")
    for turn in turns:
        verdicts.update(verdicts_of(turn))
        tools.update(tool_names(turn))
        known = {int(number) for number in turn.get("citation_map") or {}}
        invalid += [number for number in cited_numbers(turn.get("answer")) if number not in known]
        after = turn.get("course_after") or {}
        if previous is not None:
            moves += schedule_moves(previous.get("cards", []), after.get("cards", []))
        previous = after
    first_check = next((turn["index"] for turn in turns if asks_the_learner(turn.get("answer"))), None)
    opening = turns[0] if turns else {}
    last = turns[-1].get("course_after") or {} if turns else {}
    return {
        "session": row["query_id"],
        "tags": tags,
        "turns": len(turns),
        "error": row.get("error"),
        "index_mode": row.get("index_mode"),
        "verdicts": {verdict: verdicts[verdict] for verdict in VERDICTS},
        "invalid_citations": invalid,
        "turns_citing": sum(1 for turn in turns if cited_numbers(turn.get("answer"))),
        "tools": dict(tools),
        "first_check_turn": first_check,
        "reached_a_check": first_check is not None and first_check < MAX_TURNS_TO_A_CHECK,
        "opened_with_due": "due_reviews" in tool_names(opening),
        "modules_due": sorted(modules_shown(opening, modules_of_card)),
        "reviews": moves,
        "expect_failed": expectations(row, moves, last),
    }


def summarize(rows, course):
    model = one_model(rows)
    scored = [score_session(row, course) for row in rows]
    tagged = lambda tag: [session for session in scored if tag in session["tags"]]  # noqa: E731
    verdicts = Counter()
    for session in scored:
        verdicts.update(session["verdicts"])
    claims = sum(verdicts.values())
    due = tagged("due-opening")
    teaching = tagged("retrieval-question")
    reviews = [move for session in scored for move in session["reviews"]]
    return {
        "schema": SUMMARY_SCHEMA,
        "model": model,
        "sessions": len(scored),
        "errors": [session["session"] for session in scored if session["error"]],
        "index_modes": sorted({str(session["index_mode"]) for session in scored}),
        "citations": {
            "claims": claims,
            "verdicts": {verdict: verdicts[verdict] for verdict in VERDICTS},
            "supported_share": round(verdicts["supported"] / claims, 3) if claims else None,
            "invalid": {s["session"]: s["invalid_citations"] for s in scored if s["invalid_citations"]},
        },
        "reached_a_check": {
            "passed": sum(1 for session in teaching if session["reached_a_check"]),
            "of": len(teaching),
        },
        "opened_with_due": {"passed": sum(1 for session in due if session["opened_with_due"]), "of": len(due)},
        "due_span_modules": {
            "passed": sum(1 for session in due if len(session["modules_due"]) >= 2),
            "of": len(due),
        },
        "reviews_moved": {"passed": sum(1 for move in reviews if move["moved"]), "of": len(reviews)},
        "expect_failed": {s["session"]: s["expect_failed"] for s in scored if s["expect_failed"]},
        "by_session": scored,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate")
    validate.add_argument("course")
    validate.add_argument("sessions")
    score = commands.add_parser("score")
    score.add_argument("course")
    score.add_argument("rows")
    score.add_argument("out")
    args = parser.parse_args()
    course = load_json(args.course)
    if args.command == "validate":
        problems = validate_course(course) + validate_sessions(load_json(args.sessions), course)
        if problems:
            raise SystemExit("; ".join(problems))
        print("course and sessions validated")
        return
    summary = summarize(load_rows(args.rows), course)
    Path(args.out).write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in summary.items() if key != "by_session"}, indent=2))


if __name__ == "__main__":
    main()
