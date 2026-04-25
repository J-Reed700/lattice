# Corpus Shape — Research

**Status:** research synthesis, pre-spec
**Paired with:** `AESTHETIC-GUIDE.md`, `KNOWLEDGE-GRAPH-CONCEPT.md` (this is the corpus-level companion to that doc's local/temporal/exploration framing)
**Question:** for a domain-morphic vault — recipes today, papers tomorrow, personal writing the day after — what *non-graph* view of the corpus is actually useful?

---

## 1. Executive take

**Yes, but smaller than the brief implies.** The useful "shape" view is a flat, scannable list of **auto-generated collections** (Mem-style "smart collections," Pinterest-style "board sections") sitting beside the existing folder/file tree — not a map, not a chart, not a Voronoi treemap. Ship a phase-1 version that runs HDBSCAN over existing USearch embeddings, labels each cluster with a 3-5 word LLM prompt over centroid documents, and renders the result as a list of cluster cards with counts and three preview titles each. The ML is one weekend of work because the embeddings already exist; the discipline is in the labeling and the layout.

---

## 2. App survey

What competitors actually ship for "browse my collection" — observed, not theorized.

| App | Primary "browse" surface | Auto-clustering? | Auto-labels? | Notes worth stealing |
|---|---|---|---|---|
| **NotebookLM** | Sources panel (flat list, sorted) + Mind Map view | No clustering of sources; Mind Map clusters *concepts within* sources | LLM-generated mind-map nodes | The Mind Map is concept-clustering, not source-clustering. Source list itself is dumb. |
| **Mem** | "Collections" sidebar; AI suggests a collection per note at write time | Yes — auto-suggested collections per note, not global re-clustering | LLM labels at suggest-time | Closest analog. Collections are *named groups* the user can accept/reject. Not a viz, a list. |
| **Glean / Hebbia** | Search-first; "Browse" is a flat indexed catalog by source/connector | No global clustering; topic facets via filters | Manual/admin curation | Enterprise tools punt on shape — they assume you already know what you're looking for. |
| **Zotero** | Hierarchical Collections + Saved Searches (smart collections) | No ML; rule-based saved searches only | User-authored | 1000+ papers handled with manual collections + tag filters. Power-user tolerance is high. |
| **Readwise / Reader** | Library with sub-categories (Articles, PDFs, Books, Tweets) + tags | Type-based grouping; tag suggestions | Tag suggestions on import | Type-grouping does most of the work. Two-level nav (type → list). |
| **Apple Photos** | "For You" memories; People; Places; Media Types; Albums | Yes — agglomerative HAC on face/scene embeddings, on-device | Place/event/person labels | The gold standard. Multiple **facets** (people / places / type / date), each its own browse surface, none of them a graph. |
| **Spotify** | "Made For You" rows; Daily Mix N; Liked Songs; Artists | Yes — vibe clusters via audio-feature embeddings + collab filtering | Editorial naming + a few algorithmic ("Daily Mix 1") | Row-of-rows pattern. Clusters get *named playlists*, not just badges. |
| **Netflix** | Rows of rows ("Because you watched X", "Top 10", genre rows) | Yes — many micro-row algorithms, not one big clustering | "Because you watched..." gives the *reason* | The single best UX pattern: every cluster includes its rationale. Builds trust without showing the math. |
| **Pinterest** | Boards (user) + auto-suggested **Board Sections** (ML) | Yes — PinSage embeddings → Ward clustering → suggested sections | Annotation-derived labels filtered by quality | Crucially: *suggests* sections rather than auto-creating. User is in the loop. |
| **Apple Books** | Grid/list, sortable; auto-collections (Want to Read, Finished, Audiobooks) | No ML; status- and type-based | None | The simplest version that works. Type + status + sort. Almost no chrome. |
| **Scrivener** | Binder (tree) + Corkboard (cards) + Outliner | No ML; manual hierarchy | Manual | Shows that **multiple parallel views over the same nodes** beats one canvas. |
| **Finder** | Column / List / Gallery | No | No | The honest baseline. List works up to a few hundred items; below that, anything more is overhead. |
| **DEVONthink** | Folders + Smart Groups + AI "Classify" + "See Also" | Yes — AI classifier suggests folder for each doc; "See Also" surfaces neighbors | Folder names are user-authored | Auto-classification operates at *ingest*, not as a separate browse view. Same pattern as Mem. |
| **Eagle / Raindrop** | Folders + Smart Folders + Tag filters | Tag suggestions on import; rule-based smart folders | Tag suggestions only | Visual-thumbnail grids with light auto-tagging. No global clustering. |

**Patterns that recur across the winners:**

1. **Faceted browsing beats unified view.** Apple Photos has People / Places / Types / Albums as separate surfaces, each tightly designed. None tries to be the "everything view."
2. **Auto-labeled groups as flat lists**, not maps. Pinterest, Mem, Spotify all render clusters as named rows or cards in a list. Nobody ships a 2D embedding projection as the primary surface.
3. **Reasoning is shown.** Netflix's "Because you watched..." and Pinterest's annotation-derived section labels both make the *why* legible. The user doesn't need to see a graph; they need to see why this group exists.
4. **Auto-classification at ingest > re-clustering on demand.** DEVONthink, Mem, Pinterest all do work at the moment a document arrives, not when the user opens a viz. The "view" is just rendering the work that's already been done.
5. **Type / source / status grouping carries 60% of the value** with zero ML. Readwise's category sub-nav, Apple Books' Audiobooks/Finished, Zotero's Item Type column. Cheap, instantly legible.

---

## 3. Clustering and labeling — state of the art (2026)

**Embedding clustering.** HDBSCAN on sentence-embedding vectors is the standard primitive in 2026, used internally by BERTopic and most modern topic-modeling pipelines. Key practical knobs:

- `min_cluster_size`: 5 default, but for documents the heuristic is ~`sqrt(N) / 2` clamped to a floor. For Recall: 5 at 100 docs, ~15 at 1k, ~50 at 10k. Smaller values produce micro-clusters that read as noise; larger values collapse meaningful groups.
- HDBSCAN tags low-density points as **noise** (label `-1`). Render these as an "Unsorted" or "Other" bucket — do *not* hide them or force them into a cluster. The honest count of unclassifiable docs is itself useful signal.
- For a 1k-doc vault, expect **8-25 clusters** to emerge with sensible parameters. Below 8 the view is uninteresting; above 25 it stops being scannable.

**Topic modeling stacks.** BERTopic (sentence embeddings → UMAP → HDBSCAN → c-TF-IDF for keywords) reportedly outperforms LDA by 15-25% on coherence for short and mixed-domain text. The pipeline is well understood; what's still hard is *labeling*.

**LLM cluster labeling.** Two viable patterns:

1. **Centroid-doc prompt** — pick 3-5 docs nearest the cluster centroid, send to an LLM with "name this cluster in 3-5 words." Produces the best-quality labels but costs one LLM call per cluster.
2. **k-LLMmeans / "summary as centroid"** — replace numeric centroids with LLM-generated text summaries that are themselves re-embedded. Better for hierarchical drill-down but overkill for the flat-list MVP.

LLM labeling in 2026 is good enough that quality is no longer the blocker — the blocker is **consistency over re-clustering** (the cluster called "Thai dishes" today might be called "Southeast Asian curries" tomorrow if the algorithm reshuffles). The fix is to (a) cache labels keyed to a stable cluster fingerprint, and (b) allow user rename — Mem, Pinterest, and DEVONthink all do this.

**Per-doc auto-tagging at ingest.** Cheap and high-value. A small embedding model (EmbeddingGemma-class, on-device) plus a single LLM call per doc to extract 3-5 keywords gives Readwise/Mem-quality tag suggestions. This is *separate from clustering* and worth shipping independently — tags work at any corpus size, clustering only becomes interesting above some N.

**Mixed-corpus behavior.** This is the brief's hidden risk. HDBSCAN over a vault containing 400 recipes + 40 papers + 200 personal notes will produce: a few large recipe clusters, a few small paper clusters, and a noise tail of personal notes. Useful, but *only if* the labels are good — otherwise the user sees "Cluster 4 (47 docs)" and bounces. The mitigation is to also surface a **type/source facet** alongside clusters: "Recipes (440) / Papers (40) / Notes (200)" gives a legible top-level breakdown with zero ML risk.

**2D embedding projections (UMAP/Atlas-style maps).** Apple's Embedding Atlas, Nomic Atlas, etc. exist and are technically beautiful. Skip them for the primary surface — they're the corpus-level equivalent of the force-directed graph view that `KNOWLEDGE-GRAPH-CONCEPT.md` already argues against. Reserve as a power-user "explore embeddings" view if ever shipped.

---

## 4. Recommended implementation for Recall

### 4.1 Phasing

**Phase 0 — type & source facet (no ML, ship first).** A header strip on the file/vault browser showing type/source counts: `Markdown 412 · PDF 88 · Audio 12 · Email 4`. Click a type, list filters. This alone covers Readwise/Apple Books territory and is the honest baseline against which clustering must justify itself.

**Phase 1 — auto-collections (the real feature).**
- Run HDBSCAN over existing USearch embeddings on a debounced background tick (initial build, then incremental on ingest).
- For each cluster of ≥`min_cluster_size`, pick 5 representative docs (highest membership probability, like BERTopic's `get_representative_docs`).
- One LLM call per cluster → 3-5 word label + a one-sentence "what this is."
- Persist label keyed by a hash of the top-10 representative doc IDs (stable through small reshuffles; regenerates only when the cluster meaningfully changes).
- Surface as a **flat list of cluster cards** beside the folder tree.

**Phase 2 — per-doc tags at ingest.** Independent of clustering. 3-5 keywords per doc, LLM-generated at ingest from doc title + first ~500 tokens. Surface as a tag facet and as inline tags in the list view. Zero new clustering work; orthogonal value.

**Phase 3 (maybe never) — temporal shape.** A "what's been growing" strip — clusters whose doc count has grown most in the last month. Borrows Spotify's "Daily Mix N" / Netflix's row-of-rows pattern. Ships only if Phase 1 retains.

### 4.2 Where it lives

**A sibling to the folder tree, not a separate route.** The vault/file browser already exists and is the user's spatial home for "what's in here." The cluster list goes in a second left-rail panel — toggle between **Folders** and **Collections** at the top of the rail, the way Apple Photos toggles between Library / People / Places. Same content, two indexes.

Not on the Dashboard. Dashboard is a scanning/quick-action surface (per `DASHBOARD-FLATTEN-SPEC.md`) — adding a 25-card cluster list there would re-introduce exactly the bento sprawl that spec is removing. A "Top 3 collections" preview on Dashboard with a "see all" link is fine; the full surface lives with the file browser.

Not a standalone "Map" route. Standalone routes need a clear job-to-be-done; "look at the shape of your vault" is browsing, and browsing belongs adjacent to where the user already opens files.

### 4.3 Layout (in the Recall register)

Each cluster card, vertical stack in a single-column scrollable list:

- **Label** — `text-sm` weight 600 `--text-primary`. 3-5 words. No icon, no color chip.
- **Count + one-line description** — `text-xs` `--text-tertiary`, format: `47 documents · Southeast Asian recipes featuring coconut milk and lemongrass`. Interpunct separator per the existing voice.
- **Three preview titles** — `text-xs` `--text-secondary`, truncated to one line each. Click opens the doc; click the card label opens the cluster as a filtered list.
- **Hairline separator** between cards. No card border, no shadow, no tint. (Per `AESTHETIC-GUIDE.md` §6: "Use a hairline before a shadow.")
- An "Unsorted" pseudo-cluster always present at the bottom with the noise count. Honest.

No tag cloud (font-size-as-frequency reads as marketing decoration). No grid of preview thumbnails (Recall isn't visual-asset-heavy and a thumbnail grid would over-promise structure that isn't there). No timeline (deferred to Phase 3).

### 4.4 Tech

- **Clustering:** the `hdbscan` Rust crate (pure Rust, k-d tree backend) or `petal-clustering`. Both are production-grade in 2026; pick whichever has cleaner API ergonomics under review. Run on USearch-extracted vectors — no new ML pipeline.
- **Labeling:** existing chat/LLM provider, single short prompt per cluster. Cache aggressively; regenerate on a stable cluster-fingerprint change only.
- **Persistence:** new SQLite table `corpus_clusters` (id, fingerprint, label, description, generated_at) and `corpus_cluster_members` (cluster_id, doc_id, membership_score). Rebuild on background tick.
- **Trigger cadence:** rebuild full clustering on (a) initial ingest, (b) every +N% doc count growth (default 10%), (c) explicit "Rebuild collections" command in the palette. Don't rebuild on every doc add — it's flicker.

### 4.5 Sizing — when does shape become interesting?

- **<50 docs:** show only the type facet. A flat list is faster than thinking about clusters. Hide the Collections toggle entirely.
- **50-200 docs:** clustering produces 3-8 clusters; useful as a "growing into shape" preview but not yet a primary surface.
- **200-2,000 docs:** the sweet spot. Clusters are large enough to be meaningful, few enough to scan.
- **2,000-10,000 docs:** still works with `min_cluster_size` tuned upward; hierarchy starts to matter (consider Phase 4: cluster-of-clusters).
- **>10,000 docs:** beyond Phase-1 ambition. At that scale, faceted filtering (type × tag × cluster) does more work than a flat cluster list.

The threshold for *showing* the Collections toggle: ~50 docs. Below that, type counts and a simple file list are the right answer.

---

## 5. Key risks

1. **Bad labels poison the entire feature.** A cluster called "miscellaneous" or "documents about things" is worse than no clustering. Mitigation: hold the label-quality bar high; if the LLM returns a generic name, fall back to "Cluster of N" + the three preview titles and let the user rename. Never ship a label the model itself doesn't sound confident about.
2. **Cluster instability across rebuilds.** "Thai dishes (47)" becoming "Asian cooking (62)" on the next ingest reads as the app forgetting things. Mitigation: stable fingerprint hashing, label caching, and on rebuild prefer to *split* or *merge* clusters with the old labels intact rather than reshuffling whole-vault.
3. **Mixed-corpus collapse.** A vault that's 80% recipes + 20% notes will produce mostly recipe clusters and a noise tail. The user sees a homogeneous list and concludes "this doesn't see my notes." Mitigation: combine with the type facet; surface "Notes (43) — too few to cluster yet" as an honest message rather than letting them drown.
4. **The feature looks impressive but isn't used.** This is the Obsidian-graph failure mode in a different costume. Mitigation: track click-through. If clusters are visited <10% as often as folders after a month of use, the feature isn't earning its rail space — collapse it to a one-line "Browse by topic →" link in the folder tree.
5. **Latency at ingest.** Per-doc tagging (Phase 2) adds an LLM call per ingested doc. For batch imports of 1000+ docs this is painful. Mitigation: queue and rate-limit; show a non-blocking "Tagging in background" status in the corner; never gate a doc's appearance in the list on its tag completion.
6. **Re-clustering cost on a 10k-doc vault.** HDBSCAN over 10k 768-dim vectors is fast (sub-second on modern CPUs) but UMAP preprocessing — if BERTopic-style — adds seconds. Mitigation: skip UMAP for the MVP; cluster directly on USearch embeddings with cosine distance. Quality loss is small at this scale.
7. **The "auto-organize" failure mode.** Mem and DEVONthink both have features that *move* docs into auto-suggested groups. That's a feature death wish for a local-first tool; the user's mental model is "my files are where I put them." Recall's collections must be a **read-only overlay** — they index but never relocate.

---

## 6. Sources

- [Pinterest Engineering — Using machine learning to auto-organize boards (PinSage + Ward clustering + annotation labels)](https://medium.com/pinterest-engineering/using-machine-learning-to-auto-organize-boards-13a12b22bf5)
- [Apple Machine Learning Research — Recognizing People in Photos Through Private On-Device Machine Learning (HAC over face embeddings)](https://machinelearning.apple.com/research/recognizing-people-photos)
- [Apple Machine Learning Research — Learning Iconic Scenes with Differential Privacy](https://machinelearning.apple.com/research/scenes-differential-privacy)
- [Mem — Automatic Organization with Collections](https://get.mem.ai/blog/automatic-organization-with-collections)
- [Mem — New & Improved AI-Suggested Collections](https://get.mem.ai/blog/new-and-improved-collections)
- [BERTopic — Clustering documentation (HDBSCAN defaults and tuning)](https://maartengr.github.io/BERTopic/getting_started/clustering/clustering.html)
- [BERTopic — Best Practices](https://maartengr.github.io/BERTopic/getting_started/best_practices/best_practices.html)
- [HDBSCAN — Parameter Selection (min_cluster_size guidance)](https://hdbscan.readthedocs.io/en/latest/parameter_selection.html)
- [arXiv — Summaries as Centroids for Interpretable and Scalable Text Clustering (k-LLMmeans)](https://arxiv.org/pdf/2502.09667)
- [arXiv — HERCULES: Hierarchical Embedding-based Recursive Clustering Using LLMs](https://arxiv.org/html/2506.19992)
- [Towards Data Science — Advanced Topic Modeling with LLMs](https://towardsdatascience.com/advanced-topic-modeling-with-llms/)
- [Apple — Embedding Atlas (UMAP-based interactive embedding viz; reference for what NOT to ship as primary surface)](https://github.com/apple/embedding-atlas)
- [crates.io — `hdbscan` Rust crate](https://crates.io/crates/hdbscan)
- [crates.io — `petal-clustering` (DBSCAN/HDBSCAN/OPTICS in Rust)](https://crates.io/crates/petal-clustering)
- [DEVONthink — AI Classification & "See Also" forum thread](https://discourse.devontechnologies.com/t/dt3-auto-group-auto-classify/46584)
- [Zotero — Collections and Tags documentation](https://www.zotero.org/support/collections_and_tags)
- [Readwise Reader — Organizing Content](https://docs.readwise.io/reader/docs/organizing-content)
- [NotebookLM — 2026 sources panel and Mind Map updates](https://workspaceupdates.googleblog.com/2026/03/new-ways-to-customize-and-interact-with-your-content-in-NotebookLM.html)
- [Spotify Engineering — Algotorial Playlists (audio-feature clusters + editorial naming)](https://engineering.atspotify.com/2023/04/humans-machines-a-look-behind-spotifys-algotorial-playlists)
- [Createbytes — Netflix UX strategy (rows-of-rows, "Because you watched")](https://createbytes.com/insights/netflix-design-analysis-ui-ux-review)
- [Apple Support — Organize with collections in Books on Mac](https://support.apple.com/guide/books/organize-with-collections-ibks33867842/mac)
- [Eagle — Smart Collections / Smart Tagging](https://en.eagle.cool/)
- [Raindrop.io — Collections](https://help.raindrop.io/collections)
- [Literature & Latte — Organize Your Scrivener Project with the Corkboard](https://www.literatureandlatte.com/blog/organize-your-scrivener-project-with-the-corkboard)
- [Google Developers — EmbeddingGemma (on-device 768→128-dim embedding model for ingestion pipelines)](https://developers.googleblog.com/introducing-embeddinggemma/)
