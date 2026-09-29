# Conversation-memory eval results

One folder per model, never mixed: `qwen3.5-9b-local/` (bundled sidecar,
`~/.cache/lattice/models/qwen3.5-9b-q4_k_m`) and `qwen3.8-27b-remote/` (the
remote llama-server chat model). Each file is the printed summary of one run
of `src-tauri/tests/conversation_memory_evals.rs`, named by date and test, with
the `model_identity` line the harness prints. Traces stay in the temp dir: they
contain conversation text and are never committed.
