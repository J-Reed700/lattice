# Claims the course's own best sources already state

Date: 2026-10-21. Status: design, decisions taken (§15); nothing built. Code read on `fix/claim-official-sources` at `b4e447ae` (main). `path:line` is under `crates/lattice-desktop/src/features/learning/lessons/content_verification/` unless it starts with `crates/`, `lessons/` (= `crates/lattice-desktop/src/features/learning/lessons/`), `references/` (= `crates/lattice-desktop/src/features/learning/references/`) or `core/` (= `crates/lattice-core/src/`). Owner data was read from a `.backup` copy of the app database taken 2026-10-21 (never the live file); numbers from it are marked "copy".

This is one design for one builder. It covers seven fixes the owner approved, in two groups:

- Standing (§4–§8): a source's standing is judged from its content as well as its metadata, a source the course teaches from whose publisher cannot be identified is levelled by the kind of work it is and the sources it stands on rather than written off as "unknown", the bar a claim must meet is relative to the best source the course has on its subject, the owner of a subject is primary for its own account whatever the page's genre, and before a claim waits for the user the checker reads the course's highest-standing sources on that subject (the *standing reach*). Nothing here keys on vendors, domains, software or "official docs": the same rules level a university press chapter, a statute, a peer-reviewed paper, a standards text, a cookbook, a museum page or a user's uploaded textbook.
- The repair loop (§9–§12): a repair changes only the sentences that failed; a claim id never moves to another sentence; the tally says what the user did and what the machine did; a pass never reopens a passed claim, and the loop has a stated bound.

Standing rules of the owner this design obeys: any model behind the bundled llama.cpp sidecar, any OpenAI-compatible server, Ollama or a cloud model; never designed around one machine; no probing a server; no arbitrary limits or wall-clock timeouts, every size is derived from the window, the measured pace, the page budget or a rule the code already has; pre-release, no compatibility paths, schema changes only as a new dated migration; the upgrade path on his data is stated with numbers (§13); a lesson is never failed by any of this; the repo's layer, architecture and SQL-contract checks pass (`scripts/check-rust-layer-boundaries.sh`, `scripts/rust-architecture-check/baseline.txt` with its 800-line file rule, `scripts/check-sql-contracts.py`); repositories own SQL; no global statics.

## 1. The problem, measured

### 1.1 What the owner sees

Lesson `db8397ff` ("Function Calling & Tool Schemas", program `026020fa`) has 33 claims waiting for the user in the copy: 29 `insufficient_standing`, 2 `general_knowledge_proposed`, 1 `contradicted`, 1 `unsupported` (31 when the owner counted: 17 / 12 / 1 / 1). The course has 410 saved web sources, every one added by the app's own research (`learning_source_library.added_by = 'research'`, no user-added source at all). Among them are pages that state the waiting claims verbatim. The owner: these should be "SUPER easy things to source".

The lesson has been through 33 verification passes since 10-19. 119 claims were ever extracted; 62 ended `Edited { successor: None }`, 12 `Removed`, 12 verified, 31 queued, 2 open. Of the 74 edited or removed, 53 were rewritten while their last check said `supported`, 9 were general-knowledge proposals, 11 were removed while `insufficient_standing`, 1 was unsupported. 944 checks were made, a median of 7 per claim, at most 31. The review header says "74 resolved by you" after zero user actions.

### 1.2 Why: four findings

**A. Standing is judged from metadata alone, and the course's own book is "unknown".** The source most of the stuck claims rest on (15 of 33 in the copy) is version `5e2254bb`, "Tool Use & Function Calling — The LLM Stack": an online book chapter at `prakashkagitha.github.io`, 10,430 words, research-added, cited by the course throughout. Its standing row (`source_standing_assessments`, policy `source-standing-v1`) says `standing = unknown`, nature `reference`, editorial `none`, with the reasons "the publisher field is null", "the author field is null, and the site name 'The LLM Stack' does not identify a specific individual", "no editorial process signals", "personal GitHub Pages domain". The judge saw only `source_provenance.signals_json` (`PageSignals`, `core/domain/provenance/signals.rs:33-80`: origin, title, site name, publisher and author meta, declared types, dates, generator, licence, HTTP facts, self-description). It never saw the author's name sitting in the domain, the book's chaptered shape, its citations to arXiv papers and vendor documentation, or that the course chose it to teach from. In the copy 60 of 84 standing assessments are `unknown`, 23 `institutional`, 1 `identified`: the app picks a source to teach from, then refuses to let it support a single claim.

The level procedure then makes an unknown publisher `TrustLevel::Unknown` (`core/domain/provenance/level.rs:319-323`), which no bar accepts (`core/domain/provenance/sufficiency.rs:147-181`).

