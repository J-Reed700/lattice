"""Offline replay of the app's note step (chat/tool_loop/research_notes.rs) on saved turns.

The saved reading prompts' passages stand in for what a research turn has read: the message keeps its
instructions and question with the Knowledge Base Results block taken out, and the passages come back as the
results of the turn's calls (`semantic_search` for the question, then `get_document` for the section a result
opens with), RESULT_SIZE passages to a result, in label order, while they stay within RESULTS_CHARS: the share
of a window the loop's results reach before they are condensed, less the room the message and the reservation take.

What is replayed is the request the app sends when those results pass 60% of the window. Every labelled passage is
a source named r1, r2, ... and its sentences u1, u2, ...; one request asks the model, with a JSON schema, which
sentences of each source to keep, by handle. The answer is read as the app reads it (the first JSON object, what
closed of it, handles checked against what was shown). A source the answer does not settle is asked about again
in smaller requests, alone with one resend after a cut-off, and with one correction; one still unsettled is kept
whole. The notes are the kept sentences, unchanged, under the source's heading, and are used only when they are
shorter than the results. A fact is kept when the text after condensing holds one of the rubric's spellings for it
(case-insensitive), of the facts whose passage is among the results; it is cited when it sits in its own
passage's block of the notes.

`--question message` (arm SHIPPED) asks with the whole last user message before the results, as the app does.
`--question line` (arm LINE) asks with the question's line alone.

Approximations: `sentences` is not the app's splitter (UAX#29 boundaries with suppression rules) and splits where
its own rules say; headings are `[s<label>] <title>` without the file name and score the app's search headings
carry; the replay's results hold nothing but passages, so there is no rest source and no legend lines; the replay
does not check that a request fits the window, so it never splits a batch for size and never restates a
correction as the task with the correction appended. Each reply's prompt_tokens is recorded so a request that
would not have fitted shows.

  python -I -u scripts/replay_condense_ab.py --out OUT --llama URL --rubric R ... --calls A.calls.jsonl ... --window 16384
  python -I -u scripts/replay_condense_ab.py --out OUT --llama URL ... --pack 4 --result-size 8 --results-chars 130000 --window 65536
  python -I scripts/replay_condense_ab.py --out OUT --summary
"""
import argparse, json, os, re, statistics, sys
from collections import deque
from dataclasses import dataclass, field

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import replay_claude as claude  # noqa: E402
import replay_close_read_ab as base  # noqa: E402
import replay_cut_evidence_ab as cut  # noqa: E402
import replay_grouped_reading_ab as grouped  # noqa: E402
import replay_model  # noqa: E402

RESULT_SIZE = 4
RESULTS_CHARS = 26_000
# research_notes.rs CONDENSE_INSTRUCTION.
CONDENSE = ("Your context window is nearly full. Before you continue, choose from all the tool results you have so far "
            "the sentences that make up compact research notes. Keep every sentence with a fact, number, date, name, "
            "short quotation or URL that could matter to my question. Drop navigation text, boilerplate, repetition "
            "and anything that does not bear on my question. Do not answer my question yet. Answer with an entries "
            "list: each entry names a result in `of` by the handle shown beside it, and lists in `keep` the handles "
            "of that result's sentences to keep, which may be none.")
# research_notes.rs NOTES_HEADER.
NOTES_HEADER = ("Research notes so far. These condense the tool results you gathered earlier in this turn, to make room "
                "in your context window. Every citation label in them still refers to the same source. Continue the "
                "research from them, or answer if they are enough.")
# research_notes.rs NOTHING_KEPT.
NOTHING_KEPT = "Nothing here bears on the question."
# ask.rs NOT_READ, CUT_OFF, UNPLACED.
NOT_READ = "Your answer could not be read. Answer again in the form asked for."
CUT_OFF = "Your answer was cut off before it ended. Answer again, more briefly."
UNPLACED = ("Your answer pointed at something that is not shown. Point only at what is shown, by the name it is "
            "shown under.")
# ask.rs `told`.
MISSING = "Your answer said nothing about {handle}. Answer for it."
CONTRADICTED = "{what} Answer again and say one thing."
# llm_port.rs CUT_OFF_MARKERS.
CUT_OFF_MARKERS = ("length", "max_tokens", "max_output_tokens", "incomplete")
# SamplingOverride::deterministic().
SAMPLING = {"temperature": 0.0, "top_p": 1.0, "top_k": 1, "presence_penalty": 0.0}
OUTPUT_RESERVATION_CAP = 32_768
ABBREVIATIONS = {"mr", "mrs", "ms", "dr", "st", "mt", "jr", "sr", "co", "no", "vs", "etc", "e.g", "i.e"}


