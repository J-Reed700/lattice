No per-table filtering exists in Turso's sync engine — confirming whole-database sync. I have enough to deliver.

---

# Multi-Device Sync Engines for Lattice — Research Report

**Date:** 2026-09-16 · **Scope:** sync engines only (backup-only and auth/hosting covered by sibling agents)

> **Method note / caveat up front.** I verified most claims against primary sources (crates.io API, GitHub API + source files, vendor docs). My session's web-search budget was exhausted partway through, so some 2026-specific claims rest on a single source; those are flagged. Two background research agents (one on cr-sqlite/SQLSync/Anki, one on PowerSync/ElectricSQL) had not reported by the time I finished — everything below is my own verification, and their results may add nuance. A "Could not verify" list is at the end.

---

## 0. What I found in your repo (this changes the ranking)

Before the market survey, three facts about Lattice that materially affect the answer:

| Fact | Evidence | Why it matters |
|---|---|---|
| Client schema is ~45 tables, and PKs are **`id TEXT PRIMARY KEY`** almost everywhere — only **one** table uses `INTEGER PRIMARY KEY AUTOINCREMENT` | `/Users/josh/Code/lattice-temp/src-tauri/migrations/20260916000000_init_schema.sql` | You already have the single hardest prerequisite for *any* multi-writer sync: globally-unique, client-generatable row IDs. Most teams have to do a painful ID migration first. You don't. |
| **77 `REFERENCES` clauses** and `PRAGMA foreign_keys = ON` is enforced at connection init, with a test asserting it | `/Users/josh/Code/lattice-temp/src-tauri/src/infrastructure/persistence/database/init.rs:16`, `.../database/schema_validation.rs:53` | This is a hard blocker for cr-sqlite (see §4) and a real complication for any out-of-order op application. |
| Existing server scaffold is a **document-only** op-log, not a general row-sync | `/Users/josh/Code/lattice-temp/api-rust/migrations/0001_sync_init.sql` — tables `devices`, `document_heads`, `document_ops` (`seq BIGSERIAL`, `client_op_id`, `base_version`, `path/title/content/content_hash`), `device_checkpoints` | The shape is right (monotonic server seq + per-device checkpoint + client op id for idempotency) but it only models documents. Generalizing it to ~15 tables is the actual work, not a rewrite. |
| sqlx `0.8` (sqlite, no `load-extension` feature), Tauri `=2.9.5` | `src-tauri/Cargo.toml:44,65` | sqlx 0.8 *does* support runtime-loadable extensions behind the `load-extension` feature (`unsafe fn extension()`), so cr-sqlite/Graft-style extensions are technically loadable. |

One architectural recommendation that applies **regardless of which engine you pick**: split the SQLite file in two — a small `lattice.db` with the ~15 syncable tables, and a separate `derived.db` (or several) for the embedding tables, attached via `ATTACH` where joins are needed. Several of the candidate engines (Turso especially) replicate whole database files and have no per-table filter. Doing this split first de-risks every option and is useful on its own (backup size, vacuum, corruption blast radius).

---

## 1. Executive summary — ranked top 3

### #1 — Roll your own: generalize the existing axum op-log into a per-row LWW sync with HLCs

**Why it wins for Lattice specifically:** it is the only option that satisfies all four of your hard constraints simultaneously — (a) keeps sqlx + stock SQLite untouched, (b) lets you sync exactly 15 of 50 tables, (c) makes E2EE trivial (the server stores opaque op payloads; it never needs to read them), (d) self-hosted by construction with no vendor, no license, no per-user metering. You also already have ~2k lines and the right table shapes; this is a generalization, not a greenfield build.

The prior art is unusually strong and unusually close to your situation. **Anki** is a Rust desktop app on SQLite with its own sync server that ships in the same binary and has run at AnkiWeb scale for years (§5). **Actual Budget** runs a per-column op-log with hybrid logical clocks and a merkle trie over SQLite (§8). **Linear** is server-authoritative with a monotonic `lastSyncId` — essentially the model your `document_ops.seq BIGSERIAL` already implies.

**Honest cost:** 4–8 weeks for a credible v1, plus permanent ownership. The two things that bite are *schema migration across versions* (jlongster calls this "incredibly difficult") and *tombstone/op-log growth* (needs epoching/compaction). Budget for both up front.

**Suggested shape:** server-authoritative ordering (monotonic `seq`, like Linear and your current scaffold) rather than pure CRDT. It is dramatically simpler to reason about, debuggable, and is what Anki and Linear both chose. Reserve real CRDTs for note *bodies* only (see the hybrid note below).

### #2 — PowerSync, using **Raw Tables** + the Rust SDK (the "buy" option)

**Why it's a credible hedge:** PowerSync is the only commercial sync engine in 2026 with (a) a real Rust crate you can call from a Tauri backend, and (b) a documented way to sync into *your own* normal SQLite tables rather than its schema. You already run Postgres and axum, which is exactly what PowerSync needs upstream.

- Rust crate: `powersync` **0.0.7**, Apache-2.0, published 2026-08-03, repo `powersync-ja/powersync-native` pushed **today** (2026-09-16). README self-labels: *"This SDK is currently in an alpha state… Expect breaking changes and instability."* There's an `egui_todolist` example, i.e. a real desktop target.
- **Raw Tables** is the decisive feature: it bypasses PowerSync's default JSON-view schema (`ps_data__<table>` + views) so you get native tables with FKs, custom indexes and generated columns. Docs list **Rust 0.0.4+** as supported. You supply the put/delete statements and CRUD triggers (auto-generatable via `powersync_create_raw_table_crud_trigger()`), plus your own migrations.
- Self-host: PowerSync Service is **FSL-1.1-ALv2** (source-available, no "Competing Use", converts to Apache-2.0 after 2 years) — fine for shipping your own app. Docker image; no dashboard when self-hosted.
- E2EE: documented and supported — sync ciphertext, decrypt into Raw Tables locally.

**Why it's #2, not #1:** you'd be betting your sync layer on a 0.0.x crate with 20 GitHub stars, and taking on Postgres + PowerSync Service as a mandatory operational dependency. Sync rules/streams live server-side in YAML, so "which 15 tables sync" becomes config — nice — but the write path still goes through PowerSync's CRUD queue and your backend connector.

### #3 — Turso sync (`turso` crate + `sync` feature) — only if you split the DB first

**Why it's interesting:** Rust-native, bidirectional push/pull, MIT, and there is a self-hostable sync server (`tursodb ./server.db --sync-server 0.0.0.0:8080`) that the docs say "implements the same sync protocol as Turso Cloud." Conflict model is documented row-level **last-push-wins** with atomic rollback-and-replay on pull.

**Why it's only #3 — three specific problems:**
1. **It replaces your database engine.** Turso Database is a from-scratch Rust rewrite of SQLite, not a fork; the crate is `0.8.0-pre.11` (every release to date is a prerelease). Turso's own guidance is that new sync work happens in Turso, not libSQL.
2. **sqlx compatibility is embryonic.** `sqlx-turso` is `0.1.0-alpha.1`, published 2026-05-26, **78 total downloads**, no repository field on crates.io. `launchbadge/sqlx` issue #2674 (libSQL/Turso driver support) is still open. Realistically you'd rewrite your data layer onto the `turso` crate's API.
3. **Whole-database sync.** I grepped `sync/engine/src/` and found no per-table include/exclude mechanism. "Partial sync" is *page-level lazy fetch* (prefix or query bootstrap), not table selection. Your embedding tables would sync unless you physically split the file.

**And the privacy caveat:** Turso's "remote encryption" is *not* E2EE. From `bindings/rust/src/sync.rs:289-292`: *"Set encryption key (base64-encoded) for the Turso Cloud database. The key will be sent as `x-turso-encryption-key` header with sync HTTP requests."* The server receives your key. The same file notes the sync engine *does not support local at-rest encryption* at all. For a privacy-focused product, say that out loud before adopting.

### The hybrid worth considering above all three

Row-level LWW is wrong for one thing in your app: **concurrently edited note bodies**, where LWW silently discards a whole edit session. The strongest architecture I'd propose is:

- **Relational tables** (tags, favorites, decks, review history, conversation metadata) → op-log + per-row/per-column LWW with HLCs. Tiny, mergeable, easy to reason about.
- **Note/document bodies and the markdown vault** → a text CRDT (**Loro** or **Automerge**), stored as a binary blob in a `note_crdt(note_id, doc BLOB)` table, with the plain-text projection written back into your existing `documents.content` column so FTS and embeddings keep working unchanged.