**B. The checker never reads the course's higher-standing sources for a claim.** Pass A reads what the section cites (mostly the book); pass B's first page is one or two passages a page (budget 3,806 tokens at the owner's pace), by hybrid relevance over the whole course (`evidence_checks/walk.rs:58`). The page-recall eval (`/Users/josh/Code/lattice-claims-eval/evals/grounding/claim-corroboration-db8397ff/README.md`, "Results (2026-10-21)") puts the first passage of an owner's page that states the claim on page 2, 11, 17, 27, 37, 78, and never for 2 of 8 claims; third-party explainers and the book paraphrasing the same text win the early pages. A supported first check ends the claim (`evidence_checks/outcome.rs:127-134`); deeper pages are read only while unsupported (`evidence_checks/run.rs:166`); a research-led claim reads only the research round's versions (`evidence_selection.rs:421-456`); only 1 of 8 claims had such a page inside its research scope. The judge is not the problem: it accepts 6 of 9 owner passages that state the claim (the 3 misses are hand-labelled close calls), rejects 7 of 7 controls, and the source label line changes nothing.

**C. Standing is counted from one passage of one check.** The gate weighs only the passages holding the deciding check's quotation (`provenance_gate.rs:228-241`). When pass A (the book) and pass B (another source) both support, B's support is dropped (`outcome.rs:127-134`), so two independent origins never corroborate. The level rules make the subject owner's own page `Established` unless its nature is documentation, reference, a primary document or a report (`level.rs:38-43, :199-236`): a vendor's guide (`tutorial`) or a page whose nature the judge could not tell (`unknown`) rates below the bar for a specification claim. `claude.com`'s pages in the copy are judged `unknown / unknown / unknown / unknown`; `openai.com`'s 14 versions have no standing judgement at all, because no check ever read them.

**D. The repair loop rewrites sentences that passed.** `Decision::Repair` targets every Open claim whose kind `is_repairable()` (`core/domain/learning/claim_review.rs:178-183`: Unsupported, Contradicted *and* InsufficientStanding). The repair asks for edits (`repair.rs:515-536`, `lessons/text_edits.rs:54-123`); when the edit response is ambiguous it regenerates the whole section (`repair.rs:317-340`, the "requesting one validated section" line) and `merge` replaces it (`repair.rs:281-307`); then the teaching review runs over the whole lesson and may rewrite again (`repair.rs:363`, `lessons/teaching.rs:637-668`). Re-extraction matches claims by statement, quote or span overlap (`core/domain/learning/claim_review/rules.rs` `identities`); a reworded verified sentence matches nothing, its claim becomes `Edited { successor: None }` and the new sentence is a new claim with a fresh budget, checked from scratch. One further bug: the loop that hands an orphaned repaired claim's id to "an unowned extracted claim in the same field" (`claim_ledger/observe.rs:397-424`) gave the id of a quotation claim ("Source 0 states: …") to a different sentence ("The `required` array…"), so that claim's history shows reasons about another sentence. And `claim_tally` counts machine rewrites as "resolved by you" (`claim_ledger.rs:560`, `core/domain/learning/claim_review/rules.rs:325-328`).

### 1.3 What the checkpoints hold today

| key | written at | keyed by | owner's two jobs (983e5b0f, d4fdd913) |
|---|---|---|---|
| `claim-v1:` | `mod.rs:306-319` | `POLICY` (`lesson-evidence-v21`, `mod.rs:108`), model, window, the comparison's `ClaimChecks::key` (unit, statement, passages), `TRUST_POLICY_VERSION` (`source-trust-v1`) | 1,478 + 291 receipts |
| `claim-evidence-v2:` | `evidence_selection.rs:87-98` | `POLICY`, model, window, unit, statement, retrieval mode, embedding model, `RANKING` (`two-pass-hybrid-v1`) | 265 + 142 pages |
| `claim-walk-v1:` | `walk.rs:528-547` | the first page's receipt key | one per walked claim |
| `claim-research-scope-v1:`, `claim-batch-split-v1:`, `claim-ledger-v1:pending`, `inventory-section-v1:`, `lesson-numbered-sources-v1`, `teaching-sources-v1:`, `prepared-v1` | various | — | unchanged by this design |

Trust store rows: `source_standing_assessments` PK `(signals_sha256, policy, model)`; `source_subject_authority` PK `(program_id, origin_key, subject_key, policy, model)`; `claim_profiles` PK `(input_sha256, policy, model)` (`crates/lattice-desktop/migrations/20261018120000_source_provenance_and_trust.sql`). In the copy: 410 provenance rows, 84 standing rows (70 joinable to this program's versions), 194 authority rows. The authority rows already carry what the standing reach needs: `owner_origin_hints` per (origin, subject), e.g. `web:openai.com × "function calling" → owner, hints [platform.openai.com, cookbook.openai.com]`; `web:claude.com × "llm tool calling capabilities" → owner`; `web:json-schema.org × "json schema required array" → owner`; `web:owasp.org × "least privilege principle" → owner`; blogs judged `independent` with hints naming the owners.

Today's Retry rebuilt pass-B pages for many claims and re-ran 19 checks with the same verdicts: the embedding model changed from Candle to the llama.cpp GGUF, and `embedding` is in the `claim-evidence-v2` key. That key is deliberate (a new embedding is a new ranking; §5.6 of the course evidence index protects pages only against ranking drift among unchanged sources). It is a one-time cost already paid; this design does not change the key (changing it would re-key the 407 pages once more).

## 2. Decisions at a glance

| # | Decision |
|---|---|
| 1 | A new standing judgement, `source-standing-v2`, judged from the page's metadata **and** model-free evidence extracted from its content and links (bylines, copyright lines, a name in the domain, about/author links, citation profile, the shape of the work) and from how the course got it (`added_by`, the planner's selection note). The v1 rows are not deleted; they stop being read. Re-judged lazily, on the next weighing. |
| 2 | Levels (`level.rs`): a source the user added is at least `Established` ("Added by you"); a research-chosen source whose publisher stays unidentified is levelled by the kind of work and its content evidence on a graded ladder (a cited, structured reference work `Established`; an uncited reference work or a cited article `Community`; an uncited article, discussion, aggregation or marketing `Unknown`; content-farm or generated patterns excluded), §5.3; the subject's owner is `Primary` for its own account whatever the genre, except marketing, opinion, discussion, aggregation or community-edited content. |
| 3 | The bar is relative to what the course has: after the static bar and corroboration fail, a claim is sufficient when its support is the strongest the course has on that subject (the *ceiling*) and nothing at or above it contradicts it; at `Community` or below it needs two independent origin clusters. Labelled `BestAvailable`. Static bars and calibration unchanged. |
| 4 | The *standing reach*: before a claim is left unsupported, insufficient or contradicted, its walk continues over the course's own higher-standing sources on its subject (one page sized by the page budget, further pages by the break-even rule), those pages are levelled, and the claim is re-weighed with the ceiling they establish. |
| 5 | Both passes' supports count: when pass A and pass B both support, both passages are weighed; the strongest counted support names the claim's verdict and its citation. |
| 6 | A repaired claim's id continues only in new text inside the changed region of its field; children of a repair inherit the parent's remaining attempts. |
| 7 | A repair edits only the failed sentences; every protected sentence comes out byte-identical or the edit is reverted or rejected. Teaching-review findings on protected sentences wait for the user. `InsufficientStanding` is no longer repairable (no deletion for lack of standing). |
| 8 | The tally counts only user actions as "resolved by you"; machine rewrites and removals get their own count. |
| 9 | No checkpoint key changes; one migration (`selection_note`); nothing re-runs eagerly on first start (§13). |

## 3. What stays as it is

Pass A and pass B, their keys and receipts (`claim-v1`, `claim-evidence-v2`, `claim-walk-v1`); the hybrid ranking and `REACHES`; the page budget and the break-even rule (`Depth::reads_next`, `walk.rs:95-110`); the judge, its prompts and `STRICT_INTERPRETATION_POLICY`; `TRUST_POLICY_VERSION` (receipts do not depend on levels; the gate re-decides from cached judgements every pass); the static bars table and calibration (`sufficiency.rs:147-181`, `calibration.rs`); the ownership judgement (`source-subject-ownership-v1`) and the claim profile; scrutiny and web research; the user's queue and actions; `MACHINE_ATTEMPTS = 2`.

## 4. Fix A — standing judged from content: `source-standing-v2`

### 4.1 Content evidence (lattice-core, pure)

New file `core/domain/provenance/content_evidence.rs`:

```rust
pub struct ContentEvidence {
    /// Lines that name an author or publisher: "By …", "Author: …", "Written by …",
    /// "© 2024 …", "Copyright … <name>", in any language the regex table names
    /// (by/author/written by/autor/par/von/di/por). One line each, deduplicated,
    /// in text order.
    pub bylines: Vec<String>,
    /// The registrable domain's first label when a byline's letters contain it or
    /// it contains a byline token of 4+ letters (case-insensitive, letters only):
    /// "prakashkagitha" ↔ "Prakash Kagitha".
    pub name_in_domain: Option<String>,
    /// Same-site links whose path or anchor names an about/author/contact/team/
    /// people/bio/imprint page.
    pub about_links: Vec<String>,
    pub citations: CitationProfile,
    pub work: WorkShape,
    /// How the course got the source and, for research, why the planner took it.
    pub chosen: Chosen,
}
pub struct CitationProfile {
    pub external_domains: usize,        // distinct registrable domains linked, not the site's own
    pub restricted_registry_links: usize, // links whose TldRegistry is Restricted (gov/edu/mil/int)
    pub scholarly_identifiers: usize,   // DOI syntax 10.NNNN/… or an arXiv identifier in a link
    pub same_site_links: usize,
}
pub struct WorkShape {
    pub words: usize,
    pub headings: usize,
    pub heading_lines: Vec<String>,     // the same rule DocumentSignals.heading_lines uses
    pub numbered_path: bool,            // a URL path of numbered segments, e.g. /08-…/01-…
    pub code_blocks: usize,
    pub has_abstract_or_references: bool, // a heading named Abstract, References or Bibliography
}
pub struct Chosen { pub by: AddedBy, pub note: Option<String> }

impl ContentEvidence {
    pub fn of(text: &str, links: &[OutboundLink], origin: &Origin, chosen: Chosen) -> Self;
}
```

Everything is derived from the saved text, the links already recorded (`ProvenanceRecord.links`, `core/domain/provenance/record.rs:39-40`) and the origin. No list of vendors or domains; the only word lists are the byline cue words and the about-page path words, which are reader vocabulary, not publishers. The lists are bounded by construction (one line per match, deduplicated); the judgement request is sized the way `judge_standing` already guards oversized metadata (the `MetadataTooLarge` fallback with `RiskFlag::OversizedMetadata`, `core/application/services/source_trust/standing_judgement.rs:313-316`): before that fallback is reached, when the evidence does not fit the request's room, `bylines`, `about_links` and `heading_lines` are cut from the end and `truncated: true` is set on the input, so a long page is judged on less content evidence rather than not judged.

### 4.2 The judgement

`core/application/services/source_trust/standing_judgement.rs`:

- `STANDING_POLICY = "source-standing-v2"`.
- `standing_input(signals, content: &ContentEvidence, track_record)` → `{ "signals": judged_form, "content": content, "track_record": … }`.
- `standing_key(signals, content, model)` → `input_sha256 = sha256(canonical({signals: judged_form, content}))`. Two pages with the same metadata but different content get different judgements, as they must. `track_record` stays input, not key.
- `STANDING_SYSTEM_V2`: judge who publishes the page from the structured signals **and** the content evidence. The content evidence is data about the page's own text: bylines and copyright lines name an author or publisher; a name that appears both in a byline and in the domain identifies a personal site's author; about/author links say the site identifies itself; a chaptered work with many headings is a book or course text, a text with an abstract and references is a paper, a page with many citations to independent and scholarly work stands on sources; `chosen` says how the course obtained it and, for research, why the planner selected it, which is a selection, not a verdict on standing. Missing metadata is not evidence that nobody publishes the page: say `unknown` only when neither the metadata nor the content identifies who does. Everything else as v1 (self-description is never evidence; flags need a concrete signal; one reason per conclusion naming its signal).
- New reason codes in `STANDING_REASONS` and `ReasonCode` (`core/domain/provenance/reason.rs:27`): `BylineInContent`, `NameInDomain`, `AboutPageLinked`, `CitesIndependentWork`, `WorkShape`, `ChosenForCourse`; and, for the level ladder of §5.3, `WorkStandsOnSources`, `WorkUnsourced`, `ArticleCitesSources`, `UnsortedWorkCitesSources`.
- `StandingAssessment` (`core/domain/provenance/standing.rs:150-170`) gains `#[serde(default)] work: WorkEvidence`, a deterministic summary `parse_standing` and `fallback` copy from the judgement's input, never from the model's answer: `{ external_domains, scholarly_identifiers, restricted_registry_links, headings, has_abstract_or_references, numbered_path, bylines, name_in_domain: bool }`. It is what `level.rs` reads (`LevelInputs.standing.work`), so `relevel`, the source view and the gate level a version the same way without the text in hand. Its two predicates are methods: `stands_on_sources()` (two external domains, or one scholarly identifier, or one restricted-registry link) and `structured()` (two headings, or an abstract/references heading, or a numbered chapter path).
- Schema, `Answer`, `add_settled_flags` unchanged except the policy string, the key and the copied `work` summary.

### 4.3 Callers

- `provenance_gate/context.rs:472-530`: `version()` already holds `SavedText { text, links }` and `added_by`; it also loads the new `selection_note` (§4.4). `standing(llm, provenance, saved, chosen)` computes `ContentEvidence::of(&saved.text, &saved.links, &provenance.origin, chosen)` and the v2 key.
- `references/trust/judgements.rs:31-61` (`reassess_source`, the UI's "Assess"): loads `full_text` and `added_by`/`selection_note` for the active version and judges the same way.
- `references/trust/source_view.rs:106-117` (`latest_standing`): the query by `(signals_sha256, policy)` keeps its shape but receives the v2 key's `input_sha256`, computed from the version's text (`source_view.rs:199` `text()` already exists). A source with only a v1 row shows "not judged yet" with the Assess action, as an unjudged source does today.
- `core/application/services/source_trust/tests.rs` and `core/domain/provenance/fixture_tests.rs`: `ContentEvidence::default()` where a fixture has none; new fixtures in §14.

### 4.4 The selection note

Migration `crates/lattice-desktop/migrations/20261022120000_source_selection_note.sql`: `ALTER TABLE learning_source_library ADD COLUMN selection_note TEXT NULL;` — the planner's reason for the selected result ("which gap each selection can resolve, including its scope and authority", `research.rs:31`). `select_references` (`research.rs:116-228`) already parses it as `Selected.reason` (`research.rs:179-183`) and today drops it after selection; it is carried to `Saving::save` (`research.rs:689-733`), which passes it into `NewLearningSource`, and the library repository writes it (repository owns the SQL; `scripts/check-sql-contracts.py` prepares it against the migrations). Existing rows have `NULL`: the judgement sees `chosen.by` without a note. Nothing converts.

### 4.5 Cost and re-assessment

A standing judgement is one small structured call (signals + content evidence in, ~200 tokens out), cached per `(input_sha256, v2, model)`. Nothing re-judges eagerly. For the owner's held lesson, the versions weighed on its next pass are re-judged: the supports of 33 claims and the versions on the standing-reach pages, in the order of tens of calls, not 410.

## 5. Fix B — levels: the user's choice, reliance by kind of work, the owner's own account (`level.rs`)

Changes to `own_level` (`level.rs:181-250`) and `trust_level` (`level.rs:72-178`):

1. **Owner's own account.** With `relation == Owner` and `editorial != CommunityEdited`: natures `PrimaryDocument`, `Documentation`, `Reference`, `Reporting`, `Tutorial` and `Unknown` → `Primary` (`OwnerOfSubject`, "Primary for {subject}: {publisher} owns it ({basis})"; for `Tutorial`/`Unknown` the reason adds "its own guide" / "its own page"). `Marketing` → `Established` (`OwnerMarketing`, unchanged). `Opinion`, `Discussion`, `Aggregation` → `Established` (`OwnerHostedCommunity`, unchanged wording). The `CAPPED_NATURES` rule (`level.rs:48-52, :239-249`) stays. This is the generic rule "publisher = the subject's own author": a standards body for its standard, a project for its software, a legislature for its law, an author for their work, a museum for its collection record. Ownership is still the per-(origin, subject) judgement (`core/application/services/source_trust/ownership_judgement.rs:17`); no host list is consulted.
2. **Added by the user.** After `inherit` (`level.rs:165`), when `inputs.added_by == AddedBy::User` and the level is below `Established` and not `Distrusted`: level `Established`, reason `AddedByYou` ("Added by you: counts as an identified source; set a level for it to say more"). The existing "origin not stated" note stays.

   How this sits with the two rules that already exist. `decide` accepts any usable support whose `added_by` is `User` as sufficient before any bar is read (`sufficiency.rs:230-244`, "Supported by material you added; its standing is shown, not weighed"), and `claim_reviews/attach.rs:7-8` says a source the user added "is never weighted differently: it is checked like any other". Both stay, and the order is: the **judge** decides whether the user's page states the claim (it reads the page like any page); if it does, the **gate's user rule** makes it sufficient whatever its level; this floor only decides what level the Sources tab shows for the user's material, how it corroborates other claims (two independent `Established` origins settle a specification or historical claim, `corroborated`, `sufficiency.rs:437-468`) and the ceiling of §7.4. A page the user adds can therefore come back "not enough" only when the judge finds that it does not state the claim, and the review UI then shows the closest passage and says so (UI design §3.6, §4.6).
3. **Chosen for the course: reliance by the kind of work.** When `inputs.added_by ∈ {Research, Outline, Import}` and the v2 judgement still cannot identify the publisher (`Standing::Unknown`), the level is not a flat floor. It comes from the kind of work the judgement classified (`nature`) and the content evidence the judgement was shown (§4.1), as a graded ladder, one step below what an identified publisher of the same kind of work would get. The rule is `work_level(nature, work: &WorkEvidence) -> Option<TrustLevel>` in `level.rs`, applied in `own_level` in place of the `(Standing::Unknown, _) => Unknown` arm (`level.rs:319-323`) for these sources only. Rows are tried top-down; the first that matches gives the level.

   | row | nature (v2) | content evidence (`StandingAssessment.work`, §4.2) | level | reason code, text |
   |---|---|---|---|---|
   | 1 | `PrimaryDocument`, `Reference`, `Documentation` — a reference work, a book or textbook chapter, documentation | *stands on sources* and *structured*: links to at least `CORROBORATING_ORIGINS` (2) distinct external registrable domains, or one scholarly identifier, or one restricted-registry link; and at least two headings, or an abstract/references heading, or a numbered chapter path | `Established` | `WorkStandsOnSources`: "A {nature} that cites independent work ({n} sources, {s} scholarly); publisher unidentified" |
   | 2 | `PrimaryDocument`, `Reference`, `Documentation` | structured, no independent citations | `Community` | `WorkUnsourced`: "A {nature} without citations to independent work; publisher unidentified" |
   | 3 | `Reporting`, `Tutorial` — an article, explainer or tutorial | stands on sources | `Community` | `ArticleCitesSources`: "An article that cites independent work ({n} sources); publisher unidentified" |
   | 4 | `Reporting`, `Tutorial` | no independent citations | `Unknown` | `UnknownPublisher` (today's) |
   | 5 | `Unknown` nature | stands on sources and structured | `Community` | `UnsortedWorkCitesSources`: "A structured text of unclassified kind that cites independent work; publisher unidentified" |
   | 6 | `Discussion`, `Opinion`, `Aggregation`, `Marketing`, or `Unknown` nature without row 5's evidence | any | `Unknown` | `UnknownPublisher` (today's) |

   The ladder does not apply when `flags` holds `ContentFarmPattern`, `GeneratedContentPattern` or `InjectionAttempt` (the work is not what it looks like: level `Unknown`, and the injection rule distrusts as today); with `SelfDeclaredOfficial` it applies and step 8's `Community` cap (`SelfDeclaredOfficialCap`) follows, as today. Steps 7 and 9 (`CAPPED_NATURES`, `inherit`) are unchanged and run after it. No word count or any other size threshold is used: "depth" is the judgement's classification of the work's shape (`WorkShape`), the two-domain rule is the corroboration rule's own number (`sufficiency.rs:153`), and "two headings" is the least a sectioned text can have.

   Read as a ladder: an identified institutional publisher with editing is `Authoritative`; an identified publisher, or the user's own choice, or an unidentified publisher's cited reference work is `Established`; an unidentified publisher's uncited reference work, cited article or structured compendium, like community content, is `Community`, where two independent origins corroborate (§6.1) and one alone settles nothing; an unidentified publisher's uncited article, discussion, aggregation or marketing stays `Unknown`. Examples the tests use: row 1, an online regional flora with species accounts by family citing herbarium records and journal articles (biology), a local history society's online book on its town with chapters and footnotes to archives (history); row 2, a chaptered bird guide with no citations, a family-history site with chapters and no sources; row 3, a blog explainer on CRISPR citing two papers, a history magazine article citing two archives; row 4, an uncited explainer on mitosis, an uncited article on the Peloponnesian War; row 5, a long structured compendium of plant toxicity data citing journals, a structured timeline of a dynasty citing chronicles; row 6, a gardening Q&A thread, a forum debate on the causes of a war.

No change to `standing_level` (`level.rs:283-327`) for identified publishers, nor to `inherit`, overrides or the injection rule. `TRUST_POLICY_VERSION` stays `source-trust-v1`: the gate re-decides from cached judgements on every pass, and a changed decision is `NewInformation::DecisionChanged` for the ledger (`core/domain/learning/claim_review/rules.rs:96-100`).

## 6. Fix C — the bar relative to what the course has (`sufficiency.rs`)

### 6.1 The rule

`decide(profile, supports, contrary, bars, available: &Available)` (`sufficiency.rs:213-276`) gains one input and one step:

```rust
/// What the course has on the claim's subject, for the best-available rule.
pub struct Available {
    /// The highest level any course source reaches for this subject among the
    /// sources levelled for the claim and the proxies of §7.3; `None` when
    /// nothing beyond the claim's own supports is known.
    pub ceiling: Option<TrustLevel>,
    /// How many course versions on the subject the ceiling was taken over.
    pub considered: usize,
}
```

Order inside `decide`: exclusions and the user's own evidence (unchanged) → `alone` (unchanged) → `corroborated` (unchanged) → **`best_available`** (new) → `insufficient` (unchanged, plus one `Missing` variant).

`best_available(profile, usable, contrary_levels, available, reasons) -> Option<Decision>`:

- Candidates: usable supports whose passage is `Direct` and whose counted level is at or above `available.ceiling` (every usable direct support when the ceiling is `None`). A support below the ceiling is not a candidate: the course has a stronger source on this subject and it did not state the claim (it was read by the standing reach, §7).
- Blocked when any contrary's level is at or above the best candidate's level, or when the kind is spec-like and a `Primary` contradicts (the same rules `alone` and `finish_corroboration` apply, `sufficiency.rs:366-371, :518-527`).
- `ClaimKind::Measurement` never passes here (it needs its origin or two reports naming it, `reported_measurement`, `sufficiency.rs:470-506`).
- Best candidate at `Established` or above: `Sufficient { met_by: candidates best first, label: Some(Label::BestAvailable { level }) }`, reason `MetAsBestAvailable`: "{id} ({level}) is the strongest source the course has on {subject}; nothing in the course stands higher on it ({considered} sources considered)".
- Best candidate at `Community` or `Unknown`: needs at least `CORROBORATING_ORIGINS` (2) distinct `origin_cluster`s among the candidates at that level or above (`best_per_cluster`, `sufficiency.rs:547-564`); then `Sufficient` with the same label and reason "{n} independent origins, the strongest the course has on {subject}". Fewer: `None`.
- New `Label::BestAvailable { level: TrustLevel }` (`sufficiency.rs:77-85`); `references/trust/dto_map.rs` maps it to the UI ("the course's strongest source on this subject: {level}").
- `insufficient` (`sufficiency.rs:566`) gains `Missing::StrongerSourceSilent { level: TrustLevel }`, chosen when `available.ceiling > best support level` and `considered > 0`: "the course has a {level} source on this subject that does not state it". Its next steps are the existing ones (`SearchOwner`, `SearchIndependent`, `AskUser`).

`Available` travels with the decision: `FindingTrust` (`provenance_gate.rs:32-43`) and `TrustAssessment` (`core/domain/learning/claim_review/trust.rs`) gain `#[serde(default)] available: Option<Available>`; `TrustAssessment::redecide` and `relevel` (`provenance_gate/relevel.rs`) pass it back to `decide` (an assessment stored without it re-decides with `Available { ceiling: None, considered: 0 }`, which is today's behaviour).

### 6.2 Why this is the right bar

The static table says what kind of source settles a kind of claim in a field where such sources exist. The ceiling says what the course has. A history course whose best source on the Treaty of X is a university press chapter keeps the `Authoritative` bar for historical claims because the ceiling is `Authoritative`; a claim that chapter does not state waits for the user, as it should. A cooking course whose sources on a technique are two identified authors' sites verifies a practice claim they both state (`Established`, met alone) and a claim one of them states (best available, `Established`). A software course with the vendor's page in its sources keeps the `Primary` bar for specification claims about that vendor's API because the ceiling is `Primary`. The owner's book, judged `Identified` under v2 (author's name in the domain and bylines, a chaptered work citing papers and documentation), or `Established` by row 1 of the §5.3 ladder even if its author stays unidentified (a chaptered reference work citing independent papers and documentation), supports definition and practice claims where the course has nothing stronger on the subject, and corroborates with an independent origin where it does.

## 7. Fix D — the standing reach

### 7.1 Where it runs

`evidence_checks.rs:178-357` (`check_with_paging`), after `run.finish` and the gate's `weigh` (`:347-352`): a new step `standing::corroborate(llm, references, &mut results, &run.decided, context, labels, &mut answers).await?`, in a new module `evidence_checks/standing.rs`, before `sent_back` is reported. It runs inside the preparation job's trust scope (`Judgements::open`, `provenance_gate/context.rs:166-175`); outside one (no provenance stored) it does nothing, like `weigh`.

### 7.2 Which claims

`Finding::needs_standing()` (in `standing.rs`): not settled (`run.decided` holds the ordinals of quotation- and user-settled claims), and the verdict is `Unsupported` (including what `proposes_general_knowledge` would later propose, `general_knowledge.rs:67-69`), `Contradicted`, or `Supported` with `trust.decision` `Insufficient`. `Unverified` claims are not read on (the judge could not judge them).

### 7.3 The claim's standing scope (model-free, one cached hint call at most)

`Judgements::standing_scope(&mut self, profile, supported: &[String]) -> Result<StandingScope>` in `provenance_gate/context/scope.rs`:

```rust
pub(in …learning) struct StandingScope {
    /// Course versions worth reading for standing, minus the claim's supports.
    pub candidates: HashSet<String>,
    /// Each candidate's proxy level, for the ceiling: current checker only.
    pub proxies: HashMap<String, TrustLevel>,
    /// Candidates a lead points at (any checker's ownership work).
    pub led: HashSet<String>,
    /// Hosts where the subject's owner is believed to publish, from every
    /// checker's ownership judgements for the subject and its aliases.
    pub owner_hosts: Vec<String>,
}
```

Two rules, added after the owner's held lesson stalled with 33 claims (2026-10-25):

1. **A version nobody has judged is read.** Every stored judgement in that course was `source-standing-v1`; the scope read only v2, so no version had a standing proxy and the vendor documentation the course held (OpenAI's function-calling guide, Anthropic's define-tools page) was never a candidate. §4.5's plan, that the versions on the standing pages get re-judged, could not start: nothing reached a page without a judgement. A captured version with no judgement under the current policy and checker is now a candidate with no proxy; the walk levels it when it reads it (§7.4), so it raises the ceiling only once read.
2. **Earlier checkers' work is a lead, never a level.** Proxies and `Owner` relations come from the current checker's judgements alone: the ceiling decides `BestAvailable` (§6.1), and a stale `Owner` from an earlier model (one had judged a personal blog the owner of "tool schema") would block every lesser support. Ownership work only leads: the origins a judgement names as owner and the hosts it hints are `led`. The current checker's judgement is the lead once it has made one (`owner_judged` is the current checker's, so a new checker makes its own call before the scope is built); every checker's judgements on the subject's key and aliases (`SqliteSourceTrustStore::subject_leads`) lead only a subject it has not judged. In the owner's course an earlier checker had judged a personal blog the owner of "tool schema", "function calling" and "schema"; as a lead beside the current call it put the blog's 22 versions ahead of the vendor's documentation on the one page a standing walk always reads.

A version is a candidate when any of these holds, each read from the trust store or the library row without a model call:

| proxy | source of the fact | proxy level |
|---|---|---|
| the current checker's ownership judgement `Owner` for (its origin, the claim's subject) | `source_subject_authority` | `Primary` |
| a lead: the current checker judged its origin the subject's owner or hinted its host (earlier checkers' judgements on the subject and its aliases, when the current checker has none) | `source_subject_authority` | none (a belief: it is read first, it does not raise the ceiling) |
| no judgement under the current standing policy and checker | `source_standing_assessments` (absent) | none (read, levelled once read) |
| stored v2 standing `Institutional` with `PeerReviewed`/`Editorial` | `source_standing_assessments` | `Authoritative` |
| stored v2 standing `Institutional` or `Identified` | same | `Established` |
| stored v2 standing `Unknown` whose `work` summary meets a §5.3 ladder row (`work_level`, no model) | same | the row's level (`Established` for row 1, `Community` for rows 2, 3, 5) |
| `added_by = user` | `learning_source_library` | `Established` |
| `tld_registry` restricted (gov/edu/mil/int, `core/domain/provenance/origin.rs:55-80`) | `source_provenance.origin_json` | `Established` |
| a site or source override at or above `Established`, or a site-for-subject override | `Overrides` (already loaded) | the override's level |

A version Distrusted by override, or whose host is a Distrusted site, is never a candidate; nor is one the current checker judged below the ladder (Community, an unranked Unknown). The claim's current supports are removed. What it was shown is excluded passage by passage by `claim_list`: one silent excerpt of a document must not hide the rest of it.

**Reading order.** `StandingScope::tier` orders the claim's list, stably so relevance orders each tier: tier 0 is a proxy at or above `Established` or a lead, tier 1 a version nobody has judged, tier 2 a proxy below `Established`. The walk reads the course's strongest sources first.

When the current checker has no ownership judgement for the subject and the claim has a subject with a host-bearing origin to ask about: one ownership judgement `authority(llm, origin of the first passage of the claim's course list minus shown, subject)` (`context.rs:435-470`, cached under `(program, origin, subject)`), which yields the subject's `owner_origin_hints`. That is the only model call the scope may make, and it is reused by the gate when that origin later supports anything.

Needs: `SourceTrustStore` gains `authority_for_subject(program_id, subject_key) -> Vec<AuthorityAssessment>` and `standings(keys: &[JudgementKey]) -> Vec<StandingAssessment>` (port `core/application/ports/source_trust.rs`, repository `references/trust/repository.rs`); `Judgements` loads the program's provenance records once per pass (`program_provenance`, already used by `passage_labels`, `context.rs:538-579`) and the library's `added_by` per source (one query through the learning repository). Every new SQL lives in a repository.

### 7.4 The walk

- `Reach::Standing` is added to `Reach` (`walk.rs:42-54`, serde `snake_case`; additive for saved `Selection.reach` and `TwoPass.reach`). `scope_of` (`walk.rs:239-245`) returns `Scope::Keys(scope.candidates)` for it; `Walk` gains `standing: Option<StandingScope>` set by a new constructor `Walk::standing(ordinal, unit, claim, checked, shown, scope, budget)` whose reaches are `[Reach::Standing]`. `REACHES` is unchanged: the standing reach is not part of pass B's order.
- Shown: the finding's evidence (pass A's passages and the deciding or last pass-B page) minus execution runs. Pages the walk read and that stayed unsupported are not on the finding; a repeat of such a passage on a standing page is harmless (its receipt is per page composition, and `claim_list` excludes only `shown`).
- Budget: `walk_budget(&format!("standing:{decision_key}"), PageBudget::of(llm, *answers))` (`walk.rs:534-547`), where `decision_key` is `CheckedClaim.1`; the same key `claim-walk-v1:` namespace, so a resumed job pages the same way.
- Depth: the first page is read whenever the list has a passage on the subject (the D1 rule pass B applies to its first page); further pages by `Depth::reads_next` with the same `research` cost `Run::finish` computes (`run.rs:146-151`). No count, no new constant.
- Checks: `deepen` (`walk.rs:560-700`) is reused as it is for the standing walkers (`Check::Deeper`, receipts `claim-v1` per page, restored when saved, batched by `pack`/`group`). Progress: `deeper_page`/`rechecked`/`deeper_done` as today, with a stage line "Reading the course's strongest sources for {n} claims".
- Levelling: every version on a page the standing walk read is levelled for the claim's subject, `judgements.facts(Some(llm), versions, profile)` (`context.rs:312-328`), supported or not. These facts and the scope's proxies give `Available { ceiling, considered }` for the claim (§6.1): `ceiling = max(levels of supports, contraries, pages read, proxies of the scope's candidates that the claim's list returned)`, excluding `Distrusted`; `considered` = the number of distinct versions behind that maximum. Hinted-but-unjudged hosts never raise the ceiling.

