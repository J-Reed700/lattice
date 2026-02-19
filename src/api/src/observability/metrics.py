from src.observability.tracing import get_meter

meter = get_meter(__name__)

search_counter = meter.create_counter(
    "vault.search.count", description="Number of search requests", unit="1"
)

index_counter = meter.create_counter(
    "vault.index.count", description="Number of documents indexed", unit="1"
)

error_counter = meter.create_counter("vault.errors.count", description="Number of errors", unit="1")

search_latency = meter.create_histogram(
    "vault.search.latency", description="Search latency in milliseconds", unit="ms"
)

embedding_latency = meter.create_histogram(
    "vault.embedding.latency", description="Embedding generation latency", unit="ms"
)

llm_latency = meter.create_histogram(
    "vault.llm.latency", description="LLM response latency", unit="ms"
)


def record_search(query: str, results: int, latency_ms: float, mode: str = "hybrid"):
    search_counter.add(1, {"mode": mode})
    search_latency.record(latency_ms, {"mode": mode})


def record_index(filename: str, chunks: int):
    index_counter.add(1)


def record_error(error_type: str, endpoint: str):
    error_counter.add(1, {"type": error_type, "endpoint": endpoint})


def record_embedding_latency(latency_ms: float, model: str = "default"):
    embedding_latency.record(latency_ms, {"model": model})


def record_llm_latency(latency_ms: float, model: str = "default"):
    llm_latency.record(latency_ms, {"model": model})
