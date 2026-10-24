"""Offline A/B for the close-read fix (docs/design/2026-10-24-close-read-precision.md §3; results and the
command lines in evals/retrieval/2026-10-24-CLOSE-READ.md). The helpers and close_read of
scripts/replay_reading_ab.py are copied below verbatim so C0R0 behaves exactly as there (that script runs its
whole flow at import, so it cannot be imported).

Stage 1, the close read, per close-read reading (a verified copy after attribution + ordinal parse):
  C0R0  as built: old instruction, repeat while a pass takes a line, room = window - prompt - notes - reservation.
        e2e readings: read from their own logs; old readings: the A3 records where A3 ran, live otherwise.
  C1R1  §2.1 instruction after the passages, one pass, copy-bound room (2*passage + its hyphen line <= free).
  C2R1  §2.1 sentences before the passages, one pass, copy-bound room.
  C1R0  §2.1 after, one pass, reservation subtracted (only with --arms C1R0; the run script adds it when the
        decision rule says C1R1 lost recall).
Stage 2, the write, on the notes of one stage-1 arm (--notes-arm, default the decision rule's pick), seeds 1, 2:
  W0K0  WRITE as built, covered = cited, quotes all, continuation skipped when room == 0.
  W1K1  §2.5 WRITE, covered = cited or every note sentence stated (§2.4a), quotes packed to the room with the
        restatement bound (§2.4b), continuation skipped when nothing fits.

Modes:  --dry            load every input, build every request of every arm, print counts + one example per arm.
                         The server is NOT called: /props and /tokenize are stubbed (n_ctx from --recipe, tokens =
                         chars // 4) and continuations use the logged answers. Dry numbers are not measurements.
        --stage 1        run stage 1 (--arms, default C0R0,C1R1,C2R1) -> OUT/stage1.jsonl (resumes per reading+arm)
        --stage 2        run stage 2 -> OUT/stage2.jsonl (resumes per reading+arm+seed)
        --summary        print the stage-1 and stage-2 tables and the design's decision rule (§3.4)
        --decision       print the decision as one JSON line (the run script reads it)
Inputs: --rubric (repeatable), --old CALLS (old A/B prompts; run = file stem), --e2e CALLS (new e2e prompts;
        run = 'e2e-' + stem; a growing .part file is fine: only items whose write request is logged are taken),
        --pass1 / --a3 (the old A/B result records), --out DIR, --llama URL, --only qid,qid
Room and token counts come from the server (/props n_ctx, /tokenize) only; HTTP calls carry no timeout.
"""
import argparse, collections, copy, json, os, re, sys, time, urllib.request

# ---------- copied verbatim from scripts/replay_reading_ab.py ----------
READ_OLD = ("Before answering, go through every numbered passage above in order and write one line for each: its label, "
            "then the sentences of that passage that bear on my question, copied exactly as written and in the passage's "
            "own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. "
            "Account for every label. Write only these lines; the answer comes after.")
READ_NEW = ("Before answering, go through every numbered passage above in order and write one line for each, beginning "
            "with its label in brackets exactly as shown above, then the sentences of that passage that bear on my "
            "question, copied exactly as written and in the passage's own language. For a passage that says nothing "
            "bearing on my question, write its label and a single hyphen. Account for every label, and write nothing "
            "after the last line; the answer comes after.")
WRITE_NEW = ("Now answer my question directly, in the passages' own words, keeping their dates, figures and names as "
             "given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the "
             "passages, and say nothing about passages that do not bear on my question.")
CLOSE_READ = "Here again are the passages you marked with a hyphen. Read each on its own and write its line in the same form.\n\n{passages}"
CONTINUE = ("Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. "
            "Continue your answer with what each of them says, in the same form as above, citing each with its label; "
            "do not repeat what you have written. If one of those notes was mistaken, write nothing for it.")
SAMPLING_KEYS = ("temperature", "top_p", "top_k", "presence_penalty", "repeat_penalty", "max_tokens", "chat_template_kwargs",
                 "parallel_tool_calls")
# ---------- the design's texts (§3.5) ----------
CLOSE_C1 = ("Here again, each on its own, are the passages you marked with a hyphen.\n\n{passages}\n\n"
            "Write one line for each of these passages, as before: its label, then the sentences of that passage "
            "that bear on my question, copied exactly as written and in the passage's own language — or its label and "
            "a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.")
CLOSE_C2 = ("Here again, each on its own, are the passages you marked with a hyphen. Write one line for each, as before: "
            "its label, then the sentences of that passage that bear on my question, copied exactly as written and in the "
            "passage's own language — or its label and a single hyphen when it says nothing bearing on my question, "
            "however near its subject. Write only these lines.\n\n{passages}")
WRITE_W1 = ("Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. "
            "Cover every passage your notes quote and cite each with its label; a sentence that several of your notes quote, "
            "write once, citing one of them. Do not describe the notes or the passages, and say nothing about passages that "
            "do not bear on my question.")
WRITE_ENDINGS = ("Leave out a note that was mistaken.", "do not bear on my question.")
NEW_ARMS = {"C1R1": (CLOSE_C1, True), "C2R1": (CLOSE_C2, True), "C1R0": (CLOSE_C1, False)}
STAGE1_ARMS = ("C0R0", "C1R1", "C2R1", "C1R0")
STAGE2_ARMS = ("W0K0", "W1K1")
SEEDS = (1, 2)

items = {}


def norm(s):
    return re.sub(r"\s+", " ", s).strip()


def text_of(m):
    c = m.get("content", "")
    if isinstance(c, list):
        c = " ".join(x.get("text", "") for x in c if isinstance(x, dict))
    return c or ""


def passages(user):
    """label -> (normalised text, raw text) from the Knowledge Base Results block."""
    st = re.search(r"(?m)^\[1\] Document:", user).start()
    en = re.search(r"(?m)^Question: ", user).start()
    out = {}
    for b in re.split(r"\n(?=\[\d+\] Document:)", user[st:en]):
        m = re.match(r"\[(\d+)\]", b)
        if m:
            c = b.find("Content:")
            raw = (b[c + 8:] if c >= 0 else b).strip()
            out[int(m.group(1))] = (norm(raw), raw)
    return out


