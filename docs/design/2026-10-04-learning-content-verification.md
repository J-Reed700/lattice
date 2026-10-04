# Learning Studio content verification

Date: 2026-10-04. Status: first CSV verification slice implemented; broader research pipeline remains proposed.
Scope: course factuality, worked examples, answer keys, grading, and the relationship
to the existing chat verifier. Repository observations refer to the current working
tree, which contains uncommitted work. No live model evaluations were run for this
research pass.

## First implementation slice — 2026-10-04

The shared claim checker and passage ranker now serve chat and lesson generation.
Chat retains its prior policy. The strict lesson policy requires a semantic
judgment, an explicit reason, and an actual supporting quote; missing evidence,
invalid or truncated responses, low model scores, and timeouts cannot approve a
claim. Model scores remain uncalibrated and are only used to withhold approval.

Lesson preparation extracts claims from every teaching/assessment unit, audits
coverage in a separate call, retrieves passages from full immutable source
captures, executes complete Python/JavaScript fences in worked examples, and
runs independently authored CSV default/edge-case fixtures when DictReader is
used. Literal whitespace is retained in strict evidence. A single evidence-based
repair is followed by the existing teaching/key review and a full recheck.

Both direct and background publication require a backend-only report bound to
all saved blocks, rubrics, questions, answer keys, source IDs/content hashes,
checker identity, policy, and recorded runtime versions. The report and ready
transition commit together. Existing prepared lessons remain readable and are
not retroactively certified. Imported historical material is not certified by
this new generation gate. Reports contain private answer-key material and are
not returned in learner DTOs.

Automatic acquisition currently has one catalog entry: Python 3.14 CSV official
documentation for topic programs with no source history. It uses the existing
safe fetcher and source version store. It does not override selected or deleted
sources. Other topics require available reference material; unresolved claims
remain blocked. Broad topic/claim web research, general runtime coverage,
held-out accuracy measurement, model selection, learner-facing report UI, and
source-refresh recheck scheduling remain follow-up work. The author and checker
can still be the same configured model; separate calls are not model independence.

The deterministic tests exercise checker/persistence protocols with scripted
models and real restricted runtimes. They do not establish live-model accuracy,
real-world claim-extraction recall, or automatic repair quality on unseen lessons.

## Recommendation and limits

Reuse the chat verifier as the foundation of a shared evidence-checking service,
with a stricter publication policy for learning content. Combine it with per-claim
retrieval, executable checks, separate assessment validation, and human-labeled
evaluation. Another general critique prompt is insufficient.

We can enforce process guarantees: exact source provenance, checks tied to the
published revision, no silent approval after a failed check, and recorded execution
results for tested inputs on a named runtime. We cannot guarantee that every
natural-language assertion is true, every extracted claim was complete, or every
student learns the intended skill. Those require measured residual error and
separate learning-outcome evidence. A source can itself be wrong or obsolete.

Documents stay optional for the learner. Topic-only authoring should gather an
appropriate reference collection automatically when web access is available. If
reliable evidence cannot be acquired, keep the affected material in draft with a
specific explanation; do not represent missing evidence as completed verification.
An optional preview of a draft must not unlock scored assessments as checked work.

## What the current code actually does

