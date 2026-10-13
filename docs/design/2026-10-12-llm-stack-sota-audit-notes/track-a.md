# Track A: thinking switch, per-mode sampling, sidecar flags, cache telemetry

**Changed.** `llm_port.rs`: `ThinkingMode` (default Off, `for_answer()`), `thinking` and `tool_choice` (B2's exact text) on `CompletionRequest`, `SamplingOverride.presence_penalty` (`deterministic()` sets 0). `llama_cpp/request.rs` (new; `body` moved from `mod.rs`): always sends `chat_template_kwargs.enable_thinking`. With thinking off it sends no `reasoning_effort`, because Qwen 3.8 rejects `"none"`. Thinking is forced off while tools are callable. Adds a sampling profile per mode, `presence_penalty` and `tool_choice`. `streaming.rs`/`retry.rs`: one "llama.cpp request served" log line from `timings`. Ollama: `think:false` unless thinking is on, plus presence and profile. Cloud maps `tool_choice`; Anthropic also thinks when asked. Settings: answer defaults 0.7/0.8/20/repeat 1.0, plus `presencePenalty` 1.5, `answerThinking`, `thinkingSampling` (1.0/0.95/20/1.5/1.0) and `speculativeDecoding` (`contracts/settings/generation.rs`). Sidecar argv adds `--ctx-checkpoints 32 --checkpoint-min-step 1024`, and `-np 1 --spec-type draft-mtp --spec-draft-n-max N` only when `mtp_capable` and the setting are both on. `LocalServerOptions` carries the setting. UI: presence row and `ThinkingSection.tsx`. Bindings regenerated.

**Verified.** `cargo check` was clean. `cargo test --lib -- llama_cpp sidecar_manager settings ollama_client llm::cloud llm::loading llm_port verification::judge recall::study engine::factory` gave 307 passed, 0 failed. tsc clean; vitest passed 20/20. Live runs used `-ngl 99 --ctx-size 4096 -np 1 --jinja`:
- Unsloth 9B Q4_K_M: the template defaults off and never reads `reasoning_effort`, but llama-server injects `enable_thinking=true` when the switch is absent, so it thinks. `"none"` renders like `false`; `"low"` renders like no switch. True and false differ only in the prompt tail. Tools: thinking on 3/3 parsed (#20837 not reproduced); off 3/3 parsed, 0.8 s against 2.3 to 3.5 s. `tool_choice:"none"` keeps tools in the prompt (identical render), but with thinking on the 9B wrote the call as content text, so the answer was empty.
- Laguna: template off, server makes a missing switch on. **Thinking off is broken in this GGUF**: greedy decoding repeats special token 31 to `length`. Tools: off 0/3; on 1/3 parsed, 1/3 written inside the reasoning. `enable_thinking` changes how earlier turns render, so toggling it mutates the prefix.
- `timings` comes on the final usage frame with no flag: `cache_n, prompt_n, prompt_ms, predicted_n, predicted_per_second`, …
- Checkpoints, 9B, min-step 1024 against 8192: identical at 4k context. Editing 85% of the way in kept 2114 of 2630 tokens cached; editing at 50% kept 10.

**Not verified.** Min-step benefit (needs over 8k tokens), checkpoint memory, MTP, the Qwen 3.8 error (from Track D), cloud and Ollama live.

**Risks.** (1) Laguna breaks with thinking off. Fixed in the follow-up below: Laguna always thinks. (2) With B2's tools-kept final round, the 9B can answer with a text call. (3) Presence 1.5 now hits non-greedy JSON utility calls. (4) Saved settings keep 0.9/40/1.1 plus presence 1.5; reset Tuning for the card profile. (5) No `--reasoning-budget`: it is server-wide; -1 stays, bounded by stall timeout and budget. (6) ToolChoice serde is hand-written; if B2 derived it, drop one.

**Integration wiring.** In `tool_loop.rs`, before the loop: `let answer_thinking = container.settings().await.map(|s| s.llm.answer_thinking).unwrap_or(false);`. In each round, after the final-round handling: `native_request.thinking = ThinkingMode::for_answer(answer_thinking, &native_request);`. In `factory.rs` `create_local_llm_sidecar`, beside `config.speculative_decoding = …`: `config.mtp_capable = <catalog mtp_draft for this file>;`.

**Commit message**
```
Switch thinking explicitly and sample each mode by the model card

llama-server turns a missing enable_thinking into "on", and templates
disagree on their own default, so every request now says. An effort
level never was the switch. Tool rounds stay off (llama.cpp #20837).
Answers and thinking get the Qwen 3.5 card profiles with presence
penalty. The sidecar checkpoints every 1024 tokens so hybrid models
roll back less, can draft with an MTP head, and each request logs its
cached vs evaluated prompt tokens to measure prefix stability.
```

## Follow-up: Laguna thinking override

**Changed.** `llm_port.rs` gets `ThinkingPolicy { PerRequest, AlwaysOn, AlwaysOff }` with `mode_for(request)` and `or(fallback)`. A new `engine/thinking_policy.rs` reads `general.architecture` from the GGUF header (`laguna`), falling back to the file name only when the header is unreadable, and returns `AlwaysOn`. The factory passes that into `LlamaCppLlm::sidecar` (new `model_thinking` argument, `ServerConfig.model_thinking`); a remote server stays `PerRequest`. `request.rs` resolves the user's pin, then the model's policy, then the request. A `reasoning_effort` level is sent only when the request itself asked to think, so a forced-on Laguna never gets learning's `"low"`. The escape hatch was cheap: `thinkingOverride: "auto" | "on" | "off"` (default `auto`) goes into `GenerationConfig.thinking_override`, which the llama.cpp and Ollama clients both honour, with a select in `ThinkingSection.tsx`.

**Live (Laguna, `--ctx-size 4096 -np 1`, thinking profile 1.0/0.95/20/1.5/1.0).**
- Prose question, thinking on: a coherent two-sentence answer (2507 reasoning chars, 51 tok/s).
- Arithmetic, thinking on: empty content after 494 tokens. An earlier thinking-on sample at 0.7/0.8/20 answered "391", so thinking on is flaky on that prompt.
- Thinking off at T 0.7: `"| 391 |"` padded out to 495 tokens. The breakage is not greedy-only.

So thinking on is the better mode for this GGUF, not a reliable one; its quantisation (APEX-I-Compact) is a suspect.

**Wiring.** Unchanged.

**Verified.** `cargo check --lib` was clean. `cargo test --lib -- llama_cpp llm_port settings thinking_policy ollama_client` gave 184 passed, 0 failed; that was the second run, after fixing a borrow-order error in one new test. tsc clean; vitest 21/21. Tests cover detection from the header and from the file name, the policy order (user pin, then model, then request), and a forced-on Laguna body: `enable_thinking: true` on tool rounds and utility calls, the thinking profile, and no `reasoning_effort`.
