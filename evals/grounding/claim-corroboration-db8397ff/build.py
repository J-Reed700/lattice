"""Build the claim-corroboration eval inputs for one lesson from a database copy.

Reads a copy of the app database (never the live one) and a copy of the
course's passage index, plus the hand labels in spec.json, and writes:

    pairs.jsonl   claim/page pairs for live_claim_pairs_eval
    recall.jsonl  claim -> primary-doc target passages for live_page_recall_eval
                  (LATTICE_PAGE_RECALL_TARGETS)

Usage:
    python3 -I build.py --db <copy of lattice.db> --index <copy of passages.db> [--out <dir>]

What each pair holds:

- `seen`: the page the checker's last decision rests on (the claim's stored
  evidence_json, label and text as the judge read them).
- `primary`, `primary-...-nolabel`: a passage of an official doc already in the
  course, cut the way the course index cuts it (the index chunk holding the
  quote, widened by verification_context's rule), with or without the source
  label the lesson check puts above a passage.
- `framed`: the claim written the way Citations::label writes a statement that
  names "Source 0" (`Source "<book title>" states: "<claim>"`).

Pass A (what the section cites) and pass B's first page come from the job
checkpoints: the claim-evidence-v2 selection saved for the claim's statement,
and the claim-v1 receipt of that page, whose keys are recomputed here.
"""
import argparse
import datetime
import hashlib
import json
import sqlite3
from pathlib import Path

HERE = Path(__file__).resolve().parent
POLICY = "lesson-evidence-v21"
RANKING = "two-pass-hybrid-v1"
TRUST = "source-trust-v1"
MODEL = "/models/Qwen3.8-27B-Q5_K_S.gguf"
CONTEXT = 131072
SURROUNDING_CHARS = 1700
# Public suffixes that take a third label (Origin::of_url uses the full list).
MULTI_LABEL_SUFFIXES = ("github.io", "co.uk", "com.au")


