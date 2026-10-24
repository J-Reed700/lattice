"""Offline A/B for the reading-round fix (design doc sections 10.4-10.5). DRAFT, not yet run.

Replays the saved reading requests against llama-server; per prompt, in cache order:
  A0   the saved reading as is (+ label-set stop sequences, logprobs on)   replay fidelity, decision margins
  A1   A0 + relocation + ordinal parse + canonical notes, offline          parts 1-2; no verified copy -> withdrawn
  A2   A1 + reversed second look (only when A1 has a verified copy)        part 3
  A2e  the second look on a reading with NO verified copy                  does a continuation break the collapse?
  A3   A1 + close read with the hyphen set re-presented                    the structural fallback
  F    the reading again with the new READING_INSTRUCTION, then its own A1/A2 and writes
  writes: for A1, A2, A3, F (seeds 1, 2) with the coverage continuation; a reading with no verified copy
          writes WITHOUT notes (instruction removed from the message) = the pre-#174 request
  W    A2's notes (or F-A2's when F ran) with the new WRITE wording, seeds 1, 2

argv: rubric.json out.jsonl calls.jsonl [calls.jsonl ...]
env:  ONLY="qid,qid"  ARMS="A2,A2e,A3,F,W" (default A2,A2e,W)  LLAMA=http://127.0.0.1:8089
Every call records usage + timings (cache_n / prompt_n), so cache behaviour is measured, not assumed.
"""
import copy, json, math, os, re, sys, time, urllib.request

RUBRIC, OUT = sys.argv[1], sys.argv[2]
CALL_FILES = sys.argv[3:]
ONLY = set(filter(None, os.environ.get("ONLY", "").split(",")))
ARMS = set(filter(None, os.environ.get("ARMS", "A2,A2e,W").split(",")))
URL = os.environ.get("LLAMA", "http://127.0.0.1:8089")

items = {i["id"]: i for i in json.load(open(RUBRIC))["items"]}

READ_OLD = ("Before answering, go through every numbered passage above in order and write one line for each: its label, "
            "then the sentences of that passage that bear on my question, copied exactly as written and in the passage's "
            "own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. "
            "Account for every label. Write only these lines; the answer comes after.")
READ_NEW = ("Before answering, go through every numbered passage above in order and write one line for each, beginning "
            "with its label in brackets exactly as shown above, then the sentences of that passage that bear on my "
            "question, copied exactly as written and in the passage's own language. For a passage that says nothing "
            "bearing on my question, write its label and a single hyphen. Account for every label, and write nothing "
            "after the last line; the answer comes after.")
WRITE_OLD = ("Now answer my question. Your reading notes above say what each passage states about it: cover every "
             "passage you noted, cite each with its label, and take the wording from the passages themselves. Leave out a "
             "note that was mistaken.")
WRITE_NEW = ("Now answer my question directly, in the passages' own words, keeping their dates, figures and names as "
             "given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the "
             "passages, and say nothing about passages that do not bear on my question.")
SWEEP = "You did not account for {labels}. Write one line for each of them, in the same form."
SECOND_LOOK = ("Go through the passages you marked with a hyphen once more, from the last to the first: {labels}. Write one "
               "line for each in the same form — its label, then the sentences of that passage that bear on my question "
               "copied exactly as written, or a hyphen.")
CLOSE_READ = "Here again are the passages you marked with a hyphen. Read each on its own and write its line in the same form.\n\n{passages}"
CONTINUE = ("Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. "
            "Continue your answer with what each of them says, in the same form as above, citing each with its label; "
            "do not repeat what you have written. If one of those notes was mistaken, write nothing for it.")
SAMPLING_KEYS = ("temperature", "top_p", "top_k", "presence_penalty", "repeat_penalty", "max_tokens", "chat_template_kwargs",
                 "parallel_tool_calls")


# ---------- text helpers (mirror reading.rs) ----------
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


def non_note_lines(text):
    return [l for l in text.splitlines() if l.strip() and parse_line(l) is None]


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


