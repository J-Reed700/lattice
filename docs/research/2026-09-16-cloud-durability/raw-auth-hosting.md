I have comprehensive coverage across all seven topics. Delivering the report now — the hosting and compliance agents are still running, but I covered both areas independently.

# Accounts / Auth + Hosting for Lattice Cloud Sync — Research Report

**Compiled 2026-09-16.** Sources are linked inline. Confidence markers: **[V]** = verified on a primary page this session · **[S]** = secondary source only · **[U]** = unverified / conflicting. A session-wide WebSearch cap was hit partway through, so later work was done via direct WebFetch and the GitHub API; that's why some items are marked [U] rather than confirmed.

---

## 1. Executive summary

### Recommended minimal stack (1–2 people)

| Layer | Pick | Cost/mo |
|---|---|---|
| Identity | **WorkOS AuthKit** — OIDC + PKCE public client, JWKS | **$0** (free to 1M MAU) |
| Login flow | **System browser → loopback `http://127.0.0.1:<random>`** + PKCE | $0 |
| Token storage | `keyring` 4.x, **refresh token only** | $0 |
| API | Existing **axum 0.8** service + `axum-jwt-auth` 0.7 JWKS middleware | — |
| App host | **Fly.io** shared-cpu-1x 512MB (fra/ams) | **~$3.32** |
| Postgres | **Neon Launch** (Frankfurt) — 7-day PITR | **~$20** |
| Blob storage | **Cloudflare R2** (zero egress) | **$0–2** |
| Email | **Resend** free tier (3k/mo) | **$0** |
| Payments | **Paddle or Stripe Managed Payments** (merchant of record) | % of revenue |
| Apple Developer Program | required for notarized macOS builds | **$8.25** ($99/yr) |
| GDPR Art 27 EU rep | required if US entity serving EU users | ~$10–25 **[U]** |

**≈ $45–60/month at launch**, plus payment processing as a percentage. Nothing here has a seat fee or a floor that scales with users until you're well past 10k.

### The seven things that actually matter

1. **WorkOS AuthKit is free to 1,000,000 MAU**, then $2,500 per additional 1M. **[V]** Passkeys, MFA, social, and Magic Auth are included. It supports **PKCE without a client secret**, and — critically for you — its docs carry an explicit exception: *"`http://127.0.0.1` allowed in production for native clients."* **[V]** That single line is why it beats Clerk and Supabase for a desktop app; most vendors don't document desktop at all.

2. **Use loopback, not a `lattice://` deep link.** Google's own docs now say *"Custom URI schemes are no longer supported due to the risk of app impersonation"*, while loopback remains the recommended desktop mechanism. **[V]** Apple refuses both — its redirect URI *"must use the HTTPS protocol, include a domain name, and can't contain an IP address or `localhost`."* **[V]** And Tauri's deep-link path is **currently broken on packaged Linux builds** — [tauri#15928](https://github.com/tauri-apps/tauri/issues/15928), open since 2026-08-28, the bundler omits `%u` from the `.desktop` Exec line so the callback URL is silently dropped. Verified still unfixed on `dev` today.

3. **You almost certainly don't need Sign in with Apple.** Current Guideline 4.8 **[V]** exempts apps where *"Your app exclusively uses your company's own account setup and sign-in systems."* Ship only your own email/passkey login and 4.8 never triggers. Ship outside the Mac App Store (notarized DMG) and App Review doesn't apply at all. The moment you add "Sign in with Google," you owe an equivalent privacy-preserving option.

4. **Passkeys can't run inside the Tauri webview on macOS.** [tauri#7926](https://github.com/tauri-apps/tauri/issues/7926) is still open and untriaged since Sept 2023 (last comment 2026-04-07). Windows 11 works; Linux lacks the API; the community `tauri-plugin-webauthn` explicitly lists **macOS: ✗**. Passkeys work fine *in the system browser* — another reason the browser-based flow is the right call.

5. **PITR is the biggest hidden cost discriminator in managed Postgres.** Supabase charges **$100/mo** for it on top of a $25 plan; Crunchy Bridge and Fly MPG include 10-day PITR free. **[V]** This flips Supabase from "obvious cheap choice" to most expensive.

6. **Cloudflare Workers is out for this codebase.** workers-rs supports axum, but *"no Tokio or async_std support"* and everything must compile to `wasm32-unknown-unknown` **[V]** — which your `sqlx` + tokio data layer will not. Choosing Workers means rewriting persistence against D1/Hyperdrive.

7. **E2EE reduces risk more than it reduces obligations** — but it buys one concrete thing: **Art 34(3)(a)** exempts you from notifying *data subjects* of a breach where measures *"render the personal data unintelligible… such as encryption."* **[V]** You still owe the 72-hour regulator notification, and account email, billing, IP, device IDs and timestamps remain plainly personal data.

---

## 2. Findings specific to the Lattice codebase

These are grounded in the repo, not the web, and several change the plan.

**The existing sync API has no authentication and trusts a client-supplied header.** `api-rust/src/http/routes/sync.rs:87` reads `x-user-id`, parses it as `i64`, and uses it as the tenant key. Any client can set it to any integer and read any user's data. It's clearly placeholder code, but it's the exact seam where auth must land and it must not ship.

**The sync schema stores plaintext notes.** `api-rust/migrations/0001_sync_init.sql` defines `document_heads` and `document_ops` with `title TEXT`, `content TEXT`, `path TEXT`. There are **no crypto dependencies anywhere** in either `Cargo.toml`. The current design is a plaintext server-side note store — the opposite of the E2EE posture the privacy positioning implies.

**E2EE vs. the current conflict model is a real design tension.** The schema has `conflicts.resolution_content TEXT`, implying server-side merge. That's incompatible with E2EE, where the server sees only opaque ciphertext. **Decide on E2EE before you freeze the wire format**, because it changes the API shape, kills server-side merge, and determines the recovery UX.

**There is already a home-rolled auth system — in Python.** `src/api/src/auth/` (FastAPI) has JWT, bcrypt, MFA rate limiting, and a token blacklist. `jwt.py` signs with a **shared secret** (`settings.jwt_secret_key`, separate access/refresh secrets) — i.e. HS256, no JWKS, no asymmetric keys. The Rust rewrite hasn't carried any of it over. So the real decision is: **port home-rolled auth to Rust, or adopt an IdP during the rewrite.** Adopting an IdP means moving from HS256 shared-secret to RS256/ES256 + JWKS.

**JWKS crate compatibility.** Your `api-rust` is on **axum 0.8**. The most-cited crate, `jwt-authorizer` 0.15.0, depends on **axum ^0.7** and was last published **2024-08-27** **[V]** — it will not work. Use **`axum-jwt-auth` 0.7.0** (published 2026-05-19, axum ^0.8, jsonwebtoken ^10, remote JWKS with automatic caching and hourly background refresh) **[V]**, or `axum-jwks` 0.12.0 (2025-06-06, axum ^0.8) **[V]**.

**macOS signing is ad-hoc.** `tauri.conf.json` has `"signingIdentity": "-"` and `"entitlements": null`. Ad-hoc signing means the macOS Keychain designated requirement changes between builds, which triggers the "app wants to access keychain" prompt — and blocks notarization entirely. You need a real Developer ID identity before shipping keyring-stored refresh tokens.

**Good news:** `tauri-plugin-shell` resolves to **2.3.4** in `Cargo.lock`, past the 2.2.1 fix for **CVE-2025-31477** (High, scope validation allowed `file://`/`smb://` through `open`). You're not exposed — though Tauri has deprecated `shell.open` in favour of `tauri-plugin-opener` (2.5.5, 2026-08-31). Current: `tauri = "=2.9.5"`, `keyring = "3.6.3"` (latest 4.2.0), identifier `tech.lattice.app`, plugins: only `shell` — no deep-link, no single-instance, no opener.

---

## 3. Identity options

### Hosted — 2026 pricing at 1k / 10k / 100k MAU

| Provider | 1k | 10k | 100k | Free tier | Desktop/PKCE | Passkeys |
|---|---|---|---|---|---|---|
| **WorkOS AuthKit** | **$0** | **$0** | **$0** | **1M MAU** | ✅ PKCE, `127.0.0.1` allowed in prod **[V]** | ✅ free |
| **Clerk** | $0 | $0 | ~$25 | **50,000 MRU** | Docs mention only React Native/Expo **[V]** | ✅ |
| **Supabase Auth** | $0–25 | $25 | $25 | 50k MAU (Free), 100k (Pro) | PKCE; code valid 5 min, single-use **[V]** | ✅ |
| **Firebase / GCIP** | $0 | $0 | ~$275 | 50k MAU | via browser | ✅ |
| **Kinde** | $0 | $25 | ~$1,590 | 10,500 MAU | — | Pro+ only |
| **Stytch** | $0 | $0 | Unpublished **[U]** | 10,000 MAU | — | Not on pricing page |
| **Auth0** | **$70–240** | **$700–1,600** | Contact sales | 25,000 MAU | ✅ mature | ✅ all tiers |

**Clerk** is $25/mo Pro with **50,000 MRUs included**, then $0.02 each. **[V]** Note the metric: *"A user only counts as retained if they return to your app at least 24 hours after signing up"* — materially cheaper than MAU billing. Enterprise connections $75/mo each beyond the first; B2B add-on $100/mo.

**Supabase**: Free $0 / Pro $25 / Team $599; 50k / 100k / 100k MAU included, **$0.00325 per MAU** overage; SAML SSO priced separately at $0.015/MAU after 50. **[V]** Since May 2025 new projects get **asymmetric JWT signing keys by default** (RS256, optionally ECC/Ed25519) with a JWKS endpoint at `/auth/v1/jwks` **[S]** — so Rust-side local verification is clean.

**Firebase/GCIP**: free to 50k MAU, then $0.0055/MAU stepping down to $0.0025 at volume; SAML/OIDC $0.015/MAU after 50; phone auth never free. **[S]**

**Auth0 is the outlier** — $70/mo at 1k B2C, $700 at 10k, and B2C plans simply don't offer 100k self-serve. **[V]** Its 2023 repricing raised B2C Essentials overage from $0.023 to $0.07/MAU, a 300% jump. **[S]** Avoid for consumer scale.

### Self-hosted

| | License | Entry cost | Ops reality |
|---|---|---|---|
| **Keycloak** | Apache-2.0 **[V]** | VPS | 4 GB min RAM, 8 GB production; 60–90s JVM startup **[S]**. Releases every 2–4 weeks: 26.6.2 → 26.7.3 in ~3.5 months **[V]** |
| **Zitadel** | **AGPL-3.0 from v3** **[V]** | VPS | App itself ~512 MB RAM, but needs 4 CPU cores for password hashing; recommends **3-node HA cluster, 4 CPU/16 GB each** **[V]** |
| **Authentik** | Mixed (MIT core + enterprise) **[V]** | Free self-host; $5/user/mo enterprise | No hosted option: *"We do not currently provide a hosted version"* **[V]**. Three branches patched simultaneously 2026-09-09 **[V]** |
| **Ory** | Apache-2.0 core | Network: **$770/yr** Production, $0.14/aDAU **[V]** | Self-hosting Kratos+Hydra is multi-service |

**Zitadel's v2→v3 license change from Apache-2.0 to AGPL-3.0 is a genuine flag** for a commercial product. **[S]** Running an unmodified instance for your own service is generally fine, but it constrains what you can fork.

**The honest read on self-hosting:** the release cadences above *are* the argument against it. A 1–2 person team that self-hosts an IdP signs up for a security-patch treadmill on the component whose failure logs everyone out. Skip it.

### Rolling your own in axum

The Rust ecosystem is genuinely good here:

- **`jsonwebtoken` 11.0.0** (2026-07-24), 189M downloads **[V]** — excellent
- **`webauthn-rs` 0.5.5** (2026-04-30), 6.8M downloads **[V]** — production-grade (powers Kanidm)
- **`openidconnect` 4.0.1** (2025-07-06) **[V]**, **`oauth2` 5.0.0** (2025-01-21) **[V]**

For an E2EE backup service the account is genuinely thin: email → device token → paid-tier flag. No orgs, no roles, no SSO. **[S]** Magic links need transactional email — Resend is 3,000/mo free then $20/mo for 50k; AWS SES is ~$0.10/1,000 but you own IP warmup, bounces and complaints. **[S]**

**The tradeoff:** rolling your own is the best privacy story (no third-party sub-processor holding user emails, simplest GDPR posture, zero lock-in) but you own rate limiting, enumeration resistance, token entropy, replay protection and email deliverability. Auth is where solo developers get owned.

### Lock-in

Lower than folklore suggests. Clerk exports users **including password hashes** as CSV from the dashboard, with a trickle-migration path and hashers covering bcrypt/argon2/pbkdf2/scrypt/phpass. **[S]** Supabase is your own Postgres. WorkOS is standard OIDC. The real lock-in is architectural — session model, authorization, claim shapes — not the user records.

**Recommendation:** start on **WorkOS AuthKit**. It's $0 at every realistic scale, it removes a class of security work, it's standard OIDC so swapping later is config not rewrite, and it's the only vendor that explicitly documents production loopback for native clients. **Validate the desktop flow in a spike first** — the AuthKit overview page documents only web/hosted-UI scenarios, so confirm the end-to-end loopback round-trip before committing.

---

## 4. How Tauri 2 apps actually do login

**Plugin versions [V]:** `tauri-plugin-deep-link` 2.4.10 (2026-08-31; 3.0.0-alpha.0 on 2026-09-13) · `tauri-plugin-single-instance` 2.4.4 · `tauri-plugin-oauth` 2.1.0 (2026-07-07, 215 stars, last push 2026-09-15) · `tauri-plugin-opener` 2.5.5.

**RFC 8252 is normative and unambiguous [V]:** *"native apps MUST NOT use embedded user-agents to perform authorization requests"*; *"Public native app clients MUST implement PKCE"*; *"The authorization server MUST allow any port to be specified at the time of the request for loopback IP redirect URIs."* Note that RFC-conformant private-use schemes are reverse-DNS (`tech.lattice.app:/callback`), not `lattice://`.

**Google bans embedded webviews outright [V]** (policy page last modified 2026-08-05): *"A developer must not direct a Google OAuth 2.0 authorization request to an embedded user-agent under the developer's control."* Tauri's webview *is* WKWebView/WebView2 — exactly the class named. Violations return `disallowed_useragent`.

**Custom scheme vs loopback, by provider:**

| Provider | Loopback | Custom scheme |
|---|---|---|
| Google (Desktop client) | ✅ recommended **[V]** | ❌ *"no longer supported"* **[V]** |
| GitHub | ✅ *"does not need to match the port"* **[V]** | ❌ |
| WorkOS | ✅ `127.0.0.1` in production **[V]** | — |
| Microsoft Entra | ✅ `localhost` only **[V]** | ❌ **[U]** |
| **Apple** | ❌ | ❌ — HTTPS + domain required **[V]** |

Apple's documented pattern for platforms that can't run Sign in with Apple JS is **Apple → your HTTPS server → 302 to a custom scheme**. There is no way to avoid running a server for SIWA.

**Deep-link platform mechanics [V]:** macOS bakes schemes into `Info.plist` at bundle time and **cannot register at runtime** (`register()` returns `UnsupportedPlatform`) — so there is no dev-mode workaround on macOS, and you must install a real `.app` for LaunchServices to index it. Windows writes to `HKEY_CURRENT_USER\Software\Classes\<scheme>` (no admin needed). Linux writes a `.desktop` file and shells out to `xdg-mime` and `update-desktop-database` — the latter isn't installed by default on KDE ([plugins-workspace#2265](https://github.com/tauri-apps/plugins-workspace/issues/2265), open since 2025-01-06).

**On Windows and Linux the OS spawns a second instance** with the URL as argv, so you need `tauri-plugin-single-instance` **registered first**. Its argument parser bails unless the deep link is the *only* remaining argv entry — any launcher or wrapper that appends an argument silently kills the flow. **[V]**

