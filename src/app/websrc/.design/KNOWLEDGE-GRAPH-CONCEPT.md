# Knowledge Graph — Concept

**Status:** conceptual frame, pre-architecture, pre-design
**Paired with:** `AESTHETIC-GUIDE.md`. Sibling voice: `CHAT-REDESIGN-SPEC.md`, `JOURNAL-REDESIGN-SPEC.md`, `REFERENCE-REDESIGN-SPEC.md`.
**Not a spec.** This is the considered take that the architecture and design specs must honor.

---

## 1. The trap

Every personal knowledge tool eventually ships a force-directed graph view. Obsidian, Logseq, Roam's secondary plugins, Foam, Dendron, Org-Roam, and a dozen smaller entrants. They look, with small variations, the same: colored nodes, hairballing edges, a physics simulation letting the user drag blobs through a void.

They are almost never used after the first week.

The reason is not technical — rendering 2,000 nodes at 60fps is solved. The reason is that a force-directed graph of a personal knowledge base answers a question no one asks. "What does the shape of everything I've ever written look like?" is a vanity question. The useful question is closer to: **"What did I write about this thing, and what adjacent thinking have I done that I forgot?"** A hairball answers the first and hides the second inside the fuzz.

This is the single thing Josh should internalize before shipping: *the graph view is not a feature, it is a failure mode*. When it appears, it's usually a sign that the product couldn't figure out how to make its structure useful, so it rendered the structure literally and hoped the user would do the work. Obsidian's graph view is famous not because users love it, but because it's the thing that looks most impressive in a screenshot. It is the PKM equivalent of a marketing gradient — a thing that announces "look, structure!" without being structure a person can think inside.

Recall has already chosen, across Chat / Journal / References, to refuse this kind of performative chrome. The graph view has to earn its place under the same rules.

---

## 2. What the useful ones have in common

A handful of graph treatments across tools for thought earn their weight. The throughline is not the rendering; it is the **question being answered**.

- **Roam Research's anti-graph.** Roam famously de-emphasized the global graph and invested in *linked & unlinked references at the bottom of every page*. That is a graph — a 1-hop, content-relevant, prose-embedded graph. It showed up exactly where the user was thinking, about the thing they were thinking about. It's the most-used "graph feature" in PKM history, and it doesn't look like a graph.
- **Andy Matuschak's published notes.** His evergreen-note site renders backlinks as a *sidecar column of titled cards beside the open note.* No global map. He has explicitly argued against the overview graph as a thinking tool — the map is not the territory; the adjacent prose is. This is the most rigorous published position on the question.
- **Tinderbox and Kumu.** Both let the user *author layouts*. Nodes are positioned where the user puts them, not where the simulation throws them. The result is a diagram — it means something — rather than a cloud. This is a different tool entirely: the graph as *a thing the user makes*, not a thing the app renders about the user.
- **Bret Victor's "Up and Down the Ladder of Abstraction."** The governing principle: a single-level visualization lies, because all interesting phenomena have multiple scales. Useful visualization offers *movement between scales*, not a single god-view. A knowledge graph with one zoom level is epistemically wrong.
- **Code editors' "Find References" panel.** The most-used graph in software is the one engineers use fifty times a day and never think of as a graph. It shows the immediate neighborhood of a symbol, ranked by relevance, in prose form. Obvious lesson PKMs keep missing.

The pattern, abstracted: **the useful ones answer a specific question in the neighborhood of what the user is already doing; the useless ones render the global structure and hope.**

---

## 3. The single-view delusion

Should Recall's graph be one view or several modes? Several, and the modes are not toggles on a single canvas — they are *different surfaces*, born for different questions.

There are at least three legitimate questions a knowledge graph can answer for a personal KB:

1. **"What is adjacent to what I'm reading right now?"** — the *local* question. Answered best by a backlinks-and-mentions panel on every document, Roam/Matuschak-style. This is not the "graph view" at all; it lives inside the document surface. It is the most-used variant by at least an order of magnitude.
2. **"Where does this idea lead, two or three hops out?"** — the *focused exploration* question. Answered best by a radial or egocentric graph *seeded on a specific node*, showing only its neighborhood. Think: the "Find References" panel, but traversable. Never a global view.
3. **"What did I work on in April?"** — the *temporal / retrospective* question. Answered best by a timeline-as-x-axis projection: time flows horizontally, notes stack by topic cluster vertically, edges become light threads across time. This is a *different chart* that happens to share data with the graph.

