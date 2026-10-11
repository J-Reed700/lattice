# Grounding verifier: from lexical windows to a real checker

Date: 2026-09-24. Status: proposal, not built. Branch context: `conversation-memory-release-readiness`.

> **Status (2026-10-10).** Partly built since: the judge reads up to three
> windows per source (`WINDOWS_PER_SOURCE` in `chat/verification/judge.rs`), a
> numeric, date or negated claim the judge never reached is `unverified`
> rather than unsupported, the judge budget is 90 s, and `claim_verification`
> calls run at verification priority on the backend's scheduler. Learning
> reuses `claim_verification`. The table below predates these changes; the
> remaining proposals have not been re-checked against the code.

## 1. What we have, and why it fails

`src-tauri/src/features/conversation/chat/verification/` runs two passes after every answer:

1. **Lexical pass** (`lexical.rs`): split the answer into sentences, stem tokens, cut each cited page into 1,200-character windows, take the window with the highest token overlap. Above 0.32 overlap and 2 shared tokens is "supported". Anything with a digit, month or negation is escalated regardless.
2. **Judge** (`judge.rs`): a general chat model (the utility slot, Qwen3.5-9B) gets 12 claims and up to 12 passages as JSON, answers with JSON verdicts and a verbatim quote, under a wall-clock budget. Escalated claims are stamped "unsupported" until the judge overturns them.

Measured on the user's own turns (2026-09-23/24):

| symptom | cause | state |
|---|---|---|
| Every verify step ran 30–32 s and judged ~10 of ~20 claims | one batch ≈ 30 s on the 9B model; 30 s budget | fixed 09-24: 90 s budget, 3 batches in flight |
| 74 of 156 cited claims "Not found" without ever being read | escalated-but-unjudged claims are shown as unsupported | open |
| Judge rejects a claim whose facts are all on the page | it sees **one** 1,200-char window per source, chosen by token count; a sentence that folds two paragraphs together never fits one window | open |
| Claims against pages whose fetch failed | judged against the 300-char search snippet | open |
| Supported needs a verbatim ≤240-char quote from the passage | small models paraphrase; a paraphrased quote demotes a true "supported" to "unsupported" | open |

The shape is the problem, not the constants: sentence-vs-window token overlap is a 2004 summarisation heuristic (ROUGE-era), and "ask a chat model for JSON about twelve things at once" is a 2023 prompt pattern that the fact-checking literature has since replaced.

## 2. What the field does now (2024–2026)

Four families, in increasing cost.

### 2.1 Fine-tuned grounding checkers (document, claim) → P(supported)

Small models trained specifically to answer "does this document support this sentence". The reference benchmark is **LLM-AggreFact** (11 grounded-factuality datasets, balanced accuracy). Current board:

| model | size | BAcc | licence | runs in Lattice today? |
|---|---|---|---|---|
| Bespoke-MiniCheck-7B | 7B | **77.4** | CC BY-NC 4.0 (commercial licence by email) | llama-server: yes (GGUF on Ollama, 4.7 GB) |
| Claude 3.5 Sonnet | – | 77.2 | cloud | no |
| **Granite Guardian 3.3 8B** | 8B | **76.5** (0.761 no-think) | **Apache 2.0** | llama-server: yes (GGUF quantisations published) |
| Mistral Large 2 | 123B | 76.5 | – | no |
| FactCG-DeBERTa-L | 0.4B | 75.6 | research | candle has `debertav2`; untested |
| MiniCheck-Flan-T5-L | 0.8B | 75.0 | MIT | candle has `t5`; untested |
| GPT-4 (2024) | – | 75.3 | cloud | no |
| HHEM-2.1-open | 0.1B | RAGTruth-QA 74.3 | Apache 2.0 | flan-t5-base + custom head; would need porting |

Two things matter for us:

- **A 7–8B fine-tuned checker beats GPT-4-class judges** on this task at a fraction of the cost. MiniCheck's paper reports "GPT-4 accuracy at 400× lower cost". Our judge is a *general* 9B model doing the same job untrained, with JSON overhead on top.
- **The input contract is one sentence at a time.** MiniCheck's own README: multi-sentence claims "should first be broken up into sentences". The output is a single Yes/No token, and the probability comes from the logit of "Yes", not from parsing JSON. No quote extraction, no id bookkeeping, nothing to drop.

### 2.2 Long documents: max over chunks, min over sentences

