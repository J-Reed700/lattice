#!/usr/bin/env python3
"""Build independence-pairs-v1.jsonl from corpus/independence-base-v1.json.

Every pair is derived from hand-written base texts by a named construction, and
its label follows from the construction (README, "Labelling rule"):

  copy         the same text: verbatim, reformatted, with a short site
               boilerplate added, or with a few word edits (at most 4% of words)
  contains     one text holds the other whole (syndication with commentary of
               at least 60% of the original's length) or holds a contiguous
               excerpt of it (30-60% of the larger text)
  independent  different texts: independently written texts on one topic,
               texts that share one quoted sentence, a text and its
               translation, texts that share a long site template, and texts on
               different topics

Deterministic: a fixed seed, no clock. Run from anywhere:

    python3 -I evals/provenance/build_independence_pairs.py
"""

import json
import random
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
BASE = HERE / "corpus" / "independence-base-v1.json"
OUT = HERE / "independence-pairs-v1.jsonl"
SEED = 20261018

# Words inserted or substituted by a light edit: function words, so an edit
# changes wording without adding content.
FILLERS = ["also", "really", "often", "just", "still", "then", "quite", "very"]


def words(text):
    return text.split()


def light_edit(text, rate, rng):
    """Apply round(rate * words) single-word edits: substitute, delete or insert."""
    tokens = words(text)
    edits = max(1, round(rate * len(tokens)))
    for _ in range(edits):
        op = rng.choice(["substitute", "delete", "insert"])
        at = rng.randrange(len(tokens))
        if op == "substitute":
            tokens[at] = rng.choice(FILLERS)
        elif op == "delete" and len(tokens) > 1:
            del tokens[at]
        else:
            tokens.insert(at, rng.choice(FILLERS))
    return " ".join(tokens)


def char_edit(text, rate, rng):
    """The character-script analogue of light_edit."""
    chars = list(text)
    edits = max(1, round(rate * len(chars)))
    for _ in range(edits):
        at = rng.randrange(len(chars))
        chars[at] = rng.choice("的了是在和也")
    return "".join(chars)


def reformat(text):
    """Markdown and whitespace changes only: what a re-host often does."""
    sentences = re.split(r"(?<=[.!?])\s+", text)
    half = len(sentences) // 2
    first = " ".join(sentences[:half])
    second = " ".join(sentences[half:])
    return f"## {first[:40].strip()}\n\n{first}\n\n  {second}\n"


def excerpt(text, share, rng):
    tokens = words(text)
    size = max(8, round(share * len(tokens)))
    start = rng.randrange(0, len(tokens) - size + 1)
    return " ".join(tokens[start : start + size])


def char_excerpt(text, share, rng):
    size = max(10, round(share * len(text)))
    start = rng.randrange(0, len(text) - size + 1)
    return text[start : start + size]


def commentary_for(text, pool, rng, char_script=False):
    """Commentary at least 60% of the original's length, from the pool."""
    measure = len if char_script else (lambda t: len(words(t)))
    needed = 0.6 * measure(text)
    chosen = []
    candidates = pool[:]
    rng.shuffle(candidates)
    for paragraph in candidates:
        chosen.append(paragraph)
        if sum(measure(p) for p in chosen) >= needed:
            break
    return chosen


