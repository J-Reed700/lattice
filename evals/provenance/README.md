# Source provenance and trust

The labelled data that the source-trust rules are tested and calibrated against. The rules are in `crates/lattice-core/src/domain/provenance/`, and the design is in [`docs/design/2026-10-18-source-provenance-and-trust.md`](../../docs/design/2026-10-18-source-provenance-and-trust.md) (cited below as §n). Every file here is read by `cargo test -p lattice-core --lib provenance`, so if a rule or a label drifts, a test fails.

| File | Items | What it holds | Test |
| --- | --- | --- | --- |
| `sources-v1.jsonl` | 120 | Sources in eight areas and eight languages. Each has the signals capture records for it, the subject of a claim, and the expected standing, ownership and level. | `fixture_tests::every_labelled_source_*` |
| `sources-v2.jsonl` | 20 | The claim sources design's rows (`docs/design/2026-10-21-claim-official-sources.md` §14.1): each adds the `content` evidence the `source-standing-v2` judgement sees and is levelled by the §5 rules (the owner's own account, the user's own sources, the ladder for an unidentified publisher). A biology set with no vendor at all, plus history, medicine, law, science, cooking and news. | `fixture_tests::every_v2_source_gets_its_labelled_level_from_its_judgement_and_content` |
| `sufficiency-cases-v2.jsonl` | 8 | Best-available cases (§6.1 of the claim sources design): each may carry `available: {ceiling, considered}`, what the course has on the claim's subject | `fixture_tests::every_v2_sufficiency_case_decides_as_labelled` |
| `independence-pairs-v1.jsonl` | 219 | Text pairs labelled `copy`, `contains` or `independent` | `fixture_tests::the_copy_thresholds_are_the_sweeps_f1_optimum` |
| `sufficiency-cases-v1.jsonl` | 50 | Claim profiles with levelled supports and contrary passages, and the decision, the contradiction outcome and the overturn expected for each (§6, §9, §10, §11, §13) | `fixture_tests::every_sufficiency_case_decides_as_labelled` |
| `chain-cases-v1.jsonl` | 12 | Passages with links, and the citation candidates expected in rank order (§8) | `fixture_tests::every_chain_case_ranks_its_candidates_as_labelled` |
| `injection-cases-v1.jsonl` | 30 | Metadata text addressed to a reader or a model, plus self-description fields, each with the expected detection (§14) | `fixture_tests::every_injection_and_self_description_case_is_detected_as_labelled` |
| `results/` | | One JSON per run. `independence-thresholds.json` holds the threshold sweep. | |

## Where the data comes from

All of it is synthetic, and it was written by hand for this eval on 2026-10-18 by the P1 build agent (Claude Opus). No second labeller has reviewed it. Follow the grounding eval's rule: before any number from this set is quoted as a release claim, a person should re-label a blind sample and publish the agreement rate.

- **Sources.** The rows are built by `build_sources.py`. A `real` row models the signals that the `page-signals-v1` reader would emit for a real public page. The values come from the author's knowledge of that page, so they were written, not fetched. A `perturbed` row is altered or invented: impostors that call themselves official, content farms, injection attempts, mirrors, a bare IP address, library documents and pasted text. Hosts that are not public sites use reserved `example` names. The rows include archives (Wayback, archive.ph full and short links), owner-hosted forums and wikis, vendor marketing pages, government PDFs and verified mirrors (`mirror_of`).
- **Pairs.** `build_independence_pairs.py` builds every pair from the hand-written texts in `corpus/independence-base-v1.json`. Each pair is made by a named construction, and that construction sets its label (see "Labelling rule"). The script uses a fixed seed and no clock, so a rerun produces the same file.
- **Cases.** The sufficiency, chain and injection cases were written directly as JSONL. Each case's `rule` or `note` names the section or behaviour it exercises.

## Labelling rule

**Sources.** Standing, editorial process, nature and flags describe the publisher as the signals identify it. The page's prose is never used. Ownership is judged for the row's subject. `owner` means the origin publishes on its own behalf about its own subject. `affiliated` means the owner's origin but a community area, a reseller or a partner. The expected level follows from those labels by the §5 procedure. Any disagreement between a hand-assigned level and the procedure is a finding about the procedure, and is settled in the design before a label changes.

**Pairs.**