### 7.5 Combining and re-weighing

`outcome::with_standing(prior: Finding, found: Option<(Finding, Reach, usize)>, available: Available) -> Finding`:

- found `Supported`: the found passage becomes the finding's `supporting_quote`; `two_pass.supported_by` names it when its source is not among the section's cited versions (`SupportedElsewhere`, so the citation repair cites it, `repair/citation.rs`); the prior support, if any, moves to `two_pass.also_supported` (§8); `two_pass.pass = Corroboration`, `reach = Standing`, `page = n`; verdict `Supported`.
- found `Contradicted` and prior `Supported`: verdict stays `Supported`, the found passage joins `two_pass.contrary` (the overturn rule weighs it, as `combine` does today, `outcome.rs:127-134`). Found `Contradicted` and prior `Unsupported`: `Contradicted`.
- found `Unsupported` or nothing found: the prior finding, unchanged (its evidence is not extended, so its fingerprint and receipts are stable).

Then `provenance_gate::weigh` runs again for the findings the step changed or whose `Available` changed, now with the ceiling; `assess` (`provenance_gate.rs:249-374`) passes `available` into `decide`. The ledger then sees the new verdicts through `check_of` (`observe.rs:174-258`) as today: a queued claim with a sufficient decision becomes `ResolvedByEvidence` (`replay.rs:274-310` `redecided`, `:121-186` `apply_check`); one still insufficient stays queued.