def parse_line(line):
    """(label, note|None) with the ordinal fix: '1. [1] -' and '1. -' are label 1, hyphen."""
    m = re.match(r"^\s*\[?(\d+)\]?\s*[:.\-–—]?\s*(.*)$", line)
    if not m or not line.strip().startswith(("[",) + tuple("0123456789")):
        return None
    label, rest = int(m.group(1)), m.group(2).strip()
    m2 = re.match(r"^\[(\d+)\]\s*[:.\-–—]?\s*(.*)$", rest)
    if m2:
        label, rest = int(m2.group(1)), m2.group(2).strip()
    return label, (rest if re.search(r"\w", rest) else None)


def whole_lines(text, finish):
    """As reading.rs: a reply cut at the output limit loses its partial last line (the sweep asks again)."""
    if finish in ("length", "max_tokens", "incomplete"):
        return text[:text.rfind("\n")] if "\n" in text else ""
    return text


def parse_notes(text):
    out = {}
    for line in text.splitlines():
        p = parse_line(line)
        if p:
            out[p[0]] = p[1]
    return out


def sentences(note):
    return [norm(p).strip('"“”') for p in re.split(r"(?<=[.!?])\s+", note) if norm(p)]


def locate(note, texts):
    ss = sentences(note)
    return [l for l, (t, _) in texts.items() if ss and all(s in t for s in ss)]


def attribute(raw, texts):
    """Part 1: a note verbatim only in one other passage moves there. -> lines, noted, relocations"""
    lines = {l: n for l, n in raw.items() if l in texts}
    noted, moved = set(), []
    for label, note in raw.items():
        if not note or label not in texts:
            continue
        where = locate(note, texts)
        if label in where:
            noted.add(label)
        elif len(where) == 1:
            lines[where[0]] = note
            noted.add(where[0])
            moved.append((label, where[0]))
    return lines, noted, moved


def render(lines):
    return "\n".join(f"[{l}] {n if n else '-'}" for l, n in sorted(lines.items()))


def canonical_text(model_text, lines):
    """The model's own text when it is nothing but note lines in canonical form; else the lines rendered."""
    c = render(lines)
    return model_text if norm(model_text) == norm(c) else c


def cited(text):
    out = set()
    for grp in re.findall(r"\[(\d+(?:\s*,\s*\d+)*)\]", text):
        out |= {int(x) for x in re.split(r"\s*,\s*", grp)}
    return out


def facts(qid):
    t = items[qid]["turns"][-1]
    r = t.get("rubric", t)
    return [f if isinstance(f, list) else [f] for f in (r.get("must_mention") or [])]


def fact_labels(qid, texts):
    return {next((l for l, (t, _) in texts.items() if any(v in t for v in vs)), None) for vs in facts(qid)} - {None}


def missing(qid, text):
    low = text.lower()
    return [f[0] for f in facts(qid) if not any(a.lower() in low for a in f)]


def abstains(text):
    """The scorer's notion, approximately: an opening that declares the answer unavailable."""
    head = text.strip()[:220].lower()
    return any(p in head for p in ("not available in the provided", "does not contain", "contains no information",
                                   "do not contain", "no information"))


def stops_for(texts):
    n = max(texts) + 1
    return [f"[{n}]", f"\n{n}."]


def reading_metrics(lines, noted, fl):
    return {"recall": len(fl & noted), "of": len(fl), "missed": sorted(fl - noted), "false_quotes": sorted(noted - fl),
            "hyphens": sum(1 for n in lines.values() if n is None), "verified": len(noted)}


# ---------- server (no timeouts; dry mode stubs /props and /tokenize and never calls the model) ----------
URL, DRY, N_CTX, DRY_SENT = None, False, None, []


def http_json(path, body=None):
    req = URL + path if body is None else urllib.request.Request(
        URL + path, json.dumps(body).encode(), {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req))


def tokens(text):
    if DRY:
        return len(text) // 4  # DRY STUB, not a measurement
    return len(http_json("/tokenize", {"content": text})["tokens"])


def n_ctx():
    global N_CTX
    if N_CTX is None:
        N_CTX = http_json("/props")["default_generation_settings"]["n_ctx"]
    return N_CTX


def within(reserved, room):
    """reading.rs `within`: the reservation, or the room when that is less."""
    room = max(room, 0)
    return min(reserved, room) if reserved else room


def call(req, kind, **extra):
    """The original's call; in dry mode the built request is recorded and None returned."""
    r = copy.deepcopy(req)
    for k in ("stream_options", "return_progress", "id_slot"):
        r.pop(k, None)
    r["stream"] = False
    r.update(extra)
    if DRY:
        DRY_SENT.append((kind, r))
        return None
    t = time.time()
    resp = json.load(urllib.request.urlopen(urllib.request.Request(
        URL + "/v1/chat/completions", json.dumps(r).encode(), {"Content-Type": "application/json"})))
    ch = resp["choices"][0]
    msg = ch["message"]
    return {"content": msg.get("content") or "", "tool_calls": msg.get("tool_calls") or [], "finish": ch.get("finish_reason"),
            "secs": round(time.time() - t, 1), "usage": resp.get("usage"),
            "timings": {k: resp.get("timings", {}).get(k) for k in ("cache_n", "prompt_n", "prompt_ms", "predicted_n", "predicted_ms")}}


def req_record(shown, r, max_tokens=None, measured=None):
    out = {"shown": shown, "max_tokens": max_tokens, "measured_prompt": measured}
    if r is not None:
        u, t = r.get("usage") or {}, r.get("timings") or {}
        out.update(finish=r["finish"], prompt_tokens=u.get("prompt_tokens"), prompt_new=t.get("prompt_n"),
                   cache_n=t.get("cache_n"), output_tokens=u.get("completion_tokens"), secs=r["secs"])
    return out


def as_r(reply, secs):
    """A logged reply (calls file) in call()'s shape."""
    return {"content": reply.get("content") or "", "finish": reply.get("finish_reason"), "usage": reply.get("usage"),
            "timings": reply.get("timings") or {}, "secs": round(secs, 1), "tool_calls": reply.get("tool_calls") or []}


def is_whole(note, t):
    """Report split only: the note equals the passage, or the passage after its first (heading) line."""
    n = norm(note).strip('"“”')
    return n == t[0] or n == norm(t[1].split("\n", 1)[-1])


def take(lines, l, n, texts, fl, part, i, taken):
    lines[l] = n
    taken.append({"label": l, "position": i, "part": len(part), "fact": l in fl, "whole": is_whole(n, texts[l])})


