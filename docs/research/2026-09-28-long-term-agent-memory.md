# Long-term LLM agent memory research

Research memo prepared 2026-09-28. Publication dates below mean arXiv first-submission dates unless a revision is stated; these are preprints, not necessarily peer-reviewed publication dates. Titles, author lists, and dates were checked against the official arXiv abstract pages. PDFs were read for method, result, and ablation details.

## Papers and findings

### 1. Infini Memory: Maintainable Topic Documents for Long-Term LLM Agent Memory

Suozhao Ji, Baodong Wu, Zehao Wang, Lei Xia, Qingping Li, Ruisong Wang, Wenbo Ding, Zhenhua Zhu, Boxun Li, Guohao Dai, and Yu Wang. arXiv:2606.10677, submitted 2026-06-09. [arXiv metadata](https://arxiv.org/abs/2606.10677) · [PDF](https://arxiv.org/pdf/2606.10677)

The system stages observations, periodically consolidates them into topic documents, and supports iterative query-time inspection. On LongMemEval, its ablation reports 79.3% for agentic retrieval, 76.0% for BM25 plus summaries, and 41.7% for summary-only retrieval. Disabling split/merge maintenance drops the hybrid system from 76.0% to 69.3%. This supports bounded topic grouping and follow-up evidence retrieval; it also warns against using summaries as the sole store for exact values, names, and dates. Topic documents should complement, not replace, the app's evidence-linked facts ledger. The paper's scores remain benchmark- and protocol-specific.

### 2. TRUSTMEM: Learning Trustworthy Memory Consolidation for LLM Agents with Long-Term Memory

Tianyu Yang, Sudipta Paul, Vijay Srinivasan, Vivek Kulkarni, and Srinivas Chappidi. arXiv:2606.25161, submitted 2026-06-23. [arXiv metadata](https://arxiv.org/abs/2606.25161) · [PDF](https://arxiv.org/pdf/2606.25161)

Its Memory Transition Verifier checks whether a proposed update covers important new information, preserves valid prior memory, and stays faithful to source/prior state. Removing verifier reward drops the paper's Mem-α aggregate from 66.3 to 58.7; the abstract reports lower transition-level omission, corruption, and hallucination versus the strongest baseline. The transferable idea is to measure these three reviewer dimensions explicitly. The headline gains depend on verifier-guided preference/RL training; they do not establish that adding another serial LLM call will produce the same gains. The current application already has source review, so this is a review-quality and diagnostics refinement.

### 3. LazyMem: Retrieve Broadly, Construct Selectively for Efficient Long-Term Agent Memory

Jing Yu, Yibo Zhao, Jiaming Zhang, and Xiang Li. arXiv:2607.22690, submitted 2026-07-17; v2 revised 2026-07-28. The arXiv page labels it “under review.” [arXiv metadata](https://arxiv.org/abs/2607.22690) · [PDF](https://arxiv.org/pdf/2607.22690)

LazyMem retrieves a candidate pool, then uses overlapping parallel windows to select and compress only query-relevant evidence. It reports LongMemEval judge accuracy 0.85 with 213 answer-context tokens; its 4B model mean latency is 40.86s in the paper's setup, versus 55.35s for NanoMemory. That setup uses two A800 GPUs and up to 64 concurrent history-window calls, so the latency does not transfer to a local desktop/provider setting. Its error audit identifies compression/editing loss, including dropping a second operand and confusing event date with message date. If explored here, use it only as optional query-time processing over retrieved spans, keep source spans available, and do not mutate canonical memory. Their human audit also finds 7.8% judge-label noise on LoCoMo, so score differences require care.

### 4. Retrieval-Driven Memory Reconsolidation for Long-Term LLM Agents

Yuanyi Song, Yukai Wang, Xinbei Ma, Zhihui Fu, Jianghao Lin, Weiwen Liu, Jun Wang, Huarong Deng, Yong Yu, and Weinan Zhang. arXiv:2609.16053, submitted 2026-09-13. [arXiv metadata](https://arxiv.org/abs/2609.16053) · [PDF](https://arxiv.org/pdf/2609.16053)

REALM organizes typed memories as a graph, retrieves through adaptive seed/expand/filter steps, and uses retrieval feedback to strengthen useful local relations or weaken misleading ones. With reconsolidation enabled, reported averages rise from 73.96 to 75.97 on LoCoMo and 62.98 to 65.11 on LongMemEval. A cautious adaptation is to record bounded co-retrieval/usefulness signals and improve candidate expansion/ranking over time. Do not copy its autonomous content mutation policy wholesale: the paper treats forgetting as redundant, whereas this app requires explicit correction/forget behavior and protected mandatory facts. Results use benchmark-specific models and judging.

### 5. Evaluating Memory in LLM Agents via Incremental Multi-Turn Interactions

Yuanzhe Hu, Yu Wang, and Julian McAuley. arXiv:2507.05257, submitted 2025-07-07; v4 revised 2026-06-28. [arXiv metadata](https://arxiv.org/abs/2507.05257) · [PDF](https://arxiv.org/pdf/2507.05257)

MemoryAgentBench evaluates four distinct capabilities: accurate retrieval, test-time learning, long-range understanding, and selective forgetting. It is an evaluation framework rather than an optimization recipe. Use these categories as separate regression dimensions, augmented with app-specific scope isolation, evidence provenance, correction supersession, and mandatory-memory protection.

## Recommendations for this repository

The current Rust memory path already stages extraction, verifies evidence, uses a shared five-minute deadline and bounded work caps, and commits atomically. The repository also has evidence-aware durable facts and lexical/semantic recall. The papers therefore support incremental experiments, not a wholesale rewrite:

1. Establish a regression set across the four MemoryAgentBench capabilities plus app-native scope, provenance, correction, forget, and timeout/no-mutation cases. This is an engineering inference: benchmark first to locate the actual failure mode.
2. If retrieval misses dominate, test bounded topic/episode grouping and iterative local evidence expansion over the existing hybrid retrieval path. Preserve source spans and treat summaries as navigation aids, not exact evidence.
3. If update quality dominates, make reviewer checks for coverage, preservation, and faithfulness explicit and report them independently. Keep the existing staged, atomic commit model.
4. Defer query-time compression or retrieval-feedback graph structure until local latency and quality measurements justify their cost. These are more speculative and can lose details or create provider dependencies.

## Date and evidence caveats

The official arXiv pages explicitly show the first-submission dates and the LazyMem v2 revision date. Thus “recent” here means available on arXiv by 2026-09-28, not confirmed conference publication. The 2026 papers are preprints; LazyMem explicitly says under review. Reported scores should not be compared directly across papers because datasets, model configurations, judges, and protocols differ. LazyMem's own human audit demonstrates judge noise; all results are best treated as evidence for targeted local experiments rather than expected product gains.