# ---------- server ----------
def call(req, **extra):
    r = copy.deepcopy(req)
    for k in ("stream_options", "return_progress", "id_slot"):
        r.pop(k, None)
    r["stream"] = False
    r.update(extra)
    t = time.time()
    resp = json.load(urllib.request.urlopen(urllib.request.Request(
        URL + "/v1/chat/completions", json.dumps(r).encode(), {"Content-Type": "application/json"})))
    ch = resp["choices"][0]
    msg = ch["message"]
    return {"content": msg.get("content") or "", "tool_calls": msg.get("tool_calls") or [], "finish": ch.get("finish_reason"),
            "logprobs": ch.get("logprobs"), "secs": round(time.time() - t, 1), "usage": resp.get("usage"),
            "timings": {k: resp.get("timings", {}).get(k) for k in ("cache_n", "prompt_n", "prompt_ms", "predicted_n", "predicted_ms")}}


def decision_margins(logprobs, content):
    """At each line's decision token (first token after '[k] '): chosen token, its p, best alternative, its p."""
    if not logprobs or not logprobs.get("content"):
        return {}
    toks = logprobs["content"]
    offsets, pos = [], 0
    for t in toks:
        offsets.append(pos)
        pos += len(t.get("token", ""))
    out = {}
    for m in re.finditer(r"(?m)^\s*(?:\d+\.\s*)?\[(\d+)\]\s?", content):
        idx = next((i for i, o in enumerate(offsets) if o >= m.end()), None)
        if idx is None:
            continue
        t = toks[idx]
        alts = sorted(t.get("top_logprobs", []), key=lambda a: -a["logprob"])
        other = next((a for a in alts if a["token"] != t["token"]), None)
        out[int(m.group(1))] = {"token": t["token"], "p": round(math.exp(t["logprob"]), 3),
                                "alt": other and other["token"], "alt_p": other and round(math.exp(other["logprob"]), 3)}
    return out


# ---------- reading loop pieces ----------
def sweep(read_req, lines, text, texts, log):
    while True:
        unaccounted = [l for l in texts if l not in lines]
        if not unaccounted:
            return lines, text
        r = call(read_req, messages=read_req["messages"] + [
            {"role": "assistant", "content": text},
            {"role": "user", "content": SWEEP.format(labels=", ".join(f"[{l}]" for l in unaccounted))}])
        log.append(("sweep", r))
        added = {l: n for l, n in parse_notes(whole_lines(r["content"], r["finish"])).items() if l in unaccounted}
        if not added:
            return lines, text
        lines.update(added)
        text = text.rstrip() + "\n" + r["content"].strip()


def second_look(read_req, lines, text, texts, log, kind="second_look"):
    passes, flips = 0, 0
    while True:
        hyph = sorted((l for l, n in lines.items() if n is None), reverse=True)
        if not hyph:
            break
        r = call(read_req, messages=read_req["messages"] + [
            {"role": "assistant", "content": text},
            {"role": "user", "content": SECOND_LOOK.format(labels=", ".join(f"[{l}]" for l in hyph))}])
        log.append((kind, r))
        passes += 1
        flipped = 0
        for l, n in parse_notes(whole_lines(r["content"], r["finish"])).items():
            if l not in hyph or not n:
                continue
            where = locate(n, texts)
            target = l if l in where else (where[0] if len(where) == 1 else None)
            if target in hyph and lines.get(target) is None:
                lines[target] = n
                flipped += 1
        flips += flipped
        if not flipped:
            break
        text = render(lines)
    return lines, text, passes, flips


def tokens(text):
    return len(json.load(urllib.request.urlopen(urllib.request.Request(
        URL + "/tokenize", json.dumps({"content": text}).encode(), {"Content-Type": "application/json"})))["tokens"])


N_CTX = None


def n_ctx():
    global N_CTX
    if N_CTX is None:
        props = json.load(urllib.request.urlopen(URL + "/props"))
        N_CTX = props["default_generation_settings"]["n_ctx"]
    return N_CTX