# ---------- stage 1 arms ----------
def close_read_built(ctx):
    """C0R0 live: the original close_read, unchanged in behaviour, with each request recorded."""
    read_req, texts, fl = ctx["read_req"], ctx["texts"], ctx["fl"]
    lines, text, prompt_tokens = dict(ctx["lines0"]), ctx["text0"], ctx["prompt_tokens"]
    reserve = read_req.get("max_tokens") or 0
    passes, reqs, taken = 0, [], []
    while True:
        hyph = sorted(l for l, n in lines.items() if n is None)
        if not hyph:
            break
        room = n_ctx() - prompt_tokens - tokens(text) - tokens(CLOSE_READ.format(passages="")) - reserve
        batches, cur, used = [], [], 0
        for l in hyph:
            t = tokens(f"[{l}] {texts[l][1]}\n\n")
            if t > room:
                continue  # one passage alone exceeds the room: it cannot be re-presented
            if used + t > room:
                batches.append(cur); cur, used = [], 0
            cur.append(l); used += t
        if cur:
            batches.append(cur)
        if not batches:
            break
        passes += 1
        flipped = 0
        for batch in batches:
            block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in batch)
            r = call(read_req, "C0R0", stop=stops_for(texts), messages=read_req["messages"] + [
                {"role": "assistant", "content": text},
                {"role": "user", "content": CLOSE_READ.format(passages=block)}])
            reqs.append(dict(req_record(batch, r, reserve), passno=passes))
            if r is None:
                continue
            for l, n in parse_notes(whole_lines(r["content"], r["finish"])).items():
                if l in batch and n and l in locate(n, texts) and lines.get(l) is None:
                    take(lines, l, n, texts, fl, batch, batch.index(l), taken)
                    flipped += 1
        if not flipped:
            break
        text = render(lines)
    return lines, text, reqs, taken, {"passes": passes}


def close_read_once(ctx, instruction, copy_bound, kind):
    """§3.5: one showing per hyphen passage. copy_bound: a part fits when 2*passages + their label lines <= free and
    the reply may use free - shown (capped by the reservation); otherwise the built room (free - reservation)."""
    read_req, texts, fl = ctx["read_req"], ctx["texts"], ctx["fl"]
    lines, text = dict(ctx["lines0"]), ctx["text0"]
    reserve = read_req.get("max_tokens") or 0
    hyph = sorted(l for l, n in lines.items() if n is None)
    bare = ctx["prompt_tokens"] + tokens(text) + tokens(instruction.format(passages=""))
    free = n_ctx() - bare
    room = free if copy_bound else free - reserve
    parts, cur, used = [], [], 0
    for l in hyph:
        t = tokens(f"[{l}] {texts[l][1]}\n\n")
        cost = 2 * t + tokens(f"[{l}] -\n") if copy_bound else t
        if cost > room:
            continue
        if used + cost > room:
            parts.append(cur); cur, used = [], 0
        cur.append(l); used += cost
    if cur:
        parts.append(cur)
    reqs, taken = [], []
    for part in parts:
        block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in part)
        shown_tokens = tokens(block + "\n\n")
        max_out = within(reserve, free - shown_tokens) if copy_bound else reserve
        r = call(read_req, kind, stop=stops_for(texts), max_tokens=max_out, messages=read_req["messages"] + [
            {"role": "assistant", "content": text},
            {"role": "user", "content": instruction.format(passages=block)}])
        reqs.append(dict(req_record(part, r, max_out, bare + shown_tokens), passno=1))
        if r is None:
            continue
        got = parse_notes(whole_lines(r["content"], r["finish"]))
        for i, l in enumerate(part):
            n = got.get(l)
            if n and l in locate(n, texts):
                take(lines, l, n, texts, fl, part, i, taken)
    return lines, (render(lines) if taken else text), reqs, taken, {"passes": 1 if parts else 0, "free": free, "room": room}


def c0r0_from_log(ctx):
    """e2e readings: the built close read as logged; the notes after it are the write request's notes."""
    texts, fl = ctx["texts"], ctx["fl"]
    lines, reqs, taken, prev_last, passno = dict(ctx["lines0"]), [], [], None, 0
    for c in ctx["close_calls"]:
        last = text_of(c["request"]["messages"][-1])
        shown = [int(x) for x in re.findall(r"(?:^|\n\n)\[(\d+)\] ", last.split("same form.\n\n", 1)[-1])]
        if shown and (prev_last is None or shown[0] <= prev_last):
            passno += 1
        prev_last = shown[-1] if shown else prev_last
        r = as_r(c["reply"], (c.get("duration_ms") or 0) / 1000)
        reqs.append(dict(req_record(shown, r, c["request"].get("max_tokens")), passno=passno))
        for l, n in parse_notes(whole_lines(r["content"], r["finish"])).items():
            if l in shown and n and l in locate(n, texts) and lines.get(l) is None:
                take(lines, l, n, texts, fl, shown, shown.index(l), taken)
    after = {l: n for l, n in parse_notes(ctx["write_notes"]).items() if l in texts}
    return after, ctx["write_notes"], reqs, taken, {"passes": passno, "simulation_matches_write": after == lines}


def c0r0_from_a3(ctx, a3):
    """Old readings where A3 ran: its logged replies replayed onto A1's lines. The batches were not logged, so a
    line's position is its order in the reply."""
    texts, fl = ctx["texts"], ctx["fl"]
    lines, text, reqs, taken = dict(ctx["lines0"]), ctx["text0"], [], []
    for c in (c for c in a3["calls"] if c["kind"] == "close_read"):
        reqs.append(dict(req_record(None, c), passno=None))
        got = list(parse_notes(whole_lines(c["content"], c["finish"])).items())
        labels = [l for l, _ in got]
        for i, (l, n) in enumerate(got):
            if n and l in texts and l in locate(n, texts) and lines.get(l) is None:
                take(lines, l, n, texts, fl, labels, i, taken)
    noted = {l for l, n in lines.items() if n and l in locate(n, texts)}
    same = reading_metrics(lines, noted, fl)
    fidelity = {k: same[k] == a3["A3"][k] for k in ("recall", "false_quotes", "hyphens")}
    return lines, (render(lines) if taken else text), reqs, taken, {"passes": a3["A3"].get("passes"), "matches_a3": fidelity,
                                                                    "position_from": "reply order"}


