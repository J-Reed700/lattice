# Knowledge Graph — Architecture Spec

**Status:** proposed, foundation-only
**Paired with:** `KNOWLEDGE-GRAPH-CONCEPT.md` (conceptual frame), `AESTHETIC-GUIDE.md`, `TOKENS-SPEC.md`, sibling surface specs (`CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`).
**Scope:** Data model, backend Tauri commands, frontend technology choice, performance envelope, app-shell placement, phasing. **Not** final visual treatment, motion, component composition, or copy — those are downstream (animation-choreographer, visualization-architect, component-designer, voice-strategist).
**Route:** `/graph` (new).

---

## 0. Prior-art postmortem — what actually went wrong

The deleted `MentionGraph.tsx` (recovered from git commit `1e6310ee`) is 363 lines and fails in four diagnosable ways. Before designing forward, they need to be named, because each names a failure mode we must not repeat.

1. **Fake edges.** Lines 84–95 generate edges with `Math.random() > 0.7`. Every edge rendered was noise. A graph whose edges are random is literally a lie — the aesthetic "quiet, considered, precise" from `AESTHETIC-GUIDE.md §4` cannot survive this.
2. **N+1 backend calls on load.** Lines 64–82 fetch every wikilink, then for each one re-invokes `get_backlinks_for_mention` serially. A vault with 500 wikilinks does 501 IPC round-trips before a pixel renders.
3. **Scrolling state-set inside the animation loop.** Lines 110–164 mutate `newReactNodes`, then `setReactNodes(newReactNodes)` inside `applyForces`, which is called inside `requestAnimationFrame`. Every frame triggers a full React re-render. The canvas draws *on top of* the React tree re-reconciling. This is the reason canvas graph views feel laggy at ~200 nodes even though the raw math is fine.
4. **No real data model.** The component invented a `GraphReactNode` / `GraphEdge` shape inline and trusted itself. There is no canonical graph schema anywhere — backend or frontend — so any cross-component reuse is blocked at the type level.

These four are the spec's constraints. Every decision below is partly a counter to one of them.

---

## 1. What relationships actually exist in the data

The graph has nothing to render without real edges. Before proposing edge types we verify against the codebase. Below is an exhaustive inventory.

### 1.1 Edges that exist today (verified in the SQLite schema)

| Edge type | Storage | Cardinality (typical → worst) | Endpoint surface today |
|---|---|---|---|
| **Wikilinks / @mentions (shared-mention)** | `mentions` + `document_mentions` (migrations/legacy_temp/010, init_schema lines 104–125). Two documents are edge-connected if they both reference the same `mention_id`. | Power-law: most docs have 0–3 shared-mention neighbors; dense docs can share 20+ mentions with 50+ other docs. Worst case in a 5k-doc vault: ~250k joined pairs if a single "tag-like" mention is used universally. | `get_mentions_for_document`, `get_backlinks_for_mention`, `get_mentions_by_type`, `search_mentions`. **No command returns edges directly** — the join is client-side today. |
| **Chat citations (conversation → document)** | `conversation_documents (conversation_id, document_id, chunk_id, relevance_score, added_at)` (init_schema lines 286–298). Every time an assistant answer cites a doc, a row lands here. | Typical: 3–8 docs per conversation. A power user with 2k conversations → 6k–16k rows. Worst case: a few "hub" docs cited in hundreds of conversations. | Internal (`ConversationRepository::get_document_references`). **No public Tauri command surfaces this.** |
| **Co-citation (document ↔ document, via conversation)** | Derived from the above: two docs are connected if they appear together in the same conversation. | Quadratic in per-conversation citations but small per conversation (3–8 cited → 3–28 pairs). 2k conversations → 6k–50k pairs. | None. Needs a new query. |
| **Semantic similarity (chunk ↔ chunk, aggregatable to doc)** | `text_embeddings` (init_schema lines 55–62) + USearch HNSW index (`features/search/engine/vector_search/usearch_index.rs`). | Dense: every chunk has k=N nearest neighbors for any N. Document-level aggregate needs a design decision (top-k chunks per doc? mean of top-k?). | `find_similar(chunk_id, limit)` (`features/search/commands.rs:942`). **Chunk-level only**; no doc-level "related documents" command exists. |
| **Folder proximity** | `documents.file_path` (init_schema line 9). Two docs are proximal if they share a parent directory prefix. | Dense within a folder — a folder of 30 docs yields 435 pairs. Across a full vault: could be tens of thousands of pairs. | None. Trivially derivable from `list_all_documents`. |
| **Temporal co-access / recency** | `documents.access_count`, `documents.last_accessed_at`, `recent_documents` table (init_schema lines 33–34, 251–258). | A signal, not a graph primitive. Would need a sessionization step to produce "accessed in the same session" edges. | `list_all_documents` returns metadata; no sessionization exists. |
| **Document ↔ conversation (citation membership)** | Same as co-citation but rendered as a cross-type edge. | Bounded by total citations: 6k–16k edges at 2k-conversation scale. | None (same data source). |
| **Bookmarks / references (message bookmark → document)** | `conversation_message_bookmarks` plus the referenced message's sources. | Small: tens to low-hundreds per active user. | `listMessageBookmarks` (frontend). |
| **Tags (document ↔ tag ↔ document)** | `tags` + `document_tags` (init_schema lines 87–101). | Typical: 2–5 tags per doc. A popular tag creates a clique — 50-doc tag → 1,225 edges. | `get_all_tags`, tag management. Not surfaced as a graph edge. |

