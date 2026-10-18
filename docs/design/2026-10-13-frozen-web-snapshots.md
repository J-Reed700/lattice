# Frozen web snapshots

*2026-10-13. Design for capturing every cited web page as a faithful, self-contained, inert copy that renders in an embedded browser pane and survives the live site changing or disappearing.*

## 0. The ask, and the answer in one paragraph

Josh: when a cited web source is opened, today it shows "a weird mock of the website, just the markdown text", plus a link to the live web. He wants the opposite: a near-perfect replica of the page, without the ads and pop-ups, captured as a snapshot frozen in time, opened in "a really nice browser" inside the app, working forever even if the site goes down, and "really fucking secure".

The answer: at citation time the app captures the page into **one self-contained HTML file** (every stylesheet, image and font inlined as `data:` URIs; every script, frame, form and tracker removed; every link made inert and routed through the app), records its SHA-256, and serves it back to a **fully sandboxed iframe through a private `lattice-snapshot://` URL scheme** whose response carries a `default-src 'none'` Content Security Policy. The viewer gets browser chrome (address pill, frozen-on date, Reader/Page toggle, clean-view toggle, zoom, integrity line) and a link bar that offers to open a clicked link in the real browser. Nothing inside the frozen page can run code, reach the network, reach the app, or navigate anywhere. Snapshots are durable background jobs, are swept with their conversation, and travel in the encrypted backup.

## 1. What exists today

| Piece | Where | What it does |
| --- | --- | --- |
| Text snapshot per citation | `conversation_web_sources.content` (64k chars, first write wins) via `features/conversation/chat/source_snapshots.rs` | Archives the extracted *text* of every page a conversation read |
| Reader view | `src/features/reading/components/WebArticleView.tsx`, mounted by `SourceReaderBody.tsx` | Renders that text as paragraphs with cited-passage marks. This is the "weird mock" |
| HTTP page read | `features/web/services/web/page.rs` (`read_page`), `address_guard.rs` (SSRF guard, redirect policy, public-only resolver), `stealth.rs` (client builder, browser headers, pacing) | Fetches and extracts page text; falls back to the hidden browser on 403/503 or thin text |
| Hidden browser reader | `features/web/services/browser_reader.rs` | Loads a page in an incognito, capability-less hidden window; the page reports its text by navigating to `lattice-page-reader.invalid#…`, intercepted by `on_navigation` |
| Saved web articles (imports) | `features/web/services/archive/` (`~/.lattice/web-archive/<domain>/<slug>-<uuid>/{article.md,…}`), `HTMLViewer.tsx` (srcdoc iframe with `allow-same-origin`) | A *different* feature: imported articles as library documents |
| Durable jobs | `shared/runtime/jobs` (`JobRuntime`, `JobHandler`, `JobKindConfig`, `NewJob`) | Persisted background work with retry/backoff, used by explorer indexing and learning |
| Backups | `features/backup/archive` packs `files/**`, `vault/**`, `web-archive/**` and the DB | Encrypted archive |
| App CSP | `crates/lattice-desktop/tauri.conf.json` `app.security.csp` | No `frame-src`, so `default-src 'self'` governs frames today |

The Reader view's passage marks are good and stay. The new work adds a second way of looking at the same citation, the **Page** view, and the capture behind it.

## 2. Goals and non-goals

Goals

1. Fidelity: the Page view looks like the site did, with its layout, type, images and fonts.
2. Permanence: a snapshot never changes after capture, never depends on the network, and outlives the site.
3. Security: a captured page is inert. No script, no network, no IPC, no navigation, no storage, no referrer. A hostile page cannot touch the app or the user's machine either at capture or at view time.
4. Quiet: ad slots, consent walls, newsletter pop-ups and scroll locks are gone or hidden by default, reversibly.
5. Honest: the viewer says when it was captured, whether the copy is complete, and whether the stored file still matches its recorded hash.
6. No new infrastructure: no hosted service, no Docker, no second job system, no second storage system.

Non-goals (now)

- Archiving video or audio streams. Posters are kept; media sources are dropped.
- Re-capturing a page that was captured successfully. First capture wins, as the text snapshot already does. A failed or missing capture may be retried.
- Full-text search inside the Page view. The Reader view keeps passage marks and matching.
- A WARC/MHTML export. The single-file HTML is already a portable artifact; an "Export snapshot…" menu item is a follow-up.
- Capturing pages behind a login. Captures carry no cookies.

## 3. Research digest

Formats and tools considered:

- **SingleFile** (browser extension and CLI; AGPL) is the de-facto standard for "one HTML file": it inlines resources as base64 `data:` URIs, removes scripts by default because they "can alter the rendering and there is no guarantee they will work offline", loads deferred (lazy) images before saving, and produces files that are large because of base64. Professional archivists prefer WARC. We take the technique, not the code (license, and it is a 1 MB JS bundle built for a browser context).
- **freeze-dry** (WebMemex, MIT) snapshots the live DOM "as currently shown, after scripts have run", fetches subresources and inlines them as `data:` URLs, removes scripts and interactivity, "disables anything that could change the page, such as scripts and network access", expands relative links and timestamps the snapshot. Its Memento conventions (`<meta name="memento-datetime">`, `<link rel="original">`, RFC 7089) are adopted here so a snapshot declares what it is.
- **MHTML** (Chrome "save complete", CDP `Page.captureSnapshot`) and Apple **.webarchive** (`WKWebView.createWebArchiveData`) are platform formats. Neither renders in a plain `<iframe>`, both keep scripts, and `.webarchive` is macOS-only. Rejected.
- **WARC + replay** (pywb, Webrecorder, ArchiveBox) is the archival gold standard but needs a replay server rewriting URLs at view time, which reintroduces a live-ish network surface. Rejected for a desktop app whose promise is "inert".
- **Sandboxing**: the `sandbox` attribute without `allow-scripts` and without `allow-same-origin` gives the frame an opaque origin, no script execution (including `javascript:` URLs), no forms, no popups, no top navigation and no downloads. A CSP `sandbox` directive on the response applies the same even if the attribute were lost. WebKit and Blink additionally enforce the embedding page's CSP inside sandboxed `srcdoc` frames; we do not rely on inheritance because the snapshot document is served with its own policy.
- **frame-src and in-frame navigation**: Chromium checks `frame-src` against the parent for every subframe navigation (it moved the check into a browser-side NavigationThrottle so redirects cannot bypass it); the spec text and WebKit are less explicit, and a Mozilla bug argues the navigating frame itself is the fetch client. Conclusion: the parent's `frame-src` is a helpful extra layer, not the mechanism. The mechanism is that *no navigation initiator survives sanitization* except anchors, and anchors are rewritten to the app's own scheme (see §8.3).
- **204 on navigation**: all desktop browsers keep the current page when a navigation response is `204 No Content`. The link endpoint relies on this and the viewer still snaps the frame back if a frame `load` ever fires after the first.
- **lol_html 3.0.1** (Cloudflare, BSD-3-Clause): streaming HTML rewriter with a CSS-selector API, used in production at Cloudflare's edge. It rewrites attributes, removes elements with their content, and hands text chunks per element, which is exactly the two-pass collect/rewrite shape below, with everything it is not told to touch passed through byte-for-byte (fidelity).

