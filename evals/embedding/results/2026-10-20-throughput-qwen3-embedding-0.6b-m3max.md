# Throughput: Qwen3-Embedding-0.6B F16 on the bundled llama-server, M3 Max

- Date: 2026-10-20. M3 Max (36 GB), Metal, sidecar `llama/v0.7.0-r1`, window 2,048 tokens a slot, `-fa auto`.
- Test: `embedding_throughput_on_the_llama_server` (`#[ignore]`), 160 real passages per set. The passages are the owner's own text and are not committed.
- "index loop" is `passage_index::build::run` end to end: store reads, embedding at `BATCH_SIZE` 16, vectors and saves.

| Set (tokens) | Slots | Model, 16 per call | Model, whole set | Index loop |
|---|---|---|---|---|
| code, MusicVST (64,599) | 1 | 16.62/s · 6,710 tok/s | 16.92/s | 16.72/s |
| | 2 | 16.25/s · 6,562 tok/s | 16.95/s | 16.61/s |
| | 4 | 16.98/s · 6,855 tok/s | 16.81/s | 16.22/s |
| code, drift-temp (79,638) | 1 | 13.70/s · 6,821 tok/s | 13.78/s | 13.50/s |
| | 2 | 13.76/s · 6,850 tok/s | 13.64/s | 13.33/s |
| | 4 | 13.62/s · 6,780 tok/s | 13.58/s | 13.39/s |
| prose, course widths (42,483) | 1 | 22.49/s · 5,972 tok/s | 23.08/s | 21.98/s |
| | 2 | 23.44/s · 6,224 tok/s | 23.06/s | 23.69/s |
| | 4 | 25.05/s · 6,650 tok/s | 24.73/s | 24.61/s |

Compared with Candle on the same machine after #134 (one passage per pass, about 3,300–3,800 tok/s), the index loop runs:
- prose at 24.6/s against 13.9/s;
- drift-temp code at 13.4/s against 6.5/s.

The slot count barely moves the pace on Metal: 1, 2 and 4 slots are within run noise on code, and within about 12% on prose. The extra slots cost KV memory, about 112 KiB per token of window. Whether to ship fewer slots is a separate, measured follow-up (design §18 Q5).

Not measured here: Vulkan and the CPU build (design §15).