### 7.6 Cost

Per claim that needs standing: one `claim_list` over the scope, one judge comparison per page read (packed per section like other requests), standing and ownership judgements for the versions on the pages read (cached, a handful per claim), and at most one ownership-hint call per subject lacking one. Every call is the same shape and size as the calls the pipeline already makes; nothing scales with the course's size except the cached judgements of the pages actually read.

## 8. Fix E — both passes' supports count (`outcome.rs`, `provenance_gate.rs`)

- `TwoPass` (`outcome.rs:49-66`) gains `#[serde(default, skip_serializing_if = "Vec::is_empty")] also_supported: Vec<SupportingPassage>`, with `SupportingPassage { passage: EvidencePassage, quote: String }` (the shape of `ContraryPassage`, `outcome.rs:42-47`).
- `combine` (`outcome.rs:112-162`): when pass A supports and pass B also supports, B's `supporting_passage` is pushed onto `also_supported` (today it is dropped). Everything else unchanged.
- `supporting_passages` (`provenance_gate.rs:228-241`) returns the passages holding the quotation **and** every `also_supported` passage, one per source version, in that order. `weighed` (`:488-517`) levels them all; `assess` weighs them all, so independent clusters corroborate.
- `FindingTrust::assessment` (`provenance_gate.rs:78-95`) names as `supported` the first id of `Decision::Sufficient.met_by` when the decision is sufficient, else the deciding quotation's passage. After `assess`, `weigh` (`:444-486`) rewrites `finding.supporting_quote` and `two_pass.supported_by` to that strongest support when it is not the current one, so `check_of` (`observe.rs:174-258`) records the claim as verified by the source with standing and the citation repair cites it.
- "A supported check by a source without standing must not end the walk": the walk's stop rule inside `deepen` is unchanged (a page's `Supported` ends it; the gate cannot run between pages without judgements). The continuation for weak support is the standing reach (§7), which runs after the gate has said the support is insufficient. This is stated so the builder does not add a mid-walk gate.

## 9. Fix F — a claim id never moves to another sentence (`observe.rs:397-424`)

The loop over `repairs` keeps its purpose (a rewritten sentence continues its claim and its attempts) and gains these conditions for a successor, all computed from `self.pending.previous` (the lesson before the repair) and the current candidate:

1. Same unit and field, unowned (unchanged).
2. Its quote is **new text**: it does not occur in the previous field text.
3. Its span lies inside the field's **changed region**: the bytes between the common prefix and the common suffix of the previous and current field texts (`text_edits`-style exact comparison; `ClaimLocation.span`, `core/domain/learning/claim_review.rs:100-102`; a claim without a span does not qualify).
4. Among several qualifying claims, the one whose statement shares most words with `record.before` (the tokeniser `repair/citation.rs::shared_terms` already uses), ties to the earliest span. The others are **children of the repair** (§9.1).
5. None qualifying: the orphan keeps its orphan path (`observe.rs:462-477`), but the `RepairAttempted` event records `outcome: Rewritten, after: Some(<changed region text>)` when the region is non-empty and `Removed` only when the region is empty.

### 9.1 Children inherit the budget

Every unowned extracted claim whose span lies in a target's changed region and that is not the successor is started with `ClaimEvent::BornOfRepair { parent: ClaimId, attempts_left: u8 }` appended after `Extracted` (`observe.rs:497-517` `start`), `attempts_left` being the parent's `machine_attempts_left` after the attempt is charged. Replay (`core/domain/learning/claim_review/replay.rs`) keeps `inherited_budget: Option<u8>` on the cursor and `first_failure` (`:49-63`) uses it instead of `MACHINE_ATTEMPTS` when set. A split sentence therefore adds no attempts (§12).

## 10. Fix G — a repair changes only the failed sentences

### 10.1 Protected sentences

`repair::Protected` (new, in `repair.rs` or a sibling `repair/confine.rs` to respect the 800-line rule): for the section under repair, the `location().quote` of every live ledger claim in that unit that is **not** a target of this repair: verified, confirmed, general knowledge, queued, reopened, open-but-not-targeted, unchecked. Built in `repair_context` (`repair_loop.rs:464-517`) from the ledger and carried in the context as `"protected": [{unit, field, quote}]` so a resumed repair (`PendingRepair`, `repair_loop.rs:384-388`) has it.

### 10.2 Confinement

`repair::confine(previous: &Value, repaired: &Value, targets: &[Protected], protected: &[Protected]) -> Result<Value>` runs on every string leaf of the section, after `apply_or_replace_section` (`repair.rs:309-342`, both the edit path and the regenerated-section path) and before schema validation in `repair_group` (`repair.rs:515-536`):

