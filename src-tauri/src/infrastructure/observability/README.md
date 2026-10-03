# Observability

`infrastructure::observability` sets up logging and optional OpenTelemetry
(OTEL) trace export for the Lattice desktop backend.

```
observability/
├── tracing.rs   # Subscriber setup: stdout + daily log file, optional OTLP export
├── metrics.rs   # Metrics counters + MetricsSnapshot
├── errors.rs    # track_error helper (structured error log line)
└── mod.rs
```

`infrastructure/setup/observability.rs::setup_tracing()` is the single entry
point, called at startup. It asks `tracing::is_otel_enabled()`; if OTEL is on it
calls `init_otel_tracing("lattice-desktop", None)` and falls back to
`init_regular_tracing()` if that fails. `shutdown_tracing()` flushes the OTEL
provider on exit.

## Logs

Both modes log to stdout and to a daily-rotated file, keeping the newest 7:

- macOS: `~/Library/Application Support/lattice/logs/lattice.log.<date>`
- Linux: `~/.local/share/lattice/logs/lattice.log.<date>`
- Windows: `%LOCALAPPDATA%\lattice\logs\lattice.log.<date>`

(`dirs::data_local_dir()/lattice/logs`, separate from the app data directory.)

`RUST_LOG` sets the filter. When unset, a debug build uses
`info,lattice=debug,lattice_desktop=debug,tokenizers=error` and a release
build `info,tokenizers=error`.

## OpenTelemetry

OTEL export is off unless enabled through the environment:

| Variable | Effect | Default |
|----------|--------|---------|
| `OTEL_ENABLED` | `true` turns export on | off |
| `OTEL_TRACES_EXPORTER` | containing `otlp` also turns export on | unset |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `grpc` or `http/protobuf` | `http/protobuf` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | collector URL; `/v1/traces` is appended for HTTP | `http://localhost:4318` (HTTP), `http://localhost:4317` (gRPC) |

The service name is fixed to `lattice-desktop`, with `service.version` from the
crate version. Spans go through a batch exporter with the SDK's default
sampler. Spans come from `#[tracing::instrument]` on backend functions (search,
conversation CRUD, web fetch and extraction, repository batch writes, app
initialization), so what you see is whatever `tracing` spans exist in the code.

### Local Jaeger

```bash
docker run -d --name jaeger -p 4317:4317 -p 4318:4318 -p 16686:16686 \
  jaegertracing/all-in-one:latest

OTEL_ENABLED=true npm run tauri dev
```

Open http://localhost:16686 and pick the `lattice-desktop` service. Any other
OTLP collector (Tempo, Honeycomb) works by pointing
`OTEL_EXPORTER_OTLP_ENDPOINT` at it; `OTEL_EXPORTER_OTLP_HEADERS` is read by the
exporter for auth headers.

## Metrics and error tracking

`Metrics` holds atomic counters (searches, LLM requests, cache hits/misses,
embeddings, indexing operations and errors) and produces a `MetricsSnapshot`.
`features/metrics/` wraps it in a `get_metrics` command, but that command is not
registered in any plugin, and nothing outside tests calls the `record_*`
methods, so the counters stay at zero. `track_error` likewise has no callers.
Treat both as unused until they are wired in.

## Tips

- Name spans after the operation: `#[tracing::instrument(name = "search_documents", skip(state))]`.
- Put context in fields, not the message: `tracing::info!(results = n, duration_ms, "search done")`.
- If no traces arrive, check the stderr line `[lattice] Initializing OpenTelemetry — protocol: ..., endpoint: ...`
  printed at startup, and that the collector listens on that protocol's port.
