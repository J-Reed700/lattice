# Dashboard Redesign — Implementation Specification

**Status:** proposed
**Sibling pilots:** `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`. This spec inherits their design language; differences are stated explicitly.
**Supersedes:** `DASHBOARD-FLATTEN-SPEC.md` — the flatten was the right surgery for the wrong product. The aesthetic discipline survives; the framing of "scanning surface for vanity metrics" does not.
**Paired with:** `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, `PRODUCT-THESIS.md`.
**Scope:** `src/app/websrc/components/Dashboard/**` only. No backend, no API, no data-model changes. The existing `useDashboardQuery` hook is reused; we add at most two derived selectors.
**Route:** `/` (canonical home; no change).

---

## 1. Current state

`Dashboard.tsx` is one 423-line file freshly flattened against the previous bento-card / gradient-headline iteration. It now renders:

- A serif `Dashboard` title + today's date (`Dashboard.tsx:185-192`).
- A four-column "stats" strip: Indexed / Storage / Searches today / Last indexed (`195-208`).
- A `Continue` section listing up to three recent chat conversations (`211-248`).
- A `Today` section showing the journal entry for today, or a `Start today's entry` row (`251-300`).
- A `Recent references` section listing the last five bookmarks (`303-355`).
- A `Recently indexed` section listing the last five documents (`358-386`).
- A `Quick actions` section with five rows: New conversation / New journal entry / Add files / Add web page / Search (`389-417`).

It is clean, tokenized, and obeys the aesthetic. It is also generic — the kind of dashboard a project-management tool might ship. **Three of the four "stats" do not move under load** (`Storage` is hardcoded to `0`, `Searches today` is hardcoded to `0`, `Last indexed` is hardcoded to `'Never'` — see `useDashboardQuery.ts:172-177`). The header section visually weights vanity metrics that are, today, vanity zeros.

The `Quick actions` section duplicates affordances that already exist in the left navigation rail. Five vertical rows for "New conversation / Add files / Search" is screen real estate paying for nothing the user hasn't already learned.

The implementation is honest. The framing is not.

---

## 2. Product soul of Dashboard

Dashboard is the **corpus-state briefing**. The user opens Recall and the home screen tells them, in one read, what their substrate looks like and what's happening to it. Not "welcome back" — a status report on the thing they're operating on.

Per the product thesis: Recall is domain-morphic. The Dashboard is the most direct test of "any sentence is the palm of your query" — it's where the user first sees what Recall has *become* in their hands. A cookbook user should land here and feel: a recipe collection, growing. A research user should land here and feel: a paper library, recently fed. A legal user should land here and feel: a case file, indexed.

The surface answers four questions, in order:

1. **What is in my vault?** (Shape and scale, not a number alone.)
2. **What is happening to it right now?** (Active ingest, queued imports, recent additions.)
3. **Where did I leave off?** (The compound loop: last conversation, last journal entry, last reference saved.)
4. **What's the next obvious move?** (One forward affordance, contextual to state — not five generic ones.)

This is not a "home screen." Home screens decorate. This is a briefing — the thing a power user reads each morning to decide whether to open Chat or Journal first.

Per AESTHETIC-GUIDE §1, "the interface is a lens onto the user's own material." On Dashboard, the user's material is *the corpus itself* — its shape, its motion, its recent activity. The composition should look like a careful reading of that material, not a celebration of having one.

---

## 3. Layout

### 3.1 Single-column reading column, wider than siblings

Centered single column at **`max-width: 920px`**. Wider than Chat/Journal/References (`760px`) because Dashboard is a *briefing*, not a *reading column* — multiple parallel strips of information sit in horizontal sub-grids inside the column, and 760px crowds them. Still narrow enough to read top-to-bottom in one scan; never edge-to-edge.

Padding: `32px` horizontal / `48px` top / `64px` bottom. Vertical section spacing: **`56px`** (one step larger than the flatten spec's 48px because each section is a self-contained briefing block, and the rhythm between them needs to feel like deliberate paragraph breaks, not list rows).

### 3.2 Why a single column, not a tri-pane

The siblings use a `280px` sidebar + main pane because each surface has a navigable list (conversations, entries, references). Dashboard is *the landing page*; there is nothing to navigate within Dashboard itself. The left nav (Layout) already provides global navigation. A Dashboard-internal sidebar would either duplicate the global rail or invent navigation that doesn't exist (the user doesn't pick "which dashboard" — there's one).

So Dashboard is full-canvas. The global nav is on its left; everything else is the briefing.

### 3.3 No header chrome

The current `Dashboard` H1 + date are the closest thing to a page header. Keep them, but treat them as the **briefing's lede** — the first paragraph of the report, not a dashboard title. The word "Dashboard" itself is removed (it tells the user where they are; the URL and nav already do that).

---

## 4. The four briefing blocks

Top to bottom, in a single column:

### 4.1 Lede — Today, in your vault

The first thing the user sees. One serif sentence + a date sub-line. Replaces the current `Dashboard` H1.

**Layout:**

- Date sub-line at top: `--font-sans`, `text-xs`, uppercase, tracking +loose, weight 500, `--text-muted`. Format: `FRIDAY · APRIL 17`. Bottom margin: `12px`.
- Lede sentence: `--font-serif`, `text-3xl` (30px), weight 600, tracking `-0.02em`, `--text-primary`. **One sentence**, dynamically composed from corpus state. Examples (template-driven, no per-corpus variants — see §6):
  - First-run: `Your vault is empty. Start by adding the first folder.`
  - Steady-state: `1,247 documents in your vault. 4 added this week.`
  - Active ingest: `1,247 documents — and 18 more landing now.`
  - Stale: `1,247 documents. Nothing new in 23 days.`
  - Bottom margin: `40px`.

The lede earns the only `text-3xl` on the page. It is the briefing's headline. It changes daily — that's the point. It does not say "Welcome" or "Hello." Recall is an instrument; instruments don't greet you.

**No icon, no avatar, no gradient.** Single typographic element with one supporting label.

### 4.2 Block one — Corpus shape

Answers "what's in my vault?" Not by a count alone but by a small visual.

**Section heading:** `Your corpus`, `--font-sans`, `text-sm`, weight 500, `--text-secondary`, `8px` bottom-padding, hairline `--border-subtle` underneath.

**Body — three elements in a horizontal sub-grid (12px col-gap, 20px row-gap, wraps below 720px):**

1. **Type breakdown bar.** A single horizontal bar, `8px` tall, full sub-grid width on its own row, divided into proportional segments by document category. Categories derive from existing `DocumentMetadata.category` and `fileType` (no backend change). Five buckets max — anything beyond is grouped as `Other`. Each segment is `--text-tertiary` at varying tonal opacity: the largest segment at 100%, descending. **No accent color, no rainbow**. The bar is monochromatic; the *proportions* carry the signal. Tooltip on hover (Radix `Tooltip`) shows `{category} · {count} documents · {percentage}%`.

   Below the bar, an inline legend in `--font-sans`, `text-xs`, `--text-tertiary`, interpunct-separated: `PDFs 612 · Web articles 398 · Notes 142 · Code 78 · Other 17`. The legend is the chart's accessibility layer — anyone can read it without seeing the bar.

2. **Three counter pairs**, sub-grid, three columns of equal width, gap `48px`:
   - `1,247 documents` · sub-label: `across 38 folders`
   - `4 ingested this week` · sub-label: `last on Wed`
   - `38 days of history` · sub-label: `oldest doc Dec 2025`

   Counter style: value at `--font-sans`, `text-xl`, weight 600, tabular-nums, `--text-primary`. Sub-label at `text-xs`, `--text-tertiary`, `4px` top margin. **No icons.** No tinted backgrounds. The counters are typography, not cards.

The block reads as *the corpus introducing itself* — proportions, scale, age. A cookbook user sees their ratio of recipes to web clippings. A researcher sees their ratio of PDFs to notes. The structure is identical; the values change. (See §6 for the domain-morphic argument.)

### 4.3 Block two — What's happening now

Answers "what is happening to my corpus right now?" The compound loop has an *intake* — this block surfaces it.

**Section heading:** `Activity`, same treatment as §4.2.

**Body:**

- **If indexing is in progress** (existing `IndexProgress.status === 'processing' | 'scanning'`): a single live status row at the top, full-width.
  - Left: `--font-serif` italic at `text-base`, `--text-secondary`: `Indexing 18 of 47 files…`
  - Right: tabular-nums percentage `38%` at `text-sm`, `--text-tertiary`.
  - Below: a `2px` thin progress bar in `--accent`, full-width, `4px` top-margin. No glow, no shimmer. The single legitimate use of `--accent` on this surface.
  - Below progress, the current file path truncated at left, `text-xs`, `--font-mono`, `--text-muted`.

- **If no active ingest:** a single line, no bar:
  - `--font-sans`, `text-sm`, `--text-tertiary`: `Index is idle. Last activity 4h ago.`

- **Below the live row, "Recent activity":** a flat list (no border, hairline `--border-subtle` between rows) of up to **5 most-recent items across the loop, mixed by timestamp**:
  - Document indexed: `+ {fileName} · {category}` (icon: small `Plus`, 12px, `--text-muted`)
  - Conversation started: `Asked: "{firstUserMessage truncated to 80c}"` (icon: `MessageSquare`, 12px, `--text-muted`)
  - Reference saved: `Saved: "{title or preview truncated to 80c}"` (icon: `Bookmark`, 12px, `--text-muted`)
  - Journal entry written: `Wrote: "{entry title or first sentence}"` (icon: `NotebookPen`, 12px, `--text-muted`)

  Row layout: 12px icon (left, `--text-muted`) · `text-sm` `--text-primary` content · right-aligned `text-xs` `--text-muted` relative timestamp. `12px` vertical padding. Hover: `--surface` background. Click navigates to the appropriate surface (document → file viewer; conversation → `/chat?conversationId=…`; reference → `/references?referenceId=…`; journal → `/journals?journalSpaceId=…&entryId=…`).

This list is the **compound loop made visible** — the user sees their own activity weaving across surfaces. The mixed timeline is the point: in the new thesis, Chat / Journal / References are interleaved, not separate workflows.

### 4.4 Block three — Where you left off

Answers "where do I pick up?" — three specific entry points, each a substantial row, **only rendered if state warrants it**.

**Section heading:** `Pick up`, same treatment as §4.2.

**Body — up to three rows, each ~64px:**

1. **Last conversation** (if exists, `recentConversations[0]`): row layout matches the current `Continue` row at `Dashboard.tsx:217-244` but restyled per sibling pattern:
   - Left: `MessageSquare` icon, 14px, `--text-tertiary`.
   - Title: `--font-sans`, `text-sm`, weight 500, `--text-primary`, line-clamp-1.
   - Sub-line: `text-xs`, `--text-tertiary`, line-clamp-1, `bookmark.lastMessagePreview` truncated to ~120c.
   - Right: relative time, `text-xs`, `--text-muted`, tabular-nums.
   - Hover: `--surface` background, `--radius-sm`.
   - Click: navigates to `/chat?conversationId=…`.

2. **Today's journal entry** (if exists) or **Pick up your journal** (if a journal exists but no entry today): same row pattern, `NotebookPen` icon. If today entry exists, sub-line shows entry preview. If only journal exists, sub-line shows `Last entry {N} days ago`. If no journal: row is omitted.

3. **Latest reference** (if exists, `recentReferences[0]`): same row pattern, `Bookmark` icon. Sub-line shows the reference's `title || messagePreview` truncated. Right: origin tag (`CHAT` / `JOURNAL`) at `text-xxs` uppercase tracking +loose `--text-muted`, then time.

If all three are absent (true first-run), this entire block is omitted — the lede (§4.1) and `Block four` (§4.5) carry the user.

The current `Recent references` and `Recently indexed` sections are **deleted as separate sections** — references are subsumed into "Pick up" (the most recent one) and "Activity" (saves appear in the mixed timeline). Recently indexed documents appear in "Activity" (each ingest is one row).

### 4.5 Block four — One forward move

The single CTA. Replaces the five `Quick actions`.

**Layout:** one full-width row, `72px` tall, hairline `--border-subtle` top + bottom (no card frame). Inside the row:

- Left: `--font-serif`, `text-lg`, weight 500, `--text-primary`. The verb that matters most given current state. State machine:
  - Empty corpus: `Add your first folder.`
  - Corpus exists, no recent conversation: `Ask your first question.`
  - Active conversation last touched <24h: `Pick up your conversation.`
  - All caught up, recent activity: `What's on your mind today?`
  - Stale (no activity 7+ days): `Add something new to think about.`
- Right: a single `Button` (shadcn primary variant, `--accent` fill, `--accent-fg` text) with the matching action: `Add folder` / `New conversation` / `Open conversation` / `New conversation` / `Add files`.
- Click: navigates to the appropriate surface or opens the appropriate flow.

There is **one** forward move on the Dashboard at any given time. Picking it is the briefing's job; the user shouldn't have to.

The current `Quick actions` 5-row grid is deleted. Those affordances live in the left nav (`/chat`, `/journals`, `/ingest`, `/search`) where they belong as global navigation. A dashboard that lists global navigation as content has confused itself with a sitemap.

---

## 5. Information density

Substantive but scanning-friendly.

- **Words on screen at steady-state:** ~180-220, including the lede sentence, counter labels, activity rows, pick-up sub-lines, and the forward CTA. Roughly 3× the current Dashboard's ~70 words after the flatten — *but distributed across earned blocks, not crammed into a single dense panel*.
- **Vertical extent at 1080p, no scroll:** lede + corpus shape + activity (5 rows) fit above the fold. Pick-up + forward CTA fit below the fold by design — they reward a small scroll, which signals to the user that there is *more here than the headline*.
- **No empty cards.** If a block has no data (e.g., activity list when corpus is fresh and nothing has happened), the block renders a single `--text-tertiary` `text-sm` line: `No activity yet — start with a conversation or add a file.` and nothing else. Empty blocks teach the model.
- **No skeleton cascade.** Per AESTHETIC-GUIDE §3, content appears; it does not perform an entrance. Use the existing `DashboardSkeleton` flat-block treatment unchanged but add no animation.

The density target is "morning-paper briefing." Substantial, scannable in 15 seconds, rewards a 60-second read with full corpus context. Not sparse, not crowded.

---

## 6. Domain-morphic handling

The single most important question: how does Dashboard reveal "what your corpus is" without hardcoding per-corpus variants?

**Answer: derive everything from `DocumentMetadata.category` and `DocumentMetadata.fileType`, both of which already exist.** No new schemas, no per-domain config.

The dashboard's domain-morphic feel emerges from:

1. **The type breakdown bar (§4.2.1)** — visible distribution of categories. A cookbook vault is dominated by `web_article` + `pdf`. A research vault is dominated by `pdf` + `text` (notes). A legal vault is dominated by `pdf`. The user's eye reads the proportion as identity — *without* the UI ever labeling them "cookbook" or "research."

2. **The lede sentence (§4.1)** — phrased in terms of *documents*, not "recipes" or "papers" or "cases." The word "documents" is generically true; the *number* and the *recency* are what make it personal. `1,247 documents in your vault` reads differently for the user with 1,247 papers vs. 1,247 recipes vs. 1,247 statutes — even though the sentence is identical.

3. **The activity timeline (§4.3)** — surfaces actual filenames and conversation titles. `+ Pad-thai-final.pdf · Recipe` is unambiguously cookbook. `Asked: "How does GDPR apply to..."` is unambiguously legal. The user's content does the categorizing; we don't.

4. **The forward CTA (§4.5)** — uses generic verbs (`Ask`, `Pick up`, `Add`). Generic in language; specific in action because the navigation target is the user's actual conversation / journal / corpus.

**What we explicitly do NOT do:**

- No "domain detection." We don't try to classify the vault as "cookbook mode" or "research mode." Classification is brittle (mixed corpora are common — the same vault holds recipes *and* receipts) and creates the sin of telling the user what their corpus is, when the corpus's job is to tell them.
- No domain-specific terminology or icons. `documents` always means documents. `Add folder` always means add a folder.
- No corpus-shape "personality" beyond the proportions and the activity. The chart is monochromatic; it doesn't try to feel "warm" for cookbooks or "cold" for legal.

The aesthetic stays neutral; the *content* is what's domain-morphic. This is the only way the principle scales: any new corpus type works on day zero, no UI change needed.

---

## 7. Motion

Same vocabulary as Chat/Journal/References. Specifically:

**Animates:**

- Live ingest progress bar fill: `width` transition `--duration-base` `--ease-out`. No shimmer.
- Activity timeline new-row appearance (when the dashboard polls and a new event comes in): `opacity 0 → 1` over `--duration-fast`. No `translateY` rise. No staggered cascade — single-element fade per arriving row.
- Hover on Pick-up rows / Activity rows: instant `--surface` background, no transform.
- Tooltip on type-breakdown bar segments: Radix default `--duration-fast` fade.

**Does NOT animate:**

- Page mount. No staggered fadeInUp. No entrance cascade.
- Lede sentence change (when corpus state shifts): instant.
- Counter values updating on poll: instant. No tick-up animation. (Tick-ups are performative — AESTHETIC-GUIDE §6.)
- Forward CTA hover: color state change only.
- Type-breakdown bar segment proportions changing on data update: instant.

The Dashboard is a briefing surface. Briefings don't perform; they report.

---

## 8. Component ownership

### Decomposition strategy: extract while redesigning

The current `Dashboard.tsx` is 423 lines but cleanly written. Restyle in place is feasible but the natural decomposition matches the four briefing blocks, so we extract them as we go. Each leaf is `<200` LOC.

### Files to create (in `components/Dashboard/`)

| File | Responsibility | Approx LOC |
|---|---|---|
| `Dashboard.tsx` | Shell. Reads `useDashboardQuery`, derives the lede string and CTA state, renders the four blocks in order. No JSX beyond the column shell. | 120 |
| `DashboardLede.tsx` | The serif lede sentence + date sub-line. Pure presentation; takes `lede: string` and `dateLabel: string`. | 50 |
| `CorpusShape.tsx` | Block one: type-breakdown bar, legend, three counter pairs. Pulls category + fileType buckets from documents data. Computes proportions. | 180 |
| `CorpusActivity.tsx` | Block two: live ingest row (when active) + recent activity timeline. Composes the mixed timeline from documents, conversations, references, journal entries. | 200 |
| `CorpusPickUp.tsx` | Block three: up to three rows for last conversation / today's journal / latest reference. Each row is a small inline component. | 140 |
| `ForwardMove.tsx` | Block four: state-driven verb + button. Pure function of corpus state. | 80 |
| `useCorpusBriefing.ts` | Hook that composes the dashboard's derived state: lede sentence, CTA state, type breakdown buckets, mixed activity timeline. Pulls from `useDashboardQuery`'s data plus `getIndexProgress` for live status. | 220 |
| `useIndexProgressPolling.ts` | Small hook that wraps `VaultAPI.getIndexProgress` polling at 2s when active, 30s when idle. Returns `{ isActive, percentage, currentFile }`. | 50 |

### Files to modify

| File | Change |
|---|---|
| `hooks/queries/useDashboardQuery.ts` | Extend the returned `DashboardData` to include: (a) per-category buckets from `documents` (the existing `listAllDocuments(10000)` already provides them — just compute and return); (b) the latest 5 entries across documents/conversations/references/journals as a `mixedActivity` array; (c) corpus age (oldest indexedAt). Backward-compatible; consumers that don't read the new fields keep working. **Delete** the hardcoded `storageUsed: 0`, `searchCount: 0`, `lastIndexed: 'Never'` — they were vanity zeros. |
| `components/Dashboard/DashboardSkeleton.tsx` | Update to match the new four-block layout — flat blocks at the lede / corpus shape / activity / pick-up positions. No shimmer. |
| `components/Dashboard/DashboardError.tsx` | No change needed — already token-clean. |

### Files to delete

- `components/Dashboard/DashboardHeader.tsx`, `DashboardContent.tsx`, `DashboardStats.tsx`, `DashboardEmpty.tsx`, `QuickActions.tsx`, `RecentActivity.tsx`, `RecentDocuments.tsx` — confirmed unreferenced (per `DASHBOARD-FLATTEN-SPEC.md` §1). Final cleanup of dead bento-card code.
- `components/StatCard/`, `components/BentoCard/` — confirm dead per the flatten spec; delete after this redesign lands and verifies no other consumer exists.

### shadcn primitives used

- `Tooltip` — type-breakdown bar segment hover.
- `Button` — forward CTA.
- All present in codebase. No new primitive additions.

### Cross-surface reuse

- Activity-row layout pattern is similar to (but not identical to) the sibling sidebar conversation/entry rows — keep local; the dashboard rows have a mixed-icon convention that doesn't generalize.

---

## 9. Scope boundaries

This spec is **NOT**:

- Adding any new backend API. The category breakdown is computed client-side from `listAllDocuments` (already returns category + fileType per document). Corpus age is computed from `indexedAt`. Mixed activity timeline is computed from existing query results.
- Adding "domain detection" or per-corpus styling.
- Adding a knowledge-graph visualization (separate concept — see `KNOWLEDGE-GRAPH-CONCEPT.md`). The type-breakdown bar is the only visualization on Dashboard.
- Polling more frequently than 2s during active ingest, 30s otherwise. Existing `useDashboardQuery` 30s `refetchInterval` is unchanged for non-ingest data.
- Adding analytics tracking ("searches today") — that data is not collected today, and surfacing fake numbers was the sin of the prior version.
- Adding a "Today" calendar widget. Date appears once, in the lede sub-line; that's enough.
- Touching the global Layout / left-nav rail. The Dashboard is content within Layout, not a Layout change.
- Implementing the storage-used metric. If we want it, it's a backend addition (compute total bytes across `documents`); spec'd out of this pass.
- Wiring the type-breakdown bar to filter the FileBrowser when clicked. Cross-surface filtering is a reasonable follow-up; this pass keeps the bar as a read-only signal.

---

## 10. Risks

1. **The lede sentence engine.** The state machine in `useCorpusBriefing` (empty / steady / active / stale) is straightforward but the *phrasing* matters — these are the user's first words from the app each morning. Three or four passes from voice review, expected. Default phrasings in §4.1 are a starting point, not final.

2. **Type breakdown bar with sparse categories.** When the vault has ≤3 categories, the bar has ≤3 segments, which can read as deliberately empty. Decision: the bar always renders (its presence teaches the visual). When ≤2 categories, the legend below it does the work. Rendering only when 3+ categories would create surface inconsistency.

3. **Mixed activity timeline ordering.** Sorting four heterogeneous source-types by timestamp requires that each have a comparable timestamp. Documents have `indexedAt`. Conversations have `updatedAt`. References have `createdAt`. Journal entries have `updatedAt`. All present in current data; verify that the resulting interleaving reads sensibly during implementation. If a particular source dominates (e.g., 12 doc indexings in the last hour drown out the single conversation from earlier), consider per-source caps (max 3 per source type) before falling back to pure timestamp sort.

4. **Live ingest progress relies on `getIndexProgress`.** Verified it exists (`api.ts:990`). The polling hook is straightforward; verify it doesn't conflict with the existing `useIndexing` hook's listeners during implementation.

5. **Forward CTA state machine.** The "what to suggest next" logic is opinionated. If the rules in §4.5 don't match a user's intuition (e.g., they have a stale conversation and a fresh journal — which to surface?), revise. Default priority order: empty corpus → fresh ingest needed → recent conversation to resume → fresh question prompt → stale-corpus prompt.

6. **Dashboard width vs sibling surfaces.** Dashboard is `920px`; siblings are `760px`. The dashboard CTA row, when scrolled past the sibling-width zone, may feel visually disconnected from the rest of the app. Mitigation: the column is centered; the surrounding canvas is `--bg`; the visual register stays consistent. If review finds it disorienting, narrow to `860px`.

---

## 11. Tokens and patterns needed beyond TOKENS-SPEC

None required. TOKENS-SPEC is sufficient.

Patterns inherited from sibling specs:

- Activity timeline row hover = `--surface` background fill (Chat sidebar pattern, `CHAT-REDESIGN §5.2`).
- Section heading style (`text-sm` weight 500 `--text-secondary` + hairline) — local to Dashboard, distinct from sibling surfaces' sidebar headings (which are `text-xxs` uppercase). Section headings here are bigger because they sit above heterogeneous blocks, not lists.
- Counter style (value at `text-xl` weight 600 tabular-nums + sub-label) — original to this spec; if it proves useful, extract.
- Forward CTA row pattern (`72px` row, hairline top + bottom, serif text + button) — original to this spec.

---

## 12. Questions for Josh

1. **Lede sentence — generic or domain-aware?** The spec keeps it generic (`documents`, never `recipes`/`papers`). If you want it to *attempt* domain awareness (`1,247 recipes` when `category` is dominantly `recipe`), it's an additive change but it crosses the line into "dashboard tells you what your corpus is." Default: generic. Confirm.

2. **Forward CTA priority order.** Spec'd as: empty → ingest → resume conversation → fresh prompt → stale prompt. Alternative orderings (e.g., always prefer journal continuation when one exists) are equally defensible. This is a "what does Josh want to do most often when opening Recall?" call.

3. **Type breakdown bar — replace with sparkline-style activity over time?** The bar shows *composition*; a sparkline would show *growth*. Both are domain-morphic signals. Spec'd as composition because composition is the more identity-revealing signal; growth-over-time is a follow-up. Confirm.
