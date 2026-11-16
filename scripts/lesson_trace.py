#!/usr/bin/env python3
"""Prints each lesson's trace records from the app log, in order.

Reads the lines with target lattice_learning::lesson_trace, written by
crates/lattice-learning/src/lessons/trace.rs.

usage: lesson_trace.py LOG_FILE [LESSON_SUBSTRING]
       lesson_trace.py diff LOG_A LOG_B [LESSON_SUBSTRING]

diff matches lessons across two runs by title and prints, per lesson and
statement, the records only A has and the records only B has.
"""
import re
import sys
from collections import Counter

TARGET = "lattice_learning::lesson_trace"
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
HEAD = re.compile(r"^(?P<ts>\S+)\s+(?P<level>[A-Z]+)\s+(?P<rest>.*)$")
SPAN = re.compile(r"([A-Za-z_][\w.]*)\{(.*?)\}:")
AFTER_TARGET = re.compile(r"^\s*(?P<file>\S+?):(?P<line>\d+):\s?(?P<body>.*)$")
KEY = re.compile(r"[A-Za-z_][\w.]*")
ESCAPES = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", '"': '"', "'": "'", "0": "\0"}


def strip_ansi(text):
    return ANSI.sub("", text)


def read_quoted(text, at):
    """The Rust Debug string starting at text[at] == '"': (value, index after)."""
    assert text[at] == '"'
    out = []
    i = at + 1
    while i < len(text):
        c = text[i]
        if c == '"':
            return "".join(out), i + 1
        if c == "\\":
            i += 1
            e = text[i]
            if e == "u" and text[i + 1] == "{":
                end = text.index("}", i)
                out.append(chr(int(text[i + 2 : end], 16)))
                i = end + 1
                continue
            out.append(ESCAPES.get(e, e))
            i += 1
            continue
        out.append(c)
        i += 1
    raise ValueError("unterminated string")


def read_value(text, at):
    """A field value at text[at]: a string, a list of strings, or a bare token."""
    if text[at] == '"':
        return read_quoted(text, at)
    if text[at] == "[":
        items = []
        i = at + 1
        while i < len(text) and text[i] != "]":
            if text[i] in ", ":
                i += 1
                continue
            value, i = read_value(text, i)
            items.append(value)
        return items, i + 1
    end = at
    while end < len(text) and not text[end].isspace():
        end += 1
    return text[at:end], end


def read_fields(text):
    """`key=value key="escaped" key=[...]` pairs, in order, as a dict."""
    fields = {}
    i = 0
    while i < len(text):
        if text[i].isspace():
            i += 1
            continue
        m = KEY.match(text, i)
        if not m or m.end() >= len(text) or text[m.end()] != "=":
            j = i
            while j < len(text) and not text[j].isspace():
                j += 1
            i = j
            continue
        value, i = read_value(text, m.end() + 1)
        fields[m.group(0)] = value
    return fields


def parse_line(raw):
    """One record from a log line, or None when it is not a trace record."""
    line = strip_ansi(raw.rstrip("\r\n"))
    if TARGET + ":" not in line:
        return None
    head = HEAD.match(line)
    if not head:
        return None
    rest = head.group("rest")
    at = rest.index(TARGET + ":")
    spans_text = rest[:at]
    after = AFTER_TARGET.match(rest[at + len(TARGET) + 1 :])
    if not after:
        return None
    spans = {}
    for name, body in SPAN.findall(spans_text):
        spans[name] = read_fields(body)
    body = after.group("body")
    kind, _, tail = body.partition(" ")
    return {
        "time": head.group("ts"),
        "level": head.group("level"),
        "spans": spans,
        "kind": kind,
        "fields": read_fields(tail),
    }


def owner_of(record):
    spans = record["spans"]
    if "lesson" in spans and "lesson" in spans["lesson"]:
        return "lesson " + spans["lesson"]["lesson"]
    if "course" in spans and "course" in spans["course"]:
        return "course " + spans["course"]["course"]
    return "(no lesson or course)"


