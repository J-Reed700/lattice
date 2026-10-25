# Lattice

Lattice is a local-first desktop knowledge base. It indexes the files on your
machine, searches them by keyword and by meaning, and answers questions with
local language models — every answer cites the documents it came from.

Lattice needs no account or cloud service, and this version sends no product
telemetry. Your index stays on your computer. If you explicitly choose a cloud
chat provider, remote model server, or custom tool, that turn's prompt and the
retrieved passages it needs are sent to that service; the
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

The first launch offers a model bundle recommended for your hardware: an
embedding model (Qwen3-Embedding where there is a GPU backend, MiniLM
otherwise) and a local chat model. Downloads continue in the background, the
full catalog stays in Settings, and after that it works fully offline.
Optional chat providers (Ollama, a remote llama.cpp server, OpenAI,
Anthropic, Google) are available if you want them; `crates/lattice-sync/` holds an
experimental sync service the app does not use yet.

For a cloud provider, open **Settings → Chat**, save its API key, and choose
from the available models. Use **Refresh** to reload the provider's list.
Models from providers with saved keys also appear in each chat's model picker;
choosing one there changes that conversation's model, keeping the app default.

## Getting started

The desktop release targets are Windows 11 x64, macOS 13.3+ (Apple Silicon and
Intel), and Ubuntu 24.04 LTS x64. Intel Mac support is being qualified and uses
CPU inference. See [platform support](docs/development/platform-support.md)
for its build instructions and remaining release gates.

Native compatibility tests build and exercise isolated app packages on macOS
(both architectures), Windows x64 and Ubuntu. See the
[desktop test guide](e2e/desktop/README.md) for local commands and CI coverage.

### Install on a new machine

One command installs everything the build needs, builds the app, installs it
and opens it. Steps that are already done are skipped, so running it again
just rebuilds and reinstalls.

macOS or Linux, from a checkout:

```bash
./setup.sh
```

Without a checkout (the source goes to `~/lattice`):

```bash
curl -fsSL https://raw.githubusercontent.com/J-Reed700/lattice/main/setup.sh | bash
```

Windows: double-click `setup.cmd`, or without a checkout run
`irm https://raw.githubusercontent.com/J-Reed700/lattice/main/setup.ps1 | iex`
in PowerShell.

What the script installs when it is missing:

| | macOS | Linux (apt, dnf) | Windows (winget) |
| --- | --- | --- | --- |
| Build tools | Xcode Command Line Tools | WebKitGTK 4.1, GTK and the other [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), a C/C++ toolchain, git, curl, python3, the Vulkan loader | Visual Studio 2022 Build Tools (C++), Git, Python 3 |
| Rust | rustup, stable (1.95 or newer) | same | same |
| Node.js | 24, unpacked into `~/.local/share/lattice/toolchain` (or `$XDG_DATA_HOME/lattice/toolchain`) | same | Node.js LTS |
| Project | `npm ci`, the pinned `llama-server` sidecar | same | same |
| App | `Lattice.app` in `/Applications` | the `.deb` or `.rpm`, installed with apt or dnf | the NSIS installer, current user |

The build needs about 8 GB of memory, counting swap: compiling the app crate
peaked near 7 GB in testing. On Linux the script checks memory and swap before
it starts and prints the commands to add swap when there is too little.

On Arch Linux the script installs the prerequisites with pacman, but there is
no Arch package to build, so use `./setup.sh --dev` there.

On first launch the app shows "Set up AI" with a chat model and an embedding
model sized to the machine's memory. Click Install. The downloads run in the
background and both models are switched on when they finish.

Other modes: `./setup.sh --check` lists what is missing and changes nothing,
`--deps` stops once the build prerequisites are installed, `--dev` runs
`npm run tauri:dev` instead of installing, and `--yes` never prompts. The
Windows script takes `-Check`, `-Deps`, `-Dev` and `-Yes`.
`e2e/turnkey/` tests the Linux path on clean Ubuntu containers; see its
[README](e2e/turnkey/README.md).

### Set up by hand

You'll need Node.js 20.19+ or 22.12+ (CI uses 24), Rust 1.95 or newer, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform. Local generation uses a bundled `llama-server` sidecar; the build
refuses to start without it. See
[crates/lattice-desktop/binaries](crates/lattice-desktop/binaries/README.md)
for how it is fetched.

```bash
npm ci
bash crates/lattice-desktop/scripts/fetch-llama-binaries.sh
npm run tauri:dev
```

The frontend can run on its own with `npx vite` (port 5173), but anything that
talks to the Rust backend needs the Tauri dev process.

## Repository layout

```text
.
├── crates/
│   ├── lattice-core/      # Domain, application ports and contracts; no Tauri
│   ├── lattice-desktop/   # The Tauri app: features, infrastructure, commands,
│   │                      # migrations, sidecar binaries and app configuration
│   └── lattice-sync/      # Experimental sync service (its own workspace)
├── src/                   # React and TypeScript frontend
├── e2e/                   # Playwright renderer journeys and native desktop tests
├── evals/                 # Retrieval, grounding and memory eval sets and results
├── docs/                  # Architecture, ADRs, design docs and user docs
├── scripts/               # Repository checks and developer utilities
└── Cargo.toml             # Rust workspace: lattice-core and lattice-desktop
```

`lattice-desktop` depends on `lattice-core`, never the other way round; see
[Rust architecture](docs/RUST_ARCHITECTURE.md) for the layer rules.

## Development checks

Frontend:

```bash
npm run type-check
npm run lint
npm test -- --run
npx vite build && node scripts/check-initial-js-budget.mjs dist
```

Production renderer smoke tests (Chromium and WebKit, with simulated desktop IPC):

```bash
npx playwright install --with-deps chromium webkit
npm run test:e2e
```

The suite builds its own renderer in `e2e-results/renderer` and serves it on port
4173. Keep that port free; the tests do not reuse a running development server.

Desktop backend (run cargo from the app crate, whose `.cargo/config.toml`
sets offline SQLx and the test thread stack):

```bash
cd crates/lattice-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace --lib
```

Architecture and contract checks:

```bash
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
python3 scripts/check-sql-contracts.py
python3 scripts/check-tauri-command-inventory.py
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for project conventions and the full
change checklist.

## License

Lattice is available under the [MIT License](LICENSE).
