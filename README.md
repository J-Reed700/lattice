# Recall Monorepo

Recall is a local-first knowledge system with:
- a desktop app (`Tauri + Rust + React`) for indexing/searching local content
- an optional backend service (`FastAPI + PostgreSQL/pgvector`) for API/sync workflows

## Repository Layout

```text
.
├── vault/
│   ├── desktop/                  # Tauri desktop app
│   │   ├── websrc/               # React/TypeScript frontend
│   │   └── src/                  # Rust crate root (Cargo.toml)
│   │       └── src/crates/recall # Main Rust application crate
│   ├── backend/                  # FastAPI service
│   │   ├── src/                  # Python source
│   │   ├── migrations/           # Alembic + SQL migration assets
│   │   └── tests/                # pytest suite
│   ├── docs/user/                # User-facing docs
│   └── websrc/types/api/         # Generated API types
├── docs/                         # Cross-cutting architecture/API docs
└── README.md
```

## Tech Stack

- Desktop UI: React 19, TypeScript, Vite, Tailwind
- Desktop Runtime: Tauri 2, Rust, SQLite (`sqlx`), ONNX/ML tooling
- Backend API: FastAPI, SQLAlchemy, Alembic
- Backend Storage: PostgreSQL + `pgvector`
- Testing: Vitest/Playwright (desktop), pytest (backend)

## Quick Start

### Desktop App

Prereqs:
- Node.js 18+
- Rust toolchain
- Tauri prerequisites for your OS

Commands:

```bash
cd vault/desktop
npm install
npm run tauri:dev
```

Notes:
- `npm run tauri:dev` launches the full desktop app dev loop (frontend + Tauri shell + Rust backend).
- You do not need to run a separate `cargo build` just to start local desktop development.

Rust-only workflows (from repo root):

```bash
cargo check --manifest-path vault/desktop/src/Cargo.toml
cargo build --manifest-path vault/desktop/src/Cargo.toml
cargo test --manifest-path vault/desktop/src/Cargo.toml
cargo fmt --manifest-path vault/desktop/src/Cargo.toml
```

Frontend-only workflows (desktop web UI):

```bash
npm run -C vault/desktop type-check
npm run -C vault/desktop test
npm run -C vault/desktop lint
```

Useful desktop commands:

```bash
npm run lint
npm run test
npm run test:e2e
npm run tauri:build
```

Release build (desktop bundle):

```bash
npm run -C vault/desktop tauri:build
```

### Backend Service

Prereqs:
- Python 3.11+
- Poetry
- PostgreSQL 15+ with `pgvector`

Commands:

```bash
cd vault/backend
poetry install
cp .env.example .env
poetry run alembic upgrade head
poetry run vault-api
```

Useful backend commands:

```bash
poetry run pytest
poetry run ruff check .
poetry run mypy src/
```

## Primary Entry Points

- Backend app factory: `vault/backend/src/api/app.py`
- Backend runner: `vault/backend/src/main.py`
- Desktop Rust binary entry: `vault/desktop/src/src/crates/recall/main.rs`
- Desktop React entry: `vault/desktop/websrc/main.tsx`

## Documentation Conventions

- Cross-cutting architecture/API contracts: `docs/`
- Backend-specific technical docs: `vault/backend/docs/`
- Desktop-specific technical docs: `vault/desktop/docs/` and `vault/desktop/src/docs/`
- User-facing product docs: `vault/docs/user/`
- Historical/one-off implementation notes: `vault/backend/docs/archive/` and `vault/desktop/src/docs/archive/`

Generated artifacts (test reports, build logs, ad-hoc output files) should not be committed.

## License

MIT
