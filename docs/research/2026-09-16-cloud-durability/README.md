# Cloud durability for Lattice: backup first, sync later

Research date: 2026-09-16. Three parallel research passes (backup tooling, sync engines, accounts/hosting/compliance) are in the `raw-*.md` files next to this one, with source URLs, pricing, and every unverified claim flagged. This file is the synthesis and the recommendation.

## The problem, restated

The worry is durability: a dead drive today means the user's Lattice data is gone. That is a **backup** problem, not a **sync** problem. Backup is one-way, has no conflicts, needs no accounts, and needs no server. Sync is the expensive thing. Everything below is ordered so the durability worry is solved first at near-zero infrastructure, with sync as a later, optional, monetizable layer.

What a full backup has to cover, from the codebase:

| Data | Where it lives | Backup? |
|---|---|---|
| Notes, tags, favorites, decks, reviews, conversations, daily notes, settings | SQLite (`lattice.db`) | Yes |
| Text/image embeddings, cluster runs, model catalog cache | SQLite, same file | **No.** Re-derivable, and likely most of the file size |
| Imported source documents (PDF/DOCX) | `~/.lattice/files/{sha256}/name.ext` (content-addressed library) | Yes |
| Vault markdown | User's vault folder on disk | Yes (users may already sync it themselves) |
| Downloaded models | Model dir | No |

There is already a `features/backup` slice (create, restore, scheduled auto-backup, export repository). That is where all of this hangs.

## Recommendation in one paragraph

Ship an **encrypted snapshot archive written to a user-chosen folder** first. It takes roughly a week, costs nothing to run, and solves the dead-drive case for anyone with iCloud, Dropbox, OneDrive, Google Drive, a NAS, or a USB stick. Then add **direct upload to the user's own cloud storage** (S3-compatible first, then OneDrive, then Google Drive) so restores do not depend on a sync client having hydrated the file. Only after that, and only if there is a business reason, build **hosted end-to-end-encrypted backup or sync**. If sync ever happens, roll your own op-log protocol in the style of Anki; every off-the-shelf sync engine forces you off sqlx. Delete `src/api` now regardless.

## Phase 1: encrypted backup archive to a folder (do this now)

**What it is.** A scheduled job produces one timestamped `.lattice-backup` file: a consistent SQLite snapshot with embedding tables excluded, plus the files library and the vault, tarred, zstd-compressed, encrypted client-side, with a manifest. The user picks the destination with the native folder picker. If they pick their iCloud Drive or Dropbox folder, that cloud provider does the off-site part.

**Why first.** Zero servers, zero storage cost, zero data processing agreements, zero OAuth review queues, and it is the market norm (Bear, Joplin filesystem target, Anki `.colpkg`, Cryptomator). It directly answers "my drive died".

**How, concretely.**