### 1.2 Edges that do NOT exist today

- **Explicit user-authored "related to" links** between documents. The schema has no such column.
- **Temporal sessionization.** No "access session" concept; `last_accessed_at` is a single scalar.
- **User annotations linking documents.** Highlights exist inside journal notebook pages but do not link pairs of documents.
- **Embedded hyperlinks** inside document bodies. Markdown documents may contain `[text](url)` but nothing parses these into an edge table today.

### 1.3 Honest assessment: which edge types are *visually meaningful*

"Visually meaningful" = when two documents are connected by this edge, a reasonable user, on inspection, says "yes, those two are related." Forcing this lens disqualifies several:

- **Wikilinks (shared-mention) — HIGH meaning.** If I wrote `[[Alice]]` in two different notes, those two notes are about Alice. That is a real, author-intentioned link.
- **Co-citation via chat — HIGH meaning.** If I asked a question and the assistant cited both docs, my own use pattern connects them. Evidence-bearing and usage-grounded.
- **Chat-to-document citations — HIGH meaning** as a cross-type edge: "this conversation touched this doc."
- **Semantic similarity — MEDIUM meaning.** USearch chunk-nearest-neighbors find textually similar passages. Often useful; often spurious (two docs share a boilerplate paragraph, or both quote the same library's docs). Without a threshold that's been tuned on real Recall vaults, this is a noise risk.
- **Folder proximity — LOW to MEDIUM meaning.** File-system placement is a weak signal — users cluster by topic sometimes, by date sometimes, by project sometimes. As the *only* edge type it's cheating (you're drawing the folder tree, not a knowledge graph). As a *supplementary* edge type (slightly stronger weight for same-folder pairs) it's fine.
- **Tags — HIGH meaning** when tags are used well, but tag discipline is a per-user variable. A user with 20 tags gets a meaningful clustering; a user with 200 barely-used tags gets clique hairballs.
- **Temporal / recency — LOW meaning without sessionization.** "Accessed close in time" is noisy; "accessed in the same session I was thinking about X" requires building session infrastructure we do not have.

---

## 2. Canonical data model

One schema, shipped as the type that everything — backend endpoints, hooks, components — speaks.

### 2.1 Nodes

```typescript
type GraphNodeKind = 'document' | 'conversation' | 'mention';

interface GraphNode {
  id: string;                   // canonical ID with kind-prefixed namespace: "doc:<docId>", "conv:<convId>", "mention:<mentionId>"
  kind: GraphNodeKind;
  label: string;                // display name; for documents: file_name; for conversations: title; for mentions: name
  // Light metadata for filtering/rendering — no full content, no chunks
  meta: {
    // Common
    updatedAt: string;          // ISO 8601
    // Document-only
    filePath?: string;
    fileType?: string;          // "pdf", "md", "txt", ...
    category?: string;          // documents.category
    wordCount?: number;
    sourceType?: 'local' | 'web';
    // Conversation-only
    messageCount?: number;
    spaceId?: string;
    // Mention-only
    mentionType?: 'person' | 'concept' | 'wikilink';
    documentCount?: number;     // how many docs reference this mention (precomputed)
  };
}
```

**Why namespaced IDs.** The graph will mix kinds in Phase 2+. A single `id: string` space that accidentally collides document and mention UUIDs (which are both UUID v4) would be a silent bug. `"doc:..." | "conv:..." | "mention:..."` makes the bug impossible.

### 2.2 Edges

