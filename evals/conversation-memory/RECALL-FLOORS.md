# The two memory relevance floors

Written 2026-11-02, before any run. Two thresholds in the memory-pools code were set without a measurement:

| Floor | Value | Where | What it decides |
|---|---|---|---|
| `RECALL_RELEVANCE_FLOOR` | 0.25 | `history_tools/recall.rs` | The share of a turn's weight an older message must hold to be recalled for it |
| `SHARED_RELEVANCE_FLOOR` | 0.6 | `lattice-core/.../conversation_memory.rs` | How close in meaning an item or message must be when it shares no word with the turn |

This folder holds the set and the method that measure them, so the floors can be set from data and not guessed.

## The gates

Automatic recall of a chat's older messages lets a candidate through when it is an exact identifier, or its lexical
share of the turn is at least the recall floor, or its cosine similarity to the turn's query is at least the shared
floor. The lexical share is the weight of the turn's words the message holds over the weight of the turn's two rarest
words, counted in that conversation (`relevance_by_message`). It can exceed 1.

A memory item shared from another chat of the same pool is offered to a turn (`prepare_knowledge`) when it is the
chat's own, or shares a word with the turn, or its semantic score (`knowledge_semantic_scores`, the best cosine over
its evidence messages) is at least the shared floor. A chat's own items are not at issue here.

## What is measured

For every probe turn and every candidate it could bring back, the signals those gates read. No floor is applied while
measuring; `scripts/analyse_memory_floors.py` sweeps them afterwards.

- **Recall rows** (`gate: "recall"`): the candidate's `lexical_share` (absent when it holds none of the turn's words),
  its `similarity` and `exact_identifier` flag from the shipped hybrid search, and whether that search returned it
  (`found`). The search is given the query the turn forms with its recent turns, as the app gives it.
- **Shared rows** (`gate: "shared"`): whether the item `shares_word` with the turn, and its `similarity`.

Every row also names the embedding model: `embedding_model`, the vector-space identity the port reports (the GGUF's
hash, with the document prefix's id when the model's family uses one; it is the string stored beside every vector), and
`embedding_name`, its file name. The keys of a
row are `probe`, `kind`, `gate`, `candidate`, `relevant`, `found`, `lexical_share`, `shares_word`, `similarity`,
`exact_identifier`, `embedding_model` and `embedding_name`. A key that does not apply to a gate is null.

## The set

`recall-floors-v1.json`. Synthetic: every person, place and fact is invented.

- **Seven conversations** of 40 messages each, on baking, a PostgreSQL deployment, half-marathon training,
  a vegetable garden, a React form, a household budget sheet and a Python clean-up script.
- **80 recall probes**, 16 of each kind, each asked after a stated assistant message. The conversation is rebuilt as
  it stood then. The probe turn is saved before recall runs, as the chat saves it, so the word index counts it. The
  last four messages before it are its recent turns, which the prompt already carries; the candidates are the older
  messages (22 to 36 per probe).
- **Three pools** of three chats each, 12 items in each, every item a fact the user stated in one chat, quoted from
  its message. **32 shared probes**, 16 of each kind. The candidates are all 12 items of the pool.

| Kind | What the turn is | Relevant candidates |
|---|---|---|
| `exact_restatement` | Restates earlier wording, often with its numbers or identifiers | 31 |
| `paraphrase` | Asks about an earlier exchange in other words, sharing no rare word with it | 38 |
| `follow_up` | A short turn whose meaning comes from the recent turns | 35 |
| `instruction_only` | "Make it shorter", "now as a table", "thanks": nothing behind it | none |
| `distractor` | A new topic that shares common words, or one moderately rare word, with old messages | none |
| `shared_bearing` | A turn that a fact stated in another chat of the pool bears on | 19 |
| `shared_unrelated` | A turn that no item of the pool bears on | none |

A candidate is labelled relevant when it holds information the answer to the turn needs. The user's statement and the
assistant's answer to it count alike. Every other candidate is irrelevant. The set has 2,480 recall candidates and 384
shared ones, so a full run writes 2,864 rows. The labels were written by one author and each probe carries a `note`
giving the reason; they are the weakest part of the method and should be read before the numbers are trusted.

## The runner

`crates/lattice-desktop/tests/memory_floors_eval.rs`, an `#[ignore]`d integration test. It scores every probe through
the shipped code and not a copy of it:

