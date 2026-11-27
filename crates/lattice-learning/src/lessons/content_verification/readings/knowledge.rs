//! `Knowledge` and `Veto`: the knowledge judge's closed-book samples, read by
//! one of two rules. Knowledge passes a statement on unanimity; the veto fails
//! it only when every sample calls it false. Every receipt of a batch is
//! restored before its first call.
use super::super::class_checks::{
    finding_of, knowledge_key, knowledge_of, saved_knowledge, standard, unjudged_finding, vetoed,
};
use super::super::classify::case_text;
use super::*;
use lattice_core::application::services::claim_verification::{KnowledgeOutcome, KnowledgeVerdict};

/// How the samples are read.
#[derive(Clone, Copy)]
enum Rule {
    Knowledge,
    Veto,
}

pub(super) struct Knowledge {
    rule: Rule,
}

impl Knowledge {
    pub(super) fn unanimous() -> Self {
        Self {
            rule: Rule::Knowledge,
        }
    }

    pub(super) fn veto() -> Self {
        Self { rule: Rule::Veto }
    }
}

#[async_trait]
impl Reader for Knowledge {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        match self.rule {
            Rule::Knowledge => Reading::Knowledge,
            Rule::Veto => Reading::Veto,
        }
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut made = Made::new(statements.len());
        let mut asks = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let case = case_text(at.candidate, statement.unit, &statement.claim.quote);
            let key = knowledge_key(at.llm, &statement.claim.statement, case.as_deref());
            match saved_knowledge(&key).await? {
                Some(saved) => {
                    let read = self.read_of(at, statement, Some((saved, true)), case.is_some());
                    made.put(at, ordinal, read);
                }
                None => asks.push((ordinal, *statement, key, case)),
            }
        }
        let mut asked: futures::stream::FuturesUnordered<_> = asks
            .into_iter()
            .map(|(ordinal, statement, key, case)| async move {
                let judged =
                    knowledge_of(at.llm, &key, &statement.claim.statement, case.as_deref()).await?;
                Ok::<_, AppError>((ordinal, statement, judged, case.is_some()))
            })
            .collect();
        while let Some(asked) = futures::StreamExt::next(&mut asked).await {
            let (ordinal, statement, judged, with_case) = asked?;
            let read = self.read_of(
                at,
                &statement,
                judged.map(|knowledge| (knowledge, false)),
                with_case,
            );
            made.put(at, ordinal, read);
        }
        made.all()
    }
}

impl Knowledge {
    /// The reading of `statement` that the knowledge judge's samples make by
    /// this rule; `None` when a reply could not be read. `with_case` says the
    /// samples were asked with the case the statement comes from.
    fn read_of(
        &self,
        at: &ReadContext<'_>,
        statement: &Statement<'_>,
        judged: Option<(KnowledgeOutcome, bool)>,
        with_case: bool,
    ) -> Read {
        let Statement {
            unit,
            claim,
            classed,
        } = *statement;
        match judged {
            None => Read::new(
                Outcome::Unjudged,
                unjudged_finding(unit, claim, classed, &[]),
                false,
            ),
            Some((knowledge, reused)) => {
                let mut outcome = match self.rule {
                    Rule::Knowledge => knowledge.knowledge_outcome(),
                    Rule::Veto => knowledge.veto_outcome(),
                };
                let mut guarded_reason: Option<String> = None;
                if matches!(self.rule, Rule::Knowledge)
                    && outcome == Outcome::Stands
                    && requires_source_not_common(&claim.statement, at.references)
                {
                    outcome = Outcome::NeedsSource;
                    guarded_reason = Some(
                        "Asked on its own, this statement names a vendor, product setting, or version-specific profile rule that requires an authoritative source rather than general knowledge."
                            .to_owned(),
                    );
                }
                let (verdict, reason) = match (outcome, self.rule) {
                    (Outcome::Refuted, _) => {
                        (ClaimVerdict::Contradicted, vetoed(&knowledge, with_case))
                    }
                    (Outcome::Stands, Rule::Knowledge) => {
                        (ClaimVerdict::Supported, standard(&knowledge))
                    }
                    (Outcome::Stands, Rule::Veto) => {
                        (ClaimVerdict::Supported, not_false(&knowledge))
                    }
                    _ => (
                        ClaimVerdict::Unsupported,
                        guarded_reason.unwrap_or_else(|| not_common(&knowledge)),
                    ),
                };
                Read::new(
                    outcome,
                    finding_of((unit, claim, classed), verdict, reason, &[], None),
                    reused,
                )
            }
        }
    }
}

/// Returns true when a statement asserts product-, vendor-, or specification-release
/// behavior that must be verified against cited sources instead of general knowledge.
pub(super) fn requires_source_not_common(
    statement: &str,
    references: &ReferenceCollection<'_>,
) -> bool {
    const VENDOR_CASE_SENSITIVE: &[&str] = &["Epic", "Cerner"];
    const VENDOR_LOWER: &[&str] = &[
        "athenahealth",
        "veradigm",
        "allscripts",
        "eclinicalworks",
        "meditech",
        "documo",
    ];
    let lower = statement.replace('\u{2019}', "'").to_ascii_lowercase();
    if VENDOR_CASE_SENSITIVE
        .iter()
        .any(|brand| contains_word(statement, brand))
        || VENDOR_LOWER
            .iter()
            .any(|brand| contains_word(&lower, brand))
    {
        return true;
    }
    for vendor in source_brand_tokens(references) {
        if contains_word(statement, &vendor) {
            return true;
        }
    }
    if has_proprietary_camel_identifier(statement) || has_mid_sentence_title_setting(statement) {
        return true;
    }
    has_version_profile_claim(&lower)
}

fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack.get(from..).and_then(|s| s.find(word)) {
        let start = from + rel;
        let end = start + word.len();
        let left_ok = start
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        let right_ok = bytes
            .get(end)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn source_brand_tokens(references: &ReferenceCollection<'_>) -> Vec<String> {
    const GENERIC_BRANDS: &[&str] = &[
        "api",
        "apis",
        "docs",
        "doc",
        "developer",
        "developers",
        "dev",
        "fhir",
        "hl7",
        "oauth",
        "oauth2",
        "smart",
        "smarthealthit",
        "http",
        "https",
        "ietf",
        "datatracker",
        "rfc",
        "rfc-editor",
        "wikipedia",
        "github",
        "gitlab",
        "npmjs",
        "nodejs",
        "nestjs",
        "angular",
        "typescript",
        "javascript",
        "python",
        "postgres",
        "postgresql",
        "mysql",
        "kafka",
        "kubernetes",
        "docker",
        "google",
        "microsoft",
        "mozilla",
        "mdn",
        "w3",
        "w3c",
        "localhost",
        "example",
        "open",
        "app",
        "portal",
        "www",
        "technical",
        "specifications",
        "specification",
        "guide",
        "manual",
        "reference",
        "overview",
        "home",
    ];
    let mut brands = Vec::new();
    for source in &references.sources {
        if let Some(url) = &source.url {
            if let Some(host) = url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split(['/', '?', '#']).next())
            {
                for label in host.split('.') {
                    let clean = label.trim().to_ascii_lowercase();
                    if clean.len() >= 4
                        && !GENERIC_BRANDS.contains(&clean.as_str())
                        && clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    {
                        let titled = capitalize_first(&clean);
                        if !brands.contains(&titled) {
                            brands.push(titled);
                        }
                    }
                }
            }
        }
    }
    brands
}

fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn has_proprietary_camel_identifier(statement: &str) -> bool {
    statement.split_whitespace().any(|raw| {
        let token = raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        let starts_lower = token.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let caps = token.chars().filter(|c| c.is_ascii_uppercase()).count();
        starts_lower && caps >= 2 && token.len() >= 10
    })
}

fn has_mid_sentence_title_setting(statement: &str) -> bool {
    let mut streak = 0usize;
    let mut title_words = 0usize;
    for (idx, raw) in statement.split_whitespace().enumerate() {
        let token = raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        let first_upper = token.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let has_lower = token.chars().any(|c| c.is_ascii_lowercase());
        let after_stop = idx == 0;
        if !after_stop && first_upper && token.len() >= 3 {
            streak += 1;
            if has_lower {
                title_words += 1;
            }
            if streak >= 4 && title_words >= 2 {
                return true;
            }
        } else {
            streak = 0;
            title_words = 0;
        }
    }
    false
}

fn has_version_profile_claim(lower: &str) -> bool {
    let mentions_fhir_release = ["r4", "r4b", "r5", "stu3", "dstu2"]
        .iter()
        .any(|rel| contains_word(lower, rel));
    mentions_fhir_release
        && [
            "register", "profile", "scope", "reject", "default", "support",
        ]
        .iter()
        .any(|cue| lower.contains(cue))
}

/// How many samples gave `verdict`.
fn count(knowledge: &KnowledgeOutcome, verdict: KnowledgeVerdict) -> usize {
    knowledge
        .samples
        .iter()
        .filter(|sample| sample.verdict == verdict)
        .count()
}

/// Why a statement the model does not call false passes the veto.
fn not_false(knowledge: &KnowledgeOutcome) -> String {
    format!(
        "Asked on its own, the model does not call this false ({} of {}).",
        count(knowledge, KnowledgeVerdict::False),
        knowledge.samples.len()
    )
}

/// Why a statement the model will not call standard knowledge needs a source.
fn not_common(knowledge: &KnowledgeOutcome) -> String {
    let reason = knowledge
        .samples
        .iter()
        .find(|sample| sample.verdict != KnowledgeVerdict::TrueCommon)
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    format!(
        "Asked on its own, the model does not call this standard knowledge ({} of {}): {reason}",
        count(knowledge, KnowledgeVerdict::TrueCommon),
        knowledge.samples.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::LearningSourceDto;

    #[test]
    fn guards_vendor_product_config_and_release_profile_claims_from_true_common() {
        let sources = vec![LearningSourceDto {
            id: "s1".into(),
            title: "Technical Specifications".into(),
            url: Some("https://open.epic.com/Technical-Specifications".into()),
            excerpt: "Overview".into(),
            acquired_at: 0,
        }];
        let refs = ReferenceCollection::lexical(&sources).unwrap();
        assert!(requires_source_not_common(
            "Epic performs scope validation to ensure permissions match the registered client profile.",
            &refs
        ));
        assert!(requires_source_not_common(
            "Configuring the App Default FHIR Version ensures that API calls align with the profile.",
            &refs
        ));
        assert!(requires_source_not_common(
            "Omit the `appDefaultFhirVersion` setting when onboarding.",
            &refs
        ));
        assert!(requires_source_not_common(
            "If a client is registered for R4 but uses R5-specific features, the server may reject the request.",
            &refs
        ));

        assert!(!requires_source_not_common(
            "The OAuth2 client credentials flow uses a client ID and secret to obtain an access token without user interaction.",
            &refs
        ));
        assert!(!requires_source_not_common(
            "Adding jitter to retry delays prevents multiple connectors from retrying at the exact same moment.",
            &refs
        ));
        assert!(!requires_source_not_common(
            "An epic outage in upstream DNS resolution delays requests.",
            &refs
        ));
    }
}
