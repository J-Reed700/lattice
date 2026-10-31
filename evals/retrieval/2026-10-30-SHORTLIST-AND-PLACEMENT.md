# Reading a shortlist, always reranking, and where the three servers run (2026-10-30)

Local Qwen3.5-9B Q4_K_M at a 16,384-token window and the remote Qwen3.8-27B, llama.cpp build 11669 (Vulkan), on a
laptop with an RTX 4070 Laptop GPU (8,188 MiB) beside an Intel Iris Xe. Code: `chat/kb_retrieval.rs`,
`search/engine/llama_reranker/`, `llm/instances.rs`, `interfaces/di/modules/pooling_servers.rs`.

## Why

A turn read every passage the window held. On the 27B that took 11 minutes a turn and 18 model calls, on any
question. Reading the best 40 cuts that to seconds if the 40 hold the answer, and whether they do depends on the
order. The cross-encoder orders them best, and it has to run on every search, which puts a third server on a GPU
with room for two.

## What reading 40 costs

`scripts/replay_shortlist_ab.py` on the 27B's saved turns: 8 questions with 13 rubric facts, the whole window's
answer against the same prompt rebuilt with the first stage's top 40 and the fused top 40.

| Arm | Facts stated | Median seconds to read and answer |
|---|---:|---:|
| Whole window (shipped run) | 13/13 | about 680 |
| First stage, top 40 | 12/13 | 35.5 |
| Fused, top 40 | 12/13 | 38.5 |

The fact lost by both is Dunlark's 70 hours, a passage at rank 211 that no reordering of the first 64 reaches.

## Why fusion and not the reranker alone

A two-hop question needs a bridge passage that is about neither its first hop nor its answer, and a cross-encoder
judging each passage against the question alone pushes it down. Bridge passages found of 17, by depth:

| Top | 5 | 10 | 20 | 30 | 40 |
|---|---:|---:|---:|---:|---:|
| First stage | 11 | 13 | 15 | 16 | 16 |
| Reranker alone | 13 | 13 | 15 | 16 | 16 |
| Rank fusion of both (k = 60) | 14 | 15 | 15 | 16 | 16 |

Fusion is at least as good at every depth, so that is what ships, over 64 candidates.

## Three servers on one card

Whole turns on mains power with nothing else running: embed the question, rerank 64 passages, answer from 20 at 150
tokens, with a unique prompt each round. The medians of rounds after the first:

| Where they run | GPU memory | Embed | Rerank | Answer | Tokens/s |
|---|---:|---:|---:|---:|---:|
| 9B and embedder on the RTX, no reranker (before this) | 7,741 MiB | 0.03 s | | 7.6 s | 39.5 |
| All three on the RTX | past 8 GB | 0.01 s | 2.4 s | **45 s** | **3.7** |
| Embedder on the CPU, every thread | 6,885 MiB | 2.36 s | 2.3 s | 6.7 s | 38.4 |
| Embedder on the CPU, four threads | 6,885 MiB | 0.75 s | 1.9 s | 6.7 s | 37.3 |
| Embedder on the Iris Xe | 6,885 MiB | 0.15 s | 2.3 s | 7.4 s | 37.1 |

All three want about 9 GB of 8. `nvidia-smi` shows use *falling* as the third loads, to about 7,000 MiB: Windows has
paged the rest out, and chat runs at a tenth of its speed from then on.

Where the embedder should go, beside a loaded 9B and reranker, with indexing timed on 96 real passages:

| Embedder on | A query | Indexing |
|---|---:|---:|
| Iris Xe | 0.15 s | 2.0 passages/s |
| CPU, 2 threads | 0.88 s | under 0.64 passages/s |
| CPU, 4 threads | 0.65 s | under 0.64 passages/s |
| CPU, 6 threads | 1.28 s | under 0.64 passages/s |
| CPU, 8 threads | 1.81 s | under 0.64 passages/s |
| RTX, nothing else loaded | 0.03 s | 9.1 passages/s |

The CPU runs did not finish 96 passages in their 150 seconds at any thread count. A pool wider than four gets slower
beside GPU work, not faster. An integrated GPU's first start after a boot took 9.7 s, then 2.4 to 2.6 s, the same as
the CPU.

The reranker on the Iris Xe scores 64 passages in 30 to 35 s, inside its 60 s limit. On the CPU it took 247 to 403 s.

