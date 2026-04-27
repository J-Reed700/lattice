# Knowledge Graph — Map of Unknowns

**Status**: Pre-architecture. Do not lock design until the section-A questions marked "BLOCKING" have an answer. This is a risks-and-questions doc, not a spec.

**Method note**: Every item below is either (a) something the code can't tell you, or (b) something the code *can* tell you but hasn't been checked. Items labeled `[verified]` were confirmed by inspection during this pass.

---

## A. Product unknowns (only Josh can answer)

### A1. What is the graph *for*? **[BLOCKING]**
The four plausible jobs-to-be-done produce materially different products:
- **Exploration** — "wander my past thinking." Optimizes for serendipity, dense view, low-friction pan/zoom.
- **Retrieval** — "find the thing near X." Optimizes for search-from-node, focused subgraph, fast click-through.
- **Contemplation** — "see the shape of my mind." Optimizes for the long session, quiet aesthetic, minimal moving parts.
- **Audit** — "what have I written about and what's isolated?" Optimizes for orphan detection and cluster summaries.
- **Who**: Josh. **Reversibility**: **one-way door for architecture** — exploration wants an always-loaded global graph; retrieval wants ephemeral per-query subgraphs; they have different backends. **Default if no answer**: Retrieval (cheapest, most legible, composes with existing Chat/Search).

### A2. Does Josh actually write wikilinks? **[BLOCKING]**
The whole `mentions` / `document_mentions` substrate assumes `[[wikilinks]]` and `@[person]` are present in content. If Josh's corpus is mostly PDFs, web captures, and flowing prose with no explicit links, the graph has ~zero edges at start and semantic edges become the *only* signal — which is a very different product (and a harder one to make trustworthy).
- **Who**: Josh (self-report) + code investigation (count rows in `document_mentions` against current vault).
- **Reversibility**: **one-way-ish** — commits to "edges = semantic" vs. "edges = authored" at the level of which edges are even *visible* to the user.
- **Default**: Mixed, with authored edges visually distinct (solid) from semantic edges (dotted, weaker).

### A3. Scope of entities: documents only, or documents + conversations + references? **[BLOCKING]**
- Frontend has Chat, Journal, ReferenceInbox as first-class surfaces. If the graph only shows `documents`, journals and references may or may not be documents depending on how they're stored (worth verifying — journals likely are, references maybe are via `source_type='web'` `[verified]`). Conversations are *not* documents; they're a separate table `[verified]`.
- **Who**: Josh.
- **Reversibility**: Medium — adding a second entity type later is possible but node rendering, color semantics, and filter UI all bake the assumption in.
- **Default**: Documents only in v1. Conversations as a filter toggle in v2.

### A4. One graph, or graph-of-X?
- Global "all my knowledge" single canvas, vs. contextual "graph rooted at this document / this conversation / this query."
- These are very different UIs. Obsidian ships both; starting with local-graph is cheaper and feels better on small corpora.
- **Who**: Josh. **Reversibility**: Medium. **Default**: Local-graph first (rooted at current document or query), global as a later surface.

### A5. Persistent layout or recomputed?
- Saved node positions imply: a node-position table, migration on node add/remove, conflict resolution on re-layout, "reset layout" button.
- Recomputed implies: identical graph re-layouts every open (deterministic seed needed) and the user can never "arrange their mind."
- **Who**: Josh. **Reversibility**: Medium (adding persistence later is straightforward; removing it feels like loss). **Default**: Recomputed, deterministic seed. Don't ship layout persistence until asked for it twice.

### A6. Privacy / portability expectations
- Is the graph image exportable (PNG/SVG)? JSON-exportable? Screenshot-shareable without leaking node labels? The app is explicitly local-first; the aesthetic guide emphasizes "never performs" — a shareable graph contradicts that tone.
- **Who**: Josh. **Reversibility**: Easy. **Default**: No export in v1; local-only; labels hideable via keyboard for screen-recording.

