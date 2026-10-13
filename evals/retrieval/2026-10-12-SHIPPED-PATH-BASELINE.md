# Shipped-path baseline, 2026-10-12 program (run 2026-10-13)

This is the first measurement of what a Lattice desktop user gets from chat,
taken through the same orchestrator the desktop runs. Every earlier number
measured a component. The retrieval eval's production mode skips the planner, HyDE,
openings, sections, the retry, scoping and the answer. `chat_rag_eval.py` talks
to a bare provider endpoint with whole documents.

**It is the pre-upgrade baseline.** It runs `origin/main` at `70cc8c92`, before
the 2026-10-12 LLM-stack program changes thinking and sampling, prompt
layout, the tool loop or retrieval defaults. Re-run the same command after
integration and compare like for like (see [Reproduce](#reproduce)).

**Headline (Qwen3.5-9B Q4_K_M, local, 16K window).**

- **Pass rate.** 20 of 34 items pass the rubric judge at pass^3 (0.59; Wilson
  95% CI 0.42–0.74), and 22 of 34 pass the mechanical checks.
- **Latency.** The median is 28 s from question to answer, and the grounding
  check lands a median 20 s after that.
- **The planner never planned.** Its catalog prompt either overflows the
  window or misses its 20 s timeout, so the corrective retry and openings never
  ran either.
- **The budget fails turns outright.** It fails 3 of 44 turns at 16K, and 31 of
  34 items at 8K.
- **Questions go to the web.** The intent step sent 4 to 6 of 41 vault
  questions to the public web, and 25 of 38 on the v1 set.

Details and evidence are under [Results](#results) and
[Defects](#defects-found).

## Method

### What runs

`src-tauri/examples/shipped_path_eval` builds the app headlessly:

1. It writes `settings.json` from `SettingsDto::default()` plus the recipe's
   overrides, then builds `Container::new` on a fresh SQLite database.
2. It activates the embedding model through the Settings command
   (`set_active_embedding_model_impl`), which establishes its artifact
   identity and prepares its vector generation.
3. It imports every corpus document through `IndexFileUseCase` with the batch
   importer's chunking (semantic, 800 tokens). Each document is written as a
   Markdown file named after its title, since dataset ids would hand BM25 a
   hint no real vault has.
4. It asks every question through `chat::headless::run_headless_turn`, which
   is the same `run_turn` the desktop window command calls.

So intent classification, routing, the corpus planner, HyDE, hybrid search,
the sufficiency check and corrective retry, openings and sections, the tool
loop, answer generation, the background grounding check and memory
consolidation are all the shipped code. Composer preferences are the
desktop's defaults (`useComposer.ts`): no switch forced on, `turnMode: auto`,
so the intent step decides whether the vault is searched.

The model is reached through a recording proxy inside the runner. The proxy
keeps every request verbatim (`*.calls.jsonl`) and reads llama-server's own
`timings.cache_n` / `prompt_n` (cached vs evaluated prompt tokens) from each
reply, without touching the LLM adapter. The run's `HOME` points at a scratch
directory, because importing copies files into `~/.lattice/files`.

Between turns the runner waits for the grounding check to land and for the
wire to go quiet (memory consolidation), so one turn's background work does
not leak into the next turn's timings.

### Recipe

| | |
|---|---|
| Recipe | `evals/retrieval/shipped-path-recipe-9b-local.json` |
| Corpus | `synthetic-library-v3.json`: 379 documents, imported as 2,264 chunks |
| Questions | `desktop-path-v1.json`: 34 items, 44 turns, 9 multi-turn conversations |
| Chat model | Qwen3.5-9B Q4_K_M (Unsloth), sha256 `03b74727…b7e8` (matches the Hugging Face LFS pointer and the 2026-09-25 run) |
| Server | bundled llama-server b11429 (v0.6.0 pin), `-ngl 99 --ctx-size 16384 -np 1 --jinja`, port 8089 |
| Utility role | the same 9B on the same server (see below) |
| Embedding | Qwen3-Embedding-0.6B on Metal; default 256-d/i8 vector compression |
| Thinking | off: the proxy adds `chat_template_kwargs.enable_thinking: false` to requests that do not set it |
| Settings | shipped defaults, except `llm.contextWindow = 16384` to match the server |

Three choices need a word.

- **Utility role.** The corpus planner and the corrective retry run *only*
  when a utility model is assigned (`chat/retrieval/kb_retrieval.rs:159`,
  `:525`). With the roles at their install defaults the turn uses a local
  plan and never retries; HyDE, intent and the claim judge fall back to the
  chat model. The baseline assigns the utility role to the same 9B, the way
  Settings → Model roles does for a server-hosted model, so that these stages
  can run. A second run with default roles is reported beside it.
- **Thinking off.** The pre-upgrade code sends no thinking switch, and this
  llama-server build then turns thinking on for the Unsloth template. A spot
  check gave 526 reasoning characters to answer "17×3" with no switch, and 0
  with the switch. The program's coordinator asked for thinking off in
  the baseline, so that is what it measures. Set `server.thinking` to
  `"as-sent"` to measure the untouched requests.
- **16K window.** The desktop sidecar sizes its window automatically, up to
  32,768. The first attempt used 8,192 to save memory and could not answer at
  all (see [Defects](#defects-found)), so the baseline uses 16,384.

### Grading

`scripts/shipped_path_eval.py score` grades each turn two ways.

- **Mechanical checks.** Every required fact is present, matched as a
  case-insensitive substring with any listed alternative accepted. No forbidden
  claim appears. Abstention matches the rubric. Every cited `[n]` exists in
  the turn's citation map. Every expected document is among those cited.
  Each check also records whether the expected documents were in the evidence
  the model was given, which separates retrieval misses from generation misses.
- **Rubric judge at pass^3.** The same 9B grades the answer against the rubric
  and the item's notes three times, with a fixed prompt, `temperature 0`,
  `top_k 1`, a fixed seed, thinking off and a JSON schema. A turn passes only
  if all three grades pass.

An item passes only if every one of its turns passes. **The judge is the same
9B that wrote the answers, so its verdict is a smoke-level signal, not a
calibrated one.** The mechanical checks exist to cross-check it, and every
disagreement is listed below.

pass^3 here means three grades of one answer. tau-bench's pass^k means k
independent runs of the system. That needs three runner invocations, which
this baseline did not make.

## Results

### Pass rates

34 items and 44 turns per arm, one app run each. The utility-role arm is the
baseline. The default-roles arm is the same recipe with no utility model.

| tag | items | utility role: mechanical | utility role: judge pass^3 | default roles: mechanical | default roles: judge pass^3 |
|---|---:|---:|---:|---:|---:|
| **overall** | 34 | 22/34 | **20/34** | 21/34 | 20/34 |
| multi-hop | 4 | 2/4 | 2/4 | 2/4 | 2/4 |
| planner-rewrite | 5 | 3/5 | 3/5 | 3/5 | 2/5 |
| corrective-retry | 5 | 3/5 | 3/5 | 3/5 | 2/5 |
| opening-or-section | 5 | 1/5 | 1/5 | 1/5 | 1/5 |
| abstain | 6 | 6/6 | 5/6 | 5/6 | 5/6 |
| conflict | 7 | 6/7 | 5/7 | 5/7 | 5/7 |
| numeric-precision | 27 | 19/27 | 18/27 | 18/27 | 17/27 |
| follow-up-memory | 8 | 5/8 | 4/8 | 3/8 | 3/8 |
| table | 3 | 3/3 | 3/3 | 3/3 | 3/3 |
| ocr | 1 | 0/1 | 0/1 | 0/1 | 0/1 |
| multilingual | 1 | 0/1 | 0/1 | 0/1 | 0/1 |
| entity-collision | 1 | 1/1 | 1/1 | 1/1 | 1/1 |
| fragment | 1 | 0/1 | 0/1 | 1/1 | 1/1 |

Judge pass^3 is 0.59 overall in both arms, with a Wilson 95% interval of
0.42–0.74. With 34 items, a difference under about 25 points between two runs
is noise. The two arms differ by one or two items per tag, so read them as the
same result. As the next section shows, they ran the same retrieval.

**Judge against mechanical checks.** The judge was stricter on 3 items and
looser on none. It caught two real failures the substring checks miss, both
answers that contain the right figure but then say the documents do not
answer the question (`dp-conflict-receipt-threshold`,
`dp-paraphrase-sick-child`, the latter in the default arm). It also failed one
correct turn by reading the item's explanatory notes as required facts
(`dp-mt-abstain-followup` turn 2; that item fails on turn 1 anyway). The three
greedy grades agreed on every turn, so at `temperature 0` pass^3 equals pass^1
here. It only guards against server nondeterminism, which did not occur.

### What actually ran

Tags say what an item was written to exercise. Each turn's record says what
ran.

| | utility role | default roles |
|---|---:|---:|
| Turns answered (of 44) | 41 | 41 |
| Turns failed by the prompt budget | 3 | 3 |
| Corpus-planner requests answered | **0 of 16** | not attempted |
| Corrective retries that ran | 0 | 0 |
| Document openings chosen by the planner | 0 | 0 |
| Turns where intent added web search | 4 | 6 |
| Tool calls by the model | 0 | 0 |
| Model calls per turn (median) | 6 | 6 |
| Grounding-check verdicts | 57 supported, 31 unsupported, 1 contradicted, 12 unverified | 59 / 34 / 0 / 18 |

The planner never produced a plan. Of its 16 requests, 4 timed out at the
20-second `PLANNER_TIMEOUT` and 12 were rejected by the server as larger than
the 16,384-token window. 26 of the 41 answered turns made no attempt because
a timeout had stood the planner down for 10 minutes. Without a plan there are no
openings, and the corrective retry cannot replan, so both arms ran the local
plan. A section lookup by identifier (`24.111`) runs from the question text
whether or not there is a plan. A source carries no tag saying whether it
came from search, an opening or a section, so a row cannot show whether that
lookup matched. The turn records still show a `corrective_search` step
(5 and 7 turns), while the trace's `kbCorrectiveRetries` is 0: the step marks
an attempt whose planning failed.

### Timings and cache

Utility-role arm, per answered turn, from the turn's own flow metrics:

| | median | p95 |
|---|---:|---:|
| Question to answer | 28.4 s | 61.5 s |
| Retrieval pipeline | 0.5 s | 20.5 s (the planner timeout) |
| Generation | 23.0 s | 43.6 s |
| Grounding check after the answer | 19.7 s | 46.0 s |

The default-roles arm is faster: 21.1 s median and 42.9 s p95 to the answer,
with retrieval p95 at 3.6 s. With a utility model set, the planner that never
plans costs the user up to 20 s on a turn.

Generation is mostly prefill. On the answer request the server reused a
median 2,126 cached prompt tokens (the system policy and tool definitions) and
evaluated 5,459 fresh ones in a median 17.2 s, at about 300 tokens/s. Across
all model calls, 33% of prompt tokens came from cache (157,916 cached against
317,464 evaluated). Each turn makes about six model calls: intent, the answer,
memory extraction, the working summary, and one claim check per checked
sentence (71 claim checks over 41 turns), plus the planner when it is not
standing down.

### Failure notes

Each note is backed by the row's evidence list and turn record.

- **Retrieval misses (the expected document never reached the model).**
  - `dp-mt-runbook-ticket`: "Who owns the runbook for ERR-4070?" The core
    runbook carrying that exact code was not in the evidence. The model
    answered from ERR-8747's runbook and declined, and turn 2 inherited the
    miss.
  - `dp-multihop-dunlark-notice`: the project brief was retrieved, but the
    supplier's contract (second hop) was not, so the model declined.
  - `dp-multihop-part-undertaking`: the purchase order was retrieved, the
    Tessivar contract was not. The model named the *Calderwick* contract's
    Floandgate undertaking for Tessivar's order. The answer is confident and
    wrong.
  - `dp-section-after-clause`: "clause 24.111" did not bring back the
    paragraph after it, and the model quoted a different agreement.
- **Evidence present, answer wrong.**
  - `dp-buried-agreement-meeting`: the 87k-character agreement was retrieved,
    but not the paragraph holding the 31-week meeting. The model said no such
    meeting exists.
  - `dp-ocr-stock-reference`: the right scan was in the evidence. The model
    could not tie "scanned minutes 05" to it and declined.
  - `dp-opening-agreement`: no opening was read (no plan), so the model
    guessed the first section from mid-document chunks.
- **Web dilution.** On 4 to 6 vault questions per arm, intent turned on web
  search and up to 5 web pages joined the vault passages. In two answers the
  web pages changed what the model answered: IRS receipt rules beside the
  vault's receipt policy, and US state sick-leave laws beside the dependant
  allowance.
- **Follow-up drift.** In `dp-mt-encryption-versions` turn 3 ("Is there any
  way round it?"), retrieval answered from a different policy's exception.
  The default arm instead hit the budget error on that conversation.
- **Budget failures.** These are listed under [Defects](#defects-found).

## Defects found

1. **The prompt budget cannot trim evidence; at 8K nothing fits** (P1). At
   `--ctx-size 8192`, 31 of 34 items failed before generation with
   `PromptBudgetExceeded` (`context_assembler/budget.rs:432`): the fixed part
   needed 5,847–8,691 tokens (median 6,733) against the 5,734 allowed (70% of
   8,192). At 16,384, 3 turns per arm still failed (11,606–13,250 tokens
   against 11,468). The fixed part is system policy, tool definitions, the
   current message and the evidence rendered into it. The evidence pool is
   sized before assembly without reserving room for the tool definitions
   (about 2,100 tokens with the policy), so when it overflows nothing can be
   trimmed and the turn fails. The sidecar auto-sizes its window up to 32,768,
   so a machine whose auto-size lands near 8K cannot answer at all, and one
   near 16K fails on long documents. Fix: reserve the policy and
   tool-definition tokens before sizing evidence, or shrink evidence first when
   the plan overflows. Run:
   `results/chat/qwen3.5-9b-local/2026-10-13-shipped-path-desktop-path-v1-ctx8192.jsonl`.
2. **The corpus planner cannot run on a 9B with a 379-document vault** (P1).
   Its prompt carries the whole catalog (53–62k characters), sized with a
   chars/4 estimate. The server rejects it as too long for the window, or the
   9B cannot prefill it inside the 20-second timeout, which stands the planner
   down for 10 minutes. The corrective retry needs the same planner, so it
   never runs either. Fix: budget the catalog with the server tokenizer the
   scheduler already has, cap it (shortlist by search first), and don't let a
   timeout disable the retry's replanning.
3. **Intent sends vault questions to the web** (P2). The enable-only intent
   step added web search on 4 to 6 of 41 turns per arm. Questions about the
   user's own policies left the machine as public search queries, and the web
   pages diluted or overrode the vault evidence in at least two answers.
4. **No tool calls in 82 answered turns** (P3, observation). Tools are
   offered on every answer request (about 2,100 cached tokens with the
   policy), but the model answered first time on every turn, including
   retrieval misses where `get_document` or a second `semantic_search` would
   have recovered the second hop.
5. **A `corrective_search` step is recorded when no retry ran** (P3). The
   trace's `kbCorrectiveRetries` is 0. Readers of the turn record should use
   the trace.
6. **An exact error code missed** (P3). `ERR-4070` did not retrieve its own
   runbook. The retrieval eval's `v3-id-runbook-bare-00` measures that path
   directly.

## v1 comparison

The same runner ran the 38-question `synthetic-library-v1.json` set (utility
role, 16K) as single-turn conversations. `shipped_path_eval.py score-dataset`
graded it with `chat_rag_eval.evaluate_answer` and the same regex labels as
the 2026-09-25 component run, on the same weights:

| | 2026-09-25 component harness | 2026-10-13 shipped path |
|---|---:|---:|
| Mechanical checks passed | 25/38 | 21/38 |
| Fact patterns matched | 26/38 | 23/38 |
| Citation structure passed | 34/38 | 25/38 |
| Turns with web search added by intent | n/a | 25/38 |
| Median latency | 2.8 s | 19.8 s |

These are not the same experiment, and the gap is not a regression
measurement. The component harness handed the model the five top-ranked whole
documents from a separate retrieval run, ran at temperature 0.2 with no
thinking switch, and made one model call per question. The shipped path
retrieves 800-token chunks itself, adds what intent decides, makes about six
calls per question and runs the grounding check.

The citation drop has a visible cause: on 25 of 38 questions, intent read a
company-handbook question ("which HTTP failures may be retried?") as a public
one, so web pages joined the evidence and were cited. Among the four unanswerable
items, the component run declined all four cleanly. The shipped path declined
one cleanly. It declined two while citing the vault document it had looked in,
which the harness counts as a failure. It answered the fourth ("Japanese
public holidays observed by the office") from four public web pages. On v1 every document is one chunk, so the
desktop's per-passage citation numbers line up one-to-one with documents
(`citations_aligned` 38/38). On v3 they do not, and `review_rag_eval.py`
packets cannot be built from v3 rows without first deduplicating citations.

## What the runner still cannot see

- **The bundled sidecar process.** The runner reaches the model as a
  user-run llama.cpp server (`provider: llamacpp`), because spawning the sidecar
  needs a Tauri `AppHandle`. The adapter and scheduler are the same, but three
  things differ. The sidecar's prefill guard is off. The stall timeout is the
  remote provider's. The window comes from settings, not from the GGUF-based
  auto-size.
- **A separate utility model.** The utility role is the chat model on the same
  single slot. A desktop with a small utility GGUF would run intent, planning
  and the claim judge on a second, faster sidecar.
- **The UI.** Citation chips, the evidence margin and claim marks are not
  rendered or checked. Only `[n]` in the text and the persisted metadata are.
- **Human-like pacing.** The runner waits for each turn's grounding check and
  memory work to finish before the next question. A user who types sooner
  would overlap them.
- **Hermeticity.** The intent step can turn on web search for a vault
  question, and on 4 turns it did. Those turns sent queries to the public web
  and read live pages, so a re-run can differ for reasons outside Lattice.
- **Repeatability.** There is one app run per item. pass^3 is three judge
  grades of that run, not three runs.
- **Where a passage came from.** `SourceDto` has no origin, so search hits,
  openings, section lookups and neighbour expansion look the same in the
  evidence. A `retrieval_origin` field on sources would close this.
- **Untested paths.** Deep research, attachments, Explorer, the summary
  index, the reranker, and manual web or Wikipedia toggles were all off or
  unused, matching the defaults.

## Reproduce

Start the one server, run the answers, then grade them (judge included) while
the server is still up. Stop the server when grading is done.

```sh
src-tauri/binaries/llama-server-aarch64-apple-darwin \
  -m ~/.cache/lattice-evals/models/Qwen3.5-9B-Q4_K_M.gguf \
  --port 8089 --host 127.0.0.1 --no-webui -ngl 99 --ctx-size 16384 -np 1 --jinja &

cd src-tauri
cargo run --example shipped_path_eval -- \
  ../evals/retrieval/shipped-path-recipe-9b-local.json \
  ../evals/retrieval/results/chat/qwen3.5-9b-local/DATE-shipped-path-desktop-path-v1.jsonl
cd ..
python3 scripts/shipped_path_eval.py score evals/retrieval/desktop-path-v1.json \
  evals/retrieval/results/chat/qwen3.5-9b-local/DATE-shipped-path-desktop-path-v1.jsonl \
  evals/retrieval/results/chat/qwen3.5-9b-local/DATE-shipped-path-desktop-path-v1.scored.jsonl \
  --endpoint http://127.0.0.1:8089 --model Qwen3.5-9B-Q4_K_M.gguf
```

Use `shipped-path-recipe-9b-local-default-roles.json` for the default-roles
arm. Use `shipped-path-recipe-9b-local-ctx8192.json` to reproduce the 8K
failure. Use `shipped-path-recipe-9b-local-v1-set.json`, then
`shipped_path_eval.py score-dataset`, for the v1 answer set. A dev build of
the example indexes v3 in about 5.5 minutes on this Mac. The runner refuses an
existing output path and fails fast if the weights hash, the server's window
or its slot count differ from the recipe. `--only ID,ID` runs a subset, and
`--keep-data` keeps the scratch app-data directory for inspection.

Each run writes three files: the answers JSONL, `*.calls.jsonl` (every model
request verbatim, with replies and timings; gzipped in this folder) and `*.run.json` (recipe and its
hash, corpus and question hashes, weights hash, server properties, the
settings actually used, embedding identity, index statistics and harness file
hashes). Scoring adds `*.scored.jsonl`, `*.summary.json`, and a
`*.manifest.json` in the answer harness's identity shape.
