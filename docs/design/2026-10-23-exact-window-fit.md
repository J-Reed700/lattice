# Window fit for learning calls: measure exactly, shrink, never refuse

Design for the 2026-10-22 "music theory" lesson failure (35,569 exact tokens vs a 32,768 window, refused in reference selection). Repo read at `c694fc79`, read-only.

## 1. Root cause

Your reading is right, with two additions. `select_noted` (`lessons/content_verification/research.rs:129-245`) packs candidates by `selection_room`/`selection_cost` (:143-150) and pops until `model_call::fits` (:160-163); both go through `grounded_generation::plan` (`lattice-core/.../grounded_generation/mod.rs:274-330`), which counts with `llm.count_tokens` = `chars / chars_per_token`. That ratio is the scheduler's *global* EMA over every prompt it has seen (`llm/scheduler/estimator.rs:50-64`, `scheduled.rs:314`), so a JSON-escaped prompt full of ♯/♭, notation and tables is counted at the course's average density, not its own. The two gates count exactly only when the estimate is tight and refuse at prompt ≥ window: `ScheduledLlm::room_for_answer` (`scheduler/scheduled.rs:78-110`) and `LlamaCppLlm::measured_body` (`llama_cpp/request.rs:95-135`, refusal text `scheduler/rejection.rs:85-105`). Nothing counts the JSON schema (it is a grammar, not prompt text) and nothing in learning counts exactly before building — `count_tokens_exact` is used only for a log line (`evidence_checks/walk.rs:458`). Second addition: the refusal was fatal only because of the call path. `research_topics` (`research.rs:423-430`) already treats `PromptExceedsWindow | InvalidInput | Other` from `research()` as "topics stay uncovered", but the repair loop's claims path `repair_loop.rs:171 → research::expand (research.rs:386 …await?) → research (:640 select_noted…await?)` propagates it, so one auxiliary selection killed a 95-minute lesson. Both halves need fixing: the accounting (one shared mechanism) and the product rule that research adds evidence and never stops a lesson.

## 2. Design

### 2a. One measurement, in core, rendered by the code that sends

Add to `lattice-core/src/application/services/grounded_generation/mod.rs`:

```rust
/// What a request measures against the window: exact where the backend can
/// count, the planner's estimate otherwise. Never an accounting error.
pub struct Measured {
    pub prompt_tokens: usize,   // system + history + rendered user message + chat framing allowance
    pub exact: bool,            // llm.counts_tokens_exactly() and the count succeeded
    pub estimated: usize,       // the planner's count of the same text (instruction+task+history tokens)
    pub allowed: usize,         // allocation.input_budget = window − output_reserved − safety_margin
}
impl Measured { pub fn fits(&self) -> bool; pub fn over(&self) -> usize; }

pub async fn measure(llm: &dyn LLMPort, request: &GroundedRequest) -> Result<Measured, GroundedGenerationError>;
```

