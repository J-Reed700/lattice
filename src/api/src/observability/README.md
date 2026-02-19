# OpenTelemetry Observability

Comprehensive observability for the Vault Backend FastAPI server using OpenTelemetry (OTEL).

## Overview

This implementation provides:
- **Distributed Tracing**: End-to-end request tracing with Jaeger
- **Custom Metrics**: Search latency, indexing counts, error rates
- **Auto-instrumentation**: FastAPI, SQLAlchemy, PostgreSQL, HTTP requests
- **Manual Spans**: Search operations, document indexing, embeddings, LLM calls

## Quick Start

### 1. Start Jaeger

```bash
docker-compose -f docker-compose.otel.yml up -d
```

Verify Jaeger is running:
```bash
curl http://localhost:16686
```

### 2. Enable OpenTelemetry

Update `.env`:
```bash
OTEL_ENABLED=true
OTEL_SERVICE_NAME=vault-backend
OTEL_SERVICE_VERSION=1.0.0
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_TRACES_EXPORTER=otlp
OTEL_METRICS_EXPORTER=otlp
ENVIRONMENT=development
```

### 3. Install Dependencies

```bash
pip install -r requirements.txt
```

### 4. Start Backend

```bash
python -m uvicorn src.api.app:app --reload
```

### 5. View Traces

Open Jaeger UI: http://localhost:16686

- Select service: `vault-backend`
- Click "Find Traces"
- Explore distributed traces

## Architecture

### Components

```
┌─────────────────────────────────────────────────┐
│           Vault Backend (FastAPI)               │
│                                                 │
│  ┌──────────────────────────────────────────┐  │
│  │   OpenTelemetry SDK                      │  │
│  │   - Tracer Provider                      │  │
│  │   - Meter Provider                       │  │
│  │   - Resource (service name, version)     │  │
│  └──────────────────────────────────────────┘  │
│                     │                           │
│                     ▼                           │
│  ┌──────────────────────────────────────────┐  │
│  │   Auto-Instrumentation                   │  │
│  │   - FastAPI (HTTP endpoints)             │  │
│  │   - SQLAlchemy (database queries)        │  │
│  │   - Psycopg2 (PostgreSQL driver)         │  │
│  │   - Requests (HTTP client)               │  │
│  └──────────────────────────────────────────┘  │
│                     │                           │
│                     ▼                           │
│  ┌──────────────────────────────────────────┐  │
│  │   Manual Instrumentation                 │  │
│  │   - Search spans (vector/bm25/hybrid)    │  │
│  │   - Indexing spans                       │  │
│  │   - Embedding generation spans           │  │
│  │   - LLM call spans                       │  │
│  └──────────────────────────────────────────┘  │
│                     │                           │
└─────────────────────┼───────────────────────────┘
                      │
                      ▼ OTLP (gRPC/HTTP)
        ┌─────────────────────────────┐
        │        Jaeger Backend       │
        │  - Collector (4317/4318)    │
        │  - Query Service            │
        │  - Storage (memory/DB)      │
        └─────────────────────────────┘
                      │
                      ▼
        ┌─────────────────────────────┐
        │        Jaeger UI            │
        │    http://localhost:16686   │
        └─────────────────────────────┘
```

## Instrumented Operations

### Automatic Instrumentation

The following are automatically traced:

#### 1. **All HTTP Endpoints** (FastAPI)
- Request method, path, headers
- Response status code
- Duration
- Client IP, user agent

#### 2. **Database Queries** (SQLAlchemy + Psycopg2)
- SQL statements
- Query parameters
- Execution time
- Connection pool usage

#### 3. **HTTP Client Requests** (Requests library)
- External API calls
- Request/response details

### Manual Instrumentation

#### Search Endpoints (`/api/v1/search`)

**Spans:**
- `api_search` - Main search operation
  - `bm25_search` - BM25 keyword search
  - `hybrid_search` - Hybrid vector + BM25
  - `vector_search` - Pure vector search

**Attributes:**
- `search.query` - Search query text
- `search.limit` - Result limit
- `search.mode` - Search mode (vector/bm25/hybrid)
- `search.offset` - Pagination offset
- `results.count` - Number of results
- `results.total` - Total matching documents
- `fusion.strategy` - Hybrid fusion strategy
- `success` - Operation success (true/false)