1. Split both texts into sentences with `repair/citation.rs::sentences` (`:15-39`, made `pub(super)`) and align them by a longest-common-subsequence over sentence strings (written in the repo, no new crate; the inputs are one field's sentences).
2. A changed hunk (sentences of the previous text replaced, deleted or inserted) is **allowed** when it overlaps a target's quote span in the previous text, or is an insertion adjacent to an allowed hunk. Every other hunk is **reverted** to the previous bytes.
3. A hunk overlapping both a target and a protected sentence is rejected: `Err(rejected(...))` ("The repair rewrote a sentence that already passed together with the failed one; the draft is kept"), which `repair_sections` already turns into `ledger.reject(&claims, …)` and keeps the section (`repair.rs:487-495`), charging the attempt.
4. After reverting, every protected quote must occur byte-identically in the leaf; otherwise reject the same way.
5. Non-string leaves (numbers, booleans, `kind`, `source_ids`) follow the existing `merge` kind check and are otherwise reverted unless a target's claim is located in that field.

The "ambiguous edit → one validated section" fallback (`repair.rs:317-340`) stays, confined the same way: a regenerated section may change only the failed sentences. The repair system prompt (`repair.rs:163-168`) adds one sentence: "Edit only the failed sentences; every other sentence is returned byte-identical and any change to it is discarded."

### 10.3 Teaching review after a repair or a user edit

`teaching::review_lesson` (`lessons/teaching.rs:274-362`) and `review_material` (`:389-689`) gain `protected: &[Protected]` (empty at first generation, `lessons/generation/lesson.rs:409`; the ledger's protected set at `repair.rs:363` and `repair_loop.rs:107`). In `review_material`:

- Findings whose `passage` text contains or lies inside a protected quote are held for the user, not repaired: `Passes::hold(keys)` adds their `FindingKey`s to `held` (`lessons/teaching/passes.rs:31-40`) before `passes.next(findings)`, so they are stopped and end in `set_aside` (the path that already exists for findings the repairs did not settle, `teaching.rs:597-607`).
- Both repair outputs (`can_edit_spans` edits, `teaching.rs:641-647`, and the whole-lesson `complete_json_with_progress`, `:649-657`) are confined per section with `repair::confine`, targets being the sentences the sent findings name and protected the protected set. A confined output that changes nothing is treated as `ModelAnswerUnusable` (findings wait for the user, `:659-666`), never a failure.

### 10.4 No deletion for lack of standing

`VerdictKind::is_repairable` (`core/domain/learning/claim_review.rs:178-183`) → `Unsupported | Contradicted` only. `may_research` (`core/domain/learning/claim_review/ledger.rs:280-289`) keeps `InsufficientStanding` by naming it explicitly (`kind.is_repairable() || matches!(kind, NoSource | InsufficientStanding { .. })`). `after_machine_failure` (`replay.rs:84-115`) then queues an `InsufficientStanding` claim once research has run (`_ => queued`), so it waits for the user and is never rewritten or removed by the machine. `claim_tally`'s `sent_back` (`claim_ledger.rs:541-545`) is unaffected.

## 11. Fix H — an honest tally

- `ClaimEvent::Superseded { successor, #[serde(default)] by: Actor }` with `Actor: Default = Machine` (`core/domain/learning/claim_review.rs`); `observe.rs:433-444` records `by: User` for the `edited_now` path and `Machine` elsewhere. `ClaimState::Edited { at, successor, by: Actor }` set by replay (`ledger.rs:474-487`).
- `ClaimCounts` (`rules.rs:303-337`): `removed` splits into `removed_by_you` (`Removed { by: User }` and `Edited { successor: None, by: User }`) and `rewritten` (`Removed { by: Machine }`, `Edited { successor: None, by: Machine }`). `general_knowledge` and `confirmed` are user states already.
- `LearningClaimTally` (`crates/lattice-desktop/src/features/learning/planning/curriculum.rs:232-250`): `resolved_by_you = confirmed + general_knowledge + removed_by_you`; new `rewritten: u32` ("rewritten or removed by the machine"). `claim_tally` (`claim_ledger.rs:534-561`) fills both. `src/features/learning/lessons/review/HeldLessonsReview.tsx:36-48` adds `[tally.rewritten, 'rewritten or removed by the machine']` to the machine list; bindings are regenerated with `npm run bindings:generate` (`package.json:55`), never edited by hand; `src/features/learning/lessons/LessonPreparationProgress.test.tsx` is updated for the new field.
- The `resume` guard in `lessons/claim_reviews/service.rs:84-121` uses `ClaimCounts::tally(..).queued` and is unaffected.

## 12. Fix I — the loop's bound

### 12.1 Invariant, pass P → P+1

With §9–§11 in place, for every verification pass that follows a `Decision::Repair(targets)`:

- **I1 (no reopen).** Every claim not in `targets` is re-extracted with the same quote, keeps its id, its state and its basis, and receives no new `Checked` event: `carry_on` (`observe.rs:522-600`) appends a check only when `new_information` is `Some` (`rules.rs:71-115`), and unchanged text with the same evidence fingerprint, checker and gate verdict gives `None`. Confinement (§10.2) guarantees the unchanged text.
- **I2 (budget).** `Σ machine_attempts_left` over machine-owned claims strictly decreases on every repair pass: each target is charged (`Rewritten`, `Removed` or `Rejected`, `ledger.rs:527-561`), a successor continues its parent's id and budget (§9), and children inherit the parent's remaining attempts (§9.1), so no sentence split or rewrite creates budget.
- **I3 (research).** Research runs once per claim and evidence fingerprint (`may_research`, `ledger.rs:280-289`; `researched` flag), and restores a budget only when it changes the claim's evidence (`apply_check` `new_evidence`, `replay.rs:137-144`).
- **I4 (failing set).** The claims that are the machine's at P+1 are a subset of the targets' successors and children at P, plus claims whose evidence research changed; the failing set never gains a claim whose text did not change.

### 12.2 Bound

Let `n` be the number of claims extracted at the first pass and `r` the number of research rounds the lesson runs (bounded by I3: at most one per claim and fingerprint). Repair passes are at most `n × MACHINE_ATTEMPTS × (1 + r)`; the standing reach adds no pass (it runs inside a pass); each pass ends in `Publish`, `NeedsReview` or one more `Repair` that consumes budget. The owner's measured loop (33 passes, 944 checks, a median of 7 checks per claim) is bounded by I1 to one check per claim per changed-evidence event.

## 13. What changes for you on first start

Nothing runs at app start. The two held lessons wait as they do now. The next time a held lesson's verification pass runs (Resume in the review after a change, or Retry on the job), this is what happens:

| what | count on the owner's data | re-run? |
|---|---|---|
| `claim-v1:` receipts | 1,478 + 291 | **restored**, none re-judged (POLICY, model, window, comparison key and `TRUST_POLICY_VERSION` unchanged) |
| `claim-evidence-v2:` pages | 265 + 142 | **restored** (RANKING unchanged); the embedding-model re-key happened today and is not repeated |
| `claim-walk-v1:` budgets | one per walked claim | restored; the standing reach writes new ones under `standing:<decision key>` |
| decisions | per claim | re-derived from receipts; the gate re-weighs every supported claim from cached judgements (no calls) plus v2 standing judgements for the versions it weighs |
| `source_standing_assessments` v1 rows | 84 | kept, no longer read; v2 rows written lazily: the supports of the 33 waiting claims and the versions on the standing-reach pages (tens of small calls, once) |
| `source_subject_authority` | 194 | kept and used (scope, proxies, hints); new rows for origins the standing reach reads and for subjects lacking any judgement (2 of the 8 eval claims had none) |
| standing-reach pages | ≤ 33 claims in `db8397ff` | one search each; one judge comparison per page, packed per section; a few pages per claim at most under the break-even rule |
| ledger | 33 queued claims | a claim the course's strongest source states becomes verified (`redecided`/`apply_check` on `Queued`); the rest stay queued with the new reason ("the course has a {level} source on this subject that does not state it", or what is missing) |
| tally | "74 resolved by you" | becomes "74 rewritten or removed by the machine" and `0` resolved by you; history is not rewritten |
| schema | — | one migration adds `learning_source_library.selection_note` (NULL for all 410 rows); no data conversion |

Before merging, the **coordinator** (not the builder) runs the app once against a **copy** of the owner's data folder with the two jobs resumed, and records in this section the counts actually observed: receipts reused, v2 judgements made, standing pages read, claims that left the queue, minutes taken.

## 14. Tests and evals

### 14.1 Unit tests the builder writes

| file | test | asserts |
|---|---|---|
| `core/domain/provenance/content_evidence_tests.rs` (new) | `a_byline_and_the_domain_name_identify_a_personal_sites_author` | "By Prakash Kagitha" + host `prakashkagitha.github.io` → `name_in_domain = Some("prakashkagitha")`, one byline |
| same | `citations_count_external_restricted_and_scholarly_links_without_a_domain_list` | links to a `.gov` page, a DOI, an arXiv id and the site's own pages → `restricted_registry_links 1`, `scholarly_identifiers 2`, `same_site_links 1`, `external_domains 3` |
| same | `a_chaptered_work_and_a_paper_have_their_shapes` | numbered URL path + many headings → `numbered_path`, `headings`; "Abstract"/"References" headings → `has_abstract_or_references` |
| `core/application/services/source_trust/tests.rs` | `v2_input_carries_content_and_chosen_and_keys_on_them` | the v2 key differs for the same signals with different content; `track_record` is not in the key; the schema still validates a v1-shaped answer |
| `core/domain/provenance/level_tests.rs` | `the_owners_own_guide_or_untyped_page_is_primary_for_its_subject` | relation Owner + nature `Tutorial` → Primary; nature `Unknown` → Primary; `Marketing` → Established; `Discussion` → Established; `CommunityEdited` → Established |
| same | `a_source_the_user_added_is_at_least_established` | AddedBy::User + standing Unknown → Established with `AddedByYou`; + standing Institutional/editorial → Authoritative (not lowered) |
| same | `a_cited_structured_reference_work_from_an_unidentified_publisher_is_established` (row 1) | the regional flora and the local-history book fixtures (§5.3) → Established with `WorkStandsOnSources`; the same with `added_by = User` stays Established with `AddedByYou` |
| same | `an_uncited_reference_work_from_an_unidentified_publisher_is_community` (row 2) | the uncited bird guide and the family-history site → Community with `WorkUnsourced` |
| same | `a_cited_article_from_an_unidentified_publisher_is_community` (row 3) | the CRISPR explainer citing two papers and the magazine article citing two archives → Community with `ArticleCitesSources`; one external domain only → row 4 |
| same | `an_uncited_article_from_an_unidentified_publisher_stays_unknown` (row 4) | the mitosis explainer and the Peloponnesian War article → Unknown with `UnknownPublisher` |
| same | `a_structured_text_of_unclassified_kind_that_cites_sources_is_community` (row 5) | the plant-toxicity compendium and the dynasty timeline → Community with `UnsortedWorkCitesSources`; unstructured → Unknown |
| same | `discussion_aggregation_marketing_and_opinion_from_an_unidentified_publisher_stay_unknown` (row 6) | the gardening Q&A thread and the forum debate → Unknown |
| same | `flagged_works_are_excluded_from_the_ladder_and_the_ladder_reads_as_one_order` | row-1 evidence with `ContentFarmPattern` or `GeneratedContentPattern` → Unknown; with `SelfDeclaredOfficial` → Community (step 8 cap); and across one biology fixture set the levels satisfy Authoritative (university herbarium, editorial) > Established (field guide author identified; cited flora, publisher unidentified) > Community (uncited bird guide; cited blog) > Unknown (uncited explainer) |
| same | `a_history_course_levels_without_any_vendor` | fixtures: a university press chapter (Institutional, Editorial, Reference) → Authoritative; a peer-reviewed paper → Authoritative; a legislature's statute page with relation Owner → Primary; a museum's collection record, Owner of the object → Primary; a historian's blog (Identified) → Established; Wikipedia (CommunityEdited) → Community; a user-uploaded archival scan (Document signals, no origin) → Established |
| `core/domain/provenance/sufficiency_tests.rs` | `the_courses_strongest_source_on_a_subject_meets_the_bar_as_best_available` | Definition claim, one Established direct support, `Available { ceiling: Some(Established), considered: 3 }` → Sufficient, `Label::BestAvailable { Established }` |
| same | `a_stronger_silent_source_blocks_best_available` | same support, `ceiling: Some(Authoritative)` → Insufficient, `Missing::StrongerSourceSilent { Authoritative }` |
| same | `two_independent_community_origins_are_best_available_but_one_is_not` | two Community supports in different clusters, ceiling Community → Sufficient; one → Insufficient; two in one cluster → Insufficient |
| same | `a_contrary_at_the_supports_level_blocks_best_available` and `a_measurement_is_never_best_available` | as named |
| same | `best_available_in_a_biology_course` | Historical/General claims about a species supported by a natural history museum page (Established) and a field guide author (Established), ceiling Established → Sufficient alone; a Specification-kind claim (a nomenclature code rule) with the code's publisher Owner in the course (ceiling Primary) and only the field guide supporting → Insufficient, `StrongerSourceSilent` |
| `evidence_checks/outcome` tests (`two_pass_tests.rs`) | `both_passes_supports_are_kept_and_the_strongest_names_the_claim` | A and B both supported → `also_supported` holds B; after `weigh` with B at Primary and A at Established, `supporting_quote` is B's and `supported_by` names B's source |
| same | `with_standing_takes_the_found_support_and_keeps_the_prior_as_corroboration` | prior Supported-insufficient + found Supported → found is the quote, prior in `also_supported`, `reach = Standing` |
| `provenance_gate_tests.rs` | `the_standing_scope_is_built_from_stored_facts_without_a_model` | stored Owner row, a restricted-registry origin, a user-added source and a hinted host → candidates and proxies as §7.3; `Judgements::cached` (no llm) succeeds |
| same | `the_ceiling_is_the_highest_levelled_or_proxied_source_on_the_subject_and_hints_do_not_count` | as named |
| `evidence_paging_tests.rs` | `a_claim_left_insufficient_reads_the_courses_strongest_sources_before_waiting` | scripted judge: pass A supports from a Community source, scope holds an Authoritative source whose passage states the claim; after `check_with_paging` the finding is Supported by the Authoritative passage with `reach = Standing`, one standing page read, its receipt saved under `claim-v1:` and restored on a second run with zero model calls |
| same | `a_claim_with_no_stronger_source_is_verified_as_best_available_without_a_standing_page` | scope empty → no search, no call; `Label::BestAvailable` |
| `claim_ledger_tests.rs` | `a_repaired_claims_id_stays_with_its_own_rewritten_sentence` | the reported case: the quotation claim is repaired; the field also holds "The `required` array…" verbatim from before, unowned; the id goes to the new sentence in the changed region, never to the pre-existing one; with no new sentence the event is `Rewritten`/`Removed` by the region |
| same | `children_of_a_repair_inherit_the_parents_remaining_attempts` | a target rewritten into two sentences: successor continues, child has `BornOfRepair`, both have `attempts_left` = parent's after the charge |
| same | `an_insufficient_standing_claim_is_never_a_repair_target_and_waits_after_research` | as named |
| `repair.rs` tests | `confinement_reverts_changes_outside_the_failed_sentences_and_keeps_protected_bytes` | an edit response that also rewords a protected sentence → the protected sentence is byte-identical, the target changed; the regenerated-section fallback likewise |
| same | `an_edit_that_merges_a_failed_and_a_protected_sentence_is_rejected_and_charged` | `ledger.reject` called, section unchanged |
| `lessons/teaching/loop_tests.rs` | `a_teaching_finding_on_a_protected_sentence_waits_for_the_user_and_the_repair_leaves_it` | held key, `set_aside` holds it, the sentence is byte-identical after the review |
| `claim_review_flow_tests.rs` | `a_repair_pass_touches_only_the_failed_sentences` (the §12 test) | 3 claims (2 verified, 1 unsupported); the scripted repair rewords a verified sentence too; after the pass: the two verified claims keep id, state, basis, no new `Checked`; the target's id continues with one attempt fewer; exactly one model check in the pass |
| same | `the_machine_attempt_budget_never_grows_across_passes` | over three scripted passes `Σ machine_attempts_left` is strictly decreasing until `NeedsReview` |
| `core/domain/learning/claim_review` rules tests | `the_tally_counts_only_the_users_actions_as_resolved_by_you` | machine `Edited{None}` and `Removed{Machine}` → `rewritten`; user ones → `removed_by_you`; `claim_tally` maps them |
| `lessons/live_page_recall_eval.rs` (non-live unit) | `the_standing_order_walks_only_the_scope` | with a fake collection: `standing` order lists only candidate versions |

Fixture sets (committed, synthetic, labelled by the builder with the labelling rule in `evals/provenance/README.md`; a second labeller is still owed): `evals/provenance/sources-v2.jsonl` extends `sources-v1.jsonl`'s format with an optional `content` (a `ContentEvidence`) and the v2 expected judgement, including the areas it already has (medicine, law, history, science, cooking, news) plus a biology set with no vendor at all (museum record, field guide author's site, university herbarium page, a nomenclature code's publisher, Wikipedia, a user-uploaded scan). `evals/provenance/sufficiency-cases-v2.jsonl` adds `available` and the best-available cases above. `fixture_tests.rs` reads both and asserts the §5 levels and §6 decisions.

### 14.2 Eval acceptance

The harnesses `live_claim_pairs_eval.rs` and `live_page_recall_eval.rs`, the eval directory `evals/grounding/claim-corroboration-db8397ff/` (README, `spec.json`, `build.py`, `pairs.jsonl`, `recall.jsonl`) and the two result JSONL files are uncommitted in `/Users/josh/Code/lattice-claims-eval` (`git status` there). The builder copies into this branch the two harnesses (additive `#[ignore]` lib tests), `README.md`, `spec.json` and `build.py`, and the README's result numbers. `pairs.jsonl`, `recall.jsonl` and the result JSONL rows stay local (owner's decision, §15: they are real excerpts of his saved pages); the builder adds `evals/grounding/claim-corroboration-db8397ff/.gitignore` listing `pairs.jsonl`, `recall.jsonl` and `../results/2026-10-21-claim-corroboration-*.jsonl` so they cannot be committed by accident, and the README says they are rebuilt with `build.py` from a database copy.

The builder does **not** run either eval or the first-start check; the coordinator runs them (§16). The builder's eval work is only the `standing` order below and the README's run lines for it.

**Recall eval, new mode.** `orders_from_env` (`live_page_recall_eval.rs:252-262`) accepts `standing`; `ORDERS` (`walk.rs:63-67`) gains `("standing", &[Reach::Standing])`. For that order, `recall_set` opens `Judgements::cached(pool, program_id, model)` (visibility widened to `pub(in crate::features::learning)`), reads the claim's stored profile (new `Judgements::profile_if_stored(statement)`; no model), builds `standing_scope` with no model, walks `Reach::Standing` with `walk_pages`, and writes per row `scope: { versions, owner_hosts, proxies_by_level }` or `scope: null` when no profile or ownership judgement is stored. `LATTICE_PAGE_RECALL_TRUST_MODEL` names the judgement model (default: the most frequent `model` in `source_subject_authority` for the program, read through the eval's own read-only query).

