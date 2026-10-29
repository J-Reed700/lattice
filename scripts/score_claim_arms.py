"""Scores the claim judge's prompt arms and batch sizes (evals/grounding/2026-10-29-CLAIM-PROMPT-ARMS.md).

  python -I scripts/score_claim_arms.py [DIR]

DIR holds `<model>-<set>-<arm>-<repeat>.jsonl[.gz]` (default evals/grounding/results/2026-10-29-claim-arms) or
`<model>-<set>-b<N>.jsonl[.gz]`. Each cell is claims, gate right / false supported / false rejected, and the
claims the judge left unusable when any. Gate right is whether the verdict lands on the right side of supported.
"""
import collections, glob, gzip, json, os, re, sys

ARMS = ["base", "fields", "parts", "closed-book", "brief", "think-on", "think-off"]
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "evals", "grounding", "results",
                    "2026-10-29-claim-arms")


def rows(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as lines:
        by_id = {}
        for line in lines:
            if line.strip():
                row = json.loads(line)
                by_id.setdefault(row["id"], row)
    return list(by_id.values())


def score(path):
    scored = rows(path)
    gate = sum((r["verdict"] == "supported") == (r["label"] == "supported") for r in scored)
    false_supported = sum(r["verdict"] == "supported" and r["label"] != "supported" for r in scored)
    false_rejected = sum(r["verdict"] != "supported" and r["label"] == "supported" for r in scored)
    unusable = sum(r["verdict"] == "unverified" for r in scored)
    cell = f"{len(scored)}, **{gate}** / {false_supported} / {false_rejected}"
    return cell + (f" ({unusable})" if unusable else "")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else ROOT
    cells = collections.defaultdict(list)
    for path in sorted(glob.glob(os.path.join(root, "*.jsonl*"))):
        name = re.sub(r"\.jsonl(\.gz)?$", "", os.path.basename(path))
        m = re.match(r"(.+?)-(gold-w1|claim-pairs-v1)-(.+)$", name)
        if not m:
            continue
        arm, repeat = (m[3].rsplit("-", 1) + [""])[:2] if re.search(r"-r\d+$", m[3]) else (m[3], "")
        cells[(m[1], m[2], arm)].append((repeat, score(path)))

    def order(key):
        arm = key[2]
        batch = re.fullmatch(r"b(\d+)", arm)
        return key[0], key[1], int(batch[1]) if batch else ARMS.index(arm) if arm in ARMS else 99

    print("| Model | Set | Arm | Claims, gate right / false supported / false rejected (unusable) |")
    print("|---|---|---|---|")
    for key in sorted(cells, key=order):
        runs = "; ".join(f"{repeat}: {cell}" if repeat else cell for repeat, cell in sorted(cells[key]))
        print(f"| {key[0]} | {key[1]} | {key[2]} | {runs} |")


if __name__ == "__main__":
    main()