**Example Trace:**
```
api_search (45ms)
  ├─ hybrid_search (35ms)
  │   ├─ vector_search (15ms)
  │   │   ├─ generate_embedding (8ms)
  │   │   └─ query_vector_db (5ms)
  │   └─ bm25_search (18ms)
  │       └─ query_fts5_index (12ms)
  └─ format_results (8ms)
```

#### Indexing Endpoints (`/api/v1/index`)

**Spans:**
- `api_trigger_indexing` - Start indexing operation

**Attributes:**
- `watch_folder_id` - Folder being indexed
- `force` - Force re-indexing
- `recursive` - Recursive indexing
- `success` - Operation success

## Custom Metrics

### Counters

#### `vault.search.count`
Number of search requests.

**Labels:**
- `mode` - Search mode (vector/bm25/hybrid)

**Usage:**
```python
from src.observability.metrics import record_search

record_search(query="test", results=10, latency_ms=45.3, mode="hybrid")
```

#### `vault.index.count`
Number of documents indexed.

**Usage:**
```python
from src.observability.metrics import record_index

record_index(filename="document.pdf", chunks=5)
```

#### `vault.errors.count`
Number of errors by type and endpoint.

**Labels:**
- `type` - Error type (ValueError, TimeoutError, etc.)
- `endpoint` - API endpoint path

**Usage:**
```python
from src.observability.metrics import record_error

record_error(error_type="ValueError", endpoint="/search")
```

### Histograms

#### `vault.search.latency`
Search operation latency in milliseconds.

**Labels:**
- `mode` - Search mode

#### `vault.embedding.latency`
Embedding generation latency.

**Labels:**
- `model` - Embedding model name

**Usage:**
```python
from src.observability.metrics import record_embedding_latency

record_embedding_latency(latency_ms=123.4, model="bge-m3")
```

#### `vault.llm.latency`
LLM response latency.

**Labels:**
- `model` - LLM model name

**Usage:**
```python
from src.observability.metrics import record_llm_latency

record_llm_latency(latency_ms=456.7, model="gpt-4")
```

## Adding Custom Instrumentation

### Example: Instrument a New Endpoint

```python
from fastapi import APIRouter
from src.observability.tracing import get_tracer
from src.observability.metrics import record_error
from opentelemetry import trace

router = APIRouter()
tracer = get_tracer(__name__)

@router.post("/custom")
async def custom_endpoint(data: dict):
    with tracer.start_as_current_span("api_custom") as span:
        span.set_attribute("data.size", len(data))

        try:
            # Your logic here
            with tracer.start_as_current_span("process_data"):
                result = process_data(data)

            span.set_attribute("success", True)
            return result

        except Exception as e:
            span.record_exception(e)
            span.set_status(trace.Status(trace.StatusCode.ERROR, str(e)))
            record_error(type(e).__name__, "/custom")
            raise
```

### Example: Add Custom Metrics

```python
from src.observability.tracing import get_meter

meter = get_meter(__name__)

# Create counter
custom_counter = meter.create_counter(
    "vault.custom.count",
    description="Custom operation count",
    unit="1"
)

# Create histogram
custom_latency = meter.create_histogram(
    "vault.custom.latency",
    description="Custom operation latency",
    unit="ms"
)

# Use metrics
custom_counter.add(1, {"operation": "test"})
custom_latency.record(123.45, {"operation": "test"})
```