Sources: SingleFile FAQ (git.tdem.in mirror of gildas-lormeau/SingleFile/faq.md); gwern.net/gwtar; WebMemex/freeze-dry README; CSP3 and WHATWG HTML sandboxing; Chromium review 2022083002 (frame-src NavigationThrottle); Mozilla bug 1557114; WebKit bug 60206 (204 handling); crates.io lol_html; docs.rs tauri 2.11.1 `Builder::register_asynchronous_uri_scheme_protocol`.

## 4. Architecture

```
 citation stored ──► job: web_page_snapshot ──► Capture ──► Sanitize ──► Store ──► (event) ──► Viewer
 (first text        (JobRuntime, kind-level     HTTP fetch   lol_html     ~/.lattice/        React pane
  snapshot wins)     concurrency 2, retry)      + resources  two-pass     web-snapshots/     <iframe sandbox>
                                                 │                        <id>.html          src = lattice-snapshot://
                                                 ▼                        + DB row (sha256)  localhost/<id>/index.html
                                          hidden browser                        ▲
                                          (wave 2: thin or                      │ verify hash, add headers (CSP), serve
                                           blocked pages)                lattice-snapshot:// protocol handler (Rust)
```

Modules (new unless marked):

| Module | Owner | Role |
| --- | --- | --- |
| `crates/lattice-desktop/src/features/web/services/snapshot/` (`mod.rs`, `capture.rs`, `sanitize.rs`, `css.rs`, `resources.rs`, `clutter.rs`, `tests.rs`) | Agent A | Pure capture engine: URL in, `CapturedPage` out |
| `crates/lattice-desktop/src/features/web/traits/page_snapshot.rs` | A (trait), B (consumer) | `PageCapturePort` |
| `crates/lattice-desktop/src/features/web/snapshots/` (`mod.rs`, `store.rs`, `repository.rs`, `job.rs`, `protocol.rs`, `gc.rs`, `dto.rs`, `commands.rs`) | Agent B | Storage, DB, job, protocol handler, commands, GC, backup |
| `crates/lattice-desktop/migrations/20261013200000_web_page_snapshots.sql` | B | Table |
| `crates/lattice-desktop/src/main.rs` (edit) | B | `register_asynchronous_uri_scheme_protocol("lattice-snapshot", …)` |
| `crates/lattice-desktop/tauri.conf.json` (edit) | C | `frame-src lattice-snapshot: http://lattice-snapshot.localhost` |
| `src/features/reading/snapshot/` (`WebPageSnapshotView.tsx`, `SnapshotToolbar.tsx`, `LinkBar.tsx`, `useWebPageSnapshot.ts`, `snapshotFrameUrl.ts`, `webPageSnapshotStore.ts`, tests) | Agent C | Viewer |
| `src/features/reading/components/SourceReaderBody.tsx` (edit) | C | Reader/Page toggle, mounts the viewer |
| `features/web/services/browser_reader.rs` (extend) + `snapshot/browser_capture.rs` | Wave 2 | Rendered-DOM capture |

## 5. Threat model and security architecture