@dataclass
class Source:
    label: int
    heading: str
    sentences: list = field(default_factory=list)


def transcript(ctx):
    """The messages up to the last result, and each call's id and result text."""
    every, head, tail = cut.blocks(ctx["bare"])
    head = head.replace("Knowledge Base Results:\n", "").rstrip()
    question = re.search(r"(?m)^Question: (.*)$", tail).group(1)
    messages = [dict(ctx["read_req"]["messages"][0]), {"role": "user", "content": f"{head}\n\n{tail.strip()}"}]
    labels, results, total = sorted(every), [], 0
    for i in range(0, len(labels), RESULT_SIZE):
        call_id = f"call_{i // RESULT_SIZE + 1}"
        first = every[labels[i]]
        document = re.search(r"(?m)^Document ID: (.*)$", first)
        heading = re.search(r"(?m)^Source heading: (.*)$", first)
        name, args = (("semantic_search", {"query": question}) if not i or not document else
                      ("get_document", {"document_id": document.group(1).strip(),
                                        **({"section": heading.group(1).strip()} if heading else {})}))
        text = "\n\n\n\n".join(every[l] for l in labels[i:i + RESULT_SIZE])
        total += len(text)
        if total > RESULTS_CHARS and results:
            break
        messages += [{"role": "assistant", "content": "", "tool_calls": [
                         {"id": call_id, "type": "function", "function": {"name": name, "arguments": json.dumps(args)}}]},
                     {"role": "tool", "tool_call_id": call_id, "content": text}]
        results.append((call_id, text, labels[i:i + RESULT_SIZE]))
    return messages, results


def packed(ctxs, k):
    """The long questions of each run, `k` at a time, as one turn that asks all of them in turn: their passages
    one after another, labelled again from [1], so one condensing holds every question's facts."""
    by_run = {}
    for c in ctxs:
        if c["set"] == "long":
            by_run.setdefault(c["run"], []).append(c)
    out = []
    for run, cs in by_run.items():
        for i in range(0, len(cs) - k + 1, k):
            group, blocks = cs[i:i + k], []
            for c in group:
                every, _, _ = cut.blocks(c["bare"])
                blocks += [re.sub(r"^\[\d+\]", f"[{len(blocks) + n + 1}]", every[l], count=1)
                           for n, l in enumerate(sorted(every))]
            _, head, _ = cut.blocks(group[0]["bare"])
            asks = " ".join(f"({n}) {re.search(r'(?m)^Question: (.*)$', c['bare']).group(1)}"
                            for n, c in enumerate(group, 1))
            bare = (head + "\n\n\n\n".join(blocks)
                    + f"\n\n\n\nQuestion: Answer each of these in turn. {asks}\n\nAnswer:")
            qids = [c["qid"] for c in group]
            out.append(dict(group[0], key=f"{run}|pack:{'+'.join(qids)}", qid=None, qids=qids, set="pack",
                            bare=bare, texts=base.passages(bare)))
    return out


def facts_of(ctx):
    return [f for q in ctx.get("qids") or [ctx["qid"]] for f in base.facts(q)]


# ---------- sentences, sources and the question ----------
def one_line(text):
    return text.replace("\r", " ").replace("\n", " ")


def sentences(text):
    """An approximation of lattice_core::shared::text::sentences, which cuts at UAX#29 sentence boundaries with
    suppression rules and at every line break; Python cannot port it exactly. Here a line is cut after a run of
    . ! ? and closing quotes or brackets when whitespace follows, the next word does not start in lowercase and the
    run does not end an abbreviation or an initial."""
    pieces = []
    for line in text.replace("\r\n", "\n").split("\n"):
        start = 0
        for m in re.finditer(r"([.!?]+)([\"'”’)\]]*)(?=\s)", line):
            rest = line[m.end():].lstrip()
            if rest[:1].islower():
                continue
            if m.group(1).endswith("."):
                token = re.search(r"(\S*)$", line[:m.start(1) + len(m.group(1)) - 1]).group(1)
                token = re.sub(r"^[^A-Za-z0-9]+", "", token).lower()
                if token in ABBREVIATIONS or (len(token) == 1 and token.isalpha()):
                    continue
            pieces.append(line[start:m.end()])
            start = m.end()
        pieces.append(line[start:])
    return [p.strip() for p in pieces if p.strip()]