### A7. Is this a daily surface or an occasional one?
Affects performance budget and polish level. An occasional surface can tolerate 1s load; a daily one cannot tolerate 300ms. Affects whether it earns sidebar real estate.
- **Who**: Josh. **Reversibility**: Easy. **Default**: Occasional; accessed via command palette, not sidebar.

---

## B. Technical unknowns (need investigation, no preference needed)

### B1. **Schema mismatch between domain and DB.** `[verified — real risk]`
The `mentions` SQL table constrains `type IN ('person', 'concept', 'wikilink')` (`migrations/20250101000000_init_schema.sql:115`), but the domain `MentionType` enum is `Person / Organization / Location / General` (`features/mentions/entity.rs:18-28`). The repository writes `'person'` and `'wikilink'` literals directly (`features/mentions/repository.rs:65,82`), bypassing the domain enum. So the domain entity is effectively dead code for extraction, and any graph relying on `MentionType` variants will not see what's in the DB. **Must be reconciled before graph architecture is locked.**
- **Who**: Code fix (one-line migration or domain-layer rewrite). **Reversibility**: Easy now, hard once the graph depends on it.

### B2. **Are there actually edges, or just a junction table?** `[verified]`
There is **no relationship/edge table**. `document_mentions` is (document → mention-name), not (document → document). Doc-to-doc edges must be derived at query time via `document_mentions` self-join on `mention_id`. No wikilink-to-resolved-document-id persistence — resolution happens in `ExtractAndResolveLinksUseCase` but the resolved ID is not stored (`features/extraction/use_cases/extract_and_resolve_links.rs:141-158`). **Edges are ephemeral and re-resolved every query.** This is an architectural decision disguised as an implementation detail.
- **Who**: Code fix if we want persisted edges. **Reversibility**: Medium — adding an edges table is non-destructive but changes the indexing pipeline.

### B3. **How many documents does the typical Recall corpus contain?**
No telemetry was found in the searched surface — `document_count`, `analytics`, `telemetry` hits are all about unrelated features. Perf budget for graph is unknown. 100 docs → trivial; 10k docs → a hairball problem.
- **Who**: Ask Josh for his own corpus size as the initial benchmark; the only user that matters for v1. **Reversibility**: N/A.

### B4. **Performance of `getBacklinksForMention` at scale?**
Current query: join `mentions` + `document_mentions`, indexed on `(document_id, mention_id)` `[verified]`. Probably fine to 10k. Not verified by benchmark. If the graph renders all backlinks for the current node on hover, this query fires constantly.
- **Who**: Benchmark with seeded data. **Reversibility**: Easy to fix with a projected edges table later.

### B5. **Does the embedding store expose k-NN via a command?** `[verified — yes]`
`semantic_search` and `find_similar` are Tauri commands (`features/search/commands.rs:664, 942`). USearch is the backend per MEMORY.md. So semantic edges are available cheaply at query time. Good news.
- **Who**: Already answered. No action.

### B6. **What's the chunk-vs-document granularity for semantic edges?**
Embeddings are per-chunk (`text_embeddings.chunk_id`) `[verified]`, not per-document. A "semantic edge" from doc A to doc B is actually "some chunk in A is near some chunk in B." Aggregation policy matters: max-similarity? mean? top-k chunks summed? Different choices produce different graphs and different trust levels.
- **Who**: Design decision, but requires understanding the embedding distribution first. **Reversibility**: Medium. **Default**: top-1 chunk pair per (A,B) with cosine threshold ≥ 0.75, render the threshold as adjustable.

### B7. **Are conversation messages and references embedded the same way as documents?**
`conversation_memory_vectors` exists `[verified]` — so conversations have their own vector space. Same model? Same dimension? Mixable into one k-NN index? Unknown without reading `modules.rs` wiring. Matters only if A3 resolves to "include conversations."
- **Who**: Code investigation. **Reversibility**: Depends.

