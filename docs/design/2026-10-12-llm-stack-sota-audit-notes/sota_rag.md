# SOTA local RAG for a privacy-first desktop vault (as of 2026-10-12)

Scope: on-device on Apple Silicon (16-64 GB), llama.cpp, Qwen 3.5-class 9B-30B generators, vault of thousands to tens of thousands of chunks. Evidence grades: **[strong]** = several controlled evals agree; **[moderate]** = one good controlled eval or consistent vendor numbers; **[weak]** = vendor or anecdotal only. Items marked (pre-2025) are older baselines.

## Chunking: what wins, what sizes, and whether to expand to parent or neighbour chunks

### Takeaway
No single splitter wins everywhere. Structure-aware chunking (paragraph, section or page) of about 256-1024 tokens beats naive fixed-character splits. Semantic and LLM chunkers rarely justify their cost. Adding document context to each chunk (contextual retrieval) has the largest measured gain, though from a 2024 vendor eval.

### Cited Findings
- Contextual retrieval (an LLM-written 50-100 token context prepended to each chunk before embedding and BM25) cut top-20 retrieval failures by 35% (5.7%→3.7%). With contextual BM25 the cut was 49% (→2.9%), and with a reranker 67% (→1.9%) **[moderate; vendor, Sep 2024]** — [Anthropic](https://www.anthropic.com/news/contextual-retrieval)
- Late chunking against contextual retrieval (ECIR 2025 workshop, Apr 2025): contextual retrieval "preserves semantic coherence more effectively" but costs more compute. Late chunking is cheaper but "sacrifices relevance and completeness" — [arXiv 2504.19754](https://arxiv.org/abs/2504.19754)
- Feb 2026 reproducibility study: structure-based methods (fixed, sentence, paragraph) beat LLM-guided chunkers for in-corpus retrieval. LumberChunker is best for in-document lookup. Contextualized (late) chunking helps in-corpus retrieval but *hurts* in-document retrieval. Chunk size correlates only weakly with in-corpus effectiveness **[moderate]** — [arXiv 2602.16974](https://arxiv.org/abs/2602.16974)
- Mar 2026 benchmark (36 methods, 6 domains, 5 embedders): Paragraph-Group chunking scored best (mean nDCG@5 ≈0.459) against fixed-character chunking (<0.244). Larger embedders score higher but stay sensitive to bad segmentation — [arXiv 2603.06976](https://arxiv.org/abs/2603.06976)
- Semantic chunking's compute cost "not justified by consistent performance gains" against fixed-size chunking (NAACL Findings 2025) **[moderate]** — [arXiv 2410.13070](https://arxiv.org/abs/2410.13070)
- NVIDIA (Jun 2025, 5 PDF datasets): page-level chunking had the best mean end-to-end accuracy (0.648, lowest variance). Token chunks scored 0.603-0.645, and 128 tokens was worst. 15% overlap beat 10% and 20% in one test. Suggested sizes are 256-512 tokens for factoid questions and about 1024 tokens or page-level for analytical ones — [NVIDIA blog](https://developer.nvidia.com/blog/finding-the-best-chunking-strategy-for-accurate-ai-responses/)
- Chroma (Jul 2024, pre-2025): a recursive splitter at about 200-400 tokens with *no* overlap was a strong default. Overlap lowered token-level IoU, and cluster-semantic and LLM chunkers did only marginally better — [Chroma research](https://research.trychroma.com/evaluating-chunking)
- Late chunking (2024): embed the whole document with a long-context encoder, then mean-pool per chunk span — [arXiv 2409.04701](https://arxiv.org/abs/2409.04701)

### Inferences
- For a vault, the best value is markdown/heading/page-aware splitting at 256-1024 tokens with a heading-path header on each chunk. The upgrade with the clearest return is LLM-generated chunk context at index time. On a local 9B model this is a one-off cost per document, and prefix-caching the document makes it cheaper.
- Late chunking is cheap with a 32K-context embedder such as Qwen3-Embedding. Its in-document regression matters for "find the passage in this note" queries.

### Gaps
- I found no controlled eval of chunk size or overlap specific to Qwen3-Embedding.
- I found no rigorous 2025-26 study of parent-child or neighbour-window expansion. The evidence is framework docs and anecdote only.

## Embedding models runnable on-device

### Takeaway
Qwen3-Embedding (0.6B/4B/8B, Jun 2025) remains the open reference. jina-embeddings-v5-text (Feb 2026) and EmbeddingGemma (Sep 2025) are the strongest sub-1B options. All support Matryoshka dimension truncation and want query-side instructions.

### Cited Findings
- Qwen3-Embedding MMTEB means are 64.33 (0.6B), 69.45 (4B) and 70.58 (8B, #1 multilingual at release). The family supports 32K context, MRL custom dimensions and instruction-aware queries — [arXiv 2506.05176](https://arxiv.org/abs/2506.05176)
- Qwen card: omitting the query instruction typically costs 1-5% retrieval quality. Documents are embedded without one — [HF Qwen3-Embedding-8B](https://huggingface.co/Qwen/Qwen3-Embedding-8B)
- jina-embeddings-v5-text (18 Feb 2026) comes in small (677M, Qwen3-0.6B base, 32K context, 1024-d) and nano (239M, 8K, 768-d). Both are distilled from Qwen3-Embedding-4B. Reported MMTEB is 67.0 (small) and 65.5 (nano), and llama.cpp and MLX are supported. AWS lists different numbers, so treat these as vendor-reported — [Jina](https://jina.ai/news/jina-embeddings-v5-text-distilling-4b-quality-into-sub-1b-multilingual-embeddings); [HF](https://huggingface.co/jinaai/jina-embeddings-v5-text-small); [Elastic](https://www.elastic.co/search-labs/blog/jina-embeddings-v5-text)
- EmbeddingGemma (308M): the top-ranked open multilingual embedder under 500M at release, with MRL 768→128 and under 200 MB RAM when quantized — [Google](https://developers.googleblog.com/en/introducing-embeddinggemma/); [arXiv 2509.20354](https://arxiv.org/abs/2509.20354)
- jina-embeddings-v4 (3.8B, Jun 2025) outputs single- and multi-vector embeddings over text and page images for visually rich documents — [arXiv 2506.18902](https://arxiv.org/abs/2506.18902). Qwen3-VL-Embedding and VL-Reranker (Jan 2026) are unified multimodal retrievers — [arXiv 2601.04720](https://arxiv.org/abs/2601.04720)
- Leaderboard caveats: the top proprietary model on the April 2026 snapshot was Gemini Embedding. RAG builders should read the Retrieval column, not the average. These come from secondary blogs and are unverified — [Techsy](https://techsy.io/en/blog/mteb-score-explained-rag)
- ColPali (2024) uses page-image multi-vector retrieval of about 1k patch vectors per page — [arXiv 2407.01449](https://arxiv.org/abs/2407.01449)

### Inferences
- On 16 GB Macs, Qwen3-Embedding-0.6B or jina-v5-small is the sensible default. On 32-64 GB, Qwen3-Embedding-4B gains about 5 MMTEB points. MRL truncation to 512-1024 dimensions keeps the index small at vault scale.
- ColPali-style multi-vector storage is feasible at about 10k pages. llama.cpp has no late-interaction scoring path, so it would need custom MaxSim code. **[unverified]**

### Gaps
- I did not reach the live MTEB v2 leaderboard. No independent 2026 on-device latency numbers on Apple Silicon were found.
- I did not check nomic-embed v2, Granite R2, snowflake-arctic-embed 2 or bge-m3 against 2026 releases.

## Rerankers

### Takeaway
Cross-encoder or listwise rerankers are the best-replicated retrieval gain **[strong]**. The 2026 tier is jina-reranker-v3/v3.5 (0.6B) about equal to Qwen3-Reranker-4B, which is ahead of Qwen3-Reranker-0.6B. Published evals rerank the top 100.

### Cited Findings
- jina-reranker-v3 (0.6B, listwise) scores BEIR 61.94 against Qwen3-Reranker-4B at 61.16 and 0.6B at 56.28, on the top 100 from jina-v3. v3.5 adds +1.10 BEIR, and under that protocol Qwen3-4B edges out v3 (62.28 vs 62.10) — [Jina](https://jina.ai/news/jina-reranker-v3-0-6b-listwise-reranker-for-sota-multilingual-retrieval/); [HF v3.5](https://huggingface.co/jinaai/jina-reranker-v3.5)
- Third-party run (Jun 2026, 14 BEIR sets): jina-v3 61.71, Qwen3-Reranker-8B 61.81, 4B 59.92, 0.6B **48.09**. The 0.6B model is fragile outside its own protocol — [arXiv 2606.19037](https://arxiv.org/abs/2606.19037)
- Qwen3-Reranker gains over a Qwen3-Embedding-0.6B first stage on the top 100 (MTEB-R): base 61.82; rerankers 65.80 (0.6B), 69.76 (4B), 69.02 (8B); bge-reranker-v2-m3 57.03 — [HF Qwen3-Reranker-4B](https://huggingface.co/Qwen/Qwen3-Reranker-4B)
- mxbai-rerank-v2 (Mar 2025; 0.5B and 1.5B, Qwen2.5-based) — [Mixedbread](https://www.mixedbread.com/blog/mxbai-rerank-v2)
- llama.cpp exposes `--reranking` and `/v1/rerank` — [llama.cpp server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md). Some Qwen3-Reranker GGUF conversions return near-zero scores and wrong rankings through `/v1/rerank`. This is one model card's claim — [HF card](https://huggingface.co/cicaba/Qwen3-Reranker-0.6B-GGUF-llamacpp-fixed)
- E2Rank (Oct 2025) unifies the embedder and a listwise reranker in one model — [arXiv 2510.22733](https://arxiv.org/abs/2510.22733)

### Inferences
- If a reranker shows "no gain" in-house, check GGUF conversion correctness (the yes/no-logit template) before abandoning reranking. The published gains are large and replicated.
- Reasonable depth: rerank the top 50-100 hybrid candidates, then feed the generator the top 5-10.

### Gaps
- I found no measured reranker latency on Apple Silicon (llama.cpp or ONNX) and no k=20/50/100 depth sweep for local models.

## Hybrid and sparse retrieval and fusion

### Takeaway
Hybrid BM25+dense is standard and consistently helps on rare terms, names and codes **[strong, mostly pre-2025]**. A tuned convex combination beats RRF when a small labelled set exists. Learned sparse adds little over BM25 given a strong dense model.

### Cited Findings
- Contextual BM25 on top of contextual embeddings lifted the failure reduction from 35% to 49% — [Anthropic](https://www.anthropic.com/news/contextual-retrieval)
- Convex combination of normalized scores beat RRF in- and out-of-domain and is sample-efficient to tune. RRF is sensitive to its k parameter (2023, pre-2025) — [arXiv 2210.11934](https://arxiv.org/abs/2210.11934)
- bge-m3 produces dense, sparse and multi-vector outputs from one model, and all three combined beat any single one on MIRACL (2024) — [arXiv 2402.03216](https://arxiv.org/abs/2402.03216)

### Inferences
- For a personal vault, keep BM25 (cheap, exact-match on identifiers). Use RRF until an eval set exists, then fit convex weights. SPLADE is a poor trade on-device.

### Gaps
- I found no 2025-26 study of learned sparse against BM25 on personal or note corpora.

## Query transformation and agentic retrieval

### Takeaway
Rewriting conversational queries into standalone ones is cheap and standard. HyDE and multi-query help weak retrievers and can hurt strong ones. Agentic loops help multi-hop questions at a large latency cost, and 2026 evidence on whether they beat a well-built single shot is mixed.

### Cited Findings
- LLM query or document expansion helps weaker retrievers but *degrades* strong ones across many datasets (2023-24, pre-2025) — [arXiv 2309.08541](https://arxiv.org/abs/2309.08541). HyDE origin (2022) — [arXiv 2212.10496](https://arxiv.org/abs/2212.10496)
- Search-R1 (RL-trained searcher on Qwen2.5-3B/7B) reports roughly 20-41% relative gains over RAG baselines on QA. The training needs GPUs, but the resulting 7B models run locally — [arXiv 2503.09516](https://arxiv.org/abs/2503.09516)
- "Is Agentic RAG worth it?" (Jan/Apr 2026, ACL Industry) compares enhanced (modular) against agentic RAG. A summary reports both gain similarly as LLM strength rises, and agentic refinement loops added nothing in one setting. Details are unverified because only the abstract was read — [arXiv 2601.07711](https://arxiv.org/abs/2601.07711)
- An operational case (Jun 2026) found agentic retrieval beat single-shot on answer quality at higher latency — [arXiv 2606.04755](https://arxiv.org/abs/2606.04755)
- Corrective RAG grades retrieval and falls back or refines on low confidence (2024) — [arXiv 2401.15884](https://arxiv.org/abs/2401.15884). Adaptive-RAG routes by query complexity (2024) — [arXiv 2403.14403](https://arxiv.org/abs/2403.14403)

### Inferences
- An intent router should choose *how much* to retrieve, never whether to skip the vault for anything that might reference personal content. A false "no retrieval" is silent and cannot be recovered. Run a cheap always-on vault probe and gate on its retrieval score, CRAG-style, instead of on a classifier alone.
- HyDE should be off by default with a Qwen3-class embedder and kept only if a local eval shows a gain.

### Gaps
- No 2026 study isolates sub-10B local models in agentic search loops.

## Long context versus RAG, compression and KV reuse

### Takeaway
RAG still wins at 128K+ and for weaker or open models. Effective context is far below advertised context. Provence-style pruning is the deployable compressor. KV-reuse research (CacheBlend, Block-Attention, TurboRAG) is not usable in llama.cpp beyond prefix caching.

### Cited Findings
- LaRA (ICML 2025, 11 models): no silver bullet. Long context wins on average at 32K, RAG wins at 128K, weaker models gain more from RAG, and RAG is better at hallucination detection — [PMLR](https://proceedings.mlr.press/v267/li25dv.html)
- NoLiMa (Feb 2025): with literal-match cues removed, most models fall below 50% of their short-context score by 32K — [arXiv 2502.05167](https://arxiv.org/abs/2502.05167). Chroma "Context Rot" (Jul 2025, 18 models): performance degrades as input grows, even on simple tasks — [Chroma](https://research.trychroma.com/context-rot)
- Provence (ICLR 2025, NAVER): a sentence-level pruner unified with the reranker, with "negligible to no drop" across domains — [arXiv 2501.16214](https://arxiv.org/abs/2501.16214)
- LLMLingua-2 (2024): 2-5x compression and 1.6-2.9x end-to-end speedup — [arXiv 2403.12968](https://arxiv.org/abs/2403.12968). RECOMP (2023) — [arXiv 2310.04408](https://arxiv.org/abs/2310.04408). xRAG (2024) needs a trained projector into the generator — [arXiv 2405.13792](https://arxiv.org/abs/2405.13792)
- CacheBlend (EuroSys 2025) cuts TTFT 2.2-3.3x by reusing non-prefix chunk KV with selective recompute (vLLM/GPU) — [arXiv 2405.16444](https://arxiv.org/abs/2405.16444). Block-Attention (ICLR 2025) and TurboRAG (up to 9.4x TTFT) need **generator fine-tuning** — [arXiv 2409.15355](https://arxiv.org/abs/2409.15355); [arXiv 2410.07590](https://arxiv.org/abs/2410.07590). RAGCache (2024) — [arXiv 2404.12457](https://arxiv.org/abs/2404.12457)
- llama.cpp offers prefix caching, `--cache-reuse N` (KV shifting for matching chunks) and slot save/restore. It has no CacheBlend-style blending — [llama.cpp server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)

### Inferences
- Keep the prompt prefix stable (system prompt, then tools, then retrieved chunks, then the question) to maximize llama.cpp prefix reuse. Do not plan around CacheBlend on-device.

### Gaps
- No 2026 LaRA-style study on Qwen 3.5-class models was found.

## Graph and memory-augmented RAG

### Takeaway
GraphRAG helps multi-hop and global-summary questions and loses on simple fact lookup. Index cost is high on a local 9B model, and the 2025-26 consensus is "selective, not default".

### Cited Findings
- Systematic comparison (Feb 2025, revised to Mar 2026): RAG and GraphRAG have distinct task strengths, and combining them helps — [arXiv 2502.11371](https://arxiv.org/abs/2502.11371). GraphRAG was about 13.4% less accurate than vanilla RAG on NQ, with added latency (GraphRAG-Bench analysis, Jun 2025) — [arXiv 2506.05690](https://arxiv.org/abs/2506.05690)
- HippoRAG 2 (Feb 2025) reports about +7 points on associative multi-hop tasks over a strong embedder without losing simple-QA performance — [arXiv 2502.14802](https://arxiv.org/abs/2502.14802)
- LazyGraphRAG: indexing cost equal to vector RAG and 0.1% of full GraphRAG, because summarization is deferred to query time — [Microsoft Research](https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/). LightRAG (2024) — [arXiv 2410.05779](https://arxiv.org/abs/2410.05779)

### Inferences
- At vault scale, cheap entity and link structure (wikilinks, entity co-occurrence for HippoRAG-like expansion) beats full LLM-extracted community graphs.

### Gaps
- No graph-RAG evals on personal-note corpora were found.

## Document parsing

### Takeaway
Small sub-1.5B document VLMs now lead OmniDocBench and are on-device candidates. Pipeline tools remain the fast default.

### Cited Findings
- PaddleOCR-VL (0.9B, Oct 2025) scores 92.86 on OmniDocBench v1.5 — [arXiv 2510.14528](https://arxiv.org/abs/2510.14528). PaddleOCR-VL-1.5 (Jan 2026) scores 94.50 — [arXiv 2601.21957](https://arxiv.org/abs/2601.21957). PaddleOCR-VL-1.6 (Jun 2026) scores 96.33 on v1.6 — [arXiv 2606.03264](https://arxiv.org/abs/2606.03264)
- On v1.6, MinerU2.5-Pro (1.2B) scores 95.75, GLM-OCR (0.9B) 95.22 and the original olmOCR (7B) 85.74 — [Roboflow](https://blog.roboflow.com/best-open-source-ocr-models/)
- On real photographed or degraded pages, MinerU2.5 scores 85.61 and PaddleOCR-VL-1.5 scores 92.05 — [arXiv 2603.04205](https://arxiv.org/abs/2603.04205)
- OmniDocBench (CVPR 2025) is the benchmark behind these scores — [arXiv 2412.07626](https://arxiv.org/abs/2412.07626). Docling is IBM's pipeline toolkit — [arXiv 2408.09869](https://arxiv.org/abs/2408.09869). granite-docling-258M (Sep 2025) — [HF](https://huggingface.co/ibm-granite/granite-docling-258M)

### Inferences
- Use a text-layer extractor first and route scanned, table-heavy or garbled pages to a sub-1B document VLM (GGUF or MLX).

### Gaps
- I found no Apple Silicon pages/sec figures and no Docling OmniDocBench score.

## Evaluation and faithfulness checking

### Takeaway
Measure retrieval (recall@k, nDCG) separately from generation. Use small, purpose-trained claim checkers rather than the generator judging itself.

### Cited Findings
- MiniCheck (2024): the 770M MiniCheck-FT5 matches GPT-4 on LLM-AggreFact (75.3 vs 74.7 BAcc) at roughly 400x lower cost — [arXiv 2404.10774](https://arxiv.org/abs/2404.10774). Bespoke-MiniCheck-7B is reported as top open checker (about 77.4), per an aggregator — [LLM-AggreFact](https://llm-aggrefact.github.io/); [HF](https://huggingface.co/bespokelabs/Bespoke-MiniCheck-7B)
- LettuceDetect (Feb 2025) is a ModernBERT token-level span detector reaching 79.22% F1 on RAGTruth — [arXiv 2502.17125](https://arxiv.org/abs/2502.17125). Vectara HHEM-2.1-open is a small CPU-capable checker — [HF](https://huggingface.co/vectara/hallucination_evaluation_model)
- Frameworks: RAGAS (2023) — [arXiv 2309.15217](https://arxiv.org/abs/2309.15217). ARES (2023) uses a small labelled set and prediction-powered inference to calibrate synthetic judges — [arXiv 2311.09476](https://arxiv.org/abs/2311.09476). LLM-judge position and self-preference biases (2023) — [arXiv 2306.05685](https://arxiv.org/abs/2306.05685)

### Inferences
- Calibrate any LLM judge against about 100 hand-labelled items (the ARES-style idea). Score claims against the specific cited span, not the whole document.

### Gaps
- No 2026 LLM-AggreFact snapshot was verified, and no Apple Silicon checker latency figures were found.
