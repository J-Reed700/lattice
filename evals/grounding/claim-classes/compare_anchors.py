"""Compare the two arms of replay_anchors.py for one model.

  python -I compare_anchors.py DIR [--read N] [--db DB --program ID]

DIR holds current.jsonl and pointing.jsonl. Prints outcome counts, how each
statement's outcome moved between the arms, what `follows` carried, how many
sentences each supporting part points at, the corrections asked for and the
tokens spent. With --db and --program, also the relation each supported
statement shows by the design's rules. With --read, also prints N statements
of every moved class for reading by hand.
"""
import argparse
import collections
import json
import os
import random
import sys

PLAIN = str.maketrans({"‘": "'", "’": "'", "‛": "'", "“": '"', "”": '"', "‟": '"',
                       "–": "-", "—": "-", "−": "-", "*": None, "_": None, "`": None})


def fold(text):
    return " ".join((text or "").translate(PLAIN).split()).lower()


def records(path):
    out = {}
    with open(path, encoding="utf-8") as lines:
        for line in lines:
            if line.strip():
                record = json.loads(line)
                out[record["key"]] = record
    return out


def relation(record, passages):
    """The relation a supported statement shows, by the design's rules in order: its
    lesson text found word for word in a passage it was checked against quotes it;
    else a part that only follows from the sentences infers it; else one sentence
    under every stated part restates it; else the stated sentences summarize it."""
    lesson = fold(record.get("quote") or record["statement"])
    if lesson and any(lesson in fold(text) for text in passages):
        return "quotes"
    parts = record["answer"]["parts"]
    if any(p["kind"] == "follows" and p["anchors"] for p in parts):
        return "infers"
    stated = [{(a["source_id"], a["start"], a["end"]) for a in p["anchors"]} for p in parts if p["kind"] == "stated"]
    stated = [anchors for anchors in stated if anchors]
    if not stated:
        return "none"
    return "restates" if set.intersection(*stated) else "summarizes"


def count_rule(record):
    """The rejected split: one distinct stated sentence restates, more summarize."""
    anchors = {(a["source_id"], a["start"], a["end"]) for p in record["answer"]["parts"] if p["kind"] == "stated"
               for a in p["anchors"]}
    return "restates" if len(anchors) == 1 else "summarizes" if anchors else "none"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("dir")
    parser.add_argument("--read", type=int, default=0)
    parser.add_argument("--db", help="the app database the replay read, for the passages a quote is found in")
    parser.add_argument("--program")
    args = parser.parse_args()
    current = records(os.path.join(args.dir, "current.jsonl"))
    pointing = records(os.path.join(args.dir, "pointing.jsonl"))
    keys = sorted(set(current) & set(pointing))
    print(f"{len(keys)} statements in both arms ({len(current)} current, {len(pointing)} pointing)")

    for name, arm in (("current", current), ("pointing", pointing)):
        outcomes = collections.Counter(arm[k]["outcome"] for k in keys)
        saved = collections.Counter((arm[k].get("saved") or {}).get("verdict") for k in keys)
        agree = sum(1 for k in keys if arm[k]["outcome"] == {"supported": "supported", "unsupported": "unsupported",
                                                             "contradicted": "contradicted"}.get(
            (arm[k].get("saved") or {}).get("verdict"), "?"))
        corrected = sum(1 for k in keys if len(arm[k].get("calls", [])) > 1)
        tokens = [c["output_tokens"] or 0 for k in keys for c in arm[k].get("calls", [])]
        inputs = [c["input_tokens"] or 0 for k in keys for c in arm[k].get("calls", [])]
        print(f"\n{name}: outcomes {dict(outcomes)}")
        print(f"  saved verdicts {dict(saved)}; same outcome as saved {agree}/{len(keys)}")
        print(f"  corrected {corrected}; input tokens {sum(inputs)}; output tokens {sum(tokens)}"
              f" (mean {sum(tokens) / max(len(tokens), 1):.0f}, max {max(tokens or [0])}, over 600: "
              f"{sum(1 for t in tokens if t > 600)})")
        told = collections.Counter((arm[k]["calls"][0:1] and arm[k].get("told")) or "-" for k in keys
                                   if len(arm[k].get("calls", [])) > 1)
        if told:
            print(f"  left unanswered after the correction: {sum(1 for k in keys if arm[k]['outcome'] == 'unanswered')}")

    moves = collections.Counter((current[k]["outcome"], pointing[k]["outcome"]) for k in keys)
    print("\nmoves current -> pointing:")
    for (a, b), n in sorted(moves.items(), key=lambda item: -item[1]):
        print(f"  {a:>13} -> {b:<13} {n}")

    answered = [k for k in keys if pointing[k].get("answer")]
    kinds = collections.Counter(p["kind"] for k in answered for p in pointing[k]["answer"]["parts"])
    print(f"\npointing part kinds {dict(kinds)}")
    only_follows = [k for k in answered if pointing[k]["outcome"] == "supported"
                    and not any(p["kind"] == "stated" for p in pointing[k]["answer"]["parts"])]
    print(f"supported through follows alone: {len(only_follows)}")
    width = collections.Counter(len(p["anchors"]) for k in answered for p in pointing[k]["answer"]["parts"]
                                if p["kind"] in ("stated", "follows"))
    print(f"sentences per stated or follows part {dict(sorted(width.items()))}")
    spread = collections.Counter(len({a["source_id"] for p in pointing[k]["answer"]["parts"] for a in p["anchors"]
                                      if p["kind"] in ("stated", "follows")}) for k in answered
                                 if pointing[k]["outcome"] == "supported")
    print(f"sources per supported statement {dict(sorted(spread.items()))}")
    if args.db and args.program:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        from replay_anchors import load
        shown = {item["key"]: [p["text"] for p in item["passages"]] for item in load(args.db, args.program)}
        supported = [k for k in answered if pointing[k]["outcome"] == "supported"]
        relations = collections.Counter(relation(pointing[k], shown.get(k, [])) for k in supported)
        print(f"relations of supported statements {dict(relations)}")
        unquoted = [k for k in supported if relation(pointing[k], shown.get(k, [])) in ("restates", "summarizes")]
        print(f"  restates or summarizes by the count rule instead: {dict(collections.Counter(count_rule(pointing[k]) for k in unquoted))}")

    if args.read:
        rng = random.Random(7)
        for move in sorted(moves):
            if move[0] == move[1]:
                continue
            chosen = [k for k in keys if (current[k]["outcome"], pointing[k]["outcome"]) == move]
            rng.shuffle(chosen)
            print(f"\n######## {move[0]} -> {move[1]} ({len(chosen)})")
            for k in chosen[:args.read]:
                show(current[k], pointing[k])
        for name, chosen in (("follows alone", only_follows),):
            rng.shuffle(chosen)
            print(f"\n######## supported through {name} ({len(chosen)})")
            for k in chosen[:args.read]:
                show(current[k], pointing[k])


def show(current, pointing):
    print(f"\nCLAIM {pointing['statement']}")
    print(f"LESSON {pointing.get('quote')}")
    for name, record in (("current", current), ("pointing", pointing)):
        print(f"  [{name}] {record['outcome']}: {(record.get('answer') or {}).get('reason', record.get('told'))}")
        for part in (record.get("answer") or {}).get("parts", []):
            if name == "current":
                print(f"     - {part['kind']}: {part['part']} {part.get('passages')}")
            else:
                print(f"     - {part['kind']}: {part['words']}")
                for anchor in part["anchors"]:
                    print(f"         {anchor['handle']}: {anchor['text'][:220]}")


if __name__ == "__main__":
    main()
