# LLM Engine

`features::llm::engine` holds the clients Lattice uses to run a chat model, and
the hardware and catalog helpers that size them. The default path is the
bundled `llama-server` (llama.cpp) sidecar; Ollama is the alternative local
backend. The remote llama.cpp server adapter lives next door in
`features/llm/llama_cpp/`, and the OpenAI/Anthropic cloud adapters in
`features/llm/cloud.rs` (explicit selection only; `Auto` never sends local
documents to a cloud provider).

## Module structure

```
engine/
├── mod.rs                    # Re-exports + ModelDownloader
├── traits.rs                 # LLMClient trait, GenerationConfig
├── types.rs                  # Shared request/response types, LLMError
├── factory.rs                # LLMConfig, create_llm, create_llm_with_fallback
├── compatibility.rs          # Checks a GGUF architecture against the pinned engine
├── sidecar_manager.rs        # llama-server lifecycle facade
│   └── sidecar_manager/      # Config, preflight, startup, readiness, process,
│                             # registry, failure classification, tests
├── sidecar_pool.rs           # Shares one llama-server per identical config
├── sidecar_client.rs         # SidecarLLMClient: OpenAI-compatible HTTP/SSE client
│   └── sidecar_client/       # Ignored live smoke test
├── ollama_client.rs          # OllamaClient (also implements LLMPort)
├── noop_client.rs            # NoOpLLMClient: fallback when nothing can load
├── circuit_breaker.rs        # Closed/Open/Half-Open breaker used by OllamaClient
├── gguf_metadata.rs          # Reads GGUF headers (context_length, KV sizing)
├── models.rs                 # ModelCatalog, ModelRecommender, PerformanceTier
├── model_catalog_adapter.rs  # HardcodedModelCatalog
├── model_storage_adapter.rs  # Legacy filesystem ModelStorage adapter
└── system/                   # detect_capabilities: platform, GPU, backend devices
```

## How a model gets loaded

1. Settings pick a provider (`LLMProvider`: `auto`, `local`, `ollama`,
   `llamacpp`, `openai`, `anthropic`, in
   `application/contracts/settings.rs`). `application/services/model_selection.rs`
   resolves `auto`/`local`/`ollama`; `features/llm/loading.rs` builds
   the client for each role (chat, router, utility).
2. Local models go through `factory::create_llm(LLMConfig::Local { .. })`.
   `SidecarManager::start_with_fallback` preflights the bundled binary, picks a
   free `127.0.0.1` port, spawns `llama-server` through `tauri-plugin-shell`,
   and waits for `GET /health`. `sidecar_pool` hands an identical request the
   already-running process, so roles pointing at one GGUF share one server; the
   process is killed when its last user drops it.
3. `LLMConfig::Ollama { .. }` returns an `OllamaClient`.
4. Every port a role gets is wrapped in `scheduler::ScheduledLlm`, one
   `InferenceScheduler` per backend: per local llama-server (sized from its
   `/props` slots and `--ctx-size`), per remote llama.cpp endpoint, per Ollama
   endpoint (`RECALL_OLLAMA_MAX_CONCURRENCY`, default 3) and per cloud
   provider (8). It admits by priority, keeps one slot for interactive work,
   pins llama-server requests to a slot by cache key, aborts on cancellation,
   and calibrates the backend's token estimate from reported usage.
5. `create_llm_with_fallback` returns a `NoOpLLMClient` instead of an error, so
   the app still starts without a usable model.

Everything above returns `Arc<dyn LLMPort>` (`application/ports/llm_port.rs`),
which is what features call. `LLMClient` is the engine-internal trait
(`generate`, `generate_stream`).

## Download compatibility check

Before a catalog GGUF download begins, `compatibility.rs` reads at most the
first 4 MiB of the selected file, extracts `general.architecture`, and compares
it with `src-tauri/scripts/llama-architectures.txt`. That manifest is generated
from the exact llama.cpp tag in `llama-server.lock`. This prevents a known
unsupported architecture from consuming the full download; it does not promise
that every quantization fits in memory or runs well on every machine.

## The llama-server binary

`tauri.conf.json` bundles `binaries/llama-server` as an `externalBin`. The
release is pinned by `src-tauri/scripts/llama-server.lock`, fetched by
`src-tauri/scripts/fetch-llama-binaries.sh` and checked by
`src-tauri/scripts/verify_llama_binaries.py`; CI builds come from
`.github/workflows/llama-build.yml`.

## Testing

```bash
cd src-tauri
cargo test --lib features::llm::engine

# Parse a real GGUF on this machine (ignored by default)
LATTICE_GGUF_MODEL=/path/to/model.gguf \
  cargo test --lib a_real_gguf_on_this_machine_parses -- --ignored
```
