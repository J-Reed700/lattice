# Recall — Product Thesis

**Status:** anchor document
**Paired with:** `AESTHETIC-GUIDE.md` (visual DNA), `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`, `DASHBOARD-FLATTEN-SPEC.md`, `KNOWLEDGE-GRAPH-CONCEPT.md`.
**Purpose:** correct the working thesis the design specs were drafted under (PKM-with-AI), and re-anchor downstream design work to what Recall actually is.

---

## 1. Product soul

**Recall is a private, local-first AI workspace whose character changes with the corpus you load it with: a research assistant for papers, a semantic cookbook for recipes, a review tool for legal documents, a reflection surface for personal writing — same app, same primitives, different soul per vault.** It is not a notes app with a chat sidebar (Notion, Mem) and not a chat app with file uploads (ChatGPT, NotebookLM). Its closest honest comparison is a **personal Glean**: AI retrieval over a corpus you fully own, on your machine, that compounds as you save what matters and write back into it.

Where the working read is correct: domain-morphism is real, and it's the differentiator. Where I'd refine it: the four primitives below are not coequal. **Corpus is the substrate** that makes the app what it is for a given user; **Conversation is the throughput**; **Capture is the ratchet** that converts throughput into permanence; **Synthesis is the compounding interest**. The asymmetry matters for prioritization (§5).

---

## 2. The four primitives

### Corpus
**What it is.** The set of files, web pages, and ingested artifacts that have been vectorized into the user's vault. The corpus *is the personality of the app for that user*. Two installs of Recall with different corpora are different products.

**Where it lives today.** Implicit. `FileBrowser` (`components/FileBrowser/FileBrowser.tsx`) shows the file list; `IngestHub` (`components/IngestHub/IngestHub.tsx`) is the onramp; the `Dashboard` shows file counts in passing. None of these surfaces *show the corpus as a corpus* — i.e., none of them lets the user see what their vault has *become*.

**Where it's still hidden.** The character of the corpus is invisible. A user with 1,200 recipes and a user with 1,200 papers see the same Dashboard and the same FileBrowser. The app does not yet teach you what your vault is.

### Conversation
**What it is.** A grounded query session. The user asks; the system retrieves from the corpus (and optionally web/wiki); the model answers with citations back to the user's own files. The Chat surface is the load-bearing one.

**Where it lives today.** `Chat` (redesigned in `CHAT-REDESIGN-SPEC.md`, shipped). Tri-pane, reading-first, serif assistant prose with citation footnotes, source citations footer. Strong shape. This is the product's most polished surface.

**Where it's still implicit.** The "Query mode" / "Auto" / "Follow-up" turn-mode distinction (CHAT-REDESIGN §4.4) is the closest the UI gets to telling the user "you can use me as a search engine over your files." The deeper truth — *every Chat session is grounded retrieval against your corpus by default* — is undersold. A first-time user with recipes loaded does not learn that "something with harissa and chickpeas under 30 min" is what this thing is for.

### Capture
**What it is.** The deliberate act of marking a moment worth keeping — bookmarking an assistant message into References, snapshotting it into a Journal entry, or pulling a passage from a document into a notebook page.

**Where it lives today.** Two surfaces: `ReferenceInbox` (the commonplace book — `REFERENCE-REDESIGN-SPEC.md`) and `Journal` (the editorial notebook — `JOURNAL-REDESIGN-SPEC.md`). Both redesigned. Both treat capture as a quiet affordance rather than a queue.

**Where it's still implicit.** Capture is currently a *terminal* act in the UI — you bookmark something, it lands in References, end of flow. The fact that captured material *re-enters retrieval* (or should) is not legible. The compound loop (§3) lives in the code but not in the UI.

### Synthesis
**What it is.** The transformation of many captures + many conversations into new prose: a summary across pinned journal entries, a "what did I learn this week" pull, a comparative analysis across cited sources. The synthesize-into-page action in Journal (JOURNAL-REDESIGN §5.5) is the only first-class synthesis affordance in the app.

