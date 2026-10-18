//! The stated parameters of source trust, each with its derivation. Every
//! number the provenance rules compare against is a field here; none is an
//! unexplained literal at its use.

/// The trust policy's version: stamped on outcomes, receipts and the
/// calibration display. Bump it when a rule or parameter changes.
pub const TRUST_POLICY_VERSION: &str = "source-trust-v1";

/// The parameters (see `POLICY` for their values and derivations).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    /// Resemblance (Jaccard of shingle sets) at or above which two texts are
    /// copies of each other.
    pub resemblance_copy: f64,
    /// Containment at or above which one text contains a copy of another.
    pub containment_copy: f64,
    /// Words per shingle.
    pub shingle_words: usize,
    /// Characters per shingle for scripts without word breaks.
    pub shingle_chars: usize,
    /// Below this share of whitespace among a text's characters, the text is
    /// shingled by characters.
    pub word_break_ratio: f64,
    /// The reliability a level must have, for a kind of claim, to be
    /// accepted as a claim's only support.
    pub accept_reliability: f64,
    /// The posterior probability at which calibration treats a measured
    /// reliability as above or below `accept_reliability` and moves a bar.
    pub bar_move_probability: f64,
    /// Pseudo-verdicts the static bar is worth in each calibration cell.
    pub prior_strength: f64,
    pub version: &'static str,
}

/// The policy in force. Each value's derivation:
///
/// - `resemblance_copy`, `containment_copy`: derived from the labelled pairs
///   in `evals/provenance/independence-pairs-v1.jsonl` by the threshold
///   sweep (`fixture_tests::the_copy_thresholds_are_the_sweeps_f1_optimum`):
///   the centre of the widest run of 0.01-grid thresholds with the highest
///   F1 for "copy" (resemblance) and for "copy or contains" (containment,
///   the larger of the two directions). The test fails if a constant
///   drifts from the data, and `results/independence-thresholds.json`
///   records the sweep.
/// - `shingle_words = 5`: five-word shingles are the width Broder's
///   resemblance work used for near-duplicate pages; shorter shingles match
///   common phrases between independent texts, longer ones let a single
///   edited word hide a copy.
/// - `shingle_chars = 5`: in scripts without word breaks five characters
///   span roughly two to three words, the same reach as five words.
/// - `word_break_ratio = 0.1`: space-separated scripts run at about one
///   whitespace per five to seven characters (15-20%); Chinese, Japanese
///   and Thai text runs near zero. One in ten sits between the two with a
///   wide margin either side.
/// - `accept_reliability = 0.9`: a level is accepted as a claim's only
///   support when nine claims in ten that it alone supports hold. A
///   published wrong claim costs the learner a misconception and the work
///   of unlearning it; a queued right claim costs the user one
///   confirmation. Accepting at reliability θ is the cheaper choice when
///   θ·(cost of a queue) > (1 − θ)·(cost of an error), so 0.9 states that
///   an error costs nine confirmations.
/// - `bar_move_probability = 0.9`: the posterior probability at which a
///   measured reliability is taken to be above (relax) or below (tighten)
///   `accept_reliability`. One number for both directions, so the bars move
///   on equally strong evidence either way; shown in Settings.
/// - `prior_strength = 16.0`: each calibration cell starts from a Beta
///   prior with the weight of sixteen verdicts and its median at
///   `accept_reliability`, so before any verdict a level is as likely above
///   the target as below it and the static table stands. Sixteen is the
///   smallest whole weight at which relaxing a bar from an empty cell takes
///   as many straight held verdicts (21) as the uniform Beta(1, 1) prior the
///   design started from; unlike the uniform prior it does not tighten an
///   accepted level before any verdict. Tightening an empty cell takes two
///   straight failures: two misses in a row are unlikely (under 1%) from
///   claims that hold nine times in ten. The calibration tests derive both
///   counts and pin this value.
pub const POLICY: Policy = Policy {
    resemblance_copy: 0.65,
    containment_copy: 0.65,
    shingle_words: 5,
    shingle_chars: 5,
    word_break_ratio: 0.1,
    accept_reliability: 0.9,
    bar_move_probability: 0.9,
    prior_strength: 16.0,
    version: TRUST_POLICY_VERSION,
};
