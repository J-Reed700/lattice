# Model capacity: how much one request holds, for each model (2026-10-28)

Status: design for sign-off. Nothing here is built.

## 1. Why

Three places decide how much one model request holds, and none of them asks the model:

| Where | Today | Problem |
|---|---|---|
| Chat reading | `PASSAGES_PER_GROUP = 4` (`chat/tool_loop/reading/groups.rs:38`) | measured on the 9B (`evals/retrieval/2026-10-26-GROUPED-READING.md`), applied to every model |
| Lesson claim checks | `page_budget` = answer tokens × prompt rate ÷ output rate (`content_verification/evidence_checks/page_budget.rs:48-56`) | sizes evidence by speed. On Haiku the prompt rate is time to first streamed delta (`llm/cloud/timing.rs:66-74`), so pages held 86 then 2,552 tokens: one passage per claim, six rounds, 8 min 18 s, 83 of 126 claims sent to web research |
| Research condensing | `CONDENSE_AT_WINDOW_SHARE = 0.6` (`chat/tool_loop/research_notes.rs:27`) | a share of the window, the same for every model |

What a model can use well is shorter than its window, differs by model and task, and falls fastest when the
other passages are on the same topic (RULER, NoLiMa, Chroma's Context Rot; our 27B noted 192/300 facts given
every passage at once and 298/300 in groups of four). Synthetic needles do not predict real tasks; RAG-shaped
tasks do (HELMET). Batch accuracy falls with batch size and depends on position (BatchPrompt).

## 2. Principles (decided)

1. **Accuracy decides a size.** Speed never enters sizing.
2. **Among equally accurate sizes, the fewest requests and rounds.** Cloud bills per token; what multiplied the
   Haiku lesson's cost was rounds, each re-asking unsettled claims and paying for their answers again.
3. **No model is held to another model's number.**
4. **No user waits for a measurement.**
5. **User work is never fanned out to a cloud model as parallel requests.**
6. **Evals run when a change touches what they measure.** Nothing runs on a schedule.

## 3. Concepts

- **Workload**: a kind of request whose size we measure. Three today: **Noting** (chat reading: passages per
  group), **Judging** (claim checks: claims per request and passages per claim), **Condensing** (research notes:
  tokens of results per pass).
- **Size**: how much one request of a workload holds. Each workload has its own size type.
- **Model identity**: exactly which model answers: the file's content hash for a downloaded GGUF, name and file
  size for a remote llama.cpp server, digest for Ollama, provider, model, thinking and effort for a cloud model.
  Thinking and effort are identity because they change the answers.