def stage1_record(ctx, arm, how, out):
    lines, text, reqs, taken, extra = out
    texts, fl = ctx["texts"], ctx["fl"]
    noted = {l for l, n in lines.items() if n and l in locate(n, texts)}
    before_nf = ctx["noted0"] - fl
    added_nf = sorted((noted - fl) - before_nf)
    s = lambda k: sum((q.get(k) or 0) for q in reqs)
    return {"key": ctx["key"], "run": ctx["run"], "query_id": ctx["qid"], "set": ctx["set"], "source": ctx["source"],
            "arm": arm, "how": how, "passages": len(texts), "fact_labels": sorted(fl),
            "before": reading_metrics(ctx["lines0"], ctx["noted0"], fl), "after": reading_metrics(lines, noted, fl),
            "facts_after": sorted(fl & noted),
            "nonfact_added": [{"label": l, "whole": is_whole(lines[l], texts[l])} for l in added_nf],
            "requests": len(reqs), "prompt_tokens": s("prompt_tokens"), "prompt_new": s("prompt_new"),
            "output_tokens": s("output_tokens"), "secs": round(s("secs"), 1),
            "pass0_took": any(t for t in taken), "taken": taken, "reqs": reqs, "extra": extra,
            "lines": {str(l): n for l, n in sorted(lines.items())}, "text": text}


# ---------- stage 2 ----------
def covered(answer, label, note):
    """K1a: cited, or every sentence of the note is stated in the answer."""
    if label in cited(answer):
        return True
    body = norm(re.sub(r"\[\d+(?:\s*,\s*\d+)*\]", "", answer))
    sents = [norm(s).strip('"“”„«»') for s in sentences(note)]
    return all(s and s in body for s in sents)


def packed_quotes(left, lines, room_after):
    """K1b: the longest label-ordered prefix of left whose instruction fits the room and leaves room for the quotes."""
    out = []
    for l in left:
        q = out + [f'[{l}] "{lines[l]}"']
        instr = CONTINUE.format(quotes=" ".join(q))
        if room_after(instr) < tokens(" ".join(q)):
            break
        out = q
    return out


def repeated_sentences(answer):
    """Design §3.3 report measure: a sentence of >30 characters, citations stripped, appearing twice."""
    body = re.sub(r"\[\d+(?:\s*,\s*\d+)*\]", "", answer)
    cnt = collections.Counter(s.lstrip("-*•# ").strip() for s in sentences(body))
    return [s for s, k in cnt.items() if k > 1 and len(s) > 30]


def write_round2(ctx, lines, notes_text, noted, arm, seed, dry_answer=None):
    """The write as built (W0K0) or with §2.4/§2.5 (W1K1). Max tokens and room from the server, as reading.rs does:
    the write held to within(reservation, room left); notes dropped when the room is smaller than they are."""
    qid, fl, reserve = ctx["qid"], ctx["fl"], ctx["read_req"].get("max_tokens") or 0
    instr = WRITE_NEW if arm == "W0K0" else WRITE_W1
    w = copy.deepcopy(ctx["read_req"])
    w.pop("tool_choice", None)
    w.pop("stop", None)
    w.update(ctx["write_sampling"])
    w["messages"][-1]["content"] = ctx["read_user"]
    notes_tok = tokens(notes_text)
    measured = ctx["prompt_tokens"] + notes_tok + tokens(instr)
    room = n_ctx() - measured
    dropped = room < notes_tok
    if dropped:  # append_notes: the request is left as it was (the pre-#174 request, as the A/B's withdrawn path)
        w["messages"][-1]["content"] = ctx["read_user"].replace("\n\n" + READ_OLD, "").replace("\n\n" + READ_NEW, "")
        measured, room, noted = ctx["prompt_tokens"], n_ctx() - ctx["prompt_tokens"], set()
    else:
        w["messages"] += [{"role": "assistant", "content": notes_text}, {"role": "user", "content": instr}]
    w["seed"] = seed
    w["max_tokens"] = within(reserve, room)
    r = call(w, arm)
    if r is None:
        if dry_answer is None:
            return None
        r = {"content": dry_answer, "finish": "stop", "tool_calls": [], "secs": 0, "usage": {}, "timings": {}}
    answer, secs, conts = r["content"], r["secs"], []
    base_msgs = w["messages"]

    def measure(instr_c):
        return measured + tokens(answer) + tokens(instr_c)

    def left_now():
        if arm == "W0K0":
            return sorted(noted - cited(answer))
        return [l for l in sorted(noted) if not covered(answer, l, lines[l])]

    left = left_now()
    while left and not r["tool_calls"] and r["finish"] != "length":
        if arm == "W0K0":
            instr_c = CONTINUE.format(quotes=" ".join(f'[{l}] "{lines[l]}"' for l in left if lines.get(l)))
            room_c = n_ctx() - measure(instr_c)
            if room_c <= 0:
                conts.append({"skipped": "no room", "left": left})
                break
            quoted = left
        else:
            q = packed_quotes(left, lines, lambda i: n_ctx() - measure(i))
            if not q:
                conts.append({"skipped": "nothing fits", "left": left})
                break
            quoted = left[:len(q)]
            instr_c = CONTINUE.format(quotes=" ".join(q))
            room_c = n_ctx() - measure(instr_c)
        c = call(w, f"{arm}:continue", messages=base_msgs + [{"role": "assistant", "content": answer},
                                                             {"role": "user", "content": instr_c}],
                 tool_choice="none", seed=seed, chat_template_kwargs={"enable_thinking": False},
                 max_tokens=within(reserve, room_c))
        rec = {"left": left, "quoted": quoted, "measured_prompt": measure(instr_c), "room": room_c,
               "max_tokens": within(reserve, room_c)}
        if c is None:  # dry: one continuation built from the logged answer, then stop
            conts.append(rec)
            break
        written = c["content"].strip()
        if written:
            answer = answer.rstrip() + "\n\n" + written
        before = set(left)
        left = left_now()
        added = len(before - set(left))
        u = c.get("usage") or {}
        rec.update(finish=c["finish"], prompt_tokens=u.get("prompt_tokens"), output_tokens=u.get("completion_tokens"),
                   secs=c["secs"], added=added)
        conts.append(rec)
        secs += c["secs"]
        if added == 0 or c["finish"] == "length":
            break
    cites_all = [int(x) for grp in re.findall(r"\[(\d+(?:\s*,\s*\d+)*)\]", answer) for x in re.split(r"\s*,\s*", grp)]
    rep = repeated_sentences(answer)
    return {"key": ctx["key"], "run": ctx["run"], "query_id": qid, "set": ctx["set"], "source": ctx["source"],
            "arm": arm, "seed": seed, "notes_dropped": dropped, "write_finish": r["finish"], "write_max_tokens": w["max_tokens"],
            "missing": missing(qid, answer), "abstains": abstains(answer),
            "pass": not missing(qid, answer) and not abstains(answer), "chars": len(answer),
            "cites_total": len(cites_all), "cites_distinct": len(set(cites_all)),
            "cites_nonfact_total": sum(1 for x in cites_all if x not in fl),
            "cites_nonfact_distinct": len(set(cites_all) - fl), "repeated": rep,
            "continuations": conts, "cont_length": any(c.get("finish") == "length" for c in conts),
            "secs": round(secs, 1), "answer": answer}