A fourth, the global hairball, answers effectively nothing and should be the last thing built, if it is built at all. If it ships, it ships as an easter egg, not as the primary surface.

The "single canvas with filters" approach — the Obsidian default — is the wrong answer because filter sliders change *what is shown*, not *what question is being asked*. Three separate surfaces with clean identities beat one configurable canvas whose identity dissolves into its own settings panel.

---

## 4. The edge problem

Edges are where most knowledge graphs lie. Four kinds of edge appear in Recall's data:

- **Wikilinks** (`[[like-this]]`). Authored by the user. *Real*. The user asserted the connection.
- **Hashtag / mention cooccurrence.** Authored by the user as a side effect. Real-ish: the co-occurrence is intentional, the connection is inferred.
- **Embedding similarity** (cosine over USearch). *Inferred by a model.* Produces many false positives: two notes about totally different topics that share a stylistic register can cosine above 0.8. This is the edge category most likely to be wrong, and the one most tempting to render because it's dense and pretty.
- **Temporal co-mention** (written in the same journal entry, same chat). Weak evidence of connection, strong evidence of adjacency of attention.

The principled position: **edges of different kinds should not be rendered as the same line.** In most graph views, every edge looks identical, and the user has no way to tell an authored link from an inferred one. This is the central epistemic sin of semantic-similarity graphs — they launder vector math through line-drawing and sell correlation as connection.

Treatment in the Recall register:

- **Authored edges** (wikilinks, mentions) render as hairline `--border-default` lines. Confident, visible, first-class.
- **Embedding-similarity edges** do not render as lines at all. They render as a *proximity signal* — neighbors appear physically closer, or appear in a "you might also look at" list, but no line is drawn. A line asserts; a list suggests. The distinction matters.
- **Temporal edges** render only in the temporal view, where time is the x-axis and "same session" is self-evident from co-location.

This is editorially honest. It is also technically easier: embedding-similarity requires no graph rendering at all, just a nearest-neighbors query we already have.

---

## 5. Density, the real ceiling

The technical ceiling is tens of thousands of nodes at 60fps. The *useful* ceiling is dramatically lower and worth naming.

A personal knowledge base is readable as a graph up to roughly 50–150 nodes in view at once. Beyond that, the eye cannot disambiguate nodes, labels must hide, and the visualization reverts to abstract-art-with-mouse. Obsidian's graph of a 5,000-note vault is a Jackson Pollock with a cursor; informative only in the trivial sense that "the vault is big."

This means the global view, *if it exists*, must aggressively cluster or sample. More likely it means the global view should not exist as a primary surface, and every meaningful graph surface should be seeded — on a note, on a tag, on a date — so that the neighborhood stays inside the readable envelope.

The editorial-restraint framing here is congruent with the performance framing: the useful ceiling and the aesthetic ceiling arrive at the same answer. Do not render everything. Render a neighborhood.

---

## 6. What interaction is this, actually

Among Navigate / Browse / Edit / Contemplate — the graph in Recall should be **primarily a navigation tool**, secondarily a browse tool, and explicitly not a contemplation tool. No one needs a screensaver.

- *Navigate*: clicking a node opens the document. This is the primary action. The graph is a spatial index.
- *Browse*: hovering a node reveals a short preview (title, 2-line excerpt, last-edited). This is the "see neighborhood without committing" mode.
- *Edit* (no). Dragging nodes to rearrange is seductive and teaches the user nothing. Recall's edges are derived from data (wikilinks in the prose, tags in the document); editing the graph means editing the prose.
- *Contemplate* (no). The graph is not a wall poster.

The corollary: every node must have a keyboard path (`/` to search, arrow keys to step through neighbors, `Enter` to open). If the graph is a navigation tool, it cannot require a mouse. This follows AESTHETIC-GUIDE §2.7 (keyboard-first) and matches the Chat / Journal / References pattern already established.

---

## 7. The aesthetic constraint

Every existing graph view is rainbow-blob territory. How do we do a graph in the Linear/Stripe Press/Arc register?

A small set of concrete moves:

