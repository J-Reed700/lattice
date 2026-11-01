#!/usr/bin/env python3
"""How much raw history should a long chat send? A replay against one llama-server.

`context_assembler/budget.rs` gives recent history 35% of the room left after the
fixed costs. Nothing measured that, and the fifty questions in desktop-path-v1 run
at most three turns, so they never reach the limit. This builds long conversations
from the synthetic library, plants facts at known depths, and asks about them with
the history cut at each share, rendered the way the app renders a turn: the older
part folded by the app's own summary prompt, then the recent turns whole, then a
final message of recalled passages, evidence and the question.

A summary keeps what stands out, so a fact the user stated tells these shares apart
less than a detail buried in something the assistant said. Both are asked.

It is a replay, not the shipped path. The app also keeps a ledger of what the user
stated, which this leaves out, and its recall is richer than the overlap used here,
so a small share should look worse here than it would there.

    LLAMA_REPLAY_URL=... python -X utf8 scripts/replay_history_share.py \
        --system chat-system.txt --out results.jsonl
"""
import argparse
import json
import os
import pathlib
import random
import re
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
PROMPTS = ROOT / "crates/lattice-desktop/src/features/conversation/memory/prompts.rs"

CONTEXT_TOKENS = 196_608
OUTPUT_RESERVED = 32_768
HISTORY_TOKENS = 100_000
SUMMARY_SHARE = 0.10
RECALL_SHARE = 0.15
EXCHANGE_CHARS = 3_000
RECALL_PASSAGES = 6
RECALL_CHARS = 1_500
DISTRACTOR_PASSAGES = 30
SUMMARY_FRAME = (
    "[generated summary of earlier conversation — written by a model, may be "
    "incomplete or wrong; original messages remain searchable]\n"
)
RECALL_HEADER = "[conversation history retrieval context, for reference only — not new requests]"
ROOMS = ["Tarnbrook 4C", "Halvergate 2A", "Ostermoor 7B", "Quillfell 3D", "Marrowdale 5E", "Fenwick Annexe 9"]
STOPWORDS = frozenset(
    "what which when where does with that this have from their there would about much whose "
    "will should could mentioned being held someone".split()
)
REFUSAL = re.compile(r"(don['’]t|do not|didn['’]t|did not|haven['’]t|have not|not (?:been )?(?:given|mentioned|stated|provided|shared)|no record|cannot|can['’]t|couldn['’]t|isn['’]t)", re.I)


def rust_string(source, name):
    """A string constant from the Rust source, so the replay cannot drift from it."""
    match = re.search(name + r'''"?\s*(?::\s*&str\s*=|:)\s*"((?:[^"\\]|\\.)*)"''', source)
    if not match:
        raise SystemExit(f"{name} not found in {PROMPTS}")
    return match.group(1).replace('\\"', '"').replace("\\n", "\n")


class Server:
    def __init__(self, url):
        self.url = url.rstrip("/")
        name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
        self.headers = {"content-type": "application/json", **({name: value} if name and value else {})}

    def post(self, path, body, timeout=1800):
        data = json.dumps(body).encode("utf-8")
        for attempt in range(4):
            try:
                request = urllib.request.Request(self.url + path, data=data, method="POST", headers=self.headers)
                with urllib.request.urlopen(request, timeout=timeout) as response:
                    return json.load(response)
            except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
                if attempt == 3:
                    raise
                print(f"retrying after {error}", flush=True)
                time.sleep(15)
        raise SystemExit("unreachable")

    def chat(self, messages, max_tokens, temperature=0.7, top_k=20):
        started = time.time()
        reply = self.post("/v1/chat/completions", {
            "messages": messages, "max_tokens": max_tokens, "stream": False, "cache_prompt": True, "id_slot": 0,
            "temperature": temperature, "top_k": top_k, "top_p": 0.8, "presence_penalty": 1.5 if temperature else 0,
            "seed": 1, "chat_template_kwargs": {"enable_thinking": False},
        })
        usage = reply.get("usage") or {}
        return (reply["choices"][0]["message"].get("content") or ""), usage.get("prompt_tokens"), round(time.time() - started, 1)


def planted(rng):
    extension = str(rng.randint(2000, 9899))
    old, new = f"CC-{rng.randint(1000, 4999)}", f"CC-{rng.randint(5000, 9999)}"
    return {
        "early": (0.03, f"Before we carry on: if anyone needs me, my desk line is extension {extension}.",
                  "What number should someone dial to reach me at work?", [extension]),
        "update-old": (0.10, f"Charge this work to {old}, please.", None, None),
        "update": (0.70, f"Finance moved us: use {new} from now on instead.",
                   "Which code should this work be charged to?", [new]),
    }