Assume every captured page is hostile. Two moments matter: capture (the app talks to the internet on the user's behalf) and view (the app renders attacker-controlled bytes inside its own window).

Layers at view time, innermost first. Any one of them alone would stop script execution; all are present.

1. **Sanitizer** removes everything executable or active (§6.4).
2. **Served CSP** `default-src 'none'; img-src data:; media-src data:; font-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; sandbox` on the protocol response, and the same policy as a `<meta http-equiv>` inside the file so an exported copy protects itself.
3. **Frame sandbox** `<iframe sandbox>` with the empty token list: opaque origin, no scripts, no forms, no popups, no top navigation, no downloads, no pointer lock, no modals.
4. **Origin separation**: the document's origin is `lattice-snapshot://localhost` (Windows: `http://lattice-snapshot.localhost`), not the app's `tauri://localhost`. Tauri's IPC is not injected into foreign origins with no capability, and the capability file names the `main` window and the app origin only.
5. **Parent CSP** `frame-src lattice-snapshot: http://lattice-snapshot.localhost` so the frame can only ever show snapshot documents.
6. **Viewer watchdog**: the viewer resets the iframe to its snapshot URL on any `load` event after the first.

| # | Threat | Mitigation |
| --- | --- | --- |
| T1 | Captured page runs code in the app | Layers 1–4. Also `javascript:` in any URL attribute is dropped by the sanitizer and blocked by the sandbox |
| T2 | Captured page phones home when viewed (tracking pixels, fonts, CSS `url()`, `@import`, prefetch) | Every external reference is inlined at capture or dropped; served CSP forbids every network scheme; `Referrer-Policy: no-referrer` header and `<meta name="referrer" content="no-referrer">` |
| T3 | SSRF during capture (page or resource at `localhost`, `169.254.169.254`, a LAN host, a redirect to one, DNS rebinding) | Reuse `address_guard`: `validate_url_resolved` before the first request, `redirect_policy()` on every hop, `PublicOnlyResolver` on the client; every subresource URL goes through the same client; only `http`/`https` |
| T4 | Resource exhaustion (decompression bombs, 10 000 images, 500 MB font) | Hard caps: 8 MiB per resource, 48 MiB per snapshot, 400 resources, 60 s per capture, 15 s per resource, 8 concurrent; bodies read as streams and cut at the cap (`content-length` is advisory, not trusted); CSS `@import` depth 3; over-cap resources are dropped and counted, never partially inlined |
| T5 | Path traversal or id injection through the URL scheme | Snapshot ids are 32 lowercase hex characters, validated by regex; the file path is derived from the id only; every other path is a 404 with an empty body |
| T6 | Tampering with a stored file | SHA-256 recorded at write; verified at every serve; mismatch returns a small static "integrity check failed" page and the viewer shows it in red |
| T7 | Content spoofing: a page that claims to be another URL | The requested URL and the final URL after redirects are recorded by the fetcher, never read from the page; the viewer shows both when they differ |
| T8 | Credential leakage | Captures send no cookies and no auth; the stealth client is a fresh profile; private hosts are refused so an intranet page can never be snapshotted |
| T9 | Hostile page in the hidden browser (wave 2) | Existing containment (incognito, no capabilities, navigation guard, no downloads, destroyed after use); the transport is one-way and size-capped; Rust fetches resources itself |
| T10 | CSS exfiltration (attribute selectors with `url()` beacons) | All `url()` either inlined or removed; nothing external can survive to be requested |
| T11 | MIME confusion (an "image" that is HTML) | `data:` URIs are built only from an allowlist of fetched content types (`image/*` minus `image/svg+xml` is allowed as `<img>` only, `font/*`, `application/font-*`, `application/x-font-*`, `text/css` is never inlined as data, it is inlined as `<style>`); `X-Content-Type-Options: nosniff` on the response |
| T12 | SVG with scripts/handlers | Inline `<svg>` is sanitized like HTML (`script`, `foreignObject` content, `on*`, SMIL `animate`/`set`/`animateMotion`/`animateTransform`/`discard` removed, `href`/`xlink:href` filtered). SVG delivered through `<img src="data:image/svg+xml…">` or CSS cannot run script by specification |
| T13 | Snapshot served to the wrong webview (a hostile page in a hidden reader window requesting `lattice-snapshot://…`) | The handler answers only when `ctx.webview_label() == "main"`; everything else is 404 |
| T14 | Disk growth | Snapshots are visible per conversation; a startup sweep removes files without rows; conversation deletion cascades the rows and triggers a sweep; typical article 1–6 MiB |

Not mitigated, by decision: at-rest encryption of snapshot files. The SQLite database beside them is plaintext too; the encrypted backup covers export. Revisit together with database encryption.

## 6. Capture pipeline

### 6.1 Triggers and the first-wins rule

- **After a turn**: `attach_cited_web_snapshots` (source_snapshots.rs) stores text snapshots for cited URLs. Immediately after each successful `store_web_source_snapshot`, and never on the turn's critical path, the chat layer asks the snapshot feature to `ensure_capture(conversation_id, url)`. That submits a job if no row exists for `(conversation_id, normalized_url)`; an existing row in any state is left alone except `failed`, which is retried only on explicit user action.
- **Manual**: the viewer's "Capture now" (no row) and "Try again" (failed row) call `capture_web_page_snapshot(conversationId, url)`, which does the same `ensure_capture` and returns the row.
- **Older citations** (before this feature) show "No frozen copy" with "Capture now".
- The normalized URL is `normalize_web_source_url` from `repository/workspace/sources.rs` so a page snapshot and its text snapshot key the same way.

### 6.2 Fetching the document

- Client: `stealth::stealth_client_builder()` with `.redirect(redirect_policy())` and `.dns_resolver(Arc::new(PublicOnlyResolver::new(stealth::proxy_hosts())))`, exactly like `WebService::new`. Reuse through a small `pub(crate) fn snapshot_client()` added beside `WebService::new` rather than a second builder (one way to do a thing). `validate_url_resolved(url)` before the first request.
- Headers: `stealth::browser_headers(profile, None)` for the document; subresources send the same profile with `Referer` = final document URL and an `Accept` matching the resource kind (`image/avif,image/webp,image/*,*/*;q=0.8` for images, `text/css,*/*;q=0.1` for CSS, `*/*` for fonts).
- Status: only 2xx is captured. 403/503 and 429 are "blocked" (wave 2 tries the browser); 4xx/5xx otherwise is `failed` with the status in the error.
- Bytes: read as a stream, stop at 16 MiB for the document. Decode with the charset from `Content-Type`, else `<meta charset>`/`http-equiv` sniff (`encoding_rs`, as `extract.rs` already does), else UTF-8 lossy. Everything downstream is UTF-8 and the emitted file declares it.
- Content type must be `text/html` or `application/xhtml+xml`. A PDF or image URL is `failed` with `unsupported_content_type` (the Reader view still shows the text snapshot).

### 6.3 Two passes

**Pass 1, collect** (lol_html, read-only handlers): resolve every candidate resource URL against the document's final URL and record it with its role:

| Source | Role |
| --- | --- |
| `img[src]`, `img[srcset]` (chosen candidate), lazy attributes (`data-src`, `data-lazy-src`, `data-original`, `data-srcset`, `data-lazy-srcset`) when `src` is absent, empty, or a placeholder (`data:` under 1 KiB) | image |
| `picture > source[srcset]` | dropped (the `img` inside is kept) |
| `video[poster]` | image |
| `svg image[href]`, `svg image[xlink:href]`, `svg use[href]` (external) | image / dropped |
| `link[rel~=stylesheet][href]` (honouring `media`) | stylesheet |
| `style` text and `style=""` attributes: `url(...)` and `@import` | image / font / stylesheet |
| `link[rel~=icon]`, `link[rel=apple-touch-icon]` | favicon (stored on the row, ≤ 16 KiB, not inlined) |
| `input[type=image][src]`, `object`, `embed`, `iframe`, `audio`, `video source`, `track` | dropped |

srcset choice: parse candidates; prefer the largest `w` descriptor ≤ 1600, else the largest `x` ≤ 2, else the first. Write it to `src`; remove `srcset`, `sizes`, `loading`, `decoding`, `fetchpriority`.

**Fetch** (async, between passes): dedupe by absolute URL; 8 concurrent; per-resource 15 s and 8 MiB; total 48 MiB and 400 resources; the whole capture 60 s. A stylesheet is fetched, decoded, its own `url()`/`@import` resolved against *its* URL and fetched (depth ≤ 3), then rewritten to a string. Record per URL: `Inlined { bytes, mime }`, `Dropped { reason }` where reason ∈ `too_large`, `over_budget`, `blocked_host`, `bad_type`, `http_<status>`, `timeout`, `network`.

**Pass 2, rewrite** (lol_html): apply §6.4 and substitute `data:` URIs. A dropped image keeps its `alt`, `width` and `height` and gets `src` removed and `data-lattice-missing="<reason>"`, so layout holds and the viewer can count. A dropped stylesheet becomes nothing.

Pass 1 and pass 2 are the same rule table read twice; keep them in one module so they cannot drift.

### 6.4 Sanitization rules

Removed with their content: `script`, `noscript` (see rule N), `iframe`, `frame`, `frameset`, `object`, `embed`, `applet`, `portal`, `fencedframe`, `form`, `input`, `button`, `select`, `textarea`, `datalist`, `option`, `base`, `meta[http-equiv]` (all of them), `meta[charset]` (we write our own), `link` (every rel; stylesheets are replaced by `<style>` in place; icons go to the row), `template` *without* `shadowrootmode`, `audio`, `video source`, `track`, `dialog`, `slot` kept, `math` kept, SMIL `animate`, `animateMotion`, `animateTransform`, `set`, `discard`, `svg script`, `svg foreignObject` (content stripped; fidelity loss accepted), `canvas` (replaced by an `img` only in wave 2), `marquee`, `blink`.

Kept: everything else, including `style`, `svg`, `picture` (its `source` children removed), `details`/`summary`, `dialog[open]`'s content becomes a plain `div`, `template[shadowrootmode]` (declarative shadow DOM, wave 2 relies on it).

Attributes:

- Removed everywhere: every `on*`, `ping`, `target`, `download`, `formaction`, `formmethod`, `autofocus`, `contenteditable`, `draggable`, `nonce`, `integrity`, `crossorigin`, `referrerpolicy`, `usemap` stays, `is`, `slot` stays, `srcdoc`, `manifest`, `background`, `dynsrc`, `lowsrc`, `codebase`, `data`, `archive`, `classid`.
- URL attributes: `src`, `href`, `xlink:href`, `poster`, `cite`, `longdesc`, `action`. Rule: the value is parsed against the document URL; a scheme other than `http`, `https` or `data` (and `data` only where inlining is allowed) is dropped. `href` on `a`/`area`/`svg a` is **rewritten** (§8.3); `href` elsewhere is dropped (links are already gone). `src`/`poster`/`xlink:href` become `data:` or are dropped. `cite`/`longdesc` are dropped.
- `style=""`: kept; `url()` rewritten or the declaration dropped; declarations containing `expression(`, `-moz-binding`, `behavior:` or `javascript:` dropped.
- `id`, `class`, `lang`, `dir`, `title`, `alt`, `width`, `height`, ARIA, `data-*` (except the lazy attributes consumed above): kept, because CSS and screen readers need them.

Text in `style` elements and fetched stylesheets: `@import` replaced by the imported sheet's rewritten text; `url()` rewritten to `data:` or the whole declaration dropped; `@namespace` kept; `@font-face` kept with each `src: url()` inlined (a font over 8 MiB drops that `src` entry only); `expression(`/`-moz-binding`/`behavior:` dropped; `@media`, `@supports`, `@layer`, `@container` passed through. A CSS parser is not needed: a tokenizer for `url(`, strings and comments is enough and avoids a heavy crate. Comments are stripped (they can hide `url(`).

Rule N, `noscript`: in the HTTP path its content would render in our scriptless frame. Keep `noscript` content only if it contains an `img` or `picture` (the lazy-loading fallback) and strip everything else inside it; otherwise remove it entirely. In the browser path (wave 2) remove it always. (lol_html may deliver `noscript` content as text; if so run the rewriter on that text and reinsert.)

Rule C, charset: the emitted document begins `<!doctype html>` and the head begins with `<meta charset="utf-8">`.

Rule E, envelope: prepended into `<head>` (created if the page has none):

```html
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; media-src data:; font-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<meta name="referrer" content="no-referrer">
<meta name="lattice-snapshot" content="1">
<meta name="memento-datetime" content="Tue, 13 Oct 2026 21:04:11 GMT">
<link rel="original" href="https://example.com/the/page">
<style id="lattice-base">/* §6.6 base layer */</style>
```

The `<meta http-equiv="Content-Security-Policy">` is the one `http-equiv` we write ourselves, after all page-authored ones are removed.

### 6.5 Output

```rust
pub struct CapturedPage {
    pub requested_url: String,
    pub final_url: String,
    pub title: Option<String>,
    pub html: String,                 // the complete self-contained document
    pub favicon: Option<DataUri>,     // ≤ 16 KiB, mime + bytes
    pub method: CaptureMethod,        // Http | Browser
    pub resources: ResourceReport,    // inlined, dropped (with reasons), bytes_inlined
    pub partial: bool,                // any cap or budget was hit
    pub partial_reasons: Vec<String>,
    pub captured_at: DateTime<Utc>,
}

pub struct CaptureRequest {
    pub url: String,
    pub snapshot_id: String,          // the sanitizer writes it into every rewritten link (§8.3)
}

#[async_trait]
pub trait PageCapturePort: Send + Sync {
    async fn capture(
        &self,
        request: CaptureRequest,
        progress: Arc<dyn Fn(CaptureProgress) + Send + Sync>,   // { done, total, message }, called at most a few times a second
        cancel: CancellationToken,
    ) -> std::result::Result<CapturedPage, CaptureError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("the site answered {status}")] Blocked { status: u16 },      // 403, 429, 503: a browser may get through (wave 2)
    #[error("unsupported content type {mime}")] UnsupportedContentType { mime: String },
    #[error("the page is larger than the capture limit")] TooLarge,
    #[error("the capture ran out of time")] Timeout,
    #[error("network error: {0}")] Network(String),
    #[error("refused: {0}")] Refused(String),                            // SSRF guard, bad URL, non-2xx that is not Blocked
    #[error("cancelled")] Cancelled,
}
impl CaptureError { pub fn is_transient(&self) -> bool { matches!(self, Self::Network(_) | Self::Timeout) } }
```

`Blocked` is the signal wave 2 uses to try the browser.

### 6.6 Clutter: what is removed and what is only hidden

Removed at capture (not content, irreversible is fine): `ins.adsbygoogle`, `[data-ad-slot]`, `[data-ad-client]`, `[id^="google_ads_"]`, `[id^="div-gpt-ad"]`, `[class~="adsbygoogle"]`, `[aria-label="advertisement" i]`, `amp-ad`, `amp-embed`, elements whose `id` or `class` *token list* contains exactly one of `ad`, `ads`, `advert`, `advertisement`, `advertising`, `ad-slot`, `ad-unit`, `ad-wrapper`, `ad-container`, `sponsored-content`, `taboola`, `outbrain`, `mgid`, `dfp`. Token matching means `address`, `header`, `loader`, `shadow` never match.

Hidden by the **clean layer** (a second `<style id="lattice-clean">` appended by the protocol handler when `?clean=1`; the viewer's "Clean" toggle, default on, so it is reversible per view):

```css
[class*="cookie" i], [id*="cookie" i], [class*="consent" i], [id*="consent" i],
[class*="gdpr" i], [class*="newsletter" i][class*="modal" i], [class*="newsletter" i][class*="popup" i],
[class*="paywall" i], [id*="paywall" i], [class*="subscribe-modal" i], [class*="interstitial" i],
[class*="modal-backdrop" i], [class*="overlay" i][class*="modal" i], [role="dialog"], [aria-modal="true"],
[class*="sticky-footer" i], [class*="smartbanner" i] { display: none !important; }
html, body { overflow: auto !important; height: auto !important; position: static !important; }
* { cursor: default !important; }
a, area { cursor: pointer !important; }
```

The base layer (`lattice-base`, always present):

```css
html { scrollbar-gutter: stable; }
img[data-lattice-missing] { background: repeating-linear-gradient(45deg, #eee 0 6px, #f6f6f6 6px 12px); min-height: 2em; }
```

Animations are left alone on purpose: sites that reveal content through an opening animation would stay invisible if animations were killed.

### 6.7 Rendered-DOM capture (wave 2)

For pages the HTTP path reports `Blocked`, or whose sanitized text (rendered-text estimate: text nodes outside `style`) is under 400 characters, capture through the hidden browser:

1. `browser_reader` gains a `capture_dom(url, timeout) -> Result<RenderedDom { final_url, title, html }>` beside `read`.
2. The injected script, after the existing settle logic: scroll the document in viewport steps to the bottom and back (≤ 5 s, 250 ms per step) to fire lazy loaders; await `document.fonts.ready`; for every `img` set `src = currentSrc` and remove `srcset`/`sizes`/`loading`; write live `value`/`checked` into attributes; convert untainted `canvas` to `<img src=data:>`; serialize with `document.documentElement.getHTML({ serializableShadowRoots: true })` when available, else `outerHTML`.
3. Transport: the page cannot talk to the app, so it reports by navigating to the reserved host in chunks. Each `location.href = 'https://lattice-page-reader.invalid/#c;<nonce>;<seq>;<total>;<base64url>'` is cancelled by `on_navigation`; Rust stores the chunk and `eval`s `window.__latticeAck(<seq>)`, and only then does the page send the next one. Chunks are 192 KiB; total is capped at 24 MiB; a missing ack for 10 s fails the capture. Ack-driven sending is required because a second navigation issued before the first's policy decision cancels the first.
4. The rendered HTML then goes through the same sanitizer and resource fetch as the HTTP path, with `noscript` removed outright and `method = Browser`.

## 7. Storage and integrity

Directory: `~/.lattice/web-snapshots/` beside `files/` and `web-archive/` (the same root the backup packs). File: `<id>.html`, written to `<id>.html.part` and renamed, 0600. `id` = `uuid::Uuid::new_v4().simple()`.

```sql
CREATE TABLE web_page_snapshots (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    normalized_url TEXT NOT NULL,
    requested_url TEXT NOT NULL,
    final_url TEXT,
    title TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending','ready','failed')),
    method TEXT CHECK (method IS NULL OR method IN ('http','browser')),
    captured_at TEXT,
    byte_size INTEGER,
    sha256 TEXT,
    resource_count INTEGER NOT NULL DEFAULT 0,
    missing_resource_count INTEGER NOT NULL DEFAULT 0,
    partial INTEGER NOT NULL DEFAULT 0,
    partial_reasons TEXT,
    error_code TEXT,
    error_message TEXT,
    favicon_data_url TEXT,
    job_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (conversation_id, normalized_url)
);
CREATE INDEX idx_web_page_snapshots_conversation ON web_page_snapshots(conversation_id);
```

A new dated migration; the squashed init file is not touched.

Integrity: `sha256` and `byte_size` are written in the same transaction that flips `status` to `ready`, after the rename. The protocol handler hashes the file on every serve (a 5 MiB file hashes in a few milliseconds) and refuses on mismatch. The viewer's status line reads "Verified" from a `verified: true` field the handler cannot fake because the viewer asks the command, not the frame.

GC: `WebSnapshotGc::sweep()` lists the directory, deletes `.part` files older than one hour and `.html` files with no `ready`/`pending` row, and reports counts. Runs at startup beside `library_gc.sweep()` in `infrastructure/setup/app.rs`, and after `delete_conversation`.

Backup: the planner packs `web-snapshots/**` with the prefix `web-snapshots/`; restore puts them back beside `web-archive`. Rows ride in the DB as usual.

## 8. Serving: the `lattice-snapshot` URL scheme

### 8.1 Registration

`main.rs`: `.register_asynchronous_uri_scheme_protocol("lattice-snapshot", |ctx, request, responder| { … })`. The handler clones the app handle, spawns onto the Tokio runtime, resolves `Container` from `app.state()`, and responds through `responder.respond(...)`.

### 8.2 URL grammar

- `GET /<id>/index.html[?clean=1]` → the snapshot (the frontend builds it with `convertFileSrc('<id>/index.html', 'lattice-snapshot')`, so the path may arrive percent-encoded; decode once, then validate).
- `GET /l?u=<percent-encoded absolute http(s) URL>&s=<id>` → link click, §8.3.
- Anything else, any other method, any other webview label → `404`, empty body, `Cache-Control: no-store`.

### 8.3 Links

The sanitizer rewrites every `a[href]`, `area[href]` and `svg a[href|xlink:href]` with an `http(s)` target to `href="/l?u=<encoded absolute url>&s=<snapshot id>"` and stores the original in `data-lattice-href` (so Reader-mode tooling and exports can still see it). Fragment-only links (`#section`) stay as they are: same-document navigation is harmless and useful. `mailto:`, `tel:` and everything else are dropped (the anchor stays, unlinked).

The handler for `/l` validates `u` (http/https, under 2048 chars) and `s`, emits `web-page-snapshot:link` `{ snapshotId, url }` to the main window and responds `204 No Content`. Desktop WebKit, Blink and Gecko stay on the current page for a 204, so the frozen page does not move. The viewer shows the link bar (§9). If a `load` event ever fires on the iframe after its first, the viewer sets `src` back to the snapshot URL.

Since the sanitizer needs the id to write links, the store allocates the id and passes it to capture through `CaptureOptions { snapshot_id }`.

### 8.4 Response

```
200 OK
Content-Type: text/html; charset=utf-8
Content-Security-Policy: default-src 'none'; img-src data:; media-src data:; font-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; sandbox
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
Cache-Control: no-store
Content-Length: …
```

Body: the stored file, with `<style id="lattice-clean">…</style>` inserted before `</head>` when `clean=1` (string insertion; the file on disk is never modified).

Errors are tiny static HTML pages with the same headers: `409` integrity mismatch ("This frozen copy no longer matches its recorded fingerprint and was not shown."), `404` unknown id, `425` while `pending`.

### 8.5 App CSP change

`tauri.conf.json` `app.security.csp` gains `frame-src lattice-snapshot: http://lattice-snapshot.localhost;`. Nothing else changes. The UI rig does not load `tauri.conf.json`, so this line is verified by a real `tauri dev` run (Josh runs the branch live).

## 9. The viewer

Mounted by `SourceReaderBody` in place of `WebArticleView` when the source is a web citation and the Page mode is selected. Both the chat reader pane and the file preview modal use `SourceReaderBody`, so both get it.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [Reader | Page]   🔒 ▣ nytimes.com /2026/10/… ▾   ❄ Frozen Oct 13, 2026   │
│                                              [✦ Clean] [− 100% +] [⋯]    │
├──────────────────────────────────────────────────────────────────────────┤
│ ↗ This page links to example.com/path    [Open in browser] [Frozen copy] ×│  ← link bar, only after a click
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│                       <iframe sandbox>  the page                          │
│                                                                          │
├──────────────────────────────────────────────────────────────────────────┤
│ ✓ Verified · 2.4 MB · 38 resources, 2 missing · captured over HTTP       │
└──────────────────────────────────────────────────────────────────────────┘
```

Anatomy and behaviour

- **Mode toggle** `Reader | Page` (segmented, `role="tablist"`). Default: Page when a `ready` snapshot exists, else Reader. The choice is remembered for the session in `webPageSnapshotStore` (`mode`, `zoom`, `clean`). Tooltip on Reader: "Text with the cited passages marked". On Page: "The page as it was captured".
- **Address pill**: favicon (from the row), host in the primary colour, path muted, single line, ellipsised in the middle. Click copies the URL and shows "Copied". When `finalUrl` differs from `requestedUrl` a small `↪` with tooltip "Redirected from …".
- **Frozen chip**: `❄ Frozen Oct 13, 2026` with the exact timestamp in the tooltip. Text uses the app's existing date formatters.
- **Clean toggle**: on by default; toggles `?clean=1` on the frame URL (a reload of the frame, nothing else). Tooltip: "Hide cookie walls, pop-ups and overlays".
- **Zoom**: 50–200 % in steps of 10; `−`, value, `+`; implemented as `transform: scale(z)` with `transform-origin: 0 0` and `width/height: calc(100% / z)` on the iframe. `⌘=`/`⌘-`/`⌘0` while the pane has focus.
- **Menu ⋯**: Open live page in browser (`openExternalUrl`), Copy link, Capture again (only when `failed`), Show in Reader.
- **Frame**: `<iframe sandbox="" referrerpolicy="no-referrer" title="Frozen copy of {host}" src={convertFileSrc(`${id}/index.html`, 'lattice-snapshot') + (clean ? '?clean=1' : '')}>`. The `sandbox` attribute is present and empty, always; a test asserts it. Background white with the app's rounded border; while the frame loads, a skeleton of three grey bars.
- **States**
  - `none`: illustration-free empty state: "No frozen copy of this page yet." + [Capture now] + "The text the answer used is still in Reader."
  - `pending`: "Capturing the page…" with the job's progress message under it ("12 of 40 resources") and a thin indeterminate bar. Live via the event.
  - `failed`: "Couldn't capture this page." + the human error (e.g. "The site answered 403, which usually means a human check.") + [Try again] + [Open live page].
  - `ready`: the frame; status line "✓ Verified · 2.4 MB · 38 resources, 2 missing · captured over HTTP". `partial` adds "· incomplete: <reasons>" in amber. Integrity failure replaces the frame with the handler's page and the status line turns red: "✗ Integrity check failed".
- **Link bar**: on `web-page-snapshot:link` for this snapshot id, slide in under the toolbar: "This page links to {host}{path}" with [Open in browser] [Open frozen copy] (only when another `ready` snapshot in this conversation has the same normalized URL; clicking switches the viewer to it) and ×. Auto-dismiss after 12 s. Links are never opened without this click.
- **Compact** (overlay/modal width < 560): toolbar collapses to icons with tooltips, status line hides, address pill keeps the host only.
- **Reader** keeps `WebArticleView` unchanged. The passage marks are a Reader feature; the Page tooltip says so if a user arrives with a clicked citation ("Marked passages are shown in Reader").
- **Theme**: chrome follows the app theme; the page keeps its own colours (no inversion).
- **Accessibility**: toggle is a tablist; the frame has a title; the link bar is `role="status"`; all buttons have labels; focus stays in the pane.

Copy is final as written above; agents do not invent alternative strings.

## 10. Jobs

Kind `web_page_snapshot`, `JobKindConfig::new(RecoveryPolicy::Requeue).concurrency(2).exclusive_per_subject().backoff(Backoff { initial: 20 s, max: 10 min })`. `subject_id` = snapshot row id. `operation_id` = `web-page-snapshot:<conversation_id>:<normalized_url>`. `requested` = `{ conversationId, url, snapshotId }`. Progress total 100: 10 after the document, then resources proportionally, 95 while writing, 100 on commit.

Handler: load the row; if not `pending` return `Completed` with the existing state (idempotent); run `capture(url, cancel)` under `until_cancelled`; write the file; commit `ready` + hash; `Completed { result_ref: snapshot_id }`. On `CaptureError::Network`/`Timeout` return the error and let `JobFailure::classify` defer; on `Blocked`/`UnsupportedContentType`/`TooLarge` mark the row `failed` with a code and return `JobOutcome::Failed`.

Events: the runtime already observes job changes; the snapshot feature emits `web-page-snapshot:changed` `{ conversationId, url, snapshot: WebPageSnapshotDto }` on every status flip and on progress, debounced to 4 per second, to the main window.

## 11. API surface

Commands (tauri-specta, registered in `features/web/plugin.rs` and `src/bin/export_bindings/main.rs`):

```rust
get_web_page_snapshot(conversation_id: String, url: String) -> Result<Option<WebPageSnapshotDto>, ApiError>
capture_web_page_snapshot(conversation_id: String, url: String) -> Result<WebPageSnapshotDto, ApiError>  // ensure or retry
list_web_page_snapshots(conversation_id: String) -> Result<Vec<WebPageSnapshotDto>, ApiError>              // for "Open frozen copy"
```

```rust
#[derive(Serialize, Deserialize, specta::Type)] #[serde(rename_all = "camelCase")]
pub struct WebPageSnapshotDto {
    pub id: String, pub conversation_id: String, pub requested_url: String, pub final_url: Option<String>,
    pub normalized_url: String, pub title: Option<String>,
    pub status: WebPageSnapshotStatus,          // "pending" | "ready" | "failed"
    pub method: Option<String>,                 // "http" | "browser"
    pub captured_at: Option<String>, pub byte_size: Option<u64>, pub sha256: Option<String>,
    pub resource_count: u32, pub missing_resource_count: u32,
    pub partial: bool, pub partial_reasons: Vec<String>,
    pub error_code: Option<String>, pub error_message: Option<String>,
    pub favicon_data_url: Option<String>,
    pub progress_message: Option<String>,       // while pending
    pub verified: Option<bool>,                 // ready rows: hash re-checked when the DTO was built
}
```

Events: `web-page-snapshot:changed` (above) and `web-page-snapshot:link` `{ snapshotId, url }`.

Frontend API: `src/features/reading/snapshot/api.ts` wrapping `commands.getWebPageSnapshot`, `commands.captureWebPageSnapshot`, `commands.listWebPageSnapshots`; event schemas added to `src/types/events.ts` with zod like the others.

## 12. Testing

Rust, unit (`cargo test --lib web::services::snapshot`, `web::snapshots`):

- sanitizer: script/handler/`javascript:` removal (HTML and SVG), meta refresh and page CSP meta removal, form removal, link rewrite to `/l?u=…&s=…` with `data-lattice-href`, fragment links untouched, `mailto:` unlinked, `noscript` rule N both ways, lazy `data-src` promotion, srcset choice (w and x descriptors), `picture > source` dropped, `style` url rewrite and declaration drop, `@import` inlining with depth cap, comment stripping, `expression(` drop, envelope present once and first, charset decoding from a windows-1252 page, clutter removal by token (and `address`/`header` untouched), oversize resource dropped with `data-lattice-missing`, budget hit sets `partial`.
- fetch policy: resource at a literal private IP refused without a request; `data:` over the cap dropped; wrong content type dropped (an HTML body claiming to be an image).
- store/repository: create pending → ready with hash; first wins (second ensure returns the same row); failed → retry creates a new job on the same row; cascade on conversation delete; sweep removes orphans and stale `.part` files and keeps live files.
- protocol: path parsing (valid id, traversal, wrong label → 404), headers on 200, 204 for `/l` plus event, 409 on tampered file, clean layer inserted only with `?clean=1`.

Frontend, vitest: viewer renders each state; iframe has `sandbox=""` and the expected `src`; clean toggle changes the URL; zoom changes the transform; link bar appears on the event and "Open in browser" calls `openExternalUrl`; mode toggle defaults; `SourceReaderBody` shows the toggle only for web citations. Type-check, lint and the existing `SourceReaderBody` tests stay green.

Live (Josh): a real `tauri dev` run confirms the `frame-src` line, the custom scheme in WKWebView, the 204 link behaviour, and that the hidden reader window cannot load the scheme.

## 13. Delivery plan

Branch `web/frozen-snapshots` in `/Users/josh/Code/lattice-snapshot` (worktree from `origin/main` at c35fd387). Shared `CARGO_TARGET_DIR=/Users/josh/Code/lattice-temp/target`; `.cargo/config.toml` caps jobs at 2. `node_modules` is a symlink to `lattice-temp`'s.

Wave 1, three Opus agents at once, disjoint files:

| Agent | Scope | Contract it must satisfy | Verification |
| --- | --- | --- | --- |
| A, capture engine | `features/web/services/snapshot/*`, `traits/page_snapshot.rs`, `Cargo.toml` (+`lol_html = "3"`), `snapshot_client()` beside `WebService::new`, trait re-export in `web/mod.rs` | §6.2–6.6, `CapturedPage`, `PageCapturePort`, `CaptureOptions { snapshot_id }` | `cargo check --lib`, `cargo test --lib web::services::snapshot` |
| B, storage/jobs/protocol | `features/web/snapshots/*`, migration, `main.rs` registration, `plugin.rs` + `export_bindings` command registration, `web/di.rs` wiring behind the trait with a `MockPageCapture` from `web/mocks/`, `source_snapshots.rs` hook (one call), `app.rs` sweep, `delete_conversation` sweep, backup prefix | §7, §8, §10, §11 | `cargo check --lib`, `cargo test --lib web::snapshots` |
| C, viewer | `src/features/reading/snapshot/*`, `SourceReaderBody.tsx` edit, `src/types/events.ts`, `tauri.conf.json` CSP, hand-written stubs for the three commands and the DTO in `src/lib/bindings.ts` in the generated style (regenerated at integration) | §9, §11 | `npx vitest run src/features/reading`, `npx tsc --noEmit`, `npx eslint` on touched files |

Integration (main session): merge the three, regenerate bindings (`cargo run --features bindings-export --bin export_bindings`), `cargo check --all-targets`, `cargo test --lib --bins` for `web::`, `conversation::chat::source_snapshots`, vitest for `src/features/reading`, type-check, lint, architecture checker. Security read of `sanitize.rs`, `css.rs`, `protocol.rs` by the main session.

Wave 2, one Opus agent: §6.7 browser capture, the thin-page fallback in the job handler, and `method = browser` in the viewer's status line.

Then: PR to `main`, authored as Josh, no trailers.

Budget rules for every agent brief: read only the sections of this document named in the brief and only the files listed; grep before reading; read line ranges; never re-read; at most four cargo invocations; pipe output through `tail -n 20`; report as a table plus verbatim failure text, ≤ 200 words.

## 14. Decisions and rejected alternatives

| Decision | Alternative rejected | Why |
| --- | --- | --- |
| One self-contained HTML file per snapshot | Directory of assets + rewritten HTML | One file hashes, backs up, exports and serves trivially; base64 bloat (~33 %) is acceptable at a few MiB per page |
| Private URL scheme + sandboxed iframe | `srcdoc` iframe | `srcdoc` forces the whole document into a DOM attribute (memory doubling at 20 MiB), inherits the app origin's CSP in ways that differ per engine, and has no response headers to carry our own policy; the scheme gives a foreign origin, our headers, and integrity checks on every load |
| Custom scheme named `lattice-snapshot` | Tauri's `asset` protocol | `asset` serves raw files with the app's CSP and no per-response headers, and its scope is a path glob, not an id check |
| Links rewritten to `/l` with a 204 | Links made inert (`href` removed) | Users expect links to do *something*; the link bar makes the action explicit and the live site is only ever opened in the real browser after a click |
| Clean layer applied at serve time | Baked into the file | Reversible per view and the stored artifact stays faithful |
| lol_html | ammonia, scraper mutation, a hand parser | ammonia escapes `<style>` text and allowlists aggressively (fidelity loss); scraper's tree is awkward to mutate; lol_html streams, preserves bytes it is not told to touch, and is maintained by Cloudflare |
| Jobs runtime for capture | `tokio::spawn` with a semaphore | Durable across restarts, retry/backoff and progress exist already; a second mechanism would violate "one way to do each thing" |
| First capture wins | Re-capture on demand | Matches the text snapshot's rule and the product's provenance stance: a page that changed is new information |
| No at-rest encryption | Encrypt snapshot files | The database beside them is plaintext; encrypting one artifact alone buys nothing yet |

## 15. Follow-ups, not in this build

- "Export snapshot…" (the file as-is, through the dialog plugin).
- Settings: "Keep frozen copies of cited pages" toggle and a storage total in Settings → Storage.
- Snapshot support in the Learning sources reader and the Journal entry reader (both show web sources; the viewer is reusable once a conversation id is at hand).
- Find-in-page for the Page view (needs an in-frame script, which we deliberately forbid; a Rust-side text index with scroll-to is the alternative).
- `<use href="sprite.svg#id">` external sprite inlining.
- Capturing `<video poster>`-less media as a poster-only placeholder card.

## 16. As built (2026-10-14)

Wave 1 was built by three Opus agents from this document, then integrated and security-read in the main session. What differs from the sections above:

Capture engine (§6)

- `form` keeps its content; only the tag and its controls go (sites wrap whole bodies in one form).
- `<base href>` is honoured for URL resolution, then removed, as browsers do.
- Fonts served as `application/octet-stream` (or untyped) are kept when their first bytes carry a WOFF/WOFF2/TTF/OTF signature.
- Also refused in CSS for T10: `image-set(`, `vbscript:`, backslash escapes outside strings, and any attribute value with an external `url(`.
- Pass 2 caps emitted bytes at 96 MiB (a sheet or image used many times cannot balloon the file); two extra partial reasons exist: `import_depth` and `timeout` for anything never fetched.
- lol_html 3 delivers `noscript` content as raw text; rule N is implemented by re-parsing that text, keeping `img`/`picture` only, and sanitizing them.
- The HTTP client is built once (`public_web_client` in `services/web.rs`); `WebService::new` uses the same builder.

Storage, job, scheme (§7, §8, §10)

- The service is a `Container` field with its own accessor; the chat layer reaches it through `ChatStorage::ensure_page_snapshot`, a defaulted no-op on the port so test runtimes stay inert.
- Retries submit a new job with operation id `<base>:retry:<uuid>`.
- The job kind is registered in the web plugin's `setup`; the backup packs `web-snapshots/` beside `web-archive/` without new constructors.
- `normalize_web_source_url` moved to `shared::http` so the web feature does not depend on the conversation feature (the layer checker forbids that edge); both tables key the same way.

Viewer (§9)

- Icon buttons carry aria-labels the spec did not list; the frozen chip formats its date with `toLocaleDateString`; the toggle appears only when a conversation is active.
- "captured over HTTP" shows for HTTP captures; the browser wording lands with wave 2.

Verification at integration: full Rust library suite 4375 passed; reading-feature vitest 194 passed; type-check, lint, bindings export, repository barrier and layer checker clean after the normalizer move and a `raw-spawn` annotation in `main.rs`. Not verified here and left to a live run: the `frame-src` line in WKWebView, the 204 link behaviour, and that the hidden reader window cannot load the scheme.