- **Capacity profile**: a model's size for a workload, its ceiling, and where they came from: **Measured** (our
  calibration, shipped), **Learned** (the model's own probes on the job), or **Starting** (nothing known yet).
- **Probe**: a hidden item with a known answer, placed in a real request and removed from its result.
- **Ladder**: the rule that moves a size on probe evidence.
- **Calibration**: our one-time measurement of a model, which produces a Measured profile.

## 4. Where a model's sizes come from

1. **Measured, shipped.** The app embeds a table of profiles for the models we calibrated: the 9B (both catalog
   files), the 27B and Haiku; Sonnet if its measurement cost is approved. A job starts at the measured size and
   never grows past it; a probe miss still shrinks it for that job.
2. **Learned, on the job.** Any other model starts at the **starting size** and climbs on its own evidence (§6).
   The size it reaches is remembered for that model identity, and its next job starts there.
3. **Starting.** 4 for counts (passages per group; claims per request, 2 passages per claim). For condensing,
   16k tokens: the smallest pass measured, which the 9B kept every fact at (`2026-10-27-CONDENSE.md`).

The window is still the hard bound: a size the window cannot hold is cut to what fits, as today.

## 5. Architecture

The design is one small domain, one application service, three narrow ports, and one adapter per workload in the
feature that owns it. Pipelines depend on one trait, `RequestSizer`, and never see where a size came from.

```
                         lattice-core                                   lattice-desktop
 ┌───────────────────────────────────────────────────────────┐   ┌────────────────────────────────────────┐
 │ domain/capacity          (pure, no I/O)                   │   │ features/capacity        (adapters)    │
 │   Workload, Size types, ModelIdentity, CapacityProfile,   │   │   ShippedTable : CapacityCatalog       │
 │   Ladder, Probe, ProbeVerdict, NonInferiority             │   │   SqliteCapacityMemory : CapacityMemory│
 │                                                           │   │   EmbeddedProbes : ProbeBank<W>        │
 │ application/ports/capacity.rs                             │   │   di.rs (registrar)                    │
 │   CapacityCatalog, CapacityMemory, ProbeBank<W>,          │   ├────────────────────────────────────────┤
 │   RequestSizer<W>, ProbeInjector<W>                       │   │ features/conversation  NotingInjector  │
 │                                                           │   │ features/learning      JudgingInjector │
 │ application/services/capacity/                            │   │ features/conversation  CondensingInj.  │
 │   CapacityService: resolve → LadderSizer / FixedSizer     │   │ features/llm           identity()      │
 └───────────────────────────────────────────────────────────┘   └────────────────────────────────────────┘
```

### 5.1 Domain: `lattice-core/src/domain/capacity/` (pure)

No I/O, no async, no imports from `application` or outer layers. Every rule a test can state lives here.

```rust
/// A kind of request whose size is measured. New workloads implement this; nothing else changes.
pub trait Workload: Send + Sync + 'static {
    type Size: Size;
    type ProbeBody: Clone + Send + Sync;   // what is placed in a request
    type Expected: Clone + Send + Sync;    // the known right answer
    const ID: WorkloadId;                  // "noting" | "judging" | "condensing"
}

/// How a workload's size moves. Growth and shrinkage are the workload's own business:
/// Judging grows claims per request first, then passages per claim.
pub trait Size: Copy + Eq + Ord + Serialize + DeserializeOwned + Send + Sync {
    fn starting() -> Self;
    fn grown(self) -> Option<Self>;        // the next size up, None at the top of its range
    fn shrunk(self) -> Option<Self>;       // the next size down, None at the floor
}

pub struct Noting;     impl Workload for Noting     { type Size = PassagesPerGroup; /* … */ }
pub struct Judging;    impl Workload for Judging    { type Size = JudgingSize;      /* … */ }
pub struct Condensing; impl Workload for Condensing { type Size = ResultTokens;     /* … */ }

pub struct JudgingSize { pub claims_per_request: u16, pub passages_per_claim: u16 }

/// Exactly which model answers. Built by the adapter that serves the model (§5.4).
pub enum ModelIdentity {
    Gguf { artifact: ArtifactIdentity },                       // content hash
    RemoteLlamaCpp { model: String, file_bytes: Option<u64> }, // weights cannot be hashed over the wire
    Ollama { digest: String },
    Cloud { provider: CloudProvider, model: String, thinking: ThinkingSetting, effort: Option<Effort> },
    Unidentified { provider: String, model: String },          // never remembered, never matched to the table
}

pub enum Provenance { Measured { run: String, items: u32 }, Learned, Starting }

pub struct CapacityProfile<W: Workload> {
    pub size: W::Size,
    pub ceiling: Option<W::Size>,   // smallest size that has missed (Learned) or the measured size (Measured)
    pub provenance: Provenance,
}

/// The ladder (§6) as a value: every transition is a pure function, so it is tested exhaustively
/// and the eval suite replays recorded outcomes through this same code (§8.3).
pub struct Ladder<W: Workload> { /* profile, probed requests at this size, settled flag */ }
impl<W: Workload> Ladder<W> {
    pub fn start(profile: CapacityProfile<W>) -> Self;
    pub fn size(&self) -> W::Size;
    pub fn probes_due(&self, items: usize) -> usize;        // 0, 1 or 2 for the next request
    pub fn observe(self, verdicts: &[ProbeVerdict]) -> Self; // grow on all-correct, shrink on any miss
    pub fn finish(self) -> CapacityProfile<W>;              // what the job learned, to remember
}

pub struct Probe<W: Workload> { pub id: ProbeId, pub body: W::ProbeBody, pub expected: W::Expected }
pub enum ProbeVerdict { Correct, Missed { reason: MissReason } }

/// Paired non-inferiority for calibration (§8.2): is accuracy(size) ≥ accuracy(reference) − δ?
pub struct NonInferiority { pub delta: f64 }
```

The domain also owns the capacity key's canonical string (`ModelIdentity::key()`), so the table, the store
and the eval records agree on one spelling.

### 5.2 Ports: `lattice-core/src/application/ports/capacity.rs`

Each port has one reason to change, and a pipeline sees only the one it needs.

```rust
/// What the pipeline sees: the size to use now, and where to report what the probes said.
pub trait RequestSizer<W: Workload>: Send {
    fn size(&self) -> W::Size;
    fn probes(&mut self, items: usize) -> Vec<Probe<W>>;   // empty when none are due
    fn observe(&mut self, verdicts: &[ProbeVerdict]);
}

/// The shipped measurements. Read-only; one adapter, built from the embedded table.
pub trait CapacityCatalog: Send + Sync {
    fn measured(&self, model: &ModelIdentity, workload: WorkloadId) -> Option<MeasuredProfile>;
}

/// What models learned on the job, per identity and workload.
#[async_trait]
pub trait CapacityMemory: Send + Sync {
    async fn recall(&self, model: &ModelIdentity, workload: WorkloadId) -> Result<Option<LearnedProfile>>;
    async fn remember(&self, model: &ModelIdentity, workload: WorkloadId, learned: &LearnedProfile) -> Result<()>;
    async fn forget(&self, model: &ModelIdentity) -> Result<()>;
}

/// Hidden items with known answers for one workload.
pub trait ProbeBank<W: Workload>: Send + Sync {
    fn draw(&self, count: usize, seed: u64) -> Vec<Probe<W>>;
}

/// How a pipeline places probes in its own request and reads them back out. One per workload,
/// implemented by the feature that owns the request shape (§7).
pub trait ProbeInjector<W: Workload> {
    type Request;
    type Reply;
    fn place(&self, request: &mut Self::Request, probes: &[Probe<W>], seed: u64) -> Placement;
    fn read(&self, reply: &Self::Reply, placement: &Placement) -> Vec<ProbeVerdict>;
    fn strip(&self, reply: Self::Reply, placement: &Placement) -> Self::Reply;
}
```

### 5.3 Service: `lattice-core/src/application/services/capacity/`

`CapacityService` composes the ports and hands out sizers. It holds no workload knowledge.

```rust
impl CapacityService {
    /// The sizer for one job: Measured if the catalog knows the model, else Learned if memory does, else Starting.
    pub async fn sizer<W: Workload>(&self, model: &ModelIdentity) -> LadderSizer<W>;
    /// Called when the job ends: remember what a Learned or Starting profile reached.
    pub async fn settle<W: Workload>(&self, model: &ModelIdentity, sizer: LadderSizer<W>) -> Result<()>;
}

pub struct LadderSizer<W> { ladder: Ladder<W>, probes: Arc<dyn ProbeBank<W>>, seed: u64 }  // the app's sizer
pub struct FixedSizer<W>  { size: W::Size }                                               // tests and calibration
```

`LadderSizer` and `FixedSizer` are interchangeable wherever a `RequestSizer` is taken: the pipelines' unit tests
and the calibration benches use `FixedSizer`, so the code under test is the code the app runs.

Budgets keep one owner. `BudgetAllocation` (`application/services/context_assembler/`) still decides what fits
the window; a size is a count limit applied beside it, never a second budget. Where a pipeline goes through
`GroundedRequest`, the size arrives as its existing `evidence_limit`.

### 5.4 Model identity

`LLMPort` gains one accessor, the same shape as the capability queries it already has (`pace`, `vision`,
`keeps_read_prompts`):

```rust
/// Exactly which model answers, for matching measured and learned capacity.
fn identity(&self) -> ModelIdentity { ModelIdentity::Unidentified { provider: …, model: … } }
```

Each adapter builds its own identity: `LlamaCppLlm` for the bundled sidecar from the model row's artifact
identity, and for a remote server from `/props` (model path and file size); `OllamaClient` from `/api/show`;
`CloudLlm` from provider, model, and the thinking and effort it sends. Wrappers (`ScheduledLlm`, `PinnedLlm`,
`PatientModel`) forward it, as they forward `keeps_read_prompts`. `llm_port.rs` is at 796 lines: its
token-estimation helpers move to `llm_port/tokens.rs` first, so the file stays under the 800-line ratchet.

A downloaded GGUF's content hash already exists for catalog models (verified at download); a model file the user
adds is hashed once when activated, as embedding models are (`docs/RUST_ARCHITECTURE.md`, Runtime ownership),
never on the launch path. Until it is hashed the model is `Unidentified`: it climbs from the starting size and
nothing is remembered.

### 5.5 Adapters: `lattice-desktop/src/features/capacity/`

A new feature owning storage only; it has no feature-to-feature edges.

- `ShippedTable`: `CapacityCatalog` from `evals/capacity/shipped/capacity-table.json`, embedded with
  `include_str!` and parsed once.
- `SqliteCapacityMemory`: `CapacityMemory` over a new table:
  `model_capacity(key TEXT, workload TEXT, size_json TEXT, ceiling_json TEXT, probes_seen INT,
  probes_missed INT, updated_at INT, PRIMARY KEY(key, workload))`, one dated migration.
- `EmbeddedProbes<W>`: `ProbeBank<W>` from `evals/capacity/probes-v1.jsonl`, embedded.
- `di.rs`: the registrar builds `CapacityService` and the composition root hands it to the features that size
  requests, through core types only.

## 6. The ladder

1. **Start** at the profile's size: measured, learned or starting.
2. **Probes per request**: one when the request holds fewer than 8 items, two at 8 or more. Random positions,
   never first, never adjacent, recorded in the turn log.
3. **Grow** after a request whose probes all came back right: the next size up, never past the ceiling, never
   past what the window holds. A Measured profile's ceiling is its measured size.
4. **Shrink** on any miss: the next size down for the rest of the job, and that size becomes the ceiling.
   A request the backend split for the window (`requests.rs:254-279`) or answered unusably counts as a miss.
5. **Settle**: once a size has held for 4 probed requests, probes drop to every fourth request, as a drift check.
   A miss re-opens it.
6. **Remember**: at the end of a job a Learned or Starting profile is saved for that identity; the next job
   starts at the size reached, not one above it.

Probes per workload:

| Workload | Probe | Right answer | Why this shape |
|---|---|---|---|
| Judging | a claim with its own 3 passages from one same-topic cluster: a paraphrase, or a swapped number, date or unit, a dropped negation, a scope carry-over, a wrong entity | the labelled verdict, with support from the right passage | self-contained inside a batch: each claim may use only its own `evidenceIds` (`claim_verification/batch.rs:108`) |
| Noting | a passage on an unrelated subject, plus a re-show of 1–2 passages the model marked with a hyphen, alone | a hyphen; and no fact appearing on re-show | a positive probe would mean inventing facts about the user's question |
| Condensing | an extra result holding two planted fact lines under a citation number bound to no source | both facts kept with their number; the lines are then deleted and the number never bound | nothing fictional can reach an answer |

Probes are hard on purpose: paraphrases and traps among same-topic passages, never verbatim copies. Their
content is synthetic and invented (§8.1), so nothing in them is true of the world.

## 7. The pipelines

Each pipeline takes a `RequestSizer` and its workload's `ProbeInjector`; neither knows about catalogs or memory.

- **Chat reading** (`reading/groups.rs`): `planned` groups by `sizer.size()` instead of `PASSAGES_PER_GROUP`.
  `NotingInjector` adds the negative probe to one group and, after the groups, the re-show.
- **Claim checks** (`evidence_checks/requests.rs`): `pack` caps a request at `claims_per_request` beside the
  window check it already makes (`batch_fits`); a claim's page is its top `passages_per_claim` ranked passages.
  `JudgingInjector` adds probe claims to the batch and removes their judgments before `finish`.
- **Condensing** (`research_notes.rs`): a pass condenses at most `size()` tokens of results, under the window
  share. `CondensingInjector` adds the planted result and scrubs its lines.

The claim-check redesign (evidence first, quotes, at most two rounds, removing `page_budget` and the
unmeasured-stop rule at `walk.rs:69-82`) is a separate decision, made from the experiments in §8.4.

## 8. Measurement

### 8.1 Datasets

All synthetic, generated deterministically and committed under `evals/capacity/`, each with a build-time check
that every planted fact occurs only where its label says.

- `synthetic-library-v4-clusters.json`: for each fact, 6–12 passages on the same subject that differ by one
  number, date, negation, scope clause or entity. Built by `scripts/build_synthetic_library_v4.py` from v3's
  machinery.
- `noting-v1.json` (turns of 16–48 passages, 4–6 facts each; the first 60 are the grouped-reading set),
  `claim-pairs-v2.jsonl` (v1's 101 hand-written pairs plus templated ones), `claim-sections-v1.json`
  (section-shaped batches with shared passages), `condensing-v1.json`, and `probes-v1.jsonl`, disjoint from
  the rest. A blind sample of 50 per set is labelled by a person before a set is frozen.

### 8.2 Calibration (ours, once per model)

- **Climb**: start at the starting size and double; stop at the first size that loses accuracy.
- **Decision**: paired non-inferiority against the best smaller size, margin δ = 5 points. A size passes when
  its loss is confidently under 5 points; when two sizes are that close, the smaller one ships.
- **Items**: about 150 per size per workload, stopping a size early once it has clearly failed. Positions are
  randomised and recorded.
- **Budget**: at most one hour per model on the 9B laptop; the harness enforces it and records which sizes it
  reached. Cloud runs take minutes and show their estimated cost for approval first.
- **Output**: a result file per run and a row in the shipped table. CI checks every row against its result file.

### 8.3 The harness

- **Rust drives**: `#[ignore]` tests under `crates/lattice-desktop/src/evals/` run each workload's real
  pipeline through a `Bench<W>`, implemented by the feature that owns the pipeline, at a `FixedSizer` size.
  Requests are built and answers parsed by the app's own code, so the suite cannot drift from the app.
- **Any backend**: a recipe names a `ModelRef` and settings; the harness builds the port the way the app does
  and refuses to run when the served identity does not match the recipe.
- **One record per scored item**, with the model identity, size, position, outcome and token cost.
- **Python scores and builds**: `scripts/capacity_eval.py` aggregates records, applies the statistics and writes
  the shipped table. It holds no prompt and parses no model output.
- **The ladder is validated with its own code**: a Rust test replays recorded calibration outcomes through
  `Ladder` over many orderings. It passes when the size the ladder reaches is within δ of the calibrated
  accuracy in at least 90% of orderings, and the first size where probes start missing is within one step of
  the first size where real accuracy drops (the probes are as hard as the work).
- The one-off replay scripts (`scripts/replay_*.py`) move to `scripts/oneoff/` and are not maintained.

### 8.4 One-time design experiments (claim checks)

Run once, on the 9B and on Haiku (Haiku with approval), before the claim-check redesign:

1. evidence first, claims after, vs claims first (today);
2. passage ids (today) vs exact quotes. Today's quote check proves nothing: `located_quote` resolves the id to
   the whole passage (`claim_verification.rs:530-542`);
3. Haiku's thinking as sent vs disabled;
4. one request per section and one packed re-ask, vs today's walk.

### 8.5 When evals run

When a change touches what they measure, on the models in the shipped table. Each prompt constant a workload
sends carries a version string that its records keep; CI fails when the shipped table was measured on a prompt
version that no longer exists, so a prompt change cannot merge without fresh results. CI itself makes no model
calls: dataset integrity, table consistency and the ladder replay.

## 9. Coverage across the app

In order of work; each row gets a bench or eval with a dataset and a decision rule, run when it changes.

1. Noting, condensing, judging and the ladder (this document).
2. Claim extraction and inventory; reference selection (owner vs impostor vs content farm); source standing
   judgements (`evals/provenance`); the chat answer verifier on every backend.
3. Recall card generation, labs and simulations, interview practice, conversation memory, routing and corpus
   planning.
4. Tangents, diagnostics. Retrieval, outline, lesson drafting, quizzes and grading keep their existing evals
   (`rag_eval.py`, `course_eval.py`).

## 10. Not doing

Fanning user work out to cloud models; speed in any sizing decision; background re-checks of a model while it
sits idle; scheduled or per-release runs; Opus calibration.

## 11. Open for sign-off

1. Condensing's starting size: 16k tokens.
2. Sonnet in the shipped table, once its cost is estimated.

## 12. Order of work

1. Domain, ports and service with their unit tests; `LLMPort::identity` and the adapters' identities.
2. Datasets, probe bank and the harness; calibrate the 9B, the 27B and Haiku; ship the table.
3. The three injectors and the pipelines' sizers.
4. The §8.4 experiments, then the claim-check redesign for its own sign-off.
5. The rest of §9.
