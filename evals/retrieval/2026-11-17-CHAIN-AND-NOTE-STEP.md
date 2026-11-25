# The chat's chain and note step on the 27B (2026-11-17)

Qwen3.8-27B on the remote llama-server the app is set to (the server reports `/models/Qwen3.8-27B-Q5_K_S.gguf`,
196,608 tokens of context in one slot). Code: `chat/tool_loop/` (`reading.rs`, `reading/notes.rs`, `calls.rs`,
`final_round.rs`, `research_notes.rs`), origin/main at 616587e48.

**The chain answers as well as it did, and a turn takes 1.8 times as long.** Read by hand, the 27B answered 49 of the
fifty questions right (66 of 67 turns). The last two 27B runs, on 2026-11-06, answered all fifty. The one miss is a
follow-up whose reading kept the wrong sentence. The mechanical scorer gave this run 5/50 because it looked for
citations with a pattern older than the chat's `[sN]` handles; #588 has it read the chat's own reply pieces instead.

A turn took 65.4 s at the median, against 36.9 s and 36.3 s, and the reading's output accounts for more than the
whole increase. Since #570 the reading answers in JSON under a schema. It writes 2.9 times the tokens it did, and at
under half the speed, because the server drafts no request that has a schema. Since these runs #598 has the reading
answer in plain lines with no schema, one line for each passage that keeps something (decision 3); no write-up in
`evals/` measures it yet.

**The note step has two halves, and the sets cover one each.** The reading's notes (`reading/notes.rs`) are written on
every turn of the fifty questions. Condensing (`research_notes.rs`) has never fired in a chat turn, not in this run and
not in any recorded one, and by the loop's own arithmetic it cannot on these questions at any window from 16K to 196K.
The app run at 16K bears that out: no turn condensed, and seven of the eight that searched again ran out of window
first. Condensing's only eval is the offline replay, level with the shipped step at 616587e48. On the 27B it kept
every planted fact with its citation, 304 of 304 and 288 of 288, against 304 and 264 on 2026-10-28, at about twice the
time per turn. Since then #598 has condensing answer in plain lines with no schema; the replay now asks and reads as
main does, and has not been run that way.

## What each set exercises

| Step | Code | The fifty questions (`shipped_path_eval`) | The replay (`replay_condense_ab.py`) |
|---|---|---|---|
| Read the passages first, a note on each | `reading.rs`, `reading/notes.rs` | every turn | no |
| Search again after reading | `calls.rs` | 9 of 67 turns, 8 at 16K | no |
| The last round: window full, out of rounds | `final_round.rs` | 7 turns out of rounds; at 16K, 7 with the window full | no |
| Condensing the results into notes | `research_notes.rs` | never | every turn: 60 at 16K, 12 packed |

## The chain on the fifty questions

| Run | Code | Right, read by hand | Mechanical | Turns with a call | Calls | Condensed | Median s/turn |
|---|---|---:|---:|---:|---:|---:|---:|
| 2026-11-06 | f920d152, a combination tree | 50/50 | 48/50 | 7 | 29 | 0 | 36.9 |
| 2026-11-06 | 31d92851, a combination tree | 50/50 | 46/50 | 9 | 28 | 0 | 36.3 |
| **2026-11-17** | **main at 616587e48** | **49/50** | **49/50** | **9** | **41** | **0** | **65.4** |
| 2026-11-17 | main at 616587e48, the app shown 16K | 48/50 | 47/50 | 8 | 11 | 0 | 59.5 |

- **The miss is a follow-up.** In `dp-mt-encryption-versions`, the third turn asks "Is there any way round it?" after
  two turns about the workstation encryption standard. The reading is asked the follow-up in the learner's words.
  From the standard's exception passage it kept only the sentence "There is no other way round it: a local addendum
  cannot create one and a manager cannot grant one". It dropped the sentence before it, which names the Gidgegate
  exception, and kept the same sentence from another standard's exception passage. The answer opened "No — there is
  no way round it", quoted the other standard's Wrixworth exception and never named Gidgegate. Both earlier runs
  named it. One question is within what runs differ by, and it was not run again.
