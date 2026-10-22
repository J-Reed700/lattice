//! What a claim's two passes decide together (course evidence index §5.1).
//!
//! Pass A says whether the section's own citation supports the claim; pass B
//! says whether the course does. Support in A stands, and a contradiction B
//! found beside it travels with the finding as a contrary passage, for the
//! provenance gate's overturn rule to weigh. Support in B alone means the
//! claim stands and its citation is wrong: the finding names the passage B
//! found (`supported_by`), the ledger records it as supported elsewhere, and
//! the section repair rewrites the citation to it. A contradiction stands
//! when no pass supports the claim; otherwise the claim is unresolved, with
//! everything both passes compared. When both passes support it, both
//! supports travel with the finding (`also_supported`), so the gate weighs
//! two origins together (claim sources design §8); the standing reach adds
//! what the course's strongest sources say ([`with_standing`], §7.5).
use super::walk::Reach;
use super::*;

/// Which of a claim's checks decided it. Pass A's comparison comes first in
/// a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::features::learning) enum Pass {
    /// The claim against the passages its own section cites.
    Attribution,
    /// The claim against the course, page by page by relevance.
    Corroboration,
}

/// Where pass B found support that the section's citations did not give: a
/// passage of the course, by its source version and bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::features::learning) struct SupportedElsewhere {
    pub(in crate::features::learning) source_id: String,
    pub(in crate::features::learning) start_byte: usize,
    pub(in crate::features::learning) end_byte: usize,
    /// The source versions the claim's section cites, none of which
    /// supported it; empty when the section cites none.
    #[serde(default)]
    pub(in crate::features::learning) cited: Vec<String>,
}

/// A passage one pass found saying otherwise about a claim the other pass
/// supported.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::features::learning) struct ContraryPassage {
    pub(in crate::features::learning) passage: EvidencePassage,
    /// The words the judge cited for the contradiction: the passage's own
    /// text when it named no passage.
    pub(in crate::features::learning) quote: String,
}

/// A passage that also supports a claim, beside the one its quotation names:
/// what the gate weighs with it (claim sources design §8).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::features::learning) struct SupportingPassage {
    pub(in crate::features::learning) passage: EvidencePassage,
    /// The words the judge cited for the support.
    pub(in crate::features::learning) quote: String,
}

/// What the two passes made of one claim, on its finding.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(in crate::features::learning) struct TwoPass {
    /// The pass whose page decided the claim; `None` for a claim no page
    /// decided (its quotation settled it, or the user did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::features::learning) pass: Option<Pass>,
    /// The pass B page that decided it, the first being 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::features::learning) page: Option<usize>,
    /// Where pass B was reading when its page decided the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::features::learning) reach: Option<Reach>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::features::learning) supported_by: Option<SupportedElsewhere>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::features::learning) contrary: Vec<ContraryPassage>,
    /// Other passages that support the claim, one per source version: the
    /// other pass's support, or the support the standing reach moved aside.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::features::learning) also_supported: Vec<SupportingPassage>,
}

/// The key of a claim's whole decision: its pass A comparison, when it has
/// one, and pass B's first page. A later pass whose two comparisons are the
/// same restores the decision, deeper pages included, without reading them.
pub(super) fn decision_key(attribution: Option<&str>, corroboration: &str) -> String {
    digest(&json!({"attribution": attribution, "corroboration": corroboration}).to_string())
}

/// The passages of a contradicted `finding` that say otherwise: those
/// holding the quotation the judge cited, or every passage it compared when
/// it cited none.
fn contrary_of(finding: &Finding) -> Vec<ContraryPassage> {
    let compared = || {
        finding
            .evidence
            .iter()
            .filter(|passage| passage.retrieval_kind != "execution")
    };
    let quote = finding.supporting_quote.clone().unwrap_or_default();
    let named: Vec<ContraryPassage> = compared()
        .filter(|passage| !quote.is_empty() && passage.text.contains(&quote))
        .map(|passage| ContraryPassage {
            passage: passage.clone(),
            quote: quote.clone(),
        })
        .collect();
    if !named.is_empty() {
        return named;
    }
    compared()
        .map(|passage| ContraryPassage {
            passage: passage.clone(),
            quote: passage.text.clone(),
        })
        .collect()
}

/// The passage of a supported `finding` that holds its quotation.
fn supporting_passage(finding: &Finding) -> Option<&EvidencePassage> {
    let quote = finding.supporting_quote.as_deref()?;
    finding
        .evidence
        .iter()
        .find(|passage| passage.retrieval_kind != "execution" && passage.text.contains(quote))
}

/// The passage of a supported `finding` with its quotation, as another
/// support of a claim.
fn supporting_of(finding: &Finding) -> Option<SupportingPassage> {
    Some(SupportingPassage {
        passage: supporting_passage(finding)?.clone(),
        quote: finding.supporting_quote.clone()?,
    })
}