# ---------- loading ----------
def load_rows(path):
    """Calls rows in order; a growing file's torn last line is dropped."""
    with open(path) as f:
        for line in f:
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                return


def group_calls(path, run):
    by = collections.OrderedDict()
    for row in load_rows(path):
        c = row.get("call") or {}
        q = c.get("request") or {}
        m = q.get("messages") or []
        if c.get("path") != "/v1/chat/completions" or not m:
            continue
        g = by.setdefault((run, row["query_id"]), {"close": [], "sweeps": []})
        last = text_of(m[-1])
        if q.get("tool_choice") == "none" and len(m) == 2 and last.endswith("the answer comes after."):
            g.setdefault("read", c)
        elif last.startswith("You did not account"):
            g["sweeps"].append(c)
        elif last.startswith("Here again"):
            g["close"].append(c)
        elif last.endswith(WRITE_ENDINGS):
            g.setdefault("write", c)
    return by


def base_ctx(run, qid, source, g):
    read_req = g["read"]["request"]
    read_user = text_of(read_req["messages"][-1])
    texts = passages(read_user)
    w = g["write"]["request"] if "write" in g else {}
    return {"key": f"{run}|{qid}", "run": run, "qid": qid, "source": source, "set": "short" if "short" in run else "long",
            "read_req": read_req, "read_user": read_user, "texts": texts, "fl": fact_labels(qid, texts),
            "write_sampling": {k: w[k] for k in SAMPLING_KEYS if k in w}}


def load(args):
    """-> list of close-read contexts, and a tally of what was skipped and why."""
    ctxs, tally = [], collections.Counter()
    pass1 = {(r["run"], r["query_id"]): r for r in load_rows(args.pass1)} if args.pass1 else {}
    a3 = {(r["run"], r["query_id"]): r for r in load_rows(args.a3)} if args.a3 else {}
    for path in args.old:
        run = os.path.basename(path).split(".")[0]
        for (run_, qid), g in group_calls(path, run).items():
            if "read" not in g or (args.only and qid not in args.only):
                continue
            tally["old prompts"] += 1
            p1 = pass1.get((run, qid))
            p1_read = next((c for c in (p1 or {}).get("calls", []) if c.get("kind") == "read"), None)
            if not p1_read:
                tally["old: no pass-1 reading (skipped; no cold reading is run)"] += 1
                continue
            ctx = base_ctx(run, qid, "old", g)
            text0 = whole_lines(p1_read["content"], p1_read["finish"])
            raw = parse_notes(text0)
            for sc in (c for c in p1.get("calls", []) if c.get("kind") == "sweep"):
                raw.update({l: n for l, n in parse_notes(whole_lines(sc["content"], sc["finish"])).items()
                            if l in ctx["texts"] and l not in raw})
                text0 = text0.rstrip() + "\n" + sc["content"].strip()
            lines1, noted1, _ = attribute(raw, ctx["texts"])
            if not noted1:
                tally["old: withdrawn (no verified copy)"] += 1
                continue
            a = a3.get((run, qid))
            ctx.update(lines0=lines1, noted0=noted1, text0=canonical_text(text0, lines1),
                       prompt_tokens=p1_read["usage"]["prompt_tokens"],
                       a3=a if a and "A3" in a and not a["A3"].get("skipped_full_recall") else None,
                       logged_answer=next((c["content"] for c in p1.get("calls", []) if c.get("kind") == "write:A1"), None))
            ctxs.append(ctx)
    for path in args.e2e:
        run = "e2e-" + os.path.basename(path).split(".")[0]
        for (run_, qid), g in group_calls(path, run).items():
            if "read" not in g or (args.only and qid not in args.only):
                continue
            tally["e2e items with a reading"] += 1
            if "write" not in g:
                tally["e2e: no write request logged (withdrawn, or still running)"] += 1
                continue
            wm = g["write"]["request"]["messages"]
            if len(wm) < 4 or wm[2].get("role") != "assistant":
                tally["e2e: write without notes"] += 1
                continue
            ctx = base_ctx(run, qid, "e2e", g)
            texts = ctx["texts"]
            pre = text_of(g["close"][0]["request"]["messages"][2]) if g["close"] else text_of(wm[2])
            lines0 = {l: n for l, n in parse_notes(pre).items() if l in texts}
            noted0 = {l for l, n in lines0.items() if n and l in locate(n, texts)}
            if not noted0:
                tally["e2e: no verified copy"] += 1
                continue
            raw = parse_notes(whole_lines(g["read"]["reply"].get("content") or "", g["read"]["reply"].get("finish_reason")))
            for sc in g["sweeps"]:
                raw.update({l: n for l, n in parse_notes(whole_lines(sc["reply"].get("content") or "",
                                                                     sc["reply"].get("finish_reason"))).items()
                            if l in texts and l not in raw})
            rec_lines, _, _ = attribute(raw, texts)
            ctx.update(lines0=lines0, noted0=noted0, text0=pre, prompt_tokens=g["read"]["reply"]["usage"]["prompt_tokens"],
                       close_calls=g["close"], write_notes=text_of(wm[2]), pre_matches_reconstruction=rec_lines == lines0,
                       logged_answer=(g["write"].get("reply") or {}).get("content"))
            ctxs.append(ctx)
    return ctxs, tally