def build():
    base = json.loads(BASE.read_text(encoding="utf-8"))
    docs = {d["id"]: d for d in base["docs"]}
    english = [d for d in base["docs"] if d["lang"] == "en"]
    cjk = [d for d in base["docs"] if d["lang"] in ("zh", "ja")]
    translations = [d for d in base["docs"] if "translation_of" in d]
    commentary = base["commentary"]
    boiler = base["boilerplate"]
    quotations = base["quotations"]
    rng = random.Random(SEED)
    pairs = []

    def add(label, construction, a, b, note):
        pairs.append(
            {
                "id": f"ip-{len(pairs) + 1:03d}",
                "label": label,
                "construction": construction,
                "a": a,
                "b": b,
                "note": note,
            }
        )

    # Copies: one per English text, cycling through the constructions, and a
    # second, edited copy for every other text.
    copy_kinds = ["verbatim", "reformat", "boilerplate", "edit-1", "edit-2", "edit-4"]
    for index, doc in enumerate(english):
        kind = copy_kinds[index % len(copy_kinds)]
        text = doc["text"]
        if kind == "verbatim":
            b = text
        elif kind == "reformat":
            b = reformat(text)
        elif kind == "boilerplate":
            b = f"{boiler['short']}\n\n{text}\n\n{boiler['short']}"
        else:
            b = light_edit(text, int(kind.split("-")[1]) / 100, rng)
        add("copy", kind, text, b, f"{doc['id']}: {kind}")
        if index % 2 == 1:
            rate = rng.choice([1, 2, 3, 4])
            add(
                "copy",
                f"edit-{rate}",
                light_edit(text, rate / 100, rng),
                text,
                f"{doc['id']}: {rate}% word edits",
            )

    # Contains: syndication with commentary for two thirds of the English
    # texts, an excerpt inside the full text for the rest and half again.
    for index, doc in enumerate(english):
        text = doc["text"]
        if index % 3 != 2:
            notes = commentary_for(text, commentary, rng)
            wrapped = "\n\n".join([notes[0], text] + notes[1:])
            add("contains", "syndication", text, wrapped, f"{doc['id']} inside commentary")
        if index % 3 != 1:
            share = rng.choice([0.3, 0.4, 0.5, 0.6])
            add(
                "contains",
                f"excerpt-{int(share * 100)}",
                excerpt(text, share, rng),
                text,
                f"{doc['id']}: a {int(share * 100)}% excerpt",
            )

    # Character scripts: copies, contains and independent pairs.
    for doc in cjk:
        text = doc["text"]
        add("copy", "verbatim", text, text, f"{doc['id']}: verbatim")
        add("copy", "char-edit-2", text, char_edit(text, 0.02, rng), f"{doc['id']}: 2% character edits")
        add("copy", "boilerplate", text, f"{boiler['zh']}\n\n{text}", f"{doc['id']}: site boilerplate")
        share = rng.choice([0.4, 0.5])
        add(
            "contains",
            f"excerpt-{int(share * 100)}",
            char_excerpt(text, share, rng),
            text,
            f"{doc['id']}: a {int(share * 100)}% excerpt",
        )
        notes = commentary_for(text, [d["text"] for d in cjk if d["id"] != doc["id"]], rng, True)
        add("contains", "syndication", text, "\n\n".join([text] + notes), f"{doc['id']} inside other text")
    for i, a in enumerate(cjk):
        for b in cjk[i + 1 :]:
            add("independent", "different-text-cjk", a["text"], b["text"], f"{a['id']} vs {b['id']}")

    # Independent: every pair of independently written texts on one topic.
    topics = {}
    for doc in english:
        topics.setdefault(doc["topic"], []).append(doc)
    for topic, group in topics.items():
        for i, a in enumerate(group):
            for b in group[i + 1 :]:
                add("independent", "same-topic", a["text"], b["text"], f"{topic}: {a['id']} vs {b['id']}")

    # Independent: two texts that quote the same sentence of a third source.
    quote_pairs = [
        ("http-caching-1", "http-caching-3", 0),
        ("http-caching-2", "http-caching-3", 0),
        ("hypertension-2", "hypertension-3", 1),
        ("vitamin-d-1", "vitamin-d-2", 1),
        ("gdpr-erasure-1", "gdpr-erasure-2", 2),
        ("gdpr-erasure-2", "gdpr-erasure-3", 2),
        ("rust-ownership-1", "rust-ownership-2", 3),
        ("rust-ownership-2", "rust-ownership-3", 3),
    ]
    for a_id, b_id, q in quote_pairs:
        quote = quotations[q]
        a = f"{docs[a_id]['text']} As {quote['of']} puts it, \"{quote['text']}\""
        b = f"\"{quote['text']}\" ({quote['of']}). {docs[b_id]['text']}"
        add("independent", "shared-quotation", a, b, f"{a_id} and {b_id} both quote {quote['of']}")

    # Independent: a text and its translation.
    for doc in translations:
        original = docs[doc["translation_of"]]
        add("independent", "translation", original["text"], doc["text"], f"{original['id']} vs its {doc['lang']} translation")

    # Independent: different texts inside the same long site template.
    for i in range(0, 24, 2):
        a, b = english[i], english[i + 1]
        wrap = lambda t: f"{boiler['long']}\n\n{t}\n\n{boiler['long']}"
        add("independent", "shared-template", wrap(a["text"]), wrap(b["text"]), f"{a['id']} and {b['id']} in one site template")

    # Independent: texts on different topics.
    for i in range(12):
        a = english[i]
        b = english[(i * 7 + 5) % len(english)]
        if a["topic"] == b["topic"]:
            continue
        add("independent", "different-topic", a["text"], b["text"], f"{a['id']} vs {b['id']}")

    return pairs


def main():
    pairs = build()
    with OUT.open("w", encoding="utf-8") as out:
        for pair in pairs:
            out.write(json.dumps(pair, ensure_ascii=False) + "\n")
    counts = {}
    for pair in pairs:
        counts[pair["label"]] = counts.get(pair["label"], 0) + 1
    print(f"{len(pairs)} pairs -> {OUT.name}: {counts}")


if __name__ == "__main__":
    main()
