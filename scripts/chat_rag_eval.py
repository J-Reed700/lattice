#!/usr/bin/env python3
"""Run the answer-producing half of Lattice RAG against an OpenAI-compatible llama.cpp server.

The evaluator consumes a labeled corpus and a retrieval JSONL produced by the
Rust `retrieval_eval` example. It renders the same default system/RAG prompts
and numeric source layout as desktop chat, calls `/v1/chat/completions`, and
scores facts, citations, abstention, and latency. Credentials are read at run
time and are never copied into output artifacts.
"""

import argparse
import json
import re
import statistics
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


DEFAULT_SYSTEM_PROMPT = (
    "You are Lattice, a precise research assistant. Use the user's documents "
    "when available. Cite sources using numeric brackets like [1], [2], [3]. "
    "Never fabricate document IDs."
)

DEFAULT_RAG_PROMPT = (
    "Answer the user's question using only the provided context. Cite every "
    "factual statement supported by the context using numeric brackets like "
    "[1], [2], [3]. If the context does not contain the answer, say that the "
    "answer is not available in the provided documents and do not guess. When "
    "the answer is unavailable, respond concisely without summarizing or citing "
    "unrelated context. Do not cite a source that does not support the associated "
    "statement. If you "
    "need to call get_document, use the exact Document ID shown in the context. "
    "For long documents, request additional pages with the page parameter.\n\n"
    "Context:\n{context}\n\nQuestion: {question}\n\nAnswer:"
)

DEFAULT_CONFIG_CANDIDATES = (
    Path("~/.opencode/config/config.yaml").expanduser(),
    Path("~/.config/opencode/config.yaml").expanduser(),
    Path("~/.config/opencode/opencode.json").expanduser(),
)

ABSTENTION_RE = re.compile(
    r"\b(?:not (?:provided|available|found|specified)|cannot determine|can't "
    r"determine|do not have|don't have|does not (?:provide|specify|contain)|"
    r"isn't specified|no information|unable to determine)\b",
    re.IGNORECASE,
)
CITATION_RE = re.compile(r"(?<!\^)\[(\d+)\]")


def load_structured(path):
    text = Path(path).read_text()
    if Path(path).suffix.lower() == ".json":
        return json.loads(text)
    try:
        import yaml  # type: ignore
    except ImportError as error:
        raise RuntimeError(
            f"{path} is YAML, but PyYAML is unavailable; use the OpenCode JSON config"
        ) from error
    return yaml.safe_load(text)


def find_config(explicit=None):
    if explicit:
        path = Path(explicit).expanduser()
        if not path.is_file():
            raise FileNotFoundError(path)
        return path
    for candidate in DEFAULT_CONFIG_CANDIDATES:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError("No OpenCode config found in the supported locations")


def provider_config(config, provider_name=None):
    providers = config.get("provider", {})
    if not isinstance(providers, dict) or not providers:
        raise ValueError("OpenCode config has no providers")
    selected = provider_name
    configured_model = str(config.get("model", ""))
    if selected is None and "/" in configured_model:
        selected = configured_model.split("/", 1)[0]
    if selected is None:
        selected = next(iter(providers))
    if selected not in providers:
        raise ValueError(f"Provider {selected!r} is absent from the OpenCode config")

    provider = providers[selected]
    options = provider.get("options", {})
    base_url = str(options.get("baseURL") or options.get("baseUrl") or "").rstrip("/")
    parsed = urllib.parse.urlparse(base_url)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ValueError("Provider baseURL must be an absolute HTTP(S) URL")
    models = provider.get("models", {})
    model = configured_model.split("/", 1)[-1] if configured_model else ""
    if not model and isinstance(models, dict) and models:
        model = next(iter(models))
    if not model:
        raise ValueError("Provider has no configured model")
    headers = options.get("headers", {})
    if not isinstance(headers, dict):
        raise ValueError("Provider headers must be an object")
    return {
        "provider": selected,
        "base_url": base_url,
        "model": model,
        "headers": {str(k): str(v) for k, v in headers.items()},
        "temperature": float(options.get("temperature", 0.2)),
        "top_p": float(options.get("topP", 0.9)),
        "top_k": int(options.get("topK", 40)),
    }


def chat_completions_url(base_url):
    base = base_url.rstrip("/")
    if base.endswith("/v1"):
        return base + "/chat/completions"
    return base + "/v1/chat/completions"


