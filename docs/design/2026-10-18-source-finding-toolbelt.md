# Source-finding toolbelt

Research and architecture, decision-complete. No code changed. Audited against `lattice-temp` main (`6424308e`, clean tree) and the logs of 2026-10-16/17/18 on 2026-10-18. Companion to `docs/design/2026-10-18-source-provenance-and-trust.md` (cited as PT §n) and `docs/design/2026-10-16-lesson-claim-review.md` (CR §n). The provenance design decides *whether a source counts*; this one decides *how research reaches the sources that count*.

Paths are written relative to the crate so they survive the move `src-tauri/` → `crates/lattice-desktop/`: `<desktop>/src/…` is today's `src-tauri/src/…`; `<core>/src/…` is today's `src-tauri/crates/lattice-core/src/…`.

## 0. The rule, in one paragraph

Finding a source is a routed, fused, budgeted search over many *finders*, not one scrape of one engine. Every finder declares what it finds, what it costs, what it needs (nothing, a free key, a paid key, a user-run instance), where the query goes and how fast it may be called; a router picks finders for the need at hand (a chat question, a lesson gap with a claim kind and a subject), runs them inside the turn's or the lesson's budget, fuses their leads by rank, and says plainly which finder answered and which failed and why. Owner-first routing (resolve the subject's owner, then search the owner's site, registry record, or standards body) is how research reaches PRIMARY sources; general web search is the fallback, not the default, for claims that have an owner. Keyless, anonymous finders are on by default; keyed ones are opt-in with keys in the keyring; nothing is hosted. A page that will not fetch is read in the hidden browser, then from the Wayback Machine as a dated copy that provenance folds into its original.

## 1. Audit: the toolbelt today

### 1.1 Search path (`<desktop>/src/features/web/services/web.rs`, `web/providers.rs`, `web/results.rs`, `web/pacing.rs`)

| Fact | Where |
|---|---|
| Three providers only: DuckDuckGo HTML, Bing HTML, Wikipedia Action API (`list=search`). Unknown names are dropped; empty → `["duckduckgo","bing"]` | `providers.rs:17-47` |
| DDG has three URL fallbacks (`html.duckduckgo.com/html`, `duckduckgo.com/html`, `lite.duckduckgo.com/lite`); Bing one (`count=10&first=`) | `providers.rs:49-77` |
| Per provider page: up to `MAX_RETRIES_PER_PROVIDER = 3` tries, `250ms × 2^(n-1)` backoff, retried on 403/429/5xx, on transport failure, and on an empty parse | `providers.rs:172-179, 196-215, 294-297` |
| Challenge detection = DDG "anomaly-modal"/"bots use duckduckgo" or Bing "403 - forbidden"/"automated queries"; the only escape is FlareSolverr from `RECALL_FLARESOLVERR_URL`, which Josh rejected on 2026-09-23 (memory) and which is never configured (`Stealth: no proxy configured` ×13 at startup) | `results.rs:103-107`, `providers.rs:224-262`, `stealth.rs:260-288, 313` |
| `requested_total = offset + page·max_results`; `required_pages = ceil(total/10).clamp(1, MAX_PROVIDER_PAGES=5)`; every provider's page 0 runs concurrently, deeper pages serially per provider until `merged ≥ requested_total` | `providers.rs:94-131`, `web.rs:85-87` |
| Merge: first provider/page wins URL dedup (`canonicalize_url_for_dedup` strips tracking params, unwraps DDG/Bing redirects), then `reorder_results_for_domain_diversity`, then the page is **sliced** out of the merged list | `results.rs:151-245`, `web.rs:369-379` |
| **Partial failures vanish**: `last_error` is overwritten per page and surfaced only when *every* provider returned nothing; `providers_used` lists successes only; the chat tool result is the bare `WebSearchOutput` | `providers.rs:133-151`, `web.rs:366-371`, `executor/web_tools.rs:37-41` |
| Search cache keyed on the whole input (query, page, offset, providers, depth…): 15 min, 64 entries; **provider pages are not cached**, so page 2 re-fetches page 1 of every provider | `pacing.rs:110-160`, `web.rs:125-127` |
| Per-site pacer: ≥ 900 ms between requests to one host, slot wait ≤ 15 s, 64 tracked sites | `web.rs:89-95`, `pacing.rs:23-108` |
| Follow-up queries come only from the caller (`followup_queries`); result mining was removed for producing noise | `web.rs:291-310` |
| Clamps: `max_results` 1..50, `depth` 1..4, `branch_queries` 1..4 | `web.rs:266, 282-283` |
| No `site:` support beyond passing it through to the engine; learning enforces its own scope after the fact (`within_search_scope`) | `web_research.rs:61-97` |
| No robots.txt, sitemap, `llms.txt`, Wayback or registry code anywhere under `features/web` (`grep -rn robots\|sitemap\|llms.txt\|web.archive.org` → 0) | — |

### 1.2 Fetch path (`web/page.rs`, `page_cache.rs`, `browser_reader.rs`, `address_guard.rs`, `stealth.rs`)

| Fact | Where |
|---|---|
| Order: page cache (24 h TTL; refusals 1 h) → SSRF guard (private/loopback/link-local refused, resolver + redirect policy enforce it on every hop, ≤ 10 redirects) → per-site pacing slot → one GET with a random browser profile's headers | `page.rs:161-222`, `page_cache.rs:37-50`, `address_guard.rs:17-175` |
| 429: one retry after `Retry-After`, parsed as integer seconds, `unwrap_or(2).min(5)` (a cap) | `page.rs:224-241` |
| 403/503/429 → hidden-webview browser reader (25 s, 2 concurrent, gives up on a click-to-verify check after 10 s); success is cached and returned, otherwise the refusal is cached and `HTTP <status>` returned | `page.rs:243-266`, `browser_reader.rs:52-56, 456-459` |
| Extracted text < 400 chars (`THIN_PAGE_CHARS`) → browser reader too; its text is kept only if longer | `page.rs:297-303`, `web.rs:78` |
| Reference reads (`PageRead::Reference`) bypass the chat page cache and use `reference_text::extract_with_url` | `page.rs:167-171, 287-295` |
| APIs are called with **browser-impersonating** headers (`stealth::random_profile`), which the Wikimedia User-Agent policy explicitly forbids for bots (§2.2) | `page.rs:214-215`, `providers.rs:404` |
| No archive fallback: a 404, a paywall, or a challenge that the browser cannot pass ends the read | `page.rs:243-271` |

### 1.3 Planners and what the LLM sees

| Fact | Where |
|---|---|
| Chat tools: `web_search` (query, max_results, page, offset, providers enum `duckduckgo|bing|wikipedia`, include_wikipedia, depth, branch_queries, followup_queries) and `fetch_url_content` (url, offset); plus `wiki_search`, `wiki_summary`, `find_image`. All optional tools ride the `web_search` toggle; the tool list varies per turn (KV-cache memory note) | `function_calling/registry.rs:327-412, 439-470`, `conversation/chat/tools.rs:4-16, 99-109` |
| Chat retrieval runs one `web_search` per turn with `max_results = web_search_max_results` (default **5**), 10 under deep research, providers `["duckduckgo","bing"]` (+wikipedia under deep research) | `retrieval/pipeline/web_pages.rs:321-351`, `<core>/…/contracts/settings/retrieval.rs:152, 313` |
| Lesson research: `plan_queries` (one JSON call; "prefer primary documentation, official manuals or authoritative scientific sources" as prose; returns an empty list when "public research cannot resolve the scoped gap") → `search` (pages of 12 while a page brings new in-scope URLs, providers fixed to DDG+Bing, no Wikipedia) → `select_references` (the model sees `{id, title, url, snippet}` only) → `capture` (full page, validated) | `references/web_research.rs:12-57, 101-140, 147-184`, `content_verification/research.rs:29-93, 230-300` |
| An **empty plan for claim gaps is an error**: `"The reference search plan was malformed. The lesson remains unpublished."` | `research.rs:239-247` |
| Outline references: a planner writes per-subtopic queries ("primary documentation, official specifications, standards, textbooks or institutional guidance" as prose), 30-36 queries per course | `planning/outline_references.rs:26, 195` |
| Neither planner can ask for anything but a web query: no owner, no kind, no `site:` (0 of 203 queries carried one), no registry, no paper lookup | logs, §1.4 |

### 1.4 What three days of logs say (2026-10-16 + 10-17; 10-18 had no web activity)

Full tables in Appendix A. The headline numbers:

**Providers rarely fail; they run thin.** 934 provider page requests (519 DDG html, 415 Bing); 4 second tries, 1 third try; **0** non-success statuses, **0** challenge pages, 1 transport failure. The DDG main/lite fallbacks were never reached. Yet of 87 page-2 searches, **35 returned 0 results and 13 returned 1** (55% ≤ 1; 82% < 10); of 8 page-3 searches, 4 returned ≤ 1. Page 1 always returned 10-12. The merged pool behind a search holds 10-54 unique URLs (p50 20, p90 23) over ~13 domains, so "page 2" is a slice of a pool that two engines' first pages mostly exhaust; the second provider pages add ~3 URLs on average (mean total 18.5 at page 1 → 15.4 reported at page 2 because the pool is re-fetched and re-merged, not extended).

**Each search costs ~2 s, and page 2 costs it again.** 202 searches, p50 1 984 ms, p90 4 016 ms, max 6.8 s; 0 cache hits (the lesson loop never repeats an identical input). Lesson `search()` asks for page 2 whenever page 1 returned 12 (always), so 87 of 202 searches were page-2 calls that re-fetched every provider's page 1.

**Fetches mostly work; failures cluster on a few hosts.** 241 fetches, 236 returned text (words p10 236, p50 1 503, p90 6 212). 19 non-success statuses: 403 ×11 (stackoverflow.com 9, sciencedirect 1, genai.owasp.org 1), 429 ×7 (deepwiki.com 3, four blogs/docs sites 1 each), 404 ×1; 2 transport errors. The browser reader rescued **25** pages (Stack Overflow 9, deepwiki 5, c-sharpcorner 2, docs.ragas.io 2, community.openai.com 2, five others 1 each) and failed 2 (sciencedirect.com and openreview.net: "behind a check that asks a person to click"). So the rescue works for JS/403 walls and not for Turnstile-class challenges; nothing rescues a 404 or a paywall.

**The primary sources are reachable, and mirrors compete with them.** Fetched hosts: learn.microsoft.com 40, github.com 20, postgresql.org 10, stackoverflow.com 9, arxiv.org 9, devblogs.microsoft.com 6, source.dot.net 5. But the outline planner also captured `learn-microsoft.com` (a typo-squat) and `lab.algebra.hr/en-us/dotnet/csharp/whats-new/` (a mirror of Microsoft Learn); both failed to fetch, both were *selected* by the model from `{title, url, snippet}` because nothing told it which domain owns .NET.

**The planners write keyword strings, never scoped ones.** 203 queries, 106 distinct, mean 7.7 words, 34 with ≥ 10 words (e.g. `.NET runtime ExecutionContext FlowSynchronousContext flow continue async void exception handling`, paged to page 5); 0 contained `site:`. The outline planner issued 36 queries for 9 subtopics and 30 for 10.

**The empty plan is a job failure.** On 10-17, **5 lesson jobs failed** with `The reference search plan was malformed. The lesson remains unpublished.`, which is the empty-`queries` branch at `research.rs:243-247` (the other "malformed" message lacks the second sentence). Two more failed because the selected sources exceeded the model's window (a different defect, noted for completeness). 7 outline reference pages and 2 lesson research pages were "unavailable" (4 "did not provide enough static reference text", 1 "too deeply nested", 1 HTTP 403, 1 HTTP 404, 2 "error sending request").

### 1.5 Diagnosis, ranked

1. **One kind of finder.** Everything is a web-engine query; a claim about `AsyncLocal<T>` cannot be routed to learn.microsoft.com, a paper claim to its DOI, an RFC claim to rfc-editor.org. Primary sources are reached by luck of ranking (often, see learn.microsoft.com 40) and mirrors compete on equal terms.
2. **Depth is paid for and returns nothing.** 48 of 95 deeper-page searches returned ≤ 1 result after ~2 s each; the pool is small and re-fetched each page.
3. **Failures are silent until total.** A dead provider never reaches the model, the step UI, or a log line with a count; `providers_used` is success-only.
4. **No archive fallback**, so 404/paywall/Turnstile pages are lost (2 of 27 rescues failed, 1 404 unrescued; dated copies are impossible).
5. **No keyed or user-supplied breadth.** When DDG+Bing are thin, there is nothing else; Bing's HTML endpoint is the only second engine and Microsoft has retired its API (§2.1), so its HTML is the next thing to break.
6. **Retries and backoff are fixed numbers** (3, 250 ms, Retry-After capped at 5 s, 2 s default) rather than derived from the turn deadline or the host's own `Retry-After`.
7. **Wrong identity for APIs.** Browser-impersonating headers on Wikipedia's API violate its User-Agent policy and invite a block; the same would be true of crates.io, Crossref, arXiv.
8. **The empty plan kills the lesson** instead of recording exhaustion (the `lattice-evidence` fix PR introduces `Expansion::Exhausted`; the toolbelt must feed it, PT §1 last row).

## 2. Research: ways an LLM agent can find sources (as of 2026-10-18)

Verification marks: **V** = I read the vendor's page on 2026-10-18; **A** = a research agent read an official page on 2026-10-18; **R** = recalled or secondary, re-check before shipping. Full per-provider table in Appendix B.

### 2.1 General search backends

