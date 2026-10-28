"""The replays' requests sent to Claude as the app's Claude adapter sends them (features/llm/cloud.rs).

A replay builds an OpenAI-style chat request for a llama-server; `call` turns it into a Messages request the way
`cloud.rs` does for a Claude model and answers in the replays' reply shape:
  - system messages joined into `system`; an assistant's tool calls become `tool_use` blocks and tool results
    `tool_result` blocks, in the order `anthropic_messages` builds them;
  - no sampling settings (Claude rejects them); a 5-family model thinks unasked, so `thinking` is adaptive and
    `max_tokens` is the configured output ceiling cut to what the window leaves, not the request's own;
  - tools kept with `tool_choice: none`; stop sequences as sent;
  - every replayed step is cache-keyed in the app, so the system prompt carries a breakpoint, and the whole prompt
    one when it continues an exchange.

The key comes from ANTHROPIC_API_KEY or the app's Windows Credential Manager entry, and goes only into the client.
Every call's usage and price are appended to LEDGER, and a call that would start past BUDGET dollars is refused.
"""
import json, math, os, sys, time

# $ per million tokens: input, output (cache writes 1.25x input, reads 0.1x).
PRICES = {"claude-haiku-5-5": (0.10, 0.50), "claude-sonnet-5-5": (2.00, 10.00), "claude-opus-5-5": (5.00, 25.00)}
MODEL, WINDOW, MAX_TOKENS, LEDGER, BUDGET = None, 131_072, 128_000, None, None
_client = None


def api_key():
    if os.environ.get("ANTHROPIC_API_KEY"):
        return os.environ["ANTHROPIC_API_KEY"].strip()
    import ctypes, ctypes.wintypes as wt

    class CREDENTIAL(ctypes.Structure):
        _fields_ = [("Flags", wt.DWORD), ("Type", wt.DWORD), ("TargetName", wt.LPWSTR), ("Comment", wt.LPWSTR),
                    ("LastWritten", wt.FILETIME), ("CredentialBlobSize", wt.DWORD),
                    ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)), ("Persist", wt.DWORD),
                    ("AttributeCount", wt.DWORD), ("Attributes", ctypes.c_void_p), ("TargetAlias", wt.LPWSTR),
                    ("UserName", wt.LPWSTR)]

    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    cred = ctypes.POINTER(CREDENTIAL)()
    if not advapi.CredReadW("anthropic.com.lattice.lattice", 1, 0, ctypes.byref(cred)):
        sys.exit("no saved Anthropic key")
    blob = bytes(cred.contents.CredentialBlob[i] for i in range(cred.contents.CredentialBlobSize))
    advapi.CredFree(cred)
    text = blob.decode("utf-16-le") if len(blob) % 2 == 0 and b"\x00" in blob else blob.decode("utf-8")
    return text.strip()


def use(model, ledger, budget=None, window=WINDOW, max_tokens=MAX_TOKENS):
    global MODEL, LEDGER, BUDGET, WINDOW, MAX_TOKENS
    if model not in PRICES:
        sys.exit(f"no price for {model}")
    MODEL, LEDGER, BUDGET, WINDOW, MAX_TOKENS = model, ledger, budget, window, max_tokens


def tokens(text):
    """The app's estimate for a cloud model (llm_port.rs DEFAULT_CHARS_PER_TOKEN)."""
    return math.ceil(len(text) / 4.0) if text else 0


def text_of(content):
    if isinstance(content, str):
        return content
    return "".join(p.get("text", "") for p in content or [] if isinstance(p, dict))


def messages_of(chat):
    """cloud/messages.rs `anthropic_messages` over a chat request's messages."""
    system, out = [], []
    for m in chat:
        role = m["role"]
        if role in ("system", "developer"):
            system.append(text_of(m["content"]))
        elif role == "tool":
            result = {"type": "tool_result", "tool_use_id": m["tool_call_id"], "content": text_of(m["content"])}
            if out and out[-1]["role"] == "user" and isinstance(out[-1]["content"], list):
                out[-1]["content"].append(result)
            else:
                out.append({"role": "user", "content": [result]})
        elif m.get("tool_calls"):
            if text_of(m.get("content")):
                out.append({"role": role, "content": text_of(m["content"])})
            for c in m["tool_calls"]:
                f = c["function"]
                out.append({"role": "assistant", "content": [{"type": "tool_use", "id": c["id"], "name": f["name"],
                                                              "input": json.loads(f["arguments"] or "{}")}]})
        elif isinstance(m["content"], list):
            out.append({"role": role, "content": [{"type": "text", "text": p["text"],
                                                   **({"cache_control": p["cache_control"]} if p.get("cache_control") else {})}
                                                  for p in m["content"]]})
        else:
            out.append({"role": role, "content": text_of(m["content"])})
    return system, out


def cached(prefix, rest):
    """A message whose `prefix` other requests share, with a breakpoint after it so they read it from the cache."""
    return [{"type": "text", "text": prefix, "cache_control": {"type": "ephemeral"}}, {"type": "text", "text": rest}]


