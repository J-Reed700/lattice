# Keep the system prompt out of the intent check; search first after a reading, measured and not shipped (2026-10-30)

Qwen3.5-9B Q4_K_M on this PC's llama-server (RTX 4070 Laptop GPU, 16,384-token window, the app's flags);
Qwen3.8-27B on the remote llama-server the app is set to (196,608-token window, one slot); Claude Haiku 5.5
through the Messages API, thinking off. Code: `chat/intent.rs`, `completion_input.rs`; the search-first wording was tried in
`chat/tool_loop/reading.rs`. These are findings 15 and 6 of the 2026-10-28 prompt audit.

## The intent classifier read the chat's system prompt as a turn

`conversation_context.rs` puts the conversation's system prompt into its context as a `System: …` entry, and
`intent.rs` printed every entry under "Recent conversation:". So the classifier that decides whether a turn
needs the user's documents or the web read about 2,900 characters of answering rules as something said in the
conversation. The median request was 3,808 characters; without that entry it is 956.

**Method.** `scripts/replay_intent_ab.py` on the 55 intent requests the 2026-10-22 shipped-path run logged
(`desktop-path-v1`), each sent as logged (AS_SENT) and with the `System:` entry taken out (NO_SYSTEM), five seeds
at the app's temperature 0.7. Every question is about the user's own documents, so the right answer is
needsKnowledgeBase true and needsWeb false; isFollowup is true on the later turns of the multi-turn items, except
the two that change the subject.

Right answers of 275 (55 requests × 5 seeds). The interval is a 95% bootstrap over requests, paired.

| Model | Flag | As sent | Without the system prompt | Difference |
|---|---|---|---|---|
| 9B | needsKnowledgeBase true | 179 | **213** | +34 [+4, +65] |
| 9B | needsWeb false | 222 | **246** | +24 [+9, +40] |
| 9B | isFollowup right | 250 | **264** | +14 [+1, +29] |
| 27B | needsKnowledgeBase true | 275 | 275 | 0 |
| 27B | needsWeb false | 275 | 275 | 0 |
| 27B | isFollowup right | 274 | 275 | +1 |
| Haiku | needsKnowledgeBase true | 270 | 269 | −1 [−6, +3] |
| Haiku | needsWeb false | 275 | 275 | 0 |
| Haiku | isFollowup right | 275 | 275 | 0 |

The 9B, which most needed the help, gains on all three flags; the 27B and Haiku were already right and stay
right. No answer failed to parse in either arm.

**Decision.** The classifier's history leaves out system entries (`completion_input::is_system_entry`).

**Still open.** The 9B still calls 62 of 275 document questions not a knowledge-base question. That flag only
forces a vault search the plan makes anyway (`routing.rs`), so it costs less than it reads; needsWeb true (29
of 275) asks the web as a fallback.

## The round after a reading said "answer directly", even when it could still search

After a reading, the next round asked: "Now answer my question directly, in the passages' own words…". That
round may still call the turn's tools, and the answer template tells the model to search before concluding that
evidence is missing; "directly" overrode it.

**Method.** `scripts/replay_write_ab.py` on the 14 saved reading prompts of the 2026-10-23 answer-coverage runs
(six short questions, eight long ones), every passage read in one group as shipped. Each question in two
conditions:

- **complete**: as saved. Its passages hold every rubric fact, so a search is a wasted round.
- **gap**: another document's question added ("… Also, <other question>"). No passage shown bears on it, so
  the turn should search for that part.

Arms on the same reading:

- **DIRECT**: no reading; the answer asked for on the passages and question alone.
- **W0**: the instruction as shipped.
- **W6F**: "If the passages leave any part of my question unanswered, do not answer yet: search for that part
  now. Otherwise, answer my question in the passages' own words…", the rest as W0.
- **W16**: W0 with "use every passage your notes quote that answers part of my question" (finding 16).
- **W6**, **W6_16**: a search sentence at the end instead of first, alone and with W16 (9B and Haiku only).

The first answer round may call tools. A call is recorded and ends the arm, since a replay cannot run it;
"on target" means its arguments name the part no passage covers.

Answer rounds that searched, of 14 (on target):

| Model | Condition | DIRECT | W0 | **W6F** | W16 |
|---|---|---|---|---|---|
| 9B | gap | 4 (2) | 0 | **2 (2)** | 0 |
| 9B | complete | 0 | 0 | **1** | 0 |
| 27B | gap | 13 (9) | 3 (3) | **14 (13)** | 5 (5) |
| 27B | complete | 9 | 1 | **8** | 0 |
| Haiku | gap | 14 (10) | 12 (12) | **14 (14)** | 13 (12) |
| Haiku | complete | 11 | 1 | **11** | 1 |

- With a part missing, W6F searched for it on every question on the 27B (3 with W0) and on Haiku (12 with
  W0). The 9B rarely searches whatever the wording (0 to 2 of 14).
- With nothing missing, W6F searches about as often as the same model does with no reading at all. By set:
  27B long 5 of 8 with both, short 3 of 6 against 4 of 6; Haiku 8 of 8 and 3 of 6 with both. On Haiku the
  questions are the same ones; on the 27B five of them are. Most are the long "everything the log records
  about …" questions, where a model cannot know the passages shown are all there is. W0's near-zero came from
  the instruction overriding that judgement.
- Where W6F answered, it stated the rubric's facts as W0 did (27B complete 21 of 21; 9B 54 of 55 and 46 of 48).
- W16 bought nothing. On the 27B with a part missing it cited 27 passages that hold no fact, against 3 with W0.
  W6 and W6_16 on the 9B searched no more than W0; on Haiku they searched as often as W6F.

**Not shipped.** The end-to-end run below lost what the replay gained.

## End to end on the 27B

`shipped_path_eval` on 10 `desktop-path-v1` questions (multi-hop, paraphrase, buried, OCR and two with no answer
in the library), today's wording against W6F, the same build otherwise. The app filled the server's window: about
168,000 tokens and 500 passages a question, 11 to 13 minutes an answer in either arm.

| | Passing the mechanical checks | Answers that searched after the reading | Median seconds |
|---|---|---|---|
| Today's wording | **9/10** | 0 | 684 |
| W6F | 7/10 | 5 | 695 |

- `dp-multihop-dunlark-notice`: the reading noted the 70-hour notice at passage 211, and today's answer gave it.
  W6F searched for the supplier in the turn's last round, answered from the search, and said no notice period was
  stated.
- `dp-abstain-error-code`: W6F's search came back empty, "the context window had no room for them", and its answer
  hedged about the search instead of saying plainly that nothing covers ERR-0001.
- No question gained.

A search after a reading of the whole window has nowhere to put its results, and comes in the turn's last round.
W6F is worth measuring again once the reading no longer fills the window.

## What this does not show

The replay ends an arm at its first tool call, so it shows whether a search was asked for, not what the search
found or how the answer changed; the end-to-end run showed that. The 9B ran with a 16,384-token window, as the
app gives it on this PC, so its readings were split into two to four groups where the 27B and Haiku read each in
one. The end-to-end comparison is 10 questions on one model. Haiku is a baseline check; Sonnet and Opus were not
run.

Records: `results/chat/<model>/2026-10-29-write-replay.jsonl.gz` and `2026-10-29-intent-replay.jsonl.gz`;
`results/chat/qwen3.8-27b-remote/2026-10-30-shipped-path-w0.jsonl.gz` and `-w6f.jsonl.gz` (answers and scores).