MiniCheck's reference inference (`minicheck/inference.py`) splits a long document into chunks on sentence boundaries, scores the claim against **every** chunk, takes the **max** across chunks, then the **min** across the claim's sentences. That is the exact answer to our "one window per source" defect: the claim is supported if *any* stretch supports it, and a multi-fact sentence is only as supported as its weakest fact. Bespoke-MiniCheck takes 32k tokens per chunk, so a 14k-character page is one chunk.

### 2.3 Span-level detectors (context, question, answer) → highlighted spans

**LettuceDetect** (KRLabs, 2025; MIT): a ModernBERT-large token classifier (0.4B, 8k-token context) trained on RAGTruth's 18k span-annotated examples. One forward pass over `[context, question, answer]` returns the character spans of the answer that the context does not support, each with a confidence. Example-level F1 on RAGTruth 79.2 vs 63.4 for GPT-4 prompted as a judge; 30–60 examples/s on one GPU. A June 2026 v2 adds a 2B generative variant with typed spans and a smaller `mmbert-base` encoder.

This is a different product surface: instead of a warning icon per sentence, the unsupported *words* are underlined. Lattice's claim-mark UI already draws per-sentence marks; span offsets fit it directly.

**Fit:** Lattice already loads `candle_transformers::models::modernbert` for embeddings, so the encoder runs in-process on Metal with no sidecar, no JSON, no time budget. Weights are safetensors; the classification head is a linear layer over token states.

### 2.4 Decompose → retrieve → verify pipelines

FActScore, SAFE and **VeriScore** (EMNLP 2024) formalised long-form factuality as three stages: extract atomic, decontextualised claims; retrieve evidence *per claim*; verify each claim against its evidence. RAG faithfulness metrics (RAGAS, MedRAGChecker 2026, RT4CHART) use the same skeleton with an NLI or fine-tuned checker in stage three. Two lessons for us:

- Evidence is **retrieved per claim** with the same machinery as search (embeddings + lexical), not picked by one token count over fixed windows.
- Claims are made self-contained before checking (pronouns resolved, elided subjects restored). Our follow-on sentences ("So needing a lot of sun really means…") are checked raw today.

### 2.5 A caution from the other direction

Cleanlab's 2025 benchmark of *real-time* RAG evaluators (FinQA, FinanceBench, ELI5, PubMedQA…) found the tiny encoder HHEM near chance on hard, reasoning-heavy answers (AUROC 0.45–0.54) while an LLM judge (gpt-4o-mini) scored 0.62–0.79 and their multi-sample TLM wrapper 0.75–0.87. Small fine-tuned checkers are excellent at *extractive* grounding and weaker where the answer needs arithmetic or synthesis. So: a fast checker first, an LLM escalation for the hard band, and an evaluation set of our own before believing either.

## 3. Proposal

### 3.1 Architecture

```
answer ──► sentences ──► (optional) decontextualise ──► claims
                                                        │
archived page text ──► sentence/window chunks ──► embed (existing model) ──► per-claim top-k windows (cosine + BM25)
                                                        │
                                       ┌────────────────┴────────────────┐
                                       ▼                                 ▼
                          Stage A: on-device checker           Stage B: LLM checker (escalation)
                          LettuceDetect-large in candle        Granite Guardian 3.3 8B via sidecar,
                          spans + confidence, ~100 ms/claim    document-first prompt, P(yes) from logprobs
                                       │                                 │
                                       └──────► max over windows, min over sentences ──► probability
                                                                                          │
                                                         verdict ∈ {supported, contradicted, unsupported, unverified}
```

**Stage A (default, every claim).** For each claim, concatenate its top-k evidence windows (≤ 8k tokens), run LettuceDetect once. Spans with confidence ≥ τ mark the claim; no spans means supported. This replaces both the lexical pass and most judge calls, and it produces the span highlights.

**Stage B (escalation).** Claims in the uncertain band (Stage A confidence 0.35–0.65), plus numeric/date/negation claims that Stage A passed, go to a fine-tuned yes/no checker on the sidecar. Prompt order is *document first, claim last* so llama-server's default prompt cache (`--cache-prompt`, on by default; slot selection by prefix similarity) reuses the document KV across the claims that cite it. Use `/completion` with `n_probs` to read P(yes) rather than parsing text. Aggregate as in §2.2.