```typescript
type GraphEdgeKind =
  | 'shared-mention'     // two documents share ≥1 mention. Phase 1 primary.
  | 'co-cited'           // two documents cited in the same conversation. Phase 1 secondary.
  | 'chat-cites'         // conversation → document citation. Phase 2 cross-type.
  | 'mention-in'         // mention → document (the mention appears in the doc). Phase 2.
  | 'semantic'           // vector-similarity neighbor (doc ↔ doc). Phase 3.
  | 'same-folder'        // file-path sibling. Phase 3 (weight modifier only).
  | 'same-tag';          // document ↔ document via tag. Phase 3.

interface GraphEdge {
  source: string;              // node id
  target: string;              // node id
  kind: GraphEdgeKind;
  weight: number;              // [0, 1] — normalized per-kind
  evidence: {
    // Kind-specific receipts. Never empty. If we can't show evidence, the edge doesn't exist.
    // shared-mention: mentions that produce the edge
    mentions?: Array<{ id: string; name: string; type: string }>;
    // co-cited: conversations that produce the edge
    conversationIds?: string[];
    // chat-cites: per-citation chunk count / max relevance score
    citationCount?: number;
    maxRelevance?: number;
    // semantic: top matching chunk pair + similarity score
    topChunkPair?: { sourceChunkId: string; targetChunkId: string; score: number };
    // same-folder: the shared parent directory prefix
    folderPath?: string;
    // same-tag: the tag ids
    tagIds?: string[];
  };
}
```

**Weight normalization rules (locked for Phase 1):**

- `shared-mention`: `min(1, count_of_shared_mentions / 5)`. Five shared mentions = fully strong; past that saturates. Prevents a single "tag-like" mention from totally dominating.
- `co-cited`: `min(1, count_of_conversations_cocitting / 3)`. Three co-citations = fully strong.
- Other weights are specified in the phase that introduces them.

### 2.3 Graph payload shape

```typescript
interface KnowledgeGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  // Stats for chrome display; avoids a second round-trip just to show "X nodes, Y edges"
  stats: {
    nodeCount: number;
    edgeCount: number;
    edgeKindBreakdown: Record<GraphEdgeKind, number>;
    generatedAt: string;       // ISO 8601
  };
  // If truncation applied (over budget), these report what was dropped
  truncation?: {
    reason: 'node-budget' | 'edge-budget';
    droppedNodeCount: number;
    droppedEdgeCount: number;
  };
}
```

### 2.4 Computed vs materialized

**Phase 1: computed on-demand, in a single SQL query per edge kind, cached in memory for the session.** Defended:

- A materialized table (`document_edges`) adds schema, migrations, and incremental-update complexity that dwarfs the rest of the feature.
- USearch-aided edges (semantic) are cheap to compute against an already-warm HNSW index but expensive to keep fresh incrementally.
- The graph view is not a hot path. It is opened occasionally, explored, closed. A 200–800ms build is acceptable — we are not in a render-per-keystroke situation.
- Single on-demand query also makes cache invalidation trivial: the query reads the tables as they are.

**Phase 2 (defer unless Phase 1 proves too slow at real vault sizes):** materialize `document_edges (source_id, target_id, kind, weight, updated_at)` with incremental maintenance triggered by document-indexing events. This is only worth building if Phase 1 measurement shows >1.5s graph generation at typical vault sizes on target hardware.

---

## 3. Backend endpoints (delta)

### 3.1 What already exists and is enough

- `list_all_documents(limit)` → `DocumentMetadataDto[]` — the node list for documents.
- `get_mentions_by_type(type)` — mention entities.
- `get_mentions_for_document(documentId)` — per-doc outgoing.
- `get_backlinks_for_mention(mentionName)` — per-mention incoming docs.
- `find_similar(chunkId, limit)` — chunk-level semantic (Phase 3 only).

### 3.2 What is missing

The existing mention endpoints **do not produce document↔document edges**. They produce either (a) list of mentions or (b) per-mention list of backlinks. To draw shared-mention edges, the frontend would have to pull every mention, then for each one pull backlinks, then compute the join client-side. That is the N+1 the old code already did and choked on. The join must happen in Rust, once, server-side.

### 3.3 New commands

Proposed surface: **one umbrella endpoint** for graph retrieval, with filter parameters. Not two pluralistic endpoints (`get_shared_mention_edges`, `get_cocitation_edges`) — the frontend never wants just one edge kind in isolation at Phase 1, and a single command keeps the caching/truncation logic in one place.