def close_read(read_req, lines, text, texts, log, prompt_tokens):
    """Re-present the hyphen set next to the decision, as many passages per request as the window's room
    allows (window - measured prompt - notes - the reading's reservation), in label order; repeat while it flips."""
    reserve = read_req.get("max_tokens") or 0
    passes, flips = 0, 0
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
            r = call(read_req, stop=stops_for(texts), messages=read_req["messages"] + [
                {"role": "assistant", "content": text},
                {"role": "user", "content": CLOSE_READ.format(passages=block)}])
            log.append(("close_read", r))
            for l, n in parse_notes(whole_lines(r["content"], r["finish"])).items():
                if l in batch and n and l in locate(n, texts) and lines.get(l) is None:
                    lines[l] = n
                    flipped += 1
        flips += flipped
        if not flipped:
            break
        text = render(lines)
    return lines, text, passes, flips


def run_reading(read_req, texts, log, kind="read", logprobs=True):
    """A0/F: one reading + sweep -> raw lines, model text, first response."""
    extra = {"stop": stops_for(texts)}
    if logprobs:
        extra.update(logprobs=True, top_logprobs=5)
    r0 = call(read_req, **extra)
    log.append((kind, r0))
    text0 = whole_lines(r0["content"], r0["finish"])
    raw = parse_notes(text0)
    raw, text0 = sweep(read_req, raw, text0, texts, log)
    return raw, text0, r0


def write_round(base, write_sampling, read_user, lines, notes_text, noted, write_text, seed, log, arm):
    """notes_text None = withdrawn: the instruction is removed and no notes are replayed (pre-#174 request)."""
    w = copy.deepcopy(base)
    w.pop("tool_choice", None)
    w.pop("stop", None)
    w.update(write_sampling)
    if notes_text is None:
        w["messages"][-1]["content"] = read_user.replace("\n\n" + READ_OLD, "").replace("\n\n" + READ_NEW, "")
    else:
        w["messages"][-1]["content"] = read_user
        w["messages"] += [{"role": "assistant", "content": notes_text}, {"role": "user", "content": write_text}]
    w["seed"] = seed
    r = call(w)
    log.append((f"write:{arm}", r))
    answer, conts = r["content"], 0
    left = (noted or set()) - cited(answer)
    while left and not r["tool_calls"]:
        quotes = " ".join(f'[{l}] "{n}"' for l, n in sorted(lines.items()) if l in left and n)
        c = call(w, messages=w["messages"] + [{"role": "assistant", "content": answer},
                                              {"role": "user", "content": CONTINUE.format(quotes=quotes)}],
                 tool_choice="none", seed=seed, chat_template_kwargs={"enable_thinking": False})
        log.append((f"continue:{arm}", c))
        conts += 1
        if not (cited(c["content"]) & left):
            break
        answer = answer.rstrip() + "\n\n" + c["content"].strip()
        left = (noted or set()) - cited(answer)
    return {"arm": arm, "seed": seed, "withdrawn": notes_text is None, "missing": missing(qid, answer),
            "abstains": abstains(answer), "continuations": conts, "tool_calls": len(r["tool_calls"]),
            "cited": sorted(cited(answer)), "answer": answer}


def reading_metrics(lines, noted, fl):
    return {"recall": len(fl & noted), "of": len(fl), "missed": sorted(fl - noted), "false_quotes": sorted(noted - fl),
            "hyphens": sum(1 for n in lines.values() if n is None), "verified": len(noted)}


def arm_from_raw(raw, text0, texts, fl, moved_key):
    lines1, noted1, moved = attribute(raw, texts)
    text1 = canonical_text(text0, lines1) if noted1 else None  # no verified copy -> withdrawn
    m = reading_metrics(lines1, noted1, fl)
    m.update({moved_key: moved, "stripped_lines": len(non_note_lines(text0)), "withdrawn": not noted1,
              "numbered_form": bool(re.match(r"\s*\d+\.", text0))})
    return lines1, noted1, text1, m


