"""Go/no-go replay for docs/design answer coverage: read every passage, then write.

For each captured round-0 answer request: a reading round (reading instruction
appended to the user message, tools kept with tool_choice none, deterministic,
thinking off), a sweep for unaccounted labels, a writing round (notes replayed as
the assistant's own message, write instruction said last, app sampling, tools
callable), then the coverage check (verbatim-noted labels minus cited labels) with
continuations while they make progress. Scored on the subset file's rubric.

argv: calls.jsonl subset.json out.jsonl [query ids...]
"""
import copy, json, re, sys, time, urllib.request

CALLS, SUBSET, OUT = sys.argv[1:4]
ONLY = set(sys.argv[4:])
items = {i['id']: i for i in json.load(open(SUBSET))['items']}

READ = ("Before answering, go through every numbered passage above in order and write one line for each: "
        "its label, then the sentences of that passage that bear on my question, copied exactly as written and in "
        "the passage's own language. For a passage that says nothing bearing on my question, write its label and a "
        "single hyphen. Account for every label. Write only these lines; the answer comes after.")
WRITE = ("Now answer my question. Your reading notes above say what each passage states about it: cover every "
         "passage you noted, cite each with its label, and take the wording from the passages themselves. Leave out a "
         "note that was mistaken.")
SWEEP = "You did not account for {labels}. Write one line for each of them, in the same form."
CONTINUE = ("Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. "
            "Continue your answer with what each of them says, in the same form as above, citing each with its label; "
            "do not repeat what you have written. If one of those notes was mistaken, write nothing for it.")


def facts(q):
    t = items[q]['turns'][-1]; r = t.get('rubric', t)
    return [f if isinstance(f, list) else [f] for f in (r.get('must_mention') or r.get('facts') or [])]

def missing(q, text):
    low = text.lower()
    return [f[0] for f in facts(q) if not any(a.lower() in low for a in f)]

def norm(s):
    return re.sub(r'\s+', ' ', s).strip()

def passages(user):
    st = re.search(r'(?m)^\[1\] Document:', user).start(); en = re.search(r'(?m)^Question: ', user).start()
    out = {}
    for b in re.split(r'\n(?=\[\d+\] Document:)', user[st:en]):
        m = re.match(r'\[(\d+)\]', b)
        if m:
            c = b.find('Content:')
            out[int(m.group(1))] = norm(b[c + 8:] if c >= 0 else b)
    return out

def parse_notes(text):
    notes = {}
    for line in text.splitlines():
        m = re.match(r'\s*\[?(\d+)\]?\s*[:.\-–—]?\s*(.*)$', line)
        if not m or not line.strip().startswith(('[',) + tuple('0123456789')):
            continue
        label, rest = int(m.group(1)), m.group(2).strip()
        notes[label] = None if not re.search(r'\w', rest) else rest
    return notes

def verbatim(note, passage):
    parts = [norm(p).strip('"“”') for p in re.split(r'(?<=[.!?])\s+', note) if norm(p)]
    return bool(parts) and all(p in passage for p in parts)

def cited(text):
    out = set()
    for grp in re.findall(r'\[(\d+(?:\s*,\s*\d+)*)\]', text):
        out |= {int(x) for x in re.split(r'\s*,\s*', grp)}
    return out

def call(path, req):
    t = time.time()
    resp = json.load(urllib.request.urlopen(urllib.request.Request(
        'http://127.0.0.1:8089' + path, json.dumps(req).encode(), {'Content-Type': 'application/json'})))
    msg = resp['choices'][0]['message']
    return msg.get('content') or '', msg.get('tool_calls') or [], resp['choices'][0].get('finish_reason'), time.time() - t

def base_request(q):
    req = copy.deepcopy(q)
    for k in ('stream_options', 'return_progress', 'id_slot'):
        req.pop(k, None)
    req['stream'] = False
    return req

requests = {}
for line in open(CALLS):
    r = json.loads(line); c = r['call']; q = c.get('request') or {}; m = q.get('messages') or []
    if q.get('tools') and m and str(m[0].get('content', '')).startswith('You are Lattice'):
        requests.setdefault(r['query_id'], (c['path'], q))

out = open(OUT, 'a')
for qid, (path, shipped) in requests.items():
    if ONLY and qid not in ONLY:
        continue
    user = shipped['messages'][-1]['content']
    texts = passages(user)
    read = base_request(shipped)
    read['messages'][-1]['content'] = user.rstrip() + '\n\n' + READ
    read.update(tool_choice='none', temperature=0.0, top_p=1.0, top_k=1, presence_penalty=0.0,
                chat_template_kwargs={'enable_thinking': False})
    read.pop('seed', None)
    notes_text, _, finish, secs = call(path, read)
    reading_s = secs; sweeps = 0
    notes = parse_notes(notes_text)
    while True:
        unaccounted = [l for l in texts if l not in notes]
        if not unaccounted:
            break
        sweep = copy.deepcopy(read)
        sweep['messages'] += [{'role': 'assistant', 'content': notes_text},
                              {'role': 'user', 'content': SWEEP.format(labels=', '.join(f'[{l}]' for l in unaccounted))}]
        more, _, finish, secs = call(path, sweep); reading_s += secs; sweeps += 1
        added = {l: n for l, n in parse_notes(more).items() if l in unaccounted}
        if not added:
            break
        notes.update(added); notes_text = notes_text.rstrip() + '\n' + more.strip()
    noted = {l for l, n in notes.items() if n and l in texts and verbatim(n, texts[l])}
    unverified = {l for l, n in notes.items() if n and l in texts} - noted
    for seed in (1, 2):
        write = base_request(shipped)
        write['messages'][-1]['content'] = read['messages'][-1]['content']
        write['messages'] += [{'role': 'assistant', 'content': notes_text}, {'role': 'user', 'content': WRITE}]
        write['seed'] = seed
        answer, calls, wfinish, wsecs = call(path, write)
        continuations = 0; cont_s = 0.0
        left = noted - cited(answer)
        while left and not calls:
            cont = base_request(shipped)
            cont['messages'] = write['messages'] + [
                {'role': 'assistant', 'content': answer},
                {'role': 'user', 'content': CONTINUE.format(quotes=' '.join(f'[{l}] "{notes[l]}"' for l in sorted(left)))}]
            cont.update(tool_choice='none', seed=seed, chat_template_kwargs={'enable_thinking': False})
            more, _, _, s = call(path, cont); cont_s += s; continuations += 1
            if not (cited(more) & left):
                break
            answer = answer.rstrip() + '\n\n' + more.strip()
            left = noted - cited(answer)
        rec = {'query_id': qid, 'seed': seed, 'passages': len(texts), 'noted': sorted(noted), 'unverified_notes': sorted(unverified),
               'hyphened': sorted(l for l, n in notes.items() if n is None), 'sweeps': sweeps, 'reading_s': round(reading_s, 1),
               'writing_s': round(wsecs, 1), 'continuations': continuations, 'continuation_s': round(cont_s, 1),
               'tool_calls': len(calls), 'cited': sorted(cited(answer)), 'left_out_after': sorted(noted - cited(answer)),
               'missing': missing(qid, answer), 'facts': len(facts(qid)), 'notes': notes_text, 'answer': answer}
        out.write(json.dumps(rec) + '\n'); out.flush()
        print(f"{qid} s{seed} | read {rec['reading_s']}s sweeps={sweeps} noted={rec['noted']} unverified={rec['unverified_notes']} | "
              f"write {rec['writing_s']}s cont={continuations} ({rec['continuation_s']}s) | {'PASS' if not rec['missing'] else 'miss ' + str(rec['missing'])}", flush=True)
