#!/usr/bin/env python3
"""The terse slice of synthetic-library-v3: the same questions, asked the way people type.

Synthetic queries are longer than real ones and reuse the document's own words,
and tuning retrieval on them picks a setup whose keyword side helps only the
long questions (arXiv 2609.14579). Each query in
`evals/retrieval/synthetic-library-v3-terse.json` asks what one v3 query asks
and keeps its relevance judgements and answer spans; its length follows the
measured length of real Lattice messages, and it avoids the answering
document's distinctive words, keeping only the names and identifiers a person
would have to type.

Commands:
  targets SLICE                     each source query's target length, for authoring
  check SLICE                       coverage, length match and keyword overlap
  build SLICE OUT                   the slice as a dataset retrieval_eval and rag_eval read
  compare SLICE V3_RUN TERSE_RUN    the slice against the same v3 queries, paired
"""
import argparse
import json
import math
import re
import statistics
from collections import Counter
from pathlib import Path

from exact_stats import ALPHA, sign_test_p
from rag_eval import score

ROOT = Path(__file__).resolve().parent.parent
TOKEN = re.compile(r"\w+", re.UNICODE)

# The measured counts are English words split on whitespace, which measures
# neither CJK text nor a query in another language, so those categories stay
# out of the slice.
EXCLUDED_CATEGORIES = {"same-language", "short CJK query"}

# The per-document cap cases (added to v3 on 2026-10-23) measure how many passages of
# one document survive, not how a query is phrased, and the slice was written before
# they existed. Leaving them out keeps the slice's population, and so every length
# target, exactly as it was.
EXCLUDED_DIMENSIONS = {"many passages from one document", "repetitive document"}


def words(text):
    return len(text.split())


def load(path):
    terse = json.loads(Path(path).read_text())
    source = json.loads((ROOT / terse["source"]).read_text())
    return terse, source


def eligible(source):
    return [q for q in source["queries"]
            if any(grade > 0 for grade in q["relevance"].values()) and q["category"] not in EXCLUDED_CATEGORIES
            and not EXCLUDED_DIMENSIONS & set(q.get("dimensions", ()))]


def reference_lengths(terse):
    """The short half of the measured messages: those at or below the median.

    The whole measured distribution is no shorter than v3's own, so matching
    all of it would not make a terse slice; the short half is the population
    the literature finds synthetic sets leave out.
    """
    counts = sorted(terse["length_reference"]["word_counts"])
    middle = statistics.median(counts)
    return [count for count in counts if count <= middle]


def length_targets(terse, source):
    """Quantile mapping: the r-th shortest source query gets the reference
    length at the same quantile, so the slice's lengths follow the reference
    while longer questions stay longer."""
    reference = reference_lengths(terse)
    queries = sorted(eligible(source), key=lambda q: (words(q["text"]), q["id"]))
    return {q["id"]: reference[int((rank + 0.5) / len(queries) * len(reference))]
            for rank, q in enumerate(queries)}


def quantiles(values):
    values = sorted(values)
    pick = lambda p: values[min(len(values) - 1, int(p * len(values)))]
    return {"p10": pick(.1), "p25": pick(.25), "median": statistics.median(values),
            "p75": pick(.75), "p90": pick(.9), "mean": round(statistics.fmean(values), 1)}


def idf_table(source):
    documents = source["documents"]
    frequency = Counter(token for doc in documents for token in set(TOKEN.findall(doc["text"].lower())))
    return lambda token: math.log((len(documents) + 1) / (frequency.get(token, 0) + 1))


def keyword_overlap(text, relevant_texts, idf):
    """Share of the query's IDF weight carried by words that also occur in a
    relevant document: 1 means every distinctive word is the document's own."""
    tokens = set(TOKEN.findall(text.lower()))
    present = set().union(*(set(TOKEN.findall(t.lower())) for t in relevant_texts)) if relevant_texts else set()
    total = sum(idf(t) for t in tokens)
    return sum(idf(t) for t in tokens & present) / total if total else 0.0


def check(terse, source):
    problems = []
    by_id = {q["id"]: q for q in source["queries"]}
    wanted = {q["id"] for q in eligible(source)}
    seen = Counter(q["source_id"] for q in terse["queries"])
    problems += [f"{qid}: written {n} times" for qid, n in seen.items() if n > 1]
    problems += [f"{qid}: not an eligible v3 query" for qid in seen if qid not in wanted]
    problems += [f"{qid}: has no terse version" for qid in sorted(wanted - set(seen))]
    for query in terse["queries"]:
        original = by_id.get(query["source_id"])
        if original and query["text"].strip().lower() == original["text"].strip().lower():
            problems.append(f"{query['source_id']}: identical to the v3 text")
    targets = length_targets(terse, source)
    documents = {d["id"]: d["text"] for d in source["documents"]}
    idf = idf_table(source)

    def overlap(text, qid):
        relevant = [documents[d] for d, g in by_id[qid]["relevance"].items() if g > 0]
        return keyword_overlap(text, relevant, idf)

    written = [q for q in terse["queries"] if q["source_id"] in targets]
    return {
        "problems": problems,
        "queries": len(written),
        "words": {"v3": quantiles([words(by_id[q["source_id"]]["text"]) for q in written]),
                  "target": quantiles([targets[q["source_id"]] for q in written]),
                  "terse": quantiles([words(q["text"]) for q in written])},
        "mean_abs_words_off_target": round(statistics.fmean(
            abs(words(q["text"]) - targets[q["source_id"]]) for q in written), 2),
        "keyword_overlap": {
            "v3": round(statistics.fmean(overlap(by_id[q["source_id"]]["text"], q["source_id"]) for q in written), 3),
            "terse": round(statistics.fmean(overlap(q["text"], q["source_id"]) for q in written), 3)},
    }