Bar, on the owner's DB copy and course index copy (the README's run lines, `LATTICE_PAGE_RECALL_ORDER=standing`): of the 6 recall claims with a stored scope (`cc-recall-11d420ee, -b5c6269d, -56a1e15e, -7cf7155f, -0ef43f21, -69c1da48`, all `insufficient_standing` in the copy), a target passage is on standing page 1 for at least 5, and within the break-even depth (`LATTICE_PAGE_RECALL_RESEARCH_SECONDS` from a run's log) for all 6; the 2 without a stored scope (`-e2e66bc4`, `-f8223602`) report `scope: null`. The `relevance` order's numbers are unchanged.

**Pairs eval.** Re-run unchanged (the judge is untouched): `primary` supported ≥ 6/9, controls 7/7 not accepted, `seen` within one pair of 14/16 and 11/15. Record the run in the README's results table.

**Non-software subject.** The course eval's committed request `evals/courses/requests/3-music-theory-production.json` (a topic-only, non-software course) is run once by the owner on this branch for stage `m1.l1` (`docs/design/2026-10-20-course-eval.md`); acceptance: no claim waits with `insufficient_standing` whose support is a source the run's trust view levels as the course's strongest on its subject; `lesson.rounds.repair ≤ 2`; `lesson.claims.quotes_unmatched = 0`; the pass-by-pass claim ids show no verified claim re-checked (`a_repair_pass_touches_only_the_failed_sentences` holds live). The numbers go into `evals/courses/results/` and `course-generation-history.md` as the course eval design requires.

**Live.** The owner resumes the two held lessons on a copy first (§13), then live. The log lines to look for: "Reading the course's strongest sources for N claims", `Claim page … reach = Standing`, the gate's `MetAsBestAvailable` / `StrongerSourceSilent` reasons in the claim cards, and the tally header.

## 15. Decisions taken by the owner (2026-10-21)

1. `pairs.jsonl`, `recall.jsonl` and the result JSONL rows stay local: they are real excerpts of the owner's saved pages, not synthetic. The harnesses, `build.py`, the README and the result numbers are committed (§14.2).
2. A flat floor for research-chosen sources with an unidentified publisher was rejected. Reliance varies by the kind of work and its content evidence on the graded ladder of §5.3 (cited, structured reference work `Established`; uncited reference work or cited article `Community`; uncited article, discussion, aggregation, marketing `Unknown`; content-farm and generated patterns excluded; steps 7–8 caps still apply; corroboration still lifts), with a unit test per row and a biology and a history fixture in each.
3. Best-available acceptance (§6.1) stays for Specification and VersionBound claims, with `Label::BestAvailable` shown; Measurement remains excluded.
4. The builder does not run evals or the first-start check; the coordinator does (§16).

No open questions remain.

## 16. Build order for one builder

This is builder **A** of the three-builder plan in the UI design (`docs/design/2026-10-21-claim-review-ui.md` §9): A builds this document, B the review screen, C the DTOs, the attach command and the mocks. A's files and the files A must not touch are listed there; §17 below names what A exposes for C.

Run `cargo check -p lattice-core -p lattice-desktop` after each step; no tests, clippy or gates until the coordinator runs them once at the end. `rustfmt` on single files, never `cargo fmt`. Keep every file at or under 800 lines (split `walk.rs`, already 747, if the standing walker pushes it over: `walk/standing.rs`).

1. **lattice-core, provenance.** `content_evidence.rs` + tests; v2 judgement (policy, input, key, prompt, reason codes, `StandingAssessment.work`); the user-added floor, the §5.3 ladder (`work_level`) and the owner's-own-account rule + the per-row tests; `Available`, `best_available`, `Label::BestAvailable`, `Missing::StrongerSourceSilent` + tests; `FindingTrust`/`TrustAssessment.available`; fixtures v2.
2. **lattice-core, claim review.** `is_repairable`/`may_research`; `Superseded.by`, `Edited.by`, `ClaimCounts` split; `BornOfRepair` + replay budget; tests.
3. **Store and migration.** Port and repository methods (`authority_for_subject`, `standings`, `selection_note` read/write); migration `20261022120000_source_selection_note.sql`; `Saving::save` writes the note.
4. **Gate.** `context.rs`: v2 standing with content evidence, `standing_scope`, `profile_if_stored`, `ceiling`; `provenance_gate.rs`: `supporting_passages` with `also_supported`, `assessment` names the strongest, `weigh` rewrites the named support, `Available` into `assess`; `reassess_source` and `source_view::latest_standing` on the v2 key.
5. **Outcome and standing reach.** `TwoPass.also_supported`, `combine`, `with_standing`; `Reach::Standing`, `Walk::standing`, `scope_of`; `evidence_checks/standing.rs` and its call in `check_with_paging`; tests in `evidence_paging_tests.rs` and `two_pass_tests.rs`.
6. **Ledger.** `observe.rs` successor rule and children; tally in `claim_ledger.rs`; DTO + `bindings:generate` + `HeldLessonsReview.tsx` + its test.
7. **Repairs.** `Protected` in `repair_context`/`PendingRepair`; `confine` and its use in `repair_group`; prompt sentence; `review_lesson`/`review_material` `protected` + `Passes::hold`; tests; the §12 flow tests.
8. **Evals (builder, no runs).** Copy the two harnesses, `README.md`, `spec.json`, `build.py` and the `.gitignore` of §14.2 (never `pairs.jsonl`, `recall.jsonl` or the result JSONL); add the `standing` order to the recall harness and `ORDERS`; add the README run lines for it and the bar. The builder does not run either eval.

Then, after the builder finishes and the coordinator has run the gates once: the **coordinator** runs the recall eval (`relevance` and `standing`) and the pairs eval on the DB and index copies and records the numbers in the README's results table and in §14.2, and runs the first-start check on a copy of the owner's data (§13) and fills in the observed counts.

What the builder must not touch: `REACHES`, `B_DEPTH`, `PageBudget`, `page_seconds` and the break-even rule; the judge prompts and `judge.rs`; `POLICY` (`lesson-evidence-v21`), `RANKING`, `TRUST_POLICY_VERSION`, any existing checkpoint key format; the static bars table and calibration; the ownership and claim-profile judgements; `text_edits::apply`'s protocol; `MACHINE_ATTEMPTS`; the research planner and web research; anything under `features/web`; `src/lib/bindings.ts` by hand; `lessons/claim_reviews/**` (builder C's) except the one-line wiring of the split `ClaimCounts` into `ClaimCountsDto`, which C does. No new wall-clock timeout, count cap or size constant anywhere.

## 17. Where this meets the review UI

The review screen (UI design §3–§5) shows the facts this document produces. Builder C reads them from the ledger and the trust store into `ClaimReviewItemDto`; builder A must leave them in these shapes:

| fact | produced here | read by C from | shown as (UI design) |
|---|---|---|---|
| the deciding support and the others | `Finding.supporting_quote`, `TwoPass.supported_by`, `TwoPass.also_supported` (§8); the ledger's `ResolvedByEvidence { source_version_id, supporting_quote }` and `TrustAssessment.supports` | `entry.trust().supports`, the latest `Checked` event's support, `ShownPassage`s | `support` (the strongest counted), `also_supported`; the highlighted sentence is `supporting_quote` located in the shown passage |
| each support's level and whether it counted | `Support.trust.level`, `Decision::Sufficient.met_by` / `Insufficient.have[].counted` | `TrustAssessment.decision` | `support.standing`, `support.counts` ("enough" / "not enough on its own" / "not counted: …") |
| why a claim waits | `VerdictKind::InsufficientStanding { bar, have }`, `Missing` (now with `StrongerSourceSilent { level }`, §6.1), `Label::BestAvailable { level }`, `Available { ceiling, considered }` | `TrustAssessment.decision`, `TrustAssessment.available` | `LearningSufficiencyDto.{missing, label, ceiling}` → the one-line reason (UI §4.1) and the "Counts as" line |
| what kind of work a source is | `StandingAssessment.{nature, publisher_kind, work}` under `source-standing-v2` (§4.2) | `source_standing_assessments` by the v2 key (C computes the key from the version's text with `ContentEvidence::of`, the same call `context.rs` makes) | `support.kind` words (UI §4.3); a source with only a v1 row shows as `page` |
| the honest tally | `ClaimCounts { verified, confirmed, general_knowledge, queued, open, removed_by_you, edited_by_you, rewritten }` (§11) | `ClaimCounts::tally` | `ClaimCountsDto`, `LearningClaimTally.rewritten`, the header line (UI §4.4) |
| what research did | `ClaimEvent::Researched { queries, sources_added, passages_found }` (unchanged) and the checks' `evidence` source ids | the ledger | `tried` (UI §5.1): searches, pages read, pages mentioning, relevant pages |
| the claim's sentence and paragraph | unchanged: `ClaimLocation { quote, span }` and the lesson field text | the lesson draft | `presentation` through `lattice_core::shared::text::plain_sentence_context` and `sentences` (C moves `repair/citation.rs::sentences` into `shared/text`; A's confinement (§10.2) then imports it from there) |
| a source the user added | the gate's user rule and §5 rule 2 | `SourceRefDto.added_by` | "a source you added · enough, it states this" |

Two shared helpers cross the builders: `shared/text::sentences` (C creates it; A's `confine` and §9's successor rule use it) and `ContentEvidence::of` (A creates it in lattice-core; C calls it to compute v2 keys for the review's sources). C's stub commit of the DTO shapes comes first (UI design §9.1); A and C both start from it.

The screenshot claim `7cf7155f` is traced through this document's rules in the UI design §8 and is expected to verify by the book alone (`Established` under v2 or the §5.3 ladder, meeting a general claim's `Established` bar), with the standing reach as the backstop. If a live run shows it still waiting, the fault is in §4 or §7, not in the screen.

## 18. Addendum, 2026-10-21 (after the build): a claim's own sentence

Status: design addendum to §9, §10, §12, §13, §14, §16 and §17, written after reading the built code on `ui/claim-review` (`c21d5d48` plus the uncommitted confinement patch). Nothing in this section is built. Where it and an earlier section disagree, this section stands. Paths follow the header's conventions; `core/` is `crates/lattice-core/src/`.

### 18.1 What the built code does, and what follows from it