**Where it lives today.** Buried inside the Journal `EntryActionRail` as a popover with three radio scopes. Strong implementation, weak surfacing.

**Where it's still hidden.** Synthesis is the most distinctive thing the app can do that ChatGPT cannot — *because synthesis runs over your corpus + your captures, not over the model's training data*. It is currently presented as an obscure journal sub-action. It should be a verb the product is built around.

---

## 3. The compound loop

**Ingest → Query → Capture → Re-query → Synthesis → re-Capture → ...**

In prose: a user drops a folder into the vault. Files are extracted and vectorized (Ingest). The user asks a question; Chat retrieves grounded passages and answers with citations (Query). The user hits the bookmark icon on the assistant turn — the moment becomes a Reference, optionally captured into a Journal entry (Capture). Days later, the user asks a related question; the system retrieves not only from the original corpus but also from the user's prior captures and journal prose (Re-query). At some point the user runs Synthesize across pinned entries; the resulting prose lands in a journal page (Synthesis). That synthesis is now itself part of the corpus — re-queryable, re-citable, re-capturable (re-Capture). The vault gets smarter as it gets used.

**Where it breaks today:**

1. **Re-query does not visibly include captures.** The Chat retrieval pipeline does index conversation/journal content (verified via `kb_retrieval` paths in the Rust backend), but the user has no UI signal that a citation in a new chat was sourced from their own past Reference. The compound effect is invisible. **Fix:** Citations from user-authored material (journal pages, captured snippets) should render with a distinct typographic mark — a small `you` or `journal` tag in the source citation row, no color.
2. **Capture is friction-heavy.** Bookmarking a message gives no visible "this is now part of your retrievable knowledge" feedback. The user does the work and gets a row in References — a destination, not a feedback signal. **Fix:** the bookmark gesture should produce a 1-second toast that names the action in product terms: *"Saved — Recall will use this in future answers."*
3. **Synthesis lives in Journal only.** A user in Chat cannot say "synthesize this thread into a journal entry" without leaving the surface. The natural seam — *"this conversation went somewhere; preserve it as prose"* — is unstaffed. **Fix:** add a "Synthesize to Journal" action on a Chat conversation (one-click; pre-fills a synthesize popover with the conversation as the source).
4. **Ingest is one-shot.** Once a folder is added, the user has no easy way to *expand* the corpus from inside a conversation ("I'm researching X, here are 30 more papers I just downloaded"). They have to leave Chat, go to IngestHub, drop files. **Fix:** drag-drop into Chat should pre-stage files for ingest, with a preview row above the composer.

The loop currently runs forward (Ingest → Query) but does not visibly close (Capture → Re-query → Synthesis). The design priority is to make the loop legible.

---

## 4. Domain-morphism as a design principle

If Recall becomes a cookbook with recipes loaded and a research assistant with papers loaded, the design must hold a tension: the surfaces stay generic (you do not build ten modes), but the *substance* the surfaces show must be specific to the corpus that's there.

**Three concrete commitments fall out:**

1. **Vocabulary stays generic.** "Documents," not "recipes" or "papers." "Conversations," not "research sessions" or "meal plans." This is non-negotiable; the moment we add a "Recipes" tab we have built a cookbook app and lost the others.
2. **Surfacing makes the corpus character legible.** Even with generic vocabulary, the *Dashboard* should show the user *what their vault is* — file-type distribution, dominant clusters (auto-named from embeddings), recent ingests. A user with recipes loaded should see their cookbook *in the data*, even though no UI element says "cookbook."
3. **The empty states do the teaching.** A first-time user with recipes loaded should see a Chat empty-state suggestion that reflects what they have: *"Ask something about your 1,247 documents — try 'a quick weeknight dinner with chickpeas'."* The suggestion is generated from the corpus, not from a hardcoded list. This is the cheapest way to make the app feel domain-aware without building modes.

