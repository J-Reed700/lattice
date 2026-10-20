# State-of-the-art refresh, 2026-10-19

A web check of what changed since the [2026-10-12 audit](2026-10-12-llm-stack-sota-audit.md),
done the day Josh asked whether Lattice still follows the latest algorithms and
architecture. Three lanes were searched, each limited to work published since about 2026-09-01:
- local models and inference;
- retrieval and grounding;
- agent harness and evals.

Every row below names its primary source and the date it was published. "Vendor numbers"
are the publisher's own benchmarks, not reproduced here.

**Verdict:** nothing published in the window supersedes Lattice's design. Two things
needed work:
- the llama.cpp pin, which had open bugs reported against it;
- whether the evals can tell a real change from noise.

## Acted on

| Finding | Source | What Lattice does |
|---|---|---|
| llama.cpp v0.7.0 (2026-10-15) changes three things: slot save and restore keeps context checkpoints and draft state; an off-by-two that discarded restored SWA states is fixed; and rerank and embedding requests no longer fill the prompt cache. | [release notes](https://github.com/ggml-org/llama.cpp/releases/tag/v0.7.0) | The sidecar is pinned to `v0.7.0`. Every launcher flag, the ready line, `/health`, `/props`, `/tokenize`, the GGML type table and the Qwen-VL and Gemma 4 image token limits were checked unchanged against the tag's source. |
| Qwen3.6-35B-A3B sometimes leaves out the `<tool_call>` opener. The whole call then arrives as `message.content`, with no `tool_calls` (2026-10-07, open). | [#30118](https://github.com/ggml-org/llama.cpp/issues/30118) | A round that offered tools now recovers a call written as text and runs it, instead of showing the call as the answer. |
| Small-N eval statistics: on under ~100 items, naive bootstrap intervals and tests on raw judge scores give false positives, most of all when the judge looks near-perfect (2026-09-20). | [arXiv 2609.35815](https://arxiv.org/abs/2609.35815) | The shipped-path eval compares runs per question with an exact paired test and reports "can't tell from noise". A labelled slice of judge verdicts corrects the pass rate. |
| Synthetic queries pick the wrong retrieval setup: they average 15.7 words against 6.8 for real ones, and their keyword side helped only the long ones (CIKM 2026, 2026-09-13). | [arXiv 2609.14579](https://arxiv.org/abs/2609.14579) | A terse slice of the v3 retrieval set, matched to the length of real queries in Lattice, is reported beside the full set. |
| Multi-turn RAG loses up to 21%, from query-rewrite drift and failure to combine evidence across turns (2026-09-29). | [arXiv 2609.36700](https://arxiv.org/abs/2609.36700) | Multi-turn cases join the desktop-path question set. |

## Waiting on upstream

- **Gemma 4 and JSON schema** ([#30256](https://github.com/ggml-org/llama.cpp/issues/30256),
  [#30759](https://github.com/ggml-org/llama.cpp/issues/30759)).
  - The bug: a `response_format` schema grammar lets Gemma 4 open a thought channel even with
    `enable_thinking: false`. On long prompts the channel never closes, and the reply ends at the length limit with
    empty content. It was reproduced on build 11429 (the old pin) and is still in v0.7.0.
  - The fix is open as [#30722](https://github.com/ggml-org/llama.cpp/pull/30722). Pin the
    first release that contains it.
  - Until then, Gemma 4 schema calls on long prompts can come back empty. The reporter's
    workaround needs a JSON-schema-to-GBNF converter, which isn't worth building for a bug
    whose fix is already in review.
- **SWA checkpoints** ([#30666](https://github.com/ggml-org/llama.cpp/pull/30666), merged
  2026-10-19, after v0.7.0). It stops the server restoring a checkpoint saved from an unrelated earlier prompt.
  It arrives with the next release.

## Try once the evals can show it

| Candidate | Source | Note |
|---|---|---|
| One fixed tool list per turn, narrowed with `tool_choice` or the grammar | [gufo#619](https://github.com/gufo-org/gufo/issues/619), 2026-10-16 | llama.cpp's `tool_choice` reaches only the grammar and parser, not the template, so the prompt prefix stays cacheable. Measure `timings.cache_n`. |
| `--cache-ram` sized from free memory | [#29324](https://github.com/ggml-org/llama.cpp/issues/29324) | `-1` is not unlimited. Hybrid models grow about 640 MiB per short prompt. |
| jina-reranker-v3.5 (0.6B) | [jina.ai](https://jina.ai/news/jina-reranker-v3-5-faster-listwise-reranking-hybrid-attention-self-distillation), 2026-08-03 | BEIR 63.2 against 56.9 for Qwen3-Reranker-0.6B (vendor numbers). The licence is non-commercial, and there is no GGUF. |
| LightOnOCR-3 (0.8B, Apache-2.0) | [model card](https://huggingface.co/lightonai/LightOnOCR-3-0.8B), 2026-10-06 | Uses the Qwen3.5-VL architecture, so it fits the existing `--mmproj` launcher. Compare with PaddleOCR-VL. |
| Muse Glimmer 30B (Apache-2.0) | [model card](https://huggingface.co/meta-models/Muse-Glimmer-30B) | For the 32 GB+ tier. Ahead of Qwen3.6-27B on MCP Atlas, IFBench and AA-LCR (vendor numbers). Thinking is set by a system-prompt line, not `enable_thinking`. |
| An answer-completeness check | [arXiv 2609.35774](https://arxiv.org/abs/2609.35774), v3 2026-10-10 | The largest effect (d=+0.48) in a 2^4 RAG-feature ablation. Small study. |
| A retrieval-sufficiency signal from the score distribution | [arXiv 2609.11646](https://arxiv.org/abs/2609.11646) | Could trigger the corrective retry without an LLM call. Transfers poorly across domains (AUROC 0.856 → 0.706). |

## Skipped

| Item | Why |
|---|---|
| Agent-controlled forgetting ([2610.10590](https://arxiv.org/abs/2610.10590)) | Editing context in place breaks the stable prefix. Mixed results from one author. |
| LangChain deep-agent memory thresholds | Fixed thresholds with no measurements. |
| Per-request reasoning budget ([#25961](https://github.com/ggml-org/llama.cpp/pull/25961)) | A fixed cap, and it lowered accuracy in a contributor's runs. Thinking stays off when tools are offered. |
| LFM2.5-24B-A2B | It always thinks, and its tool calls are Pythonic by default. |
| Hy-Memory-30B, Nemotron-3.5-Rerank-8B | Wrong fit, or too large for the target machines. |

**Nothing new came out in the window for:**
- a successor to Qwen 3.5 at Lattice's sizes;
- the claim checkers (MiniCheck, HHEM, LettuceDetect);
- open deep-research models;
- anonymous web search for agents.