- Snapshot with `VACUUM INTO` through the existing sqlx connection. It is transactional and is one of the three methods SQLite documents as safe on a live database. Do not try to add `rusqlite` for the backup API: sqlx 0.9 and rusqlite 0.40 pin incompatible `libsqlite3-sys` versions and Cargo refuses two crates linking `sqlite3`.
- Exclude embeddings by attaching a scratch database and copying only the non-derivable tables, or by dropping the embedding tables in a copy before vacuuming. Restore then triggers a re-embed.
- Snapshot to local scratch first, never directly into the synced folder.
- Encrypt with RustCrypto: `chacha20poly1305` 0.11 + `aead-stream` 0.6 (chunked STREAM, detects truncation and reordering) + `argon2` 0.6 + `zeroize`. All pure Rust, no C toolchain, so Tauri cross-builds stay painless. The 1.0-track RustCrypto releases landed mid-2026; older notes about `aead` 0.5 are stale.
- Do not use `age` as the container. Its spec forbids combining a passphrase stanza with any other recipient, which blocks the recovery-code design below.
- Key envelope: one random 256-bit data key wrapped in two slots, LUKS-style. Slot 0 is a passphrase through Argon2id (about 256 MiB, t=3, calibrated to ~1s on the user's machine, params stored in the header). Slot 1 is a 24-word BIP-39 recovery code, full entropy, wrapped through HKDF. Store an 8-byte key check value so the app can tell "wrong passphrase" from "corrupt file".
- Copy Apple's Advanced Data Protection UX: the feature cannot be turned on until the recovery code has been generated and confirmed by re-entering three random words. Offer "save to password manager" as a first-class button next to "print". Users who run a password manager are the ones who do not lose secrets.
- Write `.part`, fsync, rename. Never overwrite the previous archive; timestamped names with keep-N retention. Overwriting a large file means there is no valid backup during the re-upload.
- Skip content-defined chunking and restic-style dedup. SQLite rewrites fixed 4 KiB pages in place, so CDC buys nothing, and CDC is a documented fingerprinting side channel (arXiv:2504.02095, Percival attacking his own Tarsnap). If incremental ever matters, use fixed page-aligned blocks hashed with BLAKE3.
- Encryption cost is a rounding error. 200 MiB encrypts in tens of milliseconds and uploads in minutes. Optimize bytes uploaded, not cipher throughput.

**The restore hazard.** Cloud sync clients evict files. On macOS an evicted iCloud file reports its full size in `stat()` but `st_blocks == 0` and carries the `SF_DATALESS` flag; reading it blocks for the whole download with no progress and fails offline. Measured in the research: 5.6 seconds to read one byte of a 4 KB dataless file. OneDrive Files On-Demand and Dropbox online-only placeholders behave the same way. Before opening a backup for restore, probe for dataless or placeholder state and show a real "downloading" state rather than a hung spinner.

**The one hard rule.** The live `lattice.db`, `-wal`, and `-shm` must never sit inside a synced folder. SQLite's own docs tell app authors to block that. Anki and Obsidian document corruption from it. The current FAQ workaround ("store the database on Dropbox") should be retired once Phase 1 ships, and the app should warn if it detects its data directory inside a cloud folder.

**Effort.** About one focused week for one person: snapshot + exclusion, archive + crypto envelope, scheduler wiring, restore with placeholder detection, recovery-code UX. The crypto, archive, and detection pieces are mechanical and split cleanly across parallel agents.

## Phase 2: direct upload to the user's own cloud (next)

Same archive, pushed over an API instead of dropped in a folder. Removes the eviction hazard, gives real progress and resumable uploads, and still involves no server of ours.

**Order of backends**, driven by support burden (Joplin issue counts per backend: WebDAV 176, Nextcloud 98, OneDrive 86, Dropbox 81, S3 51 with 1 open) and by review gates:

1. **S3-compatible** with Cloudflare R2 and Backblaze B2 as named presets. No OAuth, no review queue, no user cap, free at single-user scale. Use the `object_store` crate (Apache Arrow governance, already handles R2 quirks). Set AWS checksum options to `WHEN_REQUIRED` or non-AWS providers reject uploads. Avoid `rust-s3`.
2. **OneDrive** via Microsoft Graph `Files.ReadWrite.AppFolder`. No admin consent, port ignored on the loopback redirect, upload fragments must be multiples of 320 KiB, and passing `fileSize` gives a free quota pre-check (507 if full).
3. **Google Drive** via `drive.file` only. It is classified non-sensitive, so it avoids the annual third-party CASA security assessment that restricted scopes require. Prefer a visible user folder over `appDataFolder`, which users can silently delete. Move the OAuth consent screen to production or refresh tokens expire in 7 days.
4. **Dropbox last.** Once 50 users link, you have two weeks to get production approval or linking freezes, and unlinking does not reset it. Redirect URIs must match the port exactly, so register fixed ports. The `Dropbox-API-Arg` header must escape non-ASCII, which serde_json does not do by default.
5. **Skip generic WebDAV.** It is a promise you cannot keep, and Nextcloud's chunked upload is proprietary anyway.

**OAuth mechanics for all of them.** Loopback `http://127.0.0.1:PORT` + PKCE, opened in the system browser via `tauri-plugin-opener`. Google has dropped custom URI schemes for desktop and bans embedded webviews outright. `tauri-plugin-oauth` 2.1.0 does the loopback listener and is small enough to vendor. Do not build on deep links: Tauri's Linux bundler currently emits a `.desktop` file without `%u`, so packaged builds silently drop the callback (tauri#15928, open). Store only the refresh token in the OS keyring; Windows Credential Manager caps a blob at 2,560 bytes. Upgrade `keyring` 3.6 to 4.x with the `v1` feature.

**Effort.** S3 backend about three days. Each OAuth backend about a week plus the provider's review clock.

## Phase 3: hosted E2EE backup or sync (optional, later, paid)

This is where accounts, a server, a bill, and compliance obligations appear. Build it only if you want a paid tier or want to serve users with no cloud account.

**Storage is not the cost.** For 10,000 users at 200 MB each, monthly object storage runs from about $14 (Backblaze B2) to about $100 (R2 or S3), and batching writes into a couple of packfiles per user per day drops R2 to about $33. Request pricing dominates, not gigabytes. Keep blobs out of Postgres, which costs 15 to 50 times more per GB.

**Minimal stack** for a one or two person team, about $40 to $60 per month at launch: WorkOS AuthKit for identity (free to 1M MAU, documents `127.0.0.1` loopback for native clients, standard OIDC so it is swappable), a small app instance plus managed Postgres with point-in-time recovery (Fly.io, DigitalOcean, or Crunchy Bridge; avoid Supabase for this because PITR is a $100/month add-on), R2 or B2 for blobs, a merchant-of-record for payments (Paddle or Stripe Managed Payments) so EU VAT is not your problem. The privacy-purist alternative to WorkOS is a self-issued magic-link plus device-token scheme in axum; the account model is thin enough that this is about a week plus a security review. Do not self-host Keycloak or authentik; their patch cadence is a treadmill.

**What has to change in `api-rust`.** Identity comes from a plaintext `x-user-id` header, so any client can be any user. The schema stores note `content` as plaintext and has a `conflicts.resolution_content` column, which means server-side merge on content. Both are incompatible with end-to-end encryption. **Decide E2EE before freezing the wire format**, because retrofitting it means migrating every user's data and breaking the protocol.

**Compliance reality.** E2EE reduces risk and breach-notification exposure but does not remove obligations. EDPB Guidelines 07/2020 give the worked example: a host storing encrypted blobs is still a processor. A US company serving EU users needs an Article 27 representative (about €150 to €1,500 per year). Hetzner's DPA is not concluded automatically by deploying; you must actively sign it. Stripe is an independent controller and must be disclosed as such. Only "bring your own storage" (Phases 1 and 2) plausibly keeps you outside the processor framework for note content, and even then the crash reporter can break that promise if it ever captures note text or file paths.

**If it becomes sync rather than backup.** Roll your own op-log protocol. The sync-engine survey found that PowerSync hands you a rusqlite connection (its docs say sqlx support is "being investigated", with no tracking issue), Turso replaces the SQLite engine, cr-sqlite is unmaintained upstream and needs the schema stripped of foreign keys, and ElectricSQL never had a Rust client and its cloud is winding down after the Databricks acquisition in August 2026. PowerSync is the only credible buy, but its Rust SDK is self-described pre-alpha, Raw Tables are "experimental", self-hosting needs Postgres plus a MongoDB replica set, and encrypted-at-rest is unsupported for Rust. The roll-your-own reference is Anki: Rust desktop, SQLite, axum sync server, per-row update sequence numbers, a sanity check, and a forced full-sync escape hatch, no CRDTs. Sync only the ~15 non-derivable tables; the full-sync fallback must be a logical dump, not the database file. The existing `api-rust` op-log scaffold is the right shape once auth and E2EE are fixed. Client-side effort dominates: change tracking on every synced repository, a push/pull loop with offline queueing, and a conflict strategy per table. Budget four to eight weeks for a first sync release, versus about one week for Phase 1.

## Immediate actions

1. Delete `src/api` and the leftover `python3` Debian dependency. Nothing in the shipped app uses either.
2. Build Phase 1 in `features/backup`. Design doc first (archive format, header layout, exclusion list), then fan the crypto envelope, archive writer, placeholder detector, and recovery-code UI out to parallel agents.
3. Add a startup warning if the app data directory is inside a detected cloud-sync folder, and retire the FAQ advice to put the database on Dropbox.
4. Get a Developer ID certificate. `signingIdentity: "-"` (ad-hoc) causes macOS Keychain re-prompts on every rebuild and blocks notarized distribution, which Phase 2 needs anyway.
5. Park `api-rust`. Do not extend it until the E2EE decision is made.

## Things the research could not verify

The raw reports list every open item. The ones that could change a decision: exact folder names under `~/Library/CloudStorage/` (inferred from community reports), Backblaze B2 support for conditional writes (a silent no-op would be worse than a rejection, test it), the cost of Google's CASA assessment (irrelevant if you stay on `drive.file`), and the Mac App Store external-purchase rules (moot if you ship a notarized DMG outside the store).