**Restraint is what makes domain-morphism possible.** Editorial neutrality (one accent, serif body, no decorative theming) is not a stylistic preference here — it is a *structural requirement*. A app that styles itself loudly cannot become anything else. A calm, near-flat surface lets recipes look like recipes and papers look like papers because the surface is not asserting a personality of its own.

The AESTHETIC-GUIDE was right by accident. Restating the principle in product terms: **Recall's editorial restraint is the design feature that makes Recall multi-domain.**

---

## 5. Surfaces re-prioritized

Against the corrected thesis, the surfaces rank as follows.

### Tier 1 — These ARE the product (must be world-class)

- **Chat** — the throughput surface. Where the loop happens.
- **Journal** — where capture turns into prose, where synthesis happens.
- **References** — the commonplace book, the bridge from Chat to Journal.
- **FileBrowser** — *the corpus surface*. This is the one we got wrong.

### Tier 2 — Support (must be consistent)

- **Dashboard** — the "what is my vault?" surface. Lower-frequency but high-meaning.
- **IngestHub** — the onramp. Used during corpus growth, not during use.
- **SearchInterface** — the keyword/structured search escape hatch.

### Tier 3 — Plumbing (must work, do not need to shine)

- **Settings, BackupPanel, Downloads, ModelDownload, StorageDashboard, IndexingStatus, FirstRun, WelcomeScreen** — operational chrome.

### Did we prioritize right?

Mostly yes, with one significant miss: **FileBrowser was treated as Tier 3 plumbing (token sweep only), when it should be Tier 1 alongside Chat and Journal.** FileBrowser is *the visible corpus*. Under "PKM with AI bolted on," a file list is incidental — files are inputs to indexing, not a primary surface. Under "domain-morphic substrate," the file list is the most concrete answer to "what is this app for me right now?" and a user spends real time there.

The other prioritization that wobbles: **Dashboard was flattened to a scanning home screen** (`DASHBOARD-FLATTEN-SPEC.md`), which is the right move for the "PKM" framing (calm home page). Under the corrected thesis, Dashboard should *also* be the place where corpus character becomes legible — see §6.

---

## 6. Specific re-frames

**Dashboard.** Currently a quiet home screen with stats, quick actions, recent activity (DASHBOARD-FLATTEN-SPEC.md). Under the corrected thesis it earns one more job: **show the user what their vault has become.** Not a marketing dashboard; a typographic readout. File-type distribution as inline counts (e.g. `1,247 documents · 980 PDF · 210 markdown · 57 text`); a single line naming the largest auto-cluster the embedding space identifies (e.g. `Largest cluster: ~340 documents about recipes`); recent captures count alongside recent ingests. The flattened layout from the existing spec stays; the *content* of the flat sections updates. No new visual treatment — just put the corpus character into the section that currently shows generic stats.

**FileBrowser.** Token-swept only; should be redesigned. This is the corpus surface, and it currently shows itself as a windows-explorer pane with grid/list/tree toggles, a toolbar, breadcrumbs, file icons, and a saved-search rail. Under the corrected thesis it needs: (1) a single, prominent "what is this vault about?" banner — not marketing, just a typographic readout of the corpus character; (2) the ability to *see by cluster* in addition to by folder (an embedding-derived "neighborhoods" view, generic in vocabulary, born from the same restraint as the rest of the system); (3) drop into IngestHub flow inline rather than a separate destination. **Redesign warranted, Phase 5.**

**IngestHub.** Small (151 LOC), well-scoped. Under the corrected thesis it earns moderate priority: this is where a vault becomes itself. The redesign needs: (1) a clear post-ingest signal that names the new corpus character ("Added 47 files. Vault now strongly weighted toward research papers."); (2) inline accessibility from Chat composer (drag-drop a folder into Chat → "Want to add these to your vault first?" → ingest with a callback to resume the conversation). **Redesign warranted, but secondary to FileBrowser.**

**SearchInterface.** Untouched, and probably should stay untouched. Keyword/structured search is the escape hatch for "I know the file exists, just find it." Chat is the primary retrieval surface; SearchInterface is the boolean-and-filename surface for the cases where the user wants directness. **No redesign needed; token sweep at most.** If anything, consider folding it into FileBrowser as a tab — they answer the same question with different precision.