**An earlier round of these timings is void.** The laptop ran unplugged for 92 minutes without anyone noticing, until
it hit critical battery and hibernated. The same layouts on battery read 28 tokens/s and 2 tokens/s, an 80 second
answer, against 38 and 3.7 and 45 seconds on mains. The conclusion held, the numbers did not. Every figure here is
from the rerun, which read the mains state each round and would have stopped had it been lost.

## Pricing what is really on the GPU

Admission priced the wrong device and the wrong amount. The Vulkan build lists the Iris Xe first, with 32 GB of shared
RAM, so the app priced everything against 32 GB, and the estimate had the 9B a gigabyte too large. Neither mattered
while it only warned beside a setting. llama-server's own buffers at the app's flags, with `nvidia-smi` beside them:

| Model | Weights on GPU | Cache | State | Compute | Sum | `nvidia-smi` | Was priced | Now priced |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 9B at 16,384 | 4,861 | 512 | 201 | 120 | 5,694 | 5,721 | 6,615 | 5,680 |
| Reranker | 604 | 448 | | 208 | 1,260 | 1,278 | 1,353 | 1,249 |
| Embedder | 1,136 | 896 | | 96 | 2,128 | 2,147 | 2,334 | 2,182 |

In MiB. Two terms were wrong. The whole file was counted as on the GPU, when a model with a separate `output.weight`
leaves its 545.62 MiB `token_embd.weight` in RAM; a tied one sends a copy up, so its whole file does go. And compute
held logits over the vocabulary for 512 tokens, when the graph is reserved for one output a slot.

`--list-devices` cannot replace the estimate. With the 9B loaded it still reports the idle 7,180 MiB free.

Priced that way, this card seats the 9B, the reranker in the 1 GiB margin, and not the embedder, which is what the
turns above measured.

## End to end

`shipped_path_eval` on the remote 27B, with the reranker on this PC's RTX: the questions of the whole-window run
above, and the two it never reached.

| | Whole window | Shortlist, reranked |
|---|---:|---:|
| Questions | 15 | 17 |
| Median seconds a question | 688 | **44** |
| All questions | 199 minutes | **16 minutes** |
| Model calls a turn | 18.5 | **7** |
| Passing the mechanical checks | 10/15 | **11/15** on the same questions, 13/17 in all |

Of the 15 both ran, `dp-buried-handbook-ticket`, `dp-abstain-home-address` and `dp-abstain-order` now pass, and
`dp-multihop-dunlark-notice` and `dp-abstain-error-code` no longer do.

The checks undercount both runs. `ABSTENTION_RE` does not know "could not find" or "no record", so a right refusal
can fail: this run's `dp-abstain-error-code`, `dp-abstain-named-clause` and `dp-mt-abstain-followup` all refuse
correctly and fail. Read by hand, this run has one wrong answer.

`dp-multihop-dunlark-notice` is that one. The turn names Quorvin Metrology as the supplier, then says no passage
gives a notice period. The 70 hours sits at rank 211, past the 64 reranked, and the fact the whole window found is
out of a shortlist's reach. Search tools were on offer in the round that wrote the answer, and the model did not
call them: the round after a reading tells it to answer. That is the wording
[`2026-10-30-INTENT-AND-SEARCH-FIRST.md`](2026-10-30-INTENT-AND-SEARCH-FIRST.md) tried to change and could not,
because a reading of the whole window left no room for a search's results. It has room now.

## What this does not show

The placement was measured on one laptop. The seating is checked on this machine's real GGUF files and live device
list, and the servers were started by hand with the flags `build_server_args` gives, but no chat has yet run through
the app's own `PoolingServers`. The end-to-end run starts its servers itself, one each with nothing else to fit.

A chat model that fills the card leaves the reranker on the integrated GPU, at about 33 s a search, and on a machine
with no integrated GPU, on the CPU, where it runs out of its 60 s and search keeps its first-stage order for an hour.

Indexing beside a chat costs a reload: between two turns the chat model is idle, so a full batch may unload it, and
the next turn loads it back.

The end-to-end comparison is 15 questions on one model, on a build that differs from the whole-window run by more
than the shortlist. Haiku and Sonnet were not run.

Records: `results/chat/qwen3.8-27b-remote/2026-10-30-shipped-path-reranked.jsonl.gz` (answers and scores), beside
`-w0.jsonl.gz`.