def load_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def render_context(ranked_ids, documents, top_k):
    selected = []
    seen = set()
    for document_id in ranked_ids:
        if document_id in documents and document_id not in seen:
            selected.append(document_id)
            seen.add(document_id)
        if len(selected) >= top_k:
            break
    context = "\n\n".join(
        f"[{index}] Document: {documents[document_id].get('title', document_id)}\n"
        f"Document ID: {document_id}\nContent: {documents[document_id]['text']}"
        for index, document_id in enumerate(selected, 1)
    )
    return selected, context


def request_answer(provider, question, context, timeout, max_tokens):
    body = {
        "model": provider["model"],
        "messages": [
            {"role": "system", "content": DEFAULT_SYSTEM_PROMPT},
            {
                "role": "user",
                "content": DEFAULT_RAG_PROMPT.format(context=context, question=question),
            },
        ],
        "stream": False,
        "temperature": provider["temperature"],
        "top_p": provider["top_p"],
        "top_k": provider["top_k"],
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": False},
    }
    headers = {"Content-Type": "application/json", **provider["headers"]}
    request = urllib.request.Request(
        chat_completions_url(provider["base_url"]),
        data=json.dumps(body).encode(),
        headers=headers,
        method="POST",
    )
    started = time.perf_counter()
    payload = None
    last_error = None
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                payload = json.load(response)
            break
        except urllib.error.HTTPError as error:
            # The body can contain echoed prompt text or server internals.
            last_error = RuntimeError(f"llama.cpp returned HTTP {error.code}")
            if error.code not in (429, 502, 503, 504) or attempt == 2:
                raise last_error from error
            retry_after = error.headers.get("Retry-After")
            try:
                delay = min(float(retry_after), 30) if retry_after else 2**attempt
            except ValueError:
                delay = 2**attempt
            time.sleep(delay)
        except urllib.error.URLError as error:
            last_error = RuntimeError("llama.cpp connection failed")
            if attempt == 2:
                raise last_error from error
            time.sleep(2**attempt)
    if payload is None:
        raise last_error or RuntimeError("llama.cpp request failed")
    elapsed_ms = (time.perf_counter() - started) * 1000
    try:
        answer = payload["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as error:
        raise RuntimeError("llama.cpp response is missing choices[0].message.content") from error
    if not isinstance(answer, str) or not answer.strip():
        raise RuntimeError("llama.cpp returned an empty answer")
    answer = re.sub(r"<think>.*?</think>", "", answer, flags=re.DOTALL).strip()
    return answer, elapsed_ms


def extract_citations(answer):
    return list(dict.fromkeys(int(value) for value in CITATION_RE.findall(answer)))


def evaluate_answer(query, expectation, answer, selected_ids, latency_ms, model):
    citations = extract_citations(answer)
    valid_citations = [value for value in citations if 1 <= value <= len(selected_ids)]
    cited_documents = [selected_ids[value - 1] for value in valid_citations]
    invalid_citations = [value for value in citations if value not in valid_citations]
    relevant = {doc for doc, grade in query["relevance"].items() if grade > 0}
    required_docs = set(expectation.get("required_citation_docs", relevant))
    expected_abstain = bool(expectation.get("expected_abstain", not relevant))
    abstained = bool(ABSTENTION_RE.search(answer))

    groups = expectation.get("required_patterns", [])
    matched_groups = [
        any(re.search(pattern, answer, re.IGNORECASE | re.DOTALL) for pattern in group)
        for group in groups
    ]
    forbidden_hits = [
        pattern
        for pattern in expectation.get("forbidden_patterns", [])
        if re.search(pattern, answer, re.IGNORECASE | re.DOTALL)
    ]
    facts_ok = all(matched_groups) and not forbidden_hits
    if expected_abstain:
        citation_ok = not citations
        answer_correct = abstained and citation_ok
    else:
        citation_ok = (
            bool(valid_citations)
            and not invalid_citations
            and set(cited_documents).issubset(relevant)
            and required_docs.issubset(cited_documents)
        )
        answer_correct = facts_ok and citation_ok and not abstained

    return {
        "query_id": query["id"],
        "answer": answer,
        "ranked_ids": selected_ids,
        "citation_map": {str(i): doc for i, doc in enumerate(selected_ids, 1)},
        "cited_ids": citations,
        "cited_document_ids": cited_documents,
        "invalid_citations": invalid_citations,
        "required_fact_groups_matched": sum(matched_groups),
        "total_required_fact_groups": len(groups),
        "forbidden_pattern_hits": forbidden_hits,
        "facts_correct": facts_ok,
        "citations_correct": citation_ok,
        "abstained": abstained,
        "expected_abstain": expected_abstain,
        "answer_correct": answer_correct,
        "latency_ms": latency_ms,
        "model": model,
    }


def aggregate(rows):
    if not rows:
        return {}
    answerable = [row for row in rows if not row["expected_abstain"]]
    unanswerable = [row for row in rows if row["expected_abstain"]]
    latencies = sorted(row["latency_ms"] for row in rows)
    percentile_index = max(0, (95 * len(latencies) + 99) // 100 - 1)
    total_fact_groups = sum(row["total_required_fact_groups"] for row in rows)
    return {
        "queries": len(rows),
        "answerable_queries": len(answerable),
        "unanswerable_queries": len(unanswerable),
        "answer_accuracy": statistics.fmean(row["answer_correct"] for row in rows),
        "fact_group_accuracy": (
            sum(row["required_fact_groups_matched"] for row in rows) / total_fact_groups
            if total_fact_groups else None
        ),
        "citation_accuracy": (
            statistics.fmean(row["citations_correct"] for row in answerable)
            if answerable else None
        ),
        "abstention_accuracy": (
            statistics.fmean(row["answer_correct"] for row in unanswerable)
            if unanswerable else None
        ),
        "mean_latency_ms": statistics.fmean(latencies),
        "p95_latency_ms": latencies[percentile_index],
        "failed_query_ids": [row["query_id"] for row in rows if not row["answer_correct"]],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dataset", type=Path)
    parser.add_argument("retrieval_run", type=Path)
    parser.add_argument("expectations", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--provider")
    parser.add_argument("--top-k", type=int, default=5)
    parser.add_argument("--max-tokens", type=int, default=512)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument(
        "--delay",
        type=float,
        default=0.5,
        help="seconds to wait between requests to avoid saturating a single llama.cpp slot",
    )
    parser.add_argument("--limit", type=int)
    parser.add_argument("--query", action="append", default=[])
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    if args.top_k < 1 or args.max_tokens < 1 or args.timeout <= 0 or args.delay < 0:
        parser.error("top-k, max-tokens, and timeout must be positive; delay cannot be negative")

    dataset = load_structured(args.dataset)
    expectations_data = load_structured(args.expectations)
    expectations = {item["id"]: item for item in expectations_data["queries"]}
    documents = {item["id"]: item for item in dataset["documents"]}
    queries = {item["id"]: item for item in dataset["queries"]}
    retrieval = {item["query_id"]: item for item in load_jsonl(args.retrieval_run)}
    if set(queries) != set(expectations) or set(queries) != set(retrieval):
        raise ValueError("Dataset, expectations, and retrieval run must contain identical query IDs")

    provider = provider_config(load_structured(find_config(args.config)), args.provider)
    selected_queries = list(queries)
    if args.query:
        unknown = set(args.query) - set(queries)
        if unknown:
            raise ValueError(f"Unknown query IDs: {sorted(unknown)}")
        selected_queries = [query_id for query_id in selected_queries if query_id in args.query]
    if args.limit is not None:
        selected_queries = selected_queries[: args.limit]

    existing = {}
    if args.resume and args.output.exists():
        existing = {row["query_id"]: row for row in load_jsonl(args.output)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    mode = "a" if args.resume else "w"
    rows = [existing[qid] for qid in selected_queries if qid in existing]
    with args.output.open(mode) as output:
        for index, query_id in enumerate(selected_queries, 1):
            if query_id in existing:
                continue
            query = queries[query_id]
            selected_ids, context = render_context(
                retrieval[query_id]["ranked_ids"], documents, args.top_k
            )
            answer, latency_ms = request_answer(
                provider, query["text"], context, args.timeout, args.max_tokens
            )
            row = evaluate_answer(
                query,
                expectations[query_id],
                answer,
                selected_ids,
                latency_ms,
                provider["model"],
            )
            output.write(json.dumps(row, ensure_ascii=False) + "\n")
            output.flush()
            rows.append(row)
            print(
                f"[{index}/{len(selected_queries)}] {query_id}: "
                f"{'PASS' if row['answer_correct'] else 'FAIL'} "
                f"({latency_ms:.0f} ms)",
                flush=True,
            )
            if args.delay:
                time.sleep(args.delay)
    print(json.dumps(aggregate(rows), indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