def clock(ts):
    return ts[11:23] if len(ts) >= 23 else ts


def as_list(value):
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def render(record):
    f = record["fields"]
    kind = record["kind"]
    stage = f.get("stage", "?")
    lines = []
    if kind == "changed":
        causes = []
        if f.get("cause_findings"):
            causes.append("findings " + ", ".join(as_list(f["cause_findings"])))
        if f.get("cause_claims"):
            causes.append("claims " + ", ".join(as_list(f["cause_claims"])))
        lines.append(
            "%s  %-20s changed   caused by %s"
            % (clock(record["time"]), stage, "; ".join(causes) or "nothing named")
        )
        for sentence in as_list(f.get("removed")):
            lines.append("      - " + sentence)
        for sentence in as_list(f.get("added")):
            lines.append("      + " + sentence)
    elif kind == "refused":
        lines.append(
            "%s  %-20s refused   %s: %s"
            % (clock(record["time"]), stage, f.get("subject", ""), f.get("reason", ""))
        )
        for sentence in as_list(f.get("proposed_removed")):
            lines.append("      proposed - " + sentence)
        for sentence in as_list(f.get("proposed_added")):
            lines.append("      proposed + " + sentence)
    elif kind == "decided":
        lines.append(
            "%s  %-20s decided   %s -> %s: %s"
            % (
                clock(record["time"]),
                stage,
                f.get("subject", ""),
                f.get("verdict", ""),
                f.get("why", ""),
            )
        )
    elif kind == "published":
        lines.append(
            "%s  %-20s published %s -> %s (sections %s, questions %s, unverified %s)"
            % (
                clock(record["time"]),
                stage,
                f.get("subject", ""),
                f.get("verdict", ""),
                f.get("sections", "?"),
                f.get("questions", "?"),
                f.get("unverified", "?"),
            )
        )
    else:
        lines.append("%s  %-20s %s %s" % (clock(record["time"]), stage, kind, f))
    return lines


def timeline(log_lines, substring=None):
    groups = {}
    order = []
    for raw in log_lines:
        record = parse_line(raw)
        if record is None:
            continue
        owner = owner_of(record)
        if substring and substring not in owner:
            continue
        if owner not in groups:
            groups[owner] = []
            order.append(owner)
        groups[owner].append(record)
    out = []
    for owner in order:
        out.append("== %s (%d records) ==" % (owner, len(groups[owner])))
        for record in groups[owner]:
            out.extend(render(record))
        out.append("")
    return out


UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
LONG_HEX = re.compile(r"(?<![0-9A-Za-z])[0-9a-f]{16,}(?![0-9A-Za-z])")
CLAIM_SUBJECT = re.compile(r"^claim (\S+): (.*)$", re.DOTALL)
ATTEMPT = re.compile(r"\s*\(attempt \d+, escalated \w+\)$")
# Fields that name a claim or time the record, not what it decided.
UNSTABLE_FIELDS = {"cause_claims"}


def normalize(text):
    """Text with the ids that differ between runs replaced."""
    return LONG_HEX.sub("<id>", UUID.sub("<id>", text))


def lesson_title(records):
    """A lesson's title as the trace carries it: the first sentence of its first draft.

    The lesson span names only an id, which differs between runs; the first
    drafting record holds the draft's sentences, title first.
    """
    for record in records:
        fields = record["fields"]
        if record["kind"] == "changed" and fields.get("stage") == "drafting":
            added = as_list(fields.get("added"))
            if added:
                return added[0]
    return None


def statement_of(record):
    """The statement a record is about, without the claim id; else its subject."""
    if record["kind"] == "changed":
        return "(lesson text)"
    subject = record["fields"].get("subject")
    if subject is None:
        return "(lesson)"
    subject = ATTEMPT.sub("", subject)
    claim = CLAIM_SUBJECT.match(subject)
    if claim:
        return "claim: " + normalize(claim.group(2))
    return normalize(subject)