def read_out(path):
    return list(load_rows(path)) if os.path.exists(path) else []


# ---------- stage runners ----------
def warm(ctx, notes_text, instruction):
    """One token on the reading prompt + notes, so every live arm is timed on a warm prefix (the first arm would
    otherwise pay the cold prompt alone). Not attributed to any arm."""
    r = call(ctx["read_req"], "warmup", max_tokens=1, messages=ctx["read_req"]["messages"] + [
        {"role": "assistant", "content": notes_text}, {"role": "user", "content": instruction}])
    if r is not None:
        print(f"  warm-up {ctx['key']}: {r['timings'].get('prompt_n')} tokens {r['secs']}s", flush=True)


def run_stage1(args, ctxs):
    path = os.path.join(args.out, "stage1.jsonl")
    done = {(r["key"], r["arm"]) for r in read_out(path)}
    out = open(path, "a")
    for ctx in ctxs:
        recs = []
        live = [a for a in args.arms if (ctx["key"], a) not in done
                and (a in NEW_ARMS or (a == "C0R0" and ctx["source"] == "old" and not ctx.get("a3")))]
        if live:
            warm(ctx, ctx["text0"], CLOSE_C1.format(passages=""))
        for arm in args.arms:
            if (ctx["key"], arm) in done:
                continue
            if arm in NEW_ARMS:
                instr, cb = NEW_ARMS[arm]
                recs.append(stage1_record(ctx, arm, "live", close_read_once(ctx, instr, cb, arm)))
            elif arm == "C0R0":
                if ctx["source"] == "e2e":
                    recs.append(stage1_record(ctx, arm, "log", c0r0_from_log(ctx)))
                elif ctx.get("a3"):
                    recs.append(stage1_record(ctx, arm, "a3", c0r0_from_a3(ctx, ctx["a3"])))
                else:
                    recs.append(stage1_record(ctx, arm, "live", close_read_built(ctx)))
            for r in recs[-1:]:
                out.write(json.dumps(r) + "\n")
                out.flush()
        if recs:
            print(ctx["key"], " | ".join(f"{r['arm']}({r['how']}) {r['after']['recall']}/{r['after']['of']} "
                                         f"nf+{len(r['nonfact_added'])} req {r['requests']} {r['secs']}s" for r in recs), flush=True)


def run_stage2(args, ctxs, notes_arm):
    s1 = {r["key"]: r for r in read_out(os.path.join(args.out, "stage1.jsonl")) if r["arm"] == notes_arm}
    path = os.path.join(args.out, "stage2.jsonl")
    done = {(r["key"], r["notes_arm"], r["arm"], r["seed"]) for r in read_out(path)}
    out = open(path, "a")
    for ctx in ctxs:
        n = s1.get(ctx["key"])
        if not n:
            print(ctx["key"], f"no {notes_arm} notes in stage 1; skipped", flush=True)
            continue
        lines = {int(l): v for l, v in n["lines"].items()}
        noted = {l for l, v in lines.items() if v and l in locate(v, ctx["texts"])}
        res = []
        if any((ctx["key"], notes_arm, a, s) not in done for a in STAGE2_ARMS for s in SEEDS):
            warm(ctx, n["text"], WRITE_NEW)
        for seed in SEEDS:
            for arm in STAGE2_ARMS:
                if (ctx["key"], notes_arm, arm, seed) in done:
                    continue
                r = write_round2(ctx, lines, n["text"], noted, arm, seed)
                r["notes_arm"] = notes_arm
                out.write(json.dumps(r) + "\n")
                out.flush()
                res.append(r)
        if res:
            print(ctx["key"], " | ".join(f"{r['arm']}/s{r['seed']} {'P' if r['pass'] else 'miss'} conts {len(r['continuations'])}"
                                         f"{' LEN' if r['cont_length'] else ''} rep {len(r['repeated'])} {r['secs']}s" for r in res),
                  flush=True)


# ---------- dry run ----------
def abbreviate(req):
    m = req["messages"]
    last = text_of(m[-1])
    return {"messages": [x["role"] for x in m], "max_tokens": req.get("max_tokens"), "stop": req.get("stop"),
            "seed": req.get("seed"), "tool_choice": req.get("tool_choice"), "temperature": req.get("temperature"),
            "last_user_chars": len(last), "last_user_head": last[:260], "last_user_tail": last[-240:]}


def dry(args, ctxs, tally):
    global DRY
    DRY = True
    print(f"DRY RUN: no server call. STUBS: n_ctx = {n_ctx()} (recipe expected_ctx), tokens = chars // 4.\n")
    for k, v in tally.items():
        print(f"  {k}: {v}")
    by_src = collections.Counter((c["source"], c["set"]) for c in ctxs)
    print(f"  close-read readings: {len(ctxs)}  " + ", ".join(f"{s}/{t} {n}" for (s, t), n in sorted(by_src.items())))
    print(f"  of which missed facts before the close read: {sum(1 for c in ctxs if len(c['fl'] & c['noted0']) < len(c['fl']))}")
    e2e = [c for c in ctxs if c["source"] == "e2e"]
    print(f"  e2e: pre-close notes equal to reading+sweeps+attribution: {sum(c['pre_matches_reconstruction'] for c in e2e)}/{len(e2e)}")
    c0 = collections.Counter("log" if c["source"] == "e2e" else ("a3" if c.get("a3") else "live") for c in ctxs)
    print(f"  C0R0 sources: {dict(c0)}\n")
    ex = {}
    print("STAGE 1")
    for arm in STAGE1_ARMS:
        DRY_SENT.clear()
        reqs = shown = took = 0
        fid = collections.Counter()
        for ctx in ctxs:
            if arm == "C0R0":
                if ctx["source"] == "e2e":
                    o = c0r0_from_log(ctx)
                    fid["log simulation = write notes"] += o[4]["simulation_matches_write"]
                elif ctx.get("a3"):
                    o = c0r0_from_a3(ctx, ctx["a3"])
                    fid["a3 replay = A3 record"] += all(o[4]["matches_a3"].values())
                else:
                    o = close_read_built(ctx)
            else:
                instr, cb = NEW_ARMS[arm]
                o = close_read_once(ctx, instr, cb, arm)
            reqs += len(o[2])
            shown += sum(len(q["shown"] or []) for q in o[2])
            took += len(o[3])
        sent = [r for k, r in DRY_SENT if k == arm]
        print(f"  {arm}: requests {reqs} (built now {len(sent)}), passages shown {shown}, lines taken from logs {took}"
              + (f", fidelity {dict(fid)}" if fid else "")
              + ("  [C0R0 live builds pass 1 only: later passes depend on replies]" if arm == "C0R0" else ""))
        if sent:
            ex[arm] = sent[0]
    print("\nSTAGE 2 (dry stand-in notes: the pre-close notes; continuations built from the logged answer)")
    for arm in STAGE2_ARMS:
        DRY_SENT.clear()
        writes = conts = left_tot = quoted_tot = 0
        for ctx in ctxs:
            noted = set(ctx["noted0"])
            for seed in SEEDS:
                r = write_round2(ctx, ctx["lines0"], ctx["text0"], noted, arm, seed,
                                 dry_answer=ctx.get("logged_answer") if seed == 1 else None)
                writes += 1
                if r:
                    for c in r["continuations"]:
                        left_tot += len(c["left"])
                        quoted_tot += len(c.get("quoted") or [])
                        conts += "skipped" not in c
        sent = [r for k, r in DRY_SENT if k == arm]
        csent = [r for k, r in DRY_SENT if k == f"{arm}:continue"]
        print(f"  {arm}: write requests {len(sent)} of {writes}; on the logged answers: left out {left_tot}, "
              f"quoted {quoted_tot}, continuation requests {len(csent)}")
        if sent:
            ex[arm] = sent[0]
        if csent:
            ex[f"{arm}:continue"] = csent[0]
    print("\nEXAMPLE REQUEST PER ARM (abbreviated)")
    for arm, r in ex.items():
        print(f"--- {arm}\n{json.dumps(abbreviate(r), ensure_ascii=False, indent=1)}")


