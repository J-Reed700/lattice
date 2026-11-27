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

struct BrandToken {
    text: String,
    allow_case_insensitive: bool,
}

/// Returns true when a statement asserts product-, vendor-, or specification-release
/// behavior that must be verified against cited sources instead of general knowledge.
pub(super) fn requires_source_not_common(
    statement: &str,
    references: &ReferenceCollection<'_>,
) -> bool {
    let lower = statement.replace('\u{2019}', "'").to_ascii_lowercase();
    for vendor in source_brand_tokens(references) {
        if contains_word(statement, &vendor.text)
            || (vendor.allow_case_insensitive
                && contains_word(&lower, &vendor.text.to_ascii_lowercase()))
        {
            return true;
        }
    }
    let has_config_context = has_config_or_onboarding_context(&lower);
    if has_config_context
        && (has_proprietary_camel_identifier(statement)
            || has_mid_sentence_title_setting(statement))
    {
        return true;
    }
    has_version_profile_claim(statement, &lower)
}

fn has_config_or_onboarding_context(lower: &str) -> bool {
    [
        "configur",
        "onboard",
        "client registration",
        "register",
        "registr",
        "setting",
        "parameter",
        "profile",
        "tenant",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
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

fn source_brand_tokens(references: &ReferenceCollection<'_>) -> Vec<BrandToken> {
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
        "com",
        "org",
        "net",
        "gov",
        "edu",
        "technical",
        "specifications",
        "specification",
        "guide",
        "manual",
        "reference",
        "overview",
        "home",
        "resources",
        "resource",
        "documentation",
        "platform",
        "system",
        "systems",
        "more",
        "with",
        "from",
        "using",
        "about",
        "introduction",
        "standard",
        "standards",
        "protocol",
        "protocols",
        "security",
        "health",
        "healthcare",
        "authentication",
        "authorization",
        "library",
        "client",
        "clients",
        "server",
        "servers",
        "service",
        "services",
        "framework",
        "endpoint",
        "endpoints",
        "token",
        "tokens",
        "launch",
        "architecture",
        "design",
        "testing",
        "engineering",
        "concept",
        "concepts",
        "practice",
        "examples",
        "semantics",
        "modern",
        "basics",
        "handling",
        "errors",
        "error",
        "validation",
        "profile",
        "profiles",
        "version",
        "versions",
    ];
    let mut brands: Vec<BrandToken> = Vec::new();
    let mut push_clean = |raw: &str, preserve_case: bool| {
        let trimmed = raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-');
        let clean = trimmed.to_ascii_lowercase();
        let all_generic_parts = clean
            .split('-')
            .all(|part| part.is_empty() || GENERIC_BRANDS.contains(&part));
        if clean.len() < 4
            || GENERIC_BRANDS.contains(&clean.as_str())
            || all_generic_parts
            || !clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return;
        }
        let allow_case_insensitive = clean.len() >= 8;
        let candidate = if preserve_case {
            if trimmed
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
                || allow_case_insensitive
            {
                Some(trimmed.to_owned())
            } else {
                None
            }
        } else {
            Some(capitalize_first(&clean))
        };
        if let Some(text) = candidate {
            if !brands
                .iter()
                .any(|existing| existing.text.eq_ignore_ascii_case(&text))
            {
                brands.push(BrandToken {
                    text,
                    allow_case_insensitive,
                });
            }
        }
    };
    for source in &references.sources {
        if let Some(url) = &source.url {
            if let Some(host) = url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split(['/', '?', '#']).next())
            {
                let labels: Vec<&str> = host.split('.').collect();
                for label in labels.iter().take(labels.len().saturating_sub(1)) {
                    push_clean(label, false);
                }
            }
        }
        for segment in source.title.split(" | ").skip(1).chain(
            source
                .title
                .split(" :: ")
                .take(1)
                .filter(|_| source.title.contains(" :: ")),
        ) {
            for word in segment.split(|ch: char| ch.is_whitespace() || ch == '.') {
                push_clean(word, true);
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

fn is_release_version_token(token: &str) -> bool {
    let Some((&first, rest)) = token.as_bytes().split_first() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    digits == rest.len()
        || (digits + 1 == rest.len() && rest.get(digits).is_some_and(u8::is_ascii_uppercase))
}

fn has_version_profile_claim(statement: &str, lower: &str) -> bool {
    let mentions_release_version = statement
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(is_release_version_token);
    mentions_release_version
        && [
            "register", "registr", "profile", "scope", "reject", "default", "support",
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
        let sources = vec![
            LearningSourceDto {
                id: "s1".into(),
                title: "Technical Specifications".into(),
                url: Some("https://open.epic.com/Technical-Specifications".into()),
                excerpt: "Overview".into(),
                acquired_at: 0,
            },
            LearningSourceDto {
                id: "s2".into(),
                title: "Standard Library | Engineering Concepts".into(),
                url: Some("https://docs.athenahealth.com/api".into()),
                excerpt: "Overview".into(),
                acquired_at: 0,
            },
        ];
        let refs = ReferenceCollection::lexical(&sources).unwrap();
        assert!(requires_source_not_common(
            "Epic performs scope validation to ensure permissions match the registered client profile.",
            &refs
        ));
        assert!(requires_source_not_common(
            "athenahealth enforces registered scope defaults for tenant requests.",
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
        assert!(!requires_source_not_common(
            "Calling addEventListener or getElementById attaches DOM callbacks safely.",
            &refs
        ));
        assert!(!requires_source_not_common(
            "Transport Layer Security Protocol protects HTTP traffic in transit.",
            &refs
        ));
        assert!(!requires_source_not_common(
            "A standard library provides helper functions across engineering services.",
            &refs
        ));
    }
}