def sources_of(ctx, results):
    """research_notes.rs `sources_in` for labelled passages: each passage of each result is a source under
    `[s<label>] <title>`."""
    every, _, _ = cut.blocks(ctx["bare"])
    out = []
    for _, _, labels in results:
        for label in labels:
            first = every[label].split("\n", 1)[0]
            m = re.match(r"^\[\d+\] Document: (.*)$", first)
            title = m.group(1) if m else ""
            out.append(Source(label, one_line(f"[s{label}] {title}"),
                              [one_line(s) for s in sentences(ctx["texts"][label][1])]))
    return out


def question_of(messages, mode="message"):
    """The last user message before the results (research_notes.rs `question_of`), or its question line."""
    content = next(m["content"] for m in reversed(messages) if m["role"] == "user")
    if mode == "line":
        return re.search(r"(?m)^Question: (.*)$", content).group(1)
    return content


# ---------- one request ----------
def render(question, shown):
    """The notes request's messages for `shown`, a list of (index, source): handles r<index + 1> and u<k>."""
    task = f"My question: {question}\n\nThe tool results:"
    for index, source in shown:
        task += f"\n\n[r{index + 1}] {source.heading}"
        for k, sentence in enumerate(source.sentences, 1):
            task += f"\n[u{k}] {sentence}"
    return [{"role": "system", "content": CONDENSE}, {"role": "user", "content": task}]


def schema(shown, enum=False):
    """The answer's JSON schema. Plain is what the app's contract writes; enum is what a backend that enforces
    schemas (llama.cpp) is given, each pointer limited to the handles shown."""
    handles = [f"r{index + 1}" for index, _ in shown]
    most = max((len(source.sentences) for _, source in shown), default=0)
    if enum:
        of, keep = {"type": "string", "enum": handles}, {"type": "string", "enum": [f"u{k}" for k in range(1, most + 1)]}
    else:
        of = {"type": "string",
              "description": f"One of the names the request shows, such as {handles[0] if handles else 'r1'}."}
        keep = {"type": "string", "description": "One of the names the request shows, such as u1."}
    entry = {"type": "object", "properties": {"of": of, "keep": {"type": "array", "items": keep}},
             "required": ["of", "keep"], "additionalProperties": False}
    return {"type": "object", "properties": {"entries": {"type": "array", "items": entry}},
            "required": ["entries"], "additionalProperties": False}