def body_of(req):
    """cloud.rs's Messages body for `req`, a chat request as a replay sends it to a llama-server."""
    system, messages = messages_of(req["messages"])
    tools = req.get("tools") or []
    prompt = tokens("\n".join(f"{m['role']}\n{text_of(m['content'])}" for m in req["messages"])
                    + (json.dumps(tools) if tools else ""))
    body = {"model": MODEL, "messages": messages, "max_tokens": max(1, min(MAX_TOKENS, WINDOW - prompt)),
            "thinking": {"type": "adaptive"}}
    if system:
        body["system"] = [{"type": "text", "text": "\n\n".join(system), "cache_control": {"type": "ephemeral"}}]
    if req.get("stop"):
        body["stop_sequences"] = list(req["stop"])
    if tools:
        body["tools"] = [{"name": t["function"]["name"], "description": t["function"].get("description", ""),
                          "input_schema": t["function"].get("parameters") or {"type": "object"}} for t in tools]
        choice = req.get("tool_choice")
        if choice:
            body["tool_choice"] = {"type": "none" if choice == "none" else "auto"}
    if any(m["role"] == "assistant" for m in messages):
        body["cache_control"] = {"type": "ephemeral"}
    return body


def spent():
    if not LEDGER or not os.path.exists(LEDGER):
        return 0.0
    return sum(json.loads(l)["usd"] for l in open(LEDGER, encoding="utf-8") if l.strip())


def price(u):
    inp, out = PRICES[MODEL]
    return ((u.input_tokens + 1.25 * (u.cache_creation_input_tokens or 0) + 0.1 * (u.cache_read_input_tokens or 0))
            * inp + u.output_tokens * out) / 1e6


FINISH = {"end_turn": "stop", "stop_sequence": "stop", "max_tokens": "length", "tool_use": "tool_calls"}


def call(req, kind):
    """`req` sent to Claude, answered in replay_close_read_ab.call's shape. A warmup is not sent: it exists to
    time a llama-server's prompt, and here it would only be paid for."""
    global _client
    if kind == "warmup":
        return {"content": "", "tool_calls": [], "finish": "stop", "secs": 0.0, "usage": None, "timings": {}}
    if BUDGET is not None and spent() >= BUDGET:
        sys.exit(f"stopped: ${spent():.2f} spent of the ${BUDGET:.2f} budget")
    if _client is None:
        import anthropic
        _client = anthropic.Anthropic(api_key=api_key(), max_retries=10, timeout=900)
    body = body_of(req)
    extra = {k: body.pop(k) for k in ("cache_control",) if k in body}
    t = time.time()
    with _client.messages.stream(**body, extra_body=extra or None) as stream:
        msg = stream.get_final_message()
    secs = round(time.time() - t, 1)
    u = msg.usage
    usd = price(u)
    new = u.input_tokens + (u.cache_creation_input_tokens or 0)
    if LEDGER:
        with open(LEDGER, "a", encoding="utf-8") as f:
            f.write(json.dumps({"kind": kind, "model": MODEL, "input": u.input_tokens,
                                "cache_write": u.cache_creation_input_tokens, "cache_read": u.cache_read_input_tokens,
                                "output": u.output_tokens, "stop": msg.stop_reason, "secs": secs, "usd": usd}) + "\n")
    text = "".join(b.text for b in msg.content if b.type == "text")
    calls = [{"id": b.id, "type": "function", "function": {"name": b.name, "arguments": json.dumps(b.input)}}
             for b in msg.content if b.type == "tool_use"]
    return {"content": text, "tool_calls": calls, "finish": FINISH.get(msg.stop_reason, msg.stop_reason), "secs": secs,
            "usage": {"prompt_tokens": new + (u.cache_read_input_tokens or 0), "completion_tokens": u.output_tokens},
            "timings": {"cache_n": u.cache_read_input_tokens, "prompt_n": new, "prompt_ms": None,
                        "predicted_n": u.output_tokens, "predicted_ms": None}}


def print_spend(ledger):
    """Each request kind's Claude usage and price, from the run's ledger."""
    if not os.path.exists(ledger):
        return
    rows = [json.loads(l) for l in open(ledger, encoding="utf-8") if l.strip()]
    by = {}
    for r in rows:
        k = by.setdefault((r["model"], r["kind"]), [0, 0, 0, 0, 0.0])
        for i, key in enumerate(("input", "cache_read", "output")):
            k[i] += r[key] or 0
        k[3] += 1
        k[4] += r["usd"]
    print("\n| Model | Request kind | Requests | Uncached input | Cache reads | Output (with thinking) | Dollars |")
    print("|---|---|---|---|---|---|---|")
    for (model, kind), (inp, read, out, n, usd) in sorted(by.items()):
        print(f"| {model} | {kind} | {n} | {inp:,} | {read:,} | {out:,} | {usd:.2f} |")
    print(f"\nTotal ${sum(r['usd'] for r in rows):.2f}")