1. **Nodes are labels, not shapes.** Render the note's title as text. No circle. No colored dot. The title, set in sans at `text-xs`, is the node. This is radically different from every graph view in the PKM space and immediately establishes the register. It is also how architectural diagrams in scientific papers look (cf. the Kang et al. cognitive-load paper — text nodes outperform shape-with-label nodes on recall tasks).
2. **Edges are hairlines at `--border-subtle`, one weight.** No arrowheads unless direction is semantically meaningful (it isn't for wikilinks). No colored edges. Opacity carries age (recent connections darker, old connections lighter) if and only if age matters in context.
3. **One accent, used once.** The currently-focused node uses `--accent`. Every other node is `--text-primary` or `--text-tertiary`. No category colors, no type colors, no cluster colors.
4. **Typography is the structure.** Node text weight indicates importance (high-connection nodes at weight 500, leaves at weight 400). Size stays constant — changing size makes the layout stressful.
5. **Motion confirms causality, never ambient.** No physics simulation jiggle at rest. Nodes are where they are. On interaction (hover, focus, new-node-enters-view), a single `--duration-base` transition. The moment the graph jiggles on its own, it's decoration.
6. **The canvas has a baseline grid, subtly.** A 1px grid at `--border-subtle / 2` opacity, or none at all. This is the "visible grid, unfinished-feeling" move from the aesthetic guide §4 — it says the surface is a working draft, not a polished diorama.
7. **Labels never overlap.** A collision-avoidance layout pass matters more than a pretty force-direction pass. If labels would overlap, one collapses to a dot-with-tooltip. Readability is the design.

This leaves a graph that looks like a page from a physics textbook, not a lava lamp. That is the target.

---

## 8. The Obsidian-graph specific correction

If Obsidian's graph is beautiful-for-screenshots and useless-in-practice, what do we do differently?

- **Seed, don't globe.** Every graph surface is entered *from* a note or *from* a query. There is no "Graph" tab in the nav. The graph is accessed via a key (`⌘G`) on the current document and shows *that document's neighborhood*. This is the single biggest structural break from Obsidian.
- **Two hops, then stop.** Default radius is 2 hops. More hops are a conscious expansion action, not a default. The Kang et al. paper's cognitive-load finding: recall drops precipitously past 2-hop neighborhoods in egocentric graphs.
- **Show the text, always.** Every node renders its title. Tooltips carry a 2-line excerpt. No floating nameless dots.
- **Derive, don't author.** The user does not arrange the graph. They author prose; the graph is its shadow. If the graph looks wrong, the prose is wrong, and fixing the prose is the product's job.
- **Invite exit.** Every node is a navigation affordance. The graph is a hallway to the rooms, not a room. Users should spend seconds on the graph surface, not minutes.

---

## 9. Three conceptual commitments

The architecture and design specs downstream must honor these. They are non-negotiable framings; everything else is tradeable.

1. **The graph is always seeded, never global.** There is no "show me everything" canvas as a primary surface. Every graph view is entered from a specific node, tag, or query, and shows a bounded neighborhood (default 2 hops, readable envelope ≤ ~100 nodes). A global-graph easter egg may exist; it is not the product.

2. **Authored edges render as lines. Inferred edges never do.** Wikilinks and explicit mentions become hairline connections. Embedding-similarity relationships surface only as a "nearby in meaning" list, never as drawn lines. Recall refuses to launder vector math as structure.

3. **Nodes are typography, not shapes, and the graph reads as a page from a physics textbook — not a lava lamp.** Text labels at one weight scale and one color scale (two, counting the single accent for focus). No category colors, no physics jiggle at rest, no motion without a cause. If it looks like every other PKM graph, it is wrong.

---

## 10. One non-obvious reference

Not Obsidian. Not Roam. Not Kumu.

Look at **the source-navigation view in Sourcegraph** (or, equivalently, the "Call Hierarchy" panel in a serious IDE like JetBrains or Sublime's LSP). It is the most-used knowledge graph in professional life, and it is not called a graph. It shows a specific symbol's callers and callees, two hops deep, in a tree — typography only, no shapes, no physics, keyboard-navigable, always seeded from the user's cursor. It is the answer to *"what connects to this thing I'm looking at?"* given without ceremony.

That's the target. Not a map of the world; a tool that answers the question the user is actually asking, in the neighborhood of their attention, without asking them to admire it.