**Chat.** Redesigned (`CHAT-REDESIGN-SPEC.md`). Largely right under the corrected thesis. **Add:** (a) a "Synthesize to Journal" action on the conversation overflow menu; (b) drag-drop pre-ingest in the composer; (c) when a citation source is the user's own captured material (Reference or Journal page), render a small `from your journal` / `from your references` mark inline in the source row. **Do not change** the reading-first / serif-prose / citation-popover spine — that is correct.

**Journal.** Redesigned (`JOURNAL-REDESIGN-SPEC.md`). Largely right. **Add:** the Synthesize popover should accept "this Chat conversation" as a scope, not just journal entries (extends the existing three-scope picker to four). **Do not change** the editorial-prose treatment — that is correct.

**ReferenceInbox.** Redesigned (`REFERENCE-REDESIGN-SPEC.md`). Largely right. **Add:** a single line in the reader that names the compound effect: small `text-xs --text-muted` line above the action rail, e.g. *"Recall uses your saved references in future answers."* This is the only product-promise copy I'd ever advocate adding — and it's because the loop is otherwise invisible.

---

## 7. The corpus-shape question

Josh raised: "not a graph, but show me what's in my vault." This is a real feature, not creep, *if* it is built within the discipline already established by `KNOWLEDGE-GRAPH-CONCEPT.md`. That document is correct that a global force-directed hairball is a failure mode. It is also correct that there are real questions worth answering in the corpus-shape space.

**The right answer is not a graph. It is a typographic corpus readout, in two places:**

