# Recall Monorepo

Recall is a local-first knowledge system with:
- an app (`Tauri + Rust + React`) for indexing/searching local content
- an optional API service (`FastAPI + PostgreSQL/pgvector`) for API/sync workflows

## Repository Layout

```text
.
├── src/
│   ├── app/                      # Tauri app
│   │   ├── websrc/               # React/TypeScript frontend
│   │   └── src/                  # Rust crate root (Cargo.toml)
│   │       └── src/crates/recall  # Main Rust application crate
│   ├── api/                      # FastAPI service
│   │   ├── src/                  # Python source
│   │   ├── migrations/           # Alembic + SQL migration assets
│   │   └── tests/                # pytest suite
│   ├── docs/user/                # User-facing docs
│   └── websrc/types/api/         # Generated API types
├── docs/                         # Cross-cutting architecture/API docs
└── README.md
```

## Tech Stack

- App UI: React 19, TypeScript, Vite, Tailwind
- App Runtime: Tauri 2, Rust, SQLite (`sqlx`), ONNX/ML tooling
- API: FastAPI, SQLAlchemy, Alembic
- API Storage: PostgreSQL + `pgvector`
- Testing: Vitest/Playwright (app), pytest (api)

## Quick Start

### App

Prereqs:
- Node.js 18+
- Rust toolchain
- Tauri prerequisites for your OS

Commands:

```bash
cd src/app
npm install
npm run tauri:dev
```

Notes:
- `npm run tauri:dev` launches the full app dev loop (frontend + Tauri shell + Rust).
- You do not need to run a separate `cargo build` just to start local development.

Rust-only workflows (from repo root):

```bash
cargo check --manifest-path src/app/src/Cargo.toml
cargo build --manifest-path src/app/src/Cargo.toml
cargo test --manifest-path src/app/src/Cargo.toml
cargo fmt --manifest-path src/app/src/Cargo.toml
```

Frontend-only workflows (app web UI):

```bash
npm run -C src/app type-check
npm run -C src/app test
npm run -C src/app lint
```

Useful app commands:

```bash
npm run lint
npm run test
npm run test:e2e
npm run tauri:build
```

Release build (app bundle):

```bash
npm run -C src/app tauri:build
```

### API Service

Prereqs:
- Python 3.11+
- Poetry
- PostgreSQL 15+ with `pgvector`

Commands:

```bash
cd src/api
poetry install
cp .env.example .env
poetry run alembic upgrade head
poetry run recall-api
```

Useful API commands:

```bash
poetry run pytest
poetry run ruff check .
poetry run mypy src/
```

## Primary Entry Points

- API app factory: `src/api/src/api/app.py`
- API runner: `src/api/src/main.py`
- App Rust binary entry: `src/app/src/src/crates/recall/main.rs`
- App React entry: `src/app/websrc/main.tsx`

## Documentation Conventions

- Cross-cutting architecture/API contracts: `docs/`
- API-specific technical docs: `src/api/docs/`
- App-specific technical docs: `src/app/docs/` and `src/app/src/docs/`
- User-facing product docs: `src/docs/user/`
- Historical/one-off implementation notes: `src/api/docs/archive/` and `src/app/src/docs/archive/`

Generated artifacts (test reports, build logs, ad-hoc output files) should not be committed.

## License

MIT
