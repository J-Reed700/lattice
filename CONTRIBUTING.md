# Contributing

Thanks for helping improve Lattice. Keep changes focused, include tests for
behavioral changes, and avoid committing local reports or work-session notes.

## Setup

Install Node.js 20.19+ or 22.12+ (CI uses 24), a current Rust toolchain, and
the platform prerequisites listed by Tauri. Then run:

```bash
npm ci
npm run tauri:dev
```

## Code organization

Keep the repository root for package manifests, tool configuration, and project
entry points. Put implementation and operational helpers in their owning folders:

| Location | Responsibility |
| --- | --- |
| `src/features/<feature>/` | React feature views, hooks, models, and API clients |
| `src/shared/` | Shared IPC transport and cross-feature primitives |
| `src/components/` | Shared UI and remaining feature views during migration |
| `src-tauri/src/features/<feature>/` | Desktop capabilities, including their commands, loaders, services, and repositories |
| `src-tauri/src/application/` | Cross-feature contracts, ports, and orchestration |
| `src-tauri/src/domain/` | Business rules grouped into conversation, download, models, and shared entities/value objects |
| `src-tauri/src/shared/` | Cross-cutting types, errors, IPC, filesystem, HTTP, persistence formatting, runtime, and resilience helpers |
| `src-tauri/src/infrastructure/` | Shared technical implementations, grouped by responsibility |
| `src-tauri/src/interfaces/` | Dependency injection and shared IPC adapters |
| `src-tauri/src/bin/<tool>/` | Developer executables and their private helpers |
| `src-tauri/src/desktop_e2e/` | Helpers compiled only into the instrumented desktop executable |
| `api-rust/src/` | Optional sync service, organized into HTTP, sync, and persistence modules |
| `scripts/build/` | Build entry points and their configuration |
| `scripts/dev/` | Local development launchers |
| `src-tauri/scripts/` | Desktop-crate maintenance and verification scripts |

Keep code with its owner: embedding loading belongs in `features/embedding/`,
LLM loading in `features/llm/`, messaging in `infrastructure/events/`, and
cross-feature SQL adapters in `infrastructure/persistence/repositories/`.
Avoid adding loose implementation files to the crate or infrastructure roots.
`lib.rs`, `main.rs`, and each folder's `mod.rs` define entry points and module
surfaces. Move callers to the new module path when reorganizing code.

Use `domain/models/` for model catalogs, selection, metadata, and artifact rules;
`domain/conversation/` owns conversation aggregates and memory rules, and
`domain/download/` owns sessions and progress snapshots. Shared primitives live
in `shared/types/`, while technical helpers go in their named module (`fs`,
`http`, `runtime`, `resilience`, `encoding`, `persistence`, or `text`). Keep
application errors in `shared/error/` and IPC envelopes in `shared/ipc/`.
Do not recreate a generic `shared/utils/` folder or a second `Result` alias.
`domain/error.rs` and `shared/constants.rs` remain small, explicit module roots.

The npm entry points remain the supported shortcuts: `npm run dev`,
`npm run build`, and `npm run build:node`. For direct invocation, use paths such
as `bash scripts/dev/dev.sh help` or `bash scripts/build/build.sh --help`.
Run the base-schema smoke check with `bash src-tauri/scripts/test-migrations.sh`.

## Architecture conventions

Each feature repository is the source of truth for persisted feature state.
Code in domain and use-case layers should not inspect the filesystem or issue
raw SQL to decide whether state exists. Add a typed repository operation when
one is missing. Filesystem access is appropriate for user-selected imports and
artifact cleanup; exceptions to the automated check require an inline
`repository-barrier-allow` comment explaining why.

Frontend command wrappers live in `src/features/<feature>/api/client.ts`.
`src/lib/api.ts` is the public facade, and `src/shared/ipc/` owns routing,
transport, and error normalization. Keep shared persisted queries and mutations
with their feature; every mutation must update or invalidate all relevant query
keys. Stores and model helpers must not import views.

In the frontend, use React Query for state owned by the backend. Zustand is for
UI-only preferences such as panel state, sorting, and theme. Do not mirror
backend state in a client store or `localStorage`.

Keep one representation for each domain concept. Rust DTOs exported over IPC
must be reflected in the generated TypeScript bindings rather than duplicated
by hand.

When adding a Tauri command, update all five integration points:

1. The `#[tauri::command]` implementation.
2. The feature's `tauri::generate_handler!` registration.
3. The command list in `src-tauri/build.rs`.
4. The permission entry in `src-tauri/capabilities/main.json`.
5. The command list in `src-tauri/src/bin/export_bindings/main.rs`, then
   `npm run bindings:generate`.

Missing step 3 or 4 still compiles and is rejected at runtime;
`npm run contracts:commands` catches it.

Schema changes go in a new dated migration,
`src-tauri/migrations/YYYYMMDDHHMMSS_name.sql`. Never edit a migration that
has been applied, including the squashed `20260916000000_init_schema.sql`;
sqlx checksums applied migrations and startup refuses a changed history.
Lattice is pre-release, so don't add compatibility code for old local data.
Full workspace-note edits must include the revision read by the editor. Captures
append inside a repository transaction; do not implement read/modify/write of a
whole note in the renderer. A conflict must retain the draft and surface an error.

## Before opening a pull request

Run the checks relevant to your change. The CI workflow is the authoritative
list; the common local checks are:

For all test layers, use `npm run test:all` after installing the desktop test
prerequisites. It includes renderer coverage, all non-ignored Rust tests,
Chromium/WebKit journeys and the isolated packaged desktop suite. See the
[testing guide](e2e/README.md) for individual commands, coverage boundaries and
failure artifacts. Use `npm run test:unit` for a non-interactive Vitest run.

```bash
npm run type-check
npm run lint
npm test -- --run
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml --lib
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
python3 scripts/check-sql-contracts.py
python3 scripts/check-tauri-command-inventory.py
```

If you add a dependency or a route to the startup path, build the renderer
and run the initial-JavaScript budget check CI enforces:

```bash
npx vite build
node scripts/check-initial-js-budget.mjs dist
```

If an IPC contract changes, also run:

```bash
npm run bindings:generate
npm run contracts:check
```

## Repository hygiene

Do not commit assistant prompts, chat transcripts, generated audit output,
scratchpads, test reports, crash logs, or paths specific to your machine. Keep
documentation about current behavior near the code it describes; use issues or
pull requests for temporary plans and progress reports.
