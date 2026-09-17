# Lattice

Lattice is a local-first desktop knowledge base. It indexes files on your
computer, supports keyword and semantic search, and uses local language models
for retrieval and chat.

The project is under active development.

## Features

- Local document indexing and full-text search
- Semantic search with on-device embeddings
- Retrieval-backed chat with source citations
- Notes, references, study tools, and import workflows
- Optional model providers and an experimental sync service

## Repository layout

```text
.
├── src/           # React and TypeScript frontend
├── src-tauri/     # Rust backend and Tauri configuration
├── api-rust/      # Experimental Rust sync service
├── docs/user/     # User documentation
├── docs/          # Architecture and API documentation
└── scripts/       # Repository checks and developer utilities
```

## Run the desktop app

You will need Node.js 20 or newer, a current Rust toolchain, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform.

```bash
npm ci
npm run tauri:dev
```

The frontend can be run on its own with `npm run dev`, but features that call
the Rust backend require the Tauri development process.

## Development checks

Frontend:

```bash
npm run type-check
npm run lint
npm test -- --run
```

Desktop backend:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Architecture and contract checks:

```bash
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
python3 scripts/check-sql-contracts.py
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for project conventions and the full
change checklist.

## License

Lattice is available under the [MIT License](LICENSE).