| Provider | Status | Auth / price | Limits | Fit for Lattice |
|---|---|---|---|---|
| DuckDuckGo HTML/lite | Live; unofficial; soft blocks (HTTP 202 / "anomaly" page) for datacentre IPs and fast pacing (R); 0 blocks in our 934 home-IP requests | none | none published | Keep as the default keyless engine, paced; detect the anomaly page (already done) and **fail over** instead of retrying |
| Bing HTML | Live, unofficial; the **Bing Search APIs were retired 2025-08-11**, replacement is "Grounding with Bing Search" inside Azure AI Agents (summaries, not results) — V | none | — | Keep while it works; treat as the first to break; never the only second engine |
| Brave Search API | Live — V: Search $5/1 000 requests, "$5 in free credits every month" (≈1 000 queries), 50 qps; card "to prevent abuse"; storing results needs "a plan that includes storage rights"; own index; `site:` supported (R) | paid key (free credit) | 50 qps | Best keyed general engine; **opt-in with the user's key** |
| Tavily | Live — V: 1 000 free credits/month, no card, basic search 1 credit, advanced 2, $0.008/credit after; acquired by Nebius 2026 (R) | free key | not stated | Good opt-in for LLM-shaped results; results are extracted snippets, never evidence |
| Exa | Live; $7/1 000 search-with-contents (A, pricing changelog); neural + keyword, categories `paper`, `github`, `company`, find-similar | paid key | — | Opt-in; useful for papers/repos when the registry finders miss |
| Google Programmable Search JSON | **Closed to new customers; existing customers must leave by 2027-01-01** (V) | — | — | Do not build on it |
| Mojeek API | Live, own index; £1-3/1 000 (A) | paid key | 1-10 qps | Opt-in only; small index |
| Marginalia | Live, free non-commercial key by email, CC-BY-NC-SA (A, docs 2025-12-08) | free key | shared key often limited | Optional "independent web" finder; licence excludes commercial use |
| Kagi Search API | Live; account-bound bot token (A), ≈$2.50/1 000 (R) | paid | — | Opt-in with the user's own token |
| SearXNG | Live; JSON format must be enabled per instance (403 otherwise), public instances rate-limit (429) (A) | none; user-run URL | instance-set | **User-supplied instance URL**, off by default; never a public instance |
| Serper / SerpApi | Google scraping; SerpApi sued by Google 2025-12-19 (R) | paid | — | Excluded: legal exposure for a distributed app |
| You.com, Perplexity Search, Jina `s.jina.ai` | Live, keyed, $1-5/1 000 (R) | paid/free credit | — | Not first-class; the adapter shape admits them later |
| Startpage, Qwant | No public API found (R) | — | — | Excluded |

Takeaway: there is exactly one broad keyless engine left (DDG), one fragile unofficial second (Bing HTML), and a set of keyed engines the user can bring. Resilience has to come from *finders that are not web engines*.

### 2.2 Primary-source finders by domain

**Entities and official sites.**
- Wikidata: `wbsearchentities` → QID; `Special:EntityData/Q….json` or SPARQL → **P856 official website** (V: datatype URL, format `^(https?|gemini)://\S+$`, qualifiers for start/end time and archive URL; related P1324 source code repository, P2078 user manual URL, P1401 issue tracker, P1581 official blog, P12203 official wiki). CC0, keyless. This is the cleanest "subject → owner domain" lookup that exists; it feeds PT §12.2 subject ownership directly.
- Wikipedia Action API: `list=search`, `prop=extracts`, `action=parse&prop=externallinks|sections` (the References section as citation leads); REST `page/summary/{title}` returns `wikibase_item` (R). Requires a **descriptive User-Agent with contact** (V: "`<client>/<version> (<contact>) <library>/<version>`"; "Do not copy a browser's user agent"; generic agents "may be blocked without notice").

**Software.** Registries that return owner URLs (A, PyPI V): crates.io `homepage/repository/documentation` (policy RFC 3463: ≤ 1 request/s, unique identifying User-Agent, prefer index/dump for bulk — V via RFC text); PyPI `/pypi/{name}/json` → `info.project_urls{Homepage, Documentation, Source…}` (V); npm `registry.npmjs.org/{pkg}` → `homepage`, `repository.url`; RubyGems `/api/v1/gems/{name}.json` → `homepage_uri`, `documentation_uri`, `source_code_uri`; NuGet registration → `projectUrl`, repository; Maven POM `<url>`/`<scm>`; Go: the module path *is* the repo. GitHub REST: unauthenticated **60 requests/hour per IP**, 5 000 with a token (V); `repos/{o}/{r}` → `homepage`, README, releases, tags; code search needs auth. docs.rs is a URL pattern, not an API. Site discovery on the owner's domain: `robots.txt` (`Sitemap:` lines, RFC 9309), `sitemap.xml`, and the `llms.txt` convention (A, llmstxt.org: H1, blockquote summary, H2 link sections, `Optional`; `llms-full.txt` is a community extension). Algolia DocSearch keys belong to each site's widget with no documented third-party permission: **not used**. Read the Docs server-side search is public per project (R). MDN has no stable search API; its content repo is CC BY-SA (R).

**Academic.** OpenAlex (A, help.openalex.org rate-limits page): keyless **10 req/s**, free key 30 req/s, `referenced_works`, `best_oa_location`, `open_access.oa_url`, DOI resolution. Crossref REST (A): polite pool via `mailto=` or UA, `reference` array where the publisher opened it, `resource.primary.URL` (publisher landing page), `link` full-text URLs. Semantic Scholar Graph (A): `/paper/search`, `/paper/{id}/references|citations`, `openAccessPdf`; unauthenticated pool is shared and 429-prone, free key 1 rps. arXiv API (A, ToU): **≤ 1 request per 3 s**, metadata CC0, PDFs may be linked but not stored or served. Unpaywall `/v2/{DOI}?email=` → `best_oa_location.url_for_pdf` (R). Europe PMC REST: references, citations, OA full text XML (R). PubMed E-utilities 3 rps / 10 with key (R). CORE (key), DOAJ, DBLP (`ee` URL, no references), OpenCitations (DOI→DOI edges). **Papers with Code is dead** (Meta sunset it July 2025; A).

**Standards.** IETF: `rfc-editor.org/rfc-index.xml`, per-RFC `.json`, Datatracker `api/v1/doc/document/` (obsoletes/updates relations), keyless (R). W3C `api.w3.org/specifications` HAL JSON (R). WHATWG index at `spec.whatwg.org` (no API; the `whatwg/spec.whatwg.org` repo lists specs). ECMA PDFs by URL pattern. ISO OBP: metadata and previews only, full text paid, never cached. NIST NVD 5 requests/30 s keyless (R).

**Government and law.** Keyless: Federal Register API (`html_url`, `full_text_xml_url`), legislation.gov.uk (`/data.xml`, `/data.akn`, Atom feeds; OGL v3), EUR-Lex by CELEX with content negotiation at `publications.europa.eu/resource/celex/…`, data.gov CKAN, SEC EDGAR (descriptive `User-Agent: Name email`, ≤ 10 rps). Free api.data.gov key: govinfo, regulations.gov, Congress.gov (1 000-5 000/h). CourtListener v4: token; the documented authenticated limits (5/min, 50/h, 125/day) are tight (A). All R unless marked.

**Community.** Stack Exchange API 2.3: 300 requests/day per IP without a key, **10 000/day with a free app key**; a `backoff` field (seconds) must be obeyed; CC BY-SA attribution (the citation must link the post and author) — numbers from third-party docs since `api.stackexchange.com` was unreachable from here (R). This replaces scraping stackoverflow.com, which 403s every direct fetch today (9 of 9). HN Algolia keyless. Discourse forums expose `.json` with `Retry-After` on 429 (R). Reddit requires OAuth + approval since 2025-11 (R): excluded.

**Archives.** Wayback Availability API (V): `archive.org/wayback/available?url=…&timestamp=YYYYMMDDhhmmss` → `archived_snapshots.closest{url, timestamp, status, available}`, one snapshot, no auth, no stated limits. CDX server (V): `web.archive.org/cdx/search/cdx?url=…&output=json&fl=timestamp,original,statuscode,digest&filter=statuscode:200&collapse=digest&from=&to=&limit=N`, `matchType=exact|prefix|host|domain`; no limits in the README, but the service has applied ~60 requests/min with 429s and hour-long firewall blocks for clients that ignore them since the 2024 outage (R, secondary 2026-02-06). Raw bytes without the toolbar: `web.archive.org/web/<timestamp>id_/<original>` (R, standard). Save Page Now needs S3-style keys and publishes the URL: **not used**. archive.today: no API, Cloudflare, excluded. Common Crawl index `index.commoncrawl.org/CC-MAIN-2026-34-index?url=…&output=json` + ranged WARC fetch: keyless historical fallback (A). Memento TimeTravel aggregator: intermittently unreliable (R).

**News and fact-checking.** GDELT DOC 2.0 (keyless, ~3 months of coverage), Google Fact Check Tools (`claims:search`, Google key), NewsAPI (free tier is development-only, 24 h delay), Media Cloud (key). Relevant only for "current events" claims; not in the first waves.

### 2.3 Retrieval techniques (what the evidence supports)

| Technique | Evidence | Use here |
|---|---|---|
| Reciprocal rank fusion, `score = Σ 1/(k + rank)`, k = 60 | Cormack, Clarke & Büttcher, SIGIR 2009: beats Condorcet and CombMNZ without score calibration (R) | Fuse finders with incomparable scores (engine rank, registry exactness, OpenAlex relevance) |
| Canonical URL dedupe | RFC 3986 §6 normalisation; `rel=canonical` (RFC 6596); tracking-param stripping | Already partly in `results.rs:151`; move to core and add `rel=canonical` + archive unwrapping (PT §3 relations) |
| Query reformulation on thin results | HyDE (Gao et al. 2022), step-back (Zheng et al. 2023), Self-Ask decomposition (Press et al. 2022), query2doc (Wang et al. 2023): gains mostly for dense retrieval over weak queries (R) | Keyword engines want *fewer* terms and exact phrases: reformulate by **dropping** terms and quoting the subject, not by expanding; the model writes the alternative, as `web.rs:291-310` already decided |
| Owner-first / site-restricted routing | Practice; consistent with SAFE (Wei et al., DeepMind 2024) reading results per fact (R) | The main lever for Primary sources (§3.4) |
| Citation-chain following (snowballing) | Wohlin 2014; Semantic Scholar/OpenAlex reference lists; RFC obsoletes/updates; PT §8 | `follow_links` tool + academic reference finders |
| Claim-type-aware routing | FEVER (Thorne et al. 2018), ClaimBuster, SAFE 2024 (R) | PT §12.3 claim profile → finder kinds (§3.4 table) |
| Agentic loops: search → read → extract leads → follow | ReAct 2022, WebGPT 2021, Self-RAG 2023, Search-R1 2025, OpenAI deep research 2025 (R) | The lesson loop already is one; give it leads (owner, references) instead of only queries |
| Snippets are not evidence | Lattice's own grounding-check history (memory 2026-09-23); SAFE reads pages | Unchanged: leads are fetched before they count |

### 2.4 Ranked additions (impact on finding PRIMARY sources)