### B8. **Is there any existing client for the backlinks command?**
`BacklinksResponse` exists in `websrc/types/api/mentions.ts` `[verified]`, but a search for `getBacklinks` / `invoke.*backlink` turned up no callers. The backend command is functional; no UI consumes it. Clean slate — worth confirming before design starts assuming there's an existing surface to graft onto.
- **Who**: Code investigation. **Reversibility**: Irrelevant, just useful knowledge.

---

## C. Design unknowns (flag now, decide later)

- **C1. Layout algorithm** (force-directed / hierarchical / radial / fixed grid). UX-load-bearing. Force-directed is the cliché; it's also the source of the "hairball" problem. Defer to design phase, but flag: restraint aesthetic argues *against* jitter-prone simulations. A static deterministic layout may feel more "considered."
- **C2. Single canvas vs split-pane with a reading column** — Obsidian's right-pane-backlinks pattern is arguably a better fit for Recall's editorial aesthetic than a canvas. Real question: is this a graph *view* or a graph *tool*?
- **C3. Mobile / responsive** — probably out of scope (Tauri desktop), but confirm before building with zoom/pan gestures.
- **C4. Dark/light parity** — the aesthetic guide is dark-first `[verified]`. Light-mode graph readability (edge contrast, node labels) is non-trivial; don't ship light until it's tuned.
- **C5. Edge directionality rendering** — wikilinks are directional (A links to B), co-mentions are undirected, semantic similarity is undirected. Mixing them needs a visual language.
- **C6. What's the canvas tech?** — SVG scales to ~500 nodes; Canvas/WebGL beyond. Frontend is React; no existing graph lib is imported. This is a dependency decision with long tail.

---

## D. Scope unknowns

- **D1. MVP definition.** "Rebuilt properly" is ambiguous between "one honest view of real edges" and "Obsidian-tier feature parity." The former ships in a week; the latter is a quarter. **Default**: honest-view MVP — real edges only (wikilinks + @mentions), no semantic, no persistence, no filters. Semantic edges as phase 2 once MVP proves the base feels trustworthy.
- **D2. Chat integration** — "graph of documents cited in this conversation" is genuinely useful and the existing `conversation_documents` table `[verified]` supports it cheaply. Could be the *first* shipped graph view rather than a global graph. Worth raising.
- **D3. Time-evolution / temporal view** — almost certainly out of scope. Flag only to kill explicitly.
- **D4. Filtering by tag / mention type / source_type** — low cost to add, high usefulness, but each filter is a UI decision that bakes in the mental model. Scope carefully.

---

## E. Failure-mode unknowns (ship-and-feel-bad risks)

- **E1. Empty graph.** A new user or a wikilink-averse user opens the graph and sees nothing. The empty state is load-bearing; if it reads as "broken," trust is lost permanently. Needs genuine design thought, not a stock EmptyState.
- **E2. Hairball graph.** 3k documents, heavy cross-linking, everything connects to everything. Force-directed produces pudding. The user feels the app is lying about structure. **Mitigation owed before ship**: progressive disclosure (show neighborhood, not global) or automatic clustering.
- **E3. Semantic edge distrust.** If we add semantic edges and the user sees "this cooking recipe is connected to this git commit message" because of a spurious embedding near-miss, the feature permanently reads as noise. **One bad suggestion per session is enough to kill the feature.** Threshold tuning and edge-type legend are not optional.
- **E4. Performance cliff.** At some N, pan/zoom lags. The cliff is where frustration begins. User tells themselves "the app is slow" about the whole product, not the graph. Worth setting a performance budget *up front* and a graceful degradation path (cap visible nodes, not crash).
- **E5. Stale graph.** User writes a new linked note; graph doesn't update until reload. Small detail, catastrophic for "instrument" framing.
- **E6. Layout non-determinism.** Graph looks different every time. For a "considered" aesthetic, this reads as flaky. Seed the layout.
- **E7. Label collision / illegibility.** Nodes overlapping labels. Common failure. Belongs in the shipped-feels-bad column because the aesthetic guide explicitly values legibility.
- **E8. Double-write to backend.** Bug-category risk given the existing schema/domain mismatch (B1). If graph is wired before B1 is fixed, we get a live feature on a broken substrate.