A claim's `ClaimLocation.quote` is not its sentence. The extractor is given the unit's text cut into *passages*: a cut falls at the first line break after 600 characters, or at 1,400 characters (`inventory.rs:68-98`, the rule at `:75`). The model names a passage by id and `resolve` (`inventory.rs:140-180`) stores that passage's whole text as the claim's `quote`. `observe::locate` (`claim_ledger/observe.rs:112-148`) finds the field that contains it and `ClaimLocation::locate` (`core/domain/learning/claim_review.rs:105-122`) takes `span_of` it. A paragraph of five sentences with four claims gives four claims with one identical 700-character quote. The owner's screenshot card shows exactly that quote.

§9–§12 were written as if the quote were the sentence, and the §14 tests built their fixtures with one-sentence quotes, so none of them met a passage. Read against the passage, five consequences:

1. **Confinement protects passages.** `Protected::of_ledger` (`repair/confine.rs`) collects quotes; `confine_text` marks a sentence protected when it overlaps any protected quote and failed when it overlaps a target quote. A failed claim that shares its passage with any live claim has every sentence of that passage both failed and protected, so every hunk the repair writes trips `on_target && on_protected`, `Err`, `ledger.reject`, one attempt charged; twice, then the user. Passages usually hold several claims, so most failed claims would reach the user unrepaired. The opposite of the goal.
2. **A repair anywhere in a passage reopens every verified claim in it, with a fresh budget.** After a repair the passage text differs, so every claim located by it has a new quote; `new_information` (`core/domain/learning/claim_review/rules.rs:71-76`) says `SentenceChanged`; `carry_on` (`observe.rs:563-643`) appends the pass's `Checked`; and `apply_check` (`core/domain/learning/claim_review/replay.rs:136-205`) does not treat `ResolvedByEvidence` as settled (`is_settled`, `claim_review.rs:299-307`, lists Confirmed, Removed, Edited and GeneralKnowledge only), so a failing check on a verified claim falls through `match &cursor.state { .. _ => first_failure(..) }`: Open again, `MACHINE_ATTEMPTS` restored, and no `Reopened` event, because `reopen_cause` (`rules.rs:162-173`) only reopens a verified claim when its source changes. `reopen_cause` states the intent; `apply_check` does not keep it. This is the mechanism behind "verifying 2 claims became 38" and "it keeps bringing back old claims it already verified".
3. **Identity is lost for the rest.** Re-extraction after a repair rewords statements; with the passage changed, `identities` (`rules.rs:208-280`) finds neither the same quote nor the same statement, and its span fallback `span_of(text, &old.quote)` (`:257`) finds nothing because the old passage is gone. Orphans end `Edited { successor: None, by: Machine }` and their sentences become new claims with new budgets: 62 of the owner's 119.
4. **The successor rule (§9) can never fire.** `heirs` (`claim_ledger/successor.rs`) accepts a claim only when its span lies inside the changed region widened to sentences; a passage span never does. Every repaired claim is recorded `Rewritten` and its sentence starts over.
5. **The review acts on the passage.** `presentation` (`claim_reviews/view/presentation.rs:20-37`) hands `plain_sentence_context` the passage, which "takes them all"; "Remove it" passes `whole_sentence` (`claim_reviews/service.rs:299`, `:458-471`) for any passage that begins a line and ends in a full stop, and `review_pass::apply` (`review_pass.rs:172-183`) would delete the paragraph; a confirmation binds to the passage (`service.rs:272`); the teaching review holds for the user every finding whose passage lies inside a protected quote (`teaching/protected.rs::on_protected`), which is every finding in any paragraph with a live claim.

The builder's patch at `confine.rs:266, :311` (a word-for-word repeat of a failed sentence counts as failed unless it lies inside a protected quote) is right and stays; it met a symptom of the same cause, a failed sentence repeated beyond a 1,400-character cut.

### 18.2 Decision: a claim is located by one sentence, found without a model

**The locator.** `lattice_core::shared::text::claim_sentence` (new file `core/shared/text/claim_sentence.rs`, `pub mod` beside `sentences.rs`):

```rust
/// The one sentence of `text`, among those overlapping `within`, that
/// `statement` restates; `None` when no sentence holds the greater part of
/// the statement's words.
pub fn claim_sentence(text: &str, within: (usize, usize), statement: &str) -> Option<(usize, usize)>;
```

Rules, in order, all deterministic:

1. **Candidates**: the spans of `sentences(text)` (`core/shared/text/sentences.rs`) that overlap `within`. A sentence cut by a passage boundary is a candidate whole. No candidate → `None`.
2. **Terms** of a text: `plain_text` (markdown, link syntax and `[n]` markers removed), `fold_diacritics`, lowercased, split into maximal runs of alphanumerics and `_`, keeping `.`, `,` and `'` when both neighbours are alphanumeric (`3,000`, `2.5`, `e.g`, `don't`); a run shorter than two characters is dropped unless it is digits. No stop list and no language table: a word that carries no information is found by the next rule, in any language.
3. **Weights**: with `S` candidates and `df(t)` the number of candidates holding `t`, `w(t) = ln(1 + S / max(df(t), 1))`. A term in every candidate weighs least; a term in one weighs most. Two terms match when equal, or when both have at least four characters and one is the other plus at most two more (`schema`/`schemas`, `list`/`lists`, `call`/`called`).
4. **Score** of a candidate: the summed weight of the statement's distinct terms it holds. Ties: the candidate holding more of the statement's adjacent term pairs in order, then the earlier one.
5. **Floor**: the winner is the claim's sentence only when its score is at least half the statement's total weight (every statement term counted, absent ones at `ln(1 + S)`). Below it the statement draws on more than one sentence or paraphrases, and the claim keeps the passage, as today.

Worked. "In autumn, **monarch butterflies** migrate up to 3,000 miles [1]. They winter in Mexico." with the statement "Monarch butterflies winter in Mexico": `S` = 2; `in` is in both (ln 2), every other term in one (ln 3); sentence 1 holds monarch, butterflies, in = 2.89; sentence 2 holds winter, in, mexico = 2.89; pairs: sentence 1 has "monarch butterflies" (1), sentence 2 has "winter in" and "in mexico" (2); sentence 2 wins at 57 % of the total and is located. The owner's card: "Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values." is located by the statement that opens the same way; the bullet "**`type`**: Should always be `"function"` [5]." by a statement that `type` is `"function"`.

**Why one sentence and not a run.** Protection narrower than a claim's words costs nothing: a sentence no target names is reverted by `confine_text` whether protected or not (§10.2 rule 2), so protection decides only the rejection rule and the byte-identity check. Protection wider than a claim's words is the whole fault of 18.1. A statement that genuinely spans two sentences is located by the one holding more of it; the other is "neither" (18.5) and comes back byte-identical.

**Why not the extractor.** Asking the model for the sentence changes the inventory schema and so `inventory-section-v1:` (`section_checkpoints.rs:13-24` keys on `INVENTORY_POLICY` and the content), re-extracting every held lesson's sections on first start; small models misquote, so the app would need this locator anyway as the check and the fallback; and the locator's cost is string work over one passage. **No model call is made to locate a claim: not at extraction, repair, review or display.** If the share of located claims measured in 18.7 is low on real lessons, the extractor option is the next step, with this locator verifying what it returns.

**Idempotence.** Applied to a located sentence (`within` = its own span) the locator returns it, or `None` when the one candidate falls under the floor, and every caller keeps the quote on `None`; applied to a quote the field no longer holds, callers keep the quote. The same function therefore serves stored passages, stored sentences and quotes relocated by `inventory/revisions/locations.rs::anchor`.

### 18.3 Where the sentence lives

The passage stays the unit of extraction, checking and checkpointing. Nothing keyed on it changes: `inventory-section-v1:`, `claim-v1:` (`ClaimChecks::key` is unit, statement, passages), `claim-evidence-v2:`, `claim_reuse.rs` (statement and prior quote), `report.rs:55-80` (unit, quote, statement), `receipts.rs::settled_finding`, `learner_view.rs:48-55`, the fidelity audit's passage ids.

The **ledger** holds the sentence. `observe::locate` (`observe.rs:112-148`) gains `statement: Option<&str>`. With `Some`, after finding the field, it calls `claim_sentence(text, within, statement)`, `within` being the first occurrence of the quote in the field (the whole field when the quote is not found), and builds `ClaimLocation { quote: text[run], span: Some(run), .. }`. The span comes from the run, not from `span_of(text, quote)`, so a short sentence that recurs elsewhere still has its one place; `ClaimLocation.span`'s comment changes to say so. With `None` (code examples, `observe.rs:342-363`, whose quote is the code) and when the locator returns `None`, the location is built as today. The call at `observe.rs:334` passes `Some(&finding.statement)`.

One helper for every other reader of a stored quote, so the review and the sync points agree with the pass before the pass has run:

```rust
// content_verification/claim_ledger/located.rs (new; re-exported from content_verification)
/// `location.quote` narrowed to the claim's sentence in `field_text` when the
/// stored quote is wider than one; the quote itself otherwise.
pub(in crate::features::learning) fn located_quote(location: &ClaimLocation, statement: &str, field_text: &str) -> String;
```

| site | today | with `located_quote` |
|---|---|---|
| `claim_reviews/view/presentation.rs:28` | `plain_sentence_context(&text, &location.quote)` | `plain_sentence_context(&text, &located_quote(location, entry.statement(), &text))` |
| `claim_reviews/view.rs:449-456` `ClaimLocationDto.quote`/`span`; `claim_reviews/dto/history.rs:126` | stored quote | located quote and its span |
| `claim_reviews/service.rs:272` Confirm | `HumanBinding::new(&entry.location().quote, ..)` | bound to the located quote |
| `service.rs:285-295` Edit | `before` must occur once | `before` is accepted when it is the located raw sentence or its `plain_text` (the UI sends `presentation.sentence`, which is plain); the stored `UserAction::Edit.before` is always the raw located sentence |
| `service.rs:299` Remove | `whole_sentence(&text, &entry.location().quote)` | on the located quote |
| `service.rs:343` undo | `!text.contains(&entry.location().quote)` | located quote |
| `review_pass.rs:115` `Change::Remove { quote }` | stored quote | located quote (`changes` has the candidate) |
| `audit/ledger.rs:63` `AuditClaim.quote` | stored quote | unchanged: after the first pass it is the sentence (18.7 states the one-time cost) |

### 18.4 The ledger across passes (replaces I1 of §12.1 and amends §9)

**(i) A location that narrows is not a changed sentence.** `carry_on`'s rule at `observe.rs:584-598` ("a confirmation is bound to its sentence, not to the quote an extraction picked: while the bound sentence is still in its field, nothing about it changed") drops its `Confirmed` condition and holds for every state: `SentenceChanged` stands only when the field no longer contains the stored quote. A stored passage that still stands byte-identical, re-located to one of its sentences, appends one `Extracted` event (`observe.rs:600-609`, which already fires when the location differs; `core/.../ledger.rs:412-419` makes it the entry's location) and nothing else: no `Reopened`, no `Checked`. The doc comment on `NewInformation::SentenceChanged` says so. The one risk (a short sentence that recurs in its field and was rewritten in one place) is closed by confinement: `overlaps_any` protects every occurrence of a protected quote, so a protected sentence cannot be rewritten anywhere in its field.

**(ii) A verified claim is not re-judged on reselected evidence.** `new_information` (`rules.rs`): when `prev.state()` is `ResolvedByEvidence` and the basis fingerprint differs, return `None` instead of `EvidenceChanged`, without going on to the gate comparison (the gate weighed other evidence; `DecisionChanged` still applies on the same fingerprint, as source provenance §18.7 intends). `SentenceChanged`, `SourceChanged`, `CheckerChanged`, `UserSource` and `UserHint` are unchanged. This is what `reopen_cause` already promises for a verified claim (only its source changing reopens it) and what `apply_check`'s fall-through broke. The re-extraction's statement is still adopted by `Extracted` as today, so `receipts.rs::settled_finding` and the audit key by the current wording.

With (i) and (ii), I1 reads: *every claim not in `targets` whose sentence is unchanged keeps its id, state and basis and receives no `Checked`*; confinement guarantees the unchanged sentence. A claim the locator could not narrow keeps the status quo of 18.1 (2) and (3); it is the residue `K` measured in 18.7.

**(iii) Identity.** `identities` (`rules.rs:245-270`): the old span is `span_of(text, &old.quote).or(old.location().span)`; nothing else changes. With sentence quotes, `same_quote` matches every unchanged sentence whatever the extractor reworded.

