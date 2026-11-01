# Searching after a reading, citing what a search finds, and all fifty questions (2026-10-31)

The remote Qwen3.8-27B through `shipped_path_eval`, which is the app's own chat path, with the reranker on this PC's
RTX, on mains power. Code: `chat/tool_loop/reading.rs`, `chat/tool_loop/calls.rs`, `chat/retrieval/tool_format.rs`,
`chat/prior_evidence.rs`, `scripts/shipped_path_eval.py`.

## Why

[`2026-10-30-SHORTLIST-AND-PLACEMENT.md`](2026-10-30-SHORTLIST-AND-PLACEMENT.md) left one wrong answer, Dunlark's
notice period, and had run 17 of the 50 questions. The model named the supplier and then answered from what it held,
when one search would have found the rest. Reading 40 passages leaves room for that search. Every run below is the
same recipe on successive builds.

## The scorer was wrong first

Reading these answers by hand showed the checks failing right refusals. "Could not find" and "no record" were not
refusals to `ABSTENTION_RE`, and a question whose rubric expects a refusal was failed for saying so in other words.
`scripts/shipped_path_eval.py` now counts a sentence that says something is unavailable and shares a word with the
question, and judges a refusal against what the rubric expects: right where it expects one, wrong where it does not.

**This was changed after reading the answers it now passes.** It is the reason all fifty questions were run: the 17
had already been seen. Rescored, the earlier runs read as follows, with the old scorer's figures beside them.

| Run | Questions | Passing, corrected | Passing, old scorer | Median seconds | Turns with a tool call |
|---|---:|---:|---:|---:|---:|
| Whole window | 15 | 14 | 10 | 680 | 2 |
| Shortlist, reranked | 17 | 16 | 13 | 37.5 | 3 |
| Told to search before answering | 17 | 16 | 14 | 35.0 | 9 |
| And a search's results are citable | 17 | **17** | 10 | 38.1 | 6 |

`must_not_claim` is still a plain substring check, so naming another entity's value under its right owner fails.
That was left alone, and such cases are counted below as what they are.

## Why Dunlark failed twice

After the reading, the round that wrote the answer was told to answer, so it did. Told instead to search whatever the
passages left open, it searched and found the 70 hours, and the answer still failed: it could not cite them. A search
tool's results were numbered from 1 in their own list, which collided with the passages the turn already held, and
they had no number in the turn at all. Results now show under the turn's own numbers, and the answer reads "70 hours
of written notice ... [41]", citing `contract-04`.

## Three things reading the answers found

**A reading of nothing dropped the search.** When every passage came back a hyphen, the turn took the reading out of
its message and with it the sentence telling the model to search, so a follow-up refused. Now that case says none of
the passages bears on the question and asks for a search. The refusals got better too: they now search before
refusing rather than refuse from the 40. On 9 questions alone: the ring and Merridew follow-ups pass and all five
refusals still refuse.

**Recall carried passages nobody used.** A follow-up recalled every passage the last turn read, up to 93 labels, and
any of them could be quoted as a note. Only what an answer cited is recalled now, at most 47.

**A question naming a document still read the others.** The reading took Brackleton's clause 24.112 for Tessivar's.
Replayed on all 50 first readings, about 20 s each:

| Sentence added to the reading | Unchanged | Expected document lost | Side effect |
|---|---:|---:|---|
| "...or when it comes from a different document than the one I name" | 37 | 0 | `dp-abstain-error-code` 0 to 35 notes, `dp-conflict-ring-threshold` 2 to 11 |
| "When my question names a document, a passage from any other document gets a hyphen." | 38 | 0 | none; handbook 3 to 1, encryption 7 to 3, OCR 22 to 1 |

The second shipped. The same sentence inside the hyphen clause did the opposite of what it says.

## Three more wordings, and one more citation

**Keeping exceptions.** The first 50-question run lost the Gidgegate exception on `dp-mt-encryption-versions`, the
one wrong fact it had. The reading now asks for the sentences that bear on the question "including any exception or
condition".

**Not narrating.** The first run opened 12 of 67 answers by describing its notes. The instruction that closes the
notes now says not to describe them or say whether they are enough.

**What was left out.** After the answer, a passage the notes quote and the answer does not use is asked for again.
That step exists for lists, where the writer drops facts. Sixteen captured continuations replayed:

| Closing wording | Replied with a hyphen | Largest appended |
|---|---:|---:|
| "Continue your answer with what each of them says" | 0 | 3,092 characters |
| "Add only what my question still needs", or a single hyphen | 12 | |

Everything dropped was noise: Merridew's 40 rows, the Spanish travel policy, the other handbook editions. A
continuation that adds no passage is now dropped whatever it says.