```rust
#[tauri::command]
#[specta::specta]
pub async fn get_knowledge_graph(
    container: State<'_, Container>,
    request: GetKnowledgeGraphRequestDto,
) -> Result<KnowledgeGraphDto>;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetKnowledgeGraphRequestDto {
    /// Which edge kinds to include. Phase 1 default: ["shared-mention", "co-cited"].
    pub edge_kinds: Vec<String>,

    /// Optional focus node id ("doc:<uuid>") — restricts the returned graph to this
    /// node + its 1-hop (and 2-hop if `focus_depth == 2`) neighborhood.
    /// None = full graph (subject to budgets).
    pub focus_node_id: Option<String>,

    /// 1 or 2. Ignored if focus_node_id is None. Default 1.
    pub focus_depth: Option<u8>,

    /// Hard node budget. Default 2,000. Nodes beyond this are dropped
    /// (lowest-degree first) and surfaced via `truncation`.
    pub max_nodes: Option<usize>,

    /// Hard edge budget. Default 8,000. Edges beyond this are dropped
    /// (lowest-weight first) and surfaced via `truncation`.
    pub max_edges: Option<usize>,

    /// Minimum edge weight in [0, 1]. Default 0.0 (no floor).
    /// Useful for filtering semantic edges below a noise threshold once they ship.
    pub min_edge_weight: Option<f32>,

    /// Optional space scope. When set, only documents that are members of this
    /// conversation-space are included. Mirrors the sibling surfaces' space scoping.
    pub space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
    pub stats: GraphStatsDto,
    pub truncation: Option<GraphTruncationDto>,
}
```

**Why an umbrella endpoint.** The three alternative designs and their rejections:

- *Separate commands per edge kind.* Shifts the join / dedup / budget logic to the frontend. Repeats the N+1 failure mode. Rejected.
- *Paginated / streaming endpoint.* A force-directed layout cannot start until all nodes and edges are known (node positions are mutually dependent). Streaming gives no benefit. Rejected.
- *Per-node expansion (`get_graph_neighbors(nodeId)`).* Correct for Phase 2+ (a "click to expand" interaction), but Phase 1 needs a single bounded fetch. We add neighbor expansion as a second command *if* Phase 2 surfaces it.

**Single-command pagination-at-the-server.** `max_nodes` / `max_edges` are the pagination primitive. The server truncates by a defensible heuristic (lowest-degree nodes, lowest-weight edges) and reports what was dropped. The frontend never asks for page 2 — instead, the user refines filters (edge kinds, min weight, focus node) and the query re-runs.

### 3.4 Implementation sketch (for modular-builder follow-up, not binding)

Lives at `src/app/src/src/features/graph/` (new feature folder, per the vertical-slice direction).

```
features/graph/
  mod.rs
  commands.rs         — the Tauri command binding
  dto.rs              — request + response shapes
  use_cases/
    get_graph.rs      — orchestrates edge builders, budgets, truncation
    mod.rs
  edge_builders/
    shared_mention.rs — single SQL query joining document_mentions to itself
    co_cited.rs       — single SQL query joining conversation_documents to itself
    mod.rs
  plugin.rs           — Tauri plugin registration
```

The `shared_mention.rs` builder is **one SQL query** (SQLite-compatible):

```sql
SELECT
  dm1.document_id    AS source_id,
  dm2.document_id    AS target_id,
  COUNT(DISTINCT dm1.mention_id) AS shared_count,
  GROUP_CONCAT(DISTINCT m.id || '|' || m.name || '|' || m.type) AS mentions_blob
FROM document_mentions dm1
JOIN document_mentions dm2
  ON dm1.mention_id = dm2.mention_id
  AND dm1.document_id < dm2.document_id  -- undirected: source < target deduplicates
JOIN mentions m ON m.id = dm1.mention_id
GROUP BY dm1.document_id, dm2.document_id
HAVING shared_count >= 1;
```

Co-cited is the same shape against `conversation_documents`. Both return in low single-digit milliseconds at typical scale because `document_mentions(document_id, mention_id)` is already indexed (migration 010). The N+1 becomes 1.

### 3.5 Frontend API surface

Adds one entry to `src/app/websrc/lib/api.ts` — mirroring the existing patterns:

```typescript
getKnowledgeGraph: async (request: GetKnowledgeGraphRequest): Promise<ApiResult<KnowledgeGraph>> =>
  apiCall<KnowledgeGraph>('get_knowledge_graph', { request }),
```

Types (`GraphNode`, `GraphEdge`, `KnowledgeGraph`) live in `websrc/lib/types/graph.ts`, mirroring the `specta`-generated Rust shapes.

---

## 4. Frontend technology

### 4.1 The choice

**Sigma.js + Graphology.**

- Bundle: ~110 KB minified + gzipped for the pair (`sigma` ~60KB, `graphology` + `graphology-layout-forceatlas2` ~50KB). Roughly one-fifth of Cytoscape.
- Rendering ceiling: WebGL; comfortable at 10,000 nodes / 30,000 edges at 60fps on modern hardware. Documented sigma benchmarks run 50k nodes in reduced-detail mode.
- Developer ergonomics: Graphology is a plain in-memory graph data structure (separable from rendering); Sigma is a renderer over it. Swap renderers if needed. Force-atlas-2 and force-link layouts are first-party modules. Event model (click / hover / drag) is straightforward.