1. **On the Dashboard:** a quiet section titled `Vault shape` with three lines of plain prose:
   - File-type distribution as inline counts (e.g. `1,247 documents · 980 PDF · 210 markdown · 57 text`).
   - The 3–5 largest auto-named clusters from the embedding space (the AI generates the labels from the cluster's centroid documents — *"Recipes," "Research on transformer attention," "Project Smith correspondence"*). Inline list, hairline-separated.
   - Recent ingest activity as a one-line summary.

2. **In the FileBrowser:** a `Neighborhoods` view tab alongside `List` / `Grid` / `Tree`. The neighborhoods are the same auto-clusters, rendered as a flat list of clusters with a count and a 5-document preview each. Click to drill into the cluster as if it were a folder. Generic vocabulary throughout — never "recipes," always "documents in this neighborhood."

**What it is not:** a force-directed canvas. Not nodes-and-edges. Not a "knowledge graph view" in the Obsidian sense. The KNOWLEDGE-GRAPH-CONCEPT document already defended this position; the corpus-shape question is its *application to the vault as a whole* rather than to a single document's neighborhood.

**Cost.** The clustering exists already as a derivable from USearch's nearest-neighbors data. Auto-naming requires a small LLM call per cluster (cheap, cacheable). The UI is two flat sections. Modest engineering, high product clarity.

**Honest caveat.** Auto-cluster naming is hard to do well. If the labels are bad, the feature looks dumb. Validate with three real corpora (recipes, papers, mixed personal) before shipping. If labels are unreliable, fall back to "Cluster 1 (340 documents)" with a `Show 5 examples` expansion — the existence of clusters is the value, the labels are the polish.

---

## 8. The three biggest design mistakes we might have made

**1. We treated FileBrowser as plumbing.** Under the PKM framing, the file list is an input to the index — uninteresting. Under the corrected thesis, the file list is the corpus, and the corpus is the soul of the app for that user. We token-swept FileBrowser and moved on. We should have given it the same Tier-1 attention as Chat and Journal. Until FileBrowser becomes a place where the user can *see what their vault is*, the domain-morphic claim is invisible to a first-time user. **Fixable. Phase 5 priority 1.**

**2. We made the compound loop invisible.** Every redesigned surface (Chat, Journal, References) is internally beautiful, but none of them tell the user what the *system* does. A user can bookmark fifty messages and never realize Recall will use those bookmarks in future retrieval. A user can synthesize a journal entry and never realize the synthesis is now searchable. The loop runs in the backend; it is missing from the UI. We were so disciplined about removing decorative chrome that we removed the small product-promise signals that would teach users what the app is. **Fixable. Three small additions across Chat, References, and Journal — see §6.**

**3. We designed for "writers" instead of "people with corpora."** Journal is gorgeous editorial prose. References is a commonplace book. Both lean writerly. But Recall is just as much for *the recipe collector who never writes a journal entry*, *the legal-doc reviewer who only ever reads*, and *the researcher who treats papers as queryable rather than commonplace-able*. The redesigns implicitly assume a user who writes. They are correct that Chat is reading-first, but they are silent on the user who only ever ingests + queries — a fully legitimate Recall user. **Partially fixable.** The fix is not redesigning Journal/References; it's making sure Dashboard, FileBrowser, and Chat hold up *without ever requiring the user to enter Journal or References*. Read-only users must feel served by Tiers 1+2 alone.

---

## 9. What ships next (Phase 5)

In order:

**Phase 5.1 — Make the loop legible (small, immediate).** The three additions from §6 — citation-source marking when sources are user-authored, post-bookmark product-promise toast, "Synthesize to Journal" from a Chat conversation, "this Chat conversation" as a Synthesize scope inside Journal, and the single product-promise line in the Reference reader. None of these require new components; all are inline copy + small wiring. Expected: 2–3 days. Ships the most product-meaning per line of code on the roadmap.

**Phase 5.2 — Redesign FileBrowser as the corpus surface.** Full spec, sibling to CHAT/JOURNAL/REFERENCE. Two-pane shell to match the family. Add the `Neighborhoods` view (§7). Add the inline "Vault shape" header (§7). Token sweep + decomposition (the file is 1,009 lines and likely soup). Expected: 1.5–2 weeks of design + implementation. **This is the surface that makes the corrected thesis visible.**

**Phase 5.3 — Add the corpus readout to Dashboard.** Quiet, in the existing flattened layout. The `Vault shape` section per §7. Auto-cluster naming behind a backend command. Expected: 3–5 days, mostly backend cluster-labeling work.

**Phase 5.4 — IngestHub light-touch redesign.** Post-ingest naming, drag-drop-from-Chat affordance. Expected: 1 week. Lower priority because it is the lowest-frequency surface.

**Phase 5.5 — SearchInterface review.** Decide: fold into FileBrowser as a tab, or keep separate with a token sweep. No redesign budget. Expected: 1–2 days.

**What does not ship in Phase 5:** a global graph view (will not ship at all, per KNOWLEDGE-GRAPH-CONCEPT). A per-document seeded graph (deferred — wait until users ask). Tags. Full-text search across captures (backend work, not a design phase). Multi-vault support.

The single sentence: **Phase 5 makes the corpus visible (FileBrowser + Dashboard) and the loop legible (small inline product-promise additions across Chat / Journal / References).** When Phase 5 ships, a first-time user with recipes loaded will see — without anyone telling them — that Recall has become their cookbook.

---

## 10. Summary in one paragraph

Recall is not a PKM app. It is a domain-morphic AI workspace whose character is set by the corpus a user loads, and whose value compounds as the user captures and synthesizes against that corpus. The four primitives are Corpus (the substrate), Conversation (the throughput), Capture (the ratchet), Synthesis (the compounding interest). The Chat / Journal / Reference redesigns are correct in their own right but designed in isolation; they need three small inline additions to make the compound loop visible to the user. The biggest unbuilt surface is FileBrowser, which under the corrected thesis is Tier 1 — the *visible corpus* — and was treated as plumbing. Phase 5 ships the loop signals first (cheap, high product-meaning), then a full FileBrowser redesign (the corpus surface), then a Dashboard corpus-shape readout. Editorial restraint stays — and is now defended structurally, because it is the only design language that lets one app become a cookbook, a research assistant, a legal reviewer, and a journal without rewriting itself.
