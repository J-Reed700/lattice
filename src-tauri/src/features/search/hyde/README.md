# HyDE Query Processing

`features::search::hyde` interprets a chat turn before retrieval: it classifies the
query, and for questions asks the utility LLM for a hypothetical answer
(Hypothetical Document Embeddings) that can be searched alongside the raw
query. It also rewrites the turn into a web search query and proposes
deep-research follow-ups.

## Module structure

```
hyde/
├── mod.rs                # Exports QueryClassifier, HyDEGenerator, HyDEService
├── query_classifier.rs   # Regex classifier: Greeting / Question / Command
├── hyde_generator.rs     # LLM prompts: hypothetical document, web query, follow-ups
├── hyde_service.rs       # Orchestrator: classify, then generate
└── README.md
```

The shared types live in `src-tauri/src/domain/qa/hyde.rs`: `QueryType`,
`SearchStrategy` (`HyDEOnly`, `RawOnly`, `Hybrid`), `ToolIntent` and
`HyDEInterpretation`.

## Query classification

`QueryClassifier::classify` is pure and synchronous (patterns via `lazy_regex`):

1. **Greeting** (checked first): the whole query is `hi`, `hello`, `hey`,
   `greetings` or `good morning/afternoon/evening`, optionally followed by
   `!`/`?`.
2. **Command**: starts with `search`, `find`, `show`, `list`, `index`,
   `delete`, `remove`, `open`, `create` or `update`.
3. **Question**: contains a question word or phrase (`what`, `how`, `why`,
   `when`, `where`, `who`, `which`, `can you`, `could you`, `explain`,
   `tell me`).
4. Anything else defaults to **Question**.

All matches are case-insensitive and the query is trimmed first. Greetings take
the fast path with no LLM call.

## Where it runs

The conversation retrieval pipeline
(`features/conversation/chat/retrieval/pipeline.rs`) builds a
`HyDEService` on the utility LLM, not the chat LLM, and calls
`classify_query_with_context`, `generate_web_search_query_with_context` and
`generate_research_followups_with_context` with a window of recent
conversation (and any attached document's digest).

```rust
use lattice::features::search::hyde::HyDEService;

let service = HyDEService::new(utility_llm); // Arc<dyn LLMPort>
let interpretation = service.interpret_query("how does WAL mode work?").await?;
```

## Testing

```bash
cd src-tauri
cargo test --lib features::search::hyde      # classifier unit tests
cargo test --test hyde_phase2_tests      # generator + service with a mock LLMPort
```