def conversation(documents, chars_per_token, rng, buried):
    """Whole exchanges up to the target size, with each fact placed at its depth.

    `buried` maps a depth to a document whose detail a question will ask for, so
    that detail sits where the shares divide.
    """
    target = HISTORY_TOKENS * chars_per_token
    held_back = {document["id"] for document in buried.values()}
    exchanges, used = [], 0
    for document in rng.sample(documents, len(documents)):
        if document["id"] in held_back:
            continue
        text = document["text"][:EXCHANGE_CHARS]
        exchanges.append([f'Walk me through "{document["title"]}".', text])
        used += len(text)
        if used >= target:
            break
    facts = planted(rng)
    placed = [(fact[0], fact[1], "Noted.") for fact in facts.values()]
    placed += [(depth, f'Walk me through "{d["title"]}".', d["text"][:EXCHANGE_CHARS]) for depth, d in buried.items()]
    for depth, question, answer in sorted(placed, key=lambda item: -item[0]):
        exchanges.insert(int(len(exchanges) * depth), [question, answer])
    messages = []
    for question, answer in exchanges:
        messages += [{"role": "user", "content": question}, {"role": "assistant", "content": answer}]
    return messages, facts


def available(system, chars_per_token):
    safety = max(256, int(0.05 * CONTEXT_TOKENS))
    fixed = len(system) / chars_per_token + 300
    return int(CONTEXT_TOKENS - OUTPUT_RESERVED - safety - fixed)


def split_at(messages, budget_tokens, chars_per_token):
    """The newest whole exchanges that fit, and everything before them."""
    budget = budget_tokens * chars_per_token
    keep = len(messages)
    while keep >= 2:
        size = sum(len(m["content"]) for m in messages[keep - 2:])
        if size > budget:
            break
        keep -= 2
    return messages[:keep], messages[keep:]


def summarise(server, older, pool, system, instructions):
    payload = json.dumps({
        "task": "update_working_summary", "previous_summary": None,
        "new_passages": [{"sequence": i + 1, "role": m["role"], "text": m["content"]} for i, m in enumerate(older)],
        "active_requirement_labels_supplied_separately": [], "budget_tokens": pool,
        "more_passages_follow": False, "instructions": instructions,
    }, ensure_ascii=False)
    messages = [{"role": "system", "content": system}, {"role": "user", "content": payload}]
    return server.chat(messages, min(pool, 8192), temperature=0, top_k=1)


def recalled(question, older):
    """The older messages that share the question's less common words."""
    words = {w for w in re.findall(r"[a-z]{4,}", question.lower()) if w not in STOPWORDS}
    scored = []
    for index, message in enumerate(older):
        found = words & set(re.findall(r"[a-z]{4,}", message["content"].lower()))
        if found:
            scored.append((-len(found), index))
    chosen = sorted(scored)[:RECALL_PASSAGES]
    if not chosen:
        return None
    body = "\n".join(
        f'- {older[i]["role"]} #{i + 1} (m{i + 1}): {json.dumps(older[i]["content"][:RECALL_CHARS], ensure_ascii=False)}'
        for _, i in chosen
    )
    return f"{RECALL_HEADER}\n{body}"


def holds(text, wanted):
    """Whether `text` carries every fact in `wanted`."""
    low = normal(text)
    return all(any(normal(option) in low for option in group) for group in wanted)