- the database is built by the shipped migrations, and messages are saved with the repository's own methods;
- each message's vector is made as the chat's indexer makes it (`split_text`, then `embed_single`) and stored with
  `persist_memory_vector`, by the embedding model the app loads for conversation memory (`open_gguf`, which starts it on
  a llama-server as the app's loader does); the turn is embedded with `embed_query`;
- recall signals come from `messages_holding` and `relevance_by_message`, with the turn's words, query and exact
  identifiers formed by `what_the_turn_asks`, `build_recall_query` and `exact_identifiers`, and from
  `search_source_messages`, which runs `search_memory_hybrid`, given room for every message;
- shared signals come from `knowledge_semantic_scores`, and `shares_word` from `prepare_memory` on a repository with no
  embedder, which offers an item by shared words alone.

These were `pub(super)` or private and are now `pub` so that the test can call them: `what_the_turn_asks`,
`build_recall_query`, `exact_identifiers`, `relevance_by_message`, `RECALL_RELEVANCE_FLOOR` and
`ConversationRepository::knowledge_semantic_scores`. Nothing else in the shipped code changed.

It stops rather than write a doubtful row:

- a search whose meaning branch did not run is repeated, then stops the run, so a missing similarity always means none;
- every item of a pool must be scored;
- at the shipped `SHARED_RELEVANCE_FLOOR`, the shared signals must reproduce what `prepare_memory` offers with the
  embedder on, probe by probe;
- for each recall probe the shipped `recall_for_turn` is run too, and every older message it anchors a passage group on
  must clear the recall gate by the recall signals at the shipped floors, with the same exact-identifier flag;
- the labelled set is checked for consistency before a model starts, and a message that needs more than one embedding
  chunk is refused;
- the run has a time limit, and nothing is written until every probe is scored.

It writes one JSON line per (probe, candidate), with ids and numbers and no conversation text, to a file that must not
exist, and nothing else. It reads no credential.

### Running it

```sh
SQLX_OFFLINE=true \
LATTICE_EMBED_GGUF=/path/to/Qwen3-Embedding-0.6B-f16.gguf \
LATTICE_EVAL_FLOORS_OUT=/path/to/new-rows.jsonl \
  cargo test --manifest-path crates/lattice-desktop/Cargo.toml \
    --test memory_floors_eval -- --ignored --nocapture
```

| Variable | Meaning | Default |
|---|---|---|
| `LATTICE_EMBED_GGUF` | The embedding model's GGUF, with `tokenizer.json` beside it. Use the model the app has active | required |
| `LATTICE_EVAL_FLOORS_OUT` | The JSONL file to write; it must not exist | required |
| `LATTICE_LLAMA_SERVER` | The llama-server binary | the bundled build in `crates/lattice-desktop/binaries/` |
| `LATTICE_EVAL_FLOORS_SET` | The labelled set | `recall-floors-v1.json` |
| `LATTICE_EVAL_TIME_LIMIT_SECS` | The time limit | 3600 |

Keep the PC awake for the run. Record a run under `results/<embedding model>/`, gzipped, with the analysis output in a
file beside it, one folder per model. Check the `embedding_model` on the rows before quoting a result.

## The analysis

```sh
python3 scripts/analyse_memory_floors.py /path/to/new-rows.jsonl [--step 0.05]
python3 -m unittest scripts/test_analyse_memory_floors.py
```

It refuses rows that name no embedding model or more than one. It applies each gate as the code combines its signals,
with floors compared as 32-bit floats as the code compares them, and prints, per probe kind and per floor from 0 to 1,
the share of the relevant candidates let through (`rel%`) and of the irrelevant ones (`irr%`). A last row, `off`, leaves
the floor's own arm out, which shows what the other arms let through whatever the floor. Then it prints the selection
below and, for comparison, the same figures at the floors now in the code.

## How the floors are chosen

The shared floor is the lowest value at which no more than 5% of unrelated pool items are offered. The recall floor is the lowest value at which no more than 10% of irrelevant older messages are recalled for instruction-only and distractor probes. Both are reported with the share of relevant candidates they keep, and the floors are rounded down to two decimals.

How the analysis reads that, fixed here before the data:

- Unrelated pool items are the candidates labelled irrelevant in the shared rows, from `shared_bearing` and
  `shared_unrelated` probes together. Irrelevant older messages are the candidates of the `instruction_only` and
  `distractor` probes, pooled.
- The recall gate's meaning arm is the shared floor, so the shared floor is chosen first and the recall floor is
  searched with the recall gate's similarity arm at the shared floor chosen, rounded. When no shared floor can be chosen,
  the shipped 0.6 stands in and the output says so.
- "Lowest value" is the lowest on a 0.001 grid, from 0 to 1 for the shared floor and, for the recall floor, to 1 or to
  the highest lexical share seen when that is higher, since a share can exceed 1. Rounding it down can leave it a little
  above its cap; the output then says so and gives the figures at the rounded value, the one that would be set.
- A floor cannot move what it does not govern. When the arms it leaves alone (exact identifiers, a shared word, the
  other floor) already exceed the cap, no value meets the rule, and the output says that and what those arms let through.

## What this does not show

- It is one author's synthetic set: seven conversations and three pools. A floor that fits it may not fit real chats,
  and a share from a few dozen relevant candidates is coarse.
- The recent window is fixed at four messages. The app's holds as many recent messages as the recent-history budget
  allows, so with a large context window it leaves fewer older candidates than this does, and with a small one more.
- It measures the gate, not the answer: that a candidate is let through does not show that the model uses it well.
- One embedding model per run. A different model needs its own run, and its own floors.
