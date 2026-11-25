"""Offline replay of the app's note step (chat/tool_loop/research_notes.rs) on saved turns.

The saved reading prompts' passages stand in for what a research turn has read: the message keeps its
instructions and question with the Knowledge Base Results block taken out, and the passages come back as the
results of the turn's calls (`semantic_search` for the question, then `get_document` for the section a result
opens with), RESULT_SIZE passages to a result, in label order, while they stay within RESULTS_CHARS: the share
of a window the loop's results reach before they are condensed, less the room the message and the reservation take.

What is replayed is the request the app sends when those results pass 60% of the window. Every labelled passage is
a source named r1, r2, ... and its sentences u1, u2, ...; one request asks the model, in plain lines with no
schema, which sentences of each source to keep, by handle. The answer is read as the app reads it (one line for each
result that keeps something, handles checked against what was shown). A source the answer does not settle is asked
about again in smaller requests, alone with one resend after a cut-off, sent with twice the output room, and with
one correction, sent with the room of the request it follows; one still unsettled is kept whole. The notes are the
kept sentences, unchanged, under the source's heading, and are used only when they are shorter than the results. A
fact is kept when the text after condensing holds one of the rubric's spellings for it (case-insensitive), of the
facts whose passage is among the results; it is cited when it sits in its own passage's block of the notes.

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
            "and anything that does not bear on my question. Do not answer my question yet. Answer with one line for "
            "each result that has sentences to keep: the result's handle, a colon, then the handle of each of those "
            "sentences, written out in full and separated by spaces. Leave out a result with nothing to keep. If "
            "nothing is worth keeping, answer `none`. Write nothing else.")
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


def max_tokens_for(window, scale=1):
    """context_assembler/budget.rs `plan`: the default output reservation times the request's output_scale, held to
    half the window."""
    return min(min(window // 4, OUTPUT_RESERVATION_CAP) * scale, window // 2)


def body(model, messages, window, scale=1):
    return {"model": model, "messages": messages, **SAMPLING, "repeat_penalty": 1.0,
            "max_tokens": max_tokens_for(window, scale), "chat_template_kwargs": {"enable_thinking": False}}


def sender(window):
    def send(messages, scale=1):
        return base.call(body(base.running_model(), messages, window, scale), "condense")
    return send


# ---------- reading an answer (model_contract/shape/shapes.rs `read_keeps`) ----------
RANGE_SEPARATORS = ("-", "--", "–", "—", "..", "...", "…")
ITEM, OWN = "r", "u"
U32_MAX = 4_294_967_295
U32_DIGITS = len(str(U32_MAX))
# Unicode's White_Space, which Rust's trim and char::is_whitespace use; Python's isspace adds U+001C to U+001F.
WHITE_SPACE = "".join(map(chr, [*range(0x09, 0x0E), 0x20, 0x85, 0xA0, 0x1680, *range(0x2000, 0x200B), 0x2028, 0x2029,
                                0x202F, 0x205F, 0x3000]))


def pointed(written):
    """shown.rs `Name::pointed`: a handle bare or in square brackets -> (prefix, ordinal), else None."""
    written = written.strip(WHITE_SPACE)
    if len(written) >= 2 and written.startswith("[") and written.endswith("]"):
        written = written[1:-1].strip(WHITE_SPACE)
    m = re.fullmatch(r"([a-z]+)([1-9][0-9]*)", written)
    if not m or len(m.group(2)) > U32_DIGITS or int(m.group(2)) > U32_MAX:
        return None
    return m.group(1), int(m.group(2))


def says_none(line):
    """shapes.rs `says_none`: the word for absence, in any case and with whatever punctuation or markup around it."""
    kept = [at for at, char in enumerate(line) if char.isascii() and char.isalnum()]
    return bool(kept) and line[kept[0]:kept[-1] + 1].lower() == "none"


def tokens_of(line):
    """shapes.rs `tokens_of`: the runs of ASCII letters and digits in a line, with where each starts."""
    return [(m.start(), m.group()) for m in re.finditer(r"[A-Za-z0-9]+", line)]


def range_follows(line, tokens, at):
    """shapes.rs `range_follows`: whether what stands between token `at` and the next one is a range sign."""
    if at + 1 >= len(tokens):
        return False
    start, token = tokens[at]
    between = "".join(c for c in line[start + len(token):tokens[at + 1][0]] if c not in WHITE_SPACE)
    return between in RANGE_SEPARATORS


def all_digits(token):
    return re.fullmatch(r"[0-9]+", token) is not None


def u32_or_max(digits):
    """shapes.rs `n.parse().unwrap_or(u32::MAX)` on a run of ASCII digits: leading zeros count for nothing, and a
    number past u32::MAX reads as u32::MAX."""
    significant = digits.lstrip("0")
    return U32_MAX if len(significant) > U32_DIGITS else min(int(significant or "0"), U32_MAX)


class Entry:
    """shapes.rs `Entry`: an item being read and what has been said under it so far."""

    def __init__(self, item, own_count):
        self.item, self.own_count = item, own_count
        self.kept, self.unplaced = [], False
        self.names_own = self.says_none = self.other_words = False

    def keep(self, first, last):
        self.names_own = True
        if self.unplaced:
            return
        if last < first:
            self.unplaced = True
            return
        for ordinal in range(first, last + 1):
            if not 1 <= ordinal <= self.own_count:
                self.unplaced = True
                return
            if ordinal not in self.kept:
                self.kept.append(ordinal)

    def row(self):
        if self.unplaced:
            return ("unanswered", "unplaced")
        if self.says_none and self.names_own:
            return ("contradicted", f"r{self.item + 1} both says none and names what it keeps.")
        if not self.names_own and self.other_words:
            return ("unanswered", "not_read")
        return ("answered", self.kept)


class KeepsReader:
    """shapes.rs `KeepsReader`: the lines of an answer read one by one. An item named on a line that holds none of its
    own handles stays open: the lines after it that name no item are its own."""

    def __init__(self, counts):
        self.counts, self.entries, self.open, self.stray = counts, [], None, False

    def close(self, entry):
        if entry is not None:
            self.entries.append((entry.item, entry.row()))

    def item_of(self, word):
        name = pointed(word)
        if name and name[0] == ITEM and name[1] - 1 in self.counts:
            return name[1] - 1
        return None

    def unplace_item(self, word):
        self.stray = True
        item = self.item_of(word)
        if item is not None:
            self.entries.append((item, ("unanswered", "unplaced")))

    def line(self, line):
        tokens = tokens_of(line)
        words = [token.lower() for _, token in tokens]

        def prefix_of(word):
            name = pointed(word) if word is not None else None
            return name[0] if name else None

        names_item = any(prefix_of(word) == ITEM for word in words)
        opened, self.open = self.open, None
        if names_item:
            self.close(opened)
            current = None
        else:
            current = opened
        at = 0
        while at < len(words):
            word = words[at]
            following = words[at + 1] if at + 1 < len(words) else None
            ranged = range_follows(line, tokens, at)
            prefix = prefix_of(word)
            if prefix == ITEM:
                self.close(current)
                current = None
                to_item = following if prefix_of(following) == ITEM else None
                if ranged and (to_item is not None or (following is not None and all_digits(following))):
                    self.unplace_item(word)
                    if to_item is not None:
                        self.unplace_item(to_item)
                    at += 2
                    continue
                item = self.item_of(word)
                if item is None:
                    self.stray = True
                else:
                    current = Entry(item, self.counts[item])
            elif prefix == OWN:
                first = pointed(word)[1]
                to_own = pointed(following)[1] if prefix_of(following) == OWN else None
                to_number = u32_or_max(following) if following is not None and all_digits(following) else None
                last = (to_own if to_own is not None else to_number) if ranged else None
                if current is None:
                    self.stray = True
                else:
                    current.keep(first, first if last is None else last)
                if last is not None:
                    at += 2
                    continue
            elif current is not None:
                if word == "none":
                    current.says_none = True
                else:
                    current.other_words = True
            at += 1
        if current is not None and (not names_item or not current.names_own):
            self.open = current
        else:
            self.close(current)

    def finish(self, complete):
        entry, self.open = self.open, None
        if entry is None:
            return
        if complete:
            self.close(entry)
        else:
            self.entries.append((entry.item, ("unanswered", "cut_off")))


def join(index, entries):
    """shapes.rs `join`: what the entries of one item say together: equal entries count once, differing ones
    contradict; None when there is no entry."""
    answered, unread = None, None
    for row in entries:
        if row[0] == "answered":
            if answered is not None and answered != row[1]:
                return ("contradicted", f"r{index + 1} has two entries that differ.")
            if answered is None:
                answered = row[1]
        elif unread is None:
            unread = row
    if answered is not None:
        return ("answered", answered)
    return unread


def ended_of(finish):
    """How the contract reads a finish reason (llm_port.rs `Finish::of`, then ask.rs `ended`): complete, cut_off
    or declined."""
    reason = (finish or "").lower()
    if any(marker in reason for marker in CUT_OFF_MARKERS) or "content_filter" in reason:
        return "cut_off"
    return "declined" if reason == "refusal" else "complete"


def read(reply, shown):
    """What an answer says of each of `shown`, a list of (index, source), as shapes.rs `read_keeps` reads it:
    {index: row} with row ("answered", [sentence numbers]), ("contradicted", text) or ("unanswered", why), why one
    of not_read, cut_off, unplaced, declined."""
    ended = ended_of(reply.get("finish"))
    indices = [i for i, _ in shown]
    if ended == "declined":
        return {i: ("unanswered", "declined") for i in indices}
    complete = ended == "complete"
    lines = [line for line in (part.strip(WHITE_SPACE) for part in (reply.get("content") or "").split("\n")) if line]
    if not complete and lines:
        lines.pop()
    reader = KeepsReader({i: len(source.sentences) for i, source in shown})
    none_said = False
    for line in lines:
        none_said = none_said or says_none(line)
        reader.line(line)
    reader.finish(complete)
    if not complete:
        unnamed = ("unanswered", "cut_off")
    elif reader.stray:
        unnamed = ("unanswered", "unplaced")
    elif not reader.entries and not none_said:
        unnamed = ("unanswered", "not_read")
    else:
        unnamed = None
    rows = {}
    for index in indices:
        joined = join(index, [row for item, row in reader.entries if item == index])
        rows[index] = joined or unnamed or ("answered", [])
    return rows


# ---------- asking again (model_contract/ask.rs `ask_each`, `ask_alone`, `alone`) ----------
def told(row, index):
    """ask.rs `told`: the correction a row is due, or None."""
    kind, why = row
    if kind == "contradicted":
        return CONTRADICTED.format(what=why)
    return {"not_read": NOT_READ, "cut_off": CUT_OFF, "unplaced": UNPLACED,
            "missing": MISSING.format(handle=f"r{index + 1}")}.get(why)


def ask_each(question, asked, send):
    """Asks which sentences of each of `asked` to keep, in as few requests as settle them; `send(messages,
    scale)` returns one reply, with `scale` times the output room. Returns the settlement of each source (answered,
    or unanswered and so kept whole) and what the asking cost."""
    raw, log, rows = [], [], {}
    count = {"resends": 0, "corrections": 0}
    contradicted, first = set(), []

    def post(messages, shown, scale=1):
        reply = send(messages, scale)
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
        scale = 1
        reply = post(messages, shown)
        row = note(reply, shown)[index]
        if row == ("unanswered", "cut_off"):
            count["resends"] += 1
            scale = 2
            reply = post(messages, shown, scale)
            row = note(reply, shown)[index]
        if row[0] != "answered":
            due = told(row, index)
            if due:
                count["corrections"] += 1
                again = messages + [{"role": "assistant", "content": reply.get("content") or ""},
                                    {"role": "user", "content": due}]
                row = note(post(again, shown, scale), shown)[index]
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