**Known pitfalls [V]:**
- [tauri#15928](https://github.com/tauri-apps/tauri/issues/15928) — Linux packaged builds drop deep-link URLs (missing `%u`). **Open.** Masked in dev because the runtime-registered handler *does* include `%u`. Workaround: override `bundle.linux.desktopTemplate`.
- [tauri#12726](https://github.com/tauri-apps/tauri/issues/12726) — deep-link + single-instance not working on Windows 11. Open since 2025-02-17.
- [tauri#10570](https://github.com/tauri-apps/tauri/issues/10570) — Linux deep links, open since 2024-08-11.
- **Cold-start race:** call `getCurrent()` on mount *and* register `onOpenUrl`. Listener-only loses every cold-start callback. This race doesn't exist with loopback.

**Sign in with Apple, concretely [V]:** Apple Developer Program $99/yr. Services ID associated with *"an existing primary iOS, macOS, tvOS, or watchOS App ID enabled for Sign in with Apple"* — an App ID, **not** a published App Store app (the help page is clearer than the REST docs, which contradict it **[U]**). Private email relay needs SPF/DKIM on your sending domain. **Client secret JWT expiry is capped at 15,777,000 seconds (six months)** and Apple enforces it — miss the rotation and every login breaks with `invalid_client`. Max two private keys per app, which is what makes zero-downtime rotation possible.

**Google for desktop [V]:** loopback recommended, prefer `127.0.0.1` over `localhost` (firewall issues), random ephemeral port. Client secret is "Optional" and Google acknowledges installed apps can't keep secrets. Brand verification takes **2–3 business days** manual review, needs a homepage + privacy policy on the same domain + Search Console domain ownership. Two gifts if you request only `openid email profile`: you **skip CASA entirely**, and you're exempt from the 7-day testing-mode refresh-token expiry (*"unless the only OAuth scopes requested are a subset of name, email address, and user profile"*). The 100-user unverified cap applies only when the unverified-app screen shows, i.e. sensitive/restricted scopes. **[V]** Watch: **100 refresh tokens per account per client** (oldest silently invalidated), and refresh tokens die after **six months unused**.

**Keyring storage [V]:** `keyring` 4.2.0 (2026-08-29). As of 4.0 it became a thin wrapper; the API moved to `keyring-core` plus per-platform store crates — but `keyring = "4"` with the `v1` feature preserves the old API. **Windows Credential Manager caps `CredentialBlob` at `CRED_MAX_CREDENTIAL_BLOB_SIZE` = 5×512 = 2,560 bytes** — so store **only the refresh token**; an ID token plus access token bundle will blow the cap. **macOS keychain prompts are bound to the designated requirement, not the cdhash** — per Apple DTS, *"This happens automatically if you use Developer ID… signing"*, so normal updates don't re-prompt; **ad-hoc signed builds (your current config) will.** **Linux is the hard platform**: Secret Service may be absent; the keyutils fallback is *"completely in-memory and will not persist across reboots"* — unusable for a weeks-offline app. Degrade loudly to "sign in again" rather than falling back silently.

**Skip `tauri-plugin-stronghold`.** The wrapper is maintained (2.3.2) but the underlying `iota_stronghold` crate was last published **2024-05-13** and the upstream repo's last commit was **2023-06-29**. **[V]**

---

## 5. Hosting a small axum + Postgres service

All figures verified on vendor pages this session unless marked.

| Platform | App instance | Managed Postgres | Backups/PITR | Ops burden |
|---|---|---|---|---|
| **Fly.io** | shared-cpu-1x: **$2.02** (256MB) / **$3.32** (512MB) / **$5.92** (1GB), Amsterdam; egress $0.02/GB EU+NA | **MPG Basic $38** (1GB), Starter $72 (2GB), then a cliff to $282 | **10-day PITR included**, HA on every plan | Low. But MPG docs still list *"Security patches and version upgrades"* as under development **[V]** — a maturity flag |
| **Railway** | Hobby $5 / Pro $20 base incl. matching credits; **$10/GB-RAM-mo, $20/vCPU-mo, $0.15/GB volume, $0.05/GB egress** | Postgres offered | **Undocumented on pricing page [U]** | Very low, but undocumented backups is itself the answer |
| **Render** | **Prices not retrievable — page is client-rendered [U]** | Yes | PITR **3 days (Hobby) / 7 days (Pro)** by workspace plan; free tier gets none | Low |
| **Hetzner** | **Pricing unresolvable [U]** — two sources differ 2× on the same SKU (€2.99 vs €5.99 for CX23) | **None — no managed Postgres** | DIY (pgBackRest) | **High.** ~8–16h setup, then 2–4h/mo, spiking for major upgrades |
| **DigitalOcean** | App Platform **$5** (512MiB) / **$10** (1GiB); free static sites; $0.02/GiB overage | Managed PG **$15.15** (1GiB) | Not on pricing page **[U]** | Low |
| **AWS Lightsail** | **$3.50** (IPv6-only 512MB) / **$5** (0.5GB, 1TB transfer) | **$15** Standard / **$30** HA | Not stated **[U]** | Medium |
| **Cloudflare Workers** | **$5/mo min**, 10M req + 30M CPU-ms incl.; Free plan only **10ms CPU/invocation** | D1: 5GB free then **$0.75/GB-mo**; **Hyperdrive included on Free and Paid** | D1 has time-travel | Low — **but see below** |
| **Supabase** | n/a (BaaS) | Pro **$25** | 7-day daily backups; **PITR +$100/mo** | Lowest |

**Cloudflare is disqualified for this codebase.** workers-rs does support axum via its `http` feature, but the README states *"You've got to leave your threaded async runtimes at home; meaning no Tokio or async_std support"* and *"All crates in your Worker project must compile to `wasm32-unknown-unknown`."* **[V]** Your `sqlx` + tokio layer won't. Bundle limit 64 MiB, memory 128 MB.

**Object storage** (you're storing backup blobs, and restores mean downloads):

| | Storage | Egress | Free tier |
|---|---|---|---|
| **Cloudflare R2** | **$0.015/GB-mo** ($0.01 infrequent) | **Free, no limit [V]** | 10 GB, 1M Class A, 10M Class B |
| **Backblaze B2** | **$6.95/TB-mo** (~$0.00695/GB) | Free to **3× stored data**, then $0.01/GB | Class A/B/C calls free |

**R2 wins for a backup product** despite costing ~2× on storage: restores are downloads, and free egress removes the tail risk of a user restoring a large vault.

---

## 6. Managed Postgres

| Provider | Entry production | Free tier | Storage $/GB-mo | PITR | EU regions | Lock-in |
|---|---|---|---|---|---|---|
| **Crunchy Bridge** | **$35** (Hobby-2, 2GB) | none | **$0.10** | **10 days, included, minute granularity** | Frankfurt, Ireland **[S]** | **Lowest** |
| **Neon** | ~$20 (Launch, usage) | 100 CU-h, 0.5 GB | $0.35 + $0.20 restore | 7d Launch / 30d Scale | Frankfurt, London | Low–Med |
| **Scaleway** | ~€11–17 | none | €0.099–0.149 | **Unconfirmed [U]** | Paris, AMS, Warsaw | Low |
| **Supabase** | $30, or **$130 with PITR** | 500MB, pauses | $0.125 | **+$100/mo** | AWS EU **[U]** | Low (DB) |
| **DigitalOcean** | $15.15 | none | $0.215 | **Unconfirmed [U]** | FRA/AMS/LON **[U]** | Low |
| **Fly MPG** | $38 | none | $0.28 | **10 days, included** | fra, ams, lhr | Low |
| **AWS RDS** | $23.36 (t4g.small, 1-AZ, us-east-1) | **credits only** | $0.115 | 1–35d included | eu-central-1 | **Lowest** |
| **Nile** | $15 | 1GB, never pauses | **$1.00** | **"Coming soon"** ❌ | **[U]** | Med |

**Neon**: Free / Launch / Scale (+ an Agent plan); Business is no longer self-serve. **[V]** Launch is $0.106/CU-hour + $0.35/GB; **Scale is 2.1× the compute rate** ($0.222) — so buying 30-day retention reprices all your compute. Scale-to-zero after 5 min; Neon claims *"a few hundred milliseconds"* reactivation, independent measurements say **300–800ms cold start [S]** — fine for a sync backend. **Region is fixed at project creation and cannot be changed — pick Frankfurt on day one.**

**Neon/Databricks (announced 2025-05-14, ~$1B) has so far been good for pricing** — storage fell ~80% ($1.75→$0.35/GB), minimums removed. **[V]** But Databricks cited that *">80% of databases provisioned on Neon were created by AI agents"* — the roadmap is now oriented toward ephemeral agent databases, and the price advantage looks like a parent-company subsidy. **[S]**

**Crunchy/Snowflake (closed 2025-06-06, $164.5M per Snowflake's 10-K)**: Snowflake Postgres went GA 2026-02-24 with the Crunchy team; Crunchy Bridge is still sold with no announced sunset, but shows no investment signals. **[V]** Mitigated by it being the *least* locked-in option in the table — it's stock Postgres, so exit is one `pg_dump`.

**Fly Postgres history, disambiguated [V]:** legacy `fly pg create` is an unmanaged template Fly explicitly won't support; "Fly Postgres managed by Supabase" was **deprecated 2025-04-11**; **Fly Managed Postgres (MPG)** is the current first-party product.

**AWS free tier is effectively gone** — the 12-month tier was sunset 2025-07-15; new accounts get $100 credits + up to $100 more, expiring in 12 months. **[S]** Note: AWS's own RDS PostgreSQL pricing page **still advertises the retired 750-hour free tier** **[V]** — stale marketing copy. **Aurora Serverless v2 scale-to-zero shipped (2024-11-20) but resume latency is ~15 seconds [V]**, which disqualifies it for a desktop app waking from sleep.

**Two things to do regardless of vendor:** keep encrypted note blobs in object storage, not Postgres (20–70× cheaper per GB); and run your own `pg_dump` to independent storage on a schedule, restore-tested quarterly. Three vendors above have undocumented backup behaviour and two are mid-acquisition.

---

## 7. Payments

| Route | Fee | On $5/mo you net | Tax handled by |
|---|---|---|---|
| **Stripe direct** | 2.9% + $0.30; Billing +0.7%; Tax +0.5% | **~$4.50 (90%)** | **You** — registration + remittance |
| **Stripe Managed Payments** | +**3.5%** on top of standard **[V]** | **~$4.38 (88%)** | Stripe (MoR) — 80+ countries |
| **Paddle** | 5% + $0.50 **[V]** | **$4.25 (85%)** | Paddle (MoR) |
| **Lemon Squeezy** | 5% + $0.50 **[V]** | $4.25 | LS (MoR) |
| **Polar** | Starter 5%+$0.50; Pro $20/mo at 3.8%+$0.40; +1.5% intl; $15/dispute **[V]** | $4.25 | Polar (MoR) |
| **Apple IAP** | 30%, or **15% under Small Business Program** (<$1M/yr) **[V]** | $3.50 / **$4.25** | Apple |

**Flag on Paddle:** the pricing page states **products under $10 require custom pricing** — a $5/mo subscription may not get standard terms. **[V]**

**Lemon Squeezy status:** still operating and accepting sellers, with a banner *"2026 Update: Lemon Squeezy + Stripe Managed Payments"* (post dated 2026-01-28) whose body I couldn't retrieve. **[U]** Given Stripe now sells Managed Payments directly, treat LS as a product mid-absorption and prefer Stripe Managed Payments or Paddle for new integrations.

**The MoR argument at small scale isn't the headline rate — it's EU VAT.** Selling to EU consumers without MoR means VAT-inclusive pricing (~20%) plus OSS registration and quarterly filings. That dwarfs the 2–3 point fee difference for a two-person team.

**Mac App Store:** Guideline 3.1.1(a) **[V]** — the StoreKit External Purchase Link Entitlements are *"limited to use only in the iOS or iPadOS App Store in specific storefronts"*, with the US storefront carved out of the prohibition on external purchase calls-to-action. Whether the US carve-out extends to the **Mac** App Store is **not clear from the guideline text [U]**. **Distributing outside the Mac App Store as a notarized DMG avoids all of it** — and is the normal Tauri distribution path.

---

## 8. Compliance and privacy

**Baseline obligations for a 1–2 person team storing notes:**

- **Art 30(5) won't save you.** Verbatim **[V]**: the exemption for <250 employees doesn't apply where *"the processing is not occasional."* A running sync service is never occasional. **Keep a ROPA.**
- **Art 12(3) [V]:** respond to access/export/erasure requests *"without undue delay and in any event within one month"*, extendable by two months for complexity.
- **Art 33:** 72-hour regulator notification.
- **Art 27 [V]:** a US entity with no EU establishment serving EU users must designate an EU representative. The derogation covers only processing that is *"occasional… and is unlikely to result in a risk"* — a continuous notes service doesn't qualify. Vendor pricing **unverified** (Prighter rate-limited, DataRep 404). **[U]**
- **DPO:** not triggered for this profile (no large-scale special-category or systematic monitoring).
- **If you ever ship to the Mac App Store**, Guideline 5.1.1(v) requires **in-app account deletion**. **[V]** Build it from day one — retrofitting into a sync backend is unpleasant, and GDPR Art 17 wants it anyway.

**Transfers:** the **EU-US Data Privacy Framework is still in force** — the US is listed on the Commission's own adequacy page as *"commercial organisations participating in the EU-US Data Privacy Framework"* (fetched today). **[V]** UK adequacy was renewed December 2025. **[V]** I could not verify the Latombe outcome or any appeal. **[U]** The structural hedge is EU-only hosting (Scaleway, Hetzner) — note that every US-incorporated vendor is CLOUD Act-exposed regardless of where bytes sit. **[S]**

**Does E2EE materially reduce obligations?** Partially, and less than vendors imply.

- **Concrete win [V]:** Art 34(3)(a) exempts you from notifying *data subjects* where measures *"render the personal data unintelligible to any person who is not authorised to access it, such as encryption."* You still owe Art 33 regulator notification.
- **What doesn't change:** account email, billing records, IP addresses, device IDs, timestamps, blob sizes and sync frequency remain plainly personal data. You remain a controller for all of it. Access/export/erasure duties are undiminished.
- **The key authority is CJEU Case C-413/23 P (EDPS v SRB)**, on whether pseudonymised data is personal data relative to a recipient lacking the key. **I could not verify this case from any primary source this session** — eur-lex returned empty pages, curia 404'd, the EDPS site 403'd, and gdprhub is behind a bot wall. My recollection is a judgment on **4 September 2025** adopting a relative, recipient-focused test, but **treat that as unverified model knowledge, not research.** **[U]** Get counsel to confirm before relying on it.

**Encryption policy climate:** EU Chat Control — mandatory client-side scanning was reportedly **dropped in the Council's November 2025 mandate**, with trilogues running Dec 2025–Jun 2026 and adoption expected ~July 2026. **[S — single advocacy source (Patrick Breyer); whether it was actually adopted by now is unverified [U]]**. UK — Apple received a Technical Capability Notice **2025-02-07**, withdrew Advanced Data Protection for UK users **2025-02-21**, and appealed to the Investigatory Powers Tribunal in **March 2025**. **[V]** **Everything after March 2025 is unverified.** **[U]**

**Account recovery when data is E2EE — four real patterns:**

1. **Unrecoverable.** Obsidian Sync: separate encryption password (AES-256, scrypt, GCM), *"if you forget or lose your encryption password, your data remains encrypted and unusable forever. We're not able to recover your password, or any encrypted data for you."* **[V]**
2. **Recovery key issued at setup.** 1Password Secret Key + Emergency Kit — *"We don't have a copy of your Secret Key or any way to recover or reset it for you."* Business users get admin recovery. **[V]**
3. **Split account-recovery from data-recovery.** Proton is the clearest model **[V]**: recovery *phrase* restores password **and** data; recovery *file* restores data only; recovery email/phone resets password only — *"If you have a password reset method and no data recovery method, you'll lose access to everything that was on your account before the password reset."*
4. **Enclave-escrowed secret behind a low-entropy PIN.** Signal SVR. **Out of reach for a 1–2 person team** — it needs HSM/enclave infrastructure and attestation.

**The local-first insight that makes this easy for Lattice:** because the vault lives on disk, losing the sync passphrase costs the user their *backup*, not their *notes*. Obsidian's documented remedy is exactly that — reset the remote vault and re-upload from a device that still has the data. That makes **pattern 1 (unrecoverable) genuinely shippable**, with pattern 2 as a later upgrade. Ship a downloadable recovery key at setup and a blunt confirmation dialog.

---

## 9. The no-accounts alternative: bring your own storage

**What BYOS saves:** zero server ops, zero storage cost, near-zero breach blast radius, and a much smaller compliance surface — if your servers never receive user content, you aren't processing it. **Caveat:** you'd still process account/licence/telemetry data, so you don't escape GDPR, you shrink it. Whether a client-side-only vendor is a controller/processor at all is **[U]** — I didn't reach authoritative guidance.

**Per-backend constraints:**

- **Google Drive — the good news [V]:** `drive.file` ("files you open with the app or that the user shares with it") and `drive.appdata` are both classified **non-sensitive**, requiring only basic OAuth verification. The six **restricted** scopes (`drive`, `drive.readonly`, `drive.metadata*`, `drive.activity*`, `drive.scripts`) trigger restricted-scope verification with a third-party security assessment **reverified at least every 12 months** **[V]**. Google explicitly doesn't set the price: *"The cost… is agreed on between the developer and the assessor without any involvement from Google."* Public figures range wildly ($500 to $75,000+) and are **not credible [U]**. **Stay on `drive.file` and none of this applies.**
- **Dropbox [V]:** App folder vs Full Dropbox — use App folder. Development mode caps at 500 users, but **at 50 linked users you have two weeks to obtain production approval**, and *"it will not be reviewed until your app has linked with at least 50 Dropbox users."* That's a hard gate you hit early.
- **iCloud/CloudKit:** Apple advertises CloudKit across *"iOS, iPadOS, macOS, tvOS, watchOS, visionOS and the web"* **[V]**. CloudKit Web Services exists, so Windows/Linux is *theoretically* reachable — but it requires an Apple ID web sign-in inside a desktop app, which is poor UX, and iCloud Drive ubiquity containers are macOS-only. **Practically this kills cross-platform parity. [U] — not investigated in depth.**
- **S3-compatible (user's own bucket):** technically simplest, worst UX — you're asking a note-taker for an endpoint, access key and secret.

**What BYOS costs:** credential-entry friction; conflict resolution with no server arbiter; no server-side versioning or dedup; no cross-device discovery; no share links; support burden multiplied across N backends that each fail differently; and **you still need some identity to license the paid tier**.

**The honest reading for Lattice:** BYOS is a strong *free tier* and a poor *only* option. Obsidian's model is the proof — free users bring their own Dropbox/iCloud/Syncthing, and **Obsidian Sync at $4/user/mo annual** (1 GB, 5 MB file cap, 1-month history) or **$8/mo Plus** (10 GB, 200 MB files, 12-month history) exists precisely because BYOS doesn't deliver the experience people will pay for. **[V]** Given you already have a sync server and schema in the repo, BYOS is a complement, not a pivot.

---

## 10. What I could not verify

1. **CJEU C-413/23 P (EDPS v SRB)** — no primary source reachable (eur-lex empty, curia 404/403, gdprhub bot-walled). The most load-bearing legal authority in the E2EE section. **Get counsel.**
2. **EU Chat Control final status** — one advocacy source only; whether the regulation was actually adopted by Sept 2026 is unknown.
3. **UK Apple ADP** — nothing verified after March 2025.
4. **Art 27 EU representative pricing** — Prighter HTTP 429, DataRep 404.
5. **Hetzner VPS pricing** — two sources differ 2× on the same SKU; page is client-rendered. Pull it in a browser.
6. **Render instance and Postgres prices** — client-rendered, unreachable via three different URLs.
7. **DigitalOcean and Scaleway PITR** — widely assumed included; not on their pricing pages.
8. **Railway and Xata backup/PITR** — undocumented on pricing pages.
9. **Crunchy Bridge EU region list** — secondary source only, and it's load-bearing for the 10k+ recommendation.
10. **Mac App Store US external-link carve-out** — guideline text names iOS/iPadOS; macOS applicability unclear.
11. **Lemon Squeezy's 2026 Stripe announcement** — banner seen, body not retrievable.
12. **Apple SIWA prerequisite contradiction** — REST docs say "existing app in the App Store", help page says only an App ID. The help page is more likely correct.
13. **Stytch per-MAU overage** and **Supabase EU region list** — not published / not retrievable.

**One general warning:** AWS's own RDS pricing page still advertises a free tier that was retired 14 months ago. Treat every number here as a starting point for a quote, not a quote.

---

**Two research agents (hosting depth, compliance depth) were still running at the time of writing;** I covered both areas independently via direct source fetches, so this report is complete as it stands, but their findings may add depth on Railway/Render/Hetzner pricing and on the unverified legal items above.

---

Two sibling tracks are still running despite wrap-up requests. I have independent coverage of their topics, so here is the report.

# Lattice: Accounts, Auth, Hosting & Ops — Research Report
**Compiled 2026-09-16.** Confidence markers: `[V]` = verified on a primary/vendor page this session · `[S]` = secondary source only · `[U]` = unverified, conflicting, or possibly stale.

> **Two caveats up front.** (1) This session's WebSearch budget was exhausted early (200/200), so later work was done via direct WebFetch on guessed URLs and the `gh` CLI. Coverage of vendor primary docs and GitHub is strong; coverage of forums/blogs is thin. (2) Two sibling research tracks (hosting deep-dive, compliance/BYOS deep-dive) were still running at delivery; I covered both topics independently, but their reports may add depth later.

---

## 0. What I found in your codebase first — this reframes the question

Before the market research, four facts from `/Users/josh/Code/lattice-temp` that change what you're actually deciding:

1. **The sync server already exists and has no authentication.** `/Users/josh/Code/lattice-temp/api-rust/` is axum 0.8 + sqlx 0.8 + Postgres with `/devices/register`, `/push`, `/pull`, `/ack`, `/conflicts/resolve`. Identity comes from `user_id_from_headers`, which reads a plaintext `x-user-id` header and parses it as an `i64`. Any client can be any user by setting one header. This is the exact seam where auth goes, and it must not ship as-is.

2. **The current schema is the opposite of end-to-end encrypted.** `/Users/josh/Code/lattice-temp/api-rust/migrations/0001_sync_init.sql` stores `path TEXT`, `title TEXT`, `content TEXT` in plaintext, and `conflicts.resolution_content TEXT` implies **server-side** conflict resolution. There are no crypto crates in either `Cargo.toml`. **The E2EE decision must be made before you freeze this schema and wire format** — E2EE makes server-side content merge impossible, which directly contradicts the design already in the migration.

3. **You already rolled your own auth once, in Python.** `/Users/josh/Code/lattice-temp/src/api/src/auth/` is a FastAPI JWT system (bcrypt, MFA rate limiter, token blacklist, separate access/refresh secrets). `jwt.py` signs with `settings.jwt_secret_key` — a **shared secret (HS256-style), not asymmetric**, so there is no JWKS. Any move to a hosted IdP (or to a sane self-issued scheme) means switching to RS256/ES256 + JWKS.

4. **macOS signing is ad-hoc.** `src-tauri/tauri.conf.json` has `"signingIdentity": "-"`, `"entitlements": null`, identifier `tech.lattice.app`, and only the `shell` plugin registered. Ad-hoc signing means the macOS Keychain designated requirement changes between builds → **keychain re-prompts on every rebuild**, and no notarized distribution. You need a real Developer ID ($99/yr) before storing refresh tokens in the keyring is pleasant.

Good news: `tauri-plugin-shell` resolves to **2.3.4** in `Cargo.lock`, past the 2.2.1 fix for CVE-2025-31477, so you are **not** exposed to that. `keyring` resolves to **3.6.3** (current is 4.2.0); `tauri` is pinned at `=2.9.5`.

---

## 1. Executive summary and recommended stack

### The recommendation

| Layer | Pick | Why | Cost |
|---|---|---|---|
| **Identity** | **WorkOS AuthKit** | Free to **1M MAU** `[V]`, PKCE for public clients with **no client secret** `[V]`, and — critically — **`http://127.0.0.1` is explicitly allowed in production for native clients** `[V]`. Standard OIDC + JWKS, so swapping it out later is config, not a rewrite. | **$0** |
| **Login flow** | **System browser → loopback PKCE**, not deep links | Google **no longer supports custom URI schemes** at all `[V]`; Tauri's Linux deep-link path is **actively broken** (§3). Loopback also eliminates the cold-start callback race. | $0 |
| **Token storage** | `keyring` 4.x, **refresh token only** | Windows Credential Manager caps a credential blob at **2,560 bytes** `[V]`. Access/ID tokens stay in memory. | $0 |
| **API** | Keep the existing axum 0.8 service | Add `axum-jwt-auth` 0.7.0 (axum ^0.8, remote JWKS with caching) `[V]`. Replace the `x-user-id` header. | — |
| **App host** | **Fly.io** `shared-cpu-1x` 512MB | $3.32/mo Amsterdam `[V]`. Or Hetzner if you want EU-owned. | ~$4–6 |
| **Postgres** | **Neon Launch, Frankfurt** | Usage-based, scale-to-zero, 7-day PITR. Region is **irreversible** — pick Frankfurt day one. | ~$20 |
| **Note blobs** | **Cloudflare R2** | $0.015/GB-mo, **egress genuinely free**, 10 GB free `[V]`. Restores are downloads; free egress is decisive. | $0–5 |
| **Email** | Resend free tier | 3,000/mo, 100/day `[V]`. | $0 |
| **Payments** | **Merchant of record**, outside the Mac App Store | Paddle 5%+50¢ `[V]`, Stripe Managed Payments +3.5% on top of 2.9%+30¢ `[V]`, Polar 5%+50¢ free tier `[V]`. MoR handles EU VAT registration and remittance — the single biggest compliance saving available to you. | % only |
| **Apple** | Developer Program | Needed for Developer ID + notarization regardless. | $99/yr ≈ $8.25/mo |

**Expected monthly cost at launch: roughly $35–65/month**, plus payment processing percentages and an Art 27 EU representative (price unverified — see §7). Identity is genuinely $0 at any scale you'll reach.

### The five decisions that actually matter

1. **Decide E2EE before you freeze the sync schema.** Your current migration stores plaintext content and does server-side conflict resolution. E2EE forbids both. This is a one-way door.
2. **Do not offer social login.** Apple's Guideline 4.8 has an exemption — *"Your app exclusively uses your company's own account setup and sign-in systems"* `[V]`. Offer only your own email/passkey login and Sign in with Apple is irrelevant even if you later ship to the Mac App Store. Adding Google sign-in is what creates the obligation.
3. **Loopback, not deep links.** This is forced by Google's policy and by a live Tauri bug, not a preference.
4. **Ship outside the Mac App Store.** Notarized DMG ⇒ no Guideline 4.8, no IAP, no 15–30% commission, no review. Tauri + MAS sandboxing is painful anyway.
5. **Passkeys must happen in the browser, not in your webview.** Native in-app passkeys are not viable cross-platform in 2026 (§2.5).

---

## 2. Identity options

### 2.1 Hosted auth pricing at 1k / 10k / 100k MAU

All figures fetched from vendor pricing pages on 2026-09-16 `[V]` unless marked.

| Provider | Free tier | 1k MAU | 10k MAU | 100k MAU | Notes |
|---|---|---|---|---|---|
| **WorkOS AuthKit** | **1M MAU** | **$0** | **$0** | **$0** | $2,500/mo per additional 1M. Custom domain $99/mo. Enterprise SSO $125/connection — not your problem. |
| **Clerk** | 50,000 **MRU** | $0 | $0 | ~$25 + overage | Bills **MRU** ("only counts as retained if they return ≥24h after signing up"), not MAU. Pro $25/mo incl. 50k MRU, then $0.02 each. |
| **Supabase Auth** | 50,000 MAU | $0 (Free) or $25 (Pro) | $25 | $25 | Pro includes 100k MAU, then $0.00325/MAU. SAML separately at $0.015/MAU after 50. |
| **Firebase / GCIP** | 50,000 MAU | $0 | $0 | ~$275 `[S]` | $0.0055/MAU for the 50k–100k band, stepping down at volume. SAML/OIDC $0.015/MAU. |
| **Kinde** | 10,500 MAU | $0 | $25 (Pro) | $25 + ~$1,566 | Pro $25/mo + $0.0175/extra MAU. **Passkeys are not on the free plan.** |
| **Stytch** | 10,000 MAU | $0 | $0 | Not published `[U]` | Pricing page shows no per-MAU overage rate at all. $99 one-time to remove branding. |
| **Auth0 (B2C)** | 25,000 MAU | $70–240/mo | $700–1,600/mo | "Contact us" | Free tier rose 7,500→25,000 in Sept 2024 `[S]`. Overage went $0.023→$0.07/MAU in the 2023 repricing — a 300% jump `[S]`. The cautionary tale on pricing risk. |

**WorkOS is the outlier and it is not close.** 1M MAU free, and it's the only one whose docs explicitly bless `http://127.0.0.1` in production for native clients. Be appropriately skeptical: WorkOS monetizes enterprise SSO at $125/connection, and AuthKit is customer acquisition for that funnel. If they reprice, your exit is a standard OIDC swap — which is exactly why standard OIDC matters more than the price.

### 2.2 Self-hosted

| Option | License | Current release | Footprint | Verdict |
|---|---|---|---|---|
| **Keycloak** | Apache-2.0 `[V]` | 26.7.3, 2026-08-31 `[V]` | **4 GB min, 8 GB production** `[S]`; JVM/Quarkus; 60–90s cold start `[S]` | Release cadence every 2–4 weeks (26.6.2 May 19 → 26.7.3 Aug 31 = 6 releases in 3.5 months) `[V]`. A patch treadmill for two people. |
| **Zitadel** | **AGPL-3.0 since v3** `[S]` (was Apache-2.0) | v4.17.3, 2026-09-04 `[V]` | **~512 MB RAM, <1 CPU core** for the app; +4 cores for password hashing; 3×(4 CPU/16 GB) recommended for HA `[V]` | Much lighter than Keycloak. Cloud: free 100 DAU, Pro **$100/mo for 25,000 DAU** `[V]`. **AGPL is a real flag** for a commercial product — commercial license available. |
| **authentik** | Mixed (`NOASSERTION`) `[V]` | 2026.8.2, 2026-09-09 `[V]` | — | **No hosted option** — "We do not currently provide a hosted version" `[V]`. Enterprise $5/user/mo. Note 2026.8.2 / 2026.5.7 / 2026.2.7 all shipped **the same day** — a coordinated security release across three branches. That's the maintenance reality. |
| **Ory** | Apache-2.0 (OSS) | — | — | Ory Network: Production **$770/yr** + $0.14/aDAU/mo `[V]`. Developer plan has **0 production environments** `[V]`. Prices in aDAU, not MAU. |

**Verdict: don't self-host an IdP.** The release cadences above are the argument. If you can't name who gets paged at 3am when token issuance stops, you're not ready — and a two-person team has better places to spend that attention. Zitadel is the one I'd pick if forced.

### 2.3 Rolling your own in axum

Genuinely viable here, because **your account is thin**: an email, a device list, a subscription flag, and (if E2EE) a wrapped key. No orgs, no roles, no SAML.

Crate health, checked on crates.io/docs.rs today:

| Crate | Version | Updated | Note |
|---|---|---|---|
| `jsonwebtoken` | **11.0.0** | 2026-07-24 `[V]` | 189M downloads. Healthy. |
| `webauthn-rs` | **0.5.5** | 2026-04-30 `[V]` | 6.8M downloads. Kanidm's. Production-grade. |
| `openidconnect` | 4.0.1 | 2025-07-06 `[V]` | 13.3M downloads. |
| `oauth2` | 5.0.0 | 2025-01-21 `[V]` | 50.6M downloads. |
| `axum-jwt-auth` | **0.7.0** | 2026-05-19 `[V]` | axum ^0.8, jsonwebtoken ^10, **remote JWKS with caching + background refresh**. Best fit. |
| `axum-jwks` | 0.12.0 | 2025-06-06 `[V]` | axum ^0.8, jsonwebtoken ^9. Fine. |
| `jwt-authorizer` | 0.15.0 | **2024-08-27** `[V]` | **Depends on axum ^0.7** — incompatible with your axum 0.8. Despite 499k recent downloads. **Avoid.** |

The real cost of rolling your own isn't the JWT code, it's **email deliverability** (SPF/DKIM/DMARC, bounce handling, reputation) plus rate limiting, enumeration resistance, and replay protection. Resend is $0 up to 3,000/mo then $20/mo for 50k `[V]`; AWS SES is ~$0.10/1,000 but you own warmup and suppression `[S]`.

### 2.4 Lock-in

Low across the board for user data, higher for architecture. Clerk lets you export a CSV **including password hashes** from the dashboard `[S]`, and WorkOS publishes a Clerk→WorkOS migration tool `[S]`. The durable lock-in is your session model and authorization code, not the user rows. **This is the argument for plain OIDC + JWKS over any vendor SDK.**

### 2.5 Passkeys on desktop — the finding that constrains everything

Tauri issue [#7926](https://github.com/tauri-apps/tauri/issues/7926), "Allow Passkeys auth support in WebView", is **still open and still labelled `needs triage`** — opened 2023-09-30, last activity 2026-04-07 `[V]` (checked via `gh api`). From the thread `[V]`:

- **Windows 11 / WebView2: works.**
- **macOS WKWebView:** requires the Associated Domains entitlement plus `com.apple.developer.web-browser.public-key-credential`; a commenter hit app-won't-launch breakage and gave up.
- **Linux WebKitGTK:** "lacks some WebAPI to do the authentication."

The community plugin [`Profiidev/tauri-plugin-webauthn`](https://github.com/Profiidev/tauri-plugin-webauthn) (MIT, 20 stars, last push 2026-09-01, last tagged release v0.2.0 May 2025) supports **Linux, Windows, Android — and explicitly not macOS** `[V]`.

**Conclusion: native in-app passkeys are not viable for a macOS-first cross-platform app in 2026.** Passkeys work fine in the *system browser* on all three platforms — which is another independent reason the whole login flow belongs in the browser.

---

## 3. How Tauri 2 apps concretely do login

### 3.1 The normative rules

**RFC 8252** is unambiguous `[V]`:
- *"native apps MUST NOT use embedded user-agents to perform authorization requests"* (§8.12)
- *"Public native app clients MUST implement the Proof Key for Code Exchange (PKCE)"* (§8.1)
- *"The authorization server MUST allow any port to be specified at the time of the request for loopback IP redirect URIs"* (§7.3)
- Private-use schemes *"MUST use a URI scheme based on a domain name under their control, expressed in reverse order"* — so RFC-conformant for you is `tech.lattice.app:/callback`, **not** `lattice://callback`.

**Google bans embedded webviews outright** `[V]` (policy page last modified 2026-08-05): *"A developer must not direct a Google OAuth 2.0 authorization request to an embedded user-agent under the developer's control."* Tauri's webview **is** WKWebView/WebView2 — precisely the named class.

### 3.2 Custom scheme vs loopback — what providers actually accept

| Provider | Loopback `127.0.0.1:PORT` | Custom scheme | Source |
|---|---|---|---|
| **Google (Desktop client)** | **Yes — recommended.** Use a *random available port*. Prefer `127.0.0.1` over `localhost` ("may cause issues with client firewalls"). | **No.** *"Custom URI schemes are no longer supported due to the risk of app impersonation."* OOB also dead. | `[V]` |
| **GitHub** | Yes, port need not match the registered callback | Not documented | `[S]` |
| **WorkOS** | **Yes — `http://127.0.0.1` explicitly allowed in production for native clients** | — | `[V]` |
| **Apple** | **No** — redirect URI *"must use the HTTPS protocol, include a domain name, and can't contain an IP address or `localhost`"* | **No** | `[V]` |
| **Supabase** | Configured redirect URLs; PKCE code valid **5 minutes, single use**, verifier must stay on the initiating device | Yes (documented for mobile; Flutter macOS/Windows shown) | `[V]` |

`tauri-plugin-oauth`'s own README states the rationale plainly: *"Many OAuth providers (like Google and GitHub) don't allow custom URI schemes ('deep links') as redirect URLs."* `[V]`

### 3.3 The Tauri deep-link bug you need to know about

[**tauri-apps/tauri#15928**](https://github.com/tauri-apps/tauri/issues/15928), opened 2026-08-28, **still open** `[V]`:

> "The Linux desktop entry generated by tauri-bundler declares `MimeType=x-scheme-handler/<scheme>` … but renders `Exec={{exec}}` **without any field code** (`%u`/`%U`). … **For OAuth/OIDC flows this means the callback never reaches the app, and sign-in cannot complete through the system handler.**"

And why it's insidious: the runtime-registered dev handler *does* include `%u`, which **masks the bug in dev** and only breaks in packaged builds. Fix PR #15929 has not landed — the `dev` branch template still reads `Exec={{exec}}` `[V]`.

Related open issues: [#2265](https://github.com/tauri-apps/plugins-workspace/issues/2265) (`update-desktop-database` missing on stock KDE, open since 2025-01-06), [#10570](https://github.com/tauri-apps/tauri/issues/10570) (deep links don't open app on Linux, open since 2024-08-11), [#12726](https://github.com/tauri-apps/tauri/issues/12726) (deep-link + single-instance broken on Windows, open since 2025-02-17).

**Other deep-link mechanics** `[V]`:
- **macOS:** schemes baked into `Info.plist` at bundle time; runtime `register()` returns `UnsupportedPlatform`. You must build and install a real `.app` — the dev workaround that exists on Linux/Windows **does not exist on macOS**.
- **Windows/Linux:** the OS spawns a *second instance* with the URL as argv. You need `tauri-plugin-single-instance` (2.4.4, 2026-08-31), **registered first** — "The Single Instance plugin must be the first one to be registered to work well."
- Sharp edge: `handle_cli_arguments` bails entirely unless the URL is the *only* remaining argv entry. Any launcher that appends an argument silently kills the deep link.

Current versions: `tauri-plugin-deep-link` **2.4.10** (2026-08-31), 3.0.0-alpha.0 (2026-09-13); `tauri-plugin-oauth` **2.1.0** (2026-07-07, 215 stars, last push 2026-09-15) `[V]`. The oauth plugin is thinly maintained — 8 of its last 10 commits are Renovate bumps, and [issue #50 "Tauri v3 compatibility roadmap?"](https://github.com/FabianLars/tauri-plugin-oauth/issues/50) is open. It's ~300 lines; vendoring it is defensible.

### 3.4 Sign in with Apple — you almost certainly don't need it

Guideline 4.8 current text, fetched today `[V]`, no longer names Sign in with Apple. It requires an equivalent service (name+email only, email-hiding, no ad tracking) — **unless**:

> *"Another login service is not required if: Your app **exclusively uses your company's own account setup and sign-in systems**."*

**So: offer only your own email/passkey login → 4.8 doesn't apply.** Add Google sign-in → it does. This applies to the **Mac** App Store too (the guidelines are one cross-platform document, no macOS carve-out) `[V]`.

If you ever do need it: Apple Developer Program $99/yr; a Services ID associated with **an existing primary App ID enabled for Sign in with Apple — not a published App Store app** `[V]` (Apple's REST docs say otherwise; the help page is clearer — flagging the contradiction). Client secret is a JWT whose `exp` *"can't be more than 15777000 seconds (six months)"* `[V]` — miss the rotation and every login breaks with `invalid_client` on the day, no warning. Max two private keys per app, which is what makes zero-downtime rotation possible.

**Distributing outside the Mac App Store removes all of this** `[V]`. Notarization is an automated malware scan, not App Review.

### 3.5 Google specifics worth knowing

- Only `openid`/`email`/`profile` ⇒ **non-sensitive scopes** ⇒ no CASA, no unverified-app screen, **no 100-user cap** (the cap applies only to apps that display that screen, which is triggered by sensitive/restricted scopes) `[V]`.
- Brand verification: automated in minutes, manual review "2-3 business days"; needs a homepage, a privacy policy **on the same domain**, and Search Console domain verification `[V]`.
- **The refresh-token exemption that matters:** testing-mode refresh tokens expire in 7 days *"unless the only OAuth scopes requested are a subset of name, email address, and user profile"* `[V]`. That's you.
- Refresh tokens die after **6 months unused**; limit of **100 per account per client**, and exceeding it *"automatically invalidates the oldest refresh token without warning"* `[V]`.

### 3.6 Keyring pitfalls

`keyring` is now **4.2.0** (2026-08-29) `[V]`; you're on 3.6.3. v4 split the real API into `keyring-core` + per-backend crates, but `keyring = "4"` with the `v1` feature keeps the old API `[V]` — that's the right call for two people.

- **Windows: 2,560-byte cap.** `CredentialBlobSize` *"cannot be larger than CRED_MAX_CREDENTIAL_BLOB_SIZE (5\*512) bytes"* `[V]`. Refresh tokens fit; a JSON bundle with an ID token will not. **Store only the refresh token.**
- **macOS: the folk wisdom is wrong in a useful way.** Per Apple DTS `[V]`, the ACL binds to the **designated requirement** (Team ID + bundle ID), not the cdhash — so a normally-signed Developer ID update does **not** re-prompt. You get prompts when the DR changes, which is exactly what `"signingIdentity": "-"` guarantees during development.
- **Linux is the hard one.** Secret Service may be absent; keyutils is **in-memory only and does not survive reboot** `[V]` — useless for a weeks-offline app; Flatpak needs the `org.freedesktop.portal.Secret` portal `[S]`. Degrade *loudly* to "sign in again after restart" rather than silently falling back.
- **Skip `tauri-plugin-stronghold`.** The wrapper is current (2.3.2) but the underlying `iota_stronghold` crate last published **2024-05-13** and the upstream repo's last commit was **2023-06-29** `[V]`. Wrong risk profile for a privacy product — and it doesn't solve the problem anyway, since you still need somewhere for its password.
- **Design for token loss as a normal path.** Between the Windows cap, macOS DR changes, Linux keyring absence, and Google's 6-month expiry, this is routine. Since sync is optional, the app must stay fully functional signed out.

---

## 4. Hosting a small axum + Postgres service

All prices fetched today `[V]` unless marked.

| Platform | Small app instance | Managed Postgres | Backups/PITR | Ops burden | EU |
|---|---|---|---|---|---|
| **Fly.io** | shared-cpu-1x: **$2.02** (256MB) / **$3.32** (512MB) / **$5.92** (1GB), Amsterdam. Egress $0.02/GB NA+EU | **MPG Basic $38/mo** (1GB), storage $0.28/GB, 10GB min | **10-day PITR included, HA on every plan** | Low | ams, fra, lhr |
| **Railway** | Hobby **$5**/Pro **$20** + usage: ~**$10/GB RAM-mo**, **$20/vCPU-mo**, volumes $0.15/GB, egress $0.05/GB | Postgres available | **Undocumented on the pricing page** `[U]` | Low | `[U]` |
| **Render** | **Could not retrieve** — pricing page is client-rendered, 3 attempts `[U]` | — | PITR by **workspace** plan: Hobby = 3 days, Pro+ = 7 days; **not on Free** `[V]` | Low | `[U]` |
| **Hetzner** | **Could not verify.** Sources conflict 2× on the same SKU (CX23 4GB quoted at both €2.99 and €5.99) `[U]` | **None — Hetzner offers no managed Postgres** `[V]` | DIY (pgBackRest) | **High** | DE, FI |
| **DO App Platform** | **$5** (1 vCPU/512MiB/50GiB), **$10** (1GiB/100GiB); free static sites; $0.02/GiB overage | Managed PG from **$15.15/mo** (1GiB) | Not on pricing page `[U]` | Low | FRA/AMS/LON `[U]` |
| **AWS Lightsail** | **$3.50** (IPv6-only 512MB) / **$5** (0.5GB, 1TB transfer) | **$15/mo** Standard (1GB) / **$30** HA | Not stated on pricing page `[U]` | Medium | Yes |
| **Cloudflare Workers** | **$5/mo min**, 10M req + 30M CPU-ms incl.; $0.30/M req, $0.02/M CPU-ms | D1: 5GB free then **$0.75/GB-mo**; **Hyperdrive included on Free and Paid** | — | Low | Global |
| **Supabase** | n/a (BaaS) | **Pro $25/mo** (8GB disk, 100k MAU) | 7-day daily backups; **PITR is a $100/mo add-on** | Lowest | AWS EU `[U]` |

### The Cloudflare Workers verdict — a clear "no" for you

`workers-rs` genuinely supports axum via its `http` feature `[V]`. But:
- *"You've got to leave your threaded async runtimes at home; meaning **no Tokio or async_std support**"* `[V]`
- *"All crates in your Worker project must compile to `wasm32-unknown-unknown`"* `[V]`
- README: *"Expect a few rough edges, some unimplemented APIs, and maybe a bug or two"* `[V]`
- Free plan gives **10ms CPU per invocation** `[V]` — unusable.

Your service is axum + **sqlx with `runtime-tokio-rustls`**. That does not compile to wasm32. Porting means rewriting the entire data layer against D1 or Hyperdrive. **Rule it out.**

### Object storage — the decision that actually matters for a backup product

| | Storage /GB-mo | Egress | Free tier |
|---|---|---|---|
| **Cloudflare R2** | **$0.015** (std) / $0.01 (IA) | **Free, no limit** | 10 GB, 1M Class A, 10M Class B |
| **Backblaze B2** | **$0.00695** ($6.95/TB) | Free to **3× avg monthly storage**, then $0.01/GB | Class A/B/C calls free |

B2 is ~2× cheaper on storage; R2 wins on unlimited free egress. **For a backup/restore product, pick R2** — a mass-restore event (or a user re-syncing a new laptop) is exactly the pattern that generates egress bills, and free egress makes that cost risk structurally zero.

**Keep note blobs out of Postgres.** Postgres storage runs $0.10–$0.35/GB-mo; R2 is $0.015. At any real scale that's the difference between a three-figure and a single-figure line item.

---

## 5. Managed Postgres

| Provider | Entry production | Free tier | Storage /GB-mo | PITR | Lock-in |
|---|---|---|---|---|---|
| **Neon** | ~$20 (Launch, usage) | 100 CU-h, 0.5 GB | $0.35 + $0.20 restore | 7d Launch / 30d Scale | Low-Med |
| **Crunchy Bridge** | $35 (Hobby-2) / $18 (Hobby-1) | none | **$0.10** | **10 days, included, minute granularity** | **Lowest** |
| **Supabase** | $30 (Pro+Small) | 500 MB, pauses | $0.125 | **+$100/mo** | Low (DB) / Med-High (platform) |
| **Fly MPG** | $38 (Basic) | none | $0.28 | 10 days, included | Low |
| **AWS RDS** | ~$23 (t4g.small 1-AZ, us-east-1) | **credits only** | $0.115 | 1–35d included | Lowest |
| **Hetzner** | **no managed offering** | — | — | DIY | Lowest |
| **Scaleway** | ~€11–17 | none | €0.099–0.149 | `[U]` | Low |

Key points:

- **PITR is the cost discriminator, and it inverts the obvious ranking.** Supabase Pro + Small compute + PITR = **~$130/mo**, versus **$35** for a Crunchy Bridge Hobby-2 with *better* PITR (10 days, minute granularity, included).
- **Neon's trajectory inverts across scale.** Cheapest at 1k (~$20), competitive at 10k (~$88), **most expensive mainstream option at 100k (~$500)** because Scale tier costs 2.1× Launch per CU-hour. Scale-to-zero stops paying once you have continuous traffic. Cold start is **sub-second, but 300ms–1s in independent measurement** `[S]`, not the "few hundred ms" claimed.
- **Neon region is fixed at project creation and cannot be changed** `[V]`. Only `aws-eu-central-1` and `aws-eu-west-2` in the EU. Pick Frankfurt on day one.
- **Both acquisitions are confirmed.** Databricks/Neon (announced 2025-05-14, ~$1B) has so far *cut* prices — storage $1.75→$0.35/GB `[V]` — but a Vantage analysis argues the cuts come from Databricks' AWS enterprise discounts, i.e. **a revocable parent-company subsidy** `[S]`. Snowflake/Crunchy closed 2025-06-06 for **$164.5M per Snowflake's 10-K** (below the ~$250M press figure); Snowflake Postgres went GA 2026-02-24 with the Crunchy team, while Crunchy Bridge is still sold with no sunset announced and **no Snowflake notice on its docs** `[V]` — benign neglect, not commitment.
- **Aurora Serverless v2 scale-to-zero shipped (2024-11-20) but resumes in ~15 seconds** `[V]`. AWS itself frames it for dev/test. Disqualifying for a desktop app waking from sleep.
- **AWS's 12-month free tier was sunset 2025-07-15** `[S]`; new accounts get $100 credits + up to $100 more, expiring in 12 months. **AWS's own RDS pricing page still advertises the retired free tier** `[V]` — a useful reminder that vendor pages can be 14 months stale.

**Recommendation: Neon Launch (Frankfurt) now, re-run the numbers around 10k users, expect to move to Crunchy Bridge or Scaleway.** Both ends are stock Postgres, so it's a `pg_dump`/`pg_restore` — a deliberately cheap option to hold.

---

## 6. Payments

### Rates (all `[V]` today)

| Option | Rate | MoR? | Net on $5/mo |
|---|---|---|---|
| **Stripe direct** | 2.9% + 30¢ (+1.5% intl, +1% FX); Billing 0.7%; Tax 0.5% | **No** | **~$4.50** but *you* register/remit VAT |
| **Stripe Managed Payments** | **+3.5%** on top of standard ⇒ ~6.4% + 30¢ | **Yes** — files and remits in 80+ countries | **~$4.38** |
| **Paddle** | 5% + 50¢ | Yes | **$4.25** |
| **Lemon Squeezy** | 5% + 50¢ | Yes | **$4.25** |
| **Polar** | 5% + 50¢ (Starter); 3.8% + 40¢ (Pro, $20/mo); +1.5% intl; $15/dispute | Yes | **$4.25** |
| **Apple IAP** | 30%, or **15%** under Small Business Program (<$1M/yr proceeds) | Yes | **$4.25** at 15% |

**Two flags:**

1. **Paddle requires custom pricing for products under $10** `[V]`. A $5/mo subscription may not get standard rates. Verify before designing around it.
2. **Lemon Squeezy's pricing page carries a banner: "2026 Update: Lemon Squeezy + Stripe Managed Payments"** (post dated 2026-01-28) `[V]`. No sunset notice, still accepting sellers, still 5%+50¢. But Stripe acquired it in 2024 and has now shipped its own MoR product — **I could not retrieve the announcement body** `[U]`. Treat Lemon Squeezy as strategically uncertain; if you want Stripe's rails with MoR, use Managed Payments directly.

**Notice that Apple IAP at 15% nets the same as Paddle.** The reason to avoid the Mac App Store isn't the commission — it's Guideline 4.8, in-app account deletion (5.1.1(v)), review latency, and Tauri sandboxing pain. External purchase links: the guidelines now say entitlements *"are not required for developers to include buttons, external links, or other calls to action in their **United States storefront** apps"* `[V]`, but the StoreKit External Purchase Link Entitlement is *"limited to use only in the iOS or iPadOS App Store"* `[V]` — so **the Mac App Store's position on external links is genuinely unclear to me** `[U]`.

**Distributing outside the Mac App Store avoids all of it.** Recommended.

---

## 7. Compliance and privacy

### GDPR minimum (all article text `[V]` from gdpr-info.eu)

- **Art 30(5) will not save you.** The under-250-employees exemption lapses if processing "is **not occasional**" — a running sync service is by definition not occasional. **Keep a ROPA.**
- **Art 12(3):** respond to access/erasure requests "without undue delay and in any event **within one month**", extendable by two months with notice.
- **Art 33:** notify the supervisory authority within **72 hours**.
- **Art 27:** a US company with no EU establishment serving EU users **needs an EU representative** — the derogation is for "occasional" processing, which again won't apply. **Cost unverified: Prighter returned HTTP 429 twice, datarep.com/pricing 404s** `[U]`. Budget a few hundred EUR/yr and confirm.
- **DPO:** almost certainly not required for this profile (Art 37 triggers are public authorities, large-scale systematic monitoring, or large-scale special-category data).
- **EU-US DPF: still in force.** The European Commission's own adequacy page, fetched today, lists "United States (commercial organisations participating in the EU-US Data Privacy Framework)" with no suspension noted `[V]`. UK adequacy renewed December 2025 `[V]`. I did **not** verify the Latombe challenge outcome `[U]`.

### Does E2EE materially reduce obligations? Partly — and less than people claim

**The one concrete, verified win is Art 34(3)(a)** `[V]`: no obligation to notify *data subjects* of a breach where the controller has applied measures "that render the personal data unintelligible to any person who is not authorised to access it, **such as encryption**." Note this does **not** exempt you from notifying the supervisory authority under Art 33.

**What E2EE does not do:** you still process email addresses, billing records, IP addresses, device IDs, timestamps, and blob sizes — all plainly personal data. You remain a controller with full Art 15/17/30/33 obligations. **E2EE shrinks the blast radius and the notification exposure; it does not change your status.**

On whether E2EE ciphertext is itself personal data in your hands: the governing authority is **CJEU Case C-413/23 P (EDPS v SRB)**, the appeal of General Court T-557/20, which I believe was decided **4 September 2025** and adopted a *relative* test — whether data is personal depends on the means reasonably likely to be available **to the particular recipient**. ⚠️ **I could not verify this in this session** — eur-lex returned empty pages for CELEX 62023CJ0413 in four URL forms, curia.europa.eu 404'd, EDPS returned 403, and gdprhub is behind Anubis. **This is model knowledge, not verified research. Get a lawyer's read before relying on it.**

### Encryption policy climate — relevant if you bet the roadmap on E2EE

- **EU "Chat Control" (CSAR):** mandatory client-side scanning was **removed from the Council's mandate in November 2025**; trilogues ran from 2025-12-09 through mid-2026 with adoption expected ~July 2026 `[S]`. **My only source is patrick-breyer.de, an advocacy site, and it predicts an event that should already have happened by today — I could not confirm whether the regulation was actually adopted.** `[U]` Verify against Council/Parliament sources.
- **UK:** Apple received a Technical Capability Notice **2025-02-07**, withdrew Advanced Data Protection for UK users **2025-02-21**, and appealed to the Investigatory Powers Tribunal in **early March 2025** `[V]`. **Everything after March 2025 is unverified** `[U]` — I could not establish whether the notice was withdrawn or ADP restored.

### Account recovery when data is E2EE — four real patterns

| Pattern | Example | Tradeoff | Two-person team? |
|---|---|---|---|
| **1. Unrecoverable passphrase** | **Obsidian Sync** — separate encryption password, AES-256/scrypt/GCM. Verbatim: *"if you forget or lose your encryption password, your data remains encrypted and unusable forever. We're not able to recover your password, or any encrypted data for you."* `[V]` | Simplest, most honest, zero server-side key material | **Yes — ship this** |
| **2. Recovery key issued at setup** | **1Password** Secret Key + Emergency Kit PDF. *"We don't have a copy of your Secret Key or any way to recover or reset it for you."* Business orgs get admin recovery `[V]` | Users lose the PDF | Yes |
| **3. Split account-recovery vs data-recovery** | **Proton** — recovery *phrase* restores password **and** data; recovery *file* restores data only; recovery email/phone restores password only. *"If you have a password reset method and no data recovery method, you'll lose access to everything that was on your account before the password reset"* `[V]` | Most complete; conceptually hard to explain | Yes, with care |
| **4. Enclave-escrowed secret behind a low-entropy PIN** | Signal SVR (HSM/SGX, guess-limited) | Best UX by far | **No — out of reach.** Requires operating attested secure hardware |

**The insight that makes this easy for Lattice:** because you are **local-first**, losing the sync passphrase loses the *backup*, not the *notes*. Obsidian makes exactly this point — local copies stay safe; you reset the remote vault and re-upload. That reframes pattern 1 from "catastrophic" to "annoying", and it's the honest, shippable answer. **Say it in plain language at setup, not in a tooltip.**

---

## 8. The no-accounts alternative: bring your own storage

### What each backend actually costs you

| Backend | Gate | Verdict |
|---|---|---|
| **Google Drive** | **`drive.file` and `drive.appdata` are NON-sensitive** ⇒ basic OAuth verification only, **no CASA** `[V]`. The six restricted scopes (`drive`, `drive.readonly`, `drive.metadata`, …) require restricted-scope verification with an annual third-party assessment `[V]` | **Viable** — stay on `drive.file`. This is the single most important BYOS finding. |
| **Dropbox** | Dev mode caps at 500 users; **at 50 linked users you have two weeks to get production approval** or new links freeze `[V]`. App folder vs Full Dropbox — use App folder | Viable, with a hard approval gate early |
| **iCloud / CloudKit** | Apple advertises "iOS, iPadOS, macOS, tvOS, watchOS, visionOS **and the web**" `[V]`. CloudKit Web Services exists but requires an Apple ID sign-in in a web context | **Kills cross-platform parity.** Impractical for Windows/Linux `[U]` on the details |
| **S3-compatible (user's own bucket)** | User pastes endpoint + access key + secret | Zero gatekeeping, worst UX |
| **WebDAV/Nextcloud** | None | Zero gatekeeping, heterogeneous behaviour |

⚠️ **CASA dollar figures are unreliable.** Google explicitly does not set the price — *"agreed on between the developer and the assessor without any involvement from Google"* `[V]`. Circulating figures range from $500 to $75,000+. Don't quote any of them; just stay on `drive.file`.

### What BYOS saves and costs

**Saves:** no servers, no storage bill, no PITR to manage, minimal breach blast radius, and a much simpler compliance story — though **whether a client-side-only vendor escapes processor status entirely is a genuine open question I did not verify** `[U]`. You'd still process account/licence/telemetry data regardless.

**Costs:** credential-entry friction; **conflict resolution with no server to arbitrate** (which your `conflicts` table currently assumes); no server-side versioning or dedup; no cross-device discovery; no share links; support burden multiplied across N backends that each fail differently; and **you still need some identity mechanism to license a paid tier.**

**The honest comparison point:** Obsidian ships *both*. Free users bring their own storage (iCloud/Dropbox/Syncthing); paying users get **Obsidian Sync at $4/user/mo annual (1 GB, 1-month history) or $8/mo (10 GB, 12-month history)** `[V]`. That's not a hedge — it's the business model. BYOS is the free tier's problem; hosted sync is what people pay for. **If you want revenue from sync, BYOS is the thing you offer instead, not the thing you sell.**

---

## 9. What I could not verify — check these before acting

Ordered by how much they could change a decision:

1. **CJEU C-413/23 P holding and date** — eur-lex, curia, EDPS and gdprhub all unreachable. The most load-bearing legal claim in the report, and it is **model knowledge only**.
2. **Hetzner VPS pricing** — sources conflict 2× on the same SKU; the pricing page is client-rendered. Pull it in a browser.
3. **Art 27 EU representative pricing** — Prighter 429'd twice, DataRep 404'd.
4. **EU Chat Control actual status** — single advocacy source, predicting an event that should already have occurred.
5. **UK Apple ADP status after March 2025** — nothing found.
6. **Render instance pricing** — client-rendered, 3 attempts.
7. **Mac App Store external-purchase-link position** — guidelines scope the entitlement to iOS/iPadOS but carve out "United States storefront apps" generally.
8. **Lemon Squeezy's 2026 direction** — banner found, announcement body not retrieved.
9. **Stytch per-MAU overage** — not published anywhere I could reach.
10. **Supabase/DO/Railway PITR specifics and EU region lists** — absent from their pricing pages.
11. **Apple's contradiction** on whether SIWA web flow needs a *published App Store app* or merely an App ID.
12. **Latombe v Commission outcome** — DPF confirmed in force, but the challenge's status is unknown to me.

**One meta-warning worth repeating:** AWS's own RDS pricing page still advertises a free tier that was retired 14 months ago. Treat every number here as a starting point for a quote, not a quote.

---

## 10. Concrete next steps

1. **Decide E2EE now**, before touching `0001_sync_init.sql`. If yes: `content` becomes an opaque blob in R2, `title`/`path` become ciphertext or hashes, and `conflicts.resolution_content` has to go.
2. **Replace `user_id_from_headers`** with `axum-jwt-auth` 0.7.0 validating against a JWKS. This is the single highest-severity item in the repo.
3. **Spike WorkOS AuthKit + loopback PKCE** end-to-end on all three platforms before committing. Their docs confirm `127.0.0.1` for native clients, but desktop is not a documented first-class path for any hosted vendor.
4. **Get a real Developer ID certificate** and drop `"signingIdentity": "-"` — it's blocking both notarization and stable keychain access.
5. **Swap `tauri-plugin-shell`'s `open` for `tauri-plugin-opener`** (2.5.5) — you're not exposed to the CVE at 2.3.4, but `shell.open` is deprecated for this purpose.
6. **Bump `keyring` 3.6.3 → 4.x** with the `v1` feature, and store only the refresh token.
7. **Pick Frankfurt** on Neon at project creation — it's irreversible.

---

Both remaining agents are still running despite wrap-up requests. I have independent coverage of their topics, so here is the report.

---

# Lattice: Accounts, Auth, Hosting & Ops for Optional Cloud Backup/Sync
**Research date: 2026-09-16.** Confidence markers: `[V]` verified on a primary/vendor page this session · `[S]` secondary source · `[U]` unverified or conflicting.

> **Two caveats up front.** (1) This session's WebSearch budget was exhausted early (200/200), so later work was done via direct WebFetch on guessed URLs plus the `gh` CLI. Coverage of forums/Reddit/blog anecdotes is thin; vendor-primary-doc and GitHub coverage is strong. (2) Two sub-researchers (hosting breadth, compliance depth) had not returned when I wrote this; I re-verified their topics myself, but the compliance section has real gaps I flag explicitly in §6.

---

## 0. Executive summary

### The finding that should reorder your plan

Before any vendor decision, note what is already in the repo:

- `api-rust/src/http/routes/sync.rs` derives identity from a **plaintext `x-user-id` header**. `user_id_from_headers` parses an integer and trusts it. Any client can impersonate any user by setting a header. This is the exact seam where auth goes, and it must not ship.
- `api-rust/migrations/0001_sync_init.sql` stores `title`, `content`, and `path` as **plaintext `TEXT`**. There are **no crypto crates** in either `Cargo.toml`. The current prototype is a plaintext server-side note store — the opposite of the privacy posture the product markets.
- `conflicts.resolution_content TEXT` implies **server-side conflict resolution on note content**, which is fundamentally incompatible with E2EE.

**The E2EE decision must be made before you freeze the sync wire format**, because it changes the schema, deletes server-side merge, and determines the entire account-recovery UX. Retrofitting E2EE after launch means a migration of every user's data plus a breaking protocol change.

Two more repo facts that bear on ship-readiness: `tauri.conf.json` has `"signingIdentity": "-"` (**ad-hoc signing**, so no notarization and unstable macOS Keychain ACLs across rebuilds), and the legacy Python auth at `src/api/src/auth/jwt.py` uses **shared-secret JWT signing** with no JWKS — so any move to a hosted IdP is also a move from HS256 to RS256/ES256 + JWKS verification.

### Recommended minimal stack (1–2 people)

| Layer | Pick | Monthly |
|---|---|---|
| Identity | **WorkOS AuthKit** — loopback + PKCE, public client | **$0** (free to 1M MAU) |
| App compute | **Fly.io** `shared-cpu-1x` 512MB, Frankfurt/Amsterdam | **~$3.50** |
| Database | **Neon Launch** (Frankfurt) or **Crunchy Bridge Hobby-1** | **~$18–20** |
| Encrypted blobs | **Cloudflare R2** (zero egress) | **$0–5** |
| Transactional email | **Resend** free tier (3,000/mo) | **$0** |
| Payments | **Paddle** or **Stripe Managed Payments** (MoR) | % only, no fixed |
| Apple Developer Program | required for notarized DMG | **~$8** ($99/yr) |
| GDPR Art 27 EU rep | required if you're US-based serving EU users | ~$8–25 `[U]` |
| **Total at launch** | | **≈ $40–60/mo** |

**Why WorkOS AuthKit.** It is free to 1,000,000 MAU `[V]`, it supports PKCE for public clients with no client secret `[V]`, and — the load-bearing detail — its docs explicitly carve out **`http://127.0.0.1` as allowed in production for native clients** `[V]`. That last point is what makes it usable from a desktop app at all; most competitors do not document desktop support. It is standard OIDC with a JWKS endpoint, so swapping it out later is a config change, not a rewrite.

**The honest counter-argument.** For a privacy-positioned product, adding a US identity vendor as a sub-processor holding every user's email is a real cost, and a company giving away 1M MAU is monetizing elsewhere (enterprise SSO at $125/connection) — that free tier is a customer-acquisition subsidy and is revocable. The alternative is a **minimal self-issued email magic-link + device-token scheme in the axum service you already have**. Your account is genuinely thin: identify a user, bind devices, gate a paid tier, hold an E2EE-wrapped key. You do not need orgs, roles, SAML, or MFA. If the privacy story is the product, build it yourself and own ~400 lines. I recommend WorkOS as the default only because auth is where solo teams get owned, and it buys you out of that risk for $0.

**Do not self-host an IdP.** Keycloak ships a release every 2–4 weeks (26.6.2 → 26.7.3 was six releases in ~3.5 months) `[V]`, and authentik cut 2026.8.2 / 2026.5.7 / 2026.2.7 simultaneously on 2026-09-09 — a coordinated three-branch security release `[V]`. That is a patching treadmill a two-person team cannot afford for a component whose downtime means nobody can log in.

---

## 1. Identity options

### 1.1 Pricing at 1k / 10k / 100k MAU

| Provider | Free tier | 1k | 10k | 100k | Notes |
|---|---|---|---|---|---|
| **WorkOS AuthKit** | **1M MAU** | **$0** | **$0** | **$0** | $2,500/mo per extra 1M. Custom domain $99/mo. SSO $125/connection. `[V]` |
| **Clerk** | 50,000 **MRU** | $0 | $0 | ~$1,025 | Pro $25/mo incl. 50k MRU, then $0.02. **MRU ≠ MAU** — only counts if the user returns ≥24h after signup. `[V]` |
| **Supabase Auth** | 50,000 MAU | $0 (or $25 Pro) | $25 | **$25** | Pro includes 100k MAU; $0.00325/MAU after. SAML priced separately at $0.015/MAU. `[V]` |
| **Firebase / GCIP** | 50,000 MAU | $0 | $0 | ~$275 | $0.0055/MAU in the 50k–100k band, stepping down. SAML/OIDC $0.015/MAU. Phone/SMS never free. `[S]` |
| **Kinde** | 10,500 MAU | $25 | $25 + ~$0 | ~$1,600 | Pro $25/mo, $0.0175/extra MAU. **Passkeys not on the free plan.** `[V]` |
| **Stytch** | 10,000 MAU | $0 | $0 | `[U]` | Overage rate not published. $125/SSO connection; $99 one-time to remove branding. `[V]` |
| **Auth0** | 25,000 MAU | ~$70 | ~$700 | not self-serve | B2C Essentials $35/mo base for 500 MAU, $0.07/MAU overage. B2B is 3–4× worse. `[S]` — figures came from a calculator-style page, treat as approximate. |

Self-hosted / other:

| Option | License | Cost | Reality check |
|---|---|---|---|
| **Keycloak** | Apache-2.0 `[V]` | VPS only | 4GB min, 8GB for production clustering; JVM startup 60–90s. Release every 2–4 weeks. `[S]` |
| **Zitadel** | **AGPL-3.0 from v3** `[V]` | Cloud: free 100 DAU; Pro **$100/mo** incl. 25,000 DAU `[V]` | App itself only ~512MB RAM, but needs 4 CPU cores for password hashing and recommends 3-node HA. `[V]` The v2→v3 Apache→AGPL relicense upset downstream adopters — a flag if you ever modify it. |
| **authentik** | Mixed (core MIT + enterprise) `[V]` | Self-host free; Enterprise **$5/user/mo**, external users $0.02/mo | **No hosted option offered** — "We do not currently provide a hosted version of authentik." `[V]` |
| **Ory** | Apache-2.0 core | Network: Production **$770/yr** + **$0.14/aDAU/mo** `[V]` | Prices on **aDAU**, not MAU. Developer tier has **0 production environments.** |

### 1.2 Rust / JWKS verification story

This is where Lattice's existing stack constrains you. `api-rust` is on **axum 0.8**.

| Crate | Version | Published | axum | Verdict |
|---|---|---|---|---|
| `jsonwebtoken` | **11.0.0** | 2026-07-24 | n/a | 189M downloads. The foundation. `[V]` |
| **`axum-jwt-auth`** | **0.7.0** | 2026-05-19 | **^0.8** | **Best fit.** Remote JWKS, automatic caching, hourly background refresh, retry, graceful shutdown. `[V]` |
| `axum-jwks` | 0.12.0 | 2025-06-06 | ^0.8 | Works, older `jsonwebtoken ^9`. `[V]` |
| `jwt-authorizer` | 0.15.0 | 2024-08-27 | **^0.7** | **Incompatible with your axum 0.8.** Two years stale. Still 499k recent downloads — people are on old axum. `[V]` |
| `openidconnect` | 4.0.1 | 2025-07-06 | — | Full OIDC client. `[V]` |
| `oauth2` | 5.0.0 | 2025-01-21 | — | 50M downloads. `[V]` |
| `webauthn-rs` | 0.5.5 | 2026-04-30 | — | 6.8M downloads, from the Kanidm team. Production-grade. `[V]` |

**Migration note:** your legacy Python auth signs with a shared secret (`settings.jwt_secret_key`, separate access/refresh secrets) `[V]`. Every hosted IdP issues asymmetric RS256/ES256 verified via JWKS. That is a genuine change, not a swap — but it's the right direction regardless, since it removes the secret from the resource server entirely.

### 1.3 Passkeys on desktop — the awkward truth

**In-webview passkeys are not viable cross-platform in 2026.** Tauri issue [#7926](https://github.com/tauri-apps/tauri/issues/7926) has been open since 2023-09-30, is still labelled `needs triage`, and last saw activity 2026-04-07 `[V]`. From the thread:

- **Windows 11 / WebView2:** works `[V]`
- **macOS / WKWebView:** requires the Associated Domains entitlement plus `com.apple.developer.web-browser.public-key-credential`; contributors reported the app failing to open once entitlements were set `[V]`
- **Linux / WebKitGTK:** "seemed to lack some WebAPI to do the authentication" `[V]`

The community plugin `Profiidev/tauri-plugin-webauthn` (MIT, 20 stars, last push 2026-09-01, last tagged release v0.2.0 in May 2025) supports Linux, Windows, and Android — and **explicitly does not support macOS** `[V]`. For a macOS-first product that is disqualifying.

**Resolution:** do passkeys in the **system browser**, where they work natively on all three platforms. This is the same place you already need to be for OAuth (§2), so it costs nothing architecturally.

### 1.4 Lock-in

| Provider | Exit story |
|---|---|
| **Supabase** | Lowest for data (it's your Postgres), but auth stickiness is high — `auth.users`, RLS policies against `auth.uid()`, PostgREST conventions. |
| **Clerk** | Better than its reputation: Dashboard → Settings → **"Export all users"** gives CSV **including password hashes**; supported hashers include bcrypt, argon2i/id, pbkdf2, scrypt, phpass. WorkOS even publishes a `migrate-clerk-users` tool. `[S]` |
| **WorkOS / Auth0 / Stytch / Kinde** | Standard OIDC; the real lock-in is your session model and authorization code, not the user records. |
| **Zitadel** | AGPL-3.0 since v3 — fine for running unmodified, a legal question if you fork. |

---

## 2. How Tauri 2 apps concretely do login

*(This section is from the completed Tauri research track, spot-checked by me against the repo.)*

### 2.1 The decision: loopback, not deep links

**Use `http://127.0.0.1:<ephemeral port>/callback` opened in the system browser, with PKCE.** Reasons:

**Google no longer supports custom URI schemes for desktop at all.** From Google's native-app OAuth docs `[V]`: *"Custom URI schemes are no longer supported due to the risk of app impersonation."* Loopback is the documented, recommended mechanism for macOS/Linux/Windows desktop, and the loopback deprecation applies **only to mobile** client types. The out-of-band (`urn:ietf:wg:oauth:2.0:oob`) flow is dead. Prefer `127.0.0.1` over `localhost` — Google warns the latter "may cause issues with client firewalls."

**RFC 8252 is normative and blunt** `[V]`:
- *"native apps MUST NOT use embedded user-agents to perform authorization requests"*
- *"Public native app clients MUST implement the Proof Key for Code Exchange (PKCE)"*
- *"The authorization server MUST allow any port to be specified at the time of the request for loopback IP redirect URIs"*
- RFC-conformant private-use schemes are reverse-DNS: `tech.lattice.app:/callback`, **not** `lattice://callback`. (Your identifier is already `tech.lattice.app` `[V]`.)

**Google bans embedded webviews by policy** `[V]` (policy page last modified 2026-08-05): *"A developer must not direct a Google OAuth 2.0 authorization request to an embedded user-agent under the developer's control."* Tauri's webview **is** WKWebView on macOS and WebView2 on Windows — precisely the class named.

### 2.2 The Linux landmine

**This is the single most important operational finding.** [tauri#15928](https://github.com/tauri-apps/tauri/issues/15928), opened 2026-08-28, **still open**: the Linux bundler writes `MimeType=x-scheme-handler/<scheme>` but renders `Exec={{exec}}` **without the `%u` field code**. Per the freedesktop spec, such an entry launches without the URL. The reporter's words: *"For OAuth/OIDC flows this means the callback never reaches the app, and sign-in cannot complete through the system handler."*

It is masked in dev because `register_all()` writes a *different* `.desktop` file that does include `%u`. The fix PR [#15929](https://github.com/tauri-apps/tauri/issues/15929) is open; the researcher fetched `main.desktop` from `dev` today and confirmed the line is still `Exec={{exec}}` `[V]`. Related: [plugins-workspace#2265](https://github.com/tauri-apps/plugins-workspace/issues/2265) — `update-desktop-database` may not exist on KDE systems, open since 2025-01-06.

**Loopback sidesteps all of this**, plus the cold-start race (your Rust listener is already running before you open the browser).

### 2.3 Plugin inventory vs. what Lattice has

| Plugin | Latest | Lattice has |
|---|---|---|
| `tauri-plugin-deep-link` | 2.4.10 (2026-08-31) | ❌ |
| `tauri-plugin-single-instance` | 2.4.4 (2026-08-31) | ❌ |
| `tauri-plugin-oauth` (FabianLars) | 2.1.0 (2026-07-07), 215★ | ❌ |
| `tauri-plugin-opener` | 2.5.5 (2026-08-31) | ❌ |
| `tauri-plugin-shell` | — | ✅ resolves **2.3.4** |

Good news: shell 2.3.4 is past **CVE-2025-31477** / [GHSA-c9pr-q8gx-3mgp](https://github.com/tauri-apps/plugins-workspace/security/advisories/GHSA-c9pr-q8gx-3mgp) (High, published 2025-04-02, patched in 2.2.1) `[V]`. Still, `shell.open` is deprecated in favour of `tauri-plugin-opener`.

If you do add deep links: **single-instance must be registered first** (docs: *"The Single Instance plugin must be the first one to be registered to work well."*), and note macOS **cannot register schemes at runtime at all** — `register()` returns `UnsupportedPlatform`, so you must build and install a real `.app` to test. `tauri-plugin-oauth` is thinly maintained (eight of the last ten commits are Renovate bumps; a client-disconnect panic was fixed only in May 2026; its Tauri-v3 compatibility issue #50 is open) — it's ~300 lines you should be prepared to vendor.

### 2.4 Sign in with Apple — you probably don't need it

**Current Guideline 4.8 text, verified today** `[V]`. It no longer names Sign in with Apple. It requires an "equivalent" service limiting collection to name+email, allowing the user to keep their email private, and not tracking for ads. Critically, it lists exemptions, the first being:

> *"Another login service is not required if: Your app exclusively uses your company's own account setup and sign-in systems."*

**So: if Lattice offers only its own email/magic-link/passkey login and no Google/GitHub social sign-in, Guideline 4.8 does not apply.** That is a strong argument for skipping social login entirely — which also matches the privacy positioning. (Note the trap if you *do* add Google: your own plain email login would not satisfy the second bullet, since it can't keep the user's address private without an email-relay feature.)

Guideline 4.8 **does** apply to the Mac App Store. But **a notarized direct-download DMG is not reviewed against the guidelines at all** — so if you distribute outside the Mac App Store, none of this binds you.

Apple also makes SIWA technically incompatible with your flow. From Apple's token-validation docs `[V]`: the redirect URI *"must use the HTTPS protocol, include a domain name, and can't contain an IP address or `localhost`."* No loopback, no custom scheme. Apple's own documented pattern is IdP → your HTTPS server → 302 to a custom scheme — i.e. you must run a server regardless. Other costs: $99/yr program membership, a Services ID, SPF/DKIM domain verification for the private email relay, and a **client secret JWT capped at 15,777,000 seconds (six months)** that must be rotated or every login fails with `invalid_client` on expiry day. You may hold at most two private keys per app, which is what makes zero-downtime rotation possible.

One resolved contradiction: Apple's REST docs say you need *"an existing app in the App Store"*, but the help page says only *"an existing primary iOS, macOS, tvOS, or watchOS App ID enabled for Sign in with Apple"* `[V]`. The help page is the accurate one — an App ID, not a published listing.

Also flag **Guideline 5.1.1(v)**: if your app supports account creation, you must offer **in-app account deletion** `[V]`. Build that from day one; retrofitting it into a sync backend is unpleasant, and GDPR Art 17 wants it anyway.

### 2.5 Google sign-in specifics

Three findings that all favour keeping scopes minimal (`openid email profile`):

1. **No CASA.** Restricted-scope verification requires an annual third-party security assessment; `email`/`profile` are non-sensitive and require only basic verification. Google explicitly does not set assessor prices, and the figures circulating online range from $500 to $75,000+ — **don't quote any of them** `[U]`.
2. **No 100-user cap.** The cap applies *only to apps that display the unverified-app screen*, which is triggered by sensitive/restricted scopes `[V]`. Brand verification is still needed for a public app: automated review in minutes, manual review 2–3 business days, and you must verify domain ownership in Search Console and host your privacy policy on the same domain as your homepage `[V]`.
3. **No 7-day refresh-token expiry in testing mode.** Google's docs `[V]`: testing-mode projects get 7-day refresh tokens *"unless the only OAuth scopes requested are a subset of name, email address, and user profile."* Exactly your case.

Two limits to design around: **100 refresh tokens per Google Account per client ID** (creating the 101st silently invalidates the oldest), and refresh tokens die after **six months unused** `[V]`. Weeks offline is fine; months is not.

### 2.6 Keyring storage — three real pitfalls

Lattice is on `keyring` **3.6.3**; latest is **4.2.0** (2026-08-29) `[V]`. The crate was restructured in 4.0 into `keyring-core` plus per-backend crates, but `keyring = "4"` with the `v1` feature keeps the old API — that's the right call for a small team.

**Windows: the cap is 2,560 bytes, not 512.** From Microsoft's `CREDENTIALA` reference `[V]`: `CredentialBlobSize` *"cannot be larger than CRED_MAX_CREDENTIAL_BLOB_SIZE (5\*512) bytes."* A refresh token fits easily; a **bundle** of ID + access + refresh token as one JSON blob does not — Entra access tokens alone run 2–4KB. **Store only the refresh token; keep access and ID tokens in memory.**

**macOS: the folk wisdom is wrong in a way that matters to you.** Per Apple DTS `[V]`, the keychain ACL binds to the **designated requirement** (Team ID + bundle ID), not the code hash — *"This happens automatically if you use Developer ID (or Mac App Store) signing."* So normal signed updates do **not** re-prompt. But your `tauri.conf.json` currently sets `"signingIdentity": "-"` (**ad-hoc**) `[V]`, which means every rebuild produces a binary that cannot satisfy a previously-stored item's DR. You will be fighting keychain prompts throughout development, and you cannot notarize for distribution at all. Get a Developer ID identity before building the account feature.

**Linux: don't fall back silently.** Secret Service may be absent (headless, WSL, some Flatpaks); KDE Wallet is UTF-8 only, so binary secrets need base64. The keyutils fallback is documented as *"completely in-memory and will not persist across reboots"* `[V]` — useless for an app that may be closed for weeks. Degrade **loudly** to "you'll need to sign in again" rather than quietly to an insecure store.

**Skip `tauri-plugin-stronghold`.** The wrapper is maintained, but the underlying `iota_stronghold` crate was last published **2024-05-13** and the `iotaledger/stronghold.rs` repo's last commit was **2023-06-29** `[V]`. It also doesn't solve the problem — it's an encrypted file whose key you still have to put somewhere.

---

## 3. Hosting a small axum + Postgres service

*(Verified by me directly; the hosting sub-researcher had not returned.)*

| Platform | 1 small app instance | Managed Postgres | Backups/PITR | Ops burden |
|---|---|---|---|---|
| **Fly.io** | `shared-cpu-1x` 256MB **$2.02**, 512MB **$3.32**, 1GB **$5.92** (AMS); egress $0.02/GB NA+EU `[V]` | Fly MPG Basic **$38** (1GB), storage $0.28/GB `[V]` | **10-day PITR + HA included** on every MPG plan | Low — but docs still list *"security patches and version upgrades"* as under development, a maturity flag. Also: `$72 → $282` cliff with nothing between 2GB and 8GB. |
| **Railway** | Hobby **$5**/Pro **$20** base (credits included); RAM $10/GB-mo, vCPU $20/mo, volumes $0.15/GB-mo, egress $0.05/GB `[V]` | usage-priced | **Undocumented** `[U]` | Low. For a DB you care about, undocumented backups *is* the answer — treat Railway Postgres as a dev convenience. |
| **Render** | prices not retrievable (client-rendered page) `[U]` | — | PITR **3 days** (Hobby workspace) / **7 days** (Pro+); free-tier DBs get **no** automatic logical backups `[V]` | Low. Upgrading does not backfill the recovery window. |
| **Hetzner** | **Pricing unverified** — third-party sources disagree ~2× on the same CX23 SKU (€2.99 vs €5.99) `[U]`. IPv4 €0.50/mo `[V]` | **None. Hetzner does not offer managed Postgres** `[V]` | DIY (pgBackRest + object storage) | **High.** Realistically 8–16h setup then 2–4h/month, spiking for major-version upgrades. At any sane valuation of founder time, €10/mo + 3h/month costs more than a $35 Crunchy instance. Third-party managed-on-Hetzner (Ubicloud from **$19/mo**) is the interesting middle path. |
| **DO App Platform** | **$5** (1 vCPU/512MiB/50GiB transfer), **$10** (1GiB/100GiB); $0.02/GiB overage; free static sites `[V]` | Managed PG from **$15.15** (1GiB) `[V]` | PITR widely assumed 7-day and included — **unconfirmed** `[U]` | Low |
| **AWS Lightsail** | **$3.50** (IPv6-only, 512MB) / **$5** (0.5GB, 1TB transfer) `[V]` | **$15** Standard (1GB), **$30** HA `[V]` | Not stated on pricing page `[U]` | Medium |
| **Cloudflare Workers** | Paid **$5/mo** min: 10M requests + 30M CPU-ms; then $0.30/M req, $0.02/M CPU-ms. Free: 100k req/day but **10ms CPU per invocation** `[V]` | D1: 5GB free then **$0.75/GB-mo**; Hyperdrive **included on Free and Paid** `[V]` | D1 time-travel | **Architecturally disqualifying for you — see below.** |
| **Supabase** | n/a (Postgres-only use) | Pro **$25/mo** `[V]` | 7-day daily backups; **PITR is a $100/mo add-on** `[V]` |  Lowest |

### The Cloudflare Workers verdict

`workers-rs` does support axum via its `http` feature `[V]`. But the README states plainly: *"You've got to leave your threaded async runtimes at home; meaning no Tokio or async_std support"*, and *"All crates in your Worker project must compile to wasm32-unknown-unknown."* Lattice's `api-rust` is built on **tokio + sqlx with `runtime-tokio-rustls`** `[V]`. That does not port. You would rewrite the entire data layer against D1 or Hyperdrive, and the README itself warns to *"expect a few rough edges."* Combined with the 128MB isolate memory limit and the Free plan's 10ms CPU ceiling, **this is a rewrite, not a deployment target.** Rule it out.

### Object storage for encrypted blobs

| | Storage | Egress | Free tier |
|---|---|---|---|
| **Cloudflare R2** | **$0.015**/GB-mo (IA $0.01) | **Free, no limit** `[V]` | 10 GB, 1M Class A, 10M Class B |
| **Backblaze B2** | **$0.00695**/GB-mo ($6.95/TB) | Free to **3× stored**, then $0.01/GB `[V]` | — |

B2 is less than half R2's storage price; R2 wins decisively the moment restores get heavy, and **restores are the entire point of a backup product**. Class A ops on R2 are $4.50/M, so batch your writes. **Keep encrypted note blobs out of Postgres** — Postgres storage runs $0.10–$0.35/GB-mo across every vendor, 15–50× the object-storage rate.

---

## 4. Managed Postgres

*(From the completed Postgres research track.)*

| Provider | Entry production | Free tier | Storage/GB-mo | PITR | EU regions | Lock-in |
|---|---|---|---|---|---|---|
| **Crunchy Bridge** | **$35** (Hobby-2, 2GB) | none | **$0.10** | **10 days, included, 60-second WAL, minute granularity** | Frankfurt, Ireland `[S]` | **Lowest** |
| **Neon** | **~$20** (Launch, usage-based) | 100 CU-h, 0.5GB | $0.35 + $0.20 restore | 7d Launch / 30d Scale | **Frankfurt, London only** | Low–Med |
| **Supabase** | $30 (Pro+Small) / **$130 with PITR** | 500MB, pauses | $0.125 | **+$100/mo** | AWS EU `[U]` | Low (DB) / Med–High (platform) |
| **Fly MPG** | **$38** (Basic) | none | $0.28 | **10 days, included** | fra, ams, lhr | Low |
| **AWS RDS** | **$23.36** (t4g.small, 1-AZ, us-east-1) `[S]` | **credits only** | $0.115 | 1–35d, included | eu-central-1 (~1.15×) `[U]` | **Lowest** |
| **Scaleway** | **~€11–17** (DB-DEV-S) | none | €0.099–0.149 | **unconfirmed** `[U]` | Paris, AMS, Warsaw | Low |
| **Nile** | $15 Pro | 1GB, never pauses | **$1.00** | **"coming soon"** ❌ | `[U]` | Med |

**Three things that should change your default:**

1. **Supabase's PITR is a $100/mo add-on** — 4× its own base plan. A genuinely production-worthy Supabase project with PITR is `$25 + $5 + $100 ≈ $130/mo`, versus $35 for Crunchy Bridge with better (10-day, minute-granularity) recovery. This single line item inverts the usual ranking.
2. **Neon inverts from cheapest to most expensive across scale.** At 1k users, scale-to-zero makes it ~$20. At 100k, Scale's 2.1× compute rate ($0.222 vs $0.106/CU-h) plus $0.35/GB storage puts it around **$500/mo** — the most expensive mainstream option. Plan the migration at ~10k users; it's a `pg_dump` either way. Also: **Neon's region is immutable after project creation** — pick Frankfurt on day one.
3. **Aurora Serverless v2 scale-to-zero shipped but resumes in ~15 seconds** `[V]`. AWS's own framing is that it suits *"applications where a cold start is acceptable."* A desktop app waking from sleep and hitting sync would block for 15s. Disqualifying. Neon's sub-second cold start (vendor claims "a few hundred ms"; independent measurements 300–800ms `[S]`) is the relevant comparison.

**AWS free tier is gone.** The 12-month free tier was sunset **2025-07-15** for new accounts; you now get $100 credits plus up to $100 more, expiring in 12 months, with Free Plan status ending after 6 months `[V]`. Note that **AWS's own RDS PostgreSQL pricing page still advertises the retired 750-hour free tier** `[V]` — if a vendor's own pricing page can be 14 months stale, treat every number here as a starting point for a quote.

**Acquisitions:** Databricks/Neon (announced 2025-05-14, ~$1B) has so far *lowered* Neon prices — storage fell $1.75 → $0.35/GB `[V]` — but that looks like a parent-company AWS-discount subsidy and is revocable. Snowflake/Crunchy (closed 2025-06-06, **$164.5M per Snowflake's FY2026 10-K**, below the ~$250M press reported) produced Snowflake Postgres (GA 2026-02-24) while Crunchy Bridge keeps selling with no sunset announced `[V]`. Crunchy's risk is survivable precisely because its whole identity is "it's just Postgres" — exit is one `pg_dump`.

---

## 5. Payments

| Route | Fee | On a $5/mo sub you net | Who handles VAT/sales tax |
|---|---|---|---|
| **Stripe direct** | 2.9% + $0.30; +1.5% intl; +1% FX; Billing 0.7%; Tax Basic 0.5% `[V]` | **~$4.50 (90%)** | **You.** Stripe is not MoR — registration, filing, remittance are yours. |
| **Stripe Managed Payments** | **+3.5% on top of processing** ≈ 6.4% + $0.30 `[V]` | **~$4.38 (88%)** | Stripe (MoR) — calculation, collection, filing, remittance across 80+ countries |
| **Paddle** | 5% + $0.50 `[V]` | **~$4.25 (85%)** | Paddle (MoR) |
| **Lemon Squeezy** | 5% + $0.50 `[V]` | **~$4.25 (85%)** | LS (MoR) |
| **Polar** | Starter 5% + 50¢; Pro $20/mo → 3.8% + 40¢; +1.5% intl; $15/dispute `[V]` | ~$4.25 / ~$4.41 | Polar (MoR) |
| **Apple IAP (Small Business)** | **15%** if <$1M/yr proceeds `[V]` | **$4.25 (85%)** | Apple |

**Two flags on the MoR options:**

- **Paddle requires custom pricing for products under $10** `[V]`. A $5/mo tier is below their standard threshold — talk to them before designing around it.
- **Lemon Squeezy's pricing page carries a banner: "2026 Update: Lemon Squeezy + Stripe Managed Payments"** (post dated 2026-01-28) `[V]`. It is still accepting new sellers and there is **no sunset notice** — but the direction of travel is clearly into Stripe's own MoR product. I could not retrieve the post body `[U]`. Don't build on LS without reading that announcement.

**The VAT point is what actually decides this.** At $5/mo, the MoR premium is ~$0.25/sub. Registering for and remitting EU VAT OSS yourself as a two-person team costs far more than that in time and risk. **Take the MoR.**

**Mac App Store:** the current guidelines say external purchase links *"are not required to [have] entitlements … in their United States storefront apps"*, and that in all other storefronts **except the US** the prohibition applies `[V]`. But the StoreKit External Purchase Link Entitlement is *"limited to use only in the iOS or iPadOS App Store"* `[V]` — so the Mac App Store situation is genuinely murky `[U]`. **Distributing outside the Mac App Store as a notarized DMG avoids all of it**, and Tauri apps are commonly shipped that way.

---

## 6. Compliance and privacy

### 6.1 What a 1–2 person team actually must do

- **Art 12(3):** respond to data-subject requests *"without undue delay and in any event within one month"*, extendable by two further months with notice `[V]`.
- **Art 30(5) will not save you.** The under-250-employees exemption doesn't apply where *"the processing is not occasional"* `[V]`. A continuously running sync service is not occasional. **Keep a ROPA.**
- **Art 27 EU representative:** required for a non-EU controller under Art 3(2), and the derogation is narrow — it needs processing to be *"occasional … and unlikely to result in a risk"* `[V]`. A US company running sync for EU users needs one. **Vendor pricing unverified** — Prighter returned HTTP 429 twice and DataRep's pricing page 404s `[U]`. Budget a few hundred dollars a year and confirm.
- **DPO:** almost certainly not triggered for this profile (no large-scale systematic monitoring, no special-category data at scale).
- **In-app account deletion:** required by Apple Guideline 5.1.1(v) if you ship to the App Store `[V]`, and effectively required by Art 17 regardless.

**EU-US Data Privacy Framework is still in force** — the Commission's own adequacy-decisions page, fetched today, lists *"the United States (commercial organisations participating in the EU-US Data Privacy Framework)"* with no suspension noted `[V]`. UK adequacy was renewed in December 2025 `[V]`. I did not verify the Latombe (T-553/23) outcome or any appeal `[U]`.

### 6.2 Does E2EE materially reduce obligations?

**The concrete, verifiable benefit is Art 34(3)(a)** `[V]`: you need not notify *data subjects* of a breach where you *"implemented appropriate technical and organisational protection measures … in particular those that render the personal data unintelligible to any person who is not authorised to access it, such as encryption."*

Be precise about what that does and doesn't buy:

- It exempts you from notifying **data subjects**. It does **not** exempt you from notifying the **supervisory authority** under Art 33 within 72 hours.
- It does **not** remove controller obligations. You still process email addresses, billing records, IP addresses, device IDs, timestamps, and blob sizes — all plainly personal data, all subject to access, portability, and erasure rights.
- Whether E2EE *ciphertext itself* is personal data in your hands turns on the "means reasonably likely to be used" test in Recital 26 and on the CJEU's line of pseudonymisation cases — principally **C-413/23 P (EDPS v SRB)**, the appeal from General Court T-557/20. **I could not verify this ruling in this session**: eur-lex returned empty pages for CELEX `62023CJ0413`, curia 404'd, gdprhub is behind an anti-bot wall, and EDPS returned 403 `[U]`. My recollection is that the CJEU ruled on 4 September 2025 and adopted a *relative* test (assessed from the recipient's perspective), but **treat that as unverified model knowledge, not research.** Get a lawyer's read before relying on it.

**Practical framing: E2EE is a risk-reduction and positioning decision, not a compliance-elimination decision.**

### 6.3 Encryption policy climate — verify before betting the roadmap

- **EU CSAR / "Chat Control":** per Patrick Breyer's tracker (**an advocacy source — bias flagged**), mandatory client-side scanning was removed from the Council's mandate in **November 2025**; trilogues ran December 2025 through June 2026; adoption was *expected* around July 2026 `[S]`. **It is now September 2026 and I could not confirm whether it actually passed or what the final text says about encryption** `[U]`. This is a material open question for an E2EE roadmap.
- **UK:** Apple received a Technical Capability Notice on **2025-02-07** and withdrew Advanced Data Protection for UK users on **2025-02-21**, appealing to the Investigatory Powers Tribunal in **March 2025** `[V]`. **Everything after March 2025 is unverified** `[U]` — I could not establish whether the notice was withdrawn or narrowed, or whether ADP is back in the UK.

### 6.4 Account recovery when data is E2EE

Proton draws the distinction that matters most clearly `[V]`: **account recovery ≠ data recovery.**

| Pattern | Example | Tradeoff | Shippable by 2 people? |
|---|---|---|---|
| **1. No recovery** | **Obsidian Sync** — separate encryption password, AES-256/scrypt/GCM. *"if you forget or lose your encryption password, your data remains encrypted and unusable forever."* `[V]` | Simplest, most honest, zero server trust | ✅ Yes |
| **2. Recovery key/kit at setup** | **1Password** Secret Key + Emergency Kit PDF. *"We don't have a copy of your Secret Key or any way to recover or reset it for you."* `[V]` | Users lose the PDF | ✅ Yes |
| **3. Split account vs. data recovery** | **Proton** — recovery phrase (password **and** data), recovery file (data only), recovery email/phone (password only). *"If you have a password reset method and no data recovery method, you'll lose access to everything that was on your account before the password reset."* `[V]` | Most complete; hardest to explain | ⚠️ Doable, high UX cost |
| **4. Enclave-escrowed secret, PIN-gated** | **Signal** SVR | Best UX; requires HSM/SGX infrastructure and audited attestation | ❌ Out of reach |
| **5. Delegated/social** | **Apple ADP** recovery contacts; 1Password Business admin recovery | Needs a second trusted party | ❌ Not for consumer solo use |

**Lattice's structural advantage: you are local-first.** Losing the sync passphrase loses the *backup*, not the *notes* — the local vault is untouched. Obsidian's help page makes exactly this point, recommending users disconnect and create a new remote vault. That means **Pattern 1 (+ an optional Pattern 2 recovery key) is defensible here in a way it wouldn't be for a cloud-primary product.** Say so loudly in the UI at setup.

---

## 7. The no-accounts alternative: bring your own storage

### What it saves

- **Ops:** no servers, no database, no backups, no on-call.
- **Storage cost:** zero.
- **Breach blast radius:** you hold nothing worth stealing.
- **Compliance:** substantially reduced — but **not zero**, and the crux question (whether a vendor whose *client code* touches data but whose servers never receive it is a controller/processor at all) is one I could not research to a citable answer `[U]`. You would still process licence, account, and telemetry data regardless.

### What it costs

**Per-backend friction, verified:**

- **Google Drive — the good news.** `drive.file` and `drive.appdata` are **non-sensitive** scopes requiring only basic OAuth verification `[V]`. The six restricted scopes (`drive`, `drive.readonly`, `drive.metadata`, etc.) are what trigger the annual third-party security assessment. **A notes app that only touches files it created avoids CASA entirely.** This is the single most important BYOS finding.
- **Dropbox — a real cliff.** Development mode caps at 500 users, but **once you reach 50 linked users you have two weeks to apply for and receive production approval** or you can't add more `[V]`. Your app *"will not be reviewed until [it] has linked with at least 50 Dropbox users"* — so you cannot pre-clear it.
- **iCloud — kills cross-platform parity.** Apple advertises CloudKit across *"iOS, iPadOS, macOS, tvOS, watchOS, visionOS and the web"* `[V]`. CloudKit Web Services exists, but driving an Apple-ID web sign-in from a Windows/Linux Tauri app is poor UX, and iCloud Drive ubiquity containers are macOS-only. For a tri-platform app, treat iCloud as a macOS-only convenience, not a sync backend.
- **Generic S3-compatible** (user supplies endpoint + access key + secret): technically trivial, but asking a note-taking user for an access key ID and secret is a conversion cliff.

**The structural costs:** conflict resolution with no server to arbitrate; no server-side versioning or dedup; no cross-device discovery; no share links; and a support surface multiplied by N heterogeneous backends that each fail differently. And you *still* need some identity mechanism to license a paid tier.

### The model that actually works

**Obsidian is your closest analog and it does both.** The free app syncs via whatever the user already has (iCloud, Dropbox, Syncthing); the company also sells **Obsidian Sync at $4/user/mo billed annually** ($5 monthly) for 1 GB and 1-month version history, or $8/mo Plus for 10 GB and 12 months `[V]`. BYOS is the free tier and the escape hatch; first-party sync is the revenue.

**Recommendation: ship BYOS as the free/default path and first-party E2EE sync as the paid tier.** It matches the local-first ethos, it gives privacy-maximalist users a genuine no-trust option, it makes the paid tier a convenience purchase rather than a lock-in, and it means your server is optional infrastructure — which is exactly the compliance posture you want.

---

## 8. What I could not verify

In rough order of how much it could change a decision:

1. **CJEU C-413/23 P (EDPS v SRB)** — the key authority on whether E2EE ciphertext is personal data. eur-lex, curia, gdprhub and EDPS were all unreachable. Get a lawyer's read.
2. **EU Chat Control final status** — my only source is an advocacy site saying adoption was expected July 2026; it's now September 2026 and I can't confirm the outcome.
3. **UK Apple ADP** — nothing verified after March 2025.
4. **Hetzner VPS pricing** — third-party sources disagree by ~2× on the same SKU; the pricing page is client-rendered.
5. **Render instance pricing** — client-rendered, not retrievable.
6. **Art 27 representative pricing** — Prighter 429'd, DataRep 404'd.
7. **Auth0's 1k/10k MAU figures** — from a calculator-style extraction, not a static price table.
8. **Stytch per-MAU overage** — not published.
9. **Crunchy Bridge EU region list** — secondary source only, and it's load-bearing for the 10k+ recommendation.
10. **DigitalOcean and Scaleway PITR** — widely assumed included; unconfirmed.
11. **Mac App Store external-purchase-link applicability** — the US carve-out language and the iOS-only entitlement scope conflict.
12. **CASA assessment cost** — figures online span $500 to $75,000+; all secondary. Irrelevant if you stay on `email`/`profile`.

**A standing warning:** AWS's own RDS pricing page still advertises a free tier that was retired 14 months ago. Treat every number here as the basis for a quote, not a quote.

---

## 9. Suggested sequence

1. **Decide E2EE now**, before touching the sync schema. It is the fork in the road.
2. **Replace the `x-user-id` header** with JWKS-verified bearer tokens via `axum-jwt-auth` 0.7.0.
3. **Get a Developer ID certificate** ($99/yr) and replace `"signingIdentity": "-"`. This unblocks notarization *and* stable macOS Keychain ACLs.
4. **Spike the loopback + PKCE flow** against WorkOS on all three platforms before committing — desktop support is the least-documented part of every hosted IdP.
5. **Upgrade `keyring` 3.6.3 → 4.x** and store only the refresh token.
6. **Stand up Neon Frankfurt + R2 + Fly**, and write the `pg_dump`-to-independent-storage cron on day one. Restore-test it quarterly; three of the vendors surveyed have undocumented backup behaviour.

---

The compliance agent is still running; I have enough independent coverage to deliver. Here is the report.

---

# Accounts, Auth, Hosting & Ops for Lattice Cloud Backup/Sync
**Research date: 2026-09-16.** Confidence markers: `[V]` verified on a primary source this session · `[S]` secondary source · `[U]` unverified / conflicting / possibly stale.

> **Method caveat up front:** this session's shared WebSearch budget (200 calls) was exhausted partway through by parallel research tracks. Later work was done via direct WebFetch on known URLs and the `gh` CLI. Several pricing pages (Hetzner, Render) render client-side and resisted fetching. Everything unresolved is listed in §10.

---

## 0. Executive summary

### The finding that should reframe the whole project

Before any vendor decision, look at what's already in the repo. `api-rust/` is a working axum 0.8 + sqlx 0.8 + Postgres sync service with device registration, push/pull, checkpoints and conflict resolution. Two things about it are load-bearing:

**1. There is no authentication at all.** `api-rust/src/http/routes/sync.rs:87` derives user identity like this:

```
fn user_id_from_headers(headers: &HeaderMap) -> AppResult<i64> {
    let value = headers.get("x-user-id")...
```

Any client can become any user by setting an integer header. That is fine as a prototype seam and catastrophic as a shipped artifact. It is also exactly where a JWT-verifying middleware layer drops in.

**2. The schema is the opposite of end-to-end encrypted.** `api-rust/migrations/0001_sync_init.sql` stores `path TEXT`, `title TEXT`, `content TEXT` in `document_heads` and `document_ops`, plus `conflicts.resolution_content TEXT`. There are no crypto crates in either `Cargo.toml`. The server currently reads users' notes in plaintext and performs server-side conflict resolution on content.

**The E2EE decision must be made before you freeze this schema and wire format**, because `conflicts.resolution_content` implies server-side merge, and server-side merge is incompatible with E2EE. This is the single highest-leverage architectural choice in the whole brief, and it cascades into compliance exposure (§8), recovery UX (§8.4), and whether BYOS (§9) is even a meaningful alternative.

Also grounded from the repo: Tauri pinned at `=2.9.5`, identifier `tech.lattice.app`, `keyring 3.6.3` (current is 4.2.0), `tauri-plugin-shell 2.3.4` (safely past CVE-2025-31477), only the `shell` plugin registered, and **`signingIdentity: "-"` — ad-hoc signing**, which will cause macOS Keychain re-prompts on every rebuild and blocks notarized distribution entirely. There is also a legacy Python/FastAPI auth module at `src/api/src/auth/` using HS256 shared-secret JWTs with bcrypt — so the team has already rolled its own auth once, and the Rust rewrite hasn't carried it over.

### Recommended minimal stack

| Layer | Pick | $/mo |
|---|---|---|
| **Identity** | **WorkOS AuthKit** — free to 1M MAU `[V]` | **$0** |
| **App + DB** | **DigitalOcean App Platform FRA1** ($12) + **Managed Postgres 2 GiB** ($30.45) | **$42.45** |
| **Blob storage** | **Backblaze B2** EU Central (10 GB free, then $6.95/TB) | **$0–3** |
| **Email** | **Resend** free tier (3,000/mo, 100/day) `[V]` | **$0** |
| **Payments** | **Paddle** or **Stripe Managed Payments** — percentage only | $0 fixed |
| **Apple Developer Program** | required for notarization regardless | ~$8.25 |
| **Total at launch** | | **≈ $55/month** |

**Why WorkOS AuthKit.** It is free to 1,000,000 MAU, then $2,500 per additional million `[V]`. It supports **PKCE for public clients with no client secret**, and — the detail that actually decides it — its docs state that `http://127.0.0.1` is **allowed in production for native clients** `[V]`, which is precisely the loopback redirect a Tauri app needs (§2). It exposes standard OIDC + JWKS, so Rust-side verification is `axum-jwt-auth` 0.7.0 (axum ^0.8, remote JWKS with caching) `[V]`. Because it is standard OIDC, swapping it out later is configuration, not a rewrite.

**Why DigitalOcean over the cheaper option.** Hetzner is ~$29/mo and is EU-only by construction with no CLOUD Act exposure — genuinely better for Lattice's positioning. But it has **no managed Postgres** (six independent first-party checks, all negative `[V]`), so you own pgBackRest, restore drills, and major-version upgrades. DO includes **7-day PITR at no extra charge** `[V]` with ~30 min/month of ops. For a 1–2 person team the honest comparison is *$29 plus your weekends* vs *$42*.

### Five things to internalize

1. **PITR is free almost everywhere and costs $100/mo on Supabase** `[V]`. For a product whose entire promise is "your backups are safe," that inverts the usual "Supabase is cheapest" intuition. Supabase Pro + Small + 7-day PITR is **$132/mo**.
2. **Custom URL schemes are dead for desktop OAuth.** Google states plainly: *"Custom URI schemes are no longer supported due to the risk of app impersonation"* `[V]`. Loopback is the only redirect style that works across Google, GitHub, Microsoft and WorkOS. This settles `tauri-plugin-deep-link` vs `tauri-plugin-oauth` in favour of loopback.
3. **If you offer only your own login, Apple Guideline 4.8 does not apply at all.** Verbatim exemption: *"Another login service is not required if: Your app exclusively uses your company's own account setup and sign-in systems"* `[V]`. Skip Sign in with Apple. And if you ship outside the Mac App Store, none of §7 applies.
4. **Passkeys cannot run inside the Tauri webview on macOS.** Tauri issue #7926 is open and untriaged since Sept 2023, last touched 2026-04-07 `[V]`. The community `tauri-plugin-webauthn` explicitly does **not** support macOS `[V]`. Passkeys work fine in the *system browser* — another point for the browser-based flow.
5. **Don't put note blobs in Postgres.** Postgres storage is $0.10–$0.35/GB-mo; object storage is $0.007–$0.015/GB-mo. Keep Postgres for accounts, devices, sync metadata and blob pointers.

---

## 1. Identity options

### 1.1 Hosted — 2026 pricing at 1k / 10k / 100k MAU

| Provider | Free tier | 1k MAU | 10k MAU | 100k MAU | Passkeys | Lock-in |
|---|---|---|---|---|---|---|
| **WorkOS AuthKit** | **1M MAU** `[V]` | **$0** | **$0** | **$0** | ✅ free | Low |
| **Clerk** | 50,000 **MRU** `[V]` | $0 | $0 | ~$25 + overage | ✅ | Low-Med |
| **Supabase Auth** | 50k MAU (Free), 100k (Pro) `[V]` | $0 / $25 | $25 | $25 | ✅ | Low (DB) / Med-High (platform) |
| **Firebase / GCIP** | 50k MAU `[S]` | $0 | $0 | ~$275 `[S]` | ✅ | Medium |
| **Kinde** | 10,500 MAU `[V]` | $0 | $25 + ~$0 | $25 + ~$1,566 | Pro+ only `[V]` | Medium |
| **Stytch** | 10,000 MAU `[V]` | $0 | $0 | `[U]` not published | `[U]` | Medium |
| **Auth0** | 25,000 MAU `[V]` | **$70** (B2C Essentials) `[V]` | **$700** `[V]` | not self-serve `[V]` | ✅ all tiers | Medium |

**Clerk's "MRU" is not MAU.** Their own definition: *"A user only counts as retained if they return to your app at least 24 hours after signing up"* `[V]`. Pro is $25/mo with 50,000 MRU included, then $0.02/MRU. Enterprise SSO connections $75/mo each beyond the first; B2B add-on $100/mo.

**Auth0 is the growth-penalty option.** Free jumped 7,500 → 25,000 MAU in Sept 2024 `[S]`, but B2C Essentials starts at $35/mo for 500 MAU with $0.07/MAU overage — a 300% increase from the prior $0.023 `[S]`. At 100k MAU you are in "contact us" territory.

**Supabase Auth detail** `[V]`: Free 50,000 MAU; Pro ($25/mo) 100,000 MAU included, then **$0.00325/MAU**; SAML/SSO priced separately at 50 included then $0.015/MAU; Advanced MFA (Phone) $75/mo.

**Kinde's overage is the steepest** at $0.0175/MAU on Pro `[V]` — roughly 5× Supabase's rate. And passkeys are gated behind paid tiers.

### 1.2 Self-hosted

| Option | License | Cloud option | Resource floor | Release cadence (verified) |
|---|---|---|---|---|
| **Keycloak** | Apache-2.0 `[V]` | none | **4 GB min, 8 GB prod** `[S]` (JVM/Quarkus) | 26.7.3 on 2026-08-31; ~6 releases in 3.5 months `[V]` |
| **Zitadel** | **AGPL-3.0 from v3** `[S]` (was Apache-2.0) | $0 (100 DAU) / **$100/mo (25k DAU)** `[V]` | **512 MB app** + Postgres; 4 cores for hashing; 3-node HA recommended `[V]` | v4.17.3 on 2026-09-04; v3 still patched `[V]` |
| **Authentik** | mixed (core MIT + enterprise) `[V]` | **none — "We do not currently provide a hosted version"** `[V]` | Python/Redis/Postgres | 2026.8.2 on 2026-09-09 `[V]` |
| **Ory** | Apache-2.0 (OSS) | **$770/yr** Production + $0.14/aDAU `[V]` | Multi-service (Kratos/Hydra) | — |

**The release cadence is the argument.** On 2026-09-09 Authentik shipped **2026.8.2, 2026.5.7 and 2026.2.7 simultaneously** `[V]` — a coordinated security release across three supported branches. Keycloak shipped six releases between 2026-05-19 and 2026-08-31 `[V]`. That is the patch treadmill you are signing up for. A reasonable heuristic from the research: *if you cannot say who is paged when the IdP stops issuing tokens at 3am, you are not ready to self-host one* `[S]`.

**Two lock-in flags.** Zitadel v3 relicensed from Apache-2.0 to **AGPL-3.0** `[S]`, which some downstream adopters found unwelcome; running it unmodified for your own service is normally fine, but it's a change worth knowing. Authentik has **no hosted option at all**, so "start hosted, self-host later" isn't available.

**Cheapest credible self-host:** Zitadel on a Hetzner CX33 (~€8.49/mo) + Postgres. It is meaningfully lighter than Keycloak — 512 MB for the app vs Keycloak's 4–8 GB JVM, and Keycloak takes 60–90 s to reach healthy `[S]`.

### 1.3 Rolling your own in axum

The Rust ecosystem is in good shape:

| Crate | Version | Updated | Notes |
|---|---|---|---|
| `jsonwebtoken` | **11.0.0** | 2026-07-24 `[V]` | 189M downloads; the standard |
| `webauthn-rs` | **0.5.5** | 2026-04-30 `[V]` | 6.8M downloads; Kanidm's, production-grade |
| `openidconnect` | 4.0.1 | 2025-07-06 `[V]` | 13.3M downloads |
| `oauth2` | 5.0.0 | 2025-01-21 `[V]` | 50M downloads |
| **`axum-jwt-auth`** | **0.7.0** | **2026-05-19** `[V]` | **axum ^0.8**, jsonwebtoken ^10, remote JWKS w/ auto-refresh ← **best fit** |
| `axum-jwks` | 0.12.0 | 2025-06-06 `[V]` | axum ^0.8, jsonwebtoken ^9 |
| ⚠️ `jwt-authorizer` | 0.15.0 | **2024-08-27** `[V]` | **axum ^0.7 — incompatible with your axum 0.8** |

**Concrete trap:** `jwt-authorizer` is the most-cited JWKS crate and still gets ~500k recent downloads, but it pins **axum ^0.7** and hasn't shipped since August 2024 `[V]`. `api-rust` is on axum 0.8. Use **`axum-jwt-auth` 0.7.0** instead.

**What rolling your own actually costs.** For an E2EE backup service the account is genuinely thin: email → device token → paid-tier flag. You don't need orgs, roles, MFA or social. But you own rate limiting, token entropy, user enumeration, replay, and — the underrated one — **email deliverability** (SPF/DKIM/DMARC, bounce handling, reputation). Magic links are only as good as your mail. Resend: 3,000/mo free, then $20/mo for 50,000 `[V]`; AWS SES ~$0.10/1,000 but you own IP warmup and complaint feedback loops `[S]`.

**Verdict.** For a 1–2 person team the dominant risk is shipping at all and not getting owned, and auth is where solo teams get owned. WorkOS AuthKit at $0 removes that class of work while staying standard OIDC. The counter-argument — and it is a real one for a privacy product — is that it adds a US sub-processor holding user emails, a DPA, and a DPF dependency. If that's unacceptable, the minimal magic-link + device-token scheme in your existing axum service is the principled alternative; budget a week and a security review.

⚠️ **Validate before committing:** neither WorkOS's nor Clerk's main docs discuss desktop apps. Hosted auth vendors in 2026 are web/mobile-first; Tauri/Electron is undocumented territory. Spike the loopback flow end-to-end before building on it.

---

## 2. How Tauri 2 apps actually do login

### 2.1 Loopback beats deep links — decisively

**RFC 8252 is normative** `[V]`:
- §4 / §8.12: *"native apps MUST use an external user-agent"* and *"MUST NOT use embedded user-agents to perform authorization requests"*
- §8.1: *"Public native app clients MUST implement PKCE... and authorization servers MUST support PKCE for such clients"*
- §7.3: *"The authorization server MUST allow any port to be specified at the time of the request for loopback IP redirect URIs"*
- §7.1: a conformant private-use scheme is reverse-DNS — `tech.lattice.app:/callback`, **not** `lattice://callback`

**What providers actually allow:**

| Provider | `http://127.0.0.1:PORT` | Custom scheme |
|---|---|---|
| **Google** (Desktop client) | ✅ recommended `[V]` | ❌ *"no longer supported due to the risk of app impersonation"* `[V]` |
| **GitHub** | ✅ port need not match `[V]` | not supported |
| **WorkOS** | ✅ **allowed in production for native clients** `[V]` | `[U]` |
| **Microsoft Entra** | ✅ `localhost` only `[V]` | ❌ `[S]` |
| **Apple** | ❌ *"can't contain an IP address or `localhost`"* `[V]` | ❌ HTTPS + real domain required `[V]` |

**Google bans the embedded webview explicitly** `[V]` (policy page last modified 2026-08-05): *"A developer must not direct a Google OAuth 2.0 authorization request to an embedded user-agent under the developer's control."* Violations get HTTP 403 `disallowed_useragent`. Tauri's webview **is** WKWebView/WebView2 — precisely what's named.

**Open the system browser** via `tauri-plugin-opener` 2.5.5, not `tauri-plugin-shell` (whose `open` is deprecated; CVE-2025-31477, High, was patched in shell 2.2.1 — your 2.3.4 is safe `[V]`).

### 2.2 Plugin status

| Plugin | Version | Updated |
|---|---|---|
| `tauri-plugin-deep-link` | 2.4.10 (3.0.0-alpha.0 exists) | 2026-08-31 `[V]` |
| `tauri-plugin-single-instance` | 2.4.4 | 2026-08-31 `[V]` |
| `tauri-plugin-oauth` (FabianLars) | 2.1.0, 215★ | 2026-07-07 `[V]` |

`tauri-plugin-oauth` is alive but thin: eight of its last ten commits are Renovate bumps, and it was panicking on client disconnect until a fix merged 2026-05-07 `[V]`. Its Tauri v3 compatibility issue (#50) is open `[V]`. At ~300 lines it is defensible to vendor. Its own README states the rationale: *"Many OAuth providers (like Google and GitHub) don't allow custom URI schemes."*

### 2.3 Deep-link pitfalls (if you use them anyway)

- 🚨 **Linux packaged builds silently drop deep-link URLs.** [tauri#15928](https://github.com/tauri-apps/tauri/issues/15928), opened 2026-08-28, **still open**: the bundler writes `MimeType=x-scheme-handler/<scheme>` but renders `Exec={{exec}}` with no `%u` field code, so the URL never reaches the app. *"For OAuth/OIDC flows this means the callback never reaches the app, and sign-in cannot complete."* The dev-mode runtime handler **does** include `%u`, which masks the bug. Confirmed unfixed on `dev` today `[V]`. Workaround: override `bundle.linux.desktopTemplate`.
- **macOS cannot register schemes at runtime** — `register()` returns `UnsupportedPlatform` `[V]`. You must build and install a real `.app` so LaunchServices indexes it.
- **Windows/Linux spawn a second instance**; you need `tauri-plugin-single-instance` registered **first** `[V]`. Its arg parser bails if argv has more than one extra entry — any launcher or portal that appends an argument silently kills the deep link `[V]`.
- **Cold-start race:** call `getCurrent()` on mount *and* register `onOpenUrl`. Listener-only loses every cold-start callback. This race doesn't exist with loopback, since the Rust listener is already running.
- `update-desktop-database` may be absent on KDE ([plugins-workspace#2265](https://github.com/tauri-apps/plugins-workspace/issues/2265), open since 2025-01-06) `[V]`.

### 2.4 Sign in with Apple — you probably don't need it

Guideline 4.8 verbatim `[V]` requires an alternative login only when you use a *third-party* login service, and lists this exemption: *"Your app exclusively uses your company's own account setup and sign-in systems."* Note the three criteria an alternative must meet include *"allows users to keep their email address private"* — a plain email login **cannot** satisfy that, so if you add Google you likely do need SIWA. **Offering only your own login avoids the question entirely.**

Guidelines apply to the Mac App Store too `[V]`. If you ship a notarized DMG outside the MAS, 4.8 doesn't apply at all.

If you do want SIWA: Apple Developer Program required (needed anyway for notarization); a Services ID associated with a primary App ID — **not** a published App Store app, per Apple's own help page `[V]` (their REST doc says otherwise; unresolved contradiction `[U]`); SPF/DKIM on registered domains for private email relay `[V]`; and a **client secret JWT that cannot exceed `15777000` seconds (six months)** `[V]` — miss the rotation and every login fails with `invalid_client` on expiry day. Max two private keys per app enables zero-downtime rotation.

**Apple can't use loopback or custom schemes**, so SIWA requires an HTTPS endpoint you control that 302s back to the app — Apple's own documented pattern `[V]`. Also: Apple returns the user's name/email **only on first authorization**.

### 2.5 Google specifics

Good news for your scope set. Requesting only `openid email profile` means:
- **No CASA / restricted-scope assessment.**
- **No unverified-app screen and no 100-user cap** — that cap applies only to apps showing that screen, which is triggered by sensitive/restricted scopes `[V]`.
- **Exempt from the 7-day testing-mode refresh-token expiry**, verbatim: *"unless the only OAuth scopes requested are a subset of name, email address, and user profile"* `[V]`.
- Brand verification: automated in minutes, manual review 2–3 business days `[V]`. Needs a homepage and a privacy policy **on the same domain**, plus Search Console domain verification.

Other limits: **100 refresh tokens per Google account per client** — exceeding it silently invalidates the oldest `[V]`; refresh tokens die after **six months unused** `[V]` (weeks offline is fine, months is not). Client secrets are issued but treated as non-confidential (`client_secret` is *Optional*) `[V]`.

### 2.6 Passkeys in Tauri — not viable in-app cross-platform

[tauri#7926](https://github.com/tauri-apps/tauri/issues/7926) is **open**, labelled `needs triage` since 2023-09-30, last comment 2026-04-07 `[V]`. From the thread: Windows 11 works; Android doesn't; macOS needs the Associated Domains entitlement plus `com.apple.developer.web-browser.public-key-credential` and reporters hit signing breakage; Linux WebKitGTK lacks the API. The community `tauri-plugin-webauthn` (MIT, 20★, last push 2026-09-01) supports Linux/Windows/Android and explicitly **not macOS** `[V]`.

**Do passkeys in the system browser**, where they work everywhere. Another point for the loopback flow.

### 2.7 Keyring storage

`keyring` is now **4.2.0** (2026-08-29) `[V]`; you're on 3.6.3. v4 restructured into `keyring-core` + per-backend crates, but `keyring = "4"` with the `v1` feature keeps the old API `[V]` — that's the right call for a small team.

- 🚨 **Windows Credential Manager caps a credential at `CRED_MAX_CREDENTIAL_BLOB_SIZE (5*512)` = 2,560 bytes** `[V]`. Refresh tokens fit easily; an ID+access+refresh JSON bundle does not (Entra access tokens alone run 2–4 KB). **Store only the refresh token; keep access/ID tokens in memory.**
- **macOS re-prompts are about the designated requirement, not the cdhash.** Per Apple DTS: the ACL is tagged with the creating app's DR, and *"This happens automatically if you use Developer ID (or Mac App Store) signing"* `[V]`. So normal updates don't re-prompt — but **your `signingIdentity: "-"` ad-hoc config will**, on every rebuild.
- **Linux is the hard platform.** Secret Service needs a running daemon and session bus; `linux-keyutils` is explicitly *"completely in-memory and will not persist across reboots"* `[V]` — unusable for a weeks-offline app. Flatpak needs the `org.freedesktop.portal.Secret` portal. **Degrade loudly** ("you'll need to sign in again after restart") rather than falling back silently to a file.
- **Skip `tauri-plugin-stronghold`.** The wrapper is maintained (2.3.2, 2026-08-31) but the underlying `iota_stronghold` crate last published **2024-05-13** and the upstream repo's last commit was **2023-06-29** `[V]`. It also doesn't solve the problem — you still need somewhere to put its password.
- **Token rotation:** rotate on use with reuse detection and family revocation; absolute lifetime 30–90 days. **Avoid sliding 7-day windows** — they break a laptop closed over a holiday. Treat "token is gone" as a normal path: Lattice must stay fully functional with sync signed out.

---

## 3. Hosting a small axum + Postgres service

| Platform | Realistic min $/mo | PITR | Ops | EU residency |
|---|---|---|---|---|
| **Hetzner** (self-managed PG) | **$16–29** | ❌ DIY | **High** | ★★★★★ EU-only, German entity |
| **Railway** Pro | **$21–56** | ✅ ~4 weeks | Medium | Amsterdam only |
| **Lightsail + RDS** | **$31** | ✅ up to 35 d | Medium | Frankfurt; EUSC possible |
| **Supabase** Pro (daily only) | **$32** | ❌ | Low | Frankfurt (US entity) |
| **DigitalOcean** App Platform | **$42** | ✅ 7 d | **Low** | FRA1/AMS3/LON1 (US entity) |
| **Fly.io** | **$53** | ✅ 10 d | Low-Med | ams/fra/lhr |
| **Render** Pro | **$80** | ✅ 3–7 d | **Lowest** | Frankfurt |
| **Supabase** Pro + PITR | **$132** | ✅ 7 d (+$100) | Low | Frankfurt |
| **Cloudflare Workers** | **$5 + DB** | n/a | Low | R2/D1 `eu` jurisdiction |

**Fly.io** `[V]`: machines shared-cpu-1x/256MB $2.02, shared-cpu-2x/1GB $6.64 (Amsterdam); egress $0.02/GB EU+NA, **$0.12 Africa/India**. Managed Postgres Basic $38, storage $0.28/GB, **10-day PITR and HA included**. ⚠️ Fly's own docs list *"Security patches and version upgrades"* and *"Customer-facing alerting"* as **still under development** on the managed database — a significant maturity flag. Legacy Fly Postgres is now titled *"Fly Postgres (Unmanaged)"* with a page literally called "This Is Not Managed Postgres" `[V]`. New Feb 2026 charge for MPG inter-region private traffic — co-locate app and DB.

**Railway** `[V]`: Free $0 ($1 credits), Hobby $5, Pro $20 — and **Pro seats are free**, not $20/seat as many blogs claim. Metered: RAM $10/GB-mo, vCPU $20/vCPU-mo, volumes $0.15/GB-mo, egress $0.05/GB from the first GB. **Hobby caps volumes at 5 GB**, disqualifying this workload. PITR uses pgBackRest with **~4 weeks restore window, restorable to the second** — the best window here. ⚠️ Docs still call the Postgres template *"unmanaged"* while a 2026-09-10 blog announces first-class managed HA and CVE patching; the maintenance-window page doesn't exist `[U]`. Top cause of surprise bills: using `DATABASE_PUBLIC_URL` instead of the private URL bills every query as egress.

**Render** `[V]`: **Pro is $25/workspace, not per seat**. Instances: 0.5c/512MB $7, 1c/2GB $25 (no 1 GB option). Postgres: 0.1c/256MB $6, 0.5c/1GB $19, storage $0.30/GB-mo. **PITR window follows the workspace plan — Hobby 3 days, Pro+ 7 days** — and 7 days is the ceiling on any self-serve plan. Upgrading doesn't backfill. Only the free tier sleeps (~1 min cold start). Region is immutable. Hobby allows exactly **1 seat**.

**Hetzner** `[V]` (from Hetzner's own live price feed, `updatedAt 2026-09-01` — this resolves the 2× conflict in third-party sources): CX23 2 vCPU/4 GB/40 GB **€5.49**, CX33 4/8/80 €8.49, CAX11 ARM 2/4/40 €5.99. IPv4 €0.50/mo. Backups = 20% of instance price. **EU servers include 20 TB traffic; US servers 1 TB** — overage €1.00/TB. Counterintuitively CX23 (€5.49) is *half* CPX12 (€11.49) because CX is heavily oversubscribed. **No managed Postgres exists** — six first-party checks `[V]`. Their backups are crash-consistent disk images, not Postgres-aware: no PITR, no protection against a bad `DROP TABLE`. ⚠️ SLA credits are claimed in marketing but absent from the T&Cs with no SLA document (404) `[U]`.

**DigitalOcean** `[V]`: Basic/Professional tiers were **removed**; egress now attaches to instances. `apps-s-1vcpu-1gb` $12 (150 GiB), `apps-s-1vcpu-2gb` $25. Managed PG 1 GiB $15.15, 2 GiB $30.45 (30–60 GiB storage included). **Daily backups + 7-day PITR included at base price** `[V]`. SLA 99.95% with standby, **99.5% single-node**. ⚠️ Egress overage $0.02/GiB ≈ **$20/TB vs Hetzner's $1.20/TB — 17×** — and this is a backup product where users pull data down.

**AWS Lightsail** `[V]`: instances $5 (512 MB, IPv4) / $3.50 (IPv6-only); **prices identical in us-east-1 and eu-central-1**. Managed DB 1 GB $15, HA $30, with 7-day PITR at 5-minute granularity. **But RDS db.t4g.micro + 20 GB gp3 in eu-central-1 is $16.61 vs Lightsail's $15.00** — $1.61 more buys **35-day per-second PITR**, PG 17/18 instead of a ceiling at 16, VPC networking, and read replicas. ⚠️ Lightsail's Postgres docs are rotting — the FAQ still says "PostgreSQL 9, 10, 11, and 12," recommends MySQL Workbench and mentions port 3306. Lightsail managed DBs **cannot VPC-peer** (public-internet only) — a bad look for a privacy product. **AWS European Sovereign Cloud launched** (`eusc-de-east-1`, Brandenburg, Jan 2026) with RDS/Aurora but **not Lightsail**.

**Cloudflare Workers + workers-rs** `[V]`: `worker` v0.8.6 shipped **2026-09-15**, ~427k downloads/month, with **official axum and tokio-postgres examples**. Workers Paid $5/mo (10M requests + 30M CPU-ms; overage $0.30/M and $0.02/M). Free plan allows only **10 ms CPU per invocation**. **Hyperdrive is included free on both plans** — yes, still. D1: 5 GB free then $0.75/GB-mo.

🚨 **Three blockers.** (1) **No tokio** — *"you've got to leave your threaded async runtimes at home"*; everything must compile to `wasm32-unknown-unknown`, so **`sqlx` does not come along**. This is a port, not a lift-and-shift. (2) **D1 maxes at 10 GB and is single-threaded** — it's SQLite, not your Postgres. (3) R2 and D1 support hard `eu` jurisdiction pinning `[V]`, but **Workers compute still runs at the nearest edge worldwide** unless you buy the Enterprise-only Data Localization Suite.

### Object storage

| Provider | $/TB-mo | Egress | Requests | Min duration |
|---|---|---|---|---|
| **Backblaze B2** | **$6.95** | free to **3× stored**, then $0.01/GB | **free** | **none** |
| **Hetzner** | ~$8.86–9.15 | **$1.20/TB** | free | none |
| **Cloudflare R2** | $15.00 | **$0** | A $4.50/M, B $0.36/M | none |
| **Wasabi** | $7.99 (1 TB min) | free but **capped at stored volume** | free | **90 days** |
| **Tigris** | $20.00 | $0 | A $5/M | none |
| **AWS S3** | $23.00 / $24.50 (euc1) | **$90/TB** after 100 GB | $0.005/1k PUT | none |

*500 GB stored + 100 GB out:* B2 **$3.41**, R2 $7.35, Hetzner $7.99, S3 $12.54.

🚨 **The Wasabi trap.** Exceed their egress policy and there is **no overage price** — verbatim: *"we reserve the right to limit or suspend your service."* For a backup product where a bad week means everyone restores at once, that's an availability risk, not a cost risk. Backblaze's equivalent is a *priced* $0.01/GB. On a 5 TB mass-restore: R2/Tigris $0, B2 (@2 TB stored) $0, Hetzner +$4.80, B2 (@500 GB) +$36.20, **S3 +$451.80**, Wasabi = policy action.

⚠️ **Tigris auto-replicates objects globally** by traffic pattern `[V]` — a real GDPR problem unless pinnable, which I could not confirm `[U]`.

**Recommendation: Backblaze B2 EU Central.** Cheapest storage, free API calls, no minimum duration, and a priced rather than punitive egress ceiling you'll rarely hit at 3× stored volume. Use **R2** instead if restore volume is genuinely unpredictable and you want absolute egress immunity plus hard `eu` jurisdiction.

---

## 4. Managed Postgres

| Provider | Entry prod | Free tier | Storage $/GB-mo | PITR | Lock-in |
|---|---|---|---|---|---|
| **Crunchy Bridge** | **$35** (Hobby-2) | none | **$0.10** | **10 d included, minute granularity** | **Lowest** |
| **Neon** | ~$20 (Launch, usage) | 100 CU-h, 0.5 GB | $0.35 + $0.20 restore | 7 d / 30 d (Scale) | Low-Med |
| **Scaleway** | ~€11–17 | none | €0.099–0.149 | `[U]` | Low |
| **Supabase** | $30 / **$130 w/ PITR** | 500 MB, pauses | $0.125 | **+$100/mo** | Low (DB) / Med-High |
| **DigitalOcean** | $15.15 | none | $0.215 | ✅ 7 d included | Low |
| **Fly MPG** | $38 | none | $0.28 | 10 d included | Low |
| **AWS RDS** | $13.87 (t4g.micro, euc1) | credits only | $0.137 (euc1) | **1–35 d included** | **Lowest** |
| **Nile** | $15 | 1 GB, never pauses | **$1.00** | 🚨 **"coming soon"** | Med |

**Neon** `[V]`: plan names are now Free/Launch/Scale (+ Agent); Business is no longer self-serve. Launch $0.106/CU-hour, Scale **$0.222** — 2.1× — and the only way to get 30-day PITR is to reprice *all* compute. History window is included but the instant-restore storage backing it bills **$0.20/GB-mo extra**. Scale-to-zero after 5 min; docs claim "a few hundred milliseconds," independent measurements say **300–800 ms cold start** `[S]`. EU: Frankfurt and London only, **region immutable after creation**.

**Databricks acquired Neon** (announced 2025-05-14, ~$1B) `[V]`, citing that **>80% of Neon databases were created by AI agents, not humans** — that's the strategic tell. Pricing got *better* (storage $1.75 → $0.35/GB-mo), but a credible skeptical read is that the cuts come from Databricks' AWS enterprise discounts, not engineering — i.e. **a parent-company subsidy, revocable if strategy changes** `[S]`.

**Snowflake acquired Crunchy Data**, closed 2025-06-06 for **$164.5M** per Snowflake's FY2026 10-K (below the ~$250M widely reported) `[V]`. Snowflake Postgres went GA 2026-02-24. Crunchy Bridge is still sold with no announced sunset, but the docs homepage carries **no Snowflake notice at all** — benign neglect rather than commitment. This is survivable precisely *because* Crunchy's whole identity is "it's just Postgres" and exit is one `pg_dump`.

**AWS free tier is effectively gone**: the 12-month tier was sunset 2025-07-15; new accounts get $100 credits (+$100 for onboarding), and Free Plan status ends after 6 months or when credits hit zero `[V]`. ⚠️ **AWS's own RDS PostgreSQL pricing page still advertises the retired "750 hours for one year"** `[V]` — if a vendor's own page can be 14 months stale, treat every number here as a starting point for a quote.

**Aurora Serverless v2 scale-to-zero shipped** (GA since 2024-11-20) but **won't help you**: AWS's own blog says *"If user-initiated connections are open, the instance doesn't automatically pause"*, and an sqlx pool holds connections by design. Resume latency is ~15 s regardless. Pinned at 0.5 ACU in eu-central-1 that's **$51.10/mo** — 3.7× a db.t4g.micro.

**Recommended trajectory:** Neon Launch (Frankfurt) at ~1k users → **Crunchy Bridge Standard-4 (~$73) at ~10k**, where Crunchy's $0.10/GB storage and free 10-day PITR overtake Neon's serverless premium → Crunchy or RDS at 100k, where **Neon Scale inverts to the most expensive mainstream option (~$500)**. All are stock Postgres, so the migration is `pg_dump`/`pg_restore`.

---

## 5. Payments

### Net on a hypothetical $5/month subscription

| Route | Fee | Net | Who handles VAT |
|---|---|---|---|
| **Stripe direct** | 2.9% + $0.30 `[V]` | **$4.56** | **You** |
| + Stripe Billing (0.7%) + Tax Basic (0.5%) | | **$4.50** | You register & remit |
| **Stripe Managed Payments** | +3.5% on top `[V]` → ~6.4% + $0.30 | **$4.38** | **Stripe (MoR)** |
| **Paddle** | 5% + $0.50 `[V]` | **$4.25** | **Paddle (MoR)** |
| **Lemon Squeezy** | 5% + $0.50 `[V]` | **$4.25** | **LS (MoR)** |
| **Polar** Starter | 5% + $0.50 `[V]` | **$4.25** | **Polar (MoR)** |
| **Apple IAP** (Small Business, <$1M/yr) | 15% `[V]` | **$4.25** | Apple |
| Apple IAP standard | 30% | $3.50 | Apple |

**Stripe is not merchant of record.** Stripe Tax Basic is 0.5%/transaction and **calculates only** — you still register, file and remit in every jurisdiction where you have obligations `[V]`. For a 1–2 person team selling to EU consumers, that is the real cost, and it's why MoR usually wins at small scale.

**Stripe Managed Payments is new and changes the calculus** `[V]`: Stripe becomes MoR via Link, handling *"calculation, collection, filing, and remittance"* across 80+ countries, for **+3.5%** on top of standard processing. That's ~6.4% + $0.30 — better than Paddle on small amounts, worse on large.

**Lemon Squeezy is alive but converging into Stripe.** Its pricing page carries a banner *"2026 Update: Lemon Squeezy + Stripe Managed Payments"* (post dated 2026-01-28) `[V]`. Still 5% + $0.50, still open to new sellers, **no sunset notice** — but the direction is unmistakable. ⚠️ I could not retrieve the post body `[U]`. **Don't start new business on Lemon Squeezy without reading that announcement.**

⚠️ **Paddle flags products under $10 as "custom pricing — contact sales"** `[V]`. A $5/mo subscription is 15% all-in at their published rate. Confirm they'll take it before designing around it.

**Polar** is the cheapest at volume if you pay a platform fee: Starter 5% + 50¢, **Pro $20/mo → 3.8% + 40¢** `[V]`; +1.5% international, $15/dispute, payouts $2/mo + 0.25% + $0.25.

### Mac App Store

Guideline 3.1.1(a) verbatim `[V]`: *"These entitlements are not required for developers to include buttons, external links, or other calls to action in their **United States storefront** apps... In all other storefronts, except for the United States storefront, where this prohibition does not apply, apps and their metadata may not include buttons, external links, or other calls to action that direct customers to purchasing mechanisms other than in-app purchase."*

So the post-Epic US carve-out is reflected in the live guidelines. But the StoreKit External Purchase Link Entitlement is *"limited to use only in the iOS or iPadOS App Store"* `[V]` — ⚠️ **how the US carve-out applies to the Mac App Store specifically is not resolved by this text** `[U]`.

**The clean answer: ship a notarized DMG outside the Mac App Store and none of this applies** — no IAP, no 4.8, no review. Tauri apps are commonly distributed this way, and Mac App Store distribution of Tauri apps has its own sandbox/entitlement difficulties. Given `signingIdentity: "-"` today, you need a Developer ID certificate and the $99/yr program either way.

---

## 6. Compliance and privacy basics

### 6.1 What a 1–2 person team actually must do

- **Records of processing (Art 30).** The "fewer than 250 employees" exemption is nearly useless. Verbatim, it doesn't apply where *"the processing is not occasional"* `[V]` — a continuously running sync service is by definition not occasional. **You must keep a ROPA.**
- **Subject requests (Art 12(3)).** *"without undue delay and in any event within one month"*, extendable by two months for complexity, with notice given inside the first month `[V]`. Art 15 access + Art 20 portability + Art 17 erasure. **Build export and delete as product features, not manual ops.** (Apple Guideline 5.1.1(v) also requires in-app account deletion if you ever go to the MAS `[V]`.)
- **Breach notification.** Art 33: 72 hours to the supervisory authority. Art 34: notify data subjects — **unless** Art 34(3)(a) applies.
- **EU representative (Art 27).** Required where Art 3(2) applies. The derogation covers processing that is *"occasional... and is unlikely to result in a risk"* `[V]` — a sync service fails that test. **A US company serving EU users needs an Art 27 representative.** ⚠️ I could not obtain 2026 vendor pricing (Prighter 429'd twice, DataRep 404'd) `[U]`. Budget a few hundred EUR/year and verify.
- **DPO:** not triggered on this profile (no large-scale special-category processing, no systematic monitoring as core activity).

### 6.2 Transfers

**The EU-US Data Privacy Framework is still in force.** The European Commission's own adequacy-decisions page lists *"United States (commercial organisations participating in the EU-US Data Privacy Framework)"* among current adequacy decisions, with no suspension noted `[V]`. UK adequacy was renewed in **December 2025** `[V]`. ⚠️ The Latombe challenge outcome and any appeal are unresolved in my research `[U]`.

**Practical hedge:** hosting in the EU under an EU entity (Hetzner, Scaleway) removes the transfer question entirely. Frankfurt-under-a-US-parent (DigitalOcean, Supabase, Neon, AWS) satisfies residency but leaves CLOUD Act exposure — a distinction a privacy-positioned product will eventually be asked about.

### 6.3 Does E2EE materially reduce obligations? Partly — and less than people claim

**The concrete, verified win is Art 34(3)(a)** `[V]`: no need to notify data subjects where the controller applied measures *"that render the personal data unintelligible to any person who is not authorised to access it, such as encryption."* That is a real reduction in the worst-day-of-your-life scenario. Note it does **not** exempt you from the 72-hour Art 33 notification to the regulator.

**What E2EE does not do.** You still process email addresses, billing records, IP addresses, device IDs, timestamps, blob sizes and sync metadata — all plainly personal data, all fully in scope. E2EE shrinks the blast radius and the notification exposure; it does not make you not-a-controller.

⚠️ **The key legal authority is unresolved.** The question of whether ciphertext is "personal data" for a provider holding no key turns on the *"means reasonably likely to be used"* test (Recital 26) and on whether that test is assessed **relative to the recipient**. The leading case is **CJEU C-413/23 P (EDPS v SRB)**, the appeal from General Court T-557/20. **I could not reach a primary source** — eur-lex returned empty pages, curia 404'd, the EDPS site 403'd, gdprhub was behind a bot wall `[U]`. I confirmed that **EDPB Guidelines 01/2025 on Pseudonymisation** exist and went to public consultation 17 Jan – 14 Mar 2025 `[V]`, but could not retrieve the text. **Do not rely on my recollection here; get a lawyer's read before making legal claims in marketing.**

### 6.4 Encryption policy climate

- **EU "Chat Control" (CSAR):** mandatory client-side scanning was **removed** from the Council's mandate in late 2025; the mandate still permits *voluntary* scanning without warrant or suspicion. Trilogues ran Dec 2025 – Jun 2026 with adoption expected ~July 2026 `[S]`. ⚠️ **Source is Patrick Breyer, an advocacy site — treat as directionally informative, not neutral.** It is now September 2026 and **I could not confirm whether the regulation was actually adopted or what the final text says about encryption** `[U]`.
- **UK:** Apple received a Technical Capability Notice 2025-02-07 and withdrew Advanced Data Protection for UK users 2025-02-21, appealing to the Investigatory Powers Tribunal in March 2025 `[V]`. ⚠️ **Everything after March 2025 is unverified** `[U]` — whether the notice was narrowed or withdrawn, and whether ADP is back.

The honest read: E2EE is defensible and increasingly normal, but the regulatory ground is moving. Design so that *adding* E2EE doesn't require a wire-format break — which argues for deciding **now**, before the schema freezes.

### 6.5 Account recovery when data is E2EE

Four real patterns:

1. **Unrecoverable.** Obsidian Sync: a separate encryption password, AES-256 + scrypt + GCM, and verbatim — *"if you forget or lose your encryption password, your data remains encrypted and unusable forever. We're not able to recover your password, or any encrypted data for you."* `[V]` Simplest to ship.
2. **Recovery key issued at setup.** 1Password Secret Key + Emergency Kit: *"We don't have a copy of your Secret Key or any way to recover or reset it for you"* `[V]`; Business members can be recovered by an admin. Apple ADP's recovery key/contact is the same family.
3. **Split account-recovery from data-recovery.** Proton is the clearest articulation `[V]`: recovery **phrase** restores password *and* data; recovery **file** restores data only; recovery **email/phone** resets the password only — and *"If you have a password reset method and no data recovery method, you'll lose access to everything that was on your account before the password reset."*
4. **Enclave-escrowed secret behind a low-entropy PIN.** Signal's SVR. **Out of reach for a 1–2 person team** — it requires HSM/enclave infrastructure and attestation.

**The local-first insight that makes this easy for Lattice.** Obsidian's guidance is that losing the sync password costs you the *remote* vault, not your notes — local copies are untouched, and the fix is to reset the remote vault and re-upload. Lattice is local-first, so **forgetting the backup passphrase should lose the backup, never the data**. That makes pattern 1 (with a downloadable recovery key, i.e. a light pattern 2) entirely shippable, and it's a far kinder failure mode than a cloud-first product can offer. Say so explicitly in the UI, in Proton's blunt register.

---

## 7. The no-accounts alternative: bring your own storage

### What it saves
No servers, no storage cost, no Postgres, no PITR, no backup drills. Compliance posture improves sharply: if user content never reaches your infrastructure, you are not processing it. ⚠️ **But you don't escape GDPR** — you still process account/licence/telemetry/support data, and whether a vendor whose *client code* touches data is a controller or processor for that content is a genuinely contested question I did not resolve `[U]`.

### What it costs — per backend, with real constraints

- **Google Drive: much easier than feared.** `drive.file` and `drive.appdata` are **non-sensitive** scopes requiring only basic OAuth verification `[V]`. The six restricted scopes (`drive`, `drive.readonly`, `drive.metadata`, …) require restricted-scope verification and an annual third-party security assessment, reverified *"at least every 12 months"* `[V]`. **Staying on `drive.file` avoids CASA entirely.** ⚠️ CASA dollar figures in circulation range from $500 to $75,000+ across vendor blogs — all unreliable, none primary `[U]`. Google explicitly doesn't set the price.
- **Dropbox: a hard early gate.** Dev mode caps at 500 users, and **at 50 linked users you have two weeks to obtain production approval** or you stop adding users `[V]`. Approval *"will not be reviewed until your app has linked with at least 50 Dropbox users."* Use App-folder scope, not Full Dropbox.
- **iCloud/CloudKit: kills cross-platform parity.** Apple advertises CloudKit across *"iOS, iPadOS, macOS, tvOS, watchOS, visionOS and the web"* `[V]`. CloudKit Web Services exists, but iCloud Drive ubiquity containers are Apple-only, and a web-auth flow inside a Windows/Linux desktop app is poor UX. ⚠️ Whether a **non-Mac-App-Store** app can use iCloud entitlements at all is unresolved `[U]`.
- **Generic S3 / WebDAV:** works everywhere, no approval gate — but asking a user for an endpoint, access key and secret is a brutal onboarding funnel for a notes app.

### What it costs in UX
Credential friction; conflict resolution with no server arbiter; no server-side versioning or dedup; no cross-device discovery; no share links; and **N heterogeneous backends each failing differently** — the support burden is the thing BYOS products consistently underestimate. And you still need *some* identity to license a paid tier.

**The pragmatic read:** Obsidian's own model is the proof. They give free users BYOS (iCloud/Dropbox/Syncthing) *and* sell **Obsidian Sync at $4/user/mo annual** (1 GB, 1-month history) or **$8** (10 GB, 12-month history) `[V]`. BYOS is the free tier and the escape hatch; hosted sync is the product. That is very likely the right shape for Lattice too — and it means the hosted path in §0 is still what you build.

---

## 8. What I could not verify

**Legal (get a lawyer, not me):**
- **CJEU C-413/23 P (EDPS v SRB)** holding and date — every primary source was unreachable. This is the key authority on whether E2EE ciphertext is personal data relative to a keyless recipient.
- EDPB Guidelines 01/2025 on Pseudonymisation — confirmed to exist, text not retrieved.
- Latombe v Commission outcome and appeal status.
- EU CSAR final adoption status and encryption text as of Sept 2026.
- UK Apple ADP / IPT developments after March 2025.
- Art 27 representative vendor pricing (Prighter 429, DataRep 404).

**Commercial:**
- Lemon Squeezy's 2026 Stripe Managed Payments announcement body — read it before choosing LS.
- Whether Paddle will accept a $5/mo subscription at standard rates.
- How the US external-purchase-link carve-out applies to the **Mac App Store** specifically.
- Stytch's per-MAU rate beyond 10k — not published.
- Apple's contradictory SIWA prerequisite (help page says App ID; REST doc says a published App Store app).

**Infrastructure:**
- Fly MPG's GA-vs-beta status; its "security patches and version upgrades under development"; Tigris EU region pinning.
- Railway's Pro-only backups gate (pricing table vs docs conflict) and the blog-only CVE-patching claim.
- Render instance/Postgres pricing beyond what was parsed; Hetzner SLA credits, DDoS protection, VAT reverse-charge.
- Whether Supabase bills `pg_dump`/restore downloads as egress — **critical for you**, since restores-as-downloads are the hot path.
- Whether recent `aws-sdk-s3` checksum headers are accepted by B2/Wasabi/Hetzner.
- Neon's DPA/GDPR posture post-Databricks.

**Method flag:** the WebSearch budget ran out mid-session, so third-party news sweeps (especially "did Hetzner announce managed Postgres?" and post-March-2025 UK encryption news) rest on first-party checks only. All "realistic minimum" cost tables compose sourced vendor rates with modelled usage — **the rates are verified, the consumption is my assumption.**

---

## 9. Suggested sequence

1. **Decide E2EE now**, before the sync schema and wire format freeze. It is incompatible with `conflicts.resolution_content` server-side merge.
2. **Kill the `x-user-id` header** — replace with JWT verification via `axum-jwt-auth` 0.7.0 against a JWKS endpoint. This is the smallest change that makes the existing service shippable.
3. **Spike WorkOS AuthKit + loopback PKCE in Tauri** end-to-end on all three platforms before committing. Desktop is undocumented for every hosted vendor.
4. **Get a Developer ID certificate** and replace `signingIdentity: "-"` — it currently blocks notarization and causes Keychain re-prompts.
5. **Bump `keyring` 3.6 → 4.x** (`v1` feature keeps the API) and store **only the refresh token**, under 2,560 bytes for Windows.
6. Stand up DO App Platform FRA1 + Managed Postgres + B2, and **run a restore drill before you have users**.
7. Ship export and delete as product features from day one — you need them for Art 15/17/20 anyway.

One process note worth passing on: when I asked the hosting researcher to wrap up, it correctly declined to adopt my figures on my say-so and re-derived everything from primary sources, flagging that a message had arrived from a non-user. That was the right instinct, and it's why the Hetzner numbers in §3 come from Hetzner's own live price feed rather than the conflicting third-party figures elsewhere in this report.

---

The compliance track landed and it materially corrects several things in the report I just delivered. Addendum below.

---

# Addendum — corrections and closed gaps

The compliance research returned after I delivered. It closes most of the §8 "could not verify" list and **corrects four things I got wrong or framed too loosely.** Treat this as superseding the corresponding sections.

## 🚨 Correction 1 — E2EE does *not* take you outside the processor framework

This is the most consequential correction, and it sharpens §6.3 considerably. **EDPB Guidelines 07/2020 v2.1, para 40**, worked example, verbatim:

> "**Employer A hires hosting service H to store encrypted data on H's servers.** The hosting service H does not determine whether the data it hosts are personal data nor does it process data in any other way than storing it on its servers. **As storage is one example of a personal data processing activity, the hosting service H is processing personal data on employer A's behalf and is therefore a processor.**"

Storage *is* processing (Art 4(2) lists it expressly). **Encryption changes the risk, not the role.** I said E2EE "shrinks the blast radius and the notification exposure; it does not make you not-a-controller" — that was directionally right but understated. The EDPB has a direct on-point example, and it puts a hosted E2EE backup tier squarely inside Art 28 with certainty. The Art 34(3)(a) breach-notification win stands; nothing else changes.

**The corollary matters for strategy:** BYOS is the *only* design that plausibly exits the controller/processor framework for note content. CNIL's *Guide du sous-traitant* (Sept 2017) says it in terms — *"Ne sont pas concernés… les éditeurs de logiciels"* — but conditionally, on "no access, no processing" actually holding. Which leads to:

## 🚨 Correction 2 — the crash reporter can silently destroy the BYOS argument

A crash report from a notes app can carry note text in a stack frame, clipboard contents, breadcrumb bodies, and file paths like `/Users/jane.doe/Documents/Therapy notes/2026-03 relapse.md` — **the path alone is Art 9-adjacent.** If that happens you become a controller for content you publicly promised you never receive, with an Art 33 duty and a marketing claim that just became a misrepresentation. Sentry's DPA makes you the controller and them the processor.

Design in, don't retrofit: off by default; show the user the exact payload before sending; strip local-variable capture; redact home paths to `~`; drop breadcrumb bodies; 30–90 day retention; and **make "crash the app with sensitive content in a buffer, inspect the payload" a release-gate check.** Write the invariant down: *no user-authored content transits vendor-controlled infrastructure.*

## Correction 3 — Developer ID apps *can* use CloudKit and iCloud Documents

I flagged this unresolved and leaned the wrong way. Verified three ways: Apple's [supported-capabilities matrix](https://developer.apple.com/help/account/reference/supported-capabilities-macos) shows CloudKit, iCloud Documents and iCloud KVS as **YES** for Developer ID (Game Center, HomeKit, IAP and Sign in with Apple are the ADP-only ones); TN3125's doc chain resolves to that same matrix; and Apple's Developer ID page says apps "can also take advantage of advanced capabilities such as CloudKit."

Catch: you must ship an **embedded provisioning profile**, evaluated at install *and every launch* — malformed or revoked means the app **fails to launch**, not just fails to sync. CloudKit remains Apple-platforms-only natively. **CloudKit Web Services is a dead end** — single-round-trip token rotation (unusable for concurrent sync), 30-minute expiry refreshed through an Apple-hosted web view, server-to-server keys are public-database-only, and the only protocol spec is stamped 2016. The cheap path works: write into `~/Library/Mobile Documents/com~apple~CloudDocs/` as a plain directory, zero entitlements — what Obsidian does.

## Correction 4 — CSAR was *not* adopted; the encryption climate is better than I reported

I flagged "adoption expected ~July 2026" from an advocacy source. Verified against the EP Legislative Observatory (procedure 2022/0155(COD)): **"Awaiting Parliament's position in 1st reading."** Still in trilogue; next round **29 September 2026**.

What *was* adopted on 24 July 2026 is a different instrument — **Regulation (EU) 2026/1881**, the replacement ePrivacy derogation, running to 3 April 2028 — and its E2EE carve-out is in **operative text, not recitals**: Art 1(3), *"This Regulation does not apply to interpersonal communications to which end-to-end encryption is, has been or will be applied."* The Council's Nov 2025 general approach (doc 15318/25) deleted detection obligations outright and added Art 1(5) protecting encryption expressly.

**Net: the EU encryption climate in 2026 is more favourable to an E2EE roadmap than my report implied.**

## Closed gaps

**UK ADP — narrowed by replacement, not withdrawn.** IPT ruling *Apple Inc v SSHD* [2025] UKIPTrib 1 (7 April 2025) dismissed the Home Secretary's secrecy application. The FT reported the UK withdrew the first TCN and issued a second targeting UK users only. **ADP remains unavailable to new UK users** — Apple's doc 122234, published 23 September 2025, *after* the reported climbdown. ⚠️ An IPT hearing in ***Apple No.2*** is listed for **tomorrow, 17 September 2026** — recheck before publishing anything.

**Latombe: dismissed 3 September 2025** (T-553/23, ECLI:EU:T:2025:831) — **but the appeal C-703/25 P is live and undecided.** Sharp supporting finding: the judgment leaned on DPRC judges being appointed after consulting PCLOB, and **pclob.gov currently lists exactly one sitting member**, the other three having ended terms in January 2025. That's a factual change to a mechanism the Court relied on.

**Art 27 — the derogation definitively does not help you.** EDPB Guidelines 3/2018: processing is *"'occasional' [only] if it is not carried out regularly, and occurs outside the regular course of business."* A subscription sync service fails that regardless of user count or E2EE. Real 2026 pricing: **DataRep €150/yr** (≤1,000 EU subjects, no Art 9) scaling to €2,000; headcount-priced vendors put a 2-person company in the cheapest band regardless of user count — **GDPRLocal £999/yr EU or £1,500 EU+UK**, GRC Solutions £950, EDPO €1,920. Prighter is hard-blocked behind a bot challenge with an expired cert. **DPO: not triggered** — but note Germany's BDSG §38(1) DPIA limb applies at *any* headcount.

## Three vendor findings that change operational steps

1. 🚨 **Hetzner's DPA is not automatic.** T&Cs §6.2: *"This contract for processing orders is not concluded automatically"*, and §6.3 — absent one, *"we assume that the Customer is not processing third party personal data."* **Deploy on Hetzner and do nothing and you have no Art 28 agreement at all.** In exchange it's the only vendor with a contractual EU/EEA-only commitment (DPA §3(1)).
2. **Stripe is an independent controller, not your processor** — *"has the sole and exclusive authority to determine the purposes and means"*, with Module 1 (controller-to-controller) SCCs. **Your privacy notice must disclose Stripe as a recipient-controller.**
3. **Resend's region selection is not data residency** — it *"controls where your emails are routed and sent from. It does not control where customer data is stored"*, and the DPA says processing takes place in the US. All ~22 sub-processors are US-based. Railway's DPA needs a **DocuSign counter-signature**; Render, Supabase, Cloudflare, AWS and Stripe are click-through/auto. **Fly.io: no published DPA found** (all paths 404) — worth one email.

## BYOS support burden, now quantified

The §7 argument holds and is stronger than I could show. Forum data: **Joplin Support 22.0% of 8,453 topics are sync-related**, versus **Obsidian Help 7.2% of 24,156**. Joplin owns the sync engine across seven backends; Obsidian disclaims every backend by name. **Disclaiming cuts your rate roughly 3×; it does not get you to zero.** Joplin GitHub issues by backend: WebDAV 176, Nextcloud 98, OneDrive 86, Dropbox 81, **S3 51 (only 1 open)** — versus Joplin Cloud 35.

**That last number is the recommendation.** S3-compatible with **Cloudflare R2 and Backblaze B2 as named presets** is the BYOS backend to ship first: lowest support burden by an order of magnitude, no gatekeeper, no OAuth refresh lifecycle, no review queue, no user cap, free at single-user scale, near-zero egress. Ship OneDrive second (`Files.ReadWrite.AppFolder` needs **no admin consent** for consumer accounts, and `delta` works on `/drive/special/approot`). **Defer generic WebDAV — it is a promise you cannot keep**, and Nextcloud's chunked upload is proprietary anyway.

## Licensing without accounts

**Ed25519-signed offline licence file**, buyer's name embedded in the signed payload and shown in the About pane, public key in the binary, verified locally with **no network call**, plus a time-boxed update window. Sublime Text ($99, unlimited machines, no activation server) and Beyond Compare have run this profitably for a decade in the same market.

The reason not to phone home is a genuine irony worth internalising: **Paddle Classic's own licence-activations report stores Email + Device ID + License Code together.** A BYOS app that checks a licence over the network has a *worse* privacy story in the one place users actually test it — Little Snitch — than the E2EE product it was meant to beat. And don't follow Tailscale here: identity *is* its mechanism, so importing it means holding strictly more personal data than the hosted E2EE tier. Follow Obsidian instead — put the paywall behind a service that genuinely needs a server.

## Net effect on the recommendation

The §0 stack stands. Three adjustments:

- **Actively conclude the DPA** if you go Hetzner; it does not happen by deploying.
- **Disclose Stripe as a recipient-controller**, not a processor, in the privacy notice.
- **Budget ~€150–1,500/yr for an Art 27 representative** — it is not optional on this profile.

And one strategic reframe: since hosted E2EE still makes you a processor under EDPB 07/2020 para 40, **E2EE should be justified on risk, trust and breach-notification grounds — not on an expectation that it removes compliance obligations.** The Obsidian shape from §7 remains the right one: BYOS (S3-compatible) as the free tier and escape hatch, hosted sync as the paid product, with the paywall where a server has to exist anyway.