Keeping "in the same form as above" left note lines in an answer, because the form above was the notes. "Continuing
your answer in its own form" gave a hyphen on 23 of 26 captured continuations, and on 6 of 7 in the last run below.

**A document read that could not be cited.** `get_document` took an `include_metadata` flag, and the model sent it
false on two reads. The citation is built from that metadata, so those reads had no number: one answer printed a raw
document id and another left its key fact uncited. The flag is gone. All 5 reads in the last run are citable.

## What the planner would add

Only a utility model turns the planner on, and none is set by default. With the 27B in that role as well, on the 20
questions it could change, 18 passed against 16 without, and it brought a new failure on
`dp-mt-shift-and-return`. The cost is a prompt of 72,490 tokens, mostly the 405-document catalog, at 81.7 s a call.
It evicts the one slot's cache, so a turn's median went from 32 s to 116 s. The search fix above reached the same
two follow-ups without it. **It stays off until the catalog shrinks.**

## All fifty questions

Four whole runs, 67 turns each, all under the one scorer above. "By reading" is every answer read by hand against its
rubric, counting the questions a reader would mark wrong or incomplete.

| Build | Passing | Wrong by reading | Median seconds | p95 seconds | Turns with a tool call | Turns fully supported |
|---|---:|---:|---:|---:|---:|---:|
| A search's results are citable | 48 | 1 | 36.4 | 67.3 | 11 | 27 |
| Recall of what was cited, keeping exceptions, not narrating | 44 | 3 | 32.0 | 52.6 | 8 | 30 |
| A reading of nothing, the named document, what was left out | 50 | 1 | 31.3 | 51.8 | 10 | 32 |
| **Every read citable, the answer's own form** | **49** | **2** | **31.9** | **57.7** | **19** | **31** |

The last build took a median 2.0 s to retrieve, 27.5 s to generate and 8.9 s to verify. It read 40 passages at the
median and 47 at most, with no cut-off replies, no empty answers and no invalid citations.

**The difference between 50 and 49 is not a difference between builds.** Each run indexes the library afresh, and
every passage in a prompt carries its document's random id, so no two runs send the same prompt. Between any two of
these runs only 1 to 4 of the 50 first answers are identical, even where nothing a first turn reads has changed. The
rise from 10 to 19 turns with a tool call is the same noise. A single question either way proves nothing.

**The last run passes 49 and gets 48 right, and the score misses one of the two.** Both are follow-ups:

- `dp-mt-encryption-versions` turn 3, "Is there any way round it?", fails. The search for those words led with the
  handbook and the data-export policy. The reading quoted "There is no other way round it" from five standards and
  dropped the sentence before it in the one that matters, so the answer never names the Gidgegate exception.
- `dp-mt-runbook-ticket` turn 2 **passes and is wrong.** Its escalation line is not among the 40, because "if the
  problem hasn't cleared after the wait" says nothing about ERR-4070. The answer quotes "page Anneke Voskuil and quote
  TCK-1836", taking another runbook's sentence, swapping the ticket in and citing a passage that does not say it. The
  rubric's page goes to Marisol Quenby. The check asks only for the ticket. The claim check called both sentences
  `unverified` at 60% and 65%, below the confidence it marks, so a reader sees nothing.

The cause is one thing. With no planner, a follow-up searches its own words, and these have no subject. Both were
right in earlier runs where the writer happened to search again, so a run's score depends on whether it does.
[`2026-10-31-FOLLOWUP-SEARCH.md`](2026-10-31-FOLLOWUP-SEARCH.md) fixes that and adds the check that catches the quote.

Short of wrong, reading found these. About 7 of 67 answers still open or close by talking about the passages or the
search; the script counts 2. `dp-buried-agreement-meeting` and `dp-fragment-stopcock` carry chains of 27 and 10 labels
for one repeated sentence, some from other documents. `dp-abstain-named-clause` refuses and then says the undertaking
"most likely mirrors" the others. `dp-mt-combine-renewals` is a day out on a gap it calls rough.

## What this does not show

- **This is not a held-out score.** The scorer was corrected after reading these answers, and the wordings were tuned
  on these 50 questions over four runs.
- **The checks are substring checks.** They passed an answer naming the wrong person, so 49 is an upper bound.
- **The 27B only.** Neither the 9B nor Haiku and Sonnet have run the search instruction or the new wordings end to end.
- **`StandaloneServers` only.** Nothing here went through the real `PoolingServers` placement.
- **One run each.** Reading the same turn across runs is what showed the follow-up failures are a matter of chance.

Records: `results/chat/qwen3.8-27b-remote/2026-10-31-shipped-path-{cite,full50,confirm50,final50,last50}.jsonl.gz`
(answers and scores, in table order after `cite`), and `2026-10-30-shipped-path-reranked.jsonl.gz` rescored.