- **The earlier runs' mechanical failures are right answers.** Read by hand, each one is a right answer that the
  scorer failed. Some name the other two Ostrel dates as a contrast, which the rubric's must-not list catches. Others
  quote a sentence with a word missing, which the quote check catches.
- **Searching again after reading.** 9 turns made a call, 41 calls in all, against 7 and 29, and 9 and 28. Six of the
  nine ask for something the library does not hold: the five abstain questions and the abstain follow-up. Each of
  those searched, found nothing and said so.
- **Seven turns used every round, against two and one.** A chat turn may spend five model rounds, and the fifth may
  not call a tool (`max_tool_rounds`, `final_round.rs`). Six of the seven are the abstain turns, and the seventh,
  `dp-section-after-clause`, read on three times through the agreement before it answered. 16 of the 41 calls were
  `read_more`, which #570 added to read on from where a result stopped, and four of the abstain turns used it on
  what their searches found. All seven answers are right. In the 2026-11-06 runs most abstain turns stopped
  searching on their own after two or three rounds. No turn ended with the window full or out of time, and none
  condensed.
- **Thinking was off on every request, and the app turned it off itself.** All 611 chat requests carried
  `enable_thinking: false`, the recording proxy added it to none of them, and no reply held reasoning text.

## Where the time went

The server's own timings for each request, summed by kind, for main and for f920d152. 31d92851 took 2,696 s and wrote
61,792 tokens, close to f920d152 on every row.

| Kind of request | main: requests | seconds | output tokens | tokens/s | f920d152: requests | seconds | output tokens | tokens/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| One-token request before the reading | 0 | 0 | | | 67 | 710 | | |
| The reading | 174 | 2,874 | 62,180 | 30.9 | 67 | 788 | 21,815 | 72.6 |
| Chat rounds | 110 | 887 | 10,796 | 49.5 | 96 | 373 | 11,365 | 51.4 |
| Claim checks | 193 | 272 | 13,999 | 76.2 | 207 | 320 | 16,338 | 79.0 |
| Memory | 67 | 47 | 387 | 36.2 | 71 | 321 | 6,812 | 36.3 |
| Classifying the question | 67 | 149 | 4,541 | 36.5 | 67 | 143 | 4,565 | 38.1 |
| All | 611 | 4,230 | 91,903 | | 575 | 2,654 | 60,895 | |

- **The reading writes layout.** Since #570 the reading answers with a JSON list of entries under a schema. Each
  entry is a passage's handle and the handles of the sentences it keeps. 2,465 of its 2,686 entries kept nothing.
  Each entry cost 23 tokens, and the replies came indented, 2.1 times the length of the same JSON written compactly.
  Its 62,180 output tokens are 2.9 times the 21,815 of the free-text lines it replaced.
- **The server drafts no request with a schema.** The remote server runs speculative decoding. On the free-text
  reading it accepted 16,250 of 16,477 drafted tokens, hence 72.6 tokens a second. Every request with a schema ran
  with no drafted tokens at 31 to 38 tokens a second: the reading now, and the classifier and the memory step in
  both runs.
- **That is more than the whole increase.** The reading's generation took 2,015 s against 301 s, 1,714 s more. The
  run's model time grew by 1,576 s, 4,230 s against 2,654 s. Prompt work hardly changed, 1,677 s against 1,619 s.
  The earlier runs sent a one-token request before each reading that put the turn's prompt in the server's cache,
  and the reading and the chat round then shared it. Main sends none: its reading sends the passages in batches of
  its own, so the chat round processes half its prompt itself (49% against 10%). The memory step now writes almost
  nothing, 387 tokens against 6,812.
- **The PC's load does not enter.** These are the server's timings, and the heavy-run lock kept other 27B work off it.
  The median turn also includes searching and reranking on this PC, which was busy during the first 20 minutes.

## The chain at 16K

The same fifty questions, with the app shown a window of 16,384 tokens: the recording proxy answers `GET /props` with
that `n_ctx`, and the app's context window setting matches. The server kept its 196,608 tokens and its one slot.

