# Lattice

Lattice is a local-first desktop knowledge base. It indexes the files on your
machine, searches them by keyword and by meaning, and answers questions with
local language models — every answer cites the documents it came from.

Lattice needs no account or cloud service, and this version sends no product
telemetry. Your index stays on your computer. Network features are opt-in: a
cloud chat or verifier provider, a remote model server, a custom tool, web
search and the source finders in Settings → Sources (also used by Learning
Studio research and claim checking) send the request they need, such as the
prompt and its retrieved passages or a search query, to that service. The
[privacy FAQ](docs/user/faq.md#what-leaves-my-computer) lists every networked
feature.

## Screenshots

Screenshots from the running macOS app. Click an image to view it at full size.

| Home dashboard | Explorer and chat |
| --- | --- |
| [![Home dashboard with library statistics and recent conversations](docs/screenshots/dashboard.png)](docs/screenshots/dashboard.png) | [![Explorer with a folder tree, Rust source file, and a chat about the code](docs/screenshots/explorer.png)](docs/screenshots/explorer.png) |
| **Journal and sources** | **Learning Studio** |
| [![Journal entry with inline citations and the sources panel open](docs/screenshots/journal.png)](docs/screenshots/journal.png) | [![Learning Studio with options to build a program and create a flashcard deck](docs/screenshots/studio.png)](docs/screenshots/studio.png) |

## What it does

- **Search that understands meaning.** Indexing runs locally: text is
  extracted, embedded on-device, and stored in SQLite. Search blends exact
  keyword matching with vector similarity, so "that memo about the offsite"
  finds the document even if the word "offsite" never appears in your query.
- **Chat with receipts.** Ask questions against your library and get answers
  with footnoted citations you can click through to the source. A retrieval
  trace shows which documents were pulled in and why.
- **A home page that tells the truth.** The dashboard reports what's actually
  in your library — document counts, storage used, recent files, and the
  conversations worth picking back up. If a number can't be computed, the
  tile is left out rather than showing a zero.
- **Learning Studio.** Build a learning program from your sources: a
  curriculum, practice, hands-on labs that run in a sandbox, assessments, and
  recall. Flashcard decks live here too: turn a document or a conversation
  into a deck, review with spaced repetition, and every card keeps its
  citations so you can check the evidence behind it.
- **Explorer.** Open a folder from disk beside a chat. Browse the tree, read
  files in a code viewer, and ask questions the model answers by reading that
  folder, pointing at the exact lines. Each folder gets its own search index,
  kept apart from your library.
- **Documents that know their neighbors.** Open any document and the related
  panel shows what links to it, what's similar, and which conversations cite
  it.
- **A journal and a reference inbox** for the notes and passages you want to
  keep coming back to, plus quick capture from anywhere.
- **Rich chat.** Attach pictures for vision-capable models, render math and
  Mermaid diagrams in answers, and pick a different model for a single chat.
- **Checked courses.** Learning Studio courses check each claim against its
  citation, corroborate it across the course, and hold doubtful claims for
  your review. A course can be packed up and moved to another computer.
- **Sources you can trust.** Settings → Sources configures web finders
  (keyless ones plus optional keys for Brave, Tavily, Exa and others), and
  Source trust controls how Learning Studio weighs who published what a lesson
  cites. Cited web pages are frozen as snapshots so citations keep working.
- **Backups** of the database on this computer, plus an optional encrypted
  off-device archive written to a folder you choose.

The first launch offers a model bundle recommended for your hardware: the
Qwen3-Embedding-0.6B embedding model and a local chat model, both run by a
bundled llama.cpp server (Metal, Vulkan or CPU). Until the embedding model is
downloaded, library search is keyword-only. Downloads continue in the
background, the full catalog (including optional local reranker, Whisper audio
transcription and PaddleOCR-VL scanned-PDF OCR models) stays in Settings, and
after that it works fully offline. Optional chat providers (Ollama, a remote
llama.cpp server, OpenAI, Anthropic, Google) are available if you want them;
`crates/lattice-sync/` holds an experimental sync service the app does not use
yet.

## Getting started

The desktop release targets are Windows 11 x64, macOS 13.3+ (Apple Silicon and
Intel), and Ubuntu 24.04 LTS x64. Intel Mac support is being qualified and uses
CPU inference. See [platform support](docs/development/platform-support.md)
for its build instructions and remaining release gates.

Native compatibility tests build and exercise isolated app packages on macOS
(both architectures), Windows x64 and Ubuntu. See the
[desktop test guide](e2e/desktop/README.md) for local commands and CI coverage.

You'll need Node.js 22.22.2+, 24.15+ or 26+ (CI uses 24; see `.nvmrc`), a current
Rust toolchain, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
for your platform. Local models run on a `llama-server` sidecar that is not
committed; fetch the pinned binaries before the first build (see
[crates/lattice-desktop/binaries](crates/lattice-desktop/binaries/README.md)).
On macOS, if the Metal compiler is missing, install it with
`xcodebuild -downloadComponent MetalToolchain`.

```bash
npm ci
bash crates/lattice-desktop/scripts/fetch-llama-binaries.sh
npm run tauri:dev
```

The frontend can run on its own with `npx vite` (port 5173), but anything that
talks to the Rust backend needs the Tauri dev process. `npm run dev` wraps the
same launch with extra helpers; run `bash scripts/dev/dev.sh help` to list
them. No environment variables are required; `.env.example` lists the optional
ones.

### Building an installer

```bash
npm run tauri:build
```

This verifies the bundled learning Python runtime, builds the renderer and
writes installers under `target/release/bundle/`. Release builds refuse any
sidecar whose checksum does not match
`crates/lattice-desktop/scripts/llama-server.lock`. `npm run build` wraps the
same build with optional pre-build tests and checksums
(`bash scripts/build/build.sh --help`). macOS bundles are ad-hoc signed;
Developer ID signing and notarization are not automated yet.

## Repository layout

```text
.
├── .github/workflows/     # CI, eval integrity, sidecar builds, lattice-sync
├── crates/
│   ├── lattice-core/      # Domain, application ports and contracts; no Tauri
│   ├── lattice-desktop/   # The Tauri app: features, infrastructure, commands,
│   │                      # migrations, sidecar binaries and app configuration
│   ├── lattice-sync/      # Experimental sync service (its own workspace)
│   └── vendor/            # Vendored third-party code (readability-js)
├── src/                   # React and TypeScript frontend
├── e2e/                   # Playwright renderer journeys and native desktop tests
├── evals/                 # Retrieval, grounding, course and memory eval sets
├── docs/                  # Architecture, ADRs, design docs and user docs
├── scripts/               # Repository checks, eval runners, build and dev tools
└── Cargo.toml             # Rust workspace: lattice-core and lattice-desktop
```

`lattice-desktop` depends on `lattice-core`, never the other way round; see
[Rust architecture](docs/RUST_ARCHITECTURE.md) for the layer rules.

## Development checks

[CI](.github/workflows/ci.yml) is the authoritative list. Two umbrella
commands cover most of it:

```bash
npm run test:check   # type checks, lint, IPC contracts, renderer coverage
npm run test:all     # test:check plus Rust, browser and packaged desktop tests
```

The [testing guide](e2e/README.md) explains each layer. The individual checks:

Frontend:

```bash
npm run type-check
npm run type-check:e2e
npm run lint
npm run contracts:check
npm run test:coverage          # enforces coverage floors; npm run test:unit skips them
npm run learning:runtime:verify
npx vite build && node scripts/check-initial-js-budget.mjs dist
```

Production renderer smoke tests (Chromium and WebKit, with simulated desktop IPC):

```bash
npx playwright install --with-deps chromium webkit
npm run test:e2e
```

The suite builds its own renderer in `e2e-results/renderer` and serves it on port
4173. Keep that port free; the tests do not reuse a running development server.

Desktop backend. Cargo needs `dist/` (run `npx vite build`) and the sidecar
files to exist; debug builds also accept empty placeholder sidecars. Run cargo
from the app crate, whose `.cargo/config.toml` sets offline SQLx and the test
thread stack:

```bash
cd crates/lattice-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo run --features bindings-export --bin export_bindings -- --check
cd ../..
npm run test:rust              # every non-ignored lib, bin and integration test
```

Packaged desktop tests (`npm run test:desktop:build`, `npm run test:desktop`)
are covered in the [desktop test guide](e2e/desktop/README.md).

Architecture and contract checks:

```bash
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
cargo test --locked --manifest-path scripts/data-contract-check/Cargo.toml
python3 scripts/check-sql-contracts.py
python3 scripts/check-tauri-command-inventory.py
python3 -m unittest scripts/test_tauri_command_inventory.py
```

Eval integrity (if you touch `evals/` or the eval scripts):

```bash
python3 -m unittest discover -s scripts -p 'test_*rag_eval.py'
python3 scripts/validate_rag_evals.py
```

CI also runs `npm audit --audit-level=high` and `cargo audit`.

## Other workflows

| Task | How |
| --- | --- |
| Change an IPC contract or add a Tauri command | `npm run bindings:generate`, then `npm run contracts:check`; see [CONTRIBUTING.md](CONTRIBUTING.md) |
| Change a SQL query | `bash crates/lattice-desktop/scripts/update-sqlx-cache.sh` (needs `sqlx-cli`) and commit `.sqlx/` |
| Add a migration | New dated file in `crates/lattice-desktop/migrations/`; smoke-test with `bash crates/lattice-desktop/scripts/test-migrations.sh` |
| Bump or rebuild llama.cpp | [binaries/README.md](crates/lattice-desktop/binaries/README.md) |
| Rebuild the learning Python runtime | `bash scripts/build-learning-python.sh` (macOS arm64, Python 3.11+), then `npm run learning:runtime:verify` |
| Run retrieval, grounding or course evals | [evals/retrieval/README.md](evals/retrieval/README.md), other `evals/*/README.md`, and opt-in Rust evals in [tests/README.md](crates/lattice-desktop/tests/README.md) |
| Build or test the sync service | [crates/lattice-sync/README.md](crates/lattice-sync/README.md) |
| Intel Mac builds | [platform support](docs/development/platform-support.md) |

See [CONTRIBUTING.md](CONTRIBUTING.md) for project conventions and the full
change checklist.

## License

Lattice is available under the [MIT License](LICENSE).
