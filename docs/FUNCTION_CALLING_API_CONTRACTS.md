# Function Calling API Contracts

**Last Updated**: 2026-10-02 (rewritten against the code; the 2025-11 design,
its Python backend, per-tool rate limits and audit events were never built or
have been removed)

This document defines the tools Lattice offers an LLM during a chat turn, and
the IPC commands that expose the same tools to the frontend. The JSON schemas
in `src-tauri/src/features/function_calling/registry.rs` are the source of
truth; this page summarizes them.

## Table of Contents

1. [Where it lives](#where-it-lives)
2. [Wire types and execution](#wire-types-and-execution)
3. [Library tools](#library-tools): `semantic_search`, `get_document`, `list_documents`, `list_attachments`
4. [Web tools](#web-tools): `web_search`, `fetch_url_content`, `wiki_search`, `wiki_summary`
5. [Custom query tools](#custom-query-tools)
6. [Turn-scoped tools](#turn-scoped-tools)
7. [Which tools a turn gets](#which-tools-a-turn-gets)
8. [Error handling](#error-handling)
9. [Security](#security)

---

## Where it lives

| Piece | Path |
|-------|------|
| Tool schemas, `init_function_registry`, `register_custom_query_tools` | `src-tauri/src/features/function_calling/registry.rs` |
| Wire types (`ToolDefinition`, `FunctionCall`, `FunctionResult`, `RegistryStats`) | `features/function_calling/domain.rs` |
| Tool input/output DTOs | `features/function_calling/dto.rs` |
| `FunctionExecutor` (validation + dispatch) | `features/function_calling/executor.rs`, handlers in `executor/{search,document,web,custom}_tools.rs` |
| Tauri plugin `functions` | `features/function_calling/plugin.rs` |
| Per-turn tool selection and the tool loop | `features/conversation/chat.rs` (`build_llm_tool_definitions`), `features/conversation/chat/tool_loop.rs` |
| Space/focus scoping for library tools | `features/conversation/chat/tool_loop/scoped_document_tools.rs` |

The registry and executor are built once in the DI container
(`src-tauri/src/interfaces/di/container.rs`).

### IPC commands (plugin `functions`)

| Command | Returns |
|---------|---------|
| `list_available_functions` | `ToolDefinition[]` (built-ins plus configured custom tools) |
| `execute_function(call: FunctionCall)` | `FunctionResult` |
| `get_function_stats` | `RegistryStats` |

All three return `Result<_, ApiError>`. Frontend wrappers are in `src/lib/api.ts`
(`executeFunction`, `listAvailableFunctions`, `getFunctionStats`).
`RegistryStats.call_counts` is currently always empty: nothing outside tests
calls `FunctionRegistry::record_call`.

---

## Wire types and execution

```typescript
ToolDefinition { name: string; description: string; input_schema: JSONSchema }
FunctionCall   { id: string; name: string; arguments: JSON }
FunctionResult { success: boolean; data?: JSON; error_code?: string; error_message?: string }
```

Tool names must be non-empty and use only alphanumerics and `_`; the schema
must be a JSON object (`ToolDefinition::new`). When a turn hands tools to a
model, `input_schema` becomes the provider's `parameters`
(`application::ports::ToolDefinition`).

`FunctionExecutor::execute`:

1. Rejects an unknown name with `FUNCTION_NOT_FOUND`.
2. Normalizes `date_from`/`date_to` for `semantic_search` and `list_documents`
   (date-only and naive datetimes become UTC RFC 3339; `date_to` dates become
   end of day).
3. Validates arguments against the tool's JSON schema (`jsonschema`), so the
   `minimum`/`maximum`/`enum` constraints below are enforced:
   `INVALID_ARGUMENTS` on failure.
4. Dispatches to the handler; a handler error becomes `EXECUTION_ERROR` with
   the error text. Execution never returns `Err` for a tool failure.

---

## Library tools

In a chat turn these four run through `scoped_document_tools`, which confines
them to the conversation's space plus its attachments and any document focus,
and refuses a `get_document` outside that set. Called through
`execute_function` they run unscoped against the whole library.

### `semantic_search`

Search the library with semantic, keyword (BM25) or hybrid ranking.

| Param | Type | Default | Constraint |
|-------|------|---------|------------|
| `query` | string | required | |
| `limit` | integer | 10 | 1-50 |
| `threshold` | number | 0.3 | 0.0-1.0 |
| `search_mode` | `semantic` \| `keyword` \| `hybrid` | `hybrid` | |
| `file_types` | string[] | | extensions, e.g. `["pdf","md"]` |
| `date_from`, `date_to` | date-time | | modified-at bounds |

Response (`SemanticSearchOutput`):

```typescript
{
  results: [{ document_id, filename, file_path, mime_type, score, snippet,
              chunk_index?, modified_at, size_bytes }],
  documents: [{ document_id, filename, file_path, mime_type, max_score,
                match_count, matches: [{ chunk_index?, score, excerpt }] }],
  total_found: number,
  search_time_ms: number,
  query: string
}
```

`results` is per chunk; `documents` groups the same hits by document. Scores
rank within the chosen mode and are not probabilities. Through the unscoped
executor path `threshold` applies only in `semantic` mode, and `file_types`,
`date_from` and `date_to` are accepted but not applied; the scoped chat path
applies all of them.

### `get_document`

Read a document's indexed text, paged.

| Param | Type | Default | Constraint |
|-------|------|---------|------------|
| `document_id` | string | required | |
| `include_metadata` | boolean | true | |
| `max_content_length` | integer | 50000 | 1000-100000 characters per page |
| `page` | integer | 1 | >= 1 |

Response (`GetDocumentOutput`): `document_id`, `content`, `content_truncated`,
`page`, `total_pages`, `total_chars`, `has_previous_page`, `has_next_page`,
`previous_page?`, `next_page?`, and `metadata?` (`filename`, `file_path`,
`mime_type`, `extension`, `size_bytes`, `created_at?`, `modified_at`,
`indexed_at`, `tags`, `chunk_count`). Text is reassembled from the indexed
chunks in order (raw file read only as a fallback for plain text), so pages are
internal text pages, not PDF pages. A missing id is an `EXECUTION_ERROR`
("Document '...' not found").

### `list_documents`

| Param | Type | Default | Constraint |
|-------|------|---------|------------|
| `filter_mode` | `all` \| `recent` \| `favorites` \| `by_tag` \| `by_type` | `all` | |
| `file_types` | string[] | | used by `by_type` |
| `tags` | string[] | | used by `by_tag` |
| `date_from`, `date_to` | date-time | | |
| `limit` | integer | 50 | 1-500 |
| `offset` | integer | 0 | >= 0 |
| `sort_by` | `modified` \| `created` \| `name` \| `size` | `modified` | |
| `sort_order` | `asc` \| `desc` | `desc` | |

Response (`ListDocumentsOutput`): `documents[]` (`document_id`, `filename`,
`file_path`, `mime_type`, `extension`, `size_bytes`, `modified_at`, `tags`,
`is_favorite`, `access_count`), `total`, `limit`, `offset`, `has_more`.

### `list_attachments`

No parameters. Lists files attached to the current conversation, which are not
in the library and do not appear in `list_documents`. Response
(`ListAttachmentsOutput`): `attachments[]` (`document_id`, `filename`,
`extension`, `size_bytes`, `word_count`, `indexed_at`), `total`. It only works
inside a chat turn (the scoped path); `FunctionExecutor` has no handler for it
and answers `NO_HANDLER`. Note that `build_llm_tool_definitions` does not
currently include it in a turn's tool list (it is not in `CORE_TOOL_NAMES`).

---

## Web tools

These send the query or URL off the machine. They are offered only when the
turn allows it (see [Which tools a turn gets](#which-tools-a-turn-gets)).

### `web_search`

| Param | Type | Default | Constraint |
|-------|------|---------|------------|
| `query` | string | required | |
| `max_results` | integer | 5 | 1-50 per page |
| `page` | integer | 1 | >= 1 |
| `offset` | integer | 0 | >= 0 |
| `providers` | (`duckduckgo` \| `bing` \| `wikipedia`)[] | DuckDuckGo then Bing | |
| `include_wikipedia` | boolean | false | |
| `depth` | integer | 1 | 1-4 (recursive deep research) |
| `branch_queries` | integer | 2 | 1-4 follow-up branches per depth step |
| `followup_queries` | string[] | | extra angles to run when `depth > 1` |

Response (`WebSearchOutput`): `results[]` (`title`, `url`, `snippet`,
`published_date?`, `source?`), `query`, `result_count`, `page`, `offset`,
`total_results`, `has_more`, `providers_used`, `unique_query_count`,
`unique_url_count`, `unique_domain_count`, `followup_queries` (the queries
actually run after the first). No API keys are involved; results are scraped
from the providers' HTML pages and cached in memory for 15 minutes.

### `fetch_url_content`

| Param | Type | Default |
|-------|------|---------|
| `url` | string | required |

Response (`FetchUrlContentOutput`): `url`, `title?`, `content`,
`content_truncated`, `word_count`, `fetch_time_ms`, `content_type?`,
`from_cache`. Readable text is extracted from the page and capped at 50,000
characters (5 MB body, 30 s body timeout, 10 redirects). Pages are cached on
disk under the app data `web-cache/` directory.

### `wiki_search` and `wiki_summary`

`wiki_search { query, max_results = 5 (1-10) }` returns `query`,
`result_count`, `results[]` (`title`, `url`, `snippet`).
`wiki_summary { title }` returns `title`, `url`, `extract`, `language` for the
page's lead section. Both call the Wikipedia API.

---

## Custom query tools

Users can define HTTP lookup tools in settings (`llm.customTools`,
`CustomToolSettingsDto` in `src-tauri/src/application/contracts/settings.rs`):
`enabled`, `name`, `description`, `endpoint`, `query_param`,
`max_results_param?`, `default_max_results`. Each enabled tool is registered
with the schema `{ query: string (required), max_results: integer 1-100 }`.
Execution sends `GET <endpoint>?<query_param>=<query>[&<max_results_param>=n]`,
passes the URL through the same public-address check as `fetch_url_content`,
and returns `CustomQueryToolOutput { tool_name, request_url, data }` with the
endpoint's JSON as `data`.

---

## Turn-scoped tools

Some tools are deliberately not in the process-wide registry, because they need
the conversation in hand. The tool loop adds them per turn:

- **Conversation history** (`features/conversation/chat/history_tools.rs`):
  `search_conversation_history`, `read_conversation_history`,
  `search_saved_knowledge`. Read-only; offered even on closed-book turns.
- **Explorer** (`features/explorer/tools.rs`): `list_directory`, `read_file`,
  `search_files`, `find_files`, plus `search_folder`
  (`features/explorer/index/tool.rs`) when the folder has an index. Read-only,
  confined to the folder the conversation is bound to, and offered only on
  those conversations' turns (not closed-book ones).

`execute_function` cannot reach these.

---

## Which tools a turn gets

`build_llm_tool_definitions` (`features/conversation/chat.rs`):

- Always: `semantic_search`, `get_document`, `list_documents` (`CORE_TOOL_NAMES`).
- Optional built-ins (`web_search`, `fetch_url_content`, `wiki_search`,
  `wiki_summary`): only when the user's per-turn tool allowlist includes them;
  `fetch_url_content` is also offered when the turn already carries web
  sources.
- Enabled custom tools, subject to the same allowlist.

When retrieval already grounded the turn, the model gets only
`semantic_search`, `get_document` and (with web context) `fetch_url_content`,
unless the user forced a tool turn. Closed-book turns get no external tools.
Models that do not support tool calling get none.

---

## Error handling

Tool failures come back inside a successful `FunctionResult`:

```json
{ "success": false, "error_code": "INVALID_ARGUMENTS", "error_message": "..." }
```

| `error_code` | Meaning |
|--------------|---------|
| `FUNCTION_NOT_FOUND` | Name not registered and not a configured custom tool |
| `INVALID_ARGUMENTS` | Arguments failed the tool's JSON schema |
| `NO_HANDLER` | Registered but the executor has no handler (e.g. `list_attachments` outside a chat turn) |
| `EXECUTION_ERROR` | Handler failed; `error_message` carries the `AppError` text (not found, network, invalid URL, ...) |

`execute_function` itself returns an `ApiError` only for failures outside the
tool, such as a function name that fails input validation.

---

## Security

- **Vault scoping**: in chat, library tools go through `scoped_document_tools`.
  Its test `every_vault_tool_is_scoped` fails when a tool is added to the
  registry without being classified as vault-reading or not.
- **Private-address guard (CWE-918)**: `fetch_url_content` and custom tools
  refuse non-http(s) URLs and hosts that resolve to loopback, private or
  link-local addresses, at the first request, on every redirect, and in the
  client's resolver (`features/web/services/web.rs`).
- **Size limits**: page bodies, search bodies and Wikipedia responses have byte
  caps, and `get_document` pages its output.
- **Not implemented**: there are no per-tool rate limits and no tool-specific
  audit events. `execute_function` logs success or failure through `tracing`.