This is the standard "CRDT is source of truth, SQLite is the derived read model" pattern, and it avoids the worst failure mode of pure-LWW note sync while keeping 90% of your schema on simple machinery. Loro 1.16.0 (2026-09-06, MIT) and Automerge 0.11.0 (2026-08-12, MIT) are both mature enough; neither ships a sync server, which is fine because your axum server just relays opaque byte ranges — which is also exactly how you get E2EE for note content.

---

## 2. Comparison table

| Option | Rust client (2026) | Self-host | Conflict model | E2EE | Pricing | Maturity | Integration burden on sqlx SQLite |
|---|---|---|---|---|---|---|---|
| **Roll your own op-log** | N/A (it's yours) | Yes, by definition | Your choice; recommend server-ordered seq + per-row/col LWW + HLC | Trivial (opaque op payloads) | Your hosting only | You own it | **Low friction, high labor.** Schema untouched; add `_ops`/`_meta` tables + triggers or repo-layer hooks |
| **PowerSync** | `powersync` 0.0.7, Apache-2.0, **alpha** | Yes — FSL-1.1-ALv2, Docker | Server-authoritative; client CRUD queue uploaded via your backend | Yes, documented (sync ciphertext → Raw Tables) | Free 2GB/mo, 50 clients; Pro from $49/mo; Team from $599/mo; Open Edition free | Service mature (382★, pushed today); **Rust SDK alpha** | Medium. Raw Tables keeps your schema; adds Postgres + sync service + triggers |
| **Turso sync** | `turso` 0.8.0-pre.11 + `sync` feature, MIT — **first-class** | Yes (`--sync-server`), but **no auth** on local server | Row-level **last-push-wins**; rollback-and-replay on pull | **No** — key is sent to server in a header | Free 5GB/3GB syncs; Dev $4.99/mo; Scaler $24.92/mo; Pro $416.58/mo | Engine is all-prerelease; sync docs dated 2026 | **High.** Engine swap; `sqlx-turso` is 0.1.0-alpha.1 (78 downloads); whole-DB sync |
| **ElectricSQL** | **None.** TS + Elixir clients only | Yes, Apache-2.0 | **Read-path only** — you build all writes | N/A (Postgres is plaintext source of truth) | Electric Cloud (not verified); OSS free | 10.4k★, very active | **Wrong shape.** Read-only sync from Postgres; you'd hand-roll HTTP Shape consumption *and* the entire write path |
| **cr-sqlite / vlcn** | Loadable extension; sqlx `load-extension` works | Yes (you write the server) | Column-level LWW + causal-length sets | Possible (relay opaque changesets) | Free, MIT | **Last release v0.16.3, Jan 2024.** 2026 commits are community build fixes only | **High + blocking.** Forbids enforced FKs (you have 77 + a test), forbids AUTOINCREMENT PKs, forbids non-PK UNIQUE indexes, requires DEFAULT on every NOT NULL column |
| **SQLSync** | WASM/reducer-oriented | Yes | Server-authoritative rebase of reducer mutations | Not designed for it | Free, Apache-2.0 | Last commit 2025-11-19; author moved to Graft | High; web-first architecture |
| **Graft** (Carl Sverre) | Yes, `graft` crate + SQLite extension, MIT/Apache | Yes | **Rejects conflicting commits** (OCC) — no row merge | Not documented | Free | Explicitly **Alpha**; "contact @carlsverre before production" | Medium, but wrong semantics for multi-device notes |
| **Anki rslib** (reference) | Yes — it *is* Rust + SQLite | Yes, AGPL-3 | Server-authoritative USN; **forced full sync** on divergence/schema change | No (plaintext to server) | Free | Battle-tested at scale | Reference design, not a library to import |
| **Automerge** | `automerge` 0.11.0 MIT; `samod` 0.13.0 for repo/sync | Yes (you run a relay) | CRDT (true merge) | **Keyhive/Beelay: pre-alpha, unaudited, "DO NOT use in production"** | Free | Core mature; Rust repo layer in flux (`automerge-repo` stale since 2025-10, superseded by `samod`) | Medium; per-document only, not relational |
| **Loro** | `loro` 1.16.0, MIT | Yes (you run a relay) | CRDT (true merge); movable tree, rich text | Possible (relay sees opaque bytes) | Free | Very active (releases weekly); **bus factor ~2 contributors** | Medium; per-document only |
| **yrs / y-sweet** | `yrs` 0.27.4 MIT; y-sweet is Rust | Yes, y-sweet self-hostable | CRDT | Not documented | y-sweet free/OSS | yrs active; **y-sweet last commit 2025-12-04** | Medium; Yjs ecosystem is web-first |
| **Ditto** | Yes — `dittolive-ditto` 5.1.0 (2026-08-19), **proprietary** | Enterprise tier only (BYOC/self-managed) | CRDT, P2P mesh + optional cloud | Not documented publicly | **Contact sales**; free tier = 10 cloud devices / 2GB | Mature, enterprise | High + commercial gate; own store, not your SQLite |
| **Evolu** | **No** — TypeScript only | Yes (self-hosted relay) | CRDT over SQLite | **Yes, by default** (mnemonic-derived) | Free, MIT | Active (commits 2026-09-10) | N/A for Rust |
| **Triplit** | **No** — TS only | Yes, AGPL-3.0 | Server-authoritative | No | — | **Dormant: last commit 2025-09-11, last release 2025-07-31** | N/A |
| **Jazz** | **No** — TS only | Partially | CoJSON CRDTs | Claimed | Jazz Cloud (page 404'd) | Active (2026-09-15) | N/A |
| **Rocicorp Zero** | **No** — TS only | Yes | Server-authoritative rebase | No | — | Active (`rocicorp/mono` pushed today) | N/A; requires Postgres |

---

## 3. Turso / libSQL

**Current state.** libSQL and Turso Database are now two products. libSQL (`libsql` crate 0.10.0-pre.4, last publish 2026-06-02) is the SQLite *fork* with embedded replicas; Turso Database (`tursodatabase/turso`, 24.3k★, MIT, pushed 2026-09-16) is a from-scratch Rust rewrite. Turso's own docs and the April 2026 "Turso Sync" post say plainly: if you are doing sync, use Turso, not libSQL.

**The sync model (2026).** `push()`/`pull()` replaced the old single `sync()`. Push sends *logical* row changes via CDC; pull brings either physical pages or an MVCC logical log (auto-detected; `with_logical_mvcc_pull()` forces it). Turso's benchmark post (2026-04-24) claims up to 312× faster syncs and 18× less traffic vs libSQL embedded replicas — vendor benchmark, treat accordingly.

**History worth knowing.** Offline writes entered public beta 2025-03-31 with the explicit warning: *"currently in beta quality, and not yet recommended for production use… no durability guarantees, making data loss possible"* and *"Conflict detection (but resolution is not yet implemented)."* Conflict resolution now exists and is documented (page last updated 2026-02-10), so the 2025 warnings are stale — but every `turso` crate release to date is still a `-pre` prerelease.

**Rust support: genuinely first-class.** `turso` 0.8.0-pre.11 (2026-09-11), 915k downloads, MIT; `turso_sync_engine` same version; `bindings/rust/src/sync.rs` exposes builder methods, auth-token callbacks, partial-sync options and stats. Note an inconsistency: the sync docs (`turso-docs/sync/*.mdx`) show TypeScript, Python and Go examples only — Rust appears in the SDK quickstart but not the sync guides.

**sqlx: effectively no.** `sqlx-turso` 0.1.0-alpha.1 / `sqlx-turso-core` 0.1.0-alpha.1, both published 2026-05-26, 78 and 162 total downloads respectively, no repo link. sqlx issue #2674 remains open. Plan on rewriting your data-access layer, not adapting it.

**Self-hosting.** Real but rough. `tursodb ./server.db --sync-server 0.0.0.0:8080`. The docs state: *"No auth token is needed for the local server."* More telling: the server's own source, `cli/sync_server.mdx`, is an **LLM generation prompt** (`<Code model="anthropic/claude-opus-4-5" …>`) asking for a *"simple implementation of sync server"* that *"must process one request at the time."* It is a dev/test server, not production infrastructure. The protocol itself is small — two endpoints, `/v2/pipeline` (SQL over HTTP) and `/pull-updates` (protobuf) — so implementing it in your axum server is conceivable, but the published spec page (`docs.turso.tech/sync/spec`) 404s for me.

**Pricing** (turso.tech/pricing, no date shown): Free — 100 DBs, 5GB storage, 500M rows read, 10M rows written, 3GB syncs/mo. Developer $4.99/mo. Scaler $24.92/mo (24GB, 24GB syncs). Pro $416.58/mo. Model your cost carefully: a per-user-database desktop app means *you* pay for every user's storage and sync bytes, and the Free→Developer→Scaler steps are small relative to "every user has a notes database."

**Sources:** [turso.tech/blog/sync-benchmark](https://turso.tech/blog/sync-benchmark) · [turso.tech/blog/turso-offline-sync-public-beta](https://turso.tech/blog/turso-offline-sync-public-beta) · [docs.turso.tech/sync/conflict-resolution](https://docs.turso.tech/sync/conflict-resolution) · [docs.turso.tech/sdk/rust/quickstart](https://docs.turso.tech/sdk/rust/quickstart) · [turso.tech/pricing](https://turso.tech/pricing) · [github.com/tursodatabase/turso](https://github.com/tursodatabase/turso) · [github.com/launchbadge/sqlx/issues/2674](https://github.com/launchbadge/sqlx/issues/2674) · [crates.io/crates/sqlx-turso](https://crates.io/crates/sqlx-turso)

---

## 4. PowerSync

**Architecture.** Requires a server-side source of truth: Postgres, MongoDB, MySQL or SQL Server. The PowerSync Service tails the upstream DB's replication stream, evaluates **Sync Rules / Sync Streams** (SQL queries in YAML, parameterized by JWT claims) to compute each client's data set, and streams it down. Client writes go into a local CRUD queue and are uploaded through a `BackendConnector` you implement — i.e. **your** API applies them to Postgres. So the conflict model is **server-authoritative**: whatever your backend does when it applies a write is the resolution policy.

**The local schema question — this is the crux, and the answer is better than expected.** By default PowerSync owns local storage: data lives in `ps_data__<table>` (and `ps_data_local__<table>` for local-only) with a JSON `data` column, and your "tables" are SQLite **views** that `json_extract` each column. That would be incompatible with 77 FK constraints and your index strategy. But **Raw Tables** lifts this: you `CREATE TABLE` yourself before connecting, supply put/delete statements mapping the schemaless protocol onto your columns, and attach CRUD triggers. Docs list support in JS, Dart, Kotlin, Swift and **Rust 0.0.4+**; not .NET. Caveats: FKs need `DEFERRABLE INITIALLY DEFERRED` and careful sync-priority coordination, you own migrations, and Raw Tables don't work with JS "High Performance Diffs."

**Rust client.** `powersync` 0.0.7 (Apache-2.0, 2026-08-03) from `powersync-ja/powersync-native` (20★, pushed 2026-09-16). Exposes `PowerSyncDatabase`, `ConnectionPool` (with a raw leased-connection escape hatch), `SyncStream`, `CrudTransaction`/`CrudEntry`, `BackendConnector`. docs.rs reports 61.61% documented. README: *"currently in an alpha state, intended for external testing and public feedback."*

**E2EE.** Documented. Full local-DB-at-rest encryption is per-SDK (SQLCipher / SQLite3MultipleCiphers etc. — I did **not** see Rust in that list). True E2EE is done by syncing already-encrypted payloads and decrypting into Raw Tables. Obvious tradeoff: Sync Rules can't filter on encrypted columns, so your partitioning keys must stay plaintext.

**Self-host & license.** `powersync-ja/powersync-service` (382★, pushed 2026-09-16) is **FSL-1.1-ALv2** — "any purpose other than a Competing Use," converting to Apache-2.0. Shipping Lattice is a Permitted Purpose. Docker image; no dashboard when self-hosted. Enterprise Self-Hosted adds custom write checkpoints, early CVE patches, hardened images.

**Pricing.** Free $0 (2GB synced/mo, 500MB hosted, 50 peak concurrent clients, deactivates after 1 week idle). Pro from $49/mo (30GB/mo then $1/GB; 1,000 concurrent clients then $30/1,000). Team from $599/mo. Enterprise custom. Open Edition self-host is free.

**Burden for Lattice:** you must stand up and operate Postgres (you have one) + PowerSync Service, define Sync Streams for your 15 tables, write a `BackendConnector` against your axum API, and write Raw Table definitions + triggers. Non-trivial but the sync correctness problem is bought, not built.

**Sources:** [powersync.com/pricing](https://www.powersync.com/pricing) · [docs.powersync.com/architecture/client-architecture](https://docs.powersync.com/architecture/client-architecture) · [docs.powersync.com/client-sdks/advanced/raw-tables](https://docs.powersync.com/client-sdks/advanced/raw-tables) · [docs.powersync.com/usage/use-case-examples/data-encryption](https://docs.powersync.com/usage/use-case-examples/data-encryption) · [github.com/powersync-ja/powersync-native](https://github.com/powersync-ja/powersync-native) · [github.com/powersync-ja/powersync-service](https://github.com/powersync-ja/powersync-service) · [crates.io/crates/powersync](https://crates.io/crates/powersync)

---

## 5. ElectricSQL

**Verdict: not applicable.** Rule it out early and don't spend a week on it.

The 2024 pivot is complete and permanent. The old "Satellite" bidirectional CRDT client is gone. Current docs state flatly: *"Electric does read-path sync. It syncs data out-of Postgres, into local apps and services. Electric does not do write-path sync."*

Writes are your problem, and the docs name four patterns you'd implement yourself: online writes (REST), optimistic state, shared persistent optimistic state, and through-the-database sync (PGlite + shadow tables + triggers). Every one of those assumes a web/TS client.

**Rust client: none.** Official clients are **TypeScript and Elixir** only. The sync service is Elixir. The HTTP Shape API is admittedly simple and language-agnostic — `GET /v1/shape` with `table`, `offset` (`-1` for full history), `handle`, `live` (long-poll) or `live_sse`, returning a shape log of `insert`/`update`/`delete` plus `up-to-date`/`must-refetch` control messages, with CDN-cacheable responses — and the docs say the pattern is "simple enough" to implement yourself. But you'd be writing a Rust Shape client *and* the entire write path *and* the SQLite materialization, having gained only the Postgres change-feed.

Also note two signals: the repo description is now **"The agent platform built on sync"** and `electric-sql.com` now 301s to **`electric.ax`** — the company is repositioning toward AI agents. Apache-2.0, 10.4k★, pushed 2026-09-09 — healthy, just aimed elsewhere.

**Sources:** [electric.ax/docs/guides/writes](https://electric.ax/docs/guides/writes) · [electric.ax/docs/api/http](https://electric.ax/docs/api/http) · [github.com/electric-sql/electric](https://github.com/electric-sql/electric)

---

## 6. cr-sqlite / vlcn, SQLSync, Graft, SQLite sessions, and Anki

### cr-sqlite / vlcn — conceptually perfect, practically blocked

**Maintenance: worse than the repo's "pushed 2026-08-10" suggests.** Last tagged release is **v0.16.3, 2024-01-17** — 20 months ago. The commit log jumps from **2024-06-29 straight to 2026-08-02**, and the 2026 commits are community build-plumbing PRs from outside contributors (`fix ios simulator build`, `align android loadable to 16 kb page size`, `build windows arm64 loadable`, `Fix macos headerpad`). That is a project kept alive by downstream consumers patching build targets, not one under active design. 3.8k★, MIT.

**How it works.** A runtime-loadable extension. `SELECT crsql_as_crr('table')` upgrades a table to a "conflict-free replicated relation"; the `crsql_changes` virtual table is both your change-feed (`SELECT … WHERE db_version > x AND site_id != peer`) and your apply path (`INSERT INTO crsql_changes VALUES (…)`). Columns carry `col_version`, `db_version`, `site_id`, `cl` (causal length, for delete/undelete), `seq`. Schema changes go through `crsql_begin_alter` / `crsql_commit_alter`. Published perf note: inserts ~2.5× slower than plain SQLite, reads unchanged.

**Loading from sqlx: works.** sqlx 0.8's `SqliteConnectOptions` has `unsafe fn extension(name)` and `extension_with_entrypoint(name, entry)` behind the `load-extension` feature. You'd enable that feature and ship the `crsqlite` dylib per platform.

**Why it's blocked for Lattice — from the extension's own source** (`core/rs/core/src/tableinfo.rs:914-995`), `crsql_as_crr` rejects a table if it has:
- any **unique index other than the primary key** — *"This is not allowed for CRRs"*
- **no primary key, or a nullable one** (including any nullable part of a composite PK)
- **auto-increment primary keys** — *"two concurrent nodes will assign unrelated rows the same primary key"*
- **checked foreign key constraints** — *"CRRs may have foreign keys but must not have checked foreign key constraints as they can be violated"*
- any **NOT NULL column without a DEFAULT VALUE**

Against your schema that means: turn off `PRAGMA foreign_keys = ON` (and delete the test asserting it) across 77 FK declarations; migrate the one AUTOINCREMENT table; drop/rework non-PK unique indexes; and add a DEFAULT to roughly a hundred NOT NULL columns. Mechanical, but it's a wholesale change to your data-integrity posture, permanently, in exchange for an extension whose last release predates half your codebase.

**Server side:** there is no production server. The examples are Fly.io demos and a WIP editor (`tantaman/strut`).

**Sources:** [github.com/vlcn-io/cr-sqlite](https://github.com/vlcn-io/cr-sqlite) · [docs.rs/sqlx SqliteConnectOptions](https://docs.rs/sqlx/latest/sqlx/sqlite/struct.SqliteConnectOptions.html)

### SQLSync → Graft

`orbitinghail/sqlsync` (2.9k★, Apache-2.0) last commit **2025-11-19**. Architecture is reducer-based: mutations are deterministic reducers compiled to WASM, applied optimistically locally, then rebased against a server-authoritative timeline. Web-first; not a natural fit for a Rust desktop backend.

Carl Sverre moved to **Graft** (1.5k★, MIT/Apache, pushed 2026-09-15, last release v0.2.1 2025-12-04) — a transactional storage engine doing lazy, partial, page-granularity replication on object storage, usable as a Rust crate *or* a SQLite extension. Genuinely interesting technology, tested with Antithesis. **But the semantics are wrong for you:** per the internals docs, commits are strictly serialized under optimistic concurrency, and on conflict Graft *"return[s] an error requiring the transaction to abort and retry"* — no row-level merge. Two devices editing different notes offline would produce a rejected commit, not a merge. Plus: *"Graft should be considered Alpha quality software. Thus, please contact @carlsverre before using it in production."*

**Sources:** [github.com/orbitinghail/sqlsync](https://github.com/orbitinghail/sqlsync) · [github.com/orbitinghail/graft](https://github.com/orbitinghail/graft) · [graft.rs/docs/internals](https://graft.rs/docs/internals/)

### SQLite session extension (the built-in option nobody mentions)

Worth knowing about as a building block. `sqlite3session_*` records INSERT/UPDATE/DELETE into binary **changesets** (full before+after values) or **patchsets** (compact; PK + new values), applied elsewhere via `sqlite3changeset_apply` with an `xConflict` callback. Changes are coalesced within a session; changesets can be inverted.

Limits that matter: requires a declared PRIMARY KEY; **silently ignores rows with NULL in PK columns**; no virtual-table support (so your FTS tables are invisible); requires building SQLite with `-DSQLITE_ENABLE_SESSION -DSQLITE_ENABLE_PREUPDATE_HOOK` (rusqlite has a `session` feature; **sqlx does not expose this**, which is likely disqualifying without vendoring); and no schema-migration story — schemas must match.

It gives you change *capture* and *conflict detection* for free, but no ordering, no causality, no tombstone GC. If you roll your own, it's a plausible alternative to hand-written triggers — but only if you're willing to move to rusqlite for the sync path.

**Source:** [sqlite.org/sessionintro.html](https://www.sqlite.org/sessionintro.html)

### Anki — the reference design you should actually read

This is the closest existing thing to "Lattice with sync," and it's AGPL-3 Rust you can read.

**The model** (`rslib/src/sync/collection/normal.rs`): every syncable object carries a **USN** (update sequence number). Locally-modified objects get `usn = -1` (`pending_usn`); the server assigns real USNs. `ClientSyncState` tracks `usn_at_last_sync`, `server_usn`, `pending_usn`, `local_is_newer`, and `server_media_usn`. A meta exchange at the start decides between three outcomes:

```
SyncActionRequired::NoChanges
SyncActionRequired::NormalSyncRequired
SyncActionRequired::FullSyncRequired { upload_ok, download_ok }
```

That third variant is the design's key pragmatic choice: **when things diverge too far — or the schema version changed — Anki does not merge. It forces a one-way full collection upload or download, and the user picks a direction.** Deletions are handled by explicit tombstone tables (`collection/graves.rs`), the payload moves in `chunks.rs`, and **media sync is an entirely separate protocol** (`sync/media/`) — the same split you'd want for your markdown vault. There is a hard payload size limit (`MAXIMUM_SYNC_PAYLOAD_BYTES_UNCOMPRESSED`).

**Self-hosted server:** shipped in the Rust binary since 2.1.66 (also `--syncserver` in the Python build since 2.1.57, plus a community Docker image). Auth is env vars `SYNC_USER1="user:pass"` (optionally PHC-hashed with `PASSWORDS_HASHED=1`). Stores its own copy of collection + media, default `~/.syncserver`, which *must not* be your normal data folder. It serves **plain HTTP** — docs recommend a VPN or HTTPS reverse proxy. Warning worth internalizing: *"Newer clients may depend on changes to the sync protocol, so syncing may stop working if you update your Anki clients without also updating the server."*

**What to borrow:** the USN scheme, explicit graves tables, the meta-check-then-branch flow, separating media/file sync from row sync, and — above all — the willingness to fall back to a forced full sync instead of trying to merge every edge case. That single decision is probably worth 2 months of engineering to you.

**Sources:** [github.com/ankitects/anki — rslib/src/sync](https://github.com/ankitects/anki/tree/main/rslib/src/sync) · [docs.ankiweb.net/sync-server.html](https://docs.ankiweb.net/sync-server.html)

---

## 7. CRDT libraries from Rust (Automerge, Loro, yrs)

None of these is a sync *engine* — they're merge libraries. You still write the transport, the server, auth and persistence. What you buy is correct concurrent merging of note text.

### Automerge
`automerge` **0.11.0**, MIT, 2026-08-12, 591k downloads; repo 6.6k★ pushed 2026-09-15. Active: July 2026 shipped "Hexane v1," reported as a **2–9× document performance improvement**, and August 2026 removed Hexane types from the public API to insulate callers from storage-engine churn. JS side is Automerge 3.4.1.

**Rust repo/sync layer is mid-transition — this is the risk.** `automerge-repo` (crates.io 0.3.0, **last publish 2025-10-03**; repo 78★, last push 2025-10-03) has effectively been superseded by **`samod`** (0.13.0, 2026-08-12, `alexjg/samod`, MIT) — wire-compatible with JS automerge-repo. But samod has **41 stars, 109 commits**, and its README says it is *"very much a work in progress"* with *"probably lots of things which are broken,"* advising against production use.

**E2EE: research, not product.** Ink & Switch's **Keyhive** (`inkandswitch/keyhive`, 248★, Apache-2.0, pushed 2026-09-15) provides `beelay-core` ("auth-enabled sync over end-to-end encrypted data") and `keyhive_core`. The README is unambiguous: **pre-alpha**, *"Expect there to be bugs, inconsistencies, and unstable APIs,"* *"has not been through a security audit,"* **"DO NOT use this release in production applications."** The August 2026 Automerge post notes the ARK API (`automerge-repo-keyhive`) now has docs and a Bluesky/ATProto prototype. Do not plan a 2026 product launch on this. The Automerge blog also solicits sponsorship — read that as you like.

### Loro
`loro` **1.16.0**, MIT, 2026-09-06, 674k downloads; repo 6.1k★, pushed 2026-09-10, releases roughly weekly (1.15.1 → 1.16.0 → 1.16.1 within two weeks). Rust-first with JS (WASM) and Swift bindings.

Feature fit for a notes app is the best of the three: rich text CRDT, **MovableTree** (O(1) clone, real move semantics — ideal for note hierarchies/outlining), MovableList, LWW Map, **shallow snapshots** (git-shallow-clone analogue, which is your answer to unbounded history growth), time travel, and an ephemeral/awareness store.

Two caveats. **No official sync server or hosted product** — the README describes export/import of update and snapshot byte ranges and leaves transport to you (fine for you: your axum server relays opaque bytes, which is also how you get E2EE). And **bus factor**: contributor stats are `zxch3n` 1,631 commits, `Leeeon233` 351, then bots. Excellent velocity, concentrated ownership.

### yrs / y-sweet
`yrs` **0.27.4**, MIT, 2026-08-22, **3.1M downloads** — the most-used of the three by a wide margin (largely via bindings). Repo `y-crdt/y-crdt` 2.2k★, pushed 2026-09-09.

**y-sweet is the concern.** Repo moved to `jamsocket/y-sweet` (1.0k★); last commit **2025-12-04**, and `y-sweet-core` on crates.io last published **2025-09-16 (v0.9.1)** — a year ago. It's Rust, S3-backed, horizontally scalable, MIT-ish (GitHub reports NOASSERTION). It works, but it isn't moving. Yjs's ecosystem is also web-first (y-websocket, Hocuspocus are TS), so from Rust you'd be running a server whose community lives in JavaScript.

### The pattern, and its costs
All three point at the same architecture: **the CRDT binary is the source of truth; SQLite is a derived read model you rebuild on change.** Concretely for Lattice: keep `note_crdt(note_id TEXT PRIMARY KEY, doc BLOB, version BLOB)` alongside your existing `documents` table, and on every merge write the flattened text back into `documents.content` so FTS, embeddings and every existing query keep working untouched.

Be clear-eyed about what it costs: you cannot SQL-query inside a CRDT blob; you cannot do row-level authorization server-side (the server sees opaque bytes — which is the E2EE benefit and the authz problem, simultaneously); metadata and tombstones grow with edit history (Loro's shallow snapshots and Automerge's compaction are the mitigations); loading many large binary docs at startup needs care; and migrating a CRDT's internal schema across app versions is genuinely hard. Finally: **a document CRDT is a bad fit for your relational tables.** Tags, favorites, deck review history — these are rows, not documents. Don't force them into a CRDT; use the op-log for those.

**Sources:** [automerge.org/blog/2026-august](https://automerge.org/blog/2026-august/) · [github.com/inkandswitch/keyhive](https://github.com/inkandswitch/keyhive/blob/main/README.md) · [github.com/alexjg/samod](https://github.com/alexjg/samod) · [github.com/loro-dev/loro](https://github.com/loro-dev/loro) · [github.com/jamsocket/y-sweet](https://github.com/jamsocket/y-sweet) · crates.io API for all version data

---

## 8. Ditto and the TypeScript-only field

### Ditto
A real, current Rust SDK exists — and it's the only commercial P2P SDK where that's true. `dittolive-ditto` on crates.io: stable **5.1.0 published 2026-08-19**, preview 5.2.0-preview.1 published 2026-09-16 (today), ~703k all-time downloads, license **"non-standard"** (proprietary). Docs cover cross-compilation and `.so`/`.dylib`/`.dll` targets, and recommend pinning with `"=x.y.z"`.

Product: CRDT-based peer-to-peer mesh over Bluetooth LE, P2P WiFi and LAN, with an optional cloud "Big Peer," queried via their own DQL language over a document model (their own store, not your SQLite).

**Pricing is contact-sales.** Free tier: 10 cloud device connections, 2GB storage, no SLA. Pro: 1,000+ connections, 50GB, 99% SLA — price on request. Enterprise: custom, and **self-managed / BYOC deployment is Enterprise-only**. No E2EE claim on the pricing page.

**Assessment:** the mesh/BLE capability is genuinely differentiated and irrelevant to you — Lattice is a desktop app syncing over the internet. You'd take a proprietary dependency, an enterprise sales cycle, a data model that replaces your SQLite schema entirely, and self-hosting gated behind the top tier. Not a fit.

### No-Rust-path options (dismissed, with reasons)

- **Evolu** — TypeScript only; no Rust path. Worth reading for its design though: SQLite-based, *"end-to-end encrypted by default,"* mnemonic-derived keys, self-hosted or cloud relays, MIT, actively developed (commits 2026-09-10). It's the closest thing to "what you'd build" in the TS world, and its E2EE relay model is a good template for your own server.
- **Triplit** — **dormant.** Last commit 2025-09-11, last release 2025-07-31 (the "pushed 2026-01-19" timestamp is a stale branch, not development). AGPL-3.0, TS-only. Do not build on it.
- **Jazz** (`garden-co/jazz`) — very active (2026-09-16) but TypeScript-only, and the issue count (284 open vs 190 stars) suggests heavy churn. CoJSON CRDTs, E2EE claimed. No Rust path.
- **Rocicorp Zero** (`rocicorp/mono`, 3.4k★, pushed 2026-09-16) — TS-only, requires Postgres upstream, server-authoritative rebase. Root LICENSE is Apache-2.0; I found no per-package license override, but Rocicorp has historically used source-available terms, so verify `zero-cache`'s commercial terms directly if you ever reconsider. Replicache still lives in the same monorepo (last standalone release tags are from 2023).
- **Dexie Cloud / InstantDB / Convex / Fireproof** — all browser/JS-first with no Rust client and no path into an existing sqlx SQLite schema. Not applicable.
- **CouchDB / PouchDB replication** — an old, genuinely proven desktop sync protocol, and it's just HTTP + JSON with revision trees, so a Rust client is *writable*. I could **not verify** any maintained Rust CouchDB-replication crate in 2026 (search budget exhausted). Treat as "would be a from-scratch build," which makes it strictly worse than building your own protocol tuned to your schema.

**Sources:** [ditto.com/pricing](https://www.ditto.com/pricing) · [docs.ditto.live/sdk/latest/install-guides/rust](https://docs.ditto.live/sdk/latest/install-guides/rust) · [evolu.dev/docs](https://www.evolu.dev/docs) · GitHub API for all activity data

---

## 9. Roll-your-own: what a minimal op-log / per-row LWW sync actually needs

Since this is my #1 recommendation, here's the concrete checklist, grounded in the three best-documented precedents.

**Prior art, and what each teaches:**

- **Actual Budget / James Long** (2019-06-06, still the best write-up of this exact design). Per-column op messages in a `messages_crdt` table with `(timestamp, dataset, row, column, value)`. Timestamps are **hybrid logical clocks** serialized as a collatable string — wall clock + counter + node ID — so ordering is a string compare. Clock drift beyond ±1 minute is rejected. A **merkle trie** over message timestamps lets two peers compare one root hash and, on mismatch, binary-search to the divergence point instead of exchanging full logs. Data is stored twice (materialized table + op log) for ~16MB on large databases. His flagged pain points are the ones that will bite you: schema evolution is *"incredibly difficult,"* per-field ops make things like sort-order changes awkward, **bulk updates become impossible**, and the append-only log needs periodic **epoching**.
- **Linear** (talk 2023-06-29; the detailed public analysis is the reverse-engineering writeup). Server-authoritative with a monotonically increasing global **`lastSyncId`**. Local mutations apply optimistically **in memory only** — *"client-side operations will never directly modify the tables in the local database"* — and are written to persistent storage only when the server's delta packet confirms them. Unsent transactions queue in a `_transaction` table and replay on restart, with an acknowledged hazard: non-idempotent transactions can error on replay after a crash mid-request. Conflict resolution is implicit LWW with the server as sole arbiter. Bootstrapping is full or partial, with partial indexes tracking what's been fetched.
- **Anki** — see §6. USN, graves, forced full sync as the escape hatch.

**The minimum viable component list:**

1. **Stable row identity.** Done — you already use `id TEXT PRIMARY KEY`. Fix the one AUTOINCREMENT table.
2. **A causal clock.** HLC per device. `uhlc` 0.9.0 (2026-01-12, 6.1M downloads, EPL-2.0/Apache-2.0) is the mature Rust option and saves you writing one. Persist the clock across restarts, and reject peers with excessive drift.
3. **Change capture.** Either SQLite triggers writing to a local `_ops` table, or explicit emission in your repository layer. Given your clean architecture refactor, the repository layer is probably cleaner and testable — but triggers are harder to forget. Only instrument the ~15 syncable tables.
4. **Tombstones.** A `_graves(table, row_id, hlc)` table. Deletes must be ops, not row removals, or deleted rows resurrect from the other device. Anki does exactly this.
5. **Per-device checkpoints + server sequence.** You have this: `document_ops.seq BIGSERIAL` + `device_checkpoints.last_acked_seq`. Generalize it from documents to `(table, row_id, column)`.
6. **Idempotency.** You have this too: `UNIQUE (user_id, device_id, client_op_id)`.
7. **Conflict policy, written down per table.** Per-column LWW for notes metadata, tags, favorites. For **study review history**, LWW is wrong — reviews are an append-only event log, so union/merge them and recompute scheduling state; never overwrite. Decide this table-by-table before writing code.
8. **Compaction / epoching.** Ops must not grow forever. Plan a checkpoint scheme (server-side snapshot + op-log truncation below the minimum acked seq) from day one; retrofitting it is painful.
9. **Schema migration across versions.** The hardest part. Anki's answer — version the protocol, and force a full one-way sync when versions differ — is unglamorous and correct for a small team. Adopt it.
10. **The vault folder is a separate protocol.** Sync markdown files by content hash + LWW on mtime, entirely separately from row sync (exactly as Anki splits media from collection sync). Do not try to make one protocol do both.
11. **E2EE (the payoff).** Encrypt op payloads client-side with a key derived from the user's passphrase; the server stores `(user_id, device_id, seq, ciphertext)` and orders by `seq` without reading anything. This is simple *only* because you control the protocol — it's the single strongest argument for building rather than buying, given Lattice's privacy positioning.

**Sources:** [archive.jlongster.com/using-crdts-in-the-wild](https://archive.jlongster.com/using-crdts-in-the-wild) · [linear.app/blog/scaling-the-linear-sync-engine](https://linear.app/blog/scaling-the-linear-sync-engine) · [github.com/wzhudev/reverse-linear-sync-engine](https://github.com/wzhudev/reverse-linear-sync-engine) · [crates.io/crates/uhlc](https://crates.io/crates/uhlc)

---

## 10. Could not verify / open questions

**Flagged as unverified — do not treat these as facts:**

1. **Turso's sync protocol spec.** `docs.turso.tech/sync/spec` returns 404. The repo contains a generation directive for it (`cli/sync_server.mdx`) but the `turso-docs/sync/` directory has no `spec.mdx`. I could not confirm a stable, versioned, publicly documented protocol suitable for a third-party server implementation.
2. **Turso self-hosted auth.** Docs say "No auth token is needed for the local server." I found no documented auth mechanism for a self-hosted Turso sync server. Assume none exists until proven otherwise.
3. **Turso per-table sync filtering.** No include/exclude mechanism found in `sync/engine/src/`. I'm confident it's whole-database sync, but this is inference from source structure, not documentation.
4. **PowerSync Rust + encrypted local DB.** The at-rest encryption doc lists Dart/RN/JS/Node/Kotlin/Swift ciphers; **Rust is not listed**. Unknown whether the Rust SDK supports an encrypted local file.
5. **PowerSync Raw Tables maturity in Rust.** Docs say Rust 0.0.4+ and present the feature as production, but the Rust SDK itself is self-declared alpha. Those two statements are in tension. Verify with a spike before committing.
6. **ElectricSQL Cloud pricing** — not retrieved.
7. **Jazz pricing** — `jazz.tools/pricing` returns 404.
8. **Ditto's actual license text** — crates.io reports "non-standard"; I did not retrieve the terms. Also unverified: whether the Rust SDK requires a portal-issued App ID/license token at runtime, and whether Ditto offers E2EE.
9. **Rocicorp Zero's commercial license terms** — root LICENSE is Apache-2.0, no per-package override found, but I did not confirm `zero-cache`'s production terms.
10. **A maintained Rust CouchDB-replication crate in 2026** — not verified either way.
11. **cr-sqlite's official limitations documentation** — `vlcn.io/docs/cr-sqlite/{limitations,gotchas}` both 404'd. The constraint list in §6 comes from reading `core/rs/core/src/tableinfo.rs` directly, which is authoritative but may not be exhaustive.
12. **Loro's official docs** — `loro.dev/docs` is behind Cloudflare and blocked both WebFetch and curl. Loro claims rest on the GitHub README and crates.io/GitHub API data.
13. **Whether Matt Wonlaw has publicly stated cr-sqlite's status** — I could not check (search budget exhausted). The maintenance conclusion is inferred from release tags and commit-author patterns, which is strong evidence but not a statement from the maintainer.

**Stale-source warnings:** the Turso offline-sync beta post (2025-03-31) and its "data loss possible" / "conflict resolution not implemented" warnings are superseded by 2026 docs — don't quote them as current. Conversely, cr-sqlite's README and vlcn.io materials read as current but describe a project whose last release was January 2024.

**Methodology limitation:** my web-search budget ran out roughly two-thirds through. Version numbers, dates, download counts, licenses and repo activity all come from the crates.io and GitHub APIs and are reliable. Narrative claims about company direction and roadmaps rest on fewer sources than I'd like.

---

## Recommended next step

A one-week spike that costs little and de-risks everything: **split the SQLite file** into `lattice.db` (15 syncable tables) and `derived.db` (embeddings, FTS, caches), with `ATTACH` for cross-joins. This is required by option #3, makes option #2 cleaner, is pure upside for #1, and independently improves backup size and corruption blast radius. Do it before choosing an engine, not after.

---

The cr-sqlite/SQLSync/Anki agent has reported. It found several things that **correct** my report, plus two options I missed entirely. Revision below. (The PowerSync/ElectricSQL agent is still running.)

---

# Report Revision — corrections and additions

## A. Three corrections to what I sent

### A1. ⚠️ sqlx 0.8 cannot load SQLite extensions at all — I got this wrong

I wrote that sqlx 0.8 supports runtime extension loading behind a `load-extension` feature. That was an artifact of my fetching `docs.rs/sqlx/latest`, which now serves **0.9.x**. Checking the crates.io feature list per version:

- **sqlx 0.8.6** (2025-05-19) — SQLite features are `sqlite`, `sqlite-unbundled`, `sqlite-preupdate-hook`. **No extension-loading feature exists.**
- **sqlx 0.9.0** (2026-05-06) — adds `sqlite-load-extension`, enabling `SqliteConnectOptions::extension()` / `::extension_with_entrypoint()`. Pulled in by the `sqlite` umbrella feature, but *not* by `sqlite-unbundled`.

Lattice is on `sqlx = "0.8"` (`src-tauri/Cargo.toml:65`). So **any loadable-extension option (cr-sqlite, Graft's SQLite extension, sqlite-sync) requires a sqlx 0.9 upgrade first**, with its own breaking-change surface. Add that to the integration burden column for those rows.

The one thing I got right and is worth keeping: `libsqlite3-sys`'s bundled build passes `-DSQLITE_ENABLE_LOAD_EXTENSION=1` unconditionally and never sets `SQLITE_OMIT_LOAD_EXTENSION`, so the old "bundled SQLite blocks extensions" folklore genuinely doesn't apply. The blocker is sqlx's version, not SQLite's build.

### A2. cr-sqlite's foreign-key rule is stricter than I reported — and it's now a hard no

I read the error message (*"has checked foreign key constraints"*) and concluded you'd need to disable FK enforcement. The actual check is harsher: `SELECT count(*) FROM pragma_foreign_key_list('{table}')` must be **0**. That pragma enumerates every declared `REFERENCES` clause regardless of whether `PRAGMA foreign_keys` is on.

So it isn't "turn off enforcement" — it's **strip all 77 `REFERENCES` declarations** from your schema, including every `ON DELETE CASCADE`, and move referential integrity into application code. Combined with dropping all non-PK unique indexes (cr-sqlite has **no uniqueness story across replicas, by design**), that's a schema rewrite, not a migration.

### A3. cr-sqlite is abandoned — with the maintainer's own words, and a live fork I missed

The abandonment is now documented rather than inferred. Matt Wonlaw, in cr-sqlite issue #444 ("Is this project dead?"), **2024-12-30**:

> "I'm full time on https://zero.rocicorp.dev/ these days. When that project becomes more mature I'll have more bandwidth again but I don't see that happening for at least 1-2 years."

His GitHub bio reads "Partner @ rocicorp.dev" and his last 25 public events are all pushes to `rocicorp/mono`. In July 2026 he converted the vlcn docs site to static export and removed the dead fly.dev demo links — de-provisioning, not revival. Three automigration bug reports from Oct 2025 remain unanswered. `vlcn-io/js` (which contained the only websocket sync server) last shipped **2023-12-16**.

**But there is a maintained fork I didn't find: `superfly/cr-sqlite`** — pushed 2026-09-15 (yesterday), tagging `v0.18.0-v2-migration-alpha15`. It powers `superfly/corrosion` in production; Fly's Jerome Gravel-Niquet reported 7.5M rows across ~1,000 nodes, 300K ops/sec aggregate, p99 replication ~1s, tested with Antithesis.

Two reasons it still isn't for you. Its README states the fork *"expects the **application** to handle all bookkeeping — gap detection, seq tracking, and buffering — outside the extension"* (upstream's `crsql_tracked_peers` is gone). And it carries a genuine correctness trap, verbatim: *"Applications must treat a version as complete even when some seqs are missing… In the extreme case, all seqs in a db_version can be missing."* It's shaped for a gossip mesh of homogeneous server nodes, tagged `-alpha15`, with no third-party support commitment.

**Net effect on the ranking: cr-sqlite moves from "high burden, unmaintained" to "do not adopt."** Watch the Fly fork only if it stabilizes past alpha and restores peer bookkeeping.

---

## B. Two options I missed entirely

### B1. `sqliteai/sqlite-sync` — the most active CRDT SQLite extension in 2026, blocked by its license

`github.com/sqliteai/sqlite-sync`, 560★, only **2 open issues**, by Marco Bambini (SQLite Cloud). Releases 1.0.20 → 1.1.3 across May–September 2026 — genuinely the healthiest project in this category.

It is startlingly well-matched to Lattice on features: causal-length sets, delete-wins/add-wins/grow-only sets, and — the standout — **Block-Level LWW**, line-level merge for text/markdown columns, pitched explicitly at *"keeping markdown files and agent memory in sync"* so that concurrent edits to different sections of a document both survive. That is exactly the gap that per-row LWW leaves in a notes app. Backends include **self-hosted PostgreSQL**, which maps onto your existing axum+Postgres investment. Schema rules are much milder than cr-sqlite's: TEXT/UUIDv7 PKs, DEFAULTs on NOT NULL columns, FKs discouraged-but-not-banned.

**Then the license kills it.** Elastic License 2.0 (modified), and the free grant applies only when the software is *"incorporated into or used by an **open-source project** licensed under an OSI-approved open-source license."* Worse, there's an explicit **Network Layer Restriction** requiring a commercial license if you *"implement or use an alternative network transport, communication protocol, synchronization mechanism, replication mechanism, server protocol, network backend"* — "Network Layer" being defined expansively as anything used to sync or replicate to remote systems.

Pointing this extension at your own axum server is precisely what that clause forbids. The self-hosted-Postgres path is allowed, but only because it runs *their* network layer against *your* database — you'd be adopting SQLite Cloud's protocol and gateway wholesale.

**Verdict: a vendor conversation, not a library.** Worth one email if Block-Level LWW appeals, entered knowing it's a commercial relationship with a proprietary sync protocol — which sits awkwardly against Lattice's privacy positioning. (Also: no Rust binding advertised, and nobody appears to have loaded it from Rust.)

### B2. `orbitinghail/sqlite-delta` — read this first, before writing any more sync code

`github.com/orbitinghail/sqlite-delta`, Apache-2.0/MIT, created 2025-07-15, by Carl Sverre **in collaboration with Shapr3D** (a real desktop CAD product with your exact problem shape). README, verbatim:

> "`sqlite-delta` is a collection of tested change-data-capture (CDC) patterns for SQLite databases. **This repository is educational** — the patterns are designed to be studied, understood, and adapted for your specific use case rather than used directly in production."

Two patterns: **`trigger-lite`** (track changed rows since last diff via a monotonic global sequence number in a side table — structurally identical to Anki's USN and to your `document_ops.seq`) and **`three-phase`** (triple-buffer rows to enable delta compression with minimal data movement).

Permissively licensed, tested, co-designed with a shipping desktop app, and it describes exactly the trigger-based op-log you are already half-building. This is the highest-ROI hour in the entire research effort.

---

## C. Anki: much stronger detail, and one way Lattice *cannot* copy it

The agent read the source, and the design is better than I conveyed.

**It's axum.** `rslib/sync/Cargo.toml` declares bin `anki-sync-server`; `http_server/mod.rs` imports `axum::Router`, `axum::extract::DefaultBodyLimit`, `tokio::net::TcpListener`. You would be building the same thing on the same stack.

**The whole conflict model is three branches** of `SyncMeta::compared_to_remote`: equal `mod` → NoChanges; **different `scm` (schema-modified timestamp) → FullSyncRequired**; otherwise NormalSyncRequired. That's it.

**The full-sync lever is deliberately weaponized.** When a collection exceeds the payload limit, the server sets `meta.schema = TimestampMillis::now()` — faking a schema change to force a one-way sync — logging *"collection is too large, forcing one-way sync"*. One mechanism, reused as a general escape hatch.

**The sanity check is the single best idea to steal.** After applying everything but **before committing**, the client sends row counts for cards/notes/revlog/graves/notetypes/decks/deck_config. Server compares against its own. On mismatch: `rollback_trx()`, then `set_schema_modified()` — deliberately poisoning the local schema timestamp so the *next* sync is forced into a full sync. A handful of `SELECT COUNT(*)`s converts silent divergence into a visible, recoverable event. Perhaps 50 lines.

**Clock discipline:** if `|remote.current_time − local.current_time| > 300` seconds, abort with `ClockIncorrect`. Change *detection* never touches a clock (that's USN's job); clocks are used only for conflict *resolution*, behind that hard gate.

**Conflict resolution is whole-row LWW**, not per-column: accept the remote row unless the local row is pending *and* its `mtime` ≥ remote's. `merge_revlog` doesn't merge at all — review entries are immutable events, so it just appends. Fifteen years, millions of users, no CRDT anywhere.

**The one place Lattice cannot follow Anki:** its full sync ships the entire `.anki2` file (gzipped, 100MB default limit, written to a temp file, integrity-checked by actually opening it, then atomic-renamed). **Your embedding tables would dominate that payload.** Lattice's "full sync" must be a *logical* dump of the ~15 syncable tables, not a file copy. Design for that from the start — it's the main divergence from the reference design.

Two more details worth having: the protocol is **stateful across requests** (`ServerSyncState` retains session key, USNs, chunk IDs), so self-hosting can't scale horizontally without sticky sessions; and auth is env-var users (`SYNC_USER1="user:pass"`) with PBKDF2 over a **hardcoded fixed salt** — a reasonable starting shape, but use a per-user random salt. License is **AGPL-3.0-or-later**: read the design, don't copy the code.

---

## D. Also worth correcting

- **Graft is quieter than its timestamp suggests.** `pushed_at` 2026-09-15 is entirely `renovate[bot]` on dependency branches. **Last human commit on main: 2026-06-17**; newest human-filed issue 2026-02-13; last release v0.2.1, 2025-12-04. Combined with page-level replication being structurally incompatible with "sync 15 tables, never sync embeddings," it's out.
- **SQLite session extension, sharper:** rusqlite supports it (`session` feature), but that forces `buildtime_bindgen` → a libclang dependency in CI for macOS/Windows/Linux cross-builds. Budget for it. sqlx has no session feature in 0.8.6 *or* 0.9.0. Also a footgun I should have flagged: attaching a PK-less table isn't an error — it silently records nothing.
- **SQLSync** was superseded by its own author. Carl Sverre, March 2025: *"SQLSync replicates the entire log of changes to every client… poorly suited to the constraints of edge and browser environments."* The planned Graft integration hasn't happened.

---

## Revised bottom line

The ranking holds, with #1 substantially reinforced and the SQLite-extension tier now firmly excluded:

1. **Roll your own op-log** — strengthened. Anki proves the exact architecture (Rust + SQLite + axum) at enormous scale with *no CRDT anywhere*; `sqlite-delta` hands you tested CDC patterns under a permissive license. Add to the plan: the sanity check (§C), a USN/GSN instead of timestamps for change detection, tombstones exchanged before content, a logical (not file-level) full-sync fallback, and vault-file sync as a genuinely separate protocol.
2. **PowerSync + Raw Tables + Rust SDK** — unchanged as the "buy" hedge, pending the second agent's report.
3. **Turso sync** — unchanged, still contingent on splitting the database and accepting that its "remote encryption" ships your key to the server.
4. **cr-sqlite — now a hard no** (stricter FK rule, documented abandonment, sqlx 0.9 prerequisite). `sqliteai/sqlite-sync` is the healthiest extension but its Elastic License forbids your own server. Graft and SQLSync are out on architecture.

The through-line across every source the agent read: **Anki's sync is conceptually simple and operationally paranoid — the sophistication is all in failure handling, not merge algorithms.** A small team can build and maintain that. The one person who tried hardest to build a correct general-purpose CRDT layer for SQLite gave up and went to work on something else.

---

Both agents have now reported. The PowerSync/Electric findings materially change two sections. Final revision below.

---

# Report Revision 2 — PowerSync and ElectricSQL

## The finding that settles the ranking

Across all three "buy" candidates, **every one requires abandoning sqlx**:

- **PowerSync** owns the SQLite connection and hands you `rusqlite`. Docs, verbatim: *"The Rust SDK does not currently support any higher-level SQL libraries, but we're investigating support for Diesel and sqlx."* A GitHub issue search across `powersync-ja/powersync-native` for "sqlx" or "diesel" returns **0 results** — "investigating" has no tracking issue behind it.
- **Turso** replaces the engine itself; `sqlx-turso` is `0.1.0-alpha.1` with 78 downloads.
- **cr-sqlite** needs a sqlx 0.9 upgrade *and* a schema rewrite (Revision 1).

I framed PowerSync's Raw Tables as "keeps your schema." That's true of the *schema* and false of the *data layer*. Raw Tables preserve your columns, indexes and foreign keys; they do not let sqlx drive the connection. Migrating ~50 tables of sqlx queries to rusqlite is the dominant line item, not the alpha SDK.

**Only roll-your-own keeps sqlx.** That is now the strongest single argument for #1, and it wasn't visible in my first report.

---

## A. ElectricSQL — disqualified on vendor risk, not just architecture

I reported Electric as "healthy, just aimed elsewhere." That's wrong as of five weeks ago.

**Databricks acquired Electric on 2026-08-11.** From Electric's own announcement, verbatim:

> "**Electric Cloud is winding down. Cloud users will need to self-host or move to another provider.** We've contacted every existing cloud user directly."

Everything open-sourced stays open-source (Postgres Sync, PGlite, TanStack DB, Durable Streams) — but that's a *licensing* commitment, with no maintenance pledge. The team joins Neon inside Databricks to work on Lakebase. Supporting evidence: weekly commit counts over the last 12 weeks run `22, 11, 5, 8, 8, 5, 3, 5, 3, 12, 4, 1`, with only **27 commits repo-wide since the acquisition** and nothing pushed since 2026-09-09. Cloud pricing launched 2026-04-02 — four months before the wind-down.

Two corrections to details I got right-ish but shallow:

- **No Rust client, and the community attempt is dead.** `electric-sql-client` on crates.io: latest **0.2.3, published 2024-12-27**; repo has 0 stars, 26 commits, last push 2024-12-27. It predates the 1.5 wire protocol by 20 months. (A docs.rs summary claiming a "July 2026" release is simply wrong — the registry API is unambiguous.)
- **Don't be fooled by Rust in the monorepo.** `packages/durable-streams-rust` is substantial and real, but it's a *server* for Durable Streams — an opaque event-stream primitive for agent loops — not a Postgres Sync shape client.

Also worth knowing if anyone revisits this: the domain silently moved from `electric-sql.com` to `electric.ax` with no announcement, and generic searches still surface 2023-24 material describing bidirectional SQLite/CRDT "Satellite" sync. **That product does not exist.** I nearly walked into that trap myself.

---

## B. PowerSync — still the best "buy," but the burden is roughly double what I reported

### New and genuinely good
- **There is an official Tauri plugin**, which I missed: `tauri-plugin-powersync` **0.0.6**, first published 2026-03-26, 7,856 downloads, alpha announced 2026-03-31. Relevant constraint: *"Connecting to the PowerSync service is only possible from Rust. Calling `connect()` from JavaScript will throw"* — the right shape for Lattice.
- **Sync Streams went GA 2026-05-14**, superseding Sync Rules ("now considered legacy… will be deprecated eventually"). Excluding your ~35 embedding tables is simply not defining streams for them. Clean.
- **A public Jepsen test suite exists** (`nurturenature/jepsen-powersync`, active as of 2026-09-15), reporting causal consistency, strong convergence and atomicity in a no-fault environment. Sponsored but developed in public — a real credibility signal, and rare in this category.
- **Offline queue durability is excellent:** `ps_crud` rows are written by triggers *in the same transaction* as the data change, so the queue can never desync from local data.

### Corrections and new costs
1. **Raw Tables is "Experimental," not production.** On PowerSync's own feature-status ladder that means "proof of concept only." You'd be depending on an *experimental* feature inside an *alpha* SDK simultaneously.
2. **Self-hosting needs a second database you don't have.** Not just the Service + your Postgres — also a **bucket-storage database**; the official demo stack runs a **MongoDB 7.0 replica set**, and production guidance recommends 3 nodes. Full production topology: 1 replication container (only one can replicate at a time), 2+ API containers (~100 connections each), 3-node MongoDB, TLS load balancer, daily compact job.
3. **At-rest encryption: Rust is absent from the support matrix.** Dart, React Native, JS Web, Node, Kotlin and Swift all have documented cipher integrations. Rust and .NET do not. For a privacy-focused app you'd wire SQLCipher into PowerSync's rusqlite connection yourself, unsupported. (I flagged this as unverified; now confirmed.)
4. **The upload queue is a blocking FIFO with a sharp edge.** An unacknowledged mutation retries indefinitely — and because clients don't advance checkpoints while the queue is non-empty, **a stuck write also freezes incoming sync**. Mitigation is to return 2xx for validation rejections. The proper fix, *custom write checkpoints*, is **Team ($599/mo) or Enterprise only**.
5. **Licensing is cleaner than I said.** FSL-1.1-ALv2 explicitly permits use "for your internal use and access," and the Apache-2.0 conversion is a **rolling per-release two-year clock**, not one fixed date. Client SDKs (including `powersync-native`) are plain Apache-2.0 — only the service and CLI are FSL. Also: the 2024 "Open Edition is feature-equivalent with Cloud" claim is now outdated; Open Edition excludes custom write checkpoints, hardened images, full metrics and the dashboard.
6. **Status label is inconsistent.** Docs say "alpha"; the crate's own description says "**pre-alpha**… we offer no stability guarantees." Breaking changes land at nearly every 0.0.x bump. One example in the repo.

---

## Final ranking

**1. Roll your own op-log — now clearly ahead, not narrowly.** It is the only option that keeps sqlx, the only one that needs no Postgres *and* no MongoDB, and the only one where E2EE is a design choice rather than an unsupported workaround. Anki proves the architecture (Rust + SQLite + axum) with no CRDT anywhere; `orbitinghail/sqlite-delta` gives you permissively-licensed, Shapr3D-co-designed CDC patterns to start from. Steal Anki's sanity check, its USN scheme, and its forced-full-sync escape hatch — but remember your full-sync fallback must be a *logical* dump of the ~15 tables, since you can't ship a database file containing embeddings.

**2. PowerSync + Raw Tables + Tauri plugin.** Still the only credible buy, and the Tauri plugin is a real 2026 development. But price it honestly: operate Postgres **and** a MongoDB replica set, migrate the entire data layer sqlx → rusqlite, hand-roll Raw Tables plus their migrations and triggers, build at-rest encryption yourself, and accept alpha-SDK-plus-experimental-feature risk on a small team. Reasonable if you'd rather buy a Jepsen-tested consistency model than build one.

**3. Turso sync.** Unchanged, and now differentiated mainly by *which* rewrite you prefer: PowerSync costs you sqlx; Turso costs you the SQLite engine. Turso additionally has no per-table filtering (whole-DB sync), no E2EE (the key is sent to the server in a header), and a self-hosted server with no auth.

**Excluded:** ElectricSQL (no Rust client, no write path, Cloud shutting down), cr-sqlite (abandoned upstream; FK rule requires stripping all 77 `REFERENCES`), `sqliteai/sqlite-sync` (Elastic License forbids your own server), Graft and SQLSync (page-level replication / browser-shaped), Ditto (proprietary, enterprise sales), and the TypeScript-only field.

**One prototype worth a day before committing to #2:** whether an existing sqlx pool can coexist with PowerSync's rusqlite connection on the same WAL-mode file. Plausible, entirely undocumented, and it's the difference between a partial and a total rewrite of your data layer.