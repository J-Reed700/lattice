# OpenTelemetry Observability for Vault Desktop

This module provides OpenTelemetry (OTEL) instrumentation for the Vault Desktop application, enabling distributed tracing and observability.

## Features

- **Distributed Tracing**: Track operations across Rust backend and Python sidecar
- **Performance Monitoring**: Measure latencies for search, indexing, and LLM operations
- **Error Tracking**: Capture and trace errors with context
- **Span Attributes**: Rich metadata on operations (query text, result counts, durations)

## Quick Start

### 1. Start Jaeger (Local Development)

```bash
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

### 2. Enable OpenTelemetry

Set environment variables:

```bash
export OTEL_ENABLED=true
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

Or use `.env.development`:

```env
OTEL_ENABLED=true
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
RUST_LOG=vault_desktop=info
```

### 3. Run the Application

```bash
npm run tauri dev
```

### 4. View Traces

Open Jaeger UI: **http://localhost:16686**

- Select service: `vault-desktop`
- View traces, spans, and performance metrics

## Instrumented Operations

### Rust Backend

#### Search Operations
- `search_documents` - Main search command
  - **Spans**: `search_documents`, `enrich_search_results`
  - **Attributes**: `query`, `limit`, `results_count`, `duration_ms`, `from_cache`

- `search_fast` - Fast vector search
  - **Attributes**: `query`, `limit`, `duration_ms`

- `search_hybrid` - Hybrid BM25 + vector search
  - **Attributes**: `query`, `limit`, `search_mode`, `results_count`

#### Indexing Operations
- `start_indexing` - Index folder
  - **Spans**: `start_indexing`, `extract_content`, `chunk_content`, `generate_embeddings`, `store_document`
  - **Attributes**: `path`, `recursive`, `chunks_count`, `embeddings_count`

- `index_file` - Index single file
  - **Spans**: `index_file`, `extract_content`, `chunk_content`, `generate_embeddings`
  - **Attributes**: `path`, `file_size`, `chunks_count`

#### LLM Operations
- `ask_question` - Q&A with RAG
  - **Spans**: `ask_question`, `context_retrieval`, `answer_generation`
  - **Attributes**: `question`, `max_results`, `sources_count`, `answer_length`, `duration_ms`

- `generate_embedding` - Generate text embeddings
  - **Spans**: `generate_embedding`, `python_bridge_call`
  - **Attributes**: `text_length`, `embedding_dim`, `duration_ms`

- `semantic_search` - Semantic search via Python
  - **Attributes**: `query`, `limit`, `results_count`

### Metrics Tracked

- **Search Latency**: Time from query to results
- **Indexing Throughput**: Files/chunks per second
- **LLM Response Time**: Question to answer duration
- **Cache Hit Rate**: Search cache effectiveness
- **Error Rates**: Failed operations by type

## Span Structure

### Example: Search Operation

```
search_documents (250ms)
├── rate_limit_check (1ms)
├── input_validation (2ms)
├── cache_lookup (5ms)
├── embed_query (45ms)
├── vector_search (120ms)
├── enrich_search_results (75ms)
│   ├── batch_1 (25ms)
│   ├── batch_2 (25ms)
│   └── batch_3 (25ms)
└── cache_put (2ms)
```

### Example: Q&A Operation

```
ask_question (2500ms)
├── rate_limit_check (1ms)
├── input_validation (2ms)
├── context_retrieval (200ms)
│   └── search_documents (180ms)
└── answer_generation (2300ms)
    ├── prompt_build (10ms)
    ├── llm_call (2250ms)
    └── response_parse (40ms)
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OTEL_ENABLED` | Enable OpenTelemetry | `false` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP endpoint URL | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | Service name in traces | `vault-desktop` |
| `RUST_LOG` | Logging level | `info` |

### Sampling

Currently using `AlwaysOn` sampler for development. For production:

```rust
.with_sampler(Sampler::TraceIdRatioBased(0.1)) // 10% sampling
```

## Integrations

### Jaeger (Recommended for Development)

```bash
docker run -d \
  --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

**UI**: http://localhost:16686

### Grafana Tempo

```yaml
# tempo.yaml
server:
  http_listen_port: 3200

distributor:
  receivers:
    otlp:
      protocols:
        grpc:
          endpoint: 0.0.0.0:4317
```

Run:
```bash
docker run -d \
  --name tempo \
  -p 4317:4317 \
  -p 3200:3200 \
  grafana/tempo:latest \
  -config.file=/etc/tempo.yaml
```

### Honeycomb / Lightstep

Set endpoint to your service:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io
export OTEL_EXPORTER_OTLP_HEADERS="x-honeycomb-team=YOUR_API_KEY"
```

## Best Practices

### 1. Use Descriptive Span Names

```rust
#[tracing::instrument(name = "search_documents", skip(state))]
```

### 2. Add Contextual Attributes

```rust
tracing::info!(
    query = %query,
    results_count = results.len(),
    duration_ms = elapsed.as_millis(),
    "Search completed"
);
```

### 3. Create Child Spans for Sub-Operations

```rust
let span = span!(Level::INFO, "extract_content");
let _enter = span.enter();
// ... extraction logic
drop(_enter);
```

### 4. Record Errors

```rust
.map_err(|e| {
    tracing::error!(error = %e, "Operation failed");
    format!("Failed: {}", e)
})?
```

## Troubleshooting

### Traces Not Appearing

1. **Check OTEL is enabled**:
   ```bash
   echo $OTEL_ENABLED
   # Should output: true
   ```

2. **Verify Jaeger is running**:
   ```bash
   curl http://localhost:16686
   ```

3. **Check application logs**:
   ```bash
   grep "OpenTelemetry" logs/*
   ```

### Connection Refused

- Ensure `OTEL_EXPORTER_OTLP_ENDPOINT` is correct
- Check firewall allows port 4317
- Verify collector is listening on gRPC port

### High Overhead

- Reduce sampling rate: `Sampler::TraceIdRatioBased(0.1)`
- Disable OTEL: `export OTEL_ENABLED=false`
- Lower log level: `RUST_LOG=vault_desktop=warn`

## Performance Impact

- **Overhead**: ~5-10ms per traced operation
- **Memory**: ~50MB additional with AlwaysOn sampling
- **Network**: ~1KB per span sent to collector

**Recommendation**: Use sampling in production (10-20% trace rate)

## Future Enhancements

- [ ] Frontend instrumentation (React components)
- [ ] Metrics export (Prometheus)
- [ ] Log correlation with trace IDs
- [ ] Custom dashboards (Grafana)
- [ ] Automatic anomaly detection
- [ ] Python sidecar tracing integration

## References

- [OpenTelemetry Rust](https://docs.rs/opentelemetry/)
- [Tracing Crate](https://docs.rs/tracing/)
- [Jaeger Docs](https://www.jaegertracing.io/docs/)
- [OTLP Specification](https://opentelemetry.io/docs/reference/specification/protocol/)
