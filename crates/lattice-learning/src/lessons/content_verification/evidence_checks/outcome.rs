//! What the readings of a claim leave on its finding (course evidence index
//! §5.1). Support in a claim's own citation stands, with any contradiction a
//! page of the course gave carried as a contrary passage for the overturn
//! rule. Support in a page of the course alone means the citation is wrong:
//! the finding names that page's passage (`supported_by`) and repair moves the
//! citation. A contradiction stands when nothing supports the claim; when both
//! support it, both supports travel together.
use super::walk::Reach;
use super::*;
use lattice_core::application::services::claim_verification::PartKind;

/// Which check decided a claim. The comparison with its own citation comes
/// first in a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Pass {
    /// The claim against the passages its own section cites.
    Attribution,
    /// The claim against the course's first page for it, by relevance.
    Corroboration,
}

/// Where a page of the course gave support that the section's citations did
/// not: a passage of the course, by its source version and bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SupportedElsewhere {
    pub(crate) source_id: String,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    /// The source versions the claim's section cites, none of which
    /// supported it; empty when the section cites none.
    #[serde(default)]
    pub(crate) cited: Vec<String>,
}

/// A passage found saying otherwise about a claim another reading supported.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ContraryPassage {
    pub(crate) passage: EvidencePassage,
    /// The words the judge cited for the contradiction: the passage's own
    /// text when it named no passage.
    pub(crate) quote: String,
}

/// A passage that also supports a claim, beside the one its quotation names:
/// what the gate weighs with it (claim sources design §8).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SupportingPassage {
    pub(crate) passage: EvidencePassage,
    /// The words the judge cited for the support.
    pub(crate) quote: String,
}

/// One part of a claim as the part-wise judge read it against the claim's own
/// citation (claim classes design §3.3): what the learner is told is common
/// knowledge, and what the acceptance run counts as bridged, are read from
/// these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CheckedPart {
    pub(crate) part: String,
    pub(crate) kind: PartKind,
    /// The source version that states the part; none for a part no passage
    /// states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_id: Option<String>,
}

/// How a claim was read, on its finding: the check that decided it, where its
/// support was found when its citation gave none, and what else was found.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TwoPass {
    /// The check whose page decided the claim; `None` for a claim no page
    /// decided (its quotation settled it, or the user did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pass: Option<Pass>,
    /// The page of the course that decided it, the first being 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) page: Option<usize>,
    /// Where the course was read from when the page decided the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reach: Option<Reach>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supported_by: Option<SupportedElsewhere>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) contrary: Vec<ContraryPassage>,
    /// Other passages that support the claim, one per source version: the
    /// support of the other reading.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) also_supported: Vec<SupportingPassage>,
    /// The parts the part-wise judge read the claim as: every claim checked
    /// against the passages its sentence cites.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) parts: Vec<CheckedPart>,
}

/// A claim's finding when no page of the course was read for it: the passages
/// its section cites decided it alone.
pub(in crate::lessons::content_verification) fn alone(
    mut finding: Finding,
) -> Finding {
    if matches!(
        finding.verdict,
        ClaimVerdict::Supported | ClaimVerdict::Contradicted
    ) {
        finding.two_pass.pass = Some(Pass::Attribution);
    }
    finding
}

/// The passage of a supported `finding` that holds its quotation.
fn supporting_passage(finding: &Finding) -> Option<&EvidencePassage> {
    let quote = finding.supporting_quote.as_deref()?;
    finding
        .evidence
        .iter()
        .find(|passage| passage.retrieval_kind != "execution" && passage.text.contains(quote))
}

/// The same bytes of the same source version.
pub(crate) fn same_passage(
    a: &EvidencePassage,
    b: &EvidencePassage,
) -> bool {
    a.source_id == b.source_id && a.start_byte == b.start_byte && a.end_byte == b.end_byte
}

/// The finding of a page of the course read for a claim, `at` that page
/// (where the course was read from, and which page): the page decided it, and
/// a page that supports it names where, since the section's citations
/// (`cited`) did not.
pub(in crate::lessons::content_verification) fn page_finding(
    mut finding: Finding,
    at: (Option<Reach>, usize),
    cited: &[String],
) -> Finding {
    if !matches!(
        finding.verdict,
        ClaimVerdict::Supported | ClaimVerdict::Contradicted
    ) {
        return finding;
    }
    finding.two_pass.pass = Some(Pass::Corroboration);
    finding.two_pass.reach = at.0;
    finding.two_pass.page = Some(at.1);
    if finding.verdict == ClaimVerdict::Supported {
        finding.two_pass.supported_by =
            supporting_passage(&finding).map(|passage| SupportedElsewhere {
                source_id: passage.source_id.clone(),
                start_byte: passage.start_byte,
                end_byte: passage.end_byte,
                cited: cited.to_vec(),
            });
    }
    finding
}
