# Contributing

Thanks for helping improve Lattice. Keep changes focused, include tests for
behavioral changes, and avoid committing local reports or work-session notes.

## Setup

Install Node.js 20 or newer, a current Rust toolchain, and the platform
prerequisites listed by Tauri. Then run:

```bash
cd src
npm ci
npm run tauri:dev
```

## Architecture conventions

Each feature repository is the source of truth for persisted feature state.
Code in domain and use-case layers should not inspect the filesystem or issue
raw SQL to decide whether state exists. Add a typed repository operation when
one is missing. Filesystem access is appropriate for user-selected imports and
artifact cleanup; exceptions to the automated check require an inline
`repository-barrier-allow` comment explaining why.

In the frontend, use React Query for state owned by the backend. Zustand is for
UI-only preferences such as panel state, sorting, and theme. Do not mirror
backend state in a client store or `localStorage`.

Keep one representation for each domain concept. Rust DTOs exported over IPC
must be reflected in the generated TypeScript bindings rather than duplicated
by hand.

When adding a Tauri command, update all four integration points:

1. The `#[tauri::command]` implementation.
2. The feature's `tauri::generate_handler!` registration.
3. The command list in `src/src/build.rs`.
4. The permission entry in `src/src/capabilities/main.json`.

## Before opening a pull request

Run the checks relevant to your change. The CI workflow is the authoritative
list; the common local checks are:

```bash
npm run -C src type-check
npm run -C src lint
npm run -C src test -- --run
cargo fmt --manifest-path src/src/Cargo.toml --all -- --check
cargo clippy --manifest-path src/src/Cargo.toml --all-targets
cargo test --manifest-path src/src/Cargo.toml --lib
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
python3 scripts/check-sql-contracts.py
```

If an IPC contract changes, also run:

```bash
npm run -C src bindings:generate
npm run -C src contracts:check
```

## Repository hygiene

Do not commit assistant prompts, chat transcripts, generated audit output,
scratchpads, test reports, crash logs, or paths specific to your machine. Keep
documentation about current behavior near the code it describes; use issues or
pull requests for temporary plans and progress reports.
