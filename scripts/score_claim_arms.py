"""Scores the claim judge's prompt arms and batch sizes (evals/grounding/2026-10-29-CLAIM-PROMPT-ARMS.md).

  python -I scripts/score_claim_arms.py [DIR]
  python -I scripts/score_claim_arms.py --paired [DIR]

DIR holds `<model>-<set>-<arm>-<repeat>.jsonl[.gz]` (default evals/grounding/results/2026-10-29-claim-arms) or
`<model>-<set>-b<N>.jsonl[.gz]`. Each cell is claims, gate right / false supported / false rejected, and the
claims the judge left unusable when any. Gate right is whether the verdict lands on the right side of supported.

--paired sets each arm against base on the real claims, on the batch orders both ran (repeat rK is the same order
for every arm), claim by claim: the change in claims right, false supports and false rejections, averaged over
those orders, with a 95% bootstrap interval over the 85 claims.
"""
import collections, glob, gzip, json, os, random, re, sys

ARMS = ["base", "fields", "parts", "closed-book", "brief", "named-parts", "no-paraphrase", "think-on", "think-off"]
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


def paired(root):
    runs = collections.defaultdict(dict)
    for path in sorted(glob.glob(os.path.join(root, "*.jsonl*"))):
        m = re.match(r"(.+?)-gold-w1-(.+)-(r\d+)$", re.sub(r"\.jsonl(\.gz)?$", "", os.path.basename(path)))
        if not m:
            continue
        scored = {r["id"]: ((r["verdict"] == "supported") == (r["label"] == "supported"),
                            r["verdict"] == "supported" and r["label"] != "supported",
                            r["verdict"] != "supported" and r["label"] == "supported") for r in rows(path)}
        if len(scored) == 85:
            runs[(m[1], m[2])][m[3]] = scored
    rng = random.Random(0)
    print("| Model | Arm | Orders | Right, change (95% CI) | False supported | False rejected |")
    print("|---|---|---|---|---|---|")
    for (model, arm), repeats in sorted(runs.items(), key=lambda kv: (kv[0][0], ARMS.index(kv[0][1]))):
        base = runs.get((model, "base"))
        common = sorted(set(repeats) & set(base or {}))
        if arm == "base" or not common:
            continue
        ids = sorted(base[common[0]])
        cells = []
        for j in range(3):
            d = [sum(repeats[k][i][j] - base[k][i][j] for k in common) / len(common) for i in ids]
            boot = sorted(sum(d[rng.randrange(len(ids))] for _ in ids) for _ in range(4000))
            cells.append(f"{sum(d):+.1f} ({boot[100]:+.1f} to {boot[3899]:+.1f})")
        print(f"| {model} | {arm} | {len(common)} | " + " | ".join(cells) + " |")


def main():
    if sys.argv[1:2] == ["--paired"]:
        return paired(sys.argv[2] if len(sys.argv) > 2 else ROOT)
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