- **48 of 50 right by hand, 65 of 67 turns.** One question went unanswered: at 15:28 UTC the server reloaded its
  model and answered the first turn of `dp-abstain-named-clause` with 502 and then 503 "Loading model". It was not
  run again. The miss is `dp-multihop-dunlark-notice`, below. The follow-up main missed, the third turn of
  `dp-mt-encryption-versions`, named the Gidgegate exception at 16K.
- **Seven of the eight turns that searched again ended with the window full**, five of them after their first
  search, so there were 11 calls in all. None ran out of rounds or time, and none condensed. What came back was the
  first of ten to twenty results with its excerpt cut after the document's heading, about 500 characters, or a note
  that the result was taken out. After it, in the same tool message, the app said tools could no longer be used.
- **The cut result tells the model to call the tools the instruction after it rules out.** The result ends "Call
  get_document with a name above to read a document, or search again more narrowly" and "To read on, call read_more
  with [m1]". The instruction appended to the same message says "Tools can no longer be used, so do not write any
  tool calls." The 27B followed the instruction each time.
- **The cut results cost one answer.** `dp-multihop-dunlark-notice` needs a notice period from an agreement the
  reading did not have. At the full window the 27B searched twice, read the agreement with `get_document` and
  answered 70 hours. At 16K its search came back with the agreement cut after "## Term", and it answered that it
  could not confirm the notice period: honest, but not the answer.
- **One answer leaked a thinking tag.** In `dp-table-order-line` the 27B wrote the right answer, then `</think>`,
  then the same answer again, and the app showed all three. No answer in main or the 2026-11-06 runs has one.
- **A turn took 59.5 s at the median**, against 65.4 s at the full window, and the model 3,489 s against 4,230 s: the
  reading 2,392 s against 2,874 s, on fewer passages, and the chat rounds 636 s against 887 s, with 11 calls
  against 41.

## Condensing, replayed

`scripts/replay_condense_ab.py` sent what `research_notes.rs` sent at 616587e48 when a turn's results passed 60% of the
window: every passage was a source with its sentences numbered, one request asked with a JSON schema which sentences of
each to keep, the answer was read as the app read it, and a source left unsettled was asked about again in smaller
requests. It replaced the 2026-10-28 replay, which asked for free-text notes. A parity test
(`scripts/test_replay_condense_rag_eval.py`) checked its wording, request, schema, sampling and reading of the answer
against the Rust source. The turns, rubrics and scoring are those of 2026-10-27 and 2026-10-28.

**Since these runs (2026-11-19).** #598 (fe0cca7d0) has condensing answer in plain lines, one line for each result that
keeps something, with no schema, and #596 (59c195557) sends a cut-off answer again with twice its room only when that
room is larger. The replay now sends and reads as main does after both, and its parity test checks it against main's
snapshot. The numbers below come from the replay at 38311e48a, which asked under a JSON schema and sent a cut-off
answer again with the same room, as main at 616587e48 did (decision 2). No answer asked alone was cut off in these
runs, so none was sent again; under #596 the replay would have sent the same requests.

| Turns | Run | Facts kept | with citation | Notes used | After / before | Requests | Cut off | Median s/turn |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 60 at 16K | 2026-10-28, a message of its own | 304/304 | 304 | 58/60 | 0.18 | | | 14.2 |
| 60 at 16K | **2026-11-17, as shipped** | **304/304** | **304** | **60/60** | **0.11** | 68 | 0 | 24.1 |
| 60 at 16K | 2026-11-17, the question line alone | 304/304 | 304 | 60/60 | 0.11 | 82 | 0 | 25.2 |
| 12 packed | 2026-10-28, a message of its own | 264/288 | 261 | 12/12 | 0.03 | | | |
| 12 packed | **2026-11-17, as shipped** | **288/288** | **288** | **12/12** | **0.13** | 23 | 0 | 170.9 |
| 12 packed | 2026-11-17, the question line alone | 288/288 | 288 | 12/12 | 0.12 | 19 | 1 | 173.8 |

- **Nothing was lost.** The 27B's notes of 2026-10-28 left out two of four sources on 2 of 12 packed turns. Picking
  sentences by number, and asking again about a source the answer leaves unsettled, kept every fact under its own
  source's label. Read by hand on a 16K turn and a packed one, the notes are the planted sentences word for word, and
  every other source reads "Nothing here bears on the question."