def digest(value):
    text = json.dumps(value, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(text.encode()).hexdigest()


def registrable(url):
    host = (url or "").split("//", 1)[-1].split("/", 1)[0].split(":", 1)[0].lower()
    labels = host.split(".")
    take = 3 if any(host.endswith("." + s) for s in MULTI_LABEL_SUFFIXES) else 2
    return ".".join(labels[-take:]) if host else None


def effective_date(dates):
    """lattice_core::domain::provenance::effective_date, seconds since the epoch."""
    acquired = dates.get("acquired_at")
    stated = [d for d in (dates.get("published"), dates.get("modified")) if d is not None and d <= acquired]
    if stated:
        return max(stated)
    for weak in (dates.get("http_last_modified"), dates.get("archived_at")):
        if weak is not None and weak <= acquired:
            return weak
    return None


class Course:
    def __init__(self, db, index, program_id):
        self.db, self.index, self.program_id = db, index, program_id
        self.rows = {}

    def version(self, prefix):
        row = self.db.execute(
            "SELECT id, title, coalesce(resolved_url, requested_url), full_text "
            "FROM learning_source_versions WHERE program_id = ? AND id LIKE ?",
            (self.program_id, prefix + "%"),
        ).fetchall()
        assert len(row) == 1, f"source version {prefix} is not unique"
        return row[0]

    def label(self, version_id):
        """provenance_gate::passage_labels: title, registrable domain, date."""
        _, title, url, _ = self.version(version_id)
        record = self.db.execute(
            "SELECT origin_json, signals_json FROM source_provenance WHERE program_id = ? AND source_version_id = ?",
            (self.program_id, version_id),
        ).fetchone()
        domain, date = registrable(url), None
        if record:
            domain = json.loads(record[0]).get("registrable_domain") or domain
            at = effective_date(json.loads(record[1]).get("dates") or {})
            if at is not None:
                date = datetime.datetime.fromtimestamp(at, datetime.timezone.utc).strftime("%Y-%m-%d")
        parts = [f'From "{title.strip()}"'] + ([domain] if domain else []) + ([f"dated {date}"] if date else [])
        return " · ".join(parts)

    def passage_around(self, prefix, quote):
        """The index chunk holding `quote`, widened as verification_context widens a hit."""
        version_id, _, _, text = self.version(prefix)
        at = text.find(quote)
        assert at >= 0 and text.count(quote) == 1, f"quote not unique in {prefix}: {quote!r}"
        raw = text.encode()
        q0 = len(text[:at].encode())
        q1 = q0 + len(quote.encode())
        chunks = self.index.execute(
            "SELECT span_start, span_end FROM passages WHERE document = ? ORDER BY span_start", (version_id,)
        ).fetchall()
        holding = [c for c in chunks if c[0] <= q0 and q1 <= c[1]] or [c for c in chunks if c[0] <= q0 < c[1]]
        middle = (q0 + q1) / 2
        s, e = min(holding, key=lambda c: abs((c[0] + c[1]) / 2 - middle)) if holding else (q0, q1)
        cs, ce = len(raw[:s].decode(errors="ignore")), len(raw[:e].decode(errors="ignore"))
        start = max(0, cs - SURROUNDING_CHARS)
        end = min(len(text), ce + SURROUNDING_CHARS)
        newline = text[start:cs].find("\n")
        if start != 0 and 0 <= newline <= 300:
            start += newline + 1
        newline = text[ce:end].rfind("\n")
        if end != len(text) and newline >= 0 and end - (ce + newline) <= 300:
            end = ce + newline
        piece = text[start:end]
        b0 = len(text[:start].encode())
        return {
            "source_version_id": version_id,
            "start_byte": b0,
            "end_byte": b0 + len(piece.encode()),
            "text": piece,
            "quote_span": [q0, q1],
        }


def selection_and_receipt(db, lesson_id, unit, statement, embedding, sources_text):
    """Pass B's saved first page for the statement and the judge's receipt for it."""
    key = lesson_id + "/claim-evidence-v2:" + digest({
        "policy": POLICY, "model": MODEL, "context": CONTEXT, "unit": unit, "statement": statement,
        "retrieval": "hybrid", "embedding": embedding, "ranking": RANKING,
    })
    row = db.execute("SELECT data_json FROM job_checkpoints WHERE key = ? ORDER BY updated_at DESC LIMIT 1", (key,)).fetchone()
    if not row:
        return None, None
    selection = json.loads(row[0])
    lead = {"researched": 0, "cited": 1}
    page = []
    for p in selection["passages"]:
        text = sources_text(p["source_id"]).encode()[p["start"]:p["end"]].decode()
        page.append((p["source_id"], text, p["start"], p["end"], p.get("kind") or "hybrid"))
    page.sort(key=lambda x: (lead.get(x[4], 2), x[0], x[2], x[3], x[1]))
    comparison = digest({"unit": unit, "statement": statement, "passages": [[a, b, c, d] for a, b, c, d, _ in page]})
    receipt_key = lesson_id + "/claim-v1:" + digest(
        {"policy": POLICY, "model": MODEL, "context": CONTEXT, "comparison": comparison, "trust": TRUST}
    )
    receipt = db.execute("SELECT data_json FROM job_checkpoints WHERE key = ? LIMIT 1", (receipt_key,)).fetchone()
    return selection | {"page": page}, (json.loads(receipt[0]) if receipt else None)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True)
    parser.add_argument("--index", required=True)
    parser.add_argument("--spec", default=str(HERE / "spec.json"))
    parser.add_argument("--out", default=str(HERE))
    args = parser.parse_args()
    spec = json.loads(Path(args.spec).read_text())
    lesson_id, program_id = spec["lesson_id"], spec["program_id"]
    db = sqlite3.connect(f"file:{args.db}?mode=ro", uri=True)
    index = sqlite3.connect(f"file:{args.index}?mode=ro", uri=True)
    embedding = index.execute("SELECT value FROM meta WHERE key = 'identity'").fetchone()[0]
    course = Course(db, index, program_id)
    texts = {}

    def sources_text(version_id):
        if version_id not in texts:
            texts[version_id] = db.execute(
                "SELECT full_text FROM learning_source_versions WHERE program_id = ? AND id = ?", (program_id, version_id)
            ).fetchone()[0]
        return texts[version_id]

    claims = {}
    for claim_id, unit, statement, verdict, evidence in db.execute(
        "SELECT claim_id, unit_index, statement, verdict_kind, evidence_json FROM learning_lesson_claims "
        "WHERE lesson_id = ? AND json_extract(state_json, '$.state') IN ('queued', 'open') ORDER BY unit_index, claim_id",
        (lesson_id,),
    ):
        selection, receipt = selection_and_receipt(db, lesson_id, unit, statement, embedding, sources_text)
        seen = json.loads(evidence)
        b_spans = {(p[0], p[2], p[3]) for p in (selection or {}).get("page", [])}
        cited = [
            {"source_version_id": p["sourceVersionId"], "start_byte": p["startByte"], "end_byte": p["endByte"]}
            for p in seen
            if not p["sourceVersionId"].startswith("block-")
            and (p["sourceVersionId"], p["startByte"], p["endByte"]) not in b_spans
        ]
        claims[claim_id[:8]] = {
            "claim_id": claim_id, "unit": unit, "statement": statement, "verdict": verdict, "seen": seen,
            "cited": cited, "researched": (selection or {}).get("researched", []),
            "pass_b_verdict": (receipt or {}).get("verdict"),
        }
    # A claim whose page holds no pass A passage takes its section's, as the
    # other waiting claims of the section show it.
    for c in claims.values():
        if not c["cited"]:
            seen_spans = set()
            for other in claims.values():
                for p in other["cited"] if other["unit"] == c["unit"] else []:
                    span = (p["source_version_id"], p["start_byte"], p["end_byte"])
                    if span not in seen_spans:
                        seen_spans.add(span)
                        c["cited"].append(p)

    pairs, recall = [], {}

    def pair(pair_id, claim, kind, passages, entry, extra=None):
        pairs.append({
            "id": pair_id, "claim_id": claim["claim_id"], "lesson_id": lesson_id, "kind": kind,
            "claim": extra.pop("claim_text") if extra and "claim_text" in extra else claim["statement"],
            "passages": passages, "label": entry["label"], "difficulty": entry["difficulty"],
            "note": entry["note"], "pipeline_verdict": claim["verdict"],
        } | (extra or {}))

    for short, entry in spec["seen"].items():
        claim = claims[short]
        page = []
        for p in claim["seen"]:
            block = p["sourceVersionId"].startswith("block-")
            page.append(({} if block else {"label": course.label(p["sourceVersionId"])}) | {"text": p["text"]})
        pair(f"cc-{short}-seen", claim, "seen", page, entry,
             {"source_version_ids": [p["sourceVersionId"] for p in claim["seen"]]})

    for entry in spec["primary"]:
        claim = claims[entry["claim"]]
        cut = course.passage_around(entry["source"], entry["quote"])
        meta = {"source_version_ids": [cut["source_version_id"]], "span": [cut["start_byte"], cut["end_byte"]]}
        base = f"cc-{entry['claim']}-{entry['suffix']}"
        pair(base, claim, "primary", [{"label": course.label(cut["source_version_id"]), "text": cut["text"]}], entry, dict(meta))
        if entry.get("unlabelled"):
            pair(base + "-nolabel", claim, "primary_unlabelled", [{"text": cut["text"]}], entry, dict(meta))
        if entry.get("recall"):
            row = recall.setdefault(entry["claim"], {
                "id": f"cc-recall-{entry['claim']}", "claim_id": claim["claim_id"], "lesson_id": lesson_id,
                "program_id": program_id, "unit": claim["unit"], "claim": claim["statement"],
                "cited": claim["cited"], "targets": [], "researched": claim["researched"],
            })
            q0, q1 = cut["quote_span"]
            row["targets"].append({
                "source_version_id": cut["source_version_id"], "start_byte": q0, "end_byte": q1,
                "quote": entry["quote"],
            })

    book_title = course.version(spec["book"])[1].strip()
    for entry in spec["framed"]:
        claim = claims[entry["claim"]]
        cut = course.passage_around(entry["source"], entry["quote"])
        framed = f'Source "{book_title}" states: "{claim["statement"]}"'
        pair(f"cc-{entry['claim']}-framed-{entry['suffix']}", claim, "framed",
             [{"label": course.label(cut["source_version_id"]), "text": cut["text"]}], entry,
             {"claim_text": framed, "source_version_ids": [cut["source_version_id"]],
              "span": [cut["start_byte"], cut["end_byte"]]})

    for row in recall.values():
        row["in_research_scope"] = any(t["source_version_id"] in row["researched"] for t in row["targets"])
    out = Path(args.out)
    (out / "pairs.jsonl").write_text("".join(json.dumps(p, ensure_ascii=False) + "\n" for p in pairs))
    (out / "recall.jsonl").write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in recall.values()))
    labels = {}
    for p in pairs:
        labels.setdefault(p["kind"], {}).setdefault(p["label"], 0)
        labels[p["kind"]][p["label"]] += 1
    print(f"{len(pairs)} pairs {labels}; {len(recall)} recall claims; "
          f"{sum(c['pass_b_verdict'] is not None for c in claims.values())}/{len(claims)} pass B receipts found")


if __name__ == "__main__":
    main()