def build(terse, source):
    by_id = {q["id"]: q for q in source["queries"]}
    queries = [{**by_id[q["source_id"]], "text": q["text"]} for q in terse["queries"]]
    return {"name": terse["name"], "description": terse["description"],
            "dimensions": source.get("dimensions", {}), "documents": source["documents"],
            "queries": queries}


def subset(source, ids):
    return {**source, "queries": [q for q in source["queries"] if q["id"] in ids]}


def recall_at(row, query, k, branch=None):
    """recall@k of the fused ranking, or of one branch (`vector`, `bm25`) when
    the production-mode row records it."""
    ranked = row["ranked_ids"] if branch is None else list(dict.fromkeys(row["branch_ranked_ids"][branch]))
    positives = {doc for doc, grade in query["relevance"].items() if grade > 0}
    return len(set(ranked[:k]) & positives) / len(positives)


def compare(terse, source, v3_rows, terse_rows, k=5, alpha=ALPHA):
    """Aggregates on the same queries both ways, and a paired sign test on
    each query's recall@k: did the terse wording find more, less or the same?"""
    ids = {q["source_id"] for q in terse["queries"]}
    v3_rows = [row for row in v3_rows if row["query_id"] in ids]
    original = subset(source, ids)
    slice_dataset = build(terse, source)
    queries = {q["id"]: q for q in original["queries"]}
    before = {row["query_id"]: recall_at(row, queries[row["query_id"]], k) for row in v3_rows}
    after = {row["query_id"]: recall_at(row, queries[row["query_id"]], k) for row in terse_rows}
    better = sum(after[q] > before[q] for q in ids)
    worse = sum(after[q] < before[q] for q in ids)
    p = sign_test_p(better, worse)
    keep = ("recall_at_k", "ndcg_at_k", "mrr_at_k", "passage_recall_at_k")

    def headline(result):
        return {key: result.get(key) for key in keep}

    v3_scores, terse_scores = score(original, v3_rows, k), score(slice_dataset, terse_rows, k)
    branches = sorted(set.intersection(*(set(row.get("branch_ranked_ids") or {}) for row in v3_rows + terse_rows)))
    branch_recall = {branch: {arm: statistics.fmean(recall_at(row, queries[row["query_id"]], k, branch)
                                                   for row in rows)
                              for arm, rows in (("v3", v3_rows), ("terse", terse_rows))}
                     for branch in branches}
    return {
        "queries": len(ids),
        "v3": headline(v3_scores),
        "terse": headline(terse_scores),
        "by_dimension": {dim: {"v3": headline(v3_scores["by_dimension"][dim]),
                               "terse": headline(terse_scores["by_dimension"].get(dim, {}))}
                         for dim in v3_scores["by_dimension"]},
        "branch_recall_at_k": branch_recall,
        "recall_changes": {"better": better, "worse": worse, "same": len(ids) - better - worse},
        "p_value": p,
        "verdict": ("can't tell from noise" if p > alpha or better == worse
                    else "terse is better" if better > worse else "terse is worse"),
        "lost": sorted(q for q in ids if after[q] < before[q]),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("targets", "check"):
        sub.add_parser(name).add_argument("slice", type=Path)
    out = sub.add_parser("build")
    out.add_argument("slice", type=Path)
    out.add_argument("output", type=Path)
    pair = sub.add_parser("compare")
    pair.add_argument("slice", type=Path)
    pair.add_argument("v3_run", type=Path)
    pair.add_argument("terse_run", type=Path)
    pair.add_argument("--k", type=int, default=5)
    args = parser.parse_args()
    terse, source = load(args.slice)
    if args.command == "targets":
        texts = {q["id"]: q["text"] for q in source["queries"]}
        for qid, target in sorted(length_targets(terse, source).items(), key=lambda kv: (kv[1], kv[0])):
            print(f"{target}\t{qid}\t{texts[qid]}")
    elif args.command == "check":
        report = check(terse, source)
        print(json.dumps(report, indent=2, ensure_ascii=False))
        raise SystemExit(1 if report["problems"] else 0)
    elif args.command == "build":
        args.output.write_text(json.dumps(build(terse, source), ensure_ascii=False) + "\n")
    else:
        read = lambda path: [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
        print(json.dumps(compare(terse, source, read(args.v3_run), read(args.terse_run), args.k), indent=2))


if __name__ == "__main__":
    main()
