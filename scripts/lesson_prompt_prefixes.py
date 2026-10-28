"""For each recorded lesson call (call-N.json from the live lesson tests): its prompt, the longest prefix it
shares with any earlier call (what a prompt cache can reuse as the prompt is laid out), and the share of its text
already sent in one of the ten calls before it, anywhere in the prompt (what a layout that put shared material
first could reuse). Repeated text is counted by 64-character windows. Grouped by the call's step (its system
prompt).

usage: python -I scripts/lesson_prompt_prefixes.py CALLS_DIR
"""
import collections, glob, json, os, re, sys

W = 64
calls = []
for path in sorted(glob.glob(os.path.join(sys.argv[1], "call-*.json")), key=lambda p: int(re.search(r"call-(\d+)", p).group(1))):
    c = json.load(open(path, encoding="utf-8"))
    system = "\n".join(i.get("content") or "" for i in c["input"] if i.get("role") == "system")
    text = "\n".join(f"<{i.get('role') or i.get('type')}>{i.get('content') or i.get('output') or ''}" for i in c["input"])
    calls.append({"name": os.path.basename(path), "text": text, "step": system[:60].replace("\n", " "),
                  "system": len(system), "tokens": c.get("inputTokens") or 0,
                  "windows": {text[k:k + W] for k in range(0, max(len(text) - W, 0) + 1, 8)}})


def lcp(a, b):
    n, i = min(len(a), len(b)), 0
    while i < n and a[i] == b[i]:
        i += 1
    return i


steps = collections.OrderedDict()
for i, c in enumerate(calls):
    c["prefix"] = max((lcp(c["text"], o["text"]) for o in calls[:i]), default=0)
    seen = set().union(*(o["windows"] for o in calls[max(0, i - 10):i])) if i else set()
    c["repeated"] = len(c["windows"] & seen) / max(len(c["windows"]), 1)
    s = steps.setdefault(c["step"], {"calls": 0, "chars": 0, "tokens": 0, "prefix": 0, "repeated": 0.0, "system": c["system"]})
    s["calls"] += 1
    s["chars"] += len(c["text"])
    s["tokens"] += c["tokens"]
    s["prefix"] += c["prefix"]
    s["repeated"] += c["repeated"] * len(c["text"])

print(f"{'calls':>5} {'prompt tok':>10} {'chars':>9} {'system':>6} {'prefix shared':>13} {'repeated':>8}  step")
for step, s in sorted(steps.items(), key=lambda kv: -kv[1]["tokens"]):
    print(f"{s['calls']:>5} {s['tokens']:>10,} {s['chars']:>9,} {s['system']:>6} {s['prefix'] / s['chars']:>13.0%} "
          f"{s['repeated'] / s['chars']:>8.0%}  {step}")
chars = sum(len(c["text"]) for c in calls)
print(f"\n{len(calls)} calls, {sum(c['tokens'] for c in calls):,} prompt tokens; prefix shared with an earlier call "
      f"{sum(c['prefix'] for c in calls) / chars:.0%} of prompt text, text repeated from the ten calls before "
      f"{sum(c['repeated'] * len(c['text']) for c in calls) / chars:.0%}")
