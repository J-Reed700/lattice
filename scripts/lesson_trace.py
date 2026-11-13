#!/usr/bin/env python3
"""Prints each lesson's trace records from the app log, in order.

Reads the lines with target lattice_learning::lesson_trace, written by
crates/lattice-learning/src/lessons/trace.rs.

usage: lesson_trace.py LOG_FILE [LESSON_SUBSTRING]
"""
import re
import sys

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


def main(argv):
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