| Component | Present behavior | Consequence for reuse |
| --- | --- | --- |
| `conversation/chat/verification/lexical.rs:50` | Returns no claims when sources are empty; filters out short sentences and questions. | Cannot establish topic-only course coverage; quiz premises and short definitions need explicit inclusion. |
| `lexical.rs:98` and `:129` | Can mark claims supported by token overlap; strong overlap bypasses the judge; uncited sentences never reach the judge. | Use overlap to rank evidence, never to approve teaching facts. |
| `verification/mod.rs:384` | Selectively judges sentences. After a failed call, ordinary sentences retain lexical verdicts; number/date/negation cases become unverified. | Strict learning policy must leave every failed or unreached required check unresolved. |
| `verification/mod.rs:238` | The checked-only grounded ratio is 1.0 when nothing was checked. | This logging metric must not become a publication threshold. Coverage must be separate. The UI already has distinct no-check/pending states. |
| `verification/judge.rs` | One claim per request, up to three cited sources and multiple windows, 4,000-character evidence budget, 90-second shared deadline, three concurrent calls, 128 output tokens. | Useful bounded checker infrastructure, but its chat budget and evidence selection need a course-specific policy. It does not search for new sources. |
| `judge.rs:287` | An invalid or missing model quote can fall back to a nearby source sentence while retaining the verdict. | A real quote is not proof of entailment. A strict verdict must point to validated supporting spans, not a replacement proximity quote. |
| `verification/background.rs:82` | Loads the utility model, otherwise the chat model; utility load failure uses lexical-only checks. Hydrates archived pages and patches the answer afterward. | Model independence is not assured. Keep background chat presentation separate from a course publication gate. |
| `learning/generation.rs:631` / `teaching.rs:269` | Author and reviewer receive the same LLM handle. Review checks structure, section coverage, citations, pedagogy, and blinded MCQ keys, with one repair. | Helpful consistency checks, but factual judgments remain model-based. A quote from the lesson proves it was inspected, not that the lesson is true. |
| `learning/source_library.rs` | Immutable source versions, hashes, refresh history, full captured text bounded to 64,000 characters; lesson preparation receives excerpts bounded to 2,400 characters. | Reuse provenance/versioning; retrieve relevant passages from captured text per objective and claim instead of relying on a fixed excerpt. Respect truncation metadata. |
| `learning/embedded_runtime.rs` / `practical_runs.rs` | Restricted JavaScript and versioned Python/WASI execution for practical activities, with limits and recorded results. | Reuse execution infrastructure for isolated authoring checks. The current lesson review does not execute its worked examples. |

The September 24 grounding-modernization design is useful background but labels
itself a proposal and describes an older implementation. The current code already
has individual claim calls, multiple windows, background verification and an
unverified state. No specialized checker is instantiated by the inspected verifier;
it receives a generic `LLMPort`.

## Research and its implications

These are findings from the cited work; the proposed Lattice design is an
engineering inference, not a claim that published benchmark scores transfer here.

