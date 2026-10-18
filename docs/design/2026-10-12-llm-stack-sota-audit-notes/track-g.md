# Track G — MTP speculative decoding on Qwen3.5-9B (measurement)

MTP drafting works on Josh's M3 Max (36 GB, Metal) with the bundled v0.6.0 `llama-server`. At `--spec-draft-n-max 3` it makes decoding **1.55x faster** (geometric mean over five prompt types; range 1.32x for prose to 1.75x for JSON), and **1.83x faster** while thinking. N=3 beats N=2, 4 and 6. There were no errors or crashes. Greedy output is the same for every N, but on 2 of the 5 prompts it differs from the same weights decoded without speculation. No application code was changed.

## Setup

- **Binary:** `crates/lattice-desktop/binaries/llama-server-aarch64-apple-darwin`, `version: 0.6.0-dev (build 11429, commit d81235049)`. MTP-related `--help` text, verbatim:
  - `--spec-type none,draft-simple,draft-eagle3,draft-mtp,draft-dflash,draft-dspark,ngram-simple,ngram-map-k,ngram-map-k4v,ngram-mod,ngram-cache` "comma-separated list of types of speculative decoding to use (default: none)"
  - `--spec-draft-n-max N` "number of tokens to draft for speculative decoding (default: 3)"
  - `--spec-draft-n-min N` "minimum number of draft tokens to use for speculative decoding (default: 0)"
  - `--spec-draft-p-min, --draft-p-min P` "minimum speculative decoding probability (greedy) (default: 0.00)"
  - `--spec-draft-sampling {greedy,probabilistic}` (default greedy)
  - `-ctxcp, --ctx-checkpoints N` (default 32); `-cms, --checkpoint-min-step N` (default 8192); `-rea, --reasoning [on|off|auto]`
  - The old `--draft-max` and `--draft-min` flags are removed and now error.
- **Server arguments:** the same as the app's sidecar, plus metrics. `-ngl 99 --ctx-size 8192 --jinja --ctx-checkpoints 32 --checkpoint-min-step 1024 -np 1 --metrics --no-webui`, with `--spec-type draft-mtp --spec-draft-n-max N` added for speculation.
- **MTP file:** downloaded once from `unsloth/Qwen3.5-9B-MTP-GGUF` to `~/.cache/lattice-evals/models/qwen3.5-9b-mtp/Qwen3.5-9B-Q4_K_M.gguf`. Size 5,868,826,976 and sha256 `e8dd9481…841fe` both match the catalog.
- **MTP head confirmed in the GGUF header:**
  - `qwen35.nextn_predict_layers = 1` and `block_count = 33` (the plain file has 32).
  - `blk.32` holds a full attention and FFN block plus `nextn.eh_proj`, `nextn.enorm`, `nextn.hnorm` and `nextn.shared_head_norm`.
  - When speculation is on, the server logs `creating MTP draft context against the target model`. When it is off, it logs `model has unused tensor blk.32.* -- ignoring`.
- **The two files' main weights are not identical:** 75 of the 427 shared tensors are quantized differently. In the MTP file, `ssm_alpha` and `ssm_beta` are F32 instead of Q8_0, and `attn_qkv` is Q4_K or Q6_K instead of Q5_K. The comment in `curated.rs` that calls it "the same weights plus one MTP block" is therefore wrong at the quantization level.
- **Requests:** `/v1/chat/completions`, non-streaming (the response carries the same `timings` object as the final stream frame), `temperature 0`, `max_tokens 256`, `seed 0`, `cache_prompt false`, `chat_template_kwargs.enable_thinking=false`. Each server start got one warm-up request, which was discarded.
  - P1: a short fact (30 tokens out).
  - P2: a 250-word explanation.
  - P3: a 60-line Python function.
  - P4: a RAG prompt of 2,316 tokens (a passage of about 2,000 tokens plus a question).
  - P5: a JSON object with 8 fields (119 tokens out).
- **Repetitions:** every configuration ran in **2 separate server sessions × 2 reps = 4 samples**. Round 2 ran in reverse order, because background load drifted. CarPlay Simulator, an Xcode git helper, Discord and later another agent's `rustc` builds kept the load average at 11–16. Baseline sessions ranged from 40.2 to 43.4 tok/s, so differences under about 5% are noise.
- **Spec counters:** `/metrics` counters (`llamacpp:spec_decode_num_draft_tokens_total`, `…accepted_tokens_total`, `…drafts_total`) equal the sum of the per-request `draft_n` and `draft_n_accepted` exactly.

## Decode tok/s (mean of 4)

| Config | P1 fact | P2 prose | P3 code | P4 RAG | P5 JSON | mean |
|---|---|---|---|---|---|---|
| (a) plain 9B | 42.3 | 42.2 | 42.2 | 41.0 | 41.5 | 41.9 |
| (b) MTP file, no spec | 41.5 | 41.0 | 41.4 | 40.5 | 40.4 | 41.0 |
| (c) MTP, N=2 | 51.0 | 48.4 | 53.0 | 50.6 | 57.4 | 52.1 |
| **(f) MTP, N=3** | **67.4** | **55.6** | 66.3 | 64.2 | 72.6 | **65.2** |
| (d) MTP, N=4 | 59.6 | 52.7 | **67.4** | **66.2** | 68.6 | 62.9 |
| (e) MTP, N=6 | 57.2 | 46.6 | 65.7 | 62.7 | **74.8** | 61.4 |

The two sessions of each configuration agreed within 1–3%, except (c), which ran in the noisiest periods. A third N=3 session gave 66.1, 54.8, 65.8, 63.3 and 69.9 tok/s.