**(iv) Successor.** `heirs` (`successor.rs`): `qualifies` accepts a span that *overlaps* the sentence region (today: lies inside), so a claim the locator could not narrow that sits over the rewritten text can still continue the repaired claim or inherit from it, and never starts a budget of its own (§9.1). `RepairRecord.before` (`claim_ledger.rs:384`) is now the sentence, which is what `shared_terms` (`lessons/reference_collection/subject.rs:44`, not `repair/citation.rs` as §9 said) was meant to compare. Nothing else in §9 changes. **The claim id does not change form and no id is rewritten.**

**(v) Old histories.** Every event written by older code replays as before: an `Extracted` with a passage location is the stored state until the next pass narrows it with another `Extracted`; the one new variant, `RepairOutcome::NotAttempted { reason }` (18.5), appears only in new events. `ClaimEvent` is `#[serde(tag = "kind")]` and `RepairOutcome` `#[serde(rename_all = "snake_case")]`; adding a variant reads old JSON unchanged. No migration.

### 18.5 Confinement with sentences (replaces §10.1 and amends §10.2)

**Protected** (`Protected::of_ledger`, code unchanged): the stored quotes of every live claim in the unit that is not a target: now sentences, or a passage where the locator declined. **Targets** (`Protected::of_claims`): the targets' stored quotes. `confine_text` is unchanged, including the repeat rule.

**A sentence holding both a failed and a protected claim** is protected. The machine does not rewrite words a verified, confirmed, accepted or waiting claim stands on, and it does not spend attempts and model calls finding that out:

- In the loop at `repair_loop.rs:291` (`Decision::Repair(targets)`), before `repair_context` and `begin_repairs`, `ClaimLedger::withhold_shared(&mut self, targets: &[ClaimId]) -> Vec<ClaimId>` (new, `claim_ledger.rs` beside `begin_repairs`) returns the targets that are editable and records the rest. A target is **editable** when at least one sentence of its stored quote (`sentences(quote)`: one for a located claim, several for a passage) neither contains nor is contained in a protected quote of the same unit and field (`Protected::of_ledger(self, targets)` filtered to the field). For each target that is not editable it appends `RepairAttempted { attempt: <as begin_repairs computes it>, escalated: false, before: quote, after: None, outcome: RepairOutcome::NotAttempted { reason } }`, reason "This sentence also states something that is verified or waiting for you; rewriting it would change that too." If no target is left, the loop goes back to `decide()`, which returns `NeedsReview`.
- `RepairOutcome::NotAttempted { reason }` (`core/domain/learning/claim_review.rs:459-469`); `apply_repair` (`core/.../ledger.rs`, reached from `:477-482`) makes it `queued(kind, reason)` at once: no attempt is charged, none is left to the machine, `repair_pending` is not set. `may_repair` is false afterwards; `ClaimCounts` counts it as `queued`; the tally shows it as waiting for the user, never as rewritten.
- Order: `decide()` runs after this pass's research (`repair_loop.rs:166`, then `:255`), so a withheld claim has had its research; a later check on new information can still verify it (`apply_check` on `Queued` with `Supported` gives `ResolvedByEvidence`).
- `confine_text`'s rejection (`on_target && on_protected`) stays as the second line, for a repair that strays onto a protected sentence that is not the target's own.

**A sentence holding neither** (no live claim located to it) is unchanged territory: a hunk on it is reverted, as §10.2 rule 2 says, except an insertion adjacent to an allowed hunk. The repair's job is the failed sentence; the teaching review owns the rest with its own targets (§10.3, code unchanged, which now holds for the user only the findings that sit on a claim's own sentence).

**What the repair model is told.** `repair_context` (`repair_loop.rs:465-520`) adds to each `failedClaims` entry `"sentence": <the target's stored quote>`, the exact lesson text it may change; `section_context` (`repair.rs:111`) passes the entries through as it does. The cost is one sentence per failed claim. `CONFINEMENT` (`confine.rs`) reads: "Edit only each failed claim's `sentence`; every other sentence is returned byte-identical and any change to it is discarded." `repair_context` stops writing `"protected"` (`:518`); `Protected::from_context` is deleted; `candidate` (`repair.rs:357`) and `repair_sections` (`repair.rs:487-492`) take `Protected::of_ledger(ledger, &targets)` from the synced ledger, which is also more right: a claim the user removed since the repair was saved need not be protected. §10.1's "carried in the context so a resumed repair has it" is withdrawn.

### 18.6 The review screen (amends the last row of §17 and UI design §3.3, §3.5)

The screen shares the locator through `located_quote` (18.3): `presentation.sentence` is the claim's sentence even for a claim the ledger still locates by a passage, so the owner's card shows "Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values." with the rest of the paragraph in `context_before` and `context_after`. Edit replaces that sentence; Remove deletes that sentence and refuses, as today, when the located quote is not a whole sentence. The support highlight (`support.quote_span`) is the judge's quotation in the evidence passage and is untouched.

One copy-deck row for builder B (UI design §4.1): state `queued` whose latest `RepairAttempted` is `NotAttempted` → chip "Waiting for you", reason "This sentence also states something already verified, so the machine left it to you.", fix "Edit the sentence · Accept".

### 18.7 First start on the owner's data (amends §13)

Nothing runs at app start. Opening the review before Resume: every card shows one sentence (18.6); Edit and Remove act on it. On Resume or Retry of `db8397ff`:

| what | count (copy) | effect |
|---|---|---|
| inventory, receipts, pages | as §13 | restored; this addendum adds no extraction or judge call |
| live claims (12 verified, 33 queued, 2 open) | 47 | one `Extracted` event each with its sentence where the locator places it (the rest keep their passage); no `Reopened`, no `Checked`; states and bases unchanged |
| the 2 open claims | 2 | a sentence of its own: repaired inside it, the other sentences of its paragraph byte-identical; shared with a verified or waiting claim: `NotAttempted`, waits for the user with the reason. The next pass re-extracts only the repaired sections (one call each); their verified claims keep ids and states |
| Verifier audit | only if the course opted in (`repair_loop.rs:265`) | `audit/ledger.rs:32-37` hashes statement and quote, so narrowed quotes give new wording hashes and the verified, confirmed and accepted claims are audited once more, batched as today. Not opted in: nothing |
| `claim-ledger-v1:pending` | one or none | a saved `PendingRepair` resumes with `protected` recomputed from the ledger |
| tally, checkpoints, schema | — | unchanged; no migration |

The pass logs one line, `claim.locate: N of M claims located to one sentence; K kept their passage`, and `ClaimLocationDto.span` says which. The **coordinator** records N, M and K from the run on the copy here, beside the §13 counts. If K is a large share, 18.2's extractor option is next.

### 18.8 Tests (adds to §14.1)

| file | test | asserts |
|---|---|---|
| `core/shared/text/claim_sentence.rs` (tests module) | `a_statement_finds_its_sentence_not_the_one_naming_its_subject` | the monarch paragraph of 18.2: "Monarch butterflies winter in Mexico." → "They winter in Mexico." |
| same | `a_lead_line_and_a_bullet_are_located_apart` | the owner's "five key fields" passage: the opening statement → the opening sentence; the `type` statement → the `type` bullet, markdown and `[5]` inside |
| same | `a_history_statement_drawn_from_three_sentences_keeps_its_passage` | a Westphalia paragraph; a statement combining three sentences → `None` |
| same | `a_statute_subsection_is_located_in_a_law_passage` | a numbered statute: subsection (2) is found for a statement restating it, not (1) or (3) |
| same | `inflections_separators_and_diacritics_match_as_the_passage_writes_them` | `schemas`/`schema`, `lists`/`list`, `3,000`, `2.5`, `café`/`cafe` |
| same | `a_sentence_cut_by_a_passage_boundary_is_taken_whole_and_locating_is_idempotent` | `within` ends mid-sentence → the whole sentence; locating that sentence again returns it |
| `claim_ledger_tests.rs` | `a_location_that_narrows_in_unchanged_text_changes_nothing_but_the_location` | an entry stored with a passage quote, as older code wrote it, the field unchanged: after `observe` one `Extracted`, no `Reopened`, no `Checked`; state verified, basis unchanged |
| same | `a_verified_claim_is_not_rejudged_on_reselected_evidence_while_its_sentence_stands` | same sentence, reworded statement, a failing check with another fingerprint → `Extracted` only, state verified; the sentence changed → the check applies |
| same | `a_failed_claim_sharing_its_sentence_with_a_verified_one_waits_without_a_repair_call` | `withhold_shared` → `RepairAttempted { NotAttempted }`, `Queued`, zero model calls, `may_repair` false, the verified claim untouched; a target with one unshared sentence stays a target |
| `successor.rs` tests | `a_repaired_sentence_among_verified_ones_continues_its_claim_alone` | a passage of three located claims V1, F, V2; F rewritten; F's id continues in the new sentence; V1 and V2 are neither successors nor children |
| `repair/confine_tests.rs` | `a_failed_sentence_is_repaired_beside_verified_ones_in_the_same_passage` | **the case that broke**: one passage (a biology or law paragraph), claims V1, F, V2 located through `observe::locate` with the real passage text; the repair rewrites F → accepted, V1 and V2 byte-identical. The same with V1 unlocated (passage quote) → rejected, documenting the fallback |
| `claim_review_flow_tests.rs` | `a_repair_pass_touches_only_the_failed_sentences` (rewritten, `:614-650`) | the three claims come from **one inventory passage** (body under 600 characters) as `inputs`/`resolve` shape them, not hand-made sentence quotes; after the pass the two verified keep id, state and basis with no `Checked`; the target continues with one attempt fewer; one model check |
| `teaching/loop_tests.rs` | `a_finding_on_a_sentence_no_claim_stands_on_is_repaired_inside_a_paragraph_with_verified_claims` | held today; repaired now; the claims' sentences byte-identical |
| `claim_reviews/dto_tests.rs` | `the_review_shows_the_claims_own_sentence_for_a_location_stored_as_a_passage` | the screenshot claim's card: `sentence` is its first sentence, `context_after` the rest |
| `claim_reviews` service tests | `remove_and_edit_act_on_the_located_sentence` | Remove deletes one sentence of a passage-located claim; Edit with the plain sentence replaces the raw one |
| `core/domain/learning/claim_review` ledger tests | `histories_written_before_sentence_locations_replay_unchanged` | a JSON fixture of events with passage locations and no `NotAttempted` replays to the same states |

Course eval: `3-music-theory-production` `m1.l1` (§14.2) reports the log line's N, M and K beside `lesson.rounds.repair`; its acceptance keeps `quotes_unmatched = 0` and no verified claim re-checked.

### 18.9 Build steps (amends §16 steps 2, 6 and 7) and what stays outside

1. **lattice-core.** `shared/text/claim_sentence.rs` with its tests; `RepairOutcome::NotAttempted` and `apply_repair`; `new_information`'s rule (ii); `identities`' span fallback. `cargo check -p lattice-core`.
2. **Ledger.** `observe::locate` with the statement; `claim_ledger/located.rs`; `carry_on` rule (i); `withhold_shared` and its call at `repair_loop.rs:291`; `heirs`' overlap; the log line. `cargo check -p lattice-desktop`.
3. **Repairs.** `failedClaims[].sentence`; `CONFINEMENT`; delete `Protected::from_context` and the `"protected"` key; `Protected::of_ledger` at `repair.rs:357` and `:487-492`.
4. **Review.** The `located_quote` sites of 18.3; the copy row to builder B.
5. **Tests** of 18.8, the rewritten flow test among them. The builder runs `cargo check` only; the coordinator runs the suites once, then the first-start run on a copy (18.7) and fills in N, M and K.

Not touched: the extractor prompt and schema; `inventory.rs::passages` and its cut rule; every checkpoint key; `confine_text`'s algorithm; `MACHINE_ATTEMPTS`; the judge; `text_edits`; anything in §4–§8. The only constants added are the locator's two rules, the half floor and the two-character inflection, both named in 18.2 and tested. No model call, timeout or count cap is added anywhere.

The next optimisation, not in this addendum: `claim_reuse.rs` reuses receipts by statement, so a reworded statement over an unchanged sentence still costs a judge call in the checker (the ledger ignores its result under (ii)). Reusing by located sentence would make a repaired section's re-check free for every sentence it did not change.