- **It takes about twice as long.** 24.1 s a turn at 16K against 14.2 s. On the packed turns the first answer settled
  only 452 of 1,465 sources, so most of the work is the second request.
- **Asking with the whole message or the question line makes no difference to what is kept.** The app asks with the
  whole last user message (`question_of`), the replay's arm "as shipped". With the line alone the first answer settled
  fewer sources at 16K (995 against 1,191 of 1,341) and more on the packed turns (871 against 452 of 1,465), and both
  kept everything. One reply on a packed turn ran to the 16,384-token cap; its sources were asked about again and
  nothing was lost.
- **The replay understates what the app would send.** In a chat turn the last user message holds every passage the
  turn read, so the app's condensing request would carry them twice, once in the question and once as results. The
  replay's message has its passages taken out.

## Why condensing never fires in a chat turn

The loop condenses at the start of a round after the first, once that turn's tool results pass 60% of the window. The
full-window check comes after it (`tool_loop.rs`, `room.rs`). `condense_reach.py` runs the recorded tool rounds of
main's nine turns with a call through that arithmetic at five windows:

| Window (tokens) | Answered | Window full at round 1 | Window full at round 2 | Condensed |
|---|---:|---:|---:|---:|
| 16,384 | 1 | 8 | 0 | 0 |
| 32,768 | 6 | 0 | 3 | 0 |
| 65,536 | 9 | 0 | 0 | 0 |
| 131,072 | 9 | 0 | 0 | 0 |
| 196,608 | 9 | 0 | 0 | 0 |

- **From 64K up, the results never come near 60%.** The largest round any turn ran returned 42,914 characters. At
  64K, 60% of that turn's window is 84,673 characters.
- **Below 64K, the window fills first.** The 60% counts the tool results alone against the whole window, but the
  planned request and the tool schemas already take most of it: 43% to 93% of the window at 32K, and 87% to 186% at
  16K. A turn runs out of window before its results reach 60%.
- **The 16K row assumes more than the app sends.** In five of those turns the planned request does not fit the
  evidence pool a 16K window gets, and the app would cut its passages to fit. The window run below is the app at 16K.
  There the app cut the evidence to fit: 37 of the 67 turns held fewer passages than at the full window, and no
  turn held more than 40,495 characters of them, against up to 83,898.

## Decisions open

1. **Should condensing be reachable in a chat turn?** As built it cannot fire on these questions at any window, so the
   app carries a step no chat turn runs. Three ways on, each a change to `tool_loop/room.rs` and `tool_loop.rs`:
   - measure the 60% against the room the planned request leaves, not the whole window, so a turn whose tool results
     fill its room condenses before the window is full;
   - at small windows, keep part of the evidence pool for the loop's own results, since a turn that has read to the
     budget has no room for the search after reading (at 16K, seven of the eight turns that searched again got
     back one cut result, or none, and the instruction to stop);
   - or take condensing out of chat turns, and keep the replay as its only check.
2. **Three defects to fix first if it is made reachable.** None can show today.
   - The span it condenses starts where the planned request ends (`planned_len`, `tool_loop.rs:140`), and that is
     before the reading's notes and the instruction to search or write. Condensing would replace both with its own
     notes, and leave the planned message ending in the reading instruction.
   - The question it asks with (`question_of`, `research_notes.rs:377`) is the whole last user message, every
     passage included. Each condensing request would carry the turn's passages a second time.
   - A cut-off reply is asked again with "twice the output room" (`larger_if_room`, `ask.rs:788`), but `larger`
     doubles `output_tokens` (`request/steps.rs:69`), and every step built with `Says::new` leaves that at 0. The
     second request is the first one again. This is not condensing's alone: the reading, the verifier's judge and the
     memory steps are built the same way, and only the comparison table and the corpus planner set it. Since these
     runs #596 has `larger` double the step's room, the planner's default when the step sets none, and sends the
     answer again only when that room is larger.