**Speed-up vs (a), per prompt and geometric mean:**
- N=2: 1.21, 1.15, 1.26, 1.23, 1.38 (1.24x)
- **N=3: 1.59, 1.32, 1.57, 1.56, 1.75 (1.55x)**
- N=4: 1.41, 1.25, 1.60, 1.61, 1.65 (1.50x)
- N=6: 1.35, 1.10, 1.56, 1.53, 1.80 (1.45x)
- (b) vs (a): 0.97–0.99x. The unused head costs nothing beyond the slightly heavier quant mix.

**Acceptance (accepted/drafted, with tokens per target pass in brackets):**

| N | P1 | P2 | P3 | P4 | P5 | all |
|---|---|---|---|---|---|---|
| 2 | 86% (2.73) | 71% (2.41) | 83% (2.65) | 78% (2.57) | 90% (2.81) | 79% |
| 3 | 74% (3.22) | 56% (2.66) | 73% (3.17) | 71% (3.14) | 82% (3.47) | 68% |
| 4 | 56% (3.22) | 48% (2.90) | 69% (3.74) | 69% (3.75) | 70% (3.81) | 62% |
| 6 | 44% (3.62) | 33% (2.97) | 54% (4.16) | 52% (4.10) | 62% (4.72) | 47% |

Raising N past 3 lengthens accepted runs, but each extra draft is another sequential MTP pass through the 248k-vocabulary output head. On prose (P2) and short answers (P1) that cost exceeds the gain.

**Prompt processing and end-to-end time:**
- Prompt processing is not consistently slower with MTP. P4 ran at 493 tok/s for (a) and for N=3, and 478–489 for N=4 and N=6. The lower readings for (b) (448) and N=2 (392) followed the background load.
- End-to-end time (prompt plus decode) at N=3 vs (a): P4 10.9 s → 8.7 s, P2 6.2 → 4.8 s, P5 3.0 → 1.9 s.

**Thinking on** (`enable_thinking: true`, one run per prompt; all 256 tokens were reasoning):

| Config | tok/s | Speed-up | Acceptance |
|---|---|---|---|
| no spec | 39.6 | — | — |
| N=2 | 55.3 | 1.40x | 87% |
| N=3 | 72.3 | 1.83x | 81% |
| N=4 | 70.2 | 1.77x | 75% |

MTP works while thinking, and reasoning text accepts drafts more often than answers do.

## Memory and load

| | Peak RSS | Peak phys_footprint | `memory_pressure` free (idle → loaded) | "model loaded" log stamp | Start → `/health` |
|---|---|---|---|---|---|
| plain, no spec | 6.6–6.7 GiB | 1.50–1.65 GB | 78–79% → 61–63% | 0.88–0.91 s | 1.2 s |
| MTP, N=2–6 | 7.0–7.5 GiB | 1.91–2.18 GB | 78% → 60% | 1.07–1.26 s | 1.2–1.5 s |

- Speculation adds about 0.7 GiB of RSS and 0.5 GB of private footprint (the draft context, its KV and larger graphs). System-wide that is under 1 point of free memory. N barely matters, though N=6 is the largest.
- Both files were already in the page cache, so cold-disk load time was not measured. Starts during another agent's cargo build took 2.6–3.5 s.

## Is greedy output byte-identical?

- **(a) vs (b): no.** P2, P4 and P5 differ, which is expected because the quant mixes differ.
- **Every speculative run gives the same text.** That holds across N=2, 3, 4 and 6, all sessions and all reps: one distinct output per prompt.
- **Against the same weights without speculation (b): P1, P3 and P5 are identical; P2 and P4 are not.**
  - P2 diverges at character 380: "the refrigerant enters as a low-pressure liquid" vs "low-pressure liquid refrigerant absorbs heat".
  - P4 diverges at character 614: the closing `**` moves.
  - With thinking on, P1's reasoning diverges at character 345. The other four are identical.
- **Likely cause (inferred, not proven):** the target scores drafts in a batch of 1+N tokens, which runs different Metal kernels than single-token decode. The tiny numeric differences flip near-ties under greedy decoding. Because the output does not change with N, this looks like batch numerics, not a drafting bug. Both versions read correctly.
- **Consequence:** evals that pin exact greedy text must pin the speculation setting.

## Errors

- No request errors, no server crashes and no non-zero exits.
- The single-server guard once saw a transient `llama-server-aarch64` process from elsewhere, so one session was skipped and rerun.
- A harness bug left one idle server up for about a minute before I killed it. No second server ever ran alongside it.
- The final `pgrep -fl llama-server-aarch64` is empty.

## Recommendation

Turn `speculativeDecoding` on by default for MTP-capable models, with **N=3**, the server's own default.

- **Why N=3:** it is fastest overall (1.55x for answers, 1.83x while thinking) and has the best worst case (1.32x on prose). N=4 is within noise on code and RAG but loses on prose and short answers. N=2 leaves a third of the gain unused. N=6 only wins on JSON.
- **Cost:** about 0.7 GiB RAM and a 3% larger download.
- **One cost not measured here:** the MTP path forces `-np 1`. The shared sidecar otherwise runs automatic parallel slots (`n_parallel = 4`), so anything that sends concurrent requests to the same server will now queue: the grounding judge's three concurrent calls, and utility or router roles running beside a chat answer.
- **Before shipping the default:** time one grounding-verified turn with MTP on and off. If those paths queue badly, keep the default on only where the chat model has its own server.
- **Catalog follow-ups:** the description's "speed-up has not been measured here yet" can now cite these numbers, and the "same weights" comment needs the quant-mix correction above.