1. **Owner-first routing**: Wikidata P856/P1324 (+ Wikipedia external links as fallback) resolves a subject to its owner's origin; registries do it for software; then search *within* that origin.
2. **Site discovery on the owner's origin**: robots → sitemap → `llms.txt`, plus engine `site:` queries; a page inventory makes "the official page for X" a lookup rather than a ranking contest.
3. **Package registries + GitHub**: exact owner, docs, repo, releases/changelogs for software claims (most of Josh's courses so far).
4. **Academic finders**: OpenAlex → Crossref → Semantic Scholar → arXiv → Unpaywall for measurements, papers and their references.
5. **Wayback rescue and dated copies** in the fetch chain (rescues 404/paywall/Turnstile; gives "as of" reads).
6. **Standards finders**: RFC index/Datatracker, W3C, WHATWG, ECMA.
7. **Citation-chain tool** (`follow_links`) over fetched pages, with offsets, feeding PT §8.
8. **Resilience core**: fusion, health, breakers, honest failures, provider-page cache (this is what makes 1-7 safe to add).
9. **Stack Exchange API** instead of scraping Stack Overflow.
10. **Opt-in keyed breadth**: Brave, Tavily, Exa, a user's SearXNG; Marginalia for the independent web.

## 3. Architecture

### 3.1 Principles

1. **A finder is a port adapter with a descriptor.** What it finds, what it costs, where the query goes, how fast it may be called, and whether it is on, are data the router reads, not knowledge buried in a match arm.
2. **Needs, not queries.** Callers state a `SourceNeed` (kind, subject, scope, freshness, budget). Only the router turns needs into provider requests.
3. **Owner first, web second.** For a need with a subject that has an owner, the owner's finders run in the first wave; the web engines widen it.
4. **Budgets, not caps.** Every bound is derived from the turn deadline, the lesson's research allowance (CR §4), the host's own `Retry-After`, or a *named* parameter in `SourcingPolicy` with its reason beside it.
5. **No silent degradation.** Every outcome lists each finder that ran, with leads or a cause. Callers, the step UI, the lesson log and the health panel all see it.
6. **Anonymous by default.** Keyless finders that receive only the query are on; finders that need a key or identify the user (email polite pools) are opt-in; nothing is hosted; the user's SearXNG is a URL, not a dependency.
7. **Leads are not evidence.** A lead becomes a source only through the existing fetch → extract → provenance path (PT §4).

### 3.2 Domain model (`<core>/src/domain/sourcing/`, pure)

```rust
pub enum NeedKind { Web, OfficialDocs, Package, Paper, Standard, Law, Community, Encyclopedia, Archive, Leads }

pub struct SourceNeed {
    pub kind: NeedKind,
    pub query: String,                       // public subject terms only (planner rule stays)
    pub subject: Option<SubjectHint>,        // { name, kind: Software|Organisation|Paper|Law|Standard|Person|Other, identifiers: Vec<Identifier> }
    pub scope: Option<SiteScope>,            // host or host+path prefix; enforced after the fact as today
    pub freshness: Freshness,                // Any | After(date) | AsOf(date)
    pub want: usize,                         // leads the caller can use (derived from its window/budget)
}

pub enum Identifier { Doi(String), Arxiv(String), PackageName { ecosystem: Ecosystem, name: String }, Repo { host: String, owner: String, name: String }, Rfc(u32), Celex(String), WikidataQid(String), Url(Url) }

pub struct Lead {
    pub url: Url,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub kind: LeadKind,                      // Page | OfficialSite | Document(Identifier) | Repository | ReferenceOf(Url) | ArchivedCopy { original: Url, captured_at }
    pub owner: Option<OriginHint>,           // registrable domain the finder asserts owns the subject (registry/Wikidata), for PT §12.2
    pub found_by: Vec<(FinderId, usize)>,    // finder and rank in that finder's list; fusion fills this
}

pub struct FinderDescriptor {
    pub id: FinderId,                        // "duckduckgo", "wikidata", "crates_io", …
    pub finds: &'static [NeedKind],
    pub reaches: Reach,                      // OpenWeb | OwnerSites | Registry | Scholarly | Standards | Law | Community | Archive
    pub auth: Auth,                          // None | FreeKey { service } | PaidKey { service } | UserInstance { setting }
    pub privacy: Privacy,                    // AnonymousQuery | QueryLogged { by } | IdentifiedByKey | IdentifiedByContact
    pub cost: Cost,                          // Free | Credits { per_request } | Metered
    pub rate: RatePolicy,                    // { min_interval: Duration, per_day: Option<u32>, honours_retry_after: bool, source: &'static str /* policy URL */ }
    pub supports_scope: bool,                // native site: / domain filters
    pub identity: Identity,                  // BrowserLike (HTML scraping) | DeclaredClient (API: "Lattice/<ver> (+https://…; contact?)")
}

pub struct FinderOutcome { pub finder: FinderId, pub leads: Vec<Lead>, pub result: FinderResult, pub elapsed: Duration }
pub enum FinderResult { Answered, Empty, Failed(FailureCause), Skipped(SkipReason) }
pub enum FailureCause { Status { code: u16, retry_after: Option<Duration> }, Challenge, Transport(String), Parse(String), RateLimited { until: DateTime<Utc> }, Budget }
pub enum SkipReason { Disabled, NoKey { service }, CircuitOpen { until, last: Box<FailureCause> }, NotApplicable, OutOfBudget }

pub struct Findings { pub leads: Vec<Lead> /* fused */, pub outcomes: Vec<FinderOutcome>, pub exhausted: bool /* every applicable finder answered or failed */ }
```

Pure functions beside the types: `fusion::rrf(outcomes, policy.rrf_k) -> Vec<Lead>`; `canonical::key(url) -> CanonicalKey` (RFC 3986 normalisation, tracking params, archive unwrapping shared with PT's `services/provenance/archive.rs` reader through a core function both call); `health::Breaker` (state machine: `Closed { failures } | Open { until, cause } | HalfOpen`); `routing::plan(need, registry, health, policy) -> Plan { waves: Vec<Vec<FinderId>>, reformulate_after: WaveIndex }`.

`SourcingPolicy` (named, documented parameters; the only numbers in the design):

| Parameter | Default | Why this value |
|---|---|---|
| `rrf_k` | 60 | Cormack et al. 2009's constant; insensitive in their sweep |
| `wave_concurrency` | number of distinct hosts in the wave, at most `finder_concurrency` (default 4) | hosts are paced serially by `SitePacer` anyway; the ceiling protects the user's machine and the shared LLM budget, same reasoning as the memory cap on agents |
| `thin_wave` | a wave is thin when fused new leads < `need.want / 2` | half of what the caller asked for is the point where paying for another wave beats returning |
| `breaker_trip` | 2 consecutive `Status(403|429)`/`Challenge` or 3 consecutive transport failures | one 429 may be the pacer's fault; two in a row with pacing means the host means it |
| `breaker_open_for` | `retry_after` if given, else the finder's `rate.min_interval × 2^trips`, never past the caller's deadline | derive from the host's instruction first, the finder's own policy second |
| `search_page_ttl` | 15 min (today's `SEARCH_CACHE_TTL`) | engines re-rank within minutes; a lesson's passes repeat inside this |
| `owner_ttl` | 30 days | owners change on acquisition timescales; refreshed on first use after expiry |
| `archive_lookup_ttl` | 7 days | Wayback captures accrue slowly; a miss today is a miss this week |

### 3.3 Ports (`<core>/src/application/ports/`)

```rust
#[async_trait]
pub trait SourceFinder: Send + Sync {
    fn descriptor(&self) -> &FinderDescriptor;
    /// One finder, one need, one page; never retries across providers (the router does that).
    async fn find(&self, need: &SourceNeed, page: usize, budget: &Budget) -> FinderOutcome;
}

#[async_trait]
pub trait SourceFinding: Send + Sync {                  // what callers hold
    async fn find(&self, need: SourceNeed, budget: Budget) -> Result<Findings>;
    async fn resolve_owner(&self, subject: &SubjectHint, budget: Budget) -> Result<OwnerResolution>;   // Wikidata/registry/GitHub → origins, cached
    async fn leads_from_page(&self, url: &Url, near: Option<Span>, budget: Budget) -> Result<Vec<Lead>>; // outbound links with offsets (PT §8 candidates)
    fn health(&self) -> Vec<FinderHealth>;
}

pub struct Budget { pub deadline: Instant, pub stop: CancellationToken, pub requests: Option<u32> /* lesson research allowance, CR §4 */ }
```

`WebServiceTrait::search_web` stays as the chat `web_search` implementation and becomes a thin call to `SourceFinding::find(SourceNeed { kind: Web, … })`; `WebSearchOutput` gains `outcomes: Vec<FinderOutcomeDto>` so the model and the UI see failures. `fetch_url_content`/`fetch_reference_content` keep their signatures; `FetchUrlContentOutput` gains `read_via: ReadVia { Direct | Browser | Archive { captured_at, original } }` (PT P2 adds `signals`; the two are independent fields).

### 3.4 Router / planner (`<core>/src/application/services/sourcing/routing.rs`, pure)

Inputs: the need, the registry of descriptors, health, settings (enabled finders, keys present), policy. Output: waves.

| Need kind (from the claim profile, PT §12.3, or the chat tool's `kind`) | Wave 1 | Wave 2 (if thin) | Wave 3 |
|---|---|---|---|
| `OfficialDocs`, subject known | `resolve_owner` → site discovery on the owner (sitemap/llms.txt grep by terms) + engines with `site:<owner>` | engines without scope, reformulated | keyed engines |
| `Package { ecosystem }` | that registry (exact) + GitHub repo metadata | docs site discovery on the registry's `documentation`/`homepage` | engines `site:` |
| `Paper` / `Measurement` | OpenAlex (search or DOI) + Crossref (DOI) | Semantic Scholar + arXiv | Unpaywall for an OA copy of the chosen DOI; engines |
| `Standard` | RFC index/Datatracker, W3C, WHATWG index, ECMA pattern | engines `site:` on the body | engines |
| `Law` | Federal Register / legislation.gov.uk / EUR-Lex by jurisdiction hint | govinfo/regulations.gov if keyed | engines |
| `Community` | Stack Exchange API, HN Algolia | Discourse `.json` for a known forum | engines |
| `Encyclopedia` | Wikipedia search + extracts (+ reference leads) | Wikidata identifiers | — |
| `Archive { url, as_of }` | Wayback availability → CDX | Common Crawl index | — |
| `Web` (chat default) | DDG + Bing + (Wikipedia when asked) | enabled keyed engines + user SearXNG | reformulate |
| `Leads { url }` | the page's outbound links (no network beyond the page) | the academic finders for DOIs found in the links | — |

Wave rules: run a wave's finders concurrently under `wave_concurrency`; skip a finder whose breaker is open or whose key is missing (recorded as `Skipped`, never silently); fuse; stop when the fused leads reach `need.want`, when the wave was not thin, or when the budget is spent. Reformulation is the caller's model's job (an `alternatives: Vec<String>` slot in the need, like today's `followup_queries`), never word-mining.

Chat questions versus lesson claims: chat passes `kind: Web` unless the model chose otherwise through `find_sources`; lessons pass the claim profile's kind and subject (PT §18.5) so a Specification claim about a vendor API goes owner-first automatically.

### 3.5 Fusion and dedupe (`<core>/src/domain/sourcing/{fusion,canonical}.rs`)

RRF over each finder's ranked list; ties broken by (owner match, lead kind rank `Document > OfficialSite > Page > ArchivedCopy`, earliest wave). Dedupe by canonical key before fusion so an archived copy, a `utm_`-tagged link and the `rel=canonical` target fold into one lead with the union of `found_by`. Origin diversity (today's `reorder_results_for_domain_diversity`) becomes a post-fusion pass that never demotes a lead whose owner matches the subject. The learning loop's `within_search_scope` moves into the service (scope enforced on every lead and again after redirects, as today).

### 3.6 Resilience (`<desktop>/src/features/web/services/sourcing/{health,runner}.rs`)

- **Health per finder**: a `Breaker` (§3.2) plus counters (calls, answered, empty, failed by cause, last outcome, next allowed at). The pacer stays per host; the breaker is per finder.
- **Backoff honours `Retry-After`** (seconds or HTTP-date) up to the caller's deadline, and the finder's `rate.min_interval` otherwise; today's `min(5)` and `unwrap_or(2)` go away. A `429` with no header opens the breaker for `min_interval × 2^trips`.
- **Failover is a wave, not a loop**: if DDG is open-circuit, Bing and the enabled keyed engines run in the same wave; the outcome lists DDG as `Skipped(CircuitOpen { until, last })`.
- **Honest degradation**: `Findings.outcomes` goes to (1) the chat tool result as `outcomes: [{finder, result, leads, elapsed_ms}]`, (2) the research-step UI line ("DuckDuckGo: 10 · Bing: skipped, rate-limited until 14:02 · Wikidata: owner learn.microsoft.com"), (3) a structured log line per finder (`finder= kind= result= leads= ms= cause=`), (4) the health panel. When every applicable finder is skipped or failed, `find` returns `Err(AppError::Network(<every cause, one per line>))`, as `web.rs:366-371` does today but with all causes.
- **Paging** is a slice of the cached merged pool; a further provider page is fetched only when the pool is exhausted and the provider's last page was full (a short page means the engine has no more). This removes the 48 wasted page-2 calls.
- **Challenge pages** open the breaker; they are never retried in-process (retrying a challenge is what gets an IP blocked).

### 3.7 Caching and freshness

| Cache | Key | TTL | Store |
|---|---|---|---|
| Provider page | (finder, canonical need, page) | `search_page_ttl` | memory (today's `SearchCache` generalised) |
| Owner resolution | subject name + kind | `owner_ttl` | SQLite `sourcing_owner_cache` (lessons reuse across jobs) |
| Archive lookup | (url, as_of day) | `archive_lookup_ttl` | SQLite `sourcing_archive_lookups` |
| Page | url | 24 h (existing) | existing `page_cache` |
| Archived page | (original, captured_at) | until GC | existing page cache with the archive key; snapshots keep the HTML |

`Freshness::After(date)` bypasses provider-page entries older than the date's window and asks engines for recency where they support it; `AsOf(date)` routes to the archive finders.

### 3.8 Fetch fallback chain (`<desktop>/src/features/web/services/web/page.rs` + new `services/wayback.rs`)

```
read_page_as(url, read, want):
  cache → SSRF guard → pacer → direct GET (headers by target: BrowserLike for pages)
  non-success 403/429/503, or Turnstile/challenge text, or thin text → browser reader (as today)
  still nothing, or 404/410/451, or paywall text, or want.as_of → wayback:
      availability(url, timestamp?) → closest snapshot; if none → CDX(url, statuscode:200, collapse digest, limit from budget)
      fetch web.archive.org/web/<ts>id_/<url> through the same guard and extractor
      return ReadVia::Archive { captured_at, original }; provenance folds it to the original origin (PT §3 relations, archived_from)
  refusal cache remembers a settled refusal only when the archive also had nothing
```

The archive is a *read* fallback and a dated read on request; Lattice never asks the archive to save a page (Save Page Now publishes the URL). The archive finders obey one shared per-host pacer slot with their own `RatePolicy` (CDX: documented community limit, R) and open their breaker on the first 429, because the penalty is an hour-long block.

### 3.9 Identity, robots and terms

- Two HTTP identities: `BrowserLike` (today's stealth profiles) for HTML engines and ordinary pages, where a browser is what the site expects of a user's reading client; `DeclaredClient` (`Lattice/<version> (+https://<project url>)`, plus the user's optional contact for polite pools) for every API finder, as Wikimedia, crates.io, Crossref, SEC and arXiv require. The proxy pool and FlareSolverr code in `stealth.rs:255-410` is deleted (decided 2026-09-23; dead code now).
- `robots.txt` is honoured for **site discovery walks** (sitemap reading is crawling); a single page the user or the model asked to read is not a crawl and is fetched as a browser would. The rule and its reason live in the fetcher's doc comment.
- Licence terms ride on the lead: Stack Exchange leads carry `attribution_required: CcBySa { post_url, author }` which the citation UI already satisfies by linking; Marginalia leads carry `non_commercial`; ISO leads are metadata only. A `Terms` field on `Lead` makes this visible rather than tribal.

### 3.10 Finder catalogue (`<desktop>/src/features/web/services/finders/`)

Each adapter is one file ≤ 300 lines with a sibling `_tests.rs` over recorded fixtures; the registry lists them with descriptors.

| Finder (id) | Finds | Auth | Privacy | Identity | Default |
|---|---|---|---|---|---|
| `duckduckgo` (html, lite fallback) | Web | none | query logged by DDG | BrowserLike | on |
| `bing_html` | Web | none | query logged | BrowserLike | on |
| `wikipedia` (search, extracts, external links) | Encyclopedia, Leads | none | query logged (Wikimedia) | Declared | on |
| `wikidata` (wbsearchentities, EntityData P856/P1324/P2078) | owner resolution | none | query logged | Declared | on |
| `site_discovery` (robots → sitemap → llms.txt on an origin; term match + path ranking) | OfficialDocs | none | the owner sees the fetch | BrowserLike for pages, Declared for robots/sitemap | on |
| `crates_io`, `pypi`, `npm`, `nuget`, `rubygems`, `maven_pom`, `go_module` | Package, owner | none | registry logs | Declared (crates.io ≤ 1 rps) | on |
| `github` (repo meta, README, releases, tags) | Package, OfficialDocs | none (60/h) or user token | GitHub logs; token identifies | Declared | on; token opt-in |
| `openalex`, `crossref`, `semantic_scholar`, `arxiv`, `unpaywall`, `europe_pmc` | Paper, Leads (references) | none; Crossref/Unpaywall want contact; S2 key optional | query logged; contact identifies | Declared | on, contact-needing ones on only when a contact is set |
| `rfc_index`, `ietf_datatracker`, `w3c`, `whatwg_index`, `ecma_pattern` | Standard | none | logged | Declared | on |
| `federal_register`, `legislation_uk`, `eurlex_celex` | Law | none | logged | Declared | on |
| `govinfo`, `regulations_gov` | Law | free api.data.gov key | key identifies | Declared | off |
| `stack_exchange` | Community | none (300/day) / free key (10k/day) | logged; key identifies app | Declared | on |
| `hn_algolia`, `discourse_json` | Community | none | logged | Declared | on |
| `wayback` (availability, CDX, `id_` read), `common_crawl_index` | Archive, fetch fallback | none | URL sent to IA / CC | Declared | on (open decision 4) |
| `brave`, `tavily`, `exa`, `kagi`, `mojeek`, `marginalia` | Web (+ Paper/Repo for Exa) | user key | key identifies | Declared | off until a key exists |
| `searxng` | Web | user instance URL | the instance operator | Declared | off until a URL is set |

### 3.11 Tools exposed to the chat LLM

The KV-cache work wants one stable tool list per conversation ("mask, don't remove"). So: **four web tools, fixed**, with kinds as enums rather than one tool per kind. Masking a tool the turn forbids keeps the prefix stable. `wiki_search`/`wiki_summary` fold into `find_sources(kind: "encyclopedia")` and `fetch_url_content` (open decision 5).

```jsonc
// web_search — unchanged purpose, honest output, scope added
{ "name": "web_search",
  "description": "Search the web for pages about a question. Use find_sources instead when you need an owner's official documentation, a package, a paper, a standard or a law. Results are leads: open one with fetch_url_content before relying on it.",
  "parameters": { "type": "object", "required": ["query"], "properties": {
    "query":   { "type": "string", "description": "Public subject terms, 3-10 words; quote exact phrases" },
    "site":    { "type": "string", "description": "Optional host or host/path to search within, e.g. learn.microsoft.com or docs.python.org/3" },
    "after":   { "type": "string", "format": "date", "description": "Optional: only pages published after this date" },
    "page":    { "type": "integer", "minimum": 1, "default": 1 },
    "alternatives": { "type": "array", "items": { "type": "string" }, "description": "Up to three rewordings tried only if the first query is thin" }
  } } }
// Output: { leads: [{url,title,snippet,published,kind,owner,found_by}], outcomes: [{finder,result,leads,elapsed_ms}], page, has_more }
```

```jsonc
// find_sources — typed, owner-first
{ "name": "find_sources",
  "description": "Find primary sources for a subject: the owner's official docs, a package's registry record and repo, a paper by title or DOI, a standard (RFC, W3C, ECMA), a law, or a Q&A thread. Routes to the right registries and bodies, then the web. Returns leads with the owner's domain marked.",
  "parameters": { "type": "object", "required": ["kind", "query"], "properties": {
    "kind":    { "type": "string", "enum": ["official_docs", "package", "paper", "standard", "law", "community", "encyclopedia"] },
    "query":   { "type": "string", "description": "What to find, e.g. 'AsyncLocal<T> flow', 'tokio', 'Attention is all you need', 'RFC 9110 Retry-After'" },
    "subject": { "type": "string", "description": "The entity that owns the answer when known: a product, organisation, package or standards body" },
    "identifier": { "type": "string", "description": "Optional exact id: DOI, arXiv id, 'npm:react', 'crates:tokio', 'pypi:requests', 'rfc:9110', 'celex:32016R0679'" }
  } } }
// Output as web_search, plus owner: { domain, resolved_by: "wikidata"|"registry"|"github"|null }
```

```jsonc
// fetch_url_content — archive-aware
{ "name": "fetch_url_content",
  "description": "Read a page's text. Reads through a hidden browser when a site refuses a plain client, and from the Internet Archive when the page is gone or when as_of is given; the result says which.",
  "parameters": { "type": "object", "required": ["url"], "properties": {
    "url":    { "type": "string" },
    "offset": { "type": "integer", "minimum": 0 },
    "as_of":  { "type": "string", "format": "date", "description": "Read the archived copy nearest this date instead of the live page" }
  } } }
// Output gains read_via: "direct" | "browser" | { "archive": { "captured_at": "...", "original": "..." } }
```

```jsonc
// follow_links — citation chains, no new search
{ "name": "follow_links",
  "description": "List the links a page you have read makes, with the sentence each sits in, so you can open the page it cites instead of trusting the page that cites it. Give near_text to get only the links around a passage.",
  "parameters": { "type": "object", "required": ["url"], "properties": {
    "url":       { "type": "string" },
    "near_text": { "type": "string", "description": "A short quote from the page; links in and around its paragraph are returned first" }
  } } }
// Output: { links: [{url, anchor, context, offset, kind, owner_match}], outcomes: [...] }
```

Compactness: the four descriptions above total ≈ 330 tokens; the current `web_search` schema alone (nine properties with prose) is ≈ 290. Enum kinds keep the list fixed as finders are added. `find_image` is unchanged.

### 3.12 Integration with the Learning research loop

- `plan_queries` (`references/web_research.rs:21-57`) becomes `plan_needs`: the model returns `needs: [{kind, query, subject?, identifier?, site?}]` and the prompt gains the kind vocabulary and PT §18.5's `subject`, `kind`, `owner_origin_hints`, `newest_primary_date` per gap. The schema and parse stay as strict as today.
- `search()` (`:107-140`) calls `SourceFinding::find(need, budget)` once; paging is inside the service. Its `providers: ["duckduckgo","bing"]` literal and `PAGE_RESULTS` go away; `want` derives from how many candidates the selector's window can hold (the same arithmetic that sizes `select_references`' input today).
- `select_references` (`research.rs:29-93`) receives `owner`, `kind`, `found_by` and `terms` per candidate, so "a mirror of learn.microsoft.com" is visible as `owner: none, domain ≠ subject owner`; PT §18.4 adds the trust facts on top.
- **Exhaustion**: an empty plan for claim gaps is `Expansion::Exhausted { claims, searched }` (the `lattice-evidence` fix PR), never an error; `Findings.exhausted = true` with every finder answered or skipped is the second exhaustion signal and is recorded in the ledger as `ClaimEvent::Researched { needs, outcomes }` so the user can see *what was tried* (CR §5).
- Budget: `Budget.requests` is the ledger's research allowance per (claim, fingerprint) (CR §4, PT §8); `Budget.deadline` is the job's; the lesson's `Stop` cancels.
- `outline_references.rs` plans needs the same way (its subtopic queries already name "primary documentation, official specifications, standards").
- Ratchet: learning imports only `lattice_core::…::{SourceFinding, SourceNeed, …}` and holds `Arc<dyn SourceFinding>` the way it holds `Arc<dyn WebServiceTrait>` (`generation_jobs.rs:42`); the `features::learning → features::web` count stays ≤ 13.

**As built (fix, 2026-10-18).** Wave 1 left three learning paths on plain query strings, and every untyped search went to DuckDuckGo and Bing alone. The fix makes every learning search a typed need (`references/web_research.rs`):

- **One need shape.** `PlannedNeed { query, kind: NeedKind, subject?, identifier?, ecosystem?, site? }`, schema `need_schema(gaps)`, prompt text `NEED_GUIDE`. The model chooses `kind` from `PLANNED_KINDS` (every `NeedKind` but `archive` and `leads`, which start from an address). `PlannedNeed::checked` refuses a query outside 3–240 characters, an unlisted kind or an unknown registry, because not every provider enforces the schema.
- **Claim research** (`plan_needs`): each need names the gap it serves by number (`gap`, required, bounded by the gap count). A need without a subject takes that gap's subject, and nothing else is inferred. The old fallback copied the kind, subject and **first owner host** from the first matching gap (or gap 0) onto every need without them, and `search_need` then filtered results to that host. So a search for an independent source kept only the owner's pages, and a paper search on a .NET claim kept only learn.microsoft.com. Owner hints are now the planner's to use: a need keeps to a host only when the planner set `site`. The `queries`-only answer is gone. The user's research hints are searched verbatim as `web` needs (`PlannedNeed::web`).
- **Course building:** `outline_references::plan` plans typed needs per subtopic (`PlannedSubtopic.needs`). `gather_for` runs each through `search_need`, and outline repair plans with `plan_needs`. An outline draft saved with the old `queries` shape does not load (pre-release, no shim).
- **The lesson Verifier's `search_web` tool** takes the same schema (`need_schema(None)`), and its description carries `NEED_GUIDE`. Arguments that fail `checked` come back to the model as "Search not run: …".
- **Routing:** `web_need` (`features/web/services/web/search.rs`) no longer pins a search that names no engine to DuckDuckGo + Bing. The router chooses for every kind, `web` included (wave 1 keyless engines, wave 2 the user's keyed engines and SearXNG, §3.4), so configured keys reach plain searches too. Naming `providers` still pins them.
- **Not done:** `select_references` still sees origin facts, not `owner`/`kind`/`found_by` per candidate (§3.12, third bullet).

### 3.13 Integration with source provenance and trust (PT)

- `Lead.owner` (registry/Wikidata assertion) is an input to PT §12.2 subject ownership: a lead whose origin equals the resolved owner origin is a *candidate Primary*; the judgement still decides. A Wikidata P856 hit is a `Signal::DeclaredOfficial { by: wikidata }`, stronger than a page's self-declaration.
- `ReadVia::Archive { original, captured_at }` becomes the `archived_from` relation (PT §3, §4), so an archived primary is primary.
- `follow_links` / `leads_from_page` is the candidate generator PT §8's `follow()` pseudocode names ("links with start_byte in span ∪ nearest before/after ∪ footnote targets"); the chain fetch goes through the same fetch chain with the archive fallback.
- PT §15 ("only page fetches") is **amended**: finder calls are part of the network surface, each declared with its privacy class in the descriptor and listed in Settings; the trust *assessment* still makes no calls. This is open decision 1.

### 3.14 Settings and keys

`<core>/src/application/contracts/settings/sourcing.rs`:

```rust
pub struct SourcingSettings {
    pub finders: BTreeMap<FinderId, FinderSetting { enabled: bool }>,   // absent = the descriptor's default
    pub searxng_url: Option<Url>,
    pub contact: Option<String>,          // email or URL for polite pools; off → those finders are Skipped(NoContact) and say so
    pub archive_fallback: bool,           // default true (open decision 4)
    pub finder_concurrency: u32,          // policy default 4
}
```

Keys live in the keyring through `CredentialsPort::{set,get,has,delete}_api_key(service)` (`<core>/…/ports/credentials_port.rs:5-12`) with service ids `brave`, `tavily`, `exa`, `kagi`, `mojeek`, `marginalia`, `github`, `semantic_scholar`, `api_data_gov`, `stack_exchange`. A keyed finder is `enabled && has_api_key`; the Settings panel shows "key missing" rather than hiding it. Privacy defaults: every finder with `privacy: AnonymousQuery | QueryLogged` and `auth: None` is on; `IdentifiedByKey | IdentifiedByContact | UserInstance` is off until configured. A one-line disclosure per finder ("your query is sent to brave.com under your key") is rendered from the descriptor, not hand-written.

### 3.15 Observability

- Structured log per finder call (`sourcing.finder` target): `finder, kind, result, leads, elapsed_ms, cause, retry_after, wave`.
- `MetricsPort::snapshot` gains `sourcing: Vec<FinderHealth { id, state, calls, answered, empty, failed, skipped, last_cause, next_allowed_at, median_ms }>` for the Settings → Sources panel ("DuckDuckGo: healthy, 212 calls, median 1.9 s · Bing: open until 14:02 (429, Retry-After 60 s) · Brave: no key").
- The research step UI prints the outcomes line (§3.6) and the lesson's Sources tab shows which finder found each source (`found_by`).

### 3.16 Eval (in the repo, Josh's rule)

`evals/sourcing/` with `README.md`, `LABEL-RULES.md`, `primary-sources-v1.jsonl`, `results/`:

```jsonc
{ "id": "ps-001", "need": { "kind": "official_docs", "query": "AsyncLocal<T> value flow across await", "subject": ".NET" },
  "gold": { "owner_domains": ["learn.microsoft.com", "github.com/dotnet"], "primary_urls": ["https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1"], "mirrors": ["learn-microsoft.com", "lab.algebra.hr"] },
  "note": "the API reference page; devblogs is Established, not Primary" }
```

~120 items across kinds (official_docs 40, package 20, paper 20, standard 15, law 10, community 10, archive 5), drawn from the three days of real queries (Appendix A lists them) plus the course topics in `evals/courses/requests`. Metrics per kind and per finder set: **primary recall@5/@10** (a gold `primary_url` or any URL under a gold `owner_domain` in the fused top-k), **owner-hit rate** (`resolve_owner` returned a gold owner), **mirror rate** (a gold mirror in top-k), **time to first primary**, **finder failure/skip rates**, **requests per need**. Runner: `cargo run --bin sourcing_eval -- evals/sourcing/primary-sources-v1.jsonl --finders default|default+brave|…` (live network, run by a person, results committed as `results/<date>-<finder-set>.md` with the provider health snapshot, as `evals/retrieval/*-RESULTS.md` do). The release gate: primary recall@10 on `official_docs` and `package` ≥ the previous committed run, no finder set regressing silently.

### 3.17 Network and privacy policy (one table, rendered in Settings)

| Call | Sends | To | Default |
|---|---|---|---|
| Engine search | query | DDG, Bing (unofficial HTML) | on |
| Owner resolution | subject name | Wikidata/Wikipedia | on |
| Registry lookups | package/repo name | crates.io, PyPI, npm, NuGet, RubyGems, Maven, Go proxy, GitHub | on |
| Scholarly lookups | title/DOI (+ contact if set) | OpenAlex, Crossref, S2, arXiv, Unpaywall, Europe PMC | on; contact-needing ones only with contact |
| Standards/law lookups | title/number | rfc-editor, datatracker, w3.org, federalregister.gov, legislation.gov.uk, publications.europa.eu | on |
| Community | query | api.stackexchange.com, hn.algolia.com, a forum | on |
| Archive | the failed/dated URL | archive.org, index.commoncrawl.org | on (decision 4) |
| Keyed engines | query under the user's key | vendor | off |
| User SearXNG | query | the user's instance | off |

Never: proxies, FlareSolverr, Save Page Now, archive.today, Google/Serper/SerpApi, Reddit, any hosted relay. The user's identity never travels unless they set a contact or a key.

### 3.18 Feature layout, ratchets, file sizes

- `<core>/src/domain/sourcing/{mod, need, lead, descriptor, outcome, fusion, canonical, health, policy}.rs` + `*_tests.rs`; `application/ports/{source_finder, source_finding}.rs`; `application/services/sourcing/{routing, routing_tests}.rs`; `contracts/settings/sourcing.rs`; `contracts/web.rs` (new fields only). Pure; no I/O.
- `<desktop>/src/features/web/services/sourcing/{mod, service (SourceFinding impl), runner, health, cache, owner}.rs`; `services/finders/<id>.rs` (+ `_tests.rs`), `services/wayback.rs`, `services/site_discovery.rs`; `services/web.rs` shrinks (search delegates); `services/web/providers.rs` splits into `finders/{duckduckgo,bing_html,wikipedia}.rs`; `stealth.rs` loses proxy/FlareSolverr (→ `identity.rs`, two header builders). `di.rs` builds the registry from settings + credentials; `interfaces/di` exposes `source_finding() -> Arc<dyn SourceFinding>`.
- `features/function_calling`: registry schemas + executor handlers call the core port (no new edge). `features/conversation/chat`: tool gating names and the step-UI outcome line (its edge to web is unchanged). `features/learning`: planner/loop changes against the core port only.
- Migrations: one new dated file `migrations/<date>_sourcing_caches.sql` (owner cache, archive lookups), never the init file.
- Ratchets: `features::learning → features::web` ≤ 13; no `infrastructure → feature` edge (nothing in `infrastructure/` changes); every file ≤ 800 lines, adapters ≤ 300.

## 4. Implementation plan (Opus build agents)

Rules: ≤ 3 agents at once; `cargo check -p lattice-core -p lattice` only while building (through the shared `lockf` lock); `rustfmt <file>`, never `cargo fmt`; grep before read; each worktree gets `src-tauri/binaries/`, `dist/`, a private target dir; tests, clippy, bindings `--check`, `bash scripts/check-rust-layer-boundaries.sh` and `npm run test:unit` run **once** at the end of each wave by the integration track. Base: `main`. Paths below are crate-relative and hold after `src-tauri/` → `crates/lattice-desktop/`.

```
Wave 0 (now, 3 agents): C0 core types+ports ─┬─► R1 resilience (after C0's first commit) ──┐
                                              └─► R2 archive fallback (independent) ─────────┼─► I0 integrate + run gates once
Wave 1 (finders, 3 agents): F1 owner+site ── F2 software ── F3 scholarly+standards ─────────► I1
Wave 2 (2-3 agents): T1 chat tools ── T2 learning loop (after lattice-evidence fix PR + PT P4 land) ── F4 community+law+keyed ► I2
Wave 3 (2 agents): S1 settings+keys+health panel ── E1 eval set+runner+first results ──────► I3 (final gates, as-built notes)
```

| Track | Owned files (exclusive) | Depends on | Done when |
|---|---|---|---|
| **C0 core** | `<core>/src/domain/sourcing/**`, `application/ports/{source_finder,source_finding}.rs` (+ `ports/mod.rs`), `application/services/sourcing/**` (+ `services/mod.rs`), `contracts/settings/sourcing.rs`, `contracts/web.rs` (new fields), `evals/sourcing/LABEL-RULES.md` (vocabulary only) | nothing; **first commit** = types + port signatures within the first hour so R1 can branch | `cargo test -p lattice-core` green: RRF matches a hand-computed fusion, canonical keys fold the archive/utm/canonical cases, the breaker's transitions and `open_for` derivation are table-tested, routing emits the §3.4 waves for each kind |
| **R1 resilience** | `features/web/services/sourcing/{mod,service,runner,health,cache}.rs`, `services/finders/{duckduckgo,bing_html,wikipedia}.rs` (moved from `providers.rs`), `services/web.rs` (delegation), `services/web/{providers,pacing,results}.rs` (shrink/delete), `services/identity.rs` (from `stealth.rs`, proxy/FlareSolverr removed), `features/web/di.rs`, `interfaces/di` accessor, `mocks/web.rs`, `function_calling/executor/web_tools.rs` (outcomes in the result), `conversation/chat/web_steps.rs` (outcome line) | C0 first commit | `web_search` returns `outcomes`; a forced DDG challenge in a test opens its breaker and the same call still returns Bing's leads with DDG `Skipped`; page 2 of a 12-result search makes **no** provider request when the pool holds it; `Retry-After: 60` is honoured up to the deadline; `grep -rn "FLARESOLVERR\|RECALL_PROXY" <desktop>/src` empty |
| **R2 archive fallback** | `services/wayback.rs` (+ tests over fixtures), `services/web/page.rs` (chain), `services/page_cache.rs` (archive key, refusal rule), `contracts/web.rs::FetchUrlContentOutput.read_via` (coordinated with C0: C0 adds the field, R2 fills it) | C0 first commit | a 404 fixture with an availability hit returns `read_via: archive`; `as_of` reads the CDX-nearest copy; a CDX 429 opens the archive breaker and the chain reports it; the refusal cache does not remember a rescued URL |
| **I0** | integration branch; run gates once; as-built notes appended here | R1, R2 | ratchets unchanged; all suites green; Josh runs it live; health log lines visible |
| **F1 owner + site** | `finders/{wikidata,site_discovery}.rs`, `services/sourcing/owner.rs` (+ SQLite cache via a new dated migration), `finders/wikipedia.rs` (external links) | I0 | `resolve_owner(".NET")` → learn.microsoft.com / github.com/dotnet from fixtures; `find(OfficialDocs, subject)` returns owner-site leads first; mirrors rank below owner leads; robots `Disallow` on a sitemap path is respected in a test |
| **F2 software** | `finders/{crates_io,pypi,npm,nuget,rubygems,maven_pom,go_module,github}.rs` | I0 | each registry fixture yields homepage/docs/repo leads with `owner`; GitHub without a token stays under 60/h by the breaker's per-day budget; crates.io honours 1 rps through the pacer |
| **F3 scholarly + standards** | `finders/{openalex,crossref,semantic_scholar,arxiv,unpaywall,europe_pmc,rfc_index,ietf_datatracker,w3c,whatwg_index,ecma_pattern}.rs` | I0 | a DOI need returns the publisher landing page, an OA copy and references as `Leads`; arXiv's 3 s interval is enforced; `find(Standard, "RFC 9110")` returns rfc-editor.org first |
| **I1** | integration; eval smoke on 20 items | F1-F3 | gates once; primary recall numbers recorded in `evals/sourcing/results/` |
| **T1 chat tools** | `function_calling/registry.rs` (four schemas), `executor/web_tools.rs`, `executor.rs` names, `conversation/chat/tools.rs` (fixed list, masking), `chat/web_steps.rs`, `tool_loop/{tool_sources,call_labels}.rs`, `src/features/chat/components/turn/**` (outcome line, read_via badge), `src/lib/bindings.ts` regen | I1; coordinate with the KV-cache "fixed tool list" change if it lands first (same file `chat/turn/assemble.rs`) | the tool list is identical across turns of one conversation; `find_sources(kind: package, identifier: "crates:tokio")` round-trips; `follow_links` returns offsets that match the snapshot bytes |
| **T2 learning loop** | `references/web_research.rs` (`plan_needs`, `search` → `find`), `content_verification/research.rs` (selector facts, exhaustion), `planning/outline_references.rs` | I1, **lattice-evidence fix PR merged**, **PT P4 merged** (both own these files; T2 goes last) | an empty claim plan yields `Exhausted`, never the "malformed" error; a Specification gap with a known owner issues an `OfficialDocs` need first; `features::learning → features::web` ≤ 13 |
| **F4 community + law + keyed** | `finders/{stack_exchange,hn_algolia,discourse_json,federal_register,legislation_uk,eurlex_celex,govinfo,brave,tavily,exa,searxng,marginalia}.rs` | I1 | Stack Exchange `backoff` is obeyed and attribution rides the lead; keyed finders are `Skipped(NoKey)` without a key and answer with one (recorded fixtures); SearXNG with JSON disabled reports the 403 honestly |
| **I2** | integration; gates once | T1, T2, F4 | Josh runs a lesson and a deep-research chat live; outcome lines and badges visible |
| **S1 settings** | `features/settings/**` (sourcing settings + validation), `src/features/settings/components/SourcesTab.tsx` (new; finders, keys via the credentials commands, SearXNG URL, contact, archive toggle, health panel), `MetricsPort` snapshot field | I2 | every finder in the registry renders with its disclosure line, state and last cause; a key saved in the keyring enables its finder on the next search |
| **E1 eval** | `evals/sourcing/{README.md, primary-sources-v1.jsonl, results/}`, `<desktop>/src/bin/sourcing_eval/main.rs`, `scripts/sourcing_eval_report.py` | I2 | 120 labelled items with notes; a committed first run on the default finder set and on default+Brave; the README states the labelling rule and the blind re-label protocol as `evals/grounding/README.md` does |
| **I3** | final gates; as-built section appended here; baseline unchanged or smaller | S1, E1 | PR open; results committed |

Collision surface: `references/web_research.rs` and `content_verification/research.rs` are owned by the fix PR and PT P4 before T2; `chat/turn/assemble.rs` by the KV-cache work before T1; `contracts/web.rs` by PT P1 (`signals`) and C0 (`read_via`, `outcomes`) — C0 adds its fields in one commit and PT P1 rebases, or the reverse, whichever lands first; `interfaces/di/*` gets one accessor from R1 only.

## 5. Decisions (asked as open decisions with a recommendation; all decided by Josh on 2026-10-18)

1. **Amend PT §15 to admit finder calls** (each declared with a privacy class, shown in Settings) while trust assessment stays call-free. *Recommend: yes.* The toolbelt cannot reach primary sources without asking registries and standards bodies; the queries are public subject terms, the user's identity never travels, and the provenance design already allows owner-site searches through a provider. **DECIDED 2026-10-18 (Josh): yes.** Declared source-finder calls are allowed under provenance's network policy.
2. **Which keyed engines to support first.** *Recommend: Brave (own index, $5 monthly credit ≈ 1 000 queries, `site:`), Tavily (1 000 free credits, no card), Exa (papers/repos), plus a user SearXNG URL.* Skip Serper/SerpApi (Google litigation), Google CSE (closing), Kagi/Mojeek/You.com/Perplexity until asked. Marginalia as a free, non-commercial extra. **DECIDED 2026-10-18 (Josh): all four**: Brave Search, Tavily, Exa and a user-supplied SearXNG URL. Each is opt-in and off until the user adds a key (keyring) or a URL (settings). C0 types express them (`Auth::RequiredKey { service: Brave | Tavily | Exa }`, `Auth::UserInstance { setting: SearxngUrl }`, `Privacy::IdentifiedByKey` / `SentToUserInstance`, `SourcingSettingsDto::searxng_url`), so F4 adds adapters only. Kagi, Mojeek and Marginalia were not part of the decision, so `KeyService` holds only the decided services (one variant each to add if asked).
3. **Polite-pool contact.** Crossref, Unpaywall and NCBI ask for an email; SEC wants a name+email User-Agent. *Recommend: an optional contact field, off by default; those finders say "needs a contact" when unset.* Never a hard-coded project address: it would make every user look like one client and rate-limit them together. **DECIDED 2026-10-18 (Josh): an optional contact, off by default, never hard-coded.**
4. **Archive fallback on by default.** It sends a failed URL to archive.org (and Common Crawl's index). *Recommend: on*, same privacy class as a search query, with the toggle in Settings and the `read_via` badge on every rescued read. **DECIDED 2026-10-18 (Josh): on by default**, with the `read_via` badge.
5. **Fold `wiki_search`/`wiki_summary` into `find_sources(kind: encyclopedia)` + `fetch_url_content`.** *Recommend: yes* at T1, since the tool list is being fixed per conversation anyway; two fewer tools in the prefix. **DECIDED 2026-10-18 (Josh): yes**, one `find_sources` tool.
6. **GitHub token as an optional key.** 60 requests/hour unauthenticated is enough for a chat, tight for a course with many software claims. *Recommend: offer it, off by default*; the breaker's per-day budget keeps the keyless path within limits. **DECIDED 2026-10-18 (Josh): offered, off by default.**
7. **Rollout order.** *Recommend: Wave 0 first and alone* (two weeks of live use with the health log before finders land), because it fixes the three measured defects (silent failures, wasted depth, no archive) without changing what the model sees. **DECIDED 2026-10-18 (Josh): Wave 0 ships alone first.**

## 6. As-built notes

### 6.1 C0 core types, ports and pure services (Wave 0, 2026-10-18)

Built on `feat/source-finding`. Tests written, not run (they run once at I0). Where the code differs from §3, the code is right and the reason is here.

**Types (`<core>/src/domain/sourcing/`).**

- `NeedKind` is fieldless (the tool's `kind` enum and `&'static [..]` roles need a plain enum). A package's ecosystem rides on `Identifier::PackageName { ecosystem }`; a dated read is `Freshness::AsOf(date)`, and any need with it routes to the archive recipe (`routing::routed_kind`); a `Leads` need's page goes to `leads_from_page`.
- `SourceNeed` gains `alternatives` (the caller's rewordings, §3.4) and `also` (kinds whose first-wave finders join wave 1: chat's "Wikipedia when asked" is `also: [Encyclopedia]`).
- `SiteScope` and `OriginHint` are one type, `Origin { host, path_prefix }`, with `covers(url)` (subdomains yes, sibling paths no). `Span` is `Near::{Quote, Bytes}`, since the tool passes a quote.
- `Lead` gains `rel_canonical` (dedupe folds into it) and `terms` (§3.9). `LeadKind::ReferenceOf { page, place }` carries `follow_links`' anchor, sentence and offset. Tie precedence extends §3.5's order: `Document > OfficialSite > Repository > Page = ReferenceOf > ArchivedCopy`.
- `FinderDescriptor.finds` is `&[Role { kind, wave }]`, not `&[NeedKind]`: the wave a finder joins for each kind is descriptor data, so `routing.rs` holds only where the general engines sit. Engines are finders with a `Web` role: wave `First` = keyless, `Second` = opt-in. Added `name` (for disclosures) and `sends_to: Destination` (where the query goes; `wave_concurrency` counts distinct destinations).
- `Reach::Registry(Ecosystem)` (so "that registry, exactly" is data), plus `CodeHost`, `Encyclopedia`, `LinkedPages`.
- `Auth` is `None | OptionalKey { service } | RequiredKey { service, plan: Free | Credits | Paid } | UserInstance { setting } | Contact` (the design's `FreeKey`/`PaidKey` split cost from need; GitHub, Stack Exchange and S2 work keyless and take an optional key). Default on/off is not a field: `FinderDescriptor::activation()` derives it from `Auth` (keyless on; key, instance or contact off until configured). `KeyService` is an enum of keyring ids holding only the decided services (Brave, Tavily, Exa, GitHub, Semantic Scholar, api.data.gov, Stack Exchange). `Privacy` adds `SentToUserInstance`. `disclosure()` renders the Settings line from the descriptor.
- `RatePolicy.per_day: Option<u32>` became `quota: Option<Quota { requests, per }>` (GitHub's keyless limit is per hour), and gained `trip_on_first_refusal` (§3.8: the archive's penalty is an hour-long block).
- `SkipReason` adds `NoInstance`, `NoContact` (§3.14 names it) and `QuotaSpent`. `FailureCause` and `SkipReason` implement `Display` for the outcome line and the all-failed error.
- `SourcingPolicy` adds `BREAKER_CEILING` (24 h, documented in `policy.rs`) and helpers `wave_concurrency`, `is_thin` (`2 × new < want`, exactly "half of want") and `wants_another_wave`.

**Ports.** `Budget` lives in `ports/source_finder.rs` (`deadline: std::time::Instant`, `stop`, `requests`, plus `remaining()`/`is_spent()`). `SourceFinding` is `automock`ed for callers' tests. `SourceFinder::find` returns a `FinderOutcome`, never an error.

**Contracts.**

- `SourcingSettings` is `SourcingSettingsDto` (camelCase, `specta::Type`) by the contracts convention; `finders` is keyed by id string; `searxng_url` is `Option<String>` read through `searxng_url() -> Option<Url>` (http(s) only), because the workspace `url` crate has no `serde` feature. `gate(descriptor, keys)` returns `Disabled | NoKey | NoInstance | NoContact`; turning `archive_fallback` off also turns the `Reach::Archive` finders off. It is not yet a field of `AppSettingsDto`: S1 wires it (R1/R2 use `SourcingSettingsDto::default()`).
- `WebSearchOutput.outcomes: Vec<FinderOutcomeDto { finder, result, leads, elapsed_ms, cause }>` is left out of the payload when empty. `FetchUrlContentOutput.read_via` is left out when `Direct` (as `from_cache` is), so an ordinary read's payload is unchanged; browser and archive reads say so. The twelve desktop construction sites take `Vec::new()` / `Default::default()` until R1 and R2 fill them.

**Pure services.**

- Canonical keys (`canonical.rs`): `key(url)`, `lead_key(lead)` (`rel_canonical`, then an archived copy's original, then the URL) and `unwrap_archive(url)` (Wayback `/web/<ts>[mod_]/<original>`, with the capture time) for PT's archive reader to share. The key folds `http`/`https` (archives record either; the key never chooses what is fetched), keeps `www.` distinct, decodes percent-encoded unreserved characters and uppercases the rest, and drops the same tracking parameters as `results.rs` today (including `ref` and `source`).
- Fusion (`fusion.rs`): `rrf(lists: &[RankedList { finder, wave, leads }], k, owner)` takes waves and the owner because the tie-breaks need them; `rrf_scored` returns the scores. A lead's rank is its own `found_by` entry for that finder when present (a page-2 lead ranks 11+), else its position; scores sum in ascending rank so equal rank sets tie exactly. The representative of a folded group is the highest-precedence occurrence (a live page over its archived copy), then the best rank. `diversify(leads, owner)` is the post-fusion origin-diversity pass.
- Breaker (`health.rs`): Challenge counts toward the two-refusal streak (§3.2's table) rather than opening at once (§3.6's wording); a host instruction (`Retry-After` from a finder that honours it, `RateLimited { until }`) opens at once, for exactly that long, and a later, longer instruction extends an open window. 5xx counts as transport. 404/451/other 4xx, `Parse` and `Budget` are neutral: they neither count nor reset a streak. `trips` counts this opening, so the first derived window is `min_interval × 2`. **Deviation:** `open_for` is *not* cut at the caller's deadline. Cutting the window would let the next caller hit a host that asked for an hour (the archive case). The deadline bounds the caller's *wait* instead: `Breaker::wait_before(now, remaining)` returns the wait when the window ends before the deadline, else `None` (skip as `CircuitOpen`). Documented allowances (`Quota`) are counted in a fixed window inside `admit`.
- Routing (`application/services/sourcing/routing.rs`): `plan(need, registry, gate) -> Plan { resolve_owner, waves: [Wave { steps: [Step { finder, scope: AsAsked | Owner }], skipped, reformulated }] }` replaces `waves: Vec<Vec<FinderId>>, reformulate_after`. It takes a `FinderGate { settings, keys, refused }` (the runner passes open breakers and spent allowances as `refused`) rather than `health` and `policy`. Skips are in the plan, each finder listed once, in the first wave that wanted it. A step repeated with the same finder, scope and query is dropped, and so is an empty wave, so `web` without rewordings has two waves. Recipe choices the table left open: `official_docs` with neither subject nor scope routes like `web`; with a scope but no subject it is owner-first on the scope, without `resolve_owner`; `package` wave 3 and `standard` wave 2 search the owner (registry homepage, standards body) as `StepScope::Owner`; owner-scoped engine steps only go to engines with `supports_scope`. `law` has no jurisdiction filter yet (F4).
- Cache policy (`application/services/sourcing/cache_policy.rs`): TTLs from the policy, `is_fresh` (a future stamp is stale), `provider_page_usable` (an `After(date)` need bypasses pages stored before that date), and the provider-page, owner and archive-lookup keys.

**Tests written (run at I0):** `domain/sourcing/{canonical,fusion,health,descriptor}_tests.rs`, `application/services/sourcing/{routing,cache_policy}_tests.rs`.

### 6.2 R1 resilience (Wave 0, 2026-10-18)

Built on `feat/source-finding` after C0. Tests written, not run (I0 runs them). No lattice-core file was edited.

- **Layout.** `services/finders/{mod,serp,duckduckgo,bing_html,wikipedia}.rs` replace `web/providers.rs` and `web/results.rs` (both deleted; `serp.rs` is the result-page parser, now free functions keyed on core `canonical::key`). `services/sourcing/{mod,service,runner,health,cache}.rs` hold the `SourceFinding` implementation. `web/search.rs` is the `search_web` delegation (input → `SourceNeed`, findings → `WebSearchOutput`); `web/pacing.rs` keeps only `SitePacer`, now shared (`Arc`) by page reads and finders; the whole-input `SearchCache` is gone (the provider-page cache replaces it, so a regenerate also costs nothing). `stealth.rs` → `identity.rs`: proxy pool, FlareSolverr client and the resolver's proxy exemption deleted; `declared_client_headers` (`Lattice/<version> (+https://github.com/J-Reed700/lattice[; contact])`) added; Wikipedia now uses it. `page.rs` (R2's) changed only its `stealth` import to `identity` (3 lines), which the rename forces.
- **`FinderEntry { finder, page_size }`.** The runner needs a finder's page size to know whether a page was full ("a short page means the engine has no more"), and `SourceFinder` has no such method; the entry carries it (DDG 10, Bing `count=10`, Wikipedia 10 = MediaWiki's default `srlimit`).
- **Scraped engines trip on the first refusal.** DDG and Bing HTML set `RatePolicy::trip_on_first_refusal`, so a challenge (or 403/429) opens their breaker at once, which is §3.6's rule; C0's two-refusal streak still governs API finders (Wikipedia). Their `min_interval` is the pacer's 900 ms (neither engine publishes a rate), so a first trip holds for 1.8 s and doubles per consecutive trip. Within one call, a finder that refused is never asked again: later steps (deeper pages, reworded waves) record `Skipped(CircuitOpen { last })`.
- **Retries and waits.** A `Retry-After` (seconds or HTTP-date, no default, no cap) opens the breaker for exactly that long; the runner waits it out once, when the call is still short of `want` and the wait ends before the deadline, else the outcome names it and later calls skip the finder until then (refused at planning when the window outlasts the caller). Transport/5xx failures retry after the finder's `min_interval` until the breaker trips (three in a row): the trip count is the retry bound. The old 3-try loop, 250 ms backoff, retry-on-empty and the three-endpoint retry matrix are gone; DDG tries its next endpoint only on 404/410/5xx.
- **Paging.** Steps' first pages run concurrently (`wave_concurrency` over distinct destinations); further pages one at a time, from the step with the fewest pages, while the pool is short of `want`, the step's last page was full and brought something new, and at most `ceil(want / page_size)` pages per step. Answered pages are cached for `search_page_ttl`; empty and failed pages are not.
- **Outcomes.** `Findings.outcomes` is one per finder (leads summed, time summed; the latest failure or skip is the result, so a later answer never hides it). `search_web` puts them in `WebSearchOutput.outcomes`; `web_steps::finish_search` adds one note under the search ("duckduckgo: skipped, paused until … (bot challenge) · bing_html: 10") whenever any finder did not answer; each page logs `Source finder outcome` with `finder, kind, result, leads, ms, cause`. All failed or skipped with no leads → `Err(Network)` listing every finder's cause. `providers` names map onto finder ids (`bing` → `bing_html`); `followup_queries` became `alternatives` and run only in the reworded wave when the first was thin (they are listed in `followup_queries` only if they ran).
- **No deadline on `search_web`.** The port carries none (the tool loop and the lesson loop drop the call at their own deadline), so the delegation's budget deadline is `breaker_ceiling`: a host's wait is honoured whenever the caller is still waiting.
- **Not built in Wave 0:** `resolve_owner` and `leads_from_page` return `NotFound` naming the missing finder (F1, T1); the registry is built from `SourcingSettingsDto::default()` and no keys inside `WebService::with_timeout` (S1 wires settings and the keyring); `Container::source_finding()` (in `features/web/di.rs`) exposes the same service `search_web` uses.

**Tests written (run at I0):** `finders/{tests,serp_tests}.rs` (wiremock: challenge with no second endpoint, gone endpoint falls through, page-2 offset/ranks/`site:`, 429 `Retry-After: 60`, Bing refusal page, Wikipedia declared agent, `ratelimited`, HTTP-date parsing), `sourcing/tests.rs` (fake clock: challenge opens the breaker and the same call returns Bing's leads with DDG `Skipped`; next call skips without a request; `Retry-After: 60` waited within a 120 s deadline and not within 30 s, held until then; transport retries end at the trip; all-failed error names each cause; cached pages cost nothing; short page ends paging), `web/search_tests.rs` (outcomes in the tool output; page 2 of a 12-result search makes no provider request), `identity.rs`, `web_steps.rs`, `executor/tests.rs`.

### 6.3 R2 archive fallback (Wave 0, 2026-10-18)

Built on `feat/source-finding` after R1. Tests written, not run (I0 runs them). No lattice-core file was edited. The archive APIs were re-read on 2026-10-18 (Appendix B rows updated); nothing below is sized from an unverified figure.

- **Layout.** `services/wayback.rs`: three endpoint descriptors (`wayback_available`, `wayback_cdx`, `wayback_replay`; `Reach::Archive`, `DeclaredClient`, `QueryLogged { by: "the Internet Archive" }`), one R1 `HealthBook` (made `pub(crate)`; the only R1 edit), answer parsing. `web/page.rs`: `Want::{Live, AsOf}`, `read_live`, `PageText`; `keep_page` takes the `ReadVia`. `web/page_archive.rs`: a child of `page.rs`, split out to keep it under 800 lines (rescue, thin-page top-up, dated reads, refusal rule). `page_cache.rs`: archive entries and `ArchiveAnswer`; its tests moved to `page_cache_tests.rs`. `WebService` holds the `Wayback`, built from the finders' `FinderHttp` and the default sourcing settings (S1 wires the user's).
- **Chain.** cache → guard → pacer → direct GET → (403/429/503) browser → still nothing → archive. "Still nothing" is any non-success status after the browser step, a transport failure after the guard passed, or a shell the browser did not fill (the archived copy replaces a thin live text only when longer). A URL that fails the address guard, including one that does not resolve, is never sent to the archive; a URL that is itself a Wayback copy is not re-asked. The site's pacer slot is released before the archive steps (pages on archive.org share the archive's slot).
- **Lookup.** Availability (with `timestamp` for `as_of`); when it has no capture that answered 200, CDX `closest=<day, or now>&sort=closest&filter=statuscode:200&limit=1&fl=timestamp,statuscode`; then replay `web/<ts>id_/<asked url>`. Both indexes empty (or switched off) is `NothingArchived`; any failure or skip is `Unanswered`, naming each endpoint's outcome (`Internet Archive: wayback_available: empty; wayback_cdx: failed (HTTP 429)`). A replay 403/404/410/451 (the page cache's settled statuses) is a settled miss. **Deviation:** `collapse=digest` and a budget-derived `limit` are not sent: the chain reads one copy, the nearest, so one row is all it can use.
- **Labels.** `read_via: archive { captured_at, original }`: `captured_at` is the capture the replay served (a replay redirect to a neighbour is followed and named), `original` the URL asked for (the indexes record the crawler's variant), `url` the snapshot a person opens (`web/<ts>/<original>`). Browser reads now say `browser`; a direct read of a `web.archive.org/web/<ts>/…` URL says `archive`. Cache hits keep the label (`CachedPage.read_via`; older cache files are misses). The content is never altered: the date rides on the label. The PT doc is not in this tree, so `ArchiveOf` is left to PT, which gets `original` and `captured_at` on the output.
- **Caching.** Lookups (a capture, or a settled miss) under `archive-lookup|archive_lookup_key(url, as_of)` for `archive_lookup_ttl`; archived copies under `archived|<canonical key>|<timestamp>` with no TTL (the bounds evict). **Deviation:** the SQLite `sourcing_archive_lookups` table is not built; the page cache's files hold the lookups (no migration, its "nothing here is worth keeping" rule). A rescued read is also cached under the URL asked for (page TTL), so the next plain read is the labelled copy without a request; a dated read is cached only under its snapshot URL and archive key, never under the plain URL.
- **Refusal rule.** `remember_refusal(…, ArchiveAnswer)` keeps a settled status only when the archive had nothing (`NothingArchived`) or was not asked (`NotAsked`: switch off, or an archive URL). `Unanswered` leaves the URL retryable; the archive's breaker decides whether the archive is asked again.
- **Breakers.** Each endpoint: `min_interval` = the pacer's 900 ms (archive.org documents no rate), `trip_on_first_refusal` (verified: a 429 came without `Retry-After`, so the core breaker derives the window), `honours_retry_after` in full. No quota, because the community figures are unverified; CDX, the endpoint seen refusing, is asked only when availability has no 200 capture.
- **429 on a live page.** The old `unwrap_or(2).min(5)` retry is gone: the host's `Retry-After` (seconds or HTTP-date, `finders::retry_after`) is waited once in full, as R1's runner does; no header, no retry (browser, then archive).
- **`as_of`.** `WebService::read_page_as_of(url, day)`. The tool parameter and a `WebServiceTrait` method are T1's (the port is unchanged). A dated read checks the URL syntactically (`check_url_host`: no private literal, no localhost) since nothing goes to the live site, and is refused (`InvalidInput`) when the switch is off.
- **Not built:** paywall-text detection (a paywalled page reaches the archive only when its text is thin); the Common Crawl index (the archive recipe's second wave); the archive endpoints in the health panel (`Wayback` owns its `HealthBook`; S1 can expose its snapshot).

**Tests written (run at I0):** `wayback_tests.rs` (availability and CDX fixtures as documented and as answered live; an availability hit skips CDX; a dated lookup sends `timestamp` and asks CDX with `closest`/`sort=closest`/`filter`/`limit=1`; both empty is `NothingArchived`; a CDX 429 is reported and the next lookup skips CDX without a request; `Retry-After: 3600` held in full; the switch off sends nothing; replay `id_` URL, label and snapshot URL; replay 404 gone vs 503 unanswered), `web/page_tests.rs` (a 404 with an availability hit returns `read_via: archive` and is cached as the copy, never as a refusal; `as_of` reads the CDX-nearest copy and never stands in for the live page; a CDX 429 opens the breaker, the error names it, the refusal stays unremembered and the next read reports the pause; nothing archived settles the refusal; switch off), `page_cache_tests.rs` (refusal rule, rescued page replaces a refusal, lookup TTL, archive key across schemes and without TTL), `web/tests.rs` (`keep_page` call sites).

### 6.4 Registry records speak only for the named package (fix, 2026-10-18)

Waves 1 and 2 shipped two ways for a guess to become the subject's owner. The npm, NuGet, RubyGems and Maven finders searched with the whole question and returned every hit's homepage as an `OfficialSite` with an asserted owner; PyPI took the question's first word as a package name; Go looked the question up as a module path. Then the runner adopted the first lead's owner, so a wrong package's site scoped the owner wave and won fusion's owner boost. GitHub's repository search did the same. And with no ecosystem stated, a package need asked all seven registries, where a name like `requests` is a different package on PyPI and on npm.

- **The caller states the registry.** `find_sources` and the lesson planner's needs take `ecosystem` (`pypi`, `crates`, `npm`, `nuget`, `rubygems`, `maven`, `go`; the enum is `Ecosystem::ALL`), and the planner prompt asks for it from the lesson's language. `web_need` turns `ecosystem` + `subject` into `Identifier::PackageName`, so routing asks that registry alone (§3.4). An explicit `identifier` wins.
- **A named package is looked up by name, and only its record is kept.** `SourceNeed::package()` is the identifier's package, or a `package` need's subject with its ecosystem unknown. Each registry finder searches or looks up that name (Maven by `g:`/`a:` coordinates, Go only for a module path, whose first element holds a dot) and keeps the record whose name is the same under the registry's own rules (`Ecosystem::same_package`: PEP 503 for PyPI, `-`/`_` for crates.io, `group:artifact` for Maven, the path for Go, case folded). A need that names no package still searches by its words, and those hits stay as candidates.
- **Only a lookup of the named subject asserts its owner** (`records::asserted_owner`). A registry asserts the owner of the named package's record; when the registry was not named, only if every registry holding the name agrees on one owner (`registry_owner`). A code host asserts an owner only for a named repository. Standards bodies and other finders are unchanged.
- **A package named without its registry resolves its owner first** (`Plan::resolve_owner`, Wikidata). The registry records are then kept only when one of the resolved origins covers their homepage, repository or registry page (`records_on_owner`); a record's leads share its asserted owner and are kept or dropped together.

## Appendix A. Log statistics (2026-10-16 + 2026-10-17; 2026-10-18 had no web activity)

Source files: `~/Library/Application Support/lattice/logs/lattice.log.2026-10-1{6,7,8}` (17 747 + 3 596 + 473 lines). Counts by `grep -c`/`awk`; the message shapes are those of `providers.rs`, `web.rs`, `page.rs`, `browser_reader.rs`, `research.rs`, `outline_references.rs`.

**Provider requests**

| Metric | 10-16 | 10-17 | Total |
|---|---|---|---|
| `Web search attempt` (provider page requests) | 753 | 181 | 934 |
| via `html.duckduckgo.com` / `duckduckgo.com` / `lite.duckduckgo.com` | 419 / 0 / 0 | 100 / 0 / 0 | 519 / 0 / 0 |
| via `www.bing.com` | 334 | 81 | 415 |
| try 1 / try 2 / try 3 | 749 / 3 / 1 | 180 / 1 / 0 | 929 / 4 / 1 |
| `provider returned status` (non-success) | 0 | 0 | 0 |
| `anti-bot challenge page` | 0 | 0 | 0 |
| `Web search request failed` (transport) | 0 | 1 | 1 |
| `Web search completed` (searches) | 162 | 40 | 202 |
| served from the recent-search cache | 0 | 0 | 0 |
| `All web search providers failed` | 0 | 0 | 0 |
| `providers_used` = {bing, duckduckgo} | 162 | 40 | 202 (both engines answered every search) |

**Results per search by page** (`page=` from the preceding `Web search:` line, `results=` from `Web search completed`)

| page | searches | results distribution | mean merged pool |
|---|---|---|---|
| 1 | 106 | 12 ×87, 11 ×6, 10 ×13 | 18.5 |
| 2 | 87 | **0 ×35, 1 ×13**, 2 ×4, 3 ×6, 5 ×1, 6 ×2, 8 ×5, 9 ×5, 10 ×6, 11 ×2, 12 ×8 | 15.4 |
| 3 | 8 | 0 ×3, 1 ×1, 4, 7, 10, 12 | 27.8 |
| 4 | 1 | 12 | 54 |
| 5 | 1 (same query, `.NET runtime ExecutionContext …`) | — | — |

Latency: n = 202, min 1 464 ms, p50 1 984, p90 4 016, max 6 793. Deep-research telemetry: unique_urls min 10, p10 10, p50 20, p90 23, max 54; mean unique_domains 13.2.

**Queries**: 203 `Web search:` lines, 106 distinct, mean 7.7 words, 34 with ≥ 10 words, 0 with `site:`, all with `max_results=12, providers=["duckduckgo","bing"]` (lesson/outline research; chat retrieval uses `max_results=5`). Sample of the distinct queries (for the eval set): `.NET runtime ExecutionContext FlowSynchronousContext flow continue async void exception handling` (pages 1-5); `SynchronizationContext introduced .NET Framework 2.0 history` (four variants); `ExecutionContext.Capture implementation .NET Core immutable clone copy on write`; `Task.Yield IsCompleted false implementation dotnet runtime source code`; `How the C# compiler generates IAsyncStateMachine and MoveNext for awaitable methods`; `pgvector HNSW vs IVFFlat index performance comparison`; `PostgreSQL EXPLAIN ANALYZE reading query plans cost estimation`; `reciprocal rank fusion hybrid search BM25 vector implementation`; `RAGAS framework faithfulness answer relevance metrics`; `OWASP LLM prompt injection prevention input validation sanitization before LLM processing timing`; `JSON Schema additionalProperties false LLM function calling hallucinated fields OpenAI strict mode`; `Toolformer self-supervised tool use learning mechanism description matching`; `React 19 new features Actions useActionState useFormStatus`; `TanStack Query v5 React integration best practices`; `OAuth 2.0 OpenID Connect ASP.NET Core authentication configuration`; `Saga pattern distributed transactions choreography vs orchestration`; `nDCG normalized discounted cumulative gain calculation example`; `Behavioral interview STAR method technical leadership stories`.

**Fetches**

| Metric | 10-16 | 10-17 | Total |
|---|---|---|---|
| `Fetching URL` | 197 | 44 | 241 |
| `URL fetched` (text returned) | 194 | 42 | 236 |
| `URL fetch failed with status` | 15 | 4 | 19 |
| of which 403 / 429 / 404 | 10 / 5 / 0 | 1 / 2 / 1 | 11 / 7 / 1 |
| `Failed to fetch URL` (transport) | 2 | 0 | 2 |
| `Rate limited by host; retrying once` | 5 | 2 | 7 |
| page-cache hits / refusal-cache hits | 0 / 0 | 0 / 0 | 0 / 0 |
| `Page read through the browser` | 19 | 6 | 25 |
| `Browser read did not help` | 2 | 0 | 2 |

Failed-status hosts: stackoverflow.com 403 ×9; deepwiki.com 429 ×3; www.zeroentropy.dev, www.saumilsrivastava.ai, www.evidentlyai.com, docs.ragas.io 429 ×1 each; www.sciencedirect.com 403, genai.owasp.org 403, llmdb.app 404. Browser rescues by host: stackoverflow.com 9, deepwiki.com 5, www.c-sharpcorner.com 2, docs.ragas.io 2, community.openai.com 2, www.saumilsrivastava.ai, www.querystack.tech, leaderboard.mteb.org, ieeexplore.ieee.org, genai.owasp.org 1 each. Browser failures: sciencedirect.com and openreview.net, both "the page is behind a check that asks a person to click". Words per fetched page: p10 236, p50 1 503, p90 6 212. Fetched hosts (top): learn.microsoft.com 40, github.com 20, www.postgresql.org 10, stackoverflow.com 9, arxiv.org 9, devblogs.microsoft.com 6, source.dot.net 5, deepwiki.com 5, react.dev 4, ai-tldr.dev 4, martinfowler.com 3, techinterviewhandbook.org 3.

**Learning**

| Event | Count |
|---|---|
| `Planned outline reference searches` | 2 (subtopics=9 queries=36; subtopics=10 queries=30) |
| `Saved an outline reference` | 84 |
| `Outline reference page unavailable` | 7 (learn-microsoft.com; lab.algebra.hr/en-us/dotnet/csharp/whats-new/; sciencedirect; openreview; deepwiki ×2; leaderboard.mteb.org) |
| `Lesson research page unavailable` | 2 (llmdb.app 404; shishirpatil.github.io) |
| unavailable causes | "did not provide enough static reference text" ×4, "too deeply nested" ×1, HTTP 403 ×1, HTTP 404 ×1, "error sending request" ×2 |
| Jobs failed `The reference search plan was malformed. The lesson remains unpublished.` (empty claim plan) | **5** (all 10-17) |
| Jobs failed `The supplied sources exceed this model's context window…` | 2 (10-16) |
| `Lesson preparation stage changed` | 70 |

## Appendix B. Research table (every provider and finder; dates are when the source was read)

| Provider / finder | Reaches | Auth | Free tier / price | Rate limit | Terms for a desktop app | Privacy | `site:` | Source, date, mark |
|---|---|---|---|---|---|---|---|---|
| DuckDuckGo html / lite | Bing-derived web mix | none | free | none published; 202/"anomaly" soft block | no API terms; unsanctioned | query logged | yes | apiserpent.com/blog/scrape-duckduckgo-serp-for-free, 2026-10-18, R; own logs 0 blocks/934 |
| Bing HTML | Bing index | none | free | — | ToS forbids scraping | query logged | yes | — , R |
| Bing Search APIs | — | — | **retired 2025-08-11**; replacement Grounding with Bing Search (Azure AI Agents) | — | — | — | — | learn.microsoft.com/lifecycle/announcements/bing-search-api-retirement, 2026-10-18, V |
| Brave Search API | own index | key + card | $5/1 000; "$5 in free credits every month"; Answers $4/1 000 + $5/M tokens | 50 qps (Search), 2 qps (Answers) | storage needs "a plan that includes storage rights"; no attribution rule on page | key = account | yes (R) | brave.com/search/api, 2026-10-18, V |
| Tavily | LLM-shaped snippets | free key | 1 000 credits/month, no card; 1-2 credits/search; $0.008/credit | not stated | — | key = account | ? | docs.tavily.com/documentation/api-credits, 2026-10-18, V |
| Exa | neural web, papers, repos, companies | paid key | $7/1 000 (search + contents), Deep $12, Deep Reasoning $15 | — | — | key = account | filters | exa.ai/docs/changelog/pricing-update, 2026-10-18, A |
| Google Custom Search JSON | Google | — | **closed to new customers; ends 2027-01-01**; 100/day free, $5/1 000 for existing | 10 000/day | — | — | — | developers.google.com/custom-search/v1/overview, 2026-10-18, V |
| Mojeek API | own index | paid key | £1-3/1 000 | 1-10 qps | — | — | ? | mojeek.com/services/search/web-search-api, 2026-10-18, A |
| Marginalia | independent web | free key by email | free, CC-BY-NC-SA 4.0 | shared key limited | non-commercial | — | n/a | about.marginalia-search.com/article/api (2025-12-08), 2026-10-18, A |
| Kagi Search API | high-quality mix | account bot token | ≈$2.50/1 000 (R) | — | account-bound | key = user | ? | help.kagi.com/kagi/api/search.html, 2026-10-18, A |
| SearXNG | metasearch | user instance | free | instance limiter (429); JSON must be enabled (403) | self-host or user URL | operator | yes | docs.searxng.org/dev/search_api, 2026-10-18, A |
| Serper / SerpApi | Google scrape | paid | 2 500 / 250 free (R) | — | Google sued SerpApi 2025-12-19 (R) | key | yes | usagepricing.com SerpApi lawsuit page, 2026-10-18, R |
| You.com / Perplexity Search / Jina s.jina.ai | own indexes | key | $5/1 000 ($100 credit) / $5/1 000 / 10M tokens free non-commercial (R) | — | — | key | ? | about.you.com/resources/lower-search-api-cost; alternativeto.net 2025-09; docs.openwebui.com, 2026-10-18, R |
| Startpage, Qwant | — | — | no public API found | — | — | — | — | R |
| Wikipedia Action API | encyclopedia, external links, references | none | free | descriptive UA required; 403 for generic UAs | CC BY-SA; UA policy | query logged | n/a | foundation.wikimedia.org User-Agent policy, 2026-10-18, V |
| Wikidata (EntityData, SPARQL) | owner domains: P856, P1324, P2078, P1401, P1581, P12203 | none | free, CC0 | UA policy; SPARQL ~60 s query time/min (R) | CC0 | query logged | n/a | wikidata.org/wiki/Property:P856, 2026-10-18, V |
| crates.io API | homepage, repository, documentation | none | free | ≤ 1 request/s; unique identifying UA; prefer index/dump for bulk | RFC 3463 crates.io policy | logs | n/a | rust-lang.github.io/rfcs/3463-crates-io-policy-update.html, 2026-10-18, V |
| PyPI JSON | `info.project_urls{Homepage, Documentation, Source…}` | none | free | none stated | — | logs | n/a | docs.pypi.org/api/json, 2026-10-18, V |
| npm registry | `homepage`, `repository.url`, `bugs` | none | free | none published | npm terms | logs | n/a | docs.npmjs.com, R |
| NuGet V3 | `projectUrl`, repository | none | free | polite | — | logs | n/a | learn.microsoft.com/nuget/api/overview, R |
| Maven Central / POM | `<url>`, `<scm>` | none | free | polite | — | logs | n/a | central.sonatype.org, R |
| Go proxy | versions; module path = repo | none | free | polite | — | Google logs | n/a | go.dev/ref/mod, R |
| RubyGems | `homepage_uri`, `documentation_uri`, `source_code_uri`, `changelog_uri` | none | free | ~10 rps (R) | — | logs | n/a | guides.rubygems.org/rubygems-org-api, R |
| GitHub REST | `homepage`, README, releases, tags; code search needs auth | none / token | free | **60/h unauthenticated, 5 000/h token**; search endpoints stricter | GitHub API terms | logs; token identifies | n/a | docs.github.com/rest/using-the-rest-api/rate-limits-for-the-rest-api, 2026-10-18, V |
| GitLab API | `web_url`, `readme_url`, files | none (public) | free | ~500/min (R) | — | logs | n/a | docs.gitlab.com/ee/api, R |
| robots.txt / sitemap.xml | owner's page inventory | none | free | — | RFC 9309; honour Disallow for walks | owner sees fetch | — | sitemaps.org, RFC 9309, R |
| llms.txt / llms-full.txt | owner's curated doc index | none | free | — | site terms | owner sees fetch | — | llmstxt.org, 2026-10-18, A |
| Algolia DocSearch | per-site docs index | site's public key | — | Algolia | keys issued to the site's widget; no third-party grant | Algolia + site | — | docsearch.algolia.com, R — **not used** |
| Read the Docs | per-project server-side search | none for search | free | polite | — | logs | — | docs.readthedocs.io, R |
| MDN | web platform docs | none | free | polite | CC BY-SA content | logs | — | developer.mozilla.org, R |
| OpenAlex | works, `referenced_works`, OA locations | none / free key | free | **10 req/s keyless, 30 with key**; `/rate-limit` endpoint | CC0 | — | — | help.openalex.org/how-to-use-the-api/rate-limits-and-authentication, 2026-10-18, A |
| Crossref REST | DOI metadata, open `reference` lists, `resource.primary.URL` | none; polite pool via `mailto` | free | advertised in `X-Rate-Limit-*` headers | public | contact identifies | — | github.com/CrossRef/rest-api-doc (deprecated copy), 2026-10-18, A |
| Semantic Scholar Graph | search, `references`, `citations`, `openAccessPdf` | optional free key | free | shared anonymous pool (429-prone); 1 rps with key | API licence | key identifies | — | semanticscholar.org/product/api, 2026-10-18, A |
| arXiv API | abstracts, DOI, PDF links | none | free, metadata CC0 | **1 request / 3 s** | do not store or serve e-prints | — | — | info.arxiv.org/help/api/tou.html, 2026-10-18, A |
| Unpaywall | OA copies by DOI | email param | free | ~100k/day (R) | — | email identifies | — | unpaywall.org/products/api, R |
| Europe PMC | references, citations, OA full text | none | free | polite | — | logs | — | europepmc.org/RestfulWebService, R |
| PubMed E-utilities | abstracts, links | optional key | free | 3 rps / 10 with key; `tool`+`email` | NCBI policy | identifies | — | NBK25497, R |
| CORE / DOAJ / DBLP / OpenCitations | OA full text / journals / CS bibliography / DOI edges | key / none / none / none | free | — / ~2 rps / polite / polite | — | — | — | R |
| Papers with Code | — | — | **dead (sunset July 2025)** | — | — | — | — | codesota.com/papers-with-code/shutdown, 2026-10-18, A |
| IETF RFC index / Datatracker | RFC metadata, text, obsoletes/updates | none | free | none published | open | — | — | rfc-editor.org, datatracker.ietf.org/api, R |
| W3C API / WHATWG / ECMA / ISO OBP / NIST / OASIS | specs | none | free (ISO full text paid) | NVD 5/30 s keyless | ISO: metadata only | — | — | R |
| Federal Register / legislation.gov.uk / EUR-Lex / data.gov / SEC EDGAR | law, regulations, filings | none (SEC: descriptive UA) | free | SEC ≤ 10 rps | public domain / OGL v3 / EU reuse | UA identifies (SEC) | — | R |
| govinfo / regulations.gov / Congress.gov | US law | free api.data.gov key | free | 1 000-5 000/h | public domain | key = email | — | R |
| CourtListener v4 | opinions, dockets | token | free account | 5/min, 50/h, 125/day documented (tight) | one account per project | account | — | wiki.free.law, 2026-10-18, A |
| Stack Exchange API 2.3 | Q&A with links, scores | none / free key | 300/day per IP; 10 000/day with key | `backoff` seconds must be obeyed; ~30 rps per IP (R) | CC BY-SA attribution | key identifies app | — | apis.io / rollout.com summaries, 2026-10-18, R (api.stackexchange.com unreachable from here) |
| HN Algolia | stories, comments | none | free | ~10 000/h per IP (R) | public | logs | — | hn.algolia.com/api, R |
| Discourse `.json` | forum topics | none (public forums) | free | ~20/min per IP default, 429 + Retry-After | per forum | logs | — | docs.discourse.org, R |
| Reddit | — | OAuth + approval (since 2025-11, R) | paid beyond free | ~100/min | commercial agreement needed | account | — | R — **excluded** |
| Wayback Availability | closest snapshot | none | free | **none documented** | archive.org ToS | URL sent to IA | — | archive.org/help/wayback_api.php, re-read 2026-10-18 by R2, V: `url`, optional `timestamp` (1-14 digits, `YYYYMMDDhhmmss`) and `callback`; one `closest {available, url, timestamp, status}` (`status` a string); `{"archived_snapshots":{}}` when none; no rate, quota or User-Agent rule stated. Live probe 2026-10-18 (declared agent): 200, `closest.status "200"`; the snapshot URL names the original as the crawler recorded it (`http://user:pass@example.com/` for `example.com`) |
| Wayback CDX | capture list with filters/collapse/limit | none | free | **none documented**; live: 429 on the first request (2026-10-18), no `Retry-After`; community ~60/min (Archivarix 2026-02-06), ~15/min (waybackcollector README, undated), IA said on 2026-09-15 it added 429 protections with false positives (kunalganglani.com, secondary): all R, none used to size behaviour | as above | URL sent to IA | — | wayback-cdx-server README (raw, master), re-read 2026-10-18 by R2, V: JSON's first row is the field header; `fl`; `filter=[!]field:regex`; `collapse=field[:N]` (adjacent rows only); `from`/`to` 1-14 digits, inclusive; `limit=N` first N, `-N` last N ("may be slow"), `fastLatest`, `offset`, `showResumeKey`; 150 000-row default cap; page size 50; "Closest Timestamp Match" is in the contents with no section, so `closest=<ts>&sort=closest` was read in `CDXQuery.java`/`CDXServer.java` (master, 2026-10-18, V from source): rows sorted by distance from `closest`, and `closest` with `limit>0` turns `fastLatest` on |
| Wayback `id_` raw read | original bytes | none | free | none documented | as above | as above | — | URL form `web/<ts>id_/<url>` is not on the API page; standard and used by IA's own tools, R |
| Save Page Now 2 | — | S3 keys | — | low caps | publishes the URL | — | — | archive.org/details/spn-api-docs, R — **not used** |
| archive.today | — | none | — | Cloudflare | no API | private operator | — | R — **excluded** |
| Common Crawl index | historical captures (latest seen CC-MAIN-2026-34) | none | free | throttled at times | CC terms | — | — | index.commoncrawl.org, blog.commoncrawl.org, 2026-10-18, A |
| Memento TimeTravel / Software Heritage | aggregated mementos / source archives | none / optional token | free | unreliable / 120-1 200 per h | open | — | — | R |
| GDELT / NewsAPI / Google Fact Check Tools / Media Cloud | news, fact-checks | none / key / Google key / key | free / dev-only / free / research | ~1 per 5 s / 100 per day / — / — | — | — | — | R |

Techniques (R unless noted): RRF — Cormack, Clarke & Büttcher, "Reciprocal rank fusion outperforms Condorcet and individual rank learning methods", SIGIR 2009; URL normalisation — RFC 3986 (2005) §6, RFC 6596 (2012) `rel=canonical`; HyDE — Gao et al. 2022; step-back — Zheng et al. 2023; Self-Ask — Press et al. 2022; query2doc — Wang et al. 2023; snowballing — Wohlin 2014; FEVER — Thorne et al. 2018; SAFE — Wei et al. 2024; ReAct — Yao et al. 2022; WebGPT — Nakano et al. 2021; Self-RAG — Asai et al. 2023; Search-R1 — Jin et al. 2025.

## Appendix C. Code citation index (current tree, `src-tauri/src` = `<desktop>/src`)

`features/web/services/web.rs:78-129, 170-192, 201-229, 249-425` · `web/providers.rs:17-77, 94-131, 133-151, 159-310, 312-353, 355-404` · `web/results.rs:103-107, 151-245, 247-405` · `web/pacing.rs:23-108, 110-160` · `web/page.rs:157-313, 316-363, 365-422` · `web/address_guard.rs:17-175` · `services/page_cache.rs:36-50, 68-70, 145-170, 198-205` · `services/browser_reader.rs:48-56, 219-248, 428, 453-459` · `services/stealth.rs:27-41, 133-144, 215, 241, 260-410` · `services/reference_text.rs` · `features/web/di.rs:30, 149-188` · `features/web/traits/web.rs` · `features/learning/references/web_research.rs:12-57, 61-97, 101-140, 147-184` · `features/learning/lessons/content_verification/research.rs:29-93, 191-300` · `features/learning/planning/outline_references.rs:26, 195, 350` · `features/function_calling/registry.rs:327-412, 439-470` · `features/function_calling/executor/web_tools.rs:22-41` · `features/conversation/chat/tools.rs:4-16, 52-109` · `features/conversation/chat/retrieval/pipeline/web_pages.rs:315-351` · `features/conversation/chat/dto.rs:206-214` · `features/credentials/adapter.rs:32-76` · `<core>/src/application/ports/web_service.rs:15-62` · `<core>/src/application/ports/credentials_port.rs:5-12` · `<core>/src/application/contracts/web.rs:19-56, 78-90` · `<core>/src/application/contracts/settings/retrieval.rs:152, 313` · `<core>/src/application/ports/metrics_port.rs:16` · `scripts/rust-architecture-check/feature-graph.txt:16` · `scripts/check-rust-layer-boundaries.sh` · `evals/grounding/README.md` · `evals/retrieval/EVALUATION_PROTOCOL.md`.

Design and memory inputs: PT §0-3, §8, §12, §15, §17, §18, §22-23 · CR §4, §5 · memory notes `web-fetch-browser-reader-2026-09-23` (FlareSolverr rejected), `kv-cache-prefix-reuse-2026-10-16` (fixed tool list), `no-arbitrary-limits`, `server-agnostic-no-probing`, `document-processes-in-repo`, `agent-fanout-memory-cap`.