def signature(record):
    """What a record says, with timestamps, levels and run-specific ids left out."""
    items = []
    for name, value in sorted(record["fields"].items()):
        if name in UNSTABLE_FIELDS:
            continue
        if name == "subject":
            attempt = ATTEMPT.search(value)
            value = (attempt.group(0).strip() if attempt else "") + "|" + statement_of(record)
        if isinstance(value, list):
            value = tuple(normalize(item) for item in value)
        else:
            value = normalize(value)
        items.append((name, value))
    return (record["kind"], tuple(items))


def keyed_records(log_lines):
    """Records grouped by lesson title (else id), or by "course", in first-seen order."""
    by_owner = {}
    for raw in log_lines:
        record = parse_line(raw)
        if record is not None:
            by_owner.setdefault(owner_of(record), []).append(record)
    groups = {}
    for owner, records in by_owner.items():
        if owner.startswith("course "):
            key = "course"
        else:
            key = "lesson " + (lesson_title(records) or owner[len("lesson "):])
        while key in groups and key != "course":
            key += " (again)"
        groups.setdefault(key, []).extend(records)
    return groups


def only_in(side, other):
    """The records of `side` with no unused equal record in `other`."""
    left = Counter(signature(record) for record in other)
    out = []
    for record in side:
        sig = signature(record)
        if left[sig] > 0:
            left[sig] -= 1
        else:
            out.append(record)
    return out


def by_statement(records):
    groups = {}
    for record in records:
        groups.setdefault(statement_of(record), []).append(record)
    return groups


def indented(record, pad):
    return [pad + line for line in render(dict(record, time=""))]


def diff_logs(lines_a, lines_b, substring=None):
    groups_a = keyed_records(lines_a)
    groups_b = keyed_records(lines_b)
    keys = list(groups_a) + [key for key in groups_b if key not in groups_a]
    out = []
    compared = identical = 0
    missing_a = missing_b = 0
    total_a = total_b = 0
    for key in keys:
        if substring and substring not in key:
            continue
        records_a = groups_a.get(key, [])
        records_b = groups_b.get(key, [])
        if key not in groups_b:
            missing_b += 1
        elif key not in groups_a:
            missing_a += 1
        else:
            compared += 1
        statements_a = by_statement(records_a)
        statements_b = by_statement(records_b)
        names = list(statements_a) + [n for n in statements_b if n not in statements_a]
        body = []
        for name in names:
            gone = only_in(statements_a.get(name, []), statements_b.get(name, []))
            new = only_in(statements_b.get(name, []), statements_a.get(name, []))
            if not gone and not new:
                continue
            total_a += len(gone)
            total_b += len(new)
            body.append("  [%s]" % name)
            if gone:
                body.append("    only A:")
                for record in gone:
                    body.extend(indented(record, "      "))
            if new:
                body.append("    only B:")
                for record in new:
                    body.extend(indented(record, "      "))
        if key in groups_a and key in groups_b and not body:
            identical += 1
            continue
        note = ""
        if key not in groups_b:
            note = " (only in A)"
        elif key not in groups_a:
            note = " (only in B)"
        out.append("== %s%s ==" % (key, note))
        out.extend(body)
        out.append("")
    out.append(
        "%d compared, %d identical, %d only in A, %d only in B; records only in A: %d, only in B: %d"
        % (compared, identical, missing_b, missing_a, total_a, total_b)
    )
    return out


def read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as log:
        return log.readlines()


def main(argv):
    if len(argv) in (4, 5) and argv[1] == "diff":
        substring = argv[4] if len(argv) == 5 else None
        for line in diff_logs(read_lines(argv[2]), read_lines(argv[3]), substring):
            sys.stdout.write(line + chr(10))
        return 0
    if len(argv) not in (2, 3):
        sys.stderr.write(__doc__)
        return 2
    substring = argv[2] if len(argv) == 3 else None
    with open(argv[1], "r", encoding="utf-8", errors="replace") as log:
        for line in timeline(log, substring):
            sys.stdout.write(line + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
