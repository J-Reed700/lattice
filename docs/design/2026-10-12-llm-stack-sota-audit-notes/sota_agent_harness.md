# LLM agent harness and context engineering: state of the art (as of Oct 2026)

Scope: tool loops, deep research, context and memory, structured output, grounding, table-stakes features, evaluation. Each item is tagged **[frontier]** when the evidence comes only from cloud frontier models and **[small-OK]** when it was shown on open models of 0.6B-30B. Sources dated before 2025 are marked "older".

## 1. Tool-use loop design

### Takeaway
Anthropic, OpenAI, Manus and Cognition (2025-26) agree on the defaults:
- Start with one single-threaded agent.
- Give it a small set of tools that don't overlap, with token-efficient outputs and errors it can act on.
- Load content just in time, behind lightweight references.
- Use files as external memory.
- Use sub-agents only for read-heavy exploration, and have each return a summary of about 1-2K tokens.

### Cited Findings
- 29 Sep 2025:
  - Keep the tool set minimal and non-overlapping: "if a human engineer can't definitively say which tool to use… an agent can't either."
  - Use a few diverse canonical examples instead of long lists of edge-case rules.
  - Clearing old tool results is the lightest form of compaction.
  — [Anthropic, Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- Just-in-time vs preloading, same source:
  - Just in time: keep paths, queries or links and load the content at runtime.
  - Hybrid: preload some context up front (CLAUDE.md) and fetch the rest just in time (glob/grep). Recommended for less dynamic domains.
  - Sub-agents work in clean windows that use tens of thousands of tokens and return 1,000-2,000-token summaries.
  — [Anthropic](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- Sep 2025: consolidate and namespace tools, and return human-meaningful identifiers instead of UUIDs. Offer a `response_format` parameter (concise/detailed), plus pagination, filtering and truncation with sane defaults. Claude Code caps tool responses at 25K tokens by default. Error messages should steer the model toward correct use — [Anthropic, Writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Claude Code internals (Apr 2026 analysis):
  - Tool results are capped at 50K characters per tool and 200K characters per message. Oversized results are written to disk and replaced by a 2KB preview.
  - File reads get a 25K-token budget and default to 2,000 lines.
  - Re-reading an unchanged file returns a stub.
  - Sub-agents start blank with only the delegated prompt. A newer "fork" path shares the parent history so the prompt cache can be reused.
  — [Arize](https://arize.com/blog/context-management-in-agent-harnesses/)
- Manus (18 Jul 2025):
  - KV-cache hit rate is "the single most important metric".
  - Keep the prompt prefix stable (no timestamp at the top), keep context append-only, and serialize JSON deterministically.
  - Mask tool logits through a state machine and response prefill instead of adding or removing tools mid-task. Consistent tool-name prefixes (`browser_`, `shell_`) make group masks easy.
  - Treat the file system as memory, with reversible compression: drop a page's body but keep its URL.
  - Recite a todo.md (a typical task is about 50 tool calls).
  - Keep errors and stack traces in context, and vary serialization to avoid few-shot ruts.
  - Input-to-output token ratio is about 100:1.
  — [Manus](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)
- Jun 2025: "Don't build multi-agents." Share full traces, because actions carry implicit decisions and parallel sub-agents make conflicting ones. Use a linear agent and compress its history — [Cognition](https://cognition.ai/blog/dont-build-multi-agents)
- Apr 2025: get the most out of a single agent first, and split only when the logic is complex or tools overlap. Define explicit exit conditions (a final-output tool, no tool call, an error, max turns). Layer the guardrails, and escalate to a human once failure thresholds are crossed — [OpenAI, Practical guide to building agents](https://cdn.openai.com/business-guides-and-resources/a-practical-guide-to-building-agents.pdf)
- Jul 2025: performance degrades as input length grows across 18 models, including Qwen3, even on simple tasks, and distractors make it worse **[small-OK]** — [Chroma, Context rot](https://research.trychroma.com/context-rot)
- Progressive disclosure and MCP:
  - Nov 2025, tool search with `defer_loading`: context fell from about 77K to about 8.7K tokens, and Opus 4's MCP eval rose from 49% to 74%, at the cost of an extra search step **[frontier]**. These figures come via a third-party summary of [Anthropic](https://anthropic.com/engineering/advanced-tool-use) — [mcp.directory](https://mcp.directory/blog/mcp-context-bloat-fix-2026-tool-search-code-mode-progressive-disclosure)
  - Calling MCP tools from code cut one workflow from 150K to 2K tokens; that is a best case, not typical — [Anthropic, Code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp)
  - Agent Skills (Oct 2025) load in three levels: name and description always, SKILL.md when relevant, linked files on demand — [Anthropic](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills)
  - MCP now sits in the Linux Foundation's Agentic AI Foundation alongside AGENTS.md and goose, with AWS, Google, Microsoft and SAP as members — [CIO Dive](https://www.ciodive.com/news/big-tech-develop-open-standards-agentic-ai/807608/). Sources disagree on the date (Dec 2025 vs May 2026). Security is the top barrier to enterprise adoption — [SD Times](https://sdtimes.com/ai/mcp-dev-summit-standardizing-ai-agents-starting-with-mcp/)

### Inferences
- The Manus cache lesson transfers to local inference. llama.cpp reuses the KV prefix (`cache_prompt`, see the [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)), and prefill is the slow phase on Apple Silicon. A byte-stable system prompt and tool block, append-only history and no per-turn timestamp at the top are probably the cheapest latency win available.
- On a single-slot local server, sub-agents give context isolation but no wall-clock parallelism, and they cost roughly 15x the tokens (§2).
- Truncating tool output with a disk/reference fallback ("saved; call `read_result(id, offset)`") follows the Claude Code and Manus pattern and suits small windows.

### Gaps
- No published measurement of how tool-description quality or parallel tool calls affect 9B-30B open models.
- Gemini CLI's compression threshold was not verified.

## 2. Deep-research agent architectures

### Takeaway
There are two families:
- Prompted orchestrator/worker/citer harnesses on frontier models (Anthropic, LangChain ODR, STORM).
- Single-agent ReAct models trained with RL (OpenAI DR, Tongyi DeepResearch, Search-R1 lineage).

Only RL-trained specialists have pushed open models of 30B or less to about 25-45% on BrowseComp. For 4-9B models, summarizing context inside the loop is required.

### Cited Findings
- Anthropic, 13 Jun 2025 **[frontier]**:
  - Results: an Opus 4 lead with Sonnet 4 workers beat single-agent Opus 4 by 90.2%. On BrowseComp, token usage explains 80% of variance, and tokens, tool-call count and model together explain 95%. Agents use about 4x the tokens of chat; multi-agent systems use about 15x.
  - Planning and delegation: the lead saves its plan to memory because context beyond 200K is truncated. Each worker brief gives an objective, an output format, tool/source guidance and boundaries.
  - Effort rules sit in the prompt: 1 agent and 3-10 calls for a fact; 2-4 workers with 10-15 calls each for a comparison; more than 10 for complex research.
  - A separate CitationAgent runs as a post-pass.
  - Parallelism: 3-5 parallel workers each using 3+ parallel tools cut time by up to 90%.
  - Early agents preferred SEO content farms.
  — [Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)
- BrowseComp (Apr 2025): Deep Research, an RL-trained o3, scored 51.5%; GPT-4o with browsing scored 1.9% — [OpenAI](https://openai.com/index/browsecomp/)
- Tongyi DeepResearch 30B-A3B (Sep 2025; 3B active, Qwen3-MoE):
  - Scores: HLE 32.9, BrowseComp 43.4, BrowseComp-ZH 46.7, xbench-DeepSearch 75 — [MarkTechPost](https://www.marktechpost.com/2025/09/18/alibaba-releases-tongyi-deepresearch-a-30b-parameter-open-source-agentic-llm-optimized-for-long-horizon-research/)
  - Modes: plain ReAct shows the model's intrinsic ability, and a "Heavy" IterResearch mode adds test-time scaling. Training is agentic continued pre-training, then SFT, then on-policy GRPO **[small-OK]** — [HF card](https://huggingface.co/Alibaba-NLP/Tongyi-DeepResearch-30B-A3B)
  - An independent run scored 24.94% on BrowseComp-Plus — [arXiv 2603.01152](https://arxiv.org/pdf/2603.01152)
- Small specialists on BrowseComp, from one table with setups that don't compare cleanly **[small-OK]**: Marco-DR-8B 31.4, MiroThinker-v1.0-8B 31.1, RE-TRAC-4B 30.0, AgentCPM-Explore-4B 24.1, WebExplorer-8B 15.7 — [arXiv 2603.28376](https://arxiv.org/pdf/2603.28376). OpenResearcher-30B-A3B: BrowseComp 26.3, GAIA 64.1 — [arXiv 2603.20278](https://arxiv.org/pdf/2603.20278)
- LiteResearcher (4B): long browsing chains exhaust a 128K window, and summarizing earlier tool interactions at 64K improved results **[small-OK]** — [arXiv 2604.17931](https://arxiv.org/pdf/2604.17931)
- BrowseComp-Plus (ACL 2026) uses a fixed corpus, so retrieval and agent are measured separately. Search-R1 with BM25 scored 3.86%; GPT-5 scored 55.9% — [ACL Anthology](https://aclanthology.org/2026.acl-long.1023/)
- LangChain Open Deep Research (Jul 2025): three phases (scope, then supervisor plus workers, then write). The report is written in one pass because sections written in parallel came out disjoint — [LangChain](https://blog.langchain.com/open-deep-research/)

### Inferences
- A prompted generic Qwen 3.5 9B should score well below the RL-trained 4-8B specialists. The harness levers that remain:
  - Bounded rounds with an effort rule.
  - Summarizing in the loop at about half the window.
  - A separate citation pass.
  - Source-quality priors against content farms.
  - Writing the report once at the end.
- Tongyi DeepResearch 30B-A3B is the most credible local deep-research model for Apple Silicon (3B active). It expects its own ReAct tool format.

### Gaps
- No verified numbers in this pass for Jan-nano, WebSailor, II-Researcher, GPT-Researcher, Gemini DR or Perplexity.
- No primary-source GAIA score for Tongyi.

## 3. Context management and conversation memory

### Takeaway
Every major harness now compacts. The pattern:
- Trigger at a threshold below the window.
- Write a structured multi-section summary.
- Re-attach recent artifacts.
- Fall back to a deterministic drop if the summary still doesn't fit.

On long-term memory, vendor benchmark claims are disputed, and a plain search-over-files agent matches the specialized memory layers.

### Cited Findings
- What to keep: decisions, unresolved bugs and implementation details. What to drop: redundant tool output. Tune for recall first, then for precision — [Anthropic](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- Claude Code (Apr 2026 analysis):
  - Compaction triggers at the window minus a 13K buffer (about 167K of 200K).
  - The summary uses a 9-section schema, and the analysis scratchpad is stripped from it.
  - Up to 5 recent files are re-attached.
  - If the prompt is still too long, it drops the oldest 20% of rounds.
  — [Arize](https://arize.com/blog/context-management-in-agent-harnesses/)
- Codex:
  - OpenAI models use the standalone compact endpoint, which returns an opaque item — [OpenAI compaction guide](https://developers.openai.com/api/docs/guides/compaction)
  - Other providers get a plaintext summarization prompt. Auto-compact is capped at 90% of the window — [Vaughan](https://codex.danielvaughan.com/2026/03/31/codex-cli-context-compaction-architecture/)
  - The default was computed on the raw window, so it actually fires at 94.7% of usable context (a bug) — [openai/codex#40095](https://github.com/openai/codex/issues/40095)
- Long-running work (Nov 2025): an initializer agent writes a progress file, a JSON feature list and git commits, and each fresh session reads them back and does one feature — [Anthropic](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)
- LongMemEval (ICLR 2025; 2024 preprint):
  - Five abilities are tested: extraction, multi-session reasoning, temporal reasoning, knowledge updates, abstention.
  - Assistants drop about 30% in accuracy.
  - What helps: decomposing sessions into facts, expanding keys with facts, and time-aware query expansion.
  — [arXiv 2410.10813](https://arxiv.org/abs/2410.10813)
- Mem0 (Apr 2025), self-reported: +26% relative over OpenAI memory, 91% lower p95 latency, more than 90% token savings — [arXiv 2504.19413](https://arxiv.org/abs/2504.19413). Its LongMemEval score is 94.4 self-reported versus 49.0 in an independent run — [Digital Applied](https://www.digitalapplied.com/blog/open-source-agent-memory-mem0-letta-zep-compared)
- Zep claimed 84% on LoCoMo. Mem0 re-ran it at 58.44%, and Zep corrected to 75.14% — [getzep/zep-papers#5](https://github.com/getzep/zep-papers/issues/5)
- Letta (Aug 2025): a plain filesystem agent on gpt-4o-mini scored 74.0% on LoCoMo — [Letta](https://www.letta.com/blog/benchmarking-ai-agent-memory); corroborated in [arXiv 2605.28969](https://arxiv.org/pdf/2605.28969). That is above Mem0's 68.5% in a third-party table — [Digital Applied](https://www.digitalapplied.com/blog/open-source-agent-memory-mem0-letta-zep-compared)
- MemoryAgentBench (Jul 2025) tests retrieval, test-time learning, long-range understanding and conflict resolution. Conflict resolution is the weakest area — [arXiv 2507.05257](https://arxiv.org/abs/2507.05257)

### Inferences
- Bounded, source-backed memory matches the Letta and LongMemEval evidence. What's missing is time-aware query expansion and explicit handling of updates and conflicts.
- Context rot hits small models earlier, so compacting at about 60-75% of the real `n_ctx` is safer than at 90%. This is an inference, not measured.
- Count tokens with llama.cpp `/apply-template` followed by `/tokenize` ([server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)), not character heuristics. Codex#40095 shows threshold bugs that came from using the wrong window size.
- When compacting, cut only between complete tool-call/result pairs.

### Gaps
- No independent comparison of summary schemas.
- No results for Mem0, Zep or Letta driven by 9B-30B models.

## 4. Structured outputs

### Takeaway
Grammar-constrained decoding makes output valid at little throughput cost and does not hurt accuracy; it slightly helps on small models **[small-OK]**. It does not fix semantic errors, and the bigger practical risk is that the engine can't cover a complex schema.

### Cited Findings
- JSONSchemaBench (Jan 2025): 10K real schemas across 6 engines. On GSM8K, XGrammar scored 83.7 against 80.1 unconstrained. Guidance was best overall, and every engine covered hard schemas poorly — [arXiv 2501.10868](https://arxiv.org/html/2501.10868v3)
- XGrammar-2 (Jan 2026): on BFCL-v3, Llama-3.2-1B's schema validity rose from 22.07% to 100%, and constrained 3B beat unconstrained 70B at function calling — [arXiv 2601.04426](https://arxiv.org/pdf/2601.04426)
- Sep 2026, 0.6B-4B models on 14 tasks with Outlines and XGrammar: validity went from 78.6-92.9% to 100%. Type-coercion errors were fixed, but multi-step function-call semantics were not, and that gap depends on scale. Overhead was 1.6-3.7% of throughput — [arXiv 2609.23742](https://arxiv.org/html/2609.23742v1)
- Aug 2024 (older): format restrictions hurt reasoning — [arXiv 2408.02442](https://arxiv.org/abs/2408.02442). A rebuttal attributes this to prompting and parsing and finds structured output equal or better — [.txt](https://blog.dottxt.co/say-what-you-mean.html)
- llama.cpp's built-in JSON-schema-to-GBNF converter leaves some features unsupported — [grammars README](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md). An optional llguidance backend covers more of JSON Schema — [llguidance doc](https://github.com/ggml-org/llama.cpp/blob/master/docs/llguidance.md)

### Inferences
- "Reason, then emit": put a free-text reasoning field first, or let Qwen think before the grammar takes over. This follows from the 2024 dispute but has not been measured on Qwen 3.5.
- Validity is now handled by the grammar, so spend retries on semantic validation (IDs exist, values in range) with the error fed back to the model.
- Test every production schema against the engine. Silently dropping a grammar, as in Lattice's own bound > 2000 case, is the coverage failure JSONSchemaBench documents.

### Gaps
- No head-to-head of XGrammar and llguidance on 7-30B models.
- Field order not studied in the 2026 paper.

## 5. Verification and grounding

### Takeaway
Small dedicated claim checkers, run per claim against the cited passage, are the 2025-26 default. Each is strongest on its own training domain. Self-correction without an external signal doesn't help, and repairing flagged claims trades grounding against keeping content.

### Cited Findings
- MiniCheck (2024, older): small checkers reach GPT-4-level on LLM-AggreFact at about 400x lower cost **[small-OK]** — [arXiv 2404.10774](https://arxiv.org/abs/2404.10774); [leaderboard](https://llm-aggrefact.github.io/)
- LettuceDetect (Feb 2025): a ModernBERT token classifier with RAGTruth F1 of 79.22%, +14.8% over Luna — [arXiv 2502.17125](https://arxiv.org/html/2502.17125)
- Out of domain, detectors degrade. MiniCheck-7B beats LettuceDetect-L on several columns, and HHEM-2.1 is close — [arXiv 2604.15945](https://arxiv.org/html/2604.15945v1). HHEM-2.1-open is the smallest but strong, and MiniCheck-7B was best among fine-tuned checkers — [arXiv 2505.04847](https://arxiv.org/html/2505.04847v2)
- Repair (Aug 2026, RAGTruth, 916 answers):
  - Answers were split into claims, then fixed by deleting, replacing with source text, or rewriting. All three reduce unsupported content.
  - Deleting reduces it most but keeps only 64.3% of the text. Rewriting keeps 80.1% but reduces it least.
  - 83.5% of answers annotated clean were also edited.
  — [arXiv 2608.29307](https://arxiv.org/abs/2608.29307)
- Self-correction: intrinsic self-correction fails without feedback — [arXiv 2310.01798](https://arxiv.org/abs/2310.01798) (2023). It works only with reliable external feedback — [arXiv 2406.01297](https://arxiv.org/abs/2406.01297) (2024)
- A separate CitationAgent post-pass — [Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system). Weighting a judge's score by its token probabilities improves correlation with humans — [G-Eval, arXiv 2303.16634](https://arxiv.org/abs/2303.16634) (2023)

### Inferences
- Use a dedicated claim checker (MiniCheck, HHEM or LettuceDetect) for each claim-passage pair instead of the chat model. It can run off the critical path while the main model streams.
- Use the judge's yes/no probability as confidence (llama.cpp `n_probs`).
- Flag unsupported claims and offer to delete them rather than rewriting silently, given the 83.5% over-editing rate.

### Gaps
- No calibration study of a Qwen-9B judge.
- No latency figures for these checkers on Apple Silicon.

## 6. Table-stakes harness features (2026)

### Takeaway
Users now expect checkpoints with rewind, resume from failure, cancellation, sandboxed permission tiers, OTel-shaped traces, and prompt versioning with small regression sets.

### Cited Findings
- Claude Code 2.0 (Sep 2025) saves a checkpoint before every edit and adds `/rewind` — [Anthropic](https://www.anthropic.com/news/enabling-claude-code-to-work-more-autonomously). Sandboxing cut permission prompts by 84% — [Anthropic](https://www.anthropic.com/engineering/claude-code-sandboxing)
- Resume from the point of failure, with checkpoints, retries and rainbow deployments — [Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)
- OTel GenAI conventions are still in Development status (as of May 2026). They define create_agent, invoke_agent, invoke_workflow, plan and execute_tool spans; capturing prompts and arguments is opt-in for privacy — [Greptime](https://greptime.com/blogs/2026-05-09-opentelemetry-genai-semantic-conventions); [Dash0](https://www.dash0.com/knowledge/opentelemetry-genai-semantic-conventions-explained)

### Inferences
- Write OTel-named spans (invoke_agent, execute_tool) to a local store with content capture opt-in; that fits a privacy-first app and stays compatible with Langfuse or Phoenix if they're ever added.

### Gaps
- Langfuse's 2026 feature set was not verified in this pass. No primary 2026 source was found for cancellation semantics or prompt-versioning practice, so those parts of the takeaway are practitioner consensus, not cited evidence.

## 7. Agent evaluation

### Takeaway
Check the final state, measure reliability with pass^k, and use a single-call LLM judge with a rubric. Start with about 20 real queries, and grade on outcomes rather than exact trajectories.

### Cited Findings
- tau-bench (2024) introduced the pass^k reliability metric — [arXiv 2406.12045](https://arxiv.org/abs/2406.12045). tau2 (Jun 2025) adds a dual-control telecom domain — [arXiv 2506.07982](https://arxiv.org/abs/2506.07982)
- BFCL v4 weights: 10% non-live, 10% live, 10% irrelevance, 30% multi-turn, 40% agentic (web search and memory) — [arXiv 2609.11977](https://arxiv.org/pdf/2609.11977). Multi-turn is graded on the final environment state — [arXiv 2608.17587](https://arxiv.org/pdf/2608.17587)
- Anthropic's practice:
  - Start with about 20 queries, since early changes have large effects.
  - Judge in a single call that returns a 0-1 score plus pass/fail on a rubric (factual accuracy, citation accuracy, completeness, source quality, tool efficiency).
  - Evaluate the end state, and add human spot checks.
  — [Anthropic](https://www.anthropic.com/engineering/multi-agent-research-system)

### Inferences
- A cheap regression harness:
  - Record and replay tool results so trajectories are deterministic.
  - Assert on the final state, tools used, budgets and citations.
  - Run pass^3 at temperature > 0.
  - Use the local 9B as judge with logprob confidence.
  - Keep about 20-50 real queries per feature.

### Gaps
- No public BFCL v4 or tau2 scores for Qwen 3.5 9B-30B were verified in this pass.
