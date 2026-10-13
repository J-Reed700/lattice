# Track E1: evaluate what ships

Headless runner for the desktop's own chat turn, a 34-item rubric set, a pass^3 judge, and the pre-upgrade baseline on `70cc8c92`. Uncommitted.

## Baseline (Qwen3.5-9B Q4_K_M, local, 16K window, thinking off)

- **Pass rate.** 20 of 34 items pass the judge at pass^3 (0.59, 95% CI 0.42–0.74), and 22 of 34 pass the mechanical checks.
- **Latency.** 28 s median to the answer (p95 61 s); grounding check a median 20 s later.
- **No utility model.** 20/34 on the judge, 21/34 mechanical, 21 s median.
- **v1 set.** 21/38 mechanical, against 25/38 for the 09-25 component harness. These are different experiments, not a regression.

## Defects found

1. **The budget can't trim evidence.** At 8K, 31 of 34 items fail at `context_assembler/budget.rs:432` (5,847–8,691 tokens against 5,734 allowed); at 16K, 3 turns per arm fail. Evidence is sized without reserving room for tool definitions, so machines auto-sizing near 8K can't answer. Fix: reserve those tokens before sizing evidence, or shrink evidence first.
2. **The planner never planned.** It, and so the corrective retry, runs only when a utility model is set. Even then its 53–62k-character catalog prompt overflowed the window 12 times and hit the 20 s timeout 4 times. Each timeout stands it down for 10 minutes: 0 of 16 succeeded.
3. **Intent sends vault questions to the public web.** It did so on 4–6 of 41 turns, and 25 of 38 on v1. In at least two answers the web pages overrode the vault's answer.
4. **Minor.** The model made no tool calls in 82 turns, a `corrective_search` step is recorded when no retry ran, and `ERR-4070` missed its own runbook.

## Files

- **Library surface.** `src-tauri/src/features/conversation/chat/headless.rs`, a `pub` wrapper over `run_turn` with its `pub mod` line in `chat.rs`. No other visibility changes.
- **Runner.** `src-tauri/examples/shipped_path_eval/{main,app,proxy,recipe,record}.rs`. The proxy records each model request and the server's cached vs evaluated tokens without touching `features/llm`.
- **Scorer.** `scripts/shipped_path_eval.py` and `scripts/test_shipped_path_rag_eval.py`. `validate_rag_evals.py` now checks desktop-path sets.
- **Evals.** `evals/retrieval/desktop-path-v1.json` (34 items, 44 turns, 9 multi-turn), four `shipped-path-recipe-9b-local*.json` recipes, `2026-10-12-SHIPPED-PATH-BASELINE.md`, and the 2026-10-13 runs in `results/chat/qwen3.5-9b-local/` (call logs gzipped). `README.md` and `EVALUATION_PROTOCOL.md` link to them.

## Verified

- `python3 -m unittest discover -s scripts -p 'test_*rag_eval.py'`: 83 OK.
- `validate_rag_evals.py`: OK. `cargo fmt --check`: clean.
- `cargo check` and `cargo build --example shipped_path_eval` (dev).
- `cargo test --lib features::conversation::chat`: 364 passed.
- The weights sha256 `03b74727…` matches Hugging Face.
- With no switch the server thinks (526 reasoning characters on a spot check), and 0 with the switch.
- One server, `-np 1`, killed afterwards; `pgrep` confirmed it gone.

## Not verified, and risks

- **Judge.** It is the same 9B, so a smoke-level signal. Its greedy grades never split, so pass^3 equals pass^1 here, and it failed one correct turn.
- **Repeatability.** One app run per item; true pass^k needs repeats.
- **Sidecar.** The server runs as a user-run llama.cpp server: no prefill guard, no auto-sized window, and the utility role shares the chat model's single slot.
- **Hermeticity.** Web-searched turns depend on live pages.
- **Passage origin.** Rows can't tell openings and section hits from search hits.

## Suggested commit

```
Measure the shipped chat path with a headless runner and rubric set

Every quality number so far measured a component, never the turn a
user gets. Drive the desktop's own run_turn headlessly, record each
model request, and grade 34 desktop-path questions mechanically and
with a pass^3 judge. The baseline finds budget failures at 8K, a
planner that never plans on a 9B, and vault questions sent to the web.
```