- **Individual claims and external evidence:** [FActScore](https://aclanthology.org/2023.emnlp-main.741/)
  evaluates atomic facts against a knowledge source. [SAFE](https://arxiv.org/abs/2403.18802)
  adds search for individual facts in long responses. This supports separate
  extraction, retrieval, and checking stages, rather than one whole-lesson verdict.
- **Extraction and completeness also fail:** [VeriFact](https://arxiv.org/abs/2505.09701)
  addresses incomplete and missing extracted facts and evaluates recall alongside
  precision. A checker can appear excellent by overlooking the difficult facts.
  Preserve qualifiers, dependencies, scope and relations; audit extraction coverage.
- **Uncertainty is a legitimate result:** [VERIFY/FactBench](https://arxiv.org/abs/2410.22257)
  distinguishes supported, unsupported and undecidable content using retrieved
  evidence. Failure to find evidence is not a demonstrated contradiction.
- **Retrieval and checking need separate evaluation:** [RAGChecker](https://arxiv.org/abs/2408.08067)
  diagnoses retrieval and generation separately. The [2025 FACTS suite](https://deepmind.google/blog/facts-benchmark-suite-systematically-evaluating-the-factuality-of-large-language-models/)
  also separates grounding, search and internal-knowledge factuality. Matching a
  supplied source and being correct about the world are different tests.
- **A judge needs validation:** [FACTS Grounding](https://deepmind.google/blog/facts-grounding-a-new-benchmark-for-evaluating-the-factuality-of-large-language-models/)
  used different model families and checked judges against human ratings. A
  different model can mitigate some shared bias; agreement alone is not proof.
  [Research on intrinsic self-correction](https://arxiv.org/abs/2310.01798) found
  failures without external feedback in its studied reasoning tasks. This is not
  a universal claim about every newer model, but it argues against relying on
  repeated unaided review as our correctness standard.
- **Executable examples need substantial tests:** [EvalPlus](https://arxiv.org/abs/2305.01210)
  found that expanded tests expose previously missed incorrect code. Passing one
  generated happy-path assertion is weak evidence, particularly when the same
  author invents the implementation and expected result.
- **Specialized checking is worth benchmarking:** [MiniCheck](https://arxiv.org/abs/2404.10774)
  demonstrates efficient document-grounding models. IBM's
  [Granite Guardian 3.3 8B model card](https://huggingface.co/ibm-granite/granite-guardian-3.3-8b)
  provides a groundedness task and Apache-2.0 licensing. These are evaluation
  candidates, not a recommendation to install or switch models before testing.
  Guardian needs its own template and label mapping: its example uses `yes` to
  indicate a grounding problem. It is not a drop-in supported/unsupported judge.

## Proposed pipeline

1. **Acquire reference material before drafting.** Derive a reference plan from
   course outcomes and target versions. Prefer official specifications,
   documentation, original research and appropriate scholarly sources. Use the
   existing safe web service and selected library access; preserve the learner's
   chosen Space/document scope. Do not send private document text in public search
   queries. Store retrieved text, provenance, dates, versions and content hashes.
   Multiple copies of the same source are not independent corroboration.

2. **Write against an objective-to-evidence map.** Retrieve passages for each
   lesson objective. Retain sufficient surrounding context and exceptions. An
   excerpt missing the relevant section should trigger better retrieval, not a
   guess. Attribute interpretations and disputed accounts instead of converting
   them into universal facts. Expose references through a concise learner-facing
   source view, with exact passages available on demand.

3. **Extract and account for claims independently of the author.** Inspect every
   explanation, worked example, table, caption, quiz premise, option explanation,
   answer key and substantive feedback. Keep original offsets and a standalone
   claim preserving scope, quantifiers and conditions. Classify instructions,
   preferences and interpretations explicitly. Compare the claim inventory to the
   full content; section count alone does not establish factual coverage.

4. **Check claims using the appropriate evidence.** Source claims need relevant
   supporting passages and a semantic judgment. Search beyond the author's chosen
   citations for important unsupported claims and counterevidence. Contradictory
   sources require scoped attribution, resolution or an unresolved state. Math
   needs a suitable calculator/symbolic check where available. Product-capability
   promises must agree with actual supported features. Preserve support,
   contradiction, insufficient evidence and check failure as distinct outcomes.
   Treat token probabilities as model scores until calibrated on this task.

5. **Execute examples and validate assessments separately.** Represent runnable
   examples as code, fixtures, expected observations and runtime version, linked
   to the exact rendered code. Run in the existing restricted environment with
   time, memory and output limits. Test empty inputs, whitespace, missing/extra
   fields, types and exceptions. Use trusted reference fixtures, separately derived
   expectations and deliberately wrong solutions to test the tests. A few runs
   cannot prove a universal memory-complexity claim. Keep such reasoning subject
   to its own review. Solve MCQs without author keys, check uniqueness, and validate
   rubric alignment and partial-credit examples. Neither model agreement nor code
   execution alone settles an ambiguous question.

6. **Repair specific failures and check the resulting revision.** Send the author
   concrete evidence, failing inputs and actual outputs. Keep repairs bounded.
   Re-extract claims after edits; recheck changed material and dependencies, plus
   full lesson coverage and consistency. Retain valid previous work if a new
   candidate fails. A malformed judge response is an operational failure, not a
   factual verdict and not a reason to regenerate the entire course.

7. **Release only a checked revision.** Add a durable verification report bound
   to content hash, source versions, runtime version, checker identity and policy
   version. Promotion to ready must require that exact report in the same
   persistence operation. Required unresolved findings, missing coverage, failed
   examples or invalid answer keys block promotion. Avoid a single averaged score
   that hides a wrong answer key. Any material edit invalidates affected checks.
   Source refresh creates a recheck task for affected versions; do not silently
   rewrite lessons that already have learner attempts.

The learner sees useful progress such as gathering references, checking examples,
and preparing the next lesson. Prepare the next lesson ahead of time; cache checks
only for the same content, evidence and checker policy. A report should say what
was checked, with expandable evidence, rather than promising an infallible course.

## Concrete regression case

The retained Python lesson claimed that `DictReader` strips field-name whitespace.
The [Python CSV documentation](https://docs.python.org/3/library/csv.html#csv.Dialect.skipinitialspace)
describes the narrower optional behavior: ignoring spaces immediately after a
delimiter, disabled by default. A trusted fixture containing `" category "`
must preserve that field name under the lesson's default configuration.

Also retain the already observed empty-input versus newline-only distinction,
missing-field `None`, blank data lines, impossible string-plus-integer output,
ambiguous MCQ keys, and accumulation falsely described as constant-memory
streaming. Include correct neighboring claims to detect over-rejection. These are
regressions and development examples; they cannot also serve as the unseen test set.

## How we establish that it works

Begin with a small expert-labeled pilot, then expand across supported subjects,
course depths and model configurations. A proposed first substantial evaluation
is 300–500 claims and at least 20 complete lessons, including factual subjects,
reasoning, code and interpretive material. Those counts are a planning proposal,
not a statistically established sufficiency threshold.

Measure false approvals, false rejections, missed claims, evidence retrieval
coverage, unresolved fraction, answer-key defects, grading agreement, successful
lesson preparation, time and cost. Report uncertainty intervals and per-domain
results. A system that rejects every lesson is not useful. Set acceptance criteria
before tuning and keep a held-out set. Compare the existing review, strict shared
verifier, added retrieval, executable checks, and optional specialized checker to
show which changes actually help. Release criteria must include completed learner
journeys as well as claim-level performance.

Course quality additionally needs educator review and learner performance on new
problems and delayed assessments. The [IES learning guide](https://ies.ed.gov/ncee/wwc/PracticeGuide/1)
supports spacing and interleaving worked examples with problem-solving. These
principles inform the teaching loop, but their presence in a prompt does not
demonstrate learning gains or establish equivalence to a professionally authored
course.

## Implementation order

1. Turn the observed errors and valid counterparts into deterministic fixtures;
   specify the strict evidence report and publication contract. Extract the
   reusable checker from chat with a policy that preserves existing chat behavior.
2. Connect course source versions, objective/claim retrieval and topic reference
   gathering. Add independent claim extraction and coverage evaluation.
3. Connect authoring examples and assessment checks to restricted execution and
   evidence-based repair. Persist reports and show useful preparation status.
4. Run the held-out comparison, tune thresholds and select checker models from
   measured results. Complete end-to-end courses before claiming release quality.

The first vertical slice should be the retained CSV lesson: the new pipeline must
reject its known factual errors, accept corrected versions, and finish preparation
without a human editing the result or relaxing checks until it passes.


## Reference collection MVP follow-up

The later MVP replaces the production CSV catalog/probe branches with a
subject-neutral saved-source collection. It retains full bounded captures,
reuses the saved-source vector table with hybrid retrieval, retrieves evidence
independently for lesson claims, and exposes a learner-safe evidence report.
Reference web reads preserve headings, lists, code whitespace, and table rows.
Rust and other unsupported runtimes are disclosed as unexecuted, not rejected
solely because the language is unsupported. The CSV case remains test-only.
See [the MVP test instructions and limits](../development/learning-studio-verification.md#reference-collection-mvp--2026-10-04).
Manual source selection, a shared author/checker model, exact vector scans, and
text-only extraction are deliberate MVP limits. Automatic discovery/crawling,
independent evaluated judges, educator-reviewed outcomes, and automatic
re-verification of already-ready lessons are not implemented by this milestone.