- `measure` calls `plan()` (as `evidence_room` does), selects evidence as `run` would, and builds the **same `Vec<CompletionInput>`** `run` sends. Refactor `run` (:462-497) so the input assembly is one `fn assemble(&GroundedRequest, carried) -> Vec<CompletionInput>` used by both; `prompt_text(&[CompletionInput])` joins role + content with `"\n"` (the scheduler's rendering, `scheduled.rs:221-252`) and is counted with **one** `llm.count_tokens_exact(&text)`; add the framing allowance.
- Framing allowance: move `TEMPLATE_TOKENS_PER_MESSAGE = 8` and `TEMPLATE_TOKENS_FOR_ANSWER = 16` from `llama_cpp/request.rs:176-177` to `ports/llm_port.rs` as pub consts and have the adapter use them, so measurement and gate share one definition. Schema, `reasoning_effort` and sampling are not prompt text and are not counted (the gates do not count them either).
- `Err` only for the planner's structural refusals (`BudgetError::ModelTooSmall`/`NoInputRoom`/fixed-part overflow). A failed `count_tokens_exact` falls back to the estimate with `exact: false` (debug log), never `Err`.
- Fit criterion is `prompt_tokens ≤ allocation.input_budget`. The gates refuse only at prompt ≥ window, so a request that passes this check has at least `output_reserved + safety_margin (5 %, ≥ 256)` of headroom over the refusal line; the small differences between the scheduler's and the adapter's renderings (role prefix, `\n` joins) cannot flip the gate. No new margin is introduced; the planner's existing one is reused.

### 2b. The learning seam: `model_call.rs`

```rust
pub(in learning) async fn fits(llm: &dyn LLMPort, request: &GroundedRequest) -> bool;   // now async: measure().fits(), estimate when the backend cannot count
pub(in learning) fn source_room(llm, &GroundedRequest, ceiling) -> usize;               // unchanged: the FIRST room, by estimate

/// The settled room and the request built at it, or room 0 when even build(0) does not fit.
pub(in learning) struct Fit { pub room: usize, pub request: GroundedRequest, pub measured: Measured }

/// Build at `room`, measure once, shrink proportionally, verify; repeat while over
/// and the room still shrinks. One count when it fits; two or three on dense material.
pub(in learning) async fn fit_room(
    llm: &dyn LLMPort,
    room: usize,                                  // from source_room (estimate)
    build: impl FnMut(usize) -> GroundedRequest,  // packs material up to `room` estimate-tokens; build(0) = skeleton
) -> Fit;

/// fit_room, then `call`; a `PromptExceedsWindow` from any gate or server shrinks and calls again.
/// Ok(None) when nothing fits — the site degrades, it never errors.
pub(in learning) async fn send_fitted<T, F>(
    llm: &dyn LLMPort,
    room: usize,
    build: impl FnMut(usize) -> GroundedRequest,
    call: impl FnMut(GroundedRequest) -> F,       // model_call::send or structured_call::complete_checked_request
) -> Result<Option<T>> where F: Future<Output = Result<T>>;
```

Shrink rule inside `fit_room` (all in the planner's estimate-token units, so the site's packer needs no change):

- `E0 = estimated(build(0))` — the skeleton's estimate (no round trip); `A = allowed`.
- If `A ≤ E0`: the skeleton alone does not fit → `room = 0`, return `build(0)`.
- Build at `R`, measure: `P` exact, `E` estimate. `M = E − E0` is the material actually packed (≤ R).
- If `P ≤ A`: done. Else attribute every token of overflow to the material (the skeleton is prose counted at the average; blaming the material over-shrinks, which is the safe side):
  `R' = floor(M × (A − E0) / (P − E0))`. Since `P > A`, `R' < M ≤ R`: strictly smaller. Rebuild at `R'`, re-measure (densest-first material can still be over); repeat while over and `R' < R`; stop at `R' = 0`. Terminates by strict decrease; typically fits on the second build.
- Backend cannot count exactly (`exact: false`): `fit_room` accepts the estimate-fit as today (nothing to shrink by) and `send_fitted`'s safety net does the work.

`send_fitted` on `AppError::PromptExceedsWindow { prompt_tokens, window, counted_exactly }`:
- `counted_exactly`: treat `prompt_tokens` as `P` for the same proportional step (no extra round trip — the refusal is the measurement, as `judge::kept_after_count` already does).
- not exact (the gate's plain 4.0 estimate, or a server's own refusal with no usable count): halve the room — the only information is "too big", and halving is the split rule the tree already uses (`evidence_checks/requests.rs:255-275`, `repair.rs:438-446`, `generation/evidence.rs:556-562`). ≤ log₂(room) retries; `None` at 0.
- Any other error propagates unchanged.

### 2c. Type the server's size refusal (non-exact backends)

`scheduler/rejection.rs:225-266 rejected()`: when `too_large && !size.counted_exactly`, return `AppError::PromptExceedsWindow { prompt_tokens: size.prompt_tokens, window, counted_exactly: false, message: <the existing sentence> }` instead of `ModelRejected`, so the safety net can catch it (chat already renders `PromptExceedsWindow`, `shared/ipc/conversions.rs:134`). Keep `ModelRejected` for `too_large && counted_exactly` — the server's real limit is below the configured window, a configuration fact. Follow-up, not in this fix: `llm/cloud.rs` maps no size refusal at all (grep: none); route its 4xx through `is_size_refusal` the same way.

### 2d. Fallback summary by backend

| Backend | Pre-send | If still refused |
|---|---|---|
| Bundled or remote llama-server (`/tokenize`) | exact `measure`, shrink before sending; refusal impossible by construction (same renderer + framing, plus `output_reserved + margin` of headroom) | `send_fitted` shrinks from the refusal's exact count |
| OpenAI-compatible without `/tokenize`, Ollama, cloud | estimate (calibrated ratio) as today | typed server refusal (2c) → halve and retry; `None` at 0 |

Round trips: one `/tokenize` per fitted call when it fits (ms on a local server), two or three on dense material. Optional later optimisation: carry `Measured.prompt_tokens` on `CompletionRequest` so the gates skip their own recount.

## 3. Can a size refusal still reach the learner?

After the fix, **no learning call may fail for size unless `build(0)` — the instructions and the skeleton with no material — does not fit.** That is a window-vs-instructions fact the learner cannot change by sending less, and it surfaces as the existing `model_call::window_too_small` configuration error (ideally at the job's preflight, which should also use `measure`). Everything else degrades at its site (table below): send less material, skip the optional step, or leave the unit in the state the product already has for it (unlinked item, uncovered topic, held claim). `TooLarge::Input` messages that tell the learner to "shorten" material the app packed are removed from fitted sites.

Concretely for the failing path: `select_noted` with `None` from `send_fitted` returns `Ok(Vec::new())` with an info log ("no search result fits beside the selection's instructions; the gap stays open"), exactly like an unavailable search. And `expand` (`research.rs:386`) maps `InvalidInput | PromptExceedsWindow | Other` from `research()` to `Expansion::Unchanged(searched).timed(...)` with a warn, as `research_topics` does — research adds evidence, it never stops the lesson. `repair_loop.rs:171` then proceeds; the claims stay held for the review screen (#155).

## 4. Call sites

Legend: **pack** = `fit_room`/`send_fitted` over the site's material; **fits** = switch to the async exact `fits`; **degrade** = what replaces today's error at room 0.

| Site | Change |
|---|---|
| `content_verification/research.rs:89 selection_room`, `:94 selection_cost`, `:614-617` search sizing | keep (estimate sizes the *search depth*, not a prompt) |
| `research.rs:143-166 select_noted` | **pack**: `build(room)` = candidates whose estimate cost ≤ room → `selection_request`; `send_fitted` with `complete_checked_request`; drop the `fits` pop loop and `window_too_small` (:167-171); degrade = `Ok(vec![])`, gap open |
| `research.rs:386 expand` | tolerate research errors like `research_topics` (:423-430); return `Unchanged` |
| `planning/outline_references.rs:329` (outline-phase `select_references`) | already covered by `select_noted`; `selected.is_empty() → continue` already degrades |
| `planning/outline_linking.rs:405 fits`, `:599-604` | **fits** (async); at group of 1: **pack** its passages (`fit_room` over listed passages) instead of `Err(InvalidInput)`; degrade = item left unlinked, warn |
| `outline_linking.rs:505 share` | keep as first room; the per-group call above settles it |
| `assessment/assessment_generation.rs:61 room` | **pack** the passages in `author`; degrade = skeleton alone over → `window_too_small` (legitimate: instructions don't fit) |
| `recall/study_generation.rs:241` (claims batch) | **pack** over the claim batch; degrade: a single claim + skeleton over → `window_too_small` (legitimate) |
| `recall/study_generation.rs:325 bounded_sources` | **pack**; keep the `sources.is_empty()` message only when `build(0)` is over (legitimate), else fewer sources |
| `lessons/review_evidence.rs:193 share` | **pack** the reference passages per issue; degrade = fewer passages, review runs on what fits |
| `lessons/reference_collection/snapshots.rs:18` | keep as first room (feeds retrieval depth); callers that build the prompt from it pack |
| `lessons/generation.rs:208 fits` | **fits** (async) |
| `lessons/structured_call.rs:193` (draft room) | keep estimate (a decision between two shapes; the answer step is fit-checked exactly) |
| `structured_call.rs:284, 347, 356, 441` | **fits** (async): answer step beside draft, re-ask vs restate, doubled reservation |
| `structured_call.rs:165 complete_checked` | add `complete_checked_request(llm, GroundedRequest, progress, reasoning, accept)`; `complete_checked` delegates |
| `lessons/generation/lesson.rs:187 evidence_room`, `:305 fits` | **fits** (async) in the add-until-fits loop; `room(0)` fallback stays; nothing fits → preflight's `window_too_small` (legitimate; see lesson-window preflight) |
| `lessons/generation/recall.rs:260` | **pack** the snapshot passages; degrade = fewer passages |
| `lessons/generation/evidence.rs:409 coverage_fits`, `:429 coverage_groups`, `:556-562` | **fits** (async) and keep halving; at group of 1: **pack** its passages; degrade = pair judged uncovered (research/stays uncovered), never `Err` |
| `lessons/generation/outline.rs:180` | **pack** the source catalog (fewer listed sections); degrade = draft without catalog (uncited), warn; goal + instructions over → `window_too_small` (legitimate) |
| `content_verification/repair.rs:193 room`, `:438 fits` | **fits** (async), keep halving; at one claim: **pack** its passages; degrade = claim left held, never `Err` |
| `content_verification/judge.rs:169 kept_after_count` | already safe (refusal-driven shrink with exact numbers); leave |
| `evidence_checks/requests.rs:255` | already safe (halves on `PromptExceedsWindow`); leave |
| `model_call.rs send/window_too_small/TooLarge` | `send` unchanged; `TooLarge::Input` no longer used by fitted sites |

## 5. Tests

Fake `LLMPort` for `model_call` tests (`model_call.rs` tests module, reuse the recorder): `count_tokens = chars/4`, `count_tokens_exact = chars/2` (dense), `counts_tokens_exactly = true`, `complete` refuses with `PromptExceedsWindow { counted_exactly: true }` when its own exact count ≥ window (the gate), else records the request. A second fake `Blind` with `counts_tokens_exactly = false` whose `complete` refuses with `counted_exactly: false` until the prompt is under a hidden limit (the server).

1. core `measure`: counted text equals the text `run` hands `complete` for the same request (same renderer); `allowed == input_budget`; count failure → estimate, `exact: false`, no `Err`.
2. `fit_room` dense: estimate says fit, exact says over → returns a room whose built request measures `≤ allowed`; rooms strictly decrease; ≤ 3 exact counts.
3. `fit_room` skeleton over: `room == 0`, request is `build(0)`, no count of material, no send.
4. `fits` async: a request the estimate accepts and the exact count rejects → `false`.
5. `send_fitted` on the dense fake: no refusal reaches the caller; one send.
6. `send_fitted` on `Blind`: catches `counted_exactly: false`, halves, succeeds; exhausts → `Ok(None)`.
7. `select_noted` with dense candidates: selects among fewer, `Ok`; none fit → `Ok(vec![])`, no `window_too_small`.
8. `expand`: a `PromptExceedsWindow`/`InvalidInput` from `research` → `Expansion::Unchanged`, lesson loop continues (extend `repair_loop` tests).
9. `rejection::rejected`: `too_large && !counted_exactly` → `PromptExceedsWindow`, message unchanged; `too_large && counted_exactly` → `ModelRejected`.
10. `structured_call_tests`: answer step that only the exact count rejects falls back to one step (:284) without a refusal.

Live: re-run the course eval (`scripts/course-eval.sh run evals/courses/recipes/course-eval-9b-local.json …`, README :88-101) on the music theory course at the 32,768 window; expect no `llm.request_refused` warn in the lesson log and the lesson to finish; expect the new `learning.fit` info lines (`room`, `shrunk_to`, `exact`, `counts`) during "Choosing relevant, authoritative references". Then a forcing run at a 16,384 window on the same course: shrinking must be visible and still no refusal.

## 6. Build split

Task 1 (foundation, first, small): `lattice-core/.../grounded_generation/mod.rs` + `tests.rs` (`Measured`, `measure`, `assemble`/`prompt_text` refactor), `lattice-core/.../ports/llm_port.rs` (framing consts), `llm/llama_cpp/request.rs` (use the consts), `llm/scheduler/rejection.rs` + tests (typed refusal), then `learning/model_call.rs` (`fits` async, `Fit`, `fit_room`, `send_fitted`, tests 1-6, 9) and `learning/lessons/structured_call.rs` + `structured_call_tests.rs` (`complete_checked_request`, async fits at :284/:347/:356/:441, test 10). Publishes the signatures above.

Task 2 (verification + lesson generation sites, after task 1's signatures): `lessons/content_verification/research.rs` (select_noted, expand), `repair.rs`, `repair_loop.rs` tests, `lessons/generation/{lesson,recall,evidence,outline}.rs`, `lessons/generation.rs`, `lessons/review_evidence.rs`; tests 7-8.

Task 3 (planning, assessment, recall sites, parallel with task 2): `planning/outline_linking.rs`, `planning/outline_references.rs` (verify only), `assessment/assessment_generation.rs`, `recall/study_generation.rs`, `lessons/reference_collection/snapshots.rs`, the lesson-window preflight (use `measure`); tests for each degrade path.

Tasks 2 and 3 share no files; both compile only once task 1 lands (give them the signatures and have them `cargo check` after task 1 merges). Full gates run once, by the coordinator, after all three.
