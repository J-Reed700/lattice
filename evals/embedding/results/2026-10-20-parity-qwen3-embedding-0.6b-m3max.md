# Parity: Qwen3-Embedding-0.6B F16 GGUF vs Candle, M3 Max

- Date: 2026-10-20. Branch `feat/llama-embed-cleanup`, sidecar `llama/v0.7.0-r1` (Metal), `-fa auto`.
- GGUF: `Qwen3-Embedding-0.6B-f16.gguf`, `sha256:421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340`.
- Reference: `reference/qwen3-embedding-0.6b/candle-metal-f16.f32` (200 passages, 40 queries).
- Test: `embedding_parity_with_the_candle_reference` (`#[ignore]`).

| Check | Result |
|---|---|
| Lowest cosine over 240 vectors | 0.999992 (row 202) |
| Queries whose top-1 passage agrees | 40 / 40 |
| Server token count equals Candle's per passage | 200 / 200 |

Verdict: same vector space. An index built with Candle stays valid under the GGUF's identity.
