# Modern pipeline, stale runtime: closing Lattice's gaps

*Scope: Lattice `main` plus the uncommitted tree, 2026-10-12. Lattice paths are relative to `src-tauri/src/` unless they begin with `src-tauri/`, `evals/` or `scripts/`. Abbreviations: `chat/` = `features/conversation/chat/`; `llama_cpp/` = `features/llm/llama_cpp/`; `sidecar/` = `features/llm/engine/sidecar_manager/`; `ctx_asm/` = `application/services/context_assembler/`; `settings.rs` = `application/contracts/settings.rs`; `curated.rs` = `domain/models/curated.rs`.*

> **Review status (2026-10-12):** independently verified item by item in [`2026-10-12-llm-stack-sota-audit-verification.md`](2026-10-12-llm-stack-sota-audit-verification.md). Read that file for the corrected ranking before acting. Headline corrections: the llama.cpp pin `v0.6.0` is an upstream release from 2026-10-05 and already includes MTP, so item 8's upgrade and "stop stripping MTP tensors" are wrong and item 9 has no build prerequisite; item 1 should turn thinking **off** for tool-using rounds (llama.cpp #20837), and `reasoning_effort:"low"` is a no-op on Qwen templates while the code filters out `"none"`, the one value the server honours; item 4's "re-measure in production mode" is moot because the v3 run already was production mode.

**Lattice's retrieval and harness design is current for October 2026. The layer that configures and serves the model is not.** The tree already contains the choices that 2024-26 research settled. Retrieval is hybrid FTS5 plus dense over a Qwen3-Embedding first stage. The intent router can add searches but never skip the vault. A CRAG-style corrective retry is in place, and HyDE stays off the vault path. Evidence sits before the question in the final user turn. Claims are checked one at a time with logprob verdicts, and memory is bounded and source-backed. The gaps sit where Lattice meets the model. Qwen 3.5's `enable_thinking` switch is never sent, and sampling ignores the model card. The llama.cpp pin predates MTP speculative decoding, and the sidecar runs on default flags. The default bounded-memory layout puts query-dependent blocks ahead of the conversation history. On Qwen 3.5's hybrid architecture, that can force a full re-prefill every turn. The only downloadable reranker is a small MS MARCO cross-encoder that measured no gain. The largest current-generation chat model in the catalog is 9B, and no eval exercises the answer path that ships. **Four changes have the largest expected payoff:**
1. Set thinking mode and per-mode sampling explicitly on every call.
2. Make the prompt prefix byte-stable across turns and tool rounds.
3. Ship a 2026-tier reranker and re-test it.
4. Upgrade llama.cpp with explicit flags. This also unlocks a 35B-A3B tier for 32 GB Macs.

None of these payoffs has been measured on Lattice. That is why an eval that runs what ships ranks fifth, and why it gates the rest.

## Scorecard: half the dimensions trail, and they cluster at the model boundary