# ---------- summary + decision (§3.4) ----------
def stage1_table(recs):
    arms = [a for a in STAGE1_ARMS if any(r["arm"] == a for r in recs)]
    by = collections.defaultdict(dict)
    for r in recs:
        by[r["key"]][r["arm"]] = r
    common = [k for k, v in by.items() if all(a in v for a in arms)]
    return arms, by, common


def decide(args):
    s1 = read_out(os.path.join(args.out, "stage1.jsonl"))
    arms, by, common = stage1_table(s1)
    missed = [k for k in common if by[k]["C0R0"]["before"]["recall"] < by[k]["C0R0"]["before"]["of"]] if "C0R0" in arms else []
    new = [a for a in arms if a != "C0R0"]
    tot = lambda a, f: sum(f(by[k][a]) for k in common)
    qual = [a for a in new if "C0R0" in arms and all(set(by[k][a]["facts_after"]) >= set(by[k]["C0R0"]["facts_after"]) for k in missed)]
    if qual:
        pick = min(qual, key=lambda a: (tot(a, lambda r: len(r["nonfact_added"])), tot(a, lambda r: r["secs"])))
    elif new:
        pick = max(new, key=lambda a: tot(a, lambda r: r["after"]["recall"]))
    else:
        pick = "C0R0"
    d = {"stage1_arms": arms, "readings": len(common), "missed_fact_readings": len(missed), "qualifying": qual,
         "stage1_pick": pick, "josh_call": bool(new) and not qual,
         "c1r0_needed": "C1R1" in arms and "C1R1" not in qual and "C1R0" not in arms,
         "room_change_cost_recall": "C1R0" in arms and "C1R1" not in qual and "C1R0" in qual}
    s2 = read_out(os.path.join(args.out, "stage2.jsonl"))
    if s2:
        w = collections.defaultdict(dict)
        for r in s2:
            w[(r["key"], r["notes_arm"], r["seed"])][r["arm"]] = r
        pairs = [v for v in w.values() if all(a in v for a in STAGE2_ARMS)]
        c = lambda a, f: sum(1 for v in pairs if f(v[a]))
        d["stage2"] = {"writes_each": len(pairs),
                       "pass": {a: c(a, lambda r: r["pass"]) for a in STAGE2_ARMS},
                       "repeated": {a: c(a, lambda r: r["repeated"]) for a in STAGE2_ARMS},
                       "cont_length": {a: c(a, lambda r: r["cont_length"]) for a in STAGE2_ARMS}}
        s = d["stage2"]
        d["W1K1_ships"] = bool(pairs) and s["pass"]["W1K1"] >= s["pass"]["W0K0"] and \
            s["repeated"]["W1K1"] <= s["repeated"]["W0K0"] and s["cont_length"]["W1K1"] == 0
    return d