| label | construction |
| --- | --- |
| `copy` | The same text: verbatim, reformatted (headings, whitespace, paragraph breaks), with a short site boilerplate added, or with word edits on at most 4% of the words. For scripts without word breaks, 2% character edits. |
| `contains` | One text holds the other whole, inside commentary at least 60% of the original's length (syndication), or holds a contiguous excerpt of it that is 30-60% of the larger text. |
| `independent` | Different texts. Texts on one topic written independently, texts sharing one quoted sentence of a third source, a text and its translation (translation is not copying here), different texts in one long site template, or texts on different topics. |

The pairs are built to be separable: the sweep finds a range of thresholds at F1 = 1.0, and the policy takes the centre of that range. Real captured pages will be harder. Pairs from live courses should be added (as `independence-pairs-v2`) once capture records relations.

## Format

- `sources-v1.jsonl`: `{ id, area, language, subject: {name, kind, aliases, classes}, added_by, signals: ProvenanceSignals, expected: {standing, publisher_kind, editorial, nature, flags, relation, level}, mirror_of?: {id, relation, containment}, rationale, labeller, origin: real|perturbed }`. `signals` is the serde form of `ProvenanceSignals`, which is what `source_provenance.signals_json` stores.
- `sources-v2.jsonl`: the `sources-v1.jsonl` format plus an optional `content`, the serde form of `ContentEvidence` (`domain/provenance/content_evidence.rs`); `expected` is the v2 judgement and the level. Written by hand as JSONL by builder A on 2026-10-21 (the same labelling rule; the content evidence is part of what the labels describe). A second labeller is still owed.
- `sufficiency-cases-v2.jsonl`: the `sufficiency-cases-v1.jsonl` format plus an optional `available: {ceiling, considered}`.
- `independence-pairs-v1.jsonl`: `{ id, label, construction, a, b, note }`.
- `sufficiency-cases-v1.jsonl`: `{ id, rule, profile: {kind, subject?, temporal: {scope, version?, date?}, measurement_origin?}, supports: [{id, cluster?, level, passage?, published?, mentions_version?, added_by?, cited_urls?, owner_hints?, mirror?}], contrary?: [{id, cluster?, level, published?}], newest_primary?: {id, published}, verdicts?: [{level, kind, held, count}], expected: {decision, overturn, label?, met_by?, bar?, missing?, next?, counted?, temporal?} }`. Every source in the cases was read on 2026-10-18, and the test computes temporal validity from the dates.
- `chain-cases-v1.jsonl`: `{ id, passage_markdown, span_text, subject, owner_domains, authoritative_domains, expected_candidates_in_order, note }`.
- `injection-cases-v1.jsonl`: `{ id, field, text, expected_injection, note }` or `{ id, self_description_fields, expected_tokens }`.

## How to run

The tests run with `cargo test -p lattice-core --lib provenance`. Run cargo through the shared lock, as every Lattice build does: `CARGO_INCREMENTAL=0 lockf -k /Users/josh/Code/.lattice-cargo.lock cargo …`.

After changing the pairs, the corpus or the shingling:

1. `python3 -I evals/provenance/build_independence_pairs.py`
2. `cargo test -p lattice-core --lib provenance::fixture_tests::record_independence_thresholds -- --ignored` rewrites `results/independence-thresholds.json`.
3. Set `POLICY.resemblance_copy` and `POLICY.containment_copy` (`domain/provenance/policy.rs`) to the recorded thresholds. Until they match, `the_copy_thresholds_are_the_sweeps_f1_optimum` fails.

After changing a source row, run `python3 -I evals/provenance/build_sources.py`.

**Live judgement eval (not built yet).** `live_provenance_eval.rs` (§21, track P7) will score the three model judgements against `sources-v1` and `chain-cases-v1` on the local 9B sidecar and the remote 27B, and append the results to `results/`. The recipe for running the local sidecar is in the eval-runs note in the project memory.

## Results history

| Date | Policy | What | Result |
| --- | --- | --- | --- |
| 2026-10-18 | source-trust-v1 | Threshold sweep, 219 pairs | resemblance_copy 0.64 (F1 1.0 for 0.60-0.69); containment_copy 0.60 (F1 1.0 for 0.40-0.81) |
| 2026-10-18 | source-trust-v1 | §5 procedure on 120 labelled sources | 120/120 levels as labelled; 115/115 web origins match the compiled suffix list |
| 2026-10-18 | source-trust-v1 | P7 offline run on the integrated branch (`results/2026-10-18-offline-p7.json`) | sources 120/120 levels, 115/115 origins; pairs sweep unchanged (0.64 / 0.60, F1 1.0); sufficiency 50/50; chains 12/12; injection 30/30; the gate's sufficiency fixture 8/8; function-calling lesson 5/5 primary-backed claims pass with one check. Live judgement evals not run (no llama-server) |
