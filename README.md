# Lattice

Lattice is a local-first desktop knowledge base. It indexes the files on your
machine, searches them by keyword and by meaning, and answers questions with
local language models — every answer cites the documents it came from.

Nothing you index leaves your computer. There is no account, no cloud, and no
telemetry.

## Screenshots

The screenshots below are captured from the running app; the ones you're
missing are listed in [docs/screenshots](docs/screenshots/README.md) with
notes on what to capture.

| | |
| --- | --- |
| ![Home dashboard](docs/screenshots/dashboard.png) | ![Search results](docs/screenshots/search.png) |
| ![Chat with source citations](docs/screenshots/chat.png) | ![Flashcard review](docs/screenshots/study.png) |
| ![Related documents panel](docs/screenshots/neighborhood.png) | ![Model settings](docs/screenshots/settings.png) |

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
- **Study mode.** Turn a document or a conversation into a flashcard deck.
  Review with spaced repetition, take practice quizzes, and every card keeps
  its citations so you can check the evidence behind it.
- **Documents that know their neighbors.** Open any document and the related
  panel shows what links to it, what's similar, and which conversations cite
  it.
- **A journal and a reference inbox** for the notes and passages you want to
  keep coming back to, plus quick capture from anywhere.

The first launch downloads an embedding model (a few hundred MB); after that
it works fully offline. Optional model providers and an experimental sync
service are available if you want them.

## Getting started

The desktop release targets are Windows 11 x64, macOS 13.3+ (Apple Silicon and
Intel), and Ubuntu 24.04 LTS x64. Intel Mac support is being qualified and uses
CPU inference. See [platform support](docs/development/platform-support.md)
for its build instructions and remaining release gates.

Native compatibility tests build and exercise isolated app packages on macOS
(both architectures), Windows x64 and Ubuntu. See the
[desktop test guide](e2e/desktop/README.md) for local commands and CI coverage.

You'll need Node.js 20 or newer, a current Rust toolchain, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform.

```bash
npm ci
npm run tauri:dev
```

The frontend can run on its own with `npm run dev`, but anything that talks
to the Rust backend needs the Tauri dev process.

## Repository layout

```text
.
├── src/           # React and TypeScript frontend
├── src-tauri/     # Rust backend and Tauri configuration
├── api-rust/      # Experimental Rust sync service
├── docs/          # Architecture notes, design docs, and user docs
└── scripts/       # Repository checks and developer utilities
```

## Development checks

Frontend:

```bash
npm run type-check
npm run lint
npm test -- --run
```

Production renderer smoke tests (Chromium and WebKit, with simulated desktop IPC):

```bash
npx playwright install --with-deps chromium webkit
npm run test:e2e
```

The suite builds its own renderer in `e2e-results/renderer` and serves it on port
4173. Keep that port free; the tests do not reuse a running development server.

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