## Configuration Options

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_ENABLED` | `false` | Enable/disable OpenTelemetry |
| `OTEL_SERVICE_NAME` | `vault-backend` | Service name in traces |
| `OTEL_SERVICE_VERSION` | `1.0.0` | Service version |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTLP collector endpoint |
| `OTEL_TRACES_EXPORTER` | `otlp` | Trace exporter type |
| `OTEL_METRICS_EXPORTER` | `otlp` | Metrics exporter type |
| `ENVIRONMENT` | `development` | Deployment environment |

### Jaeger Configuration

Jaeger ports:
- **4317**: OTLP gRPC receiver (default for SDK)
- **4318**: OTLP HTTP receiver
- **16686**: Jaeger UI (web interface)
- **14268**: Jaeger collector HTTP
- **14250**: Jaeger collector gRPC

## Troubleshooting

### Traces Not Appearing

1. **Check Jaeger is running:**
   ```bash
   docker ps | grep jaeger
   ```

2. **Verify OTEL is enabled:**
   ```bash
   grep OTEL_ENABLED .env
   ```

3. **Check logs for errors:**
   ```bash
   # Look for "OpenTelemetry initialized" message
   tail -f logs/vault.log | grep -i otel
   ```

4. **Test OTLP endpoint:**
   ```bash
   curl http://localhost:4317
   # Should connect (even if returns error, connection works)
   ```

### Metrics Not Recording

1. **Verify metric reader interval:**
   Default is 30 seconds. Wait at least 30s after operation.

2. **Check span recording:**
   ```python
   span = trace.get_current_span()
   print(f"Is recording: {span.is_recording()}")
   ```

3. **Validate OTLP connection:**
   Check Docker logs:
   ```bash
   docker logs jaeger-vault
   ```

### Performance Impact

OpenTelemetry has minimal performance impact:
- **Overhead**: ~1-5% CPU, <50MB RAM
- **Latency**: <1ms per span
- **Batch processing**: Spans exported in batches (default 512)

To reduce overhead:
- Use sampling (not implemented yet)
- Increase export interval
- Disable in production if needed

## Production Deployment

### Recommended Setup

1. **Use external Jaeger/collector:**
   ```bash
   OTEL_EXPORTER_OTLP_ENDPOINT=https://jaeger.production.com:4317
   ```

2. **Enable TLS:**
   ```bash
   OTEL_EXPORTER_OTLP_PROTOCOL=grpc
   OTEL_EXPORTER_OTLP_CERTIFICATE=/path/to/cert.pem
   ```

3. **Add sampling:**
   Update `tracing.py` to add sampler:
   ```python
   from opentelemetry.sdk.trace.sampling import TraceIdRatioBased

   tracer_provider = TracerProvider(
       resource=resource,
       sampler=TraceIdRatioBased(0.1)  # Sample 10% of traces
   )
   ```

4. **Use persistent storage:**
   Configure Jaeger with Elasticsearch/Cassandra backend

### Security Considerations

- **Never expose Jaeger UI publicly** without authentication
- Use **TLS for OTLP connections** in production
- **Sanitize sensitive data** in span attributes (passwords, tokens, PII)
- Consider using **environment-specific service names** (e.g., `vault-backend-prod`)

## Integration with Other Tools

### Prometheus

Export metrics to Prometheus instead of OTLP:

```python
from opentelemetry.exporter.prometheus import PrometheusMetricReader
from prometheus_client import start_http_server

# Start Prometheus metrics server
start_http_server(port=9090)

# Use Prometheus reader instead of OTLP
metric_reader = PrometheusMetricReader()
meter_provider = MeterProvider(resource=resource, metric_readers=[metric_reader])
```

### Grafana

1. Add Jaeger as data source in Grafana
2. Import Jaeger dashboard
3. Create custom dashboards for metrics

### Sentry

Combine with Sentry for error tracking:
```python
import sentry_sdk
from sentry_sdk.integrations.opentelemetry import SentrySpanProcessor

sentry_sdk.init(dsn="your-dsn")

# Add Sentry processor to trace provider
tracer_provider.add_span_processor(SentrySpanProcessor())
```

## Examples

### Viewing a Search Trace

1. Make a search request:
   ```bash
   curl -X POST http://localhost:8000/api/v1/search \
     -H "Content-Type: application/json" \
     -d '{"query": "machine learning", "mode": "hybrid", "limit": 10}'
   ```

2. Open Jaeger UI: http://localhost:16686
3. Select service: `vault-backend`
4. Click "Find Traces"
5. Click on a trace to see detailed timeline:
   - HTTP request span
   - Database query spans
   - Search operation spans
   - Timing information

### Analyzing Performance

Find slow searches:
1. In Jaeger UI, set "Min Duration" filter
2. Look for traces >500ms
3. Expand spans to find bottlenecks
4. Common slow operations:
   - Embedding generation
   - Vector similarity search
   - Large result set processing

## References

- [OpenTelemetry Python SDK](https://opentelemetry.io/docs/instrumentation/python/)
- [FastAPI Instrumentation](https://opentelemetry-python-contrib.readthedocs.io/en/latest/instrumentation/fastapi/fastapi.html)
- [Jaeger Documentation](https://www.jaegertracing.io/docs/)
- [OTLP Specification](https://opentelemetry.io/docs/reference/specification/protocol/otlp/)