def max_tokens_for(window):
    """The planner's default output reservation, which a contract call that asks for no output size gets."""
    return min(window // 4, OUTPUT_RESERVATION_CAP)


def body(model, messages, schema, window):
    return {"model": model, "messages": messages, **SAMPLING, "repeat_penalty": 1.0,
            "max_tokens": max_tokens_for(window), "chat_template_kwargs": {"enable_thinking": False},
            "response_format": {"type": "json_schema", "json_schema": {"name": "response", "schema": schema}}}


def sender(window):
    def send(messages, schema):
        return base.call(body(base.running_model(), messages, schema, window), "condense")
    return send


# ---------- reading an answer (model_contract/shape/document.rs and shapes.rs `read_each`) ----------
_BAD = object()
_SPACE = " \t\n\r\x0c"


@dataclass
class Document:
    fields: dict = field(default_factory=dict)
    open_list: tuple = None
    closed: bool = False


def _skip(text, at):
    while at < len(text) and text[at] in _SPACE:
        at += 1
    return at


def _string_end(text, start):
    at = start + 1
    while at < len(text):
        if text[at] == "\\":
            at += 2
        elif text[at] == '"':
            return "closed", at + 1
        else:
            at += 1
    return "open", None


def _nested_end(text, start):
    depth, at = 0, start
    while at < len(text):
        c = text[at]
        if c == '"':
            kind, end = _string_end(text, at)
            if kind != "closed":
                return kind, None
            at = end
            continue
        if c in "{[":
            depth += 1
        elif c in "}]":
            depth = max(depth - 1, 0)
            if depth == 0:
                return "closed", at + 1
        at += 1
    return "open", None


def _literal_end(text, start):
    at = start
    while at < len(text):
        if text[at] in _SPACE or text[at] in ",}]":
            return ("bad", None) if at == start else ("closed", at)
        at += 1
    return "open", None


def _value_end(text, start):
    if start >= len(text):
        return "open", None
    if text[start] == '"':
        return _string_end(text, start)
    if text[start] in "{[":
        return _nested_end(text, start)
    return _literal_end(text, start)


def _parse(text, start, end):
    try:
        return json.loads(text[start:end])
    except ValueError:
        return _BAD


def _list_at(text, open_at):
    items, at, need_comma = [], open_at + 1, False
    while True:
        at = _skip(text, at)
        if at >= len(text):
            return "cut", items, None
        c = text[at]
        if c == "]":
            return "closed", items, at + 1
        if c == ",":
            if not need_comma:
                return "cut", items, None
            need_comma, at = False, at + 1
        elif need_comma:
            return "cut", items, None
        else:
            kind, end = _value_end(text, at)
            if kind != "closed":
                return "cut", items, None
            value = _parse(text, at, end)
            if value is _BAD:
                return "cut", items, None
            items.append(value)
            at, need_comma = end, True


def _object_at(text, open_at):
    first = _skip(text, open_at + 1)
    if text[first:first + 1] not in ("}", '"'):
        return None
    doc, at = Document(), first
    while True:
        at = _skip(text, at)
        c = text[at:at + 1]
        if c == "}":
            doc.closed = True
            return doc
        if c != '"':
            return doc
        kind, key_end = _string_end(text, at)
        if kind != "closed":
            return doc
        key = _parse(text, at, key_end)
        if not isinstance(key, str):
            return doc
        at = _skip(text, key_end)
        if text[at:at + 1] != ":":
            return doc
        at = _skip(text, at + 1)
        if text[at:at + 1] == "[":
            kind, items, end = _list_at(text, at)
            if kind == "cut":
                doc.open_list = (key, items)
                return doc
            doc.fields[key] = items
            at = end
        else:
            kind, end = _value_end(text, at)
            if kind != "closed":
                return doc
            value = _parse(text, at, end)
            if value is _BAD:
                return doc
            doc.fields[key] = value
            at = end
        at = _skip(text, at)
        c = text[at:at + 1]
        if c == ",":
            at += 1
        elif c == "}":
            doc.closed = True
            return doc
        else:
            return doc


def read_document(text):
    """The first JSON object in `text` as far as it was written: what closed, never repaired or completed."""
    for m in re.finditer(r"\{", text):
        doc = _object_at(text, m.start())
        if doc is not None:
            return doc
    return None


def pointed(written):
    """shown.rs `Name::pointed`: a handle bare or in square brackets -> (prefix, ordinal), else None."""
    if not isinstance(written, str):
        return None
    written = written.strip()
    if written.startswith("[") and written.endswith("]"):
        written = written[1:-1].strip()
    m = re.fullmatch(r"([a-z]+)([1-9][0-9]*)", written)
    return (m.group(1), int(m.group(2))) if m else None


def ended_of(finish):
    """How the contract reads a finish reason (llm_port.rs `Finish::of`, then ask.rs `ended`): complete, cut_off
    or declined."""
    reason = (finish or "").lower()
    if any(marker in reason for marker in CUT_OFF_MARKERS) or "content_filter" in reason:
        return "cut_off"
    return "declined" if reason == "refusal" else "complete"


def read(reply, shown):
    """What an answer says of each of `shown`, a list of (index, source): {index: row} with row
    ("answered", [sentence numbers]), ("contradicted", text) or ("unanswered", why), why one of not_read,
    cut_off, missing, declined."""
    ended = ended_of(reply.get("finish"))
    indices = [i for i, _ in shown]

    def everyone(why):
        return {i: ("unanswered", why) for i in indices}

    def unfinished():
        return {"complete": "not_read", "cut_off": "cut_off", "declined": "declined"}[ended]

    if ended == "declined":
        return everyone("declined")
    doc = read_document(reply.get("content") or "")
    if doc is None:
        return everyone(unfinished())
    entries = doc.fields.get("entries")
    if not isinstance(entries, list):
        if "entries" in doc.fields or not doc.open_list or doc.open_list[0] != "entries":
            return everyone("not_read" if doc.closed else unfinished())
        entries = doc.open_list[1]
    sentence_counts = {i: len(source.sentences) for i, source in shown}
    written = {i: [] for i in indices}
    for element in entries:
        if not isinstance(element, dict):
            continue
        name = pointed(element.get("of"))
        if not name or name[0] != "r" or name[1] - 1 not in written:
            continue
        index, kept = name[1] - 1, []
        if isinstance(element.get("keep"), list):
            for handle in element["keep"]:
                sentence = pointed(handle)
                if sentence and sentence[0] == "u" and 1 <= sentence[1] <= sentence_counts[index] \
                        and sentence[1] not in kept:
                    kept.append(sentence[1])
        written[index].append(kept)
    rows = {}
    for index in indices:
        mine = written[index]
        if not mine:
            rows[index] = ("unanswered", "missing" if doc.closed else unfinished())
        elif any(kept != mine[0] for kept in mine):
            rows[index] = ("contradicted", f"r{index + 1} has two entries that differ.")
        else:
            rows[index] = ("answered", mine[0])
    return rows


# ---------- asking again (model_contract/ask.rs `ask_each`, `ask_alone`, `alone`) ----------
def told(row, index):
    """The correction a row is due, or None."""
    kind, why = row
    if kind == "contradicted":
        return CONTRADICTED.format(what=why)
    return {"not_read": NOT_READ, "cut_off": CUT_OFF,
            "missing": MISSING.format(handle=f"r{index + 1}")}.get(why)


def ask_each(question, asked, send, enum=True):
    """Asks which sentences of each of `asked` to keep, in as few requests as settle them; `send(messages,
    schema)` returns one reply. Returns the settlement of each source (answered, or unanswered and so kept
    whole) and what the asking cost."""
    raw, log, rows = [], [], {}
    count = {"resends": 0, "corrections": 0}
    contradicted, first = set(), []

    def post(messages, shown):
        reply = send(messages, schema(shown, enum))
        raw.append(reply)
        usage = reply.get("usage") or {}
        log.append({"shown": [f"r{i + 1}" for i, _ in shown], "content": reply.get("content"),
                    "finish": reply.get("finish"), "prompt_tokens": usage.get("prompt_tokens"),
                    "secs": reply.get("secs")})
        return reply

    def note(reply, shown):
        got = read(reply, shown)
        contradicted.update(i for i, row in got.items() if row[0] == "contradicted")
        if not first:
            first.append({i for i, row in got.items() if row[0] == "answered"})
        return got

    def alone(index):
        shown = [(index, asked[index])]
        messages = render(question, shown)
        reply = post(messages, shown)
        row = note(reply, shown)[index]
        if row == ("unanswered", "cut_off"):
            count["resends"] += 1
            reply = post(messages, shown)
            row = note(reply, shown)[index]
        if row[0] != "answered":
            due = told(row, index)
            if due:
                count["corrections"] += 1
                again = messages + [{"role": "assistant", "content": reply.get("content") or ""},
                                    {"role": "user", "content": due}]
                row = note(post(again, shown), shown)[index]
        rows[index] = row

    queue = deque([list(range(len(asked)))] if asked else [])
    while queue:
        batch = queue.popleft()
        if len(batch) == 1:
            alone(batch[0])
            continue
        shown = [(i, asked[i]) for i in batch]
        reply = post(render(question, shown), shown)
        got = note(reply, shown)
        accepted = [i for i in batch if got[i][0] == "answered"]
        rows.update({i: got[i] for i in accepted})
        rest = [i for i in batch if i not in accepted]
        if not rest:
            continue
        if ended_of(reply.get("finish")) == "complete" and accepted:
            queue.appendleft(rest)
        else:
            size = max(len(batch) // 2, 1)
            for chunk in reversed([rest[i:i + size] for i in range(0, len(rest), size)]):
                queue.appendleft(chunk)
    return {"rows": [rows[i] for i in range(len(asked))], "first": first[0] if first else set(),
            "contradicted": contradicted, "raw": raw, "log": log, **count}


# ---------- the notes (research_notes.rs `block`, `notes_of`) ----------
def blocks_of(sources, settled):
    """Each source's block of the notes. `settled` holds one row for each source that has sentences, in order."""
    rows, out = iter(settled), []
    for source in sources:
        row = next(rows) if source.sentences else None
        if row and row[0] == "answered":
            lines = [source.sentences[n - 1] for n in sorted(set(row[1]))]
            entries = [f"- {line}" for line in lines] or [NOTHING_KEPT]
        else:
            entries = [f"- {line}" for line in source.sentences]
        out.append("\n".join([f"{source.heading}:", *entries]))
    return out


def notes_of(sources, settled):
    return "\n\n".join(blocks_of(sources, settled))


def message_of(sources, settled):
    return f"{NOTES_HEADER}\n\n{notes_of(sources, settled)}"


# ---------- one turn ----------
def turn(ctx, mode, send, window):
    """One record: the notes request asked and settled on `ctx`'s results, and what the notes hold."""
    messages, results = transcript(ctx)
    sources = sources_of(ctx, results)
    asked = [s for s in sources if s.sentences]
    got = ask_each(question_of(messages, mode), asked, send)
    message = message_of(sources, got["rows"])
    chars_before = sum(len(text) for _, text, _ in results)
    used = len(message) < chars_before
    after = message if used else "\n".join(text for _, text, _ in results)
    blocks = dict(zip((s.label for s in sources), blocks_of(sources, got["rows"])))
    by_label = {s.label: s for s in sources}
    facts, split = {}, []
    for spellings in facts_of(ctx):
        label = next((l for l in sorted(by_label) if any(v in ctx["texts"][l][0] for v in spellings)), None)
        if label is None:
            continue
        if not any(v.lower() in sentence.lower() for v in spellings for sentence in by_label[label].sentences):
            split.append(spellings[0])
            continue
        kept = any(v.lower() in after.lower() for v in spellings)
        cited = kept and (not used or any(v.lower() in blocks[label].lower() for v in spellings))
        facts[spellings[0]] = (kept, cited)
    replies = got["raw"]
    s = lambda key: sum(((r.get("timings") or {}).get(key) or 0) for r in replies)
    prompts = [(r.get("usage") or {}).get("prompt_tokens") or 0 for r in replies]
    return {"key": ctx["key"], "set": ctx["set"], "arm": "LINE" if mode == "line" else "SHIPPED",
            "question_mode": mode, "window": window, "max_tokens": max_tokens_for(window),
            "results": len(results), "sources": len(asked), "sentences": sum(len(a.sentences) for a in asked),
            "results_chars": chars_before, "after_chars": len(message), "used": used, "facts": len(facts),
            "kept": sum(k for k, _ in facts.values()), "cited": sum(c for _, c in facts.values()),
            "lost": [f for f, (k, _) in facts.items() if not k], "split_facts": split,
            "answered_first": len(got["first"]), "kept_whole": sum(1 for row in got["rows"] if row[0] != "answered"),
            "contradicted": len(got["contradicted"]), "requests": len(replies),
            "cut_off": sum(1 for r in replies if ended_of(r.get("finish")) == "cut_off"), "resends": got["resends"],
            "corrections": got["corrections"], "prompt_tokens": max(prompts, default=0), "prompt_new": s("prompt_n"),
            "output_tokens": s("predicted_n"), "secs": round(sum(r.get("secs") or 0 for r in replies), 1),
            "notes": message, "replies": got["log"]}


def run(args):
    path = os.path.join(args.out, "records.jsonl")
    done = {(r["key"], r["arm"]) for r in base.read_out(path)}
    out = open(path, "a", encoding="utf-8")
    ctxs = grouped.contexts(args)
    if args.pack > 1:
        ctxs = packed(ctxs, args.pack)
    print(f"{len(ctxs)} turns", flush=True)
    send = sender(args.window)
    arm = "LINE" if args.question == "line" else "SHIPPED"
    for ctx in ctxs:
        if (ctx["key"], arm) in done:
            continue
        rec = turn(ctx, args.question, send, args.window)
        out.write(json.dumps({"model": base.running_model(), **rec}) + "\n")
        out.flush()
        print(f"{ctx['key']} {arm} {rec['kept']}/{rec['facts']} cited {rec['cited']} "
              f"answered_first {rec['answered_first']}/{rec['sources']} kept_whole {rec['kept_whole']} "
              f"requests {rec['requests']} {rec['results_chars']}->{rec['after_chars']} {rec['secs']}s", flush=True)


def dry(args):
    """Builds the first turn's first request and prints it; nothing is sent or written."""
    ctxs = grouped.contexts(args)
    if args.pack > 1:
        ctxs = packed(ctxs, args.pack)
    if not ctxs:
        sys.exit("no turns to build")
    messages, results = transcript(ctxs[0])
    asked = [s for s in sources_of(ctxs[0], results) if s.sentences]
    shown = list(enumerate(asked))
    request = render(question_of(messages, args.question), shown)
    print(request[0]["content"], "\n")
    print(request[1]["content"][:3000], "\n")
    print(f"{len(asked)} sources, {sum(len(a.sentences) for a in asked)} sentences, "
          f"max_tokens {max_tokens_for(args.window)}")
    print(json.dumps(schema(shown, enum=True)))


def summary(args):
    recs = base.read_out(os.path.join(args.out, "records.jsonl"))
    print(f"model: {replay_model.one_model(recs)}")
    arms = list(dict.fromkeys(r["arm"] for r in recs))
    keys = set.intersection(*({r["key"] for r in recs if r["arm"] == a} for a in arms)) if arms else set()
    print(f"{len(keys)} turns\n")
    print("| Arm | Facts kept | with citation | Turns losing a fact | Facts the splitter cut | "
          "Sources answered first time | Sources kept whole | Notes used | After / before chars | Requests | "
          "Cut off | Prompt tokens processed | Output tokens | Seconds | Median s/turn |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for subset in ("all", "long", "short", "pack"):
        for a in arms:
            rs = [r for r in recs if r["arm"] == a and r["key"] in keys and subset in ("all", r["set"])]
            if not rs:
                continue
            name = a if subset == "all" else f"{a} ({subset})"
            print(f"| {name} | {sum(r['kept'] for r in rs)}/{sum(r['facts'] for r in rs)} | "
                  f"{sum(r['cited'] for r in rs)} | {sum(1 for r in rs if r['lost'])} | "
                  f"{sum(len(r['split_facts']) for r in rs)} | "
                  f"{sum(r['answered_first'] for r in rs)}/{sum(r['sources'] for r in rs)} | "
                  f"{sum(r['kept_whole'] for r in rs)} | {sum(1 for r in rs if r['used'])}/{len(rs)} | "
                  f"{sum(r['after_chars'] for r in rs) / max(sum(r['results_chars'] for r in rs), 1):.2f} | "
                  f"{sum(r['requests'] for r in rs)} | {sum(r['cut_off'] for r in rs)} | "
                  f"{sum(r['prompt_new'] for r in rs):,} | {sum(r['output_tokens'] for r in rs):,} | "
                  f"{sum(r['secs'] for r in rs):,.0f} | {statistics.median(r['secs'] for r in rs):.1f} |")
    for a in arms:
        lost = [f"{r['key']} {r['lost']}" for r in recs if r["arm"] == a and r["key"] in keys and r["lost"]]
        print(f"\n{a} lost: {lost or 'none'}")
    claude.print_spend(os.path.join(args.out, "claude-spend.jsonl"))


def main():
    global RESULT_SIZE, RESULTS_CHARS
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--calls", action="append", default=[])
    ap.add_argument("--question", choices=("message", "line"), default="message",
                    help="ask with the whole last user message (SHIPPED) or its question line (LINE)")
    ap.add_argument("--window", type=int, default=16384, help="the context window the turn is sized to")
    ap.add_argument("--pack", type=int, default=1, help="long questions asked together in one turn")
    ap.add_argument("--result-size", type=int, default=RESULT_SIZE)
    ap.add_argument("--results-chars", type=int, default=RESULTS_CHARS)
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--dry", action="store_true", help="print the first turn's first request and exit")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    claude.add_arguments(ap)
    args = ap.parse_args()
    RESULT_SIZE, RESULTS_CHARS = args.result_size, args.results_chars
    os.makedirs(args.out, exist_ok=True)
    if args.summary:
        return summary(args)
    base.DRY = args.dry
    if args.claude:
        claude.use(args.claude, os.path.join(args.out, "claude-spend.jsonl"), args.budget, thinking=args.thinking)
        base.CLAUDE = claude
    else:
        base.URL = args.llama
    for p in args.rubric:
        base.items.update({i["id"]: i for i in json.load(open(p, encoding="utf-8"))["items"]})
    if args.dry:
        return dry(args)
    run(args)


if __name__ == "__main__":
    main()