### 4.2 Alternatives evaluated

| Option | Bundle | Node ceiling | Why rejected |
|---|---|---|---|
| **Hand-rolled canvas + force layout** | ~0 | ~200 before jank | What we had. Rejected for the four failures in §0 and because "build your own force simulation" is a time sink with no shipping payoff. |
| **D3-force (canvas or SVG)** | ~50 KB | SVG: ~500; canvas: ~2k | D3-force is the math, not the renderer. We would still need to build a canvas or SVG layer on top, and SVG hits a jank ceiling faster than WebGL. Also: d3's force simulation API is imperative and notoriously awkward inside React's lifecycle (the prior art is full of bugs exactly like failure #3 above). |
| **react-flow / xyflow** | ~80 KB | ~500 | Optimized for *diagrams with user-authored layouts* (node-based editors, workflow builders). Not designed for force-directed exploration at thousands of nodes. Wrong tool. |
| **Cytoscape.js** | ~500 KB | ~5k+ with caveats | The most battle-tested, biggest algorithm library, heaviest bundle. Overkill for a personal knowledge base. Its visual defaults are also dated — recovering the "quiet, considered" aesthetic against Cytoscape's stock styles is more fight than benefit. Held in reserve if graph algorithms (community detection, centrality) become load-bearing in Phase 3. |
| **vis-network** | ~250 KB | ~2k | Older API, ugly defaults, hard to restyle against `AESTHETIC-GUIDE.md`. Rejected on aesthetic grounds. |
| **Custom WebGL + graphology** | ~50 KB + weeks | unlimited | Wrong tradeoff at this stage. Revisit only if Sigma's defaults block us. |

### 4.3 The defense in three sentences

Sigma.js + Graphology is a WebGL renderer over a clean in-memory graph data structure — the rendering and data layers are separable, so if we ever outgrow Sigma we keep our graph and swap the view. At 110 KB it is a quarter of Cytoscape's weight with a higher rendering ceiling (WebGL vs. canvas) for the kind of exploration this feature will actually see. Its defaults (small monochrome nodes, thin edges, no chrome) are closer to our aesthetic out of the box than Cytoscape's (decorative gradients, badges, directional arrows everywhere) — we fight less, we ship faster.

---

## 5. Performance envelope

### 5.1 Hardware assumption

Target is the median Recall user's desktop: Apple Silicon M1–M3 or equivalent x86_64, integrated GPU sufficient for WebGL. We do not design for a 2015 ultrabook.

### 5.2 Sigma rendering ceiling

- **≤2,000 nodes / ≤8,000 edges** — 60fps interaction including drag-pan-zoom, no degradation required. This is the Phase 1 default budget.
- **2,000–10,000 nodes** — 30–60fps steady, interaction may dip during force-atlas relayout. Acceptable for "I have a large vault" power users. Controlled by the server `max_nodes` parameter.
- **>10,000 nodes** — Sigma's `renderLabels: false` mode + an LOD strategy (labels only on zoom, edges faded at distance). Achievable but Phase 3 work, not Phase 1.

### 5.3 When the layout becomes visually useless

Separately from what Sigma can *render*, a force-directed layout becomes *epistemically useless* well before the rendering ceiling. Past roughly **1,000 nodes** a global force-directed layout is a hairball — the eye cannot decompose it into meaningful sub-structures. The `KNOWLEDGE-GRAPH-CONCEPT.md` document makes this case on aesthetic / philosophical grounds; this spec honors it operationally.

**Consequence:** the default load strategy for a vault larger than 1,000 documents is **not** "render everything." It is:

- **Default focused view.** The graph opens on a *recent-documents subgraph* — the top 100 most-recently-modified documents plus their 1-hop neighbors. A full-vault button ("Show everything") is available but labeled as the slow path.
- **Focus-node mode.** From Chat / Journal / Reference, a "view in graph" action opens the graph centered on a node with `focus_depth=1`.
- **Filter-driven reduction.** Edge-kind toggles let the user narrow to "just shared-mention" or "just co-cited" — both of which sparsify dramatically.

### 5.4 The 50,000-document case

A user with a 50k-document vault asking for "show me the whole graph" gets **no**. Explicitly:

- `get_knowledge_graph` without `focus_node_id` caps at `max_nodes = 2000` default. The server truncates by lowest-degree node (isolated + sparsely-connected documents) and reports truncation in the response.
- The UI surfaces the truncation — a chip reading `Showing 2,000 of 50,000 documents. [Expand]` — so the user is never silently lied to.
- "Expand" raises `max_nodes` to a higher tier (5k, 10k) with an explicit warning. Past 10k we refuse client-side with a "use focus mode" prompt.

This is the direct inverse of the old `MentionGraph`'s failure: that code silently rendered whatever the backend returned with no budget.

### 5.5 Graph build time target

End-to-end, from click on `/graph` to first rendered layout:

- **≤300ms** at 500-doc scale.
- **≤800ms** at 2k-doc scale (Phase 1 default budget).
- **≤1.5s** at 5k-doc scale (inside the default `max_nodes`).

If any of these slip in measurement, materialization (§2.4 Phase 2) is the escalation path.

---

## 6. Surface layout in the app shell

### 6.1 The route

**`/graph` — standalone route,** added to `routes.tsx` alongside `/chat`, `/journals`, `/references`. Not a modal overlay on another surface; not a sidebar tab; not a popover. The feature is too material to be chrome of another screen, and the two-pane shell siblings establish that main tools get their own route.

Nav label: **Graph**.

### 6.2 Sibling-surface layout (two-pane)

Honors the Chat / Journal / References family. Left sidebar (`280px`) + main pane (flex-1), collapse via `⌘\`, `--bg` canvas everywhere, no gradient, no blur.

**Left sidebar contents (top to bottom):**

1. **Top rail** (48px). `--surface`, hairline bottom border. Wordmark "Graph" at `text-sm` `font-serif` `weight 600`. `PanelLeft` collapse icon right.
2. **Focus input.** A shadcn `Select` or search combobox: "Focus on…" → searches documents / conversations / mentions. Selecting a node loads the graph centered there with `focus_depth=1`. Default state: empty (full-vault view). Same visual treatment as the Journal journal-picker and References origin-picker.
3. **Edge-kind filters.** A vertical list of checkboxes — one per available `GraphEdgeKind`. Phase 1 lists two: `Shared mentions` (checked), `Co-cited in chat` (checked). Each row shows a per-kind edge count `(1,247)` in `text-xxs` `--text-muted` trailing. Toggling re-runs the query. Rationale for vertical-checkbox over horizontal chips: with >3 edge kinds (Phase 2+) chips overflow the sidebar width.
4. **Node-kind filters.** Same pattern. Phase 1 shows `Documents` (checked) only. Phase 2 adds `Conversations`, `Mentions`.
5. **Weight threshold.** A slider (shadcn `Slider`) titled "Min edge weight", range 0.0–1.0, default 0.0. Updates the `min_edge_weight` on the request. `text-xs` `--text-muted` numeric readout right-aligned.
6. **Stats footer** (32px, hairline top border). `N nodes · M edges` at `text-xxs` `--text-muted`. If truncated, `N of T shown · [Expand]` where Expand raises `max_nodes`.

**Main pane contents:**

- The Sigma canvas fills the pane.
- **No top chrome** — no toolbar band, no "Knowledge Graph" heading, no breadcrumb. The sidebar is the chrome; the canvas is the content. Same rule as Chat / Journal / References.
- **Floating control rail** — bottom-right of the canvas, `--surface-raised` background, `--border-subtle` 1px, `--radius-md`, `--shadow-md`, `4px 8px` padding. Four icon-only buttons (14px lucide icons, `--text-tertiary` rest, `--text-primary` hover, no background pills):
  - `ZoomIn` (zoom in)
  - `ZoomOut` (zoom out)
  - `Maximize2` (fit to view / reset pan + zoom)
  - `RefreshCw` (re-run the backend query)
- **Selection detail popover.** Clicking a node opens a Radix `Popover` anchored to the node's screen position — `--surface-raised`, same treatment as Chat citation popovers (CHAT-REDESIGN-SPEC §3.4). Contents:
  - Node label (`text-sm` `--text-primary`).
  - Kind + meta (`text-xs` `--text-muted`): e.g. `Document · MD · 2,340 words · edited 3d ago`.
  - Connection count (`text-xs` `--text-tertiary`): `8 shared-mention · 3 co-cited`.
  - Three actions: `Open` (navigates to the document / conversation), `Focus here` (reloads graph with this node as focus), `Dismiss` (close popover).
- **Empty state** (vault has no documents or all documents have no edges): centered, no card frame, `Share2` lucide icon 32px `--text-muted`, heading `text-lg` `--font-serif` `--text-primary`: *"Nothing to connect yet."* Body `text-sm` `--text-tertiary`, max-width 400px: *"The graph shows how your documents reference each other. Add more notes with `[[wikilink]]` mentions or cite documents in Chat, and this view will fill in."*

### 6.3 Click → what happens

Per §6.2 the node click opens the detail popover. Defended against the two alternatives:

- *Click-to-focus.* Would be a destructive interaction — one accidental click changes the entire visible graph. Rejected.
- *Click-to-open.* Would conflate "explore" with "leave the graph." Rejected.

Detail-popover-with-explicit-actions preserves the user's ability to look around before committing to either leaving or re-centering. `Focus here` is the explicit re-center; `Open` is the explicit leave.

### 6.4 Cross-surface entry points

- **Chat:** the conversation's cited-documents footer (CHAT-REDESIGN §3.4) gains a trailing link: `[View in graph →]` — loads `/graph?focus=conv:<id>`.
- **Journal:** the entry's `From this conversation` strip (JOURNAL-REDESIGN §6) gains the same link: `[View in graph →]` — loads `/graph?focus=conv:<entryId>`.
- **References:** the reference reader's action rail (REFERENCE-REDESIGN §5.6) does **not** gain a graph action in Phase 1 — a single reference is not a graph neighborhood. Deferred.

### 6.5 URL params

- `/graph` — full-vault default view.
- `/graph?focus=doc:<id>` — opens focused on a document.
- `/graph?focus=conv:<id>` — opens focused on a conversation.
- `/graph?focus=mention:<id>` — opens focused on a mention (Phase 2+).
- `/graph?edges=shared-mention,co-cited` — overrides the default edge-kind filter set.

Deep-link-safe. Same param discipline as Journal / References.

---

## 7. Phasing

### 7.1 Phase 1 — the shipping minimum (1–2 agent-days)

**Goals:** real edges (no `Math.random()`), one backend call (no N+1), shipping-quality aesthetics.

- Backend: `get_knowledge_graph` command, `shared_mention` and `co_cited` edge builders, `document` node only.
- Frontend: `/graph` route, Sigma + Graphology integration, two-pane shell, edge-kind and node-kind filters, weight slider, node-click detail popover, floating control rail, empty state.
- Graph is computed on-demand (§2.4). No materialization.
- Default budget: `max_nodes=2000`, `max_edges=8000`.
- Cross-surface: "View in graph" links from Chat citations and Journal entries.

**Visibly impressive at 1–2 days** because: the edges are real (wikilinks the user actually wrote), the interaction is smooth (WebGL, proper data flow), the visual treatment is tokenized and restrained, the selection popover is well-integrated, and the "Focus here" interaction gives immediate exploratory delight.

### 7.2 Phase 2 — widen the graph

- Add `conversation` and `mention` nodes.
- Add `chat-cites` edges (conversation → document) and `mention-in` edges (mention → document).
- Add `get_graph_neighbors(nodeId, depth)` command for lazy expansion on "Focus here" from an already-open graph.
- Add clustering colorization: graphology provides Louvain community detection; color-code nodes by cluster using `--accent` at varying lightness (not multiple hues — the aesthetic guide still binds).

### 7.3 Phase 3 — semantic, folder, tag (defer until Phase 1 + 2 ship and are used)

- `semantic` edges via chunk-level USearch aggregated to document. Requires threshold tuning against real vaults. Defer, because premature semantic edges will read as spurious — see §1.3.
- `same-folder` edges as weight modifier (increase weight of other edge kinds for same-folder pairs rather than a distinct edge kind). Makes clusters read more naturally without adding visual noise.
- `same-tag` edges.
- Time-evolution: an optional time-range filter showing only documents created / modified in a window. Enables "what was I thinking about in March?" queries.
- Materialization (`document_edges` table) only if measurement shows Phase 1+2 is too slow at real-user vault sizes.

### 7.4 Phase 4 — explicitly out of scope until asked

- User-authored edges (manual "mark as related").
- Edit-in-graph (rename mentions, re-link documents).
- Export (SVG/PNG snapshot).
- Multi-select and batch operations on graph selections.
- Timeline scrubber animation ("play the graph over time").

---

## 8. Risks

### 8.1 What can go wrong in Phase 1

1. **Sparse graph looks sad.** A vault with few wikilinks produces a graph of mostly isolated dots. Mitigation: filter out degree-0 nodes by default (with a toggle `Show isolated`). The user sees structure where structure exists, not dots for the sake of dots. Phase 1 decision: isolated nodes hidden by default in the full-vault view; shown when a focus is selected (because the focused node is inherently of interest even if isolated).

2. **Dense "tag-like" mention dominates the graph.** A user who wikilinks `[[Meeting]]` in 100 notes creates a 100-clique. Mitigation: the weight normalization (min of 1, count/5) softens it, but the rendered edges still clutter. Phase 2 adds a high-degree-node dampening (reduce outgoing edge rendering intensity for nodes of degree > 50). Phase 1 lives with it; the chrome shows counts, the user can raise the min-weight slider.

3. **Perf cliff at 1,000+ documents.** Sigma handles it, but the layout converges visually poorly. Covered by §5.3 — we don't let the default view run past 2k nodes, and we default-load a recent-docs subgraph on large vaults.

4. **Force layout placement is unstable across reloads.** Two consecutive opens of the same graph show nodes in different positions (different random init → different force-atlas minima). This hurts "where did I last see X?" recall. Mitigation: seed the layout RNG deterministically (same graph → same positions). Implemented in graphology via a seeded PRNG. Easy and important; codify in Phase 1 even though it's a ten-minute detail.

5. **"View in graph" link from Chat/Journal opens the graph on a conversation node with no other connections.** Conversations with one cited document produce a 2-node graph, which is visually thin. Mitigation: the conversation focus load uses `focus_depth=2` (conversation → docs → docs-that-share-mentions-with-those-docs) to always land on something with structure. This is a Phase 1 spec decision.

6. **User expects Obsidian-style global map and gets less.** The `KNOWLEDGE-GRAPH-CONCEPT.md` sibling spec argues we are *intentionally* not that. The empty-state copy and default-focused-view are the soft landing for this mismatch. The hard case — a user who specifically wants the hairball — we serve via the `Expand` truncation affordance with an explicit warning label.

### 8.2 Reversible vs locked-in Phase 1 decisions

| Decision | Reversible? | Cost to change |
|---|---|---|
| Sigma.js as renderer | Reversible | Graphology is our data layer; swap Sigma for another renderer in a day if needed. |
| On-demand computation (no materialization) | Reversible | Add the table + maintenance in Phase 2 without touching the API shape. |
| Namespaced IDs (`"doc:..."`) | Locked | Changing this post-ship breaks all stored URL params and cross-surface links. |
| Umbrella `get_knowledge_graph` endpoint | Reversible | Splitting to per-kind endpoints later is mechanical. |
| `max_nodes=2000` default | Reversible | Tunable per-request. |
| Two-pane shell + `/graph` route | Reversible | Could fold into a modal later, but the family-of-surfaces argument makes it unlikely. |
| Shared-mention as Phase 1 primary edge | Locked for this phase | Can add others in Phase 2; cannot remove without breaking the whole thing. |
| WebGL rendering (requires WebGL context) | Mostly locked | Tauri webview on every target platform supports WebGL — low practical risk. |

---

## 9. Explicitly NOT decided by this spec

- **Final node / edge visual treatment.** Node radius curves, edge stroke widths, how focus state is rendered, how the detail popover's citation list is styled — downstream to `visualization-architect` and `component-designer`.
- **Motion choreography.** How the graph animates on load, how focus-change transitions, how the selection popover opens — downstream to `animation-choreographer`. Constraint inherited from `TOKENS-SPEC.md §7`: durations are `--duration-fast/base/slow`, easing is `--ease-out/in/linear`, no bounce, no performance. Sigma's default force-atlas "settling" animation is tolerable but the final call on whether we keep it, ramp it down, or skip-to-settled is the animation-choreographer's.
- **Filter-chip UX for edge kinds.** Vertical checkboxes per §6.2 are the spec; the exact component shape is `component-designer`.
- **Empty-state and "truncated" chip copy.** Spec'd placeholders; final text is `voice-strategist`.
- **Clustering color treatment.** Phase 2 work, and the token implications (multiple lightnesses of `--accent`) warrant a dedicated spec pass.
- **Mobile / narrow-viewport graph.** Recall is Tauri desktop. If there is ever a web-embedded or narrow-window mode, that's a later call.

---

## 10. Verification checklist (for downstream specs and implementation)

- [ ] Every color resolves to a TOKENS-SPEC token; no raw Tailwind palette, no hex literals.
- [ ] No `backdrop-blur`, no gradient background, no glow, no colored shadow.
- [ ] Only one accent (violet) on the surface — selection state, focus ring, active filter underline.
- [ ] Sigma's default node stroke / edge color is overridden to tokens before first paint.
- [ ] Two-pane shell matches Chat / Journal / References proportions (`280px` sidebar, `⌘\` collapse, `--bg` canvas).
- [ ] Nodes and edges are derived from real data — **no random generation anywhere, no placeholder shapes**.
- [ ] Single backend call per graph load — never N+1.
- [ ] Truncation is reported in-UI when it happens.
- [ ] Layout is deterministic (seeded PRNG) within a given graph shape.
- [ ] `get_knowledge_graph` response is bounded by `max_nodes` and `max_edges` even on worst-case inputs.
- [ ] URL deep-linking works: `/graph?focus=doc:<id>` and `/graph?focus=conv:<id>` restore state.
- [ ] Cross-surface "View in graph" links from Chat and Journal land on a graph with ≥2-hop neighborhood.
