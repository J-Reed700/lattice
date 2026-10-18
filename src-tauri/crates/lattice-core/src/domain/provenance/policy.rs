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