| Dimension | Lattice today | 2026 best practice | Gap | Evidence for change |
|---|---|---|---|---|
| Prompt format and layout | Plain `Label:` sections, no XML. Instructions, then `Context:`, then `Question:` last (`chat/prompting.rs:421-425`, `chat/prompt_settings.rs:14`). Evidence goes in the final user message (`application/services/completion_input.rs:53-97`). KB passages are best-first, with no date or score (`chat/prompting.rs:378-419`). No date is injected anywhere. | Evidence before question gave +15.14 F1 across 10 models ([arXiv 2601.14152](https://arxiv.org/pdf/2601.14152), snippet only). Use consistent, unambiguous delimiters; XML is the Claude-specific advice ([Claude docs](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)). | minor | weak: no Qwen delimiter study |
| Thinking mode and sampling | `reasoning_effort` is unset for chat, intent, HyDE and judge. Learning and the strict judge send `"low"` via `reasoning_effort` plus `chat_template_kwargs` (`llama_cpp/mod.rs:275-287`). `enable_thinking` is never sent. Sampling is T 0.7, top_p 0.9, top_k 40, repeat 1.1, with no presence penalty (`settings.rs:1087-1090`, `llama_cpp/mod.rs:270`). | Toggle with `enable_thinking`, since the soft switches are gone. Per-mode sampling: top_k 20, presence 1.5, repetition 1.0 ([Qwen3.5-9B card](https://huggingface.co/Qwen/Qwen3.5-9B)). | **major** | moderate: the card contradicts itself, and [Unsloth](https://unsloth.ai/docs/models/qwen3.5) disagrees on small-model defaults |
| Tool loop | Native JSON-schema tools under `--jinja` (`sidecar/readiness.rs:66-71`). 5, 10 or 12 rounds (`chat/tool_loop.rs:961-970`). Errors go back to the model (`:519-575`). Parallel calls are requested but run sequentially (`:502`). Tools are withdrawn on the final round (`:946-954`) and narrowed per turn (`chat/tools.rs:42-50`). | A small, non-overlapping tool set with actionable errors ([Anthropic](https://www.anthropic.com/engineering/writing-tools-for-agents)). Keep the tool block stable and mask tools rather than removing them ([Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)). | minor | moderate |
| Deep research | A pre-planned 6-query fan-out and 6 parallel page reads (`chat/retrieval/pipeline.rs:582-600`), then model-driven rounds in the normal loop. The loop stops when the window fills (`chat/tool_loop.rs:292-298`). There is no in-loop summarisation and no report stage (`chat/prompting.rs:308-341`). | Summarise earlier tool turns at about half the window for 4B models ([LiteResearcher](https://arxiv.org/pdf/2604.17931)). Use effort rules and a citation pass ([Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)), and write the report once ([LangChain](https://blog.langchain.com/open-deep-research/)). | **major** | moderate: most of the SOTA gap is RL-trained models, not harness |
| Verification and grounding | Post-hoc, per cited sentence, with a lexical prefilter (`chat/verification/lexical.rs:18-27`). The utility or chat model judges, with a first-token logprob verdict (`application/services/claim_verification.rs:724-746`). Uncited sentences are never judged (`lexical.rs:118-131`). A badge is the only output, with no repair (`chat/verification/background.rs:1-7`). | Run a small purpose-trained checker on each claim-passage pair ([MiniCheck](https://arxiv.org/abs/2404.10774)). Flag claims rather than rewriting them ([arXiv 2608.29307](https://arxiv.org/abs/2608.29307)). Calibrate on about 100 labels ([ARES](https://arxiv.org/abs/2311.09476)). | minor | moderate |
| Memory and compaction | Bounded, source-backed memory, on by default (`settings.rs:1108`). Pools are 25/10/15/35% (`ctx_asm/budget.rs:77-86`). The summariser keeps decisions, failures and identifiers (`features/conversation/memory/prompts.rs:54`). Extraction and verification are schema-checked. Token counts are a calibrated chars/4 estimate (`features/llm/scheduler/scheduled.rs:214-232`). | Compact at a threshold with a sectioned summary ([Arize on Claude Code](https://arize.com/blog/context-management-in-agent-harnesses/)). A plain search-over-files agent is competitive ([Letta](https://www.letta.com/blog/benchmarking-ai-agent-memory)). Add time-aware expansion ([LongMemEval](https://arxiv.org/abs/2410.10813)). | minor | moderate |
| Structured output | json_schema compiled to GBNF for memory, the planner and all learning (`llama_cpp/mod.rs:301-306`). A guard caps bounds at 2000 (`features/conversation/memory/prompts.rs:24-31`). Intent, router, compare and summaries use prompt-only JSON (`chat/intent.rs:169-190`). | A grammar gives validity ([JSONSchemaBench](https://arxiv.org/html/2501.10868v3)). Reason first and emit the schema last to avoid the open-model "format tax" ([arXiv 2604.03616](https://arxiv.org/abs/2604.03616)). | minor | weak to moderate: the notes disagree |
| Chunking and parsing | 480-token structure-aware chunks with zero overlap; fences and tables stay intact (`features/embedding/input_policy.rs:14,128-134`). A metadata header is embedded but is not stored in the chunk text (`features/indexing/use_cases/embedding_input.rs:169-189`). OCR is `NoopOcr` (`interfaces/di/modules.rs:486-519`). | Structure-aware chunks of 256-1024 tokens ([arXiv 2603.06976](https://arxiv.org/abs/2603.06976)). LLM-written chunk context for both embeddings and BM25 ([Anthropic](https://www.anthropic.com/news/contextual-retrieval)). Sub-1B document VLMs for scans ([PaddleOCR-VL](https://arxiv.org/abs/2510.14528)). | chunking current; parsing **major** | moderate |
| Embeddings | Qwen3-Embedding-0.6B on GPU, MiniLM-L6 on CPU (`curated.rs:351-413`). Query-only instruction (`features/embedding/prefixes.rs:66-68`). The shipped default is a 256-d int8 index (`settings.rs:1002-1013`). | Qwen3-Embedding is still the open reference; the 4B gains about 5 MMTEB points ([arXiv 2506.05176](https://arxiv.org/abs/2506.05176)). EmbeddingGemma suits small footprints ([Google](https://developers.googleblog.com/en/introducing-embeddinggemma/)). | minor | moderate |
| First-pass retrieval and hybrid | HNSW, with an exact scan at 20k vectors or fewer (`features/search/engine/vector_search/usearch_index.rs:57-65`). FTS5 porter plus trigram (`src-tauri/migrations/20260916000000_init_schema.sql:233-250`). RRF with k=10 and weights 0.7/0.3 (`shared/constants.rs:60-64`). Sparse is off (`features/search/di.rs:34-39`). | Hybrid is standard. A convex combination beats RRF once labels exist ([arXiv 2210.11934](https://arxiv.org/abs/2210.11934)). | current | strong for hybrid, moderate for fusion |
| Query transformation | Enable-only intent; the vault is searched by default (`chat/retrieval/mod.rs:279-284`). A schema-bound planner writes 1-3 queries (`chat/retrieval/corpus_plan.rs:132,646`). No HyDE on the vault (`chat/retrieval/kb_retrieval.rs:198`). A sufficiency retry runs (`kb_retrieval.rs:277-356`). | Rewrite into standalone queries. HyDE degrades strong retrievers ([arXiv 2309.08541](https://arxiv.org/abs/2309.08541)). Grade retrieval, then retry ([CRAG](https://arxiv.org/abs/2401.15884)). | current | moderate |
| Reranking | Off by default (`settings.rs:994`). The only downloadable model is ms-marco-MiniLM-L-6-v2 (`features/search/reranker_setup.rs:47`). It scores 48 candidates. On v3 it gave no gain (recall@5 0.7755 both ways) at 533 ms against 24.9 ms (`evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md:14,17`). | jina-reranker-v3 or Qwen3-Reranker-4B. These add 4-8 MTEB-R points over a Qwen3-Embedding-0.6B top 100 ([Qwen3-Reranker-4B card](https://huggingface.co/Qwen/Qwen3-Reranker-4B)). | **major** | strong in general, moderate for specific models |
| Context assembly and KV-cache reuse | Evidence goes last. But query-dependent recall, memory and "supporting material" blocks come before the recent turns (`ctx_asm/mod.rs:374-394`). Requests set `id_slot` and `cache_prompt` (`llama_cpp/mod.rs:307-312`). | A byte-stable prefix, append-only history and volatile content last ([Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)). Hybrid Qwen 3.5 re-processes the whole prompt when the prefix changes ([llama.cpp #20225](https://github.com/ggml-org/llama.cpp/issues/20225)). | **major** | moderate |
| llama-server flags | Pinned to the fork tag `v0.6.0` (`src-tauri/scripts/llama-server.lock`). The argv is only `-m --port --host --no-webui -ngl --ctx-size --jinja` (`sidecar/readiness.rs:49-73`). Auto context is capped at 32,768 (`sidecar/config.rs:85,91`). | Upstream is at b11577, and MTP decoding has shipped since b9180. Pass `-np 1`, a reasoning budget and more context checkpoints explicitly ([server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)). | **major** | moderate: single-source speed data, none for 9B |
| Model catalog | Ten chat entries, all Q4_K_M. The largest current-generation one is Qwen3.5-9B. There is no utility-model entry. The OCR entries are not wired up (`curated.rs:70-334,454,477`). | Qwen3.6-35B-A3B for 32 GB and up. UD-Q4_K_XL beats Q4_K_M on KLD ([Unsloth](https://unsloth.ai/docs/models/qwen3.5/gguf-benchmarks)). Use a 4B non-thinking model for utility calls ([Qwen3.5-2B card](https://huggingface.co/Qwen/Qwen3.5-2B)). | **major** | moderate |
| Evaluation | A strong retrieval harness: recall, nDCG and MRR with a paired bootstrap, over 194 v3 queries (`evals/retrieval/README.md:93-100`). Production mode skips the planner, the retry, openings and answers (`:230-234`). The answer harness is not the desktop path (`scripts/chat_rag_eval.py`). Memory and learning evals are `#[ignore]` (`src-tauri/tests/conversation_memory_evals.rs:14`). | Start with about 20 real queries, a rubric judge and end-state grading ([Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)). Measure pass^k ([tau-bench](https://arxiv.org/abs/2406.12045)). | **major** | moderate |

Several places in the notes disagree, and the ranking below depends on three of them.

**Qwen 3.5 small-model thinking.** The 27B card says thinking is on by default. Unsloth says the 0.8B-9B models ship with it off ([Unsloth](https://unsloth.ai/docs/models/qwen3.5)). Leaving the switch to the template default therefore means different behaviour on different tiers.

**Constrained decoding.** One line of work finds a measurable tax on open models ([Constraint Tax](https://arxiv.org/html/2605.26128v1): accuracy fell from 19.7% to 11.0% while validity rose from 61.5% to 100%). Another finds small gains: GSM8K rose from 80.1 to 83.7 with XGrammar ([JSONSchemaBench](https://arxiv.org/html/2501.10868v3)). The reconciliation both notes reach is that the cost comes mostly from forcing reasoning into the format. Reason first, then emit.

**Reranking.** Lattice's in-house null result conflicts with strongly replicated published gains. The notes explain the conflict. An older cross-encoder can lose to a modern first stage: bge-reranker-v2-m3 scored 57.03 against the Qwen3-Embedding-0.6B baseline of 61.82 on MTEB-R ([card](https://huggingface.co/Qwen/Qwen3-Reranker-4B)). Lattice's null result says ms-marco-MiniLM is outdated. It does not show that reranking fails.

## Fifteen changes, ranked by payoff per unit of effort

1. **Set Qwen 3.5 thinking and sampling explicitly per call type.**
   - **Change:** In `llama_cpp/mod.rs:270-287`, send `chat_template_kwargs: {"enable_thinking": …}` on every request. Turn it on for chat answers and deep research. Turn it off for intent, HyDE, the planner, the router, the judge and memory. Replace the single sampling profile (`settings.rs:1087-1090`) with the card's per-mode profiles:
     - non-thinking: T 0.7, top_p 0.8, top_k 20, presence 1.5;
     - thinking: T 1.0, top_p 0.95, top_k 20, presence 1.5;
     - repetition 1.0 rather than 1.1 in both.
   - **Check:** Use `/apply-template` to see what `reasoning_effort:"low"` renders for Qwen. That value is sent by learning (`features/learning/model_call.rs:32`) and the strict judge (`claim_verification.rs:474`). The card documents only `enable_thinking`.
   - **Why:**
     - The Qwen card documents the per-mode profiles ([Qwen3.5-9B card](https://huggingface.co/Qwen/Qwen3.5-9B)).
     - Qwen3.5-9B tool calls failed with thinking on and worked with it off ([llama.cpp #20837](https://github.com/ggml-org/llama.cpp/issues/20837)).
     - The 2B, the default under 8 GB (`curated.rs:316-334`), is "more prone to entering thinking loops" ([2B card](https://huggingface.co/Qwen/Qwen3.5-2B)).
     - The card's own tables are inconsistent, and high presence penalty can cause language mixing.
   - **Payoff:** Every call runs in its intended mode, and utility calls get faster.
   - **Effort:** Hours to 2 days.
   - **Prerequisite:** None. Get a before/after on the existing v1 chat set.

2. **Make the prompt prefix byte-stable across turns and rounds.**
   - **Change:**
     - In `ctx_asm/mod.rs:374-394`, move the history recall, recorded requirements and supporting-material blocks into the final user message, after the verbatim recent turns.
     - Rewrite the summary message only at compaction boundaries.
     - In `chat/tool_loop.rs:946-954`, keep the tools in the final-round request instead of withdrawing them. The loop already ignores final-round calls (`:432-440`).
     - Measure whether per-turn tool narrowing (`chat/turn/assemble.rs:418-446`) flips the tool block between turns.
   - **Why:**
     - Manus calls KV-cache hit rate "the single most important metric" and advises masking tools rather than removing them ([Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)).
     - On hybrid Qwen 3.5, a mutated prefix forces full re-processing. One report measured about 8 minutes per turn at 15k tokens, and the maintainer called that expected behaviour ([#20225](https://github.com/ggml-org/llama.cpp/issues/20225)). A checkpoint-restore fix was rejected ([#22384](https://github.com/ggml-org/llama.cpp/issues/22384)).
     - All three default tiers are Qwen 3.5.
   - **Payoff:** Large prefill savings on every turn and tool round. The size is unknown until measured.
   - **Effort:** Days.
   - **Prerequisite:** Log the cached and evaluated prompt tokens per request first.

3. **Replace the reranker and re-test.**
   - **Change:**
     - Add Qwen3-Reranker-0.6B to the catalog. The Candle path already scores `Qwen3ForCausalLM` rerankers (`features/search/engine/reranker.rs:193-236`).
     - Evaluate jina-reranker-v3, and Qwen3-Reranker-4B for 32 GB and up.
     - Raise the depth from 48 toward 64-100 (`settings.rs:1052-1053`).
     - Turn reranking on by default only if it wins on the v3 paired bootstrap. Retire ms-marco-MiniLM.
   - **Why:**
     - On MTEB-R over a Qwen3-Embedding-0.6B top 100, the base scores 61.82. Adding Qwen3-Reranker-0.6B gives 65.80, and the 4B gives 69.76 ([card](https://huggingface.co/Qwen/Qwen3-Reranker-4B)).
     - jina-reranker-v3 scores 61.94 on BEIR ([Jina](https://jina.ai/news/jina-reranker-v3-0-6b-listwise-reranker-for-sota-multilingual-retrieval/)).
     - Caveat: Qwen3-Reranker-0.6B fell to 48.09 in a third-party run ([arXiv 2606.19037](https://arxiv.org/abs/2606.19037)).
     - If it is ever served through llama.cpp, use the official GGUF ([ggml-org](https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF)).
   - **Payoff:** Several nDCG points, if vendor gains transfer.
   - **Effort:** Days.
   - **Prerequisite:** Measure latency. MiniLM already costs 533 ms.

4. **Drop the default 256-d int8 index compression at vault scale.**
   - **Change:** Default `vector_index_compression` (`settings.rs:1002-1013`) to uncompressed, or to 512-d or more, below a vault-size threshold.
   - **Why:**
     - mrl256-i8 scored 0.7640 recall@5 against 0.7755 (`evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md:10-17`).
     - The 10-09 headline was measured uncompressed, so the shipped default runs below the reported number (`evals/retrieval/README.md:93-100`).
     - Qwen3-Embedding supports custom MRL dimensions ([arXiv 2506.05176](https://arxiv.org/abs/2506.05176)). The RAG note infers, without measuring, that 512-1024 dimensions suit vault scale. 20k f32 vectors at 1024-d take about 80 MB.
   - **Payoff:** About +1 point of recall@5, for free.
   - **Effort:** Hours.
   - **Prerequisite:** Confirm what the exact scan reads.
   - **Caveat (added in review):** the comment at `settings.rs:1008-1010` says 256-d/i8 *matched* full precision in a production-path run, while the 2026-09-19 table shows a 0.7640 vs 0.7755 loss. Re-measure in production mode before changing the default.

5. **Evaluate what ships.**
   - **Change:**
     - Run `src-tauri/examples/retrieval_eval` with shipped defaults: planner, retry, openings and compression.
     - Build a 20-50 question answer set through the desktop chat path. Grade it with a single-call rubric judge (factual accuracy, citation accuracy, completeness) at pass^3.
     - Hand-label about 100 claim-passage pairs.
   - **Why:**
     - Anthropic starts with about 20 queries and grades the end state ([Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)).
     - pass^k measures reliability ([tau-bench](https://arxiv.org/abs/2406.12045)), and a small labelled set calibrates judges ([ARES](https://arxiv.org/abs/2311.09476)).
     - Public leaderboards cannot stand in: Epoch found possible defects in 48% of sampled BFCL items ([Epoch](https://epoch.ai/benchmarks/berkeley-function-calling-leaderboard/review)).
   - **Payoff:** This is the enabling step. Items 1, 2, 8, 9, 13 and 14 cannot be confirmed without it.
   - **Effort:** A week or more.
   - **Prerequisite:** None.

6. **Put the contextual header into BM25, then test LLM-written chunk context.**
   - **Change:**
     - Stage one: add the existing header to FTS5 in a new migration. The header holds the heading path, title and file name (`features/indexing/use_cases/embedding_input.rs:169-189`), and today it is embedded but not stored in the chunk text.
     - Stage two: have the utility model write 50-100 tokens of document context per chunk at index time.
   - **Why:**
     - Contextual embeddings cut retrieval failures by 35%, contextual BM25 by 49%, and adding reranking by 67% ([Anthropic](https://www.anthropic.com/news/contextual-retrieval), a 2024 vendor eval).
     - Contextual retrieval beats late chunking on coherence ([arXiv 2504.19754](https://arxiv.org/abs/2504.19754)). That matches Lattice's own late-chunking regression to 0.7110.
   - **Payoff:** Small for stage one. Possibly the largest retrieval gain for stage two, but untested on a local 9B.
   - **Effort:**
     - Stage one: hours to days.
     - Stage two: a week or more, plus re-indexing.
   - **Prerequisite:** Item 5.

7. **Compact inside the deep-research loop instead of stopping.**
   - **Change:**
     - When room runs low (`chat/tool_loop.rs:292-298`), replace older page bodies with the URL plus a short summary. The turn's fetch memory (`:576-598`) keeps a re-fetch possible.
     - Trigger at roughly 50-60% of the real window.
     - Add an effort rule to `chat/prompting.rs:308-341`.
   - **Why:**
     - Summarising earlier tool interactions improved a 4B researcher ([arXiv 2604.17931](https://arxiv.org/pdf/2604.17931)).
     - Clearing old tool results is the lightest form of compaction ([Anthropic](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)).
     - With context at 32,768 or less and pages up to 50,000 characters (`settings.rs:880`), a few fetches fill the window.
   - **Payoff:** Moderate. Deep-research turns get to use their 12 rounds.
   - **Effort:** Days.
   - **Prerequisite:** None.

8. **Upgrade llama.cpp and pass explicit flags.**
   - **Change:**
     - Move `src-tauri/scripts/llama-server.lock` to a current upstream build: b9180 or later for MTP, and b11577 is current ([releases](https://github.com/ggml-org/llama.cpp/releases)).
     - Add these to `sidecar/readiness.rs:49-73`:
       - `-np 1`;
       - `-fa on`;
       - more `--ctx-checkpoints`;
       - `--spec-type draft-mtp --spec-draft-n-max 2-3` for MTP GGUFs.
     - Send an explicit reasoning budget on thinking calls.
     - Stop stripping MTP tensors.
     - Revisit the 32,768 clamp (`sidecar/config.rs:85,91`). The 9B's KV is about 1 GiB at 32K.
   - **Why:**
     - MTP lifted Qwen3.6-27B from 18 to 32 tok/s on an M5 Max, but only 93 to 105 on the 35B-A3B ([stared bench](https://github.com/stared/benching-local-llms-on-apple-silicon)).
     - MTP refuses `-np` above 1 ([Unsloth MTP card](https://huggingface.co/unsloth/Qwen3.5-9B-MTP-GGUF)).
     - Post-April builds default the reasoning budget to INT_MAX, which can hang ([forge #54](https://github.com/antoinezambelli/forge/issues/54)).
     - Counter-evidence: a pre-merge fork found MTP to be a net negative on hybrids ([NodeNestor](https://github.com/NodeNestor/qwen3.5-27b-mtp-llamacpp)), and there are no 9B numbers.
   - **Payoff:** Faster decode on the dense tiers, plus Metal fixes.
   - **Effort:** A week or more.
   - **Prerequisite:** Items 2 and 5.

9. **Refresh the catalog around 2026 tiers.**
   - **Change:** In `curated.rs:70-334`:
     - Add Qwen3.6-35B-A3B as the 32 GB-and-up default.
     - Evaluate Qwen3.8-27B for 64 GB.
     - Add a Gemma 4 option.
     - Ship UD-Q4_K_XL or Q5_K_M on 32 GB, and Q6 or Q8 on 64 GB.
     - Recommend Qwen3.5-4B non-thinking as the utility model.
     - Hide the pre-Qwen3.5 entries.
   - **Why:**
     - The 35B-A3B decodes 93 tok/s against 18 for the 27B dense, with BFCL-V4 67.3 against 68.5 ([27B card](https://huggingface.co/Qwen/Qwen3.5-27B)).
     - Its long-context score is lower (AA-LCR 58.5 against 66.1).
     - On KLD, UD-Q4_K_XL scores 0.0137 at 19.2 GB against Q4_K_M's 0.0192 at 18.5 GB ([Unsloth](https://unsloth.ai/docs/models/qwen3.5/gguf-benchmarks)).
     - The 2B's non-thinking IFEval is 61.2 ([card](https://huggingface.co/Qwen/Qwen3.5-2B)).
   - **Payoff:** Large for owners of 32 GB and larger Macs.
   - **Effort:** Days.
   - **Prerequisite:** Item 8.

10. **Run independent tool calls concurrently.**
    - **Change:** In `chat/tool_loop.rs:502`, run read-only calls from one reply with `join_all`, keeping result order deterministic.
    - **Why:**
      - The deep-research prompt asks for "several calls in one reply" (`chat/prompting.rs:308-341`).
      - Parallel tool calls contributed to cuts in research time of up to 90% ([Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system), frontier evidence).
      - Network fetches parallelise even on a single-slot server.
    - **Payoff:** Wall-clock savings in multi-fetch rounds.
    - **Effort:** Hours to days.
    - **Prerequisite:** Keep citation numbering stable (`chat/retrieval/source_citations.rs:25-38`).

11. **Tune fusion on v3.**
    - **Change:** Sweep k=10 and the 0.7/0.3 weights (`shared/constants.rs:60-64`). Try a normalized convex combination in `features/search/engine/fusion.rs:66-80`.
    - **Why:** A convex combination beat RRF, and RRF is sensitive to k ([arXiv 2210.11934](https://arxiv.org/abs/2210.11934), 2023).
    - **Payoff:** Small and uncertain.
    - **Effort:** Hours.
    - **Prerequisite:** Hold out queries so the weights don't overfit the synthetic set.

12. **Give the model the date and show document dates.**
    - **Change:** Append today's date to the final user message, never the system prompt. Add document dates to passage rendering (`chat/prompting.rs:378-419`).
    - **Why:**
      - The deep-research instruction asks what is "out of date" (`:308-341`), yet the inventory found no date injection anywhere.
      - Time-aware expansion helps temporal questions ([LongMemEval](https://arxiv.org/abs/2410.10813)).
      - Timestamps belong outside the cached prefix ([Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)).
    - **Payoff:** Small. The evidence is weak: this is a logical gap, not a measured one.
    - **Effort:** Hours.
    - **Prerequisite:** Do it together with item 2.

13. **Bake off a dedicated claim checker.**
    - **Change:** On the labelled pairs from item 5, compare the current judge (`chat/verification/judge.rs`) with:
      - MiniCheck-FT5 (770M);
      - HHEM-2.1-open;
      - Bespoke-MiniCheck-7B.

      Also sample the uncited factual sentences that are never checked (`chat/verification/lexical.rs:118-131`).
    - **Why:**
      - MiniCheck-FT5 matched GPT-4 at about 400× lower cost ([arXiv 2404.10774](https://arxiv.org/abs/2404.10774)).
      - Checkers degrade out of domain ([arXiv 2604.15945](https://arxiv.org/html/2604.15945v1)).
      - Models cite passages they never used ([arXiv 2609.23053](https://arxiv.org/html/2609.23053v1)).
    - **Payoff:** Moderate. Badge quality would stop depending on which utility model the user picked.
    - **Effort:** Days to a week. The inventory shows Candle supporting only BERT, JinaBert and Qwen3.
    - **Prerequisite:** Item 5.

14. **Let learning generation reason before the schema.**
    - **Change:** Verify whether Qwen thinks before the grammar applies in lesson generation (`features/learning/lessons/generation.rs:668-712`). If it doesn't, enable thinking or draft free-form and then reformat. Move intent and the router to json_schema.
    - **Why:**
      - The format tax concentrates in open-weight models, and decoupling recovers most of it ([arXiv 2604.03616](https://arxiv.org/abs/2604.03616), abstract only).
      - llguidance would also remove the need for the 2000-bound guard ([llguidance](https://github.com/ggml-org/llama.cpp/blob/master/docs/llguidance.md)).
    - **Payoff:** Lesson quality; uncertain.
    - **Effort:** Days.
    - **Prerequisite:** Item 1.

15. **Wire OCR to a sub-1B document VLM.**
    - **Change:**
      - Replace `NoopOcr` (`interfaces/di/modules.rs:486-519`).
      - Route `needs_ocr` pages (`features/indexing/engine/extraction/pdf.rs:7-9`) to PaddleOCR-VL, GLM-OCR or granite-docling.
      - Retire the catalog's Qwen2.5-VL-7B and Florence-2 entries (`curated.rs:454,477`).
    - **Why:**
      - PaddleOCR-VL-1.6 scores 96.33 on OmniDocBench ([arXiv 2606.03264](https://arxiv.org/abs/2606.03264)).
      - GLM-OCR (0.9B) scores 95.22 ([Roboflow](https://blog.roboflow.com/best-open-source-ocr-models/)).
    - **Payoff:** Large for users with scans, whose image-only pages index nothing today; zero for everyone else.
    - **Effort:** A week or more.
    - **Prerequisite:** A separate process, because `--mmproj` cannot share a server with MTP ([Unsloth MTP card](https://huggingface.co/unsloth/Qwen3.5-9B-MTP-GGUF)).

## Already current: leave these alone

- **Evidence before the question, in the final user turn.** This matches the +15.14 F1 ordering result ([arXiv 2601.14152](https://arxiv.org/pdf/2601.14152)). Best-first passage order is what the notes recommend, so no "sandwich" reordering is needed.
- **Per-passage numeric `[n]` labels, verified programmatically.** These are the cheapest markers for small models to copy.
- **A zero-shot chat prompt.** Few-shot degrades reasoning models ([DeepSeek-R1](https://arxiv.org/abs/2501.12948)), and explicit CoT adds little for them ([Wharton](https://papers.ssrn.com/abstract=5285532)).
- **480-token structure-aware chunks with zero overlap.** Overlap evidence conflicts: none in [Chroma](https://research.trychroma.com/evaluating-chunking), 15% in one NVIDIA test. Not worth re-litigating.
- **Late chunking off by default.** In-house it scored 0.7110 against 0.7755.
- **Hybrid FTS5 plus trigram plus dense, with learned sparse off.** Sparse adds little over BM25 next to a strong dense model.
- **HyDE off the vault path, the enable-only intent router, and the CRAG-style retry.**
- **Per-claim judging scoped to cited passages, with logprob verdicts and no silent rewrite.** Repair edited 83.5% of answers annotated clean ([arXiv 2608.29307](https://arxiv.org/abs/2608.29307)).
- **Source-backed bounded memory, fallible summary framing and JSON-escaped quotes.**
- **A single-agent loop with explicit exits, errors kept in context, and resumable research jobs** ([Cognition](https://cognition.ai/blog/dont-build-multi-agents)).
- **llama.cpp as the runtime.** It leads MLX on M5 decode, 93 against 85 tok/s ([stared bench](https://github.com/stared/benching-local-llms-on-apple-silicon)).
- **An output reservation of about 32K tokens.** This matches the Qwen card's 32,768.

## Shiny but unproven here: skip or defer

- **Multi-agent deep research.** The 90.2% gain was measured on frontier models at about 15× the tokens ([Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)). It brings no parallelism on a single-slot server.
- **GraphRAG.** It was about 13.4% worse than vanilla RAG on NQ ([arXiv 2506.05690](https://arxiv.org/abs/2506.05690)). Revisit link-based expansion of the HippoRAG type only if v3 multi-hop (nDCG 0.512) becomes the priority.
- **CacheBlend, Block-Attention and TurboRAG.** They need vLLM or generator fine-tuning ([arXiv 2410.07590](https://arxiv.org/abs/2410.07590)).
- **ColPali-style multi-vector retrieval.** It needs custom MaxSim scoring, and its feasibility at vault scale is unverified.
- **LLM and semantic chunkers.** Their cost is not justified by the gains ([arXiv 2410.13070](https://arxiv.org/abs/2410.13070)).
- **Long context instead of RAG.** RAG wins at 128K and for weaker models ([LaRA](https://proceedings.mlr.press/v267/li25dv.html)).
- **q4_0 KV cache or TurboQuant on hybrid Qwen.** The KV cache is already small, and these slow long contexts ([#20969](https://github.com/ggml-org/llama.cpp/discussions/20969)).
- **XML delimiters and prompt repetition as quality fixes.** Neither has been tested on small open models ([arXiv 2512.14982](https://arxiv.org/abs/2512.14982)). A/B them only after item 5 exists.
- **Tongyi DeepResearch 30B-A3B.** It expects its own ReAct format and scored 24.94% on an independent BrowseComp-Plus run ([arXiv 2603.01152](https://arxiv.org/pdf/2603.01152)). Treat it as an experiment after item 8.
- **Mem0 and Zep memory layers.** Their benchmarks are disputed ([zep-papers#5](https://github.com/getzep/zep-papers/issues/5)).

## What the inventory could not settle

- **The pinned build.** Which upstream build `v0.6.0` corresponds to, and what its `-fa` and reasoning-budget defaults are. The internal note that it chokes on the MTP layer `blk.32` places it before b9180.
- **Qwen 3.5 thinking behaviour.** What Qwen 3.5 actually does with thinking when Lattice leaves the switch unset, and whether its template reads `reasoning_effort`.
- **Prefill cost.** How many prompt tokens are re-processed per turn today, and whether background consolidation after each turn (`features/conversation/compaction.rs:1-9,142-190`) rewrites the summary message every turn. This sizes the payoff of item 2.
- **Index contents.** Whether `chunks_fts` already indexes headings or titles, and whether the exact scan reads compressed vectors (`DEFAULT_RESCORE_FACTOR` was not determined).
- **Unread code paths.**
  - The final passage ordering and eviction in `chat/turn/assemble.rs`.
  - Whether `recency.rs` or the in-Rust `BM25Search` take part in chat retrieval.
  - Whether `local_context_window` reaches the sidecar.
- **Text-form tool calls.** Whether the text-tool-call scrubber (`chat/tool_loop.rs:441-470`) hides Qwen3-Coder XML calls that the parser missed, which is the #20837 failure mode, instead of recovering them.
- **External unknowns.**
  - Apple Silicon latency for 0.6B rerankers and checkers.
  - MTP numbers for the 9B.
  - Whether the grammar applies inside `<think>`.
  - The conflicting Qwen3.8 release dates.

## Conclusion

The pattern behind this audit is that **design decisions in Lattice were made from research, but runtime defaults were set once and never re-read against model cards.** Qwen 3.5 removed the soft switches and recommends a presence penalty of 1.5, which llama.cpp does not apply by default. The cheapest high-value work is therefore configuration, not new architecture. Qwen 3.5's hybrid DeltaNet design also changes what prompt layout means. On a dense model a mutated prefix wastes some prefill. On a hybrid it can mean re-reading the whole conversation on every turn, which turns cache layout from an optimisation into a latency correctness issue. Finally, most of the rankings above rest on vendor or single-source numbers. **Until Lattice can evaluate the path it ships, it cannot tell which of these gains are real on its own vault.** That makes item 5 the hinge of the plan.
