"""Replay recorded lesson calls (call-N.json from the live lesson tests) to Claude in order, and record what each
call wrote to and read from the prompt cache.

  MODE=app     as `cloud.rs` sent a cache-keyed request before 2026-10-27: system messages joined into `system`,
               and a top-level `cache_control: ephemeral` (the breakpoint on the last block).
  MODE=system  a breakpoint on the system prompt, and the top-level one only on a request that continues an
               exchange (one holding an assistant turn).

The API key comes from ANTHROPIC_API_KEY, or else from the app's own Windows Credential Manager entry, and goes
only into the request header. Output is capped at a few tokens and thinking is off: only the input is measured.

usage: [MODE=system] python -I replay_lesson_cache.py CALLS_DIR OUT.jsonl [MODEL]
"""
import glob, json, os, re, sys, time, urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from replay_claude import api_key  # noqa: E402

CALLS, OUT = sys.argv[1], sys.argv[2]
MODEL = sys.argv[3] if len(sys.argv) > 3 else "claude-haiku-5-5"
MODE = os.environ.get("MODE", "app")


def request_of(call):
    system, messages = [], []
    for item in call["input"]:
        role, content = item.get("role"), item.get("content")
        if item.get("type") not in (None, "message") or content is None:
            continue
        if role == "system":
            system.append(content)
        else:
            messages.append({"role": role, "content": content})
    body = {"model": MODEL, "max_tokens": 8, "messages": messages, "thinking": {"type": "disabled"}}
    if MODE == "app":
        body["cache_control"] = {"type": "ephemeral"}
        if system:
            body["system"] = "\n\n".join(system)
        return body
    # MODE "system": a breakpoint on the system prompt, which every call of a step shares, and the automatic
    # one only on a request that continues an earlier exchange.
    if system:
        body["system"] = [{"type": "text", "text": "\n\n".join(system), "cache_control": {"type": "ephemeral"}}]
    if any(m["role"] == "assistant" for m in messages):
        body["cache_control"] = {"type": "ephemeral"}
    return body


def main():
    key = api_key()
    paths = sorted(glob.glob(os.path.join(CALLS, "call-*.json")), key=lambda p: int(re.search(r"call-(\d+)", p).group(1)))
    done = set()
    if os.path.exists(OUT):
        done = {json.loads(l)["call"] for l in open(OUT, encoding="utf-8")}
    out = open(OUT, "a", encoding="utf-8")
    for path in paths:
        name = os.path.basename(path)
        if name in done:
            continue
        body = request_of(json.load(open(path, encoding="utf-8")))
        req = urllib.request.Request("https://api.anthropic.com/v1/messages", json.dumps(body).encode(),
                                     {"content-type": "application/json", "x-api-key": key,
                                      "anthropic-version": "2023-06-01"})
        t = time.time()
        try:
            usage = json.load(urllib.request.urlopen(req))["usage"]
        except urllib.error.HTTPError as error:
            print(name, "HTTP", error.code, error.read()[:300])
            break
        row = {"call": name, "secs": round(time.time() - t, 1), **{k: usage.get(k) for k in (
            "input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens")}}
        out.write(json.dumps(row) + "\n")
        out.flush()
        print(row, flush=True)


if __name__ == "__main__":
    main()
