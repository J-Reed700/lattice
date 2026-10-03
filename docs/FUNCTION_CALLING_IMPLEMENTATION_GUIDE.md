# Function Calling Implementation Guide

**Last Updated**: 2026-10-02. This replaces the 2025-11 plan, which described a
Python/FastAPI service (removed 2026-09-16) and one Tauri command per tool
(never built). The tool contracts themselves are in
[FUNCTION_CALLING_API_CONTRACTS.md](./FUNCTION_CALLING_API_CONTRACTS.md).

## How it fits together

```
chat turn (features/conversation/chat.rs)
  build_llm_tool_definitions  ── registry.list_tools() filtered for this turn
  + history tools, Explorer tools (turn-scoped, not in the registry)
        │
        ▼
LLM (llama-server sidecar / Ollama / cloud) returns tool calls
        │
        ▼
tool loop (chat/tool_loop.rs)
  ├─ history / Explorer tools  → their own modules
  ├─ library tools             → scoped_document_tools (space + focus scope)
  └─ everything else           → FunctionExecutor::execute
                                   ├─ schema validation (jsonschema)
                                   └─ handler in executor/*_tools.rs
```

The same `FunctionExecutor` backs the `functions` Tauri plugin
(`list_available_functions`, `execute_function`, `get_function_stats`), which
the frontend reaches through `src/lib/api.ts`. Both the registry and the
executor are built in `src-tauri/src/interfaces/di/container.rs`.

## Adding a built-in tool

1. **Schema**: register a `ToolDefinition` in `init_function_registry`
   (`src-tauri/src/features/function_calling/registry.rs`). Put every bound in
   the JSON schema (`minimum`, `maximum`, `enum`, `required`): the executor
   validates arguments against it before your handler runs.
2. **DTOs**: add `<Tool>Input` / `<Tool>Output` to
   `features/function_calling/dto.rs`, with `#[serde(default = ...)]` matching
   the schema defaults.
3. **Handler**: add a `TOOL_<NAME>` constant and a match arm in
   `FunctionExecutor::execute` (`features/function_calling/executor.rs`), and
   the handler in the matching `executor/*_tools.rs`. Handlers return
   `Box::pin(async move { ... })` futures so the dispatch match stays small,
   deserialize `args` into the input DTO, and finish with
   `Ok(FunctionResult::success(serde_json::to_value(output)?))`. Return `Err`
   for failures; the executor turns it into `EXECUTION_ERROR`. If the tool needs
   a new service, add it to `FunctionExecutor::new` and the container.
4. **Scope decision**: if the tool reads the user's documents, add it to
   `reads_the_vault` in
   `features/conversation/chat/tool_loop/scoped_document_tools.rs` and give it a
   scoped path there. Either way, update the expected list in that file's
   `every_vault_tool_is_scoped` test, which fails until you do.
5. **Exposure**: decide when a turn offers it. `CORE_TOOL_NAMES` in
   `features/conversation/chat.rs` are always offered;
   `OPTIONAL_BUILTIN_TOOL_NAMES` need the user's per-turn allowlist. A
   registered tool in neither list is never offered to the model.
6. **Timeline**: give it a step kind in `tool_step_kind` and a progress label
   in `tool_activity_label` (`chat/tool_loop.rs`); otherwise it shows as a
   generic "tool" step.
7. **Docs**: add its contract to `FUNCTION_CALLING_API_CONTRACTS.md`.

If the tool needs the conversation in hand (its scope comes from the turn, not
the arguments), do not put it in the registry. Follow
`chat/history_tools.rs` or `features/explorer/tools.rs`: build its
`ToolDefinition`s per turn, add them in `chat.rs` next to `tools_for_turn` /
`explorer::tools::add_to_turn`, and route its calls in the tool loop before
they reach the executor.

## Custom query tools

User-defined HTTP tools need no code: they are read from `llm.customTools` in
settings at startup, registered by `register_custom_query_tools`, and executed
by `executor/custom_tools.rs`. See the contracts page for the request shape.

## Adding or changing an IPC command

Only needed if the frontend must call something new. A Tauri command has five
integration points that must agree:

1. the `#[tauri::command]` + `#[specta::specta]` function (for this feature,
   `features/function_calling/plugin.rs`, delegating to `commands.rs`);
2. the plugin's `tauri::generate_handler!` list in the same file;
3. the command list in `src-tauri/build.rs`;
4. the permission in `src-tauri/capabilities/main.json`
   (`functions:allow-<command-name>`);
5. the command list in `src-tauri/src/export_bindings.rs`, then
   `npm run bindings:generate` to refresh `src/lib/bindings.ts`.

`python3 scripts/check-tauri-command-inventory.py` checks that points 2-4
agree (a miss in 3 or 4 compiles fine and fails only at runtime);
`npm run bindings:check` and `npm run contracts:check` cover the bindings side.
All run in CI.

## Testing

```bash
cd src-tauri
cargo test --lib features::function_calling      # registry, executor, DTOs
cargo test --lib every_vault_tool_is_scoped      # scope classification tripwire
cargo test --lib features::conversation::chat    # tool selection and the tool loop
```

Executor tests live in `features/function_calling/executor/tests.rs` and use
the mocks in `features/function_calling/mocks.rs` with an in-memory SQLite pool
(`sqlx::migrate!("./migrations")`). Test a new tool for: schema rejection of
out-of-range arguments (`INVALID_ARGUMENTS`), a successful result's shape, and
a handler failure surfacing as `EXECUTION_ERROR`.