# ---------- load the saved requests ----------
saved = {}  # (run, qid) -> {"read": (req, logged notes), "write": logged write request}
for path in CALL_FILES:
    run = os.path.basename(path).split(".")[0]
    for line in open(path):
        row = json.loads(line)
        c = row["call"]
        q = c.get("request") or {}
        m = q.get("messages") or []
        if c.get("path") != "/v1/chat/completions" or not m:
            continue
        last = text_of(m[-1])
        key = (run, row["query_id"])
        if q.get("tool_choice") == "none" and len(m) == 2 and last.endswith("the answer comes after."):
            saved.setdefault(key, {})["read"] = (q, c.get("reply", {}).get("content", ""))
        elif last.endswith("Leave out a note that was mistaken."):
            saved.setdefault(key, {}).setdefault("write", q)

out = open(OUT, "a")
pass1_by_key = {}
pass1_path = os.path.join(os.path.dirname(OUT), "ab-pass1.jsonl")
if os.path.exists(pass1_path):
    for line in open(pass1_path):
        if line.strip():
            r1 = json.loads(line)
            pass1_by_key[(r1["run"], r1["query_id"])] = r1

for (run, qid), s in saved.items():
    if "read" not in s or (ONLY and qid not in ONLY):
        continue
    read_req, logged = s["read"]
    write_sampling = {k: s["write"][k] for k in SAMPLING_KEYS if "write" in s and k in s["write"]}
    read_user = text_of(read_req["messages"][-1])
    texts = passages(read_user)
    fl = fact_labels(qid, texts)
    log = []
    rec = {"run": run, "query_id": qid, "passages": len(texts), "fact_labels": sorted(fl), "writes": []}

    # A0 + A1 (reuse from pass1 when available to avoid redundant A0 + A1 write calls)
    p1 = pass1_by_key.get((run, qid))
    p1_read = next((c for c in (p1 or {}).get("calls", []) if c.get("kind") == "read"), None)
    if p1 and p1_read and ARMS == {"A3"}:
        text0 = whole_lines(p1_read["content"], p1_read["finish"])
        raw = parse_notes(text0)
        for sc in (c for c in p1.get("calls", []) if c.get("kind") == "sweep"):
            added = {l: n for l, n in parse_notes(whole_lines(sc["content"], sc["finish"])).items() if l in texts and l not in raw}
            raw.update(added)
            text0 = text0.rstrip() + "\n" + sc["content"].strip()
        prompt_tokens = p1_read["usage"]["prompt_tokens"]
        rec["A0"] = p1["A0"]
        lines1, noted1, text1, rec["A1"] = arm_from_raw(raw, text0, texts, fl, "relocated")
        arms = {}
        a1_writes = [w for w in p1.get("writes", []) if w.get("arm") == "A1"]
        rec["writes"].extend(a1_writes)
    else:
        raw, text0, r0 = run_reading(read_req, texts, log)
        prompt_tokens = r0["usage"]["prompt_tokens"]
        logged_notes = {l: n for l, n in parse_notes(logged).items() if l in texts}
        rec["A0"] = {"same_notes_as_log": {l: n for l, n in raw.items() if l in texts} == logged_notes, "finish": r0["finish"],
                     "tokens": r0["usage"]["completion_tokens"],
                     "margins": {str(l): mm for l, mm in decision_margins(r0["logprobs"], r0["content"]).items() if l in fl}}
        lines1, noted1, text1, rec["A1"] = arm_from_raw(raw, text0, texts, fl, "relocated")
        arms = {"A1": (lines1, noted1, text1)}
        a1_writes = []

    # A2 (working reading) / A2e (collapsed reading)
    if "A2" in ARMS and noted1:
        l2, t2, p, f = second_look(read_req, dict(lines1), text1, texts, log)
        n2 = {l for l, n in l2.items() if n and l in locate(n, texts)}
        rec["A2"] = dict(reading_metrics(l2, n2, fl), passes=p, flips=f)
        arms["A2"] = (l2, n2, t2)
    if "A2e" in ARMS and not noted1:
        l2, t2, p, f = second_look(read_req, dict(lines1), canonical_text(text0, lines1), texts, log, kind="second_look_empty")
        n2 = {l for l, n in l2.items() if n and l in locate(n, texts)}
        rec["A2e"] = dict(reading_metrics(l2, n2, fl), passes=p, flips=f, numbered_reply=bool(re.match(r"\s*\d+\.", log[-1][1]["content"])))
        arms["A2e"] = (l2, n2, t2 if n2 else None)
    if "A3" in ARMS and noted1:
        if rec["A1"]["recall"] < rec["A1"]["of"]:
            l3, t3, p, f = close_read(read_req, dict(lines1), text1, texts, log, prompt_tokens)
            n3 = {l for l, n in l3.items() if n and l in locate(n, texts)}
            rec["A3"] = dict(reading_metrics(l3, n3, fl), passes=p, flips=f)
            if f > 0 or not a1_writes:
                arms["A3"] = (l3, n3, t3)
            else:
                for w in a1_writes:
                    w3 = copy.deepcopy(w)
                    w3["arm"] = "A3"
                    rec["writes"].append(w3)
        else:
            rec["A3"] = dict(reading_metrics(lines1, noted1, fl), passes=0, flips=0, skipped_full_recall=True)
            for w in a1_writes:
                w3 = copy.deepcopy(w)
                w3["arm"] = "A3"
                rec["writes"].append(w3)

    # F: the new reading instruction, cold
    if "F" in ARMS:
        f_req = copy.deepcopy(read_req)
        f_user = read_user.replace(READ_OLD, READ_NEW)
        f_req["messages"][-1]["content"] = f_user
        rawf, textf0, rf = run_reading(f_req, texts, log, kind="read_F", logprobs=False)
        lf, nf, tf, rec["F"] = arm_from_raw(rawf, textf0, texts, fl, "relocated")
        rec["F"].update(finish=rf["finish"], tokens=rf["usage"]["completion_tokens"])
        if nf:
            lf2, tf2, p, f = second_look(f_req, dict(lf), tf, texts, log, kind="second_look_F")
            nf2 = {l for l, n in lf2.items() if n and l in locate(n, texts)}
            rec["F-A2"] = dict(reading_metrics(lf2, nf2, fl), passes=p, flips=f)
            arms["F"] = (lf2, nf2, tf2)
        else:
            arms["F"] = (lf, nf, None)
        for seed in (1, 2):
            rec["writes"].append(write_round(f_req, write_sampling, f_user, arms["F"][0], arms["F"][2], arms["F"][1], WRITE_OLD, seed, log, "F"))

    # writes for the A arms (old wording), then W
    for arm in ("A1", "A2", "A2e", "A3"):
        if arm not in arms:
            continue
        lines, noted, text = arms[arm]
        for seed in (1, 2):
            rec["writes"].append(write_round(read_req, write_sampling, read_user, lines, text, noted, WRITE_OLD, seed, log, arm))
    if "W" in ARMS:
        src = "A2" if "A2" in arms else ("A2e" if "A2e" in arms and arms["A2e"][2] else "A1")
        lines, noted, text = arms[src]
        for seed in (1, 2):
            rec["writes"].append(write_round(read_req, write_sampling, read_user, lines, text, noted, WRITE_NEW, seed, log, f"W({src})"))

    rec["calls"] = [{"kind": k, "secs": r["secs"], "finish": r["finish"], "usage": r["usage"], "timings": r["timings"],
                     "content": r["content"]} for k, r in log]
    out.write(json.dumps(rec) + "\n")
    out.flush()
    reads = " | ".join(f"{a} {rec[a]['recall']}/{rec[a]['of']} fq={len(rec[a]['false_quotes'])}"
                       + (" WITHDRAWN" if rec[a].get("withdrawn") else "") for a in ("A1", "A2", "A2e", "A3", "F", "F-A2") if a in rec)
    writes = {}
    for w in rec["writes"]:
        writes.setdefault(w["arm"], []).append("P" if not w["missing"] and not w["abstains"] else ("abst" if w["abstains"] else "miss"))
    print(f"{run} {qid} | A0 same={rec['A0']['same_notes_as_log']} fin={rec['A0']['finish']} | {reads} | writes {writes}", flush=True)
