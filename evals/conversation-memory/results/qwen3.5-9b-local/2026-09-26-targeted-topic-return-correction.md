# targeted_bounded_memory_families_from_env — topic_return, mid_conversation_correction — 1 repeat — 2026-09-26T02:24Z

After the schema-grammar, deterministic-sampling and sidecar-deadline fixes. Sidecar: bundled llama-server-aarch64-apple-darwin, --parallel 2 --ctx-size 32768, LATTICE_EVAL_CONCURRENCY=2.

── topic_return · bounded_memory
   capability: Exact facts from an early exchange are recovered after a long unrelated stretch, quoting the original turn.
   model: harness=conversation-memory-eval/2026-09-23.1 provider=llamacpp utility=Qwen3.5-9B-Q4_K_M.gguf continuation=Qwen3.5-9B-Q4_K_M.gguf endpoint=http://127.0.0.1:8089/v1 context_tokens=16384 request_timeout_secs=300 compaction_deadline_secs=900 provider_attempts=3 retry_base_delay_ms=1000 extractor_prompt=memory-extractor/2026-09-20.12 verifier_prompt=memory-verifier/2026-09-20.7 summarizer_prompt=memory-summarizer/2026-09-19.1 validator=memory-validator/6+memory-verifier/2026-09-20.7 authenticated=false
   runs: 1
   active-evidence recall:             0.000 (0/1)
   mandatory-item floor shortfall:     0
   evidence-resolution rate:           1.000 (2/2)
   unquoted active items:              0
   cross-conversation evidence spans:  0
   non-user authority violations:      0
   duplicate active requirements:      0
   conditionals kept as open questions: n/a
   correct supersessions:              n/a
   silent disappearances:              0
   unresolved conflicts recorded:      0
   unexpected/duplicate conflicts:      0
   expected conflicts missing:         0
   answers carrying required quotation: 1.000 (2/2)
   answers containing forbidden text:  0
   deterministic answer contract:      1.000 (2/2)
   continuation provider failures:     0
   required evidence reached prompt:   1.000 (2/2)
   honest abstention:                  n/a
   forbidden tool attempts:            0
   read-only tool calls:               0
   tool-round limit exhaustions:       0
   peak input tokens:                  2097
   total input tokens:                 4181
   budget violations:                  0
   overflow reported (explicit):       0
   extraction / review / repair calls: 10 / 1 / 0
   oversized utility responses:        0
   recoverable utility failures:       0
   provider retries / recovered:       0 / 0
   provider retry exhaustions:         0
   continuation latency p50/p95 ms:    23205 / 23205
   utility-call latency p50/p95 ms:    11647 / 22699
   active mandatory by cycle:          [(1, 2), (2, 2), (3, 2), (4, 2), (5, 2), (6, 2), (7, 2), (8, 2), (9, 2), (10, 2)]

── mid_conversation_correction · bounded_memory
   capability: A corrected budget replaces the old value for the named project only, and the earlier value stays answerable as history.
   model: harness=conversation-memory-eval/2026-09-23.1 provider=llamacpp utility=Qwen3.5-9B-Q4_K_M.gguf continuation=Qwen3.5-9B-Q4_K_M.gguf endpoint=http://127.0.0.1:8089/v1 context_tokens=16384 request_timeout_secs=300 compaction_deadline_secs=900 provider_attempts=3 retry_base_delay_ms=1000 extractor_prompt=memory-extractor/2026-09-20.12 verifier_prompt=memory-verifier/2026-09-20.7 summarizer_prompt=memory-summarizer/2026-09-19.1 validator=memory-validator/6+memory-verifier/2026-09-20.7 authenticated=false
   runs: 1
   active-evidence recall:             1.000 (2/2)
   mandatory-item floor shortfall:     0
   evidence-resolution rate:           n/a
   unquoted active items:              0
   cross-conversation evidence spans:  0
   non-user authority violations:      0
   duplicate active requirements:      0
   conditionals kept as open questions: n/a
   correct supersessions:              1.000 (1/1)
   silent disappearances:              0
   unresolved conflicts recorded:      0
   unexpected/duplicate conflicts:      0
   expected conflicts missing:         0
   answers carrying required quotation: 1.000 (3/3)
   answers containing forbidden text:  0
   deterministic answer contract:      1.000 (3/3)
   continuation provider failures:     0
   required evidence reached prompt:   1.000 (3/3)
   honest abstention:                  n/a
   forbidden tool attempts:            0
   read-only tool calls:               0
   tool-round limit exhaustions:       0
   peak input tokens:                  2217
   total input tokens:                 6642
   budget violations:                  0
   overflow reported (explicit):       0
   extraction / review / repair calls: 11 / 1 / 1
   oversized utility responses:        0
   recoverable utility failures:       0
   provider retries / recovered:       0 / 0
   provider retry exhaustions:         0
   continuation latency p50/p95 ms:    12964 / 19270
   utility-call latency p50/p95 ms:    11581 / 79149
   active mandatory by cycle:          [(1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0)]



