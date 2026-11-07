"""Re-scores the 2026-10-28 reading replays against their passages read as UTF-8.

Those replays read their logs at the platform's default encoding, cp1252 on Windows, so a reply that wrote
the real "£" was scored a miss against "Â£" (see the correction in
evals/retrieval/2026-10-28-READING-EVERY-MODEL.md). Here a missed fact counts as noted when its line is an
exact copy of the real passage. Prints the table that correction quotes.

Usage: python scripts/rescore_reading_utf8.py
"""
import gzip
import json
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay_close_read_ab as base  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
CHAT = ROOT / "evals/retrieval/results/chat"
LOGS = {f"cov-{s}": f"2026-10-23-answer-coverage-{s}" for s in ("long8-r1", "long8-r2", "long8-r3", "short6-r1")}
LOGS |= {f"rm-{s}": f"2026-10-24-reading-misses-{s}" for s in ("long8-r1", "long8-r2", "long8-r3", "short6-r1")}
RUNS = [("qwen3.5-9b-local", "old"), ("qwen3.5-9b-local", "new"), ("claude-haiku-5-5", "old-thinking"),
        ("claude-haiku-5-5", "old-off"), ("claude-haiku-5-5", "draft-off"), ("claude-haiku-5-5", "draft-thinking"),
        ("claude-haiku-5-5", "new-off"), ("claude-sonnet-5-5", "old-thinking"), ("claude-sonnet-5-5", "draft-off"),
        ("claude-sonnet-5-5", "new-off")]
SIZES = ["G2", "G4", "G8", "G16", "G64"]


def passages():
    """`run|query` -> the passages its reading showed, read as UTF-8."""
    out = {}
    with tempfile.TemporaryDirectory() as folder:
        for run, name in LOGS.items():
            plain = Path(folder) / f"{run}.calls.jsonl"
            with gzip.open(CHAT / "qwen3.5-9b-local" / f"{name}.jsonl.calls.jsonl.gz", "rb") as packed, \
                    open(plain, "wb") as unpacked:
                shutil.copyfileobj(packed, unpacked)
            for (_, query), calls in base.group_calls(str(plain), run).items():
                if "read" in calls:
                    out[f"{run}|{query}"] = base.passages(base.text_of(calls["read"]["request"]["messages"][-1]))
    return out


def copies_its_passage(line, passage):
    sentences = base.sentences(line) if line else []
    return bool(sentences) and all(sentence in passage for sentence in sentences)


def rescored(model, wording, shown):
    """size -> (facts noted as reported, as re-scored)."""
    counts = {}
    with gzip.open(CHAT / model / f"2026-10-28-reading-{wording}.jsonl.gz", "rt", encoding="utf-8") as rows:
        for row in map(json.loads, rows):
            texts = shown[row["key"]]
            reported, found = counts.get(row["arm"], (0, 0))
            found += sum(copies_its_passage(row["lines"].get(str(label)), texts[label][0]) for label in row["missed"])
            counts[row["arm"]] = (reported + len(row["facts_noted"]), found)
    return {size: (reported, reported + found) for size, (reported, found) in counts.items()}


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    shown = passages()
    print("| Model | Wording, thinking | 2 | 4 | 8 | 16 | All at once |")
    print("|---|---|---|---|---|---|---|")
    for model, wording in RUNS:
        cells = rescored(model, wording, shown)
        row = [f"{cells[size][0]} → {cells[size][1]}" if size in cells else "" for size in SIZES]
        print(f"| {model} | {wording} | " + " | ".join(row) + " |")


if __name__ == "__main__":
    main()