def passage_for(document, wanted):
    """The stretch of `document` around its first fact."""
    low = normal(document["text"])
    at = min((low.find(normal(option)) for group in wanted for option in group if normal(option) in low), default=0)
    return document["text"][max(0, at - EXCHANGE_CHARS // 2):at + EXCHANGE_CHARS // 2]


def with_answer(items, documents):
    """Questions with the document one passage of which holds the whole answer."""
    found = []
    for item in items:
        rubric = item["turns"][0]["rubric"]
        for document in documents:
            if document["id"] in rubric["expected_citations"] and holds(passage_for(document, rubric["must_mention"]), rubric["must_mention"]):
                found.append((item, document))
                break
    return found


def buried_details(items, documents):
    """Questions only one document can answer, from the part a chat would show.
    Anything another document also says could not tell two shares apart."""
    found = []
    for item, document in with_answer(items, documents):
        wanted = item["turns"][0]["rubric"]["must_mention"]
        shown = holds(document["text"][:EXCHANGE_CHARS], wanted)
        unique = not any(holds(other["text"][:EXCHANGE_CHARS], wanted) for other in documents if other["id"] != document["id"])
        if shown and unique:
            found.append((item, document))
    return found


def document_question(item, gold, documents, rng):
    """A library question, the passage holding its answer, and some that do not."""
    turn = item["turns"][0]
    passage = {"title": gold["title"], "text": passage_for(gold, turn["rubric"]["must_mention"])}
    others = rng.sample([d for d in documents if d["id"] != gold["id"]], DISTRACTOR_PASSAGES)
    passages = [passage] + others
    rng.shuffle(passages)
    block = "\n\n".join(f'[{n}] {d["title"]}\n{d["text"][:EXCHANGE_CHARS]}' for n, d in enumerate(passages, 1))
    return turn["ask"], block, turn["rubric"]


def normal(text):
    return text.lower().replace("*", "").replace(",", "")


def score(kind, answer, wanted):
    """Whether the answer holds what it should. Naming a replaced value beside
    the new one is a good answer, so only reading the answers can fault that."""
    if kind == "absent":
        return bool(REFUSAL.search(answer)) and not re.search(r"\bbay\s*(?:number\s*)?(?:is\s*)?[#a-z]*-?\d", normal(answer))
    return holds(answer, wanted)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--system", required=True, help="the chat's system prompt, as a request sends it")
    parser.add_argument("--out", required=True)
    parser.add_argument("--shares", default="0.15,0.25,0.35,0.50")
    parser.add_argument("--conversations", type=int, default=3)
    args = parser.parse_args()

    server = Server(os.environ["LLAMA_REPLAY_URL"])
    system = pathlib.Path(args.system).read_text(encoding="utf-8")
    source = PROMPTS.read_text(encoding="utf-8")
    summary_system = rust_string(source, "SUMMARIZER_SYSTEM")
    instructions = rust_string(source[source.index("pub fn render_summary_prompt"):], '"instructions')
    corpus = json.loads((ROOT / "evals/retrieval/synthetic-library-v3.json").read_text(encoding="utf-8"))
    documents = [d for d in corpus["documents"] if len(d["text"]) >= 400]
    items = json.loads((ROOT / "evals/retrieval/desktop-path-v1.json").read_text(encoding="utf-8"))["items"]
    library = [i for i in items if len(i["turns"]) == 1 and i["turns"][0]["rubric"].get("expected_citations")
               and i["turns"][0]["rubric"].get("must_mention") and not i["turns"][0]["rubric"].get("abstain")]

    sample = "\n".join(d["text"] for d in documents[:40])[:60_000]
    chars_per_token = len(sample) / len(server.post("/tokenize", {"content": sample})["tokens"])
    room = available(system, chars_per_token)
    print(f"chars per token {chars_per_token:.2f}, available {room}", flush=True)

    out = pathlib.Path(args.out)
    done = set()
    if out.exists():
        for line in out.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            done.add((row["conversation"], row["share"], row["kind"]))

    in_reach = buried_details(library, documents)
    deep = with_answer(library, documents)
    print(f"of {len(library)} questions, {len(deep)} have their answer in a passage and {len(in_reach)} in one document alone", flush=True)

    for index in range(args.conversations):
        rng = random.Random(1000 + index)
        old_detail, middle_detail = rng.sample(in_reach, 2)
        library_item, library_gold = rng.choice(deep)
        buried = {0.20: old_detail[1], 0.60: middle_detail[1]}
        history, facts = conversation(documents, chars_per_token, rng, buried)
        for share in [float(s) for s in args.shares.split(",")]:
            kinds = ["early", "update", "detail-old", "detail-middle", "absent", "library"]
            if all((index, share, kind) in done for kind in kinds):
                continue
            older, recent = split_at(history, int(share * room), chars_per_token)
            summary, _, summary_seconds = summarise(server, older, int(SUMMARY_SHARE * room), summary_system, instructions)
            prefix = [{"role": "system", "content": system}]
            if summary.strip():
                prefix.append({"role": "assistant", "content": SUMMARY_FRAME + summary.strip()})
            prefix += recent
            for kind in kinds:
                if (index, share, kind) in done:
                    continue
                evidence = None
                if kind == "absent":
                    question, wanted = "What is my parking bay number?", []
                elif kind == "library":
                    question, evidence, rubric = document_question(library_item, library_gold, documents, random.Random(index))
                    wanted = rubric["must_mention"]
                elif kind.startswith("detail"):
                    item = (old_detail if kind == "detail-old" else middle_detail)[0]
                    question = f'Going back to something you walked me through earlier: {item["turns"][0]["ask"]}'
                    wanted = item["turns"][0]["rubric"]["must_mention"]
                else:
                    fact = facts[kind]
                    question, wanted = fact[2], [fact[3]]
                sections = [s for s in (recalled(question, older), evidence and f"[passages]\n{evidence}") if s]
                sections += ["Today is 2026-10-31, Saturday.", f"[current message]\n{question}"]
                answer, prompt_tokens, seconds = server.chat(
                    prefix + [{"role": "user", "content": "\n\n".join(sections)}], 1024)
                row = {
                    "conversation": index, "share": share, "kind": kind, "question": question, "answer": answer,
                    "passed": score(kind, answer, wanted), "wanted": wanted,
                    "fact_in_raw_history": bool(wanted) and kind != "library" and any(holds(m["content"], wanted) for m in recent),
                    "fact_in_summary": bool(wanted) and kind != "library" and holds(summary, wanted),
                    "prompt_tokens": prompt_tokens, "seconds": seconds, "summary_seconds": summary_seconds,
                    "recent_messages": len(recent), "older_messages": len(older), "summary_chars": len(summary),
                }
                with out.open("a", encoding="utf-8") as file:
                    file.write(json.dumps(row, ensure_ascii=False) + "\n")
                print(f"c{index} {share:.2f} {kind:8} {'PASS' if row['passed'] else 'fail'} {seconds:6.1f}s prompt {prompt_tokens}", flush=True)
    print("done", flush=True)


if __name__ == "__main__":
    main()