def summary(args):
    s1 = read_out(os.path.join(args.out, "stage1.jsonl"))
    arms, by, common = stage1_table(s1)
    print(f"STAGE 1 — {len(common)} readings with every arm ({', '.join(arms)})")
    subsets = [("all", lambda r: True), ("missed facts", lambda r: r["before"]["recall"] < r["before"]["of"]),
               ("missed nothing", lambda r: r["before"]["recall"] == r["before"]["of"]),
               ("short6", lambda r: r["set"] == "short"), ("long", lambda r: r["set"] == "long"),
               ("old A/B", lambda r: r["source"] == "old"), ("e2e", lambda r: r["source"] == "e2e")]
    hdr = f"{'subset':15s} {'arm':5s} {'n':>3s} {'facts':>7s} {'nonfact+':>8s} {'whole':>5s} {'sent':>4s} {'req':>4s} " \
          f"{'prompt_new':>10s} {'out':>6s} {'secs':>6s} {'take@1st':>8s} {'later':>5s} {'p0took':>6s}"
    print(hdr)
    for name, f in subsets:
        ks = [k for k in common if f(by[k][arms[0]])]
        if not ks:
            continue
        for a in arms:
            rs = [by[k][a] for k in ks]
            nf = [x for r in rs for x in r["nonfact_added"]]
            tk = [t for r in rs for t in r["taken"]]
            print(f"{name:15s} {a:5s} {len(rs):3d} {sum(r['after']['recall'] for r in rs):3d}/{sum(r['after']['of'] for r in rs):<3d} "
                  f"{len(nf):8d} {sum(x['whole'] for x in nf):5d} {sum(not x['whole'] for x in nf):4d} "
                  f"{sum(r['requests'] for r in rs):4d} {sum(r['prompt_new'] for r in rs):10d} {sum(r['output_tokens'] for r in rs):6d} "
                  f"{sum(r['secs'] for r in rs):6.0f} {sum(t['position'] == 0 for t in tk):8d} {sum(t['position'] > 0 for t in tk):5d} "
                  f"{sum(r['pass0_took'] for r in rs):6d}")
    print("  (facts = fact passages noted after the close read; nonfact+ = non-fact lines the close read accepted, split "
          "whole passage / sentence; take@1st/later = lines taken at the first vs a later position of their request; "
          "C0R0 positions from A3 records are reply order)")
    print("\nPer reading that missed facts (facts after; C0R0 is the bar):")
    for k in common:
        r0 = by[k][arms[0]]
        if r0["before"]["recall"] < r0["before"]["of"]:
            print(f"  {k:45s} before {r0['before']['recall']}/{r0['before']['of']}  " +
                  "  ".join(f"{a} {by[k][a]['facts_after']}" for a in arms))
    s2 = read_out(os.path.join(args.out, "stage2.jsonl"))
    if s2:
        w = collections.defaultdict(dict)
        for r in s2:
            w[(r["key"], r["notes_arm"], r["seed"])][r["arm"]] = r
        pairs = [v for v in w.values() if all(a in v for a in STAGE2_ARMS)]
        print(f"\nSTAGE 2 — {len(pairs)} writes per arm (notes from {sorted({r['notes_arm'] for r in s2})}, seeds {list(SEEDS)})")
        print(f"{'subset':15s} {'arm':5s} {'n':>3s} {'pass':>4s} {'abst':>4s} {'repeat':>6s} {'conts':>5s} {'skip':>4s} "
              f"{'len-fin':>7s} {'chars':>7s} {'cites':>5s} {'dist':>4s} {'nf-cit':>6s} {'nf-dist':>7s} {'secs':>6s}")
        mk = {k for k, v in by.items() if (v.get("C0R0") or next(iter(v.values())))["before"]["recall"]
              < (v.get("C0R0") or next(iter(v.values())))["before"]["of"]}
        for name, f in subsets:
            if name in ("missed facts", "missed nothing"):
                ps = [v for v in pairs if (v["W0K0"]["key"] in mk) == (name == "missed facts")]
            else:
                ps = [v for v in pairs if f(v["W0K0"])]
            if not ps:
                continue
            for a in STAGE2_ARMS:
                rs = [v[a] for v in ps]
                cs = [c for r in rs for c in r["continuations"]]
                print(f"{name:15s} {a:5s} {len(rs):3d} {sum(r['pass'] for r in rs):4d} {sum(r['abstains'] for r in rs):4d} "
                      f"{sum(bool(r['repeated']) for r in rs):6d} {sum('skipped' not in c for c in cs):5d} "
                      f"{sum('skipped' in c for c in cs):4d} {sum(r['cont_length'] for r in rs):7d} {sum(r['chars'] for r in rs):7d} "
                      f"{sum(r['cites_total'] for r in rs):5d} {sum(r['cites_distinct'] for r in rs):4d} "
                      f"{sum(r['cites_nonfact_total'] for r in rs):6d} {sum(r['cites_nonfact_distinct'] for r in rs):7d} "
                      f"{sum(r['secs'] for r in rs):6.0f}")
        print("  (repeat = answers with a sentence of >30 chars written twice; len-fin = answers with a continuation "
              "finished by `length`; skip = continuations skipped for room / nothing fitting)")
    d = decide(args)
    print("\nDECISION (§3.4, lexicographic)")
    print(f"  stage 1: qualifying {d['qualifying']} -> pick {d['stage1_pick']}"
          + ("  [no arm kept C0R0's facts: recall lost vs precision gained is Josh's call]" if d["josh_call"] else "")
          + ("  [C1R1 lost recall; run C1R0]" if d["c1r0_needed"] else "")
          + ("  [C1R0 qualifies where C1R1 does not: the room change cost recall and ships separately]"
             if d["room_change_cost_recall"] else ""))
    if "stage2" in d:
        print(f"  stage 2: {d['stage2']} -> W1K1 {'ships' if d['W1K1_ships'] else 'does not ship'}")


def main():
    global URL, N_CTX, items
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--stage", type=int, choices=(1, 2))
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--decision", action="store_true")
    ap.add_argument("--rubric", action="append", default=[])
    ap.add_argument("--old", action="append", default=[])
    ap.add_argument("--e2e", action="append", default=[])
    ap.add_argument("--pass1")
    ap.add_argument("--a3")
    ap.add_argument("--out", required=True)
    ap.add_argument("--arms", default="C0R0,C1R1,C2R1")
    ap.add_argument("--notes-arm")
    ap.add_argument("--only", default="")
    ap.add_argument("--recipe", help="dry mode only: the stub n_ctx is this recipe's server.expected_ctx")
    ap.add_argument("--llama", default=os.environ.get("LLAMA", "http://127.0.0.1:8089"))
    args = ap.parse_args()
    args.only = set(filter(None, args.only.split(",")))
    args.arms = [a for a in args.arms.split(",") if a]
    bad = [a for a in args.arms if a not in STAGE1_ARMS]
    if bad:
        sys.exit(f"unknown stage-1 arms {bad}")
    URL = args.llama
    os.makedirs(args.out, exist_ok=True)
    if args.summary or args.decision:
        if args.decision:
            print(json.dumps(decide(args)))
        else:
            summary(args)
        return
    for p in args.rubric:
        items.update({i["id"]: i for i in json.load(open(p))["items"]})
    ctxs, tally = load(args)
    if args.dry:
        if not args.recipe:
            sys.exit("--dry needs --recipe for the stub n_ctx")
        N_CTX = json.load(open(args.recipe))["server"]["expected_ctx"]
        dry(args, ctxs, tally)
    elif args.stage == 1:
        run_stage1(args, ctxs)
    elif args.stage == 2:
        notes_arm = args.notes_arm or decide(args)["stage1_pick"]
        print(f"stage 2 on {notes_arm} notes", flush=True)
        run_stage2(args, ctxs, notes_arm)
    else:
        sys.exit("give --dry, --stage 1|2, --summary or --decision")


if __name__ == "__main__":
    main()