/// A finding with what the standing reach found on the course's strongest
/// sources for its subject (claim sources design §7.5): `found` is the
/// deciding standing page's finding, where it was read (its reach and page),
/// and `cited` what the claim's section cites.
///
/// Found support becomes the claim's support, the prior one moving to
/// `also_supported`; found contradiction joins a supported claim's contrary
/// passages, for the overturn rule to weigh, and decides an unsupported one.
/// Nothing found leaves the finding as it was, its evidence unextended so its
/// fingerprint and receipts stay the same.
pub(in crate::features::learning::lessons::content_verification) fn with_standing(
    mut prior: Finding,
    found: Option<(Finding, Reach, usize)>,
    cited: &[String],
) -> Finding {
    let Some((found, reach, page)) = found else {
        return prior;
    };
    match found.verdict {
        ClaimVerdict::Supported => {
            let Some(support) = supporting_of(&found) else {
                return prior;
            };
            let moved = (prior.verdict == ClaimVerdict::Supported)
                .then(|| supporting_of(&prior))
                .flatten();
            // What the checker found saying otherwise stays for the overturn
            // rule to weigh against the found support.
            if prior.verdict == ClaimVerdict::Contradicted {
                let contrary = contrary_of(&prior);
                prior.two_pass.contrary.extend(contrary);
            }
            prior
                .two_pass
                .also_supported
                .retain(|other| other.passage.source_id != support.passage.source_id);
            prior.two_pass.also_supported.extend(moved);
            prior.two_pass.supported_by =
                (!cited.contains(&support.passage.source_id)).then(|| SupportedElsewhere {
                    source_id: support.passage.source_id.clone(),
                    start_byte: support.passage.start_byte,
                    end_byte: support.passage.end_byte,
                    cited: cited.to_vec(),
                });
            if !prior
                .evidence
                .iter()
                .any(|passage| same_passage(passage, &support.passage))
            {
                prior.evidence.push(support.passage.clone());
            }
            prior.supporting_quote = Some(support.quote);
            prior.verdict = ClaimVerdict::Supported;
            prior.reason = found.reason;
            prior.trust = None;
            prior.two_pass.pass = Some(Pass::Corroboration);
            prior.two_pass.reach = Some(reach);
            prior.two_pass.page = Some(page);
            prior
        }
        ClaimVerdict::Contradicted if prior.verdict == ClaimVerdict::Supported => {
            prior.two_pass.contrary.extend(contrary_of(&found));
            prior
        }
        ClaimVerdict::Contradicted if prior.verdict == ClaimVerdict::Unsupported => {
            let mut found = found;
            found.two_pass.pass = Some(Pass::Corroboration);
            found.two_pass.reach = Some(reach);
            found.two_pass.page = Some(page);
            found
        }
        _ => prior,
    }
}

/// The same bytes of the same source version.
pub(in crate::features::learning) fn same_passage(
    a: &EvidencePassage,
    b: &EvidencePassage,
) -> bool {
    a.source_id == b.source_id && a.start_byte == b.start_byte && a.end_byte == b.end_byte
}

/// The claim's finding from its two passes: `attribution`, pass A's finding
/// when its section cites anything, and `corroboration`, the pass B page
/// that decided it or the last one read, `at` that page (where B read, and
/// which page). `cited` is what the section cites.
pub(super) fn combine(
    attribution: Option<Finding>,
    corroboration: Finding,
    at: (Option<Reach>, usize),
    cited: &[String],
) -> Finding {
    let decided_by_b = |mut finding: Finding| {
        finding.two_pass.pass = Some(Pass::Corroboration);
        finding.two_pass.reach = at.0;
        finding.two_pass.page = Some(at.1);
        finding
    };
    let contradicted = |finding: &Finding| finding.verdict == ClaimVerdict::Contradicted;
    match attribution {
        Some(mut a) if a.verdict == ClaimVerdict::Supported => {
            a.two_pass.pass = Some(Pass::Attribution);
            if contradicted(&corroboration) {
                a.two_pass.contrary.extend(contrary_of(&corroboration));
            }
            // Both passes supported it: B's support counts too, as an
            // independent origin the gate may corroborate with.
            if corroboration.verdict == ClaimVerdict::Supported {
                a.two_pass
                    .also_supported
                    .extend(supporting_of(&corroboration));
            }
            a
        }
        attribution if corroboration.verdict == ClaimVerdict::Supported => {
            let mut b = decided_by_b(corroboration);
            b.two_pass.supported_by = supporting_passage(&b).map(|passage| SupportedElsewhere {
                source_id: passage.source_id.clone(),
                start_byte: passage.start_byte,
                end_byte: passage.end_byte,
                cited: cited.to_vec(),
            });
            if let Some(a) = attribution.filter(contradicted) {
                b.two_pass.contrary.extend(contrary_of(&a));
            }
            b
        }
        Some(mut a) if contradicted(&a) => {
            a.two_pass.pass = Some(Pass::Attribution);
            a
        }
        _ if contradicted(&corroboration) => decided_by_b(corroboration),
        attribution => unresolved(attribution, corroboration),
    }
}

/// A claim neither pass decided: unverified when either check could not be
/// judged, otherwise unsupported, with everything both compared and the
/// reason of the check that says most (the one that could not judge, or the
/// one that read something).
fn unresolved(attribution: Option<Finding>, mut corroboration: Finding) -> Finding {
    let Some(a) = attribution else {
        return corroboration;
    };
    let a_says_more = (a.verdict == ClaimVerdict::Unverified
        && corroboration.verdict != ClaimVerdict::Unverified)
        || corroboration.evidence.is_empty();
    if a_says_more {
        corroboration.verdict = a.verdict;
        corroboration.reason = a.reason;
        corroboration.supporting_quote = a.supporting_quote;
    }
    let mut evidence = a.evidence;
    evidence.extend(corroboration.evidence);
    corroboration.evidence = evidence;
    corroboration
}
