# Research harness comparison, 2026-10-25

Josh asked whether open-source research and verification harnesses do anything that would make
Lattice's course research and claim verification better. This compares them, feature by feature,
against what the 2026-10-19..21 run data showed going wrong (the review behind
[toolbelt §6.7](2026-10-18-source-finding-toolbelt.md)).

**The failures in that data:**
- 550 of 1,130 claim checks (49%) ended `insufficient_standing`;
- 197 claim checks were unsupported, and 85 of them gave "not explicitly stated" as the reason;
- saved sources were mostly SEO sites;
- one reference selection took 4.5 minutes to pick 3 pages out of 227 candidates;
- 185 of 460 research events ran no query.

It does not repeat the [2026-10-12 audit](2026-10-12-llm-stack-sota-audit.md) or the
[2026-10-19 refresh](2026-10-19-sota-refresh.md). Those cover the chat agent loop, inference and
retrieval, and the 2026-10-12 [claim-checker bake-off](../../evals/grounding/2026-10-12-CLAIM-CHECKER-BAKEOFF.md)
already settled the checker model.

## What was compared

Stars and last push are as of 2026-10-25, from the GitHub API. Each harness was read from its
README and, where a claim mattered, its code.

| Harness | Stars | Last push | Closest Lattice stage |
|---|---|---|---|
| [STORM / Co-STORM](https://github.com/stanford-oval/storm) | 31.7k | 2025-09-30 | outline research |
| [GPT Researcher](https://github.com/assafelovic/gpt-researcher) | 30.3k | 2026-10-10 | reference selection |
| [PaperQA2](https://github.com/Future-House/paper-qa) | 9.5k | 2026-10-25 | evidence selection, provenance |
| [Ai2 ScholarQA](https://github.com/allenai/ai2-scholarqa-lib) | 0.3k | 2026-10-13 | lesson writing from evidence |
| [OpenScholar](https://github.com/AkariAsai/OpenScholar) | 1.7k | 2025-08-13 | research, repair |
| [WikiChat](https://github.com/stanford-oval/WikiChat) | 1.6k | 2026-01-31 | claim inventory, checking |
| [SAFE](https://github.com/google-deepmind/long-form-factuality) (DeepMind) | 0.7k | 2026-06-18 | per-claim research |
| [Loki](https://github.com/Libr-AI/OpenFactVerification) | 1.2k | 2024-10-03 | claim checking |
| [node-DeepResearch](https://github.com/jina-ai/node-DeepResearch) (Jina) | 5.2k | 2026-05-01 | research loop |
| [Open Deep Research](https://github.com/langchain-ai/open_deep_research) (LangChain) | 12.7k | 2026-08-10 | research loop |
| [hyperresearch](https://github.com/jordan-gibbs/hyperresearch) | 4.1k | 2026-10-10 | whole pipeline |
| [deepdive](https://github.com/Socialpranker/deepdive) | 0.4k | 2026-10-25 | whole pipeline |
| [reverify](https://github.com/2akouwu/reverify) | 1.3k | 2026-10-01 | verifier design |

DeerFlow (84k), Tongyi DeepResearch, MiroThinker and smolagents were looked at and set aside.
They are general agents or RL-trained models. The 2026-10-12 audit already covers why that
does not fit a single-slot local server.

## Where Lattice is already at or past the field

No open harness found here has Lattice's claim-dependent source authority:
- trust bars per claim kind;
- origin clustering and copy detection, calibrated on `independence-pairs-v1`;
- best-available sufficiency;
- injection detection over source metadata.

The closest are hyperresearch's independence audit and deepdive's `root:` field (§2.2), both
prompted, not rule-based.

**Three other features match the best harnesses:**
- **Section-scoped repair** (`repair.rs`). It edits through `text_edits` inside
  `confine`d protected regions, carries `repairHistory` so rejected variants are not
  reintroduced, and reads code-execution observations. This is hyperresearch's "tool-locked
  patcher that can only apply surgical edits", enforced in code rather than by instruction.
- **Verbatim-quote citations checked after writing.** This is the gate hyperresearch and
  deepdive call cite-check: a hallucinated quote is a hard block.
- **Labelled evals** for the checker (`claim-pairs-v1`), provenance (fixture tests) and
  whole courses (the course eval's mechanical checks and rubric judge, with a calibration slice).
- **The checker is gated like a classifier.** This is what reverify does with known-true and
  known-false claims, failing CI if one wrong claim is VERIFIED (0 of 475 known-false).
  - `gate_tests.rs` runs the dataset's own pairs through the lexical gate in CI, one test for
    each way the bake-off's 17 false passes went wrong.
  - Human verdicts on flagged lesson claims export as new labelled pairs
    (`claim_ledger_tests.rs`), so the set grows from real use.

## Gaps, ranked by the failures they address

### 1. Rank candidates before the model selects (selection latency, SEO noise)

**What others do:**
- **ScholarQA** reranks retrieved passages with mxbai-rerank-large-v1, keeps the top 50
  (`n_rerank`), and only then calls an LLM.
- **PaperQA2** runs "re-ranking and contextual summarization" (RCS): each retrieved chunk
  is summarised against the question and scored before any reaches the answer model.
- **GPT Researcher** replayed 28 research tasks with only the context filter changed
  ([benchmark](https://github.com/assafelovic/gpt-researcher/tree/master/evals/context_filter)).
  - A usefulness-scoring model kept 73% relevant passages, against 46% for embedding similarity.
  - Plain BM25 kept 51%, so it also beat embeddings, and it runs locally.
  - The scoring model is TypeSafe's Jev, a vendor API, and 28 tasks is small.

**Lattice:**
- `select_references` gives one model call every candidate. The worst seen was 227
  candidates, with 4.5 minutes to choose 3, and two of the three chosen were a raw
  `dispatch.py` and a 25-word page.
- The app already ships Qwen3-Reranker-0.6B (`features/search/engine/reranker.rs`), a
  1.19 GB download from Settings, Search, "Rerank results". It is used only to search the
  user's own documents, and is off there by default.
  - The 2026-10-12 paired bootstrap on v3 measured nDCG@5 +0.048 [+0.002, +0.091] and
    recall@5 +0.015 [-0.027, +0.059], at 5.6 s per query and 3.8 GB on an M3 Max
    (`evals/retrieval/2026-10-24-EVAL-JOURNEY.md`).
  - In sourcing it would replace part of a model call that took minutes, so the same cost
    is small there.

**Do:** rank the candidates locally by BM25 plus the reranker against the gap's claim text,
drop pages under a length floor, and give the selector the top ~25. This is the "candidate
cap and rerank" item already in the queue, and it needs no new model.

### 2. Decide ownership from records before asking a model (`insufficient_standing`)

**What others do:**
- PaperQA2 pools Crossref, Semantic Scholar, OpenAlex and Unpaywall metadata: citation
  count, journal quality, licence and a retraction check.
- hyperresearch makes an unacknowledged retraction a hard block at its gate.

**Records that settle "who owns this page" without a model, checked live on 2026-10-25:**
- **GitHub orgs.** `GET /orgs/{org}` returns `is_verified` and the org's website.
  `anthropics`, `openai`, `microsoft`, `modelcontextprotocol` and `pallets` are all verified,
  with websites `anthropic.com`, `openai.com`, `opensource.microsoft.com`,
  `modelcontextprotocol.io` and `palletsprojects.com`. `is_verified` means the org proved at
  least one domain; the website field is not guaranteed to be that domain.
- **npm provenance.** `@anthropic-ai/sdk` carries attestations naming
  `github.com/anthropics/anthropic-sdk-typescript`.
- **Chained,** these link a package to its repository to its verified org to the publisher's
  domain. That is the "github.com is treated as one publisher" problem in the run data.

**Lattice's standing model, and what the research says about it:**
- Reliability is recorded and enforced. Each saved page gets a provenance record and three
  judgements: the publisher's standing, the origin's authority on the subject, and the
  claim's kind, which sets its bar. A claim passes only on support that meets its bar.
  `insufficient_standing` is that rule working; the problem is what the judgements are given.
- Every signal the standing judgement sees comes from the page itself: meta tags, JSON-LD
  publisher, site name and HTTP facts. No outside record, such as a verified GitHub org or a
  package's provenance, reaches it.
- The origin is the host. In the AI-engineering course, 73 saved pages share the single
  origin `web:github.com`, so `anthropics/…` and a stranger's repository are one publisher
  with one standing.
- The standing rows in that run are all `source-standing-v1`: 61 of 85 came back `unknown`,
  and one site got different answers. `source-standing-v2`, which also sees content
  evidence, has not run on these courses.
- [arXiv 2610.12788](https://arxiv.org/abs/2610.12788) (2026-10-08) tested 13 models on 500
  scenarios. Models that recognise relative source reliability equally well can differ from
  "negligible" to "complete choice reversal" in actually using it.
- In the same paper, on Qwen checkpoints:
  - restating the facts raised reliability sensitivity in both 27B models;
  - adding an explicit weighting principle raised it a further 39.2 and 47.2 points over
    structured evidence records alone;
  - the 9B model did not respond the same way.
- [arXiv 2608.20116](https://arxiv.org/abs/2608.20116) finds open models follow recency
  more consistently than explicit reliability cues.

**Do (for the trust-gate work):**
- Add verified-org, provenance-attestation and registry-URL records as ownership signals ahead
  of the standing judgement.
- Write the judgement's input as a structured evidence record with a stated weighting principle.
- For papers, read retraction status from Crossref, which the finders already call.

### 3. Search with what the last search found (zero-query events, thin evidence)

**What others do:**
- **SAFE.** Each fact gets up to five searches (`max_steps = 5`, 3 results each). Each query
  is written from the statement plus the KNOWLEDGE gathered so far (`rate_atomic_fact.py`).
- **node-DeepResearch** keeps a queue of gap questions, dedupes new ones, and stops on a
  budget.
- **OpenScholar** drafts, asks itself what is missing, retrieves and refines.
- **deepdive** stops after two rounds that add nothing new and records the open question
  instead.

**Lattice:**
- The research planner sees which queries already ran (`already_searched`), not what they
  returned.
- 185 of 460 research events ran no query.
- **Already remembered across rounds:**
  - a page the course has saved is never fetched again;
  - a query is never run twice;
  - a finder's results page is cached for 15 minutes.
- **Forgotten across rounds:**
  - candidates the selector read and turned down;
  - pages that could not be read or were unusable, such as login walls and near-empty pages.
  Both come back in the next round's results and are read and judged again.
- In the AI-engineering course, the whole run made 42 distinct searches. 12 claims went
  through 10 or more research rounds, and 205 of 460 rounds found no passage. One claim,
  "Users should not be shown raw internal errors as a final answer", went through 13 rounds,
  6 of them finding nothing, before the lesson was rewritten.

**Do:**
- Pass the planner a compact digest of what the last round found and why it fell short,
  such as "found only community pages" or "found the owner's page, but it does not state
  the limit".
- Remember turned-down and unreadable pages per claim, so they are not read again.
- Skip pages, not sites: an official site has many pages. The search for an independent
  second source already skips the sites a claim's support comes from (`excluded_sites`).
- Add the no-progress stop.

### 4. Search for what would overturn a claim

**What others do:**
- hyperresearch runs a corpus critic ("What source would overturn this?") with a targeted
  gap-fill fetch.
- deepdive makes opposition queries mandatory and flags input-level skepticism: a source that
  measures its own product or self-reports a benchmark.
- [arXiv 2210.13865](https://arxiv.org/abs/2210.13865) (older, 2022) shows fact-checking
  without counter-evidence retrieval is unrealistic.

**Lattice:**
- Research serves support gaps. The 28 contradicted verdicts came from passages found while
  looking for support.
- The `Measurement` trust bar wants a primary source, which is exactly where a vendor's own
  benchmark passes as primary.

**Do:**
- For `Measurement` and `VersionBound` claims, plan one refutation search per claim.
- Mark a measurement source that is the measured product's owner.

### 5. Let the judge accept what the evidence strongly implies (unsupported verdicts)

SAFE's rating prompt says: "The STATEMENT does not need to be explicitly supported by the
KNOWLEDGE, but should be strongly implied by the KNOWLEDGE." It reports 72% agreement with
crowd raters, and wins 76% of the disagreements it was audited on.

Lattice's strict prompt has one sentence allowing "ordinary synonyms, faithful paraphrases and
logically necessary consequences", and about twenty pushing towards unsupported. Among them:
"A plausible interpretation is not an explicit premise" and "do not use your own definitions
or domain knowledge to bridge different descriptions."

In the AI-engineering course, 85 of 197 unsupported checks gave "not explicitly stated" as
the reason. Those 85 cover 34 distinct claim wordings, classified by hand from each reason:

| Count | What happened | Example |
|---|---|---|
| 7 | The source says it in other words; it should pass | "`additionalProperties: false` prevents hallucinated fields" failed though the source says the setting forbids keys not in the schema |
| 5 | Borderline; probably should pass | "A standard function definition includes four key fields", beside passages showing exactly those four |
| 10 | Correctly failed: the claim overstates or the source disagrees | "you must use `Memory<T>`" when the source names arrays as an alternative; "returns plain text instead of a tool call" when the source says a reply can hold both |
| 11 | The passages do not hold the fact; a research problem, not the judge | "Span<T> on the heap is a compile error" is true, but no passage said so |
| 1 | The passage that states it was not assigned to the claim | "passage-20 and passage-36 … are not in the evidence list for this claim" |

So the judge wrongly fails about a third of these, and catches real overstatements in another
third.

**Do:**
- Accept strong implication and standard practitioner synonyms.
- Keep the strict reading for claims that state specifics (numbers, versions, limits) and
  for strength words (must, only, always, entirely), which is where the correct fails were.
- Add the 34 classified cases as labelled checker pairs. Gate the change on them, with no
  new false passes on `claim-pairs-v1` and `lesson-claims-w1`.

### 6. Claim dependencies and informativeness

reverify gives claims `depends_on`, so a refuted premise invalidates what was built on it. It
also gives a verified claim zero weight when it restates what the model was shown, duplicates
another claim, or is trivially true everywhere. That is the CORE refinement of FActScore:
credit only factual, informative, non-repetitive claims.

Lattice's claim ledger has neither, so a lesson's supported share can be padded by trivial
claims, and a repaired premise does not reopen its dependents.

**Do:** record a premise link in the inventory, and report the supported share over
informative claims.

### 7. Write from extracted quotes (later)

ScholarQA extracts exact quotes first, clusters them into the outline's sections, and writes
each section only from its quotes. Lattice's writer chooses quotes while writing, and they are
checked afterwards.

Writing from quotes would cut unsupported claims at the source. It is a lesson-writing
redesign, so it belongs with the learning-experience work, not here.

## Set aside

- **Multi-agent orchestration** (Open Deep Research's supervisor, hyperresearch's four
  parallel critics and triple drafts): on a single-slot local server, parallel agents give
  context isolation but no wall-clock gain (2026-10-12 audit §2).
- **Best-of-N with the verifier choosing** (reverify's `--samples`): the same cost problem;
  revisit for a cloud model.
- **Jev:** a proprietary API that would send page text off the device. BM25 plus the
  bundled reranker is the local equivalent to measure first.
- **DRACO** ([arXiv 2602.11685](https://arxiv.org/abs/2602.11685),
  [dataset](https://hf.co/datasets/perplexity-ai/draco)): rubric-graded deep-research reports.
  The course eval already grades with task rubrics, so DRACO adds no course signal.
