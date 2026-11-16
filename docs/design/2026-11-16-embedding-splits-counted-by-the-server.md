# Embedding passages counted by the server that embeds them

2026-11-16. Status: building.

## Problem

The embedding splitter counts tokens with a Hugging Face `tokenizer.json` that has to sit beside the GGUF. Few GGUF repos ship one. Of the 720 Hub repos with a GGUF embedding model in an architecture the bundled llama-server runs, 44 ship one. So the conversions people actually search for can't index in Lattice:

- CompendiumLabs bge
- nomic-ai's GGUFs
- the MiniLM GGUFs
- ggml-org embeddinggemma
- Qwen3-Embedding-4B and -8B, the upgrade path from the default

Since #572 the catalog hides them, because they would download and then fail to activate.

The second tokenizer is also a second source of truth. When the two disagree, the server is the one that's right, because its count is what gets embedded. On NFD text the `tokenizer.json` NFC-normalises first: a passage it counts as 133 tokens is embedded as 230. So the splitter can hand the server passages longer than it thinks.

## Decision

The splitter counts with the GGUF's own tokenizer, through the llama-server that embeds the passages (`POST /tokenize`). Nothing reads `tokenizer.json` for embeddings any more.

- **One counter.** The chat path already counts with a llama-server's tokenizer (`ServerTokenizer`, `features/llm/scheduler/tokenize.rs`). It moves to `infrastructure/llama_server/tokenize.rs` and both features use it. Two `/tokenize` clients would be two answers to the same question.
- **What a count covers is the caller's to say.**
  - An embedding input is counted whole, as `/v1/embeddings` tokenizes it: `add_special: true, parse_special: true`. That includes the BOS/EOS the server adds, because the window has to hold them.
  - A chat budget counts a piece's own tokens and charges framing per message, as it does today: `add_special: false`.
  - The request always states both flags.
- **The algorithm doesn't change.** `InputPolicy` keeps its binary search, its readable cuts and its trailing-fragment merge. It counts through a `TokenCounter` it is handed, rather than a tokenizer it owns. The service is the counter, so a count reaches whichever server is running.
- **One restart rule.** A call that fails while the server's process is dead starts the server once more and repeats the call. Every other failure is final. The embed request and the count request share this one helper. Counts start the server again for `EmbeddingWork::Light`.
- **Counts take no slot.** llama-server answers `/tokenize` on its HTTP threads, not in a slot. It runs at least slots + 2 of them, so a count never waits behind an embedding batch. A count does hold the server's in-flight guard, so a server mid-split never looks idle.

## Evidence

All measured with the bundled llama-server 0.7.0 (build 11669) and the curated Qwen3-Embedding-0.6B f16. The corpus was the repo's docs (129 texts, 3.64M characters, 2,359 chunks) plus edge cases: CJK, Arabic and Hindi, emoji, NFD, and a literal special-token string.

- **The count is the count that gets embedded.** For every sample, `/tokenize` equals the `usage.prompt_tokens` that `/v1/embeddings` reported for the same input.
- **Same splits.** Splitting with server counts gives the same chunks as `tokenizer.json` for 128 of the 129 texts. The odd one out is the NFD text: 7 chunks instead of 4, because the server counts the whole text as 2,963 tokens where `tokenizer.json` says 1,684. The server's split is the correct one.
- **Speed.**
  - Mean `/tokenize` time is 1.11 ms. At about 14 counts per chunk, splitting costs about 16 ms a chunk, against about 11 ms with `tokenizer.json`.
  - Embedding that chunk costs far more, so splitting stays a small share of indexing.
  - The tokenizer's cache answers the repeated counts, like the prefix that is counted again at every split.

## Rejected

- **Rebuilding the tokenizer in-process from GGUF metadata.** That would mean reimplementing llama.cpp's BPE, SPM, WPM and UGM tokenizers and their per-model pre-tokenizer rules. It would be a second tokenizer again, and it would drift with every llama.cpp bump.
- **Keeping `tokenizer.json` as a preferred or fallback counter.** That gives two counters, and the one picked depends on which files happen to be present. The NFD case shows the extra one is also less accurate.
- **One `/tokenize` call per text with `with_pieces`, cutting at token offsets.** Fewer calls, but a prefix's count is not the sum of its pieces at a BPE boundary. It would also change every split. The measured speed doesn't call for it.

## Prefixes

Once `tokenizer.json` isn't needed, the popular families can actually run. Each needs the prefixes it was trained with, so the table gains the entries below. Each comes from the model's own `config_sentence_transformers.json` prompts.

| family | query | document |
|---|---|---|
| embeddinggemma 1 and 2 (name, or `gemma-embedding` / `gemma-embedding2` architecture) | `task: search result \| query: ` | `title: none \| text: ` |
| mxbai-embed-large | `Represent this sentence for searching relevant passages: ` | — |
| snowflake arctic-embed v1 and v1.5 | `Represent this sentence for searching relevant passages: ` | — |
| snowflake arctic-embed v2.0 | `query: ` | — |
| jina-embeddings-v5 retrieval | `Query: ` | `Document: ` |
| jina-embeddings-v5, other tasks | — | `Document: ` |

All knowledge of families and architectures stays in `prefixes.rs`, which now also holds the architecture fallback. The jina v5 small GGUFs are `qwen3`, and so far the fallback gave them Qwen3's instruction. Jina v5 is in the catalog today, so that was a live bug: queries got the wrong instruction and documents got none.

A document prefix joins the vector space's identity. Jina v5 and embeddinggemma indexes therefore get a new space and are rebuilt. The query-only entries (mxbai, arctic) leave stored vectors as they are.

## Commits

1. **Refactor, no change in behaviour.**
   - Move `ServerTokenizer`.
   - Add `TokenCounter`, and make the splitter, `EmbeddingPort::split_text` / `window_parts` and their callers async.
   - `connect(server, facts, prefixes)`.
   - The single restart helper.
   - Still counted with `tokenizer.json`.
2. **Tests only.** Server-counted splits, the request's flags and key, a restart for a count, starting and activating without `tokenizer.json`, the curated entry's single file, and the prefix table. They are seen failing on commit 1.
3. **The change.** Count through the server, drop every `tokenizer.json` requirement, and add the prefix entries.

## After this lands

- #572's catalog rule that hides embedding repos without `tokenizer.json` goes, as a test and fix pair on whichever of the two PRs merges second.
- `mb2/split` adds `query_parts` beside `window_parts`. It becomes async the same way.