3. **What the reading's answer costs.** On this server, the reading's longer output accounts for more than the run's
   added model time, and the median turn is about 29 s longer. Most of the reading's output is entries for passages that
   keep nothing, written with indentation, and no request with a schema is drafted. Three ways to cut it, each to be
   measured on the 27B and the 9B before it ships:
   - entries for only the passages that keep something, with every other passage counted as keeping nothing;
   - the same schema with indentation ruled out;
   - or accepting the cost as the price of an answer the app can check.

   Since these runs #598 has taken the first way, as plain lines with no schema; no write-up in `evals/` measures it on
   the 27B or the 9B yet.
4. **Which question a follow-up's reading judges against.** The reading gets the learner's own words ("Is there any
   way round it?"), as #316 chose. The planner had restated that question to name the standard, and a reading of the
   restatement would have excluded the other standard's passage by the contract's own rule. This is one miss in 67
   turns, so a change needs a run that shows it.
5. **What a cut result says when the window is full.** At 16K the result that filled the window still pointed to
   `get_document` and `read_more` (`retrieval/tool_format.rs`), and the instruction appended after it said tools could
   no longer be used. The 27B followed the instruction all seven times. The 9B, the model a 16K window is likelier to
   come with, has not been run there. When no call can follow, the result could leave its pointers out, with a test
   that the window-full message names no tool.

## Method

- **Binaries.** `shipped_path_eval`, built for debug from origin/main at 616587e48 in a worktree of its own. The window
  run's binary is 1c14f601c: 616587e48 with this branch's `report_ctx` change, so the recording proxy answers
  `GET /props` with a smaller `n_ctx` while the server keeps its own. Both binaries had their main thread's stack
  reserve set to 32 MiB with `editbin /STACK:33554432` before they ran. Built as they are, the first run overflowed its
  main thread's 1 MiB stack right after indexing, before its first model call; #585 gives the examples the app's
  reserve.
- **Recipe.** The 2026-11-06 27B recipe: the reranker (Qwen3 reranker 0.6B, Q8_0) on this PC, the fifty questions of
  `evals/retrieval/desktop-path-v1.json` (67 turns), thinking off. The window run is the same recipe with the app shown
  16,384 tokens (`server.report_ctx` in this branch's binary, `server.reported_ctx` on main since #584, and the app's
  context window setting to match).
- **The 27B to itself.** Each run held the heavy-run lock shared with the other agents' 27B work, and started only once
  the server's one slot was idle. CPU load on this PC was 46% at the start of the main run, and another worktree's eval
  binary was building beside it for its first 20 minutes, so its times are not clean.
- **The replay.** Scripts at 38311e48a, the eight recorded reading runs and the rubrics of 2026-10-27 and 2026-10-28.
  At 16K: `--window 16384`. Packed: `--pack 4 --result-size 8 --results-chars 130000 --window 65536`. Each
  configuration ran the whole message (`--question message`) and then the question line (`--question line`).
- **Reach.** `scripts/condense_reach.py` puts each recorded turn's tool rounds through the loop's arithmetic at other
  windows. It assumes the model makes the same calls when their results are cut, and answers where it answered. Its
  test reads every constant back from the Rust source and checks its allowance and budget against the cases the
  loop's own tests pin.
- **Reading by hand.** Every answer of both runs was read against its rubric. The mechanical scores judged none of
  them.
- **The mechanical column.** Both runs wrote their rows before #588, so their scores here come from #588 (ff9e29174):
  `shipped_path_eval --rescore` reads each saved answer with the app's `ReplyText::read`, in a conversation that named
  `[s1]` up to the turn's highest citation so far, and its scorer scores the result. No model was called. The scorer
  at 616587e48 gave the same rows 5/50 and 4/50.

## Records

In `results/chat/qwen3.8-27b-remote/`, each row naming the model the server reported:

- `2026-11-17-shipped-path-main.jsonl.gz`: the fifty questions on main, rescored and scored as above.
- `2026-11-17-shipped-path-window16k.jsonl.gz`: the same at 16K.
- `2026-11-17-condense-16k.jsonl.gz` and `2026-11-17-condense-packed.jsonl.gz`: the replay, 60 turns at 16K and 12
  packed, each turn as shipped (`SHIPPED`) and with the question line alone (`LINE`).

The recordings of every request and the run records stay out of the repository, because they name the private server.