Model choice for Stage B: **Granite Guardian 3.3 8B** (Apache 2.0, 76.5 BAcc, has a `groundedness` mode with documents passed in the chat template). Bespoke-MiniCheck-7B scores 0.9 points higher but is non-commercial. If the user's own chat model is the remote 27B, it can also serve as Stage B with the same yes/no contract; it is a single slot, so it queues.

**Evidence retrieval.** At archive time (`source_snapshots`), chunk the page into ~300-token windows and embed them with the active embedding model; store beside the snapshot. At verify time, embed each claim, take top-k windows per cited source by cosine, and add the top BM25 window as a tiebreaker. This is the same retrieval stack the answer used, so the checker sees what the model saw.

**Verdicts.** Four states, with a probability. `unverified` is new: a claim nothing checked (budget, missing archive, fetch failure). The UI's zod enum (`src/types/conversation.ts`) and `ClaimHoverCard` gain the state and say "not checked" rather than "Not found". Never flip an unchecked claim to unsupported.

**Off the critical path.** Emit the answer's `done` first; run verification as a follow-up step that patches `metadata.verification` and emits a `verification-ready` event. The turn record already has a "Checking the answer" step; it keeps running after the answer is readable. The time budget then becomes a ceiling on background work, not on the user's wait.

### 3.2 Phases

1. **Eval set first (½ day).** Export the 156 persisted claim verdicts plus their cited archived text; hand-label ~100 (the user's own turns are the distribution that matters). Add 200 RAGTruth-QA items. Report BAcc for: current code, Stage A alone, Stage B alone, A+B. Nothing ships without a number.
2. **Evidence retrieval + max/min aggregation (1 day).** Multi-window evidence per claim in the existing judge; `unverified` state; verification off the critical path. This alone removes causes 2–4 from §1 with no new model.
3. **Stage B swap (1 day).** Yes/No checker contract with logprobs; Granite Guardian 3.3 8B as a curated utility model; document-first prompt; remove JSON batching and the verbatim-quote rule (quotes become the span from Stage A, or the highest-scoring evidence window).
4. **Stage A in candle (2–3 days).** LettuceDetect-large token classifier; span offsets into the claim-mark UI; Stage B demoted to escalation.
5. **Decontextualisation (optional).** One small-model call per answer to rewrite follow-on sentences as standalone claims, VeriScore-style. Only if the eval set shows pronoun/ellipsis failures.

### 3.3 Costs and risks

- **Memory.** LettuceDetect-large is ~0.8 GB in fp16 in-process; Granite Guardian 8B Q4 is ~5 GB in the sidecar and replaces, not adds to, the current 9B utility model.
- **Latency.** Stage A: tens of ms per claim on Metal. Stage B: prefix caching makes the second claim against a page cheap; the first pays for the page.
- **Generalisation.** RAGTruth-trained detectors are strongest on extractive QA and summarisation; the user's answers synthesise across pages. The eval set decides whether Stage A can be trusted alone. Keep Stage B for the hard band either way.
- **Licensing.** Everything recommended is MIT or Apache 2.0. Bespoke-MiniCheck is listed for comparison only.

## 4. Sources

- MiniCheck (EMNLP 2024): https://arxiv.org/abs/2404.10774 — chunk/max/min inference: https://github.com/Liyan06/MiniCheck
- LLM-AggreFact leaderboard: https://llm-aggrefact.github.io/
- Bespoke-MiniCheck-7B (CC BY-NC 4.0): https://huggingface.co/bespokelabs/Bespoke-MiniCheck-7B — Ollama GGUF: https://ollama.com/library/bespoke-minicheck
- Granite Guardian 3.3 8B (Apache 2.0, groundedness): https://huggingface.co/ibm-granite/granite-guardian-3.3-8b
- LettuceDetect (MIT, ModernBERT spans): https://arxiv.org/abs/2502.17125 — https://github.com/KRLabsOrg/LettuceDetect — https://huggingface.co/KRLabsOrg/lettucedect-large-modernbert-en-v1
- HHEM-2.1-open (Apache 2.0): https://huggingface.co/vectara/hallucination_evaluation_model
- Paladin-mini (2025, 3.8B grounding model): https://arxiv.org/abs/2506.20384
- Real-Time Evaluation Models for RAG (Cleanlab, 2025): https://arxiv.org/abs/2503.21157
- VeriScore (EMNLP Findings 2024): https://arxiv.org/abs/2406.19276
- MedRAGChecker (2026, claim-level RAG verification): https://arxiv.org/abs/2601.06519
- llama-server prompt cache / `n_probs`: https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
