//! Ungrounded external company/employer framing detection for Finding D.5 and
//! scenario-local research filtering.

use super::spec_checks::prose_without_fences;
use crate::dto::LearningSourceDto;
use lattice_core::shared::text::sentences;

#[cfg(test)]
#[path = "company_framing_tests.rs"]
mod tests;

const EMPLOYER_CUES: &[&str] = &[
    "role",
    "job",
    "position",
    "interview",
    "interviewing",
    "onboarding",
    "offer",
    "career",
    "team",
    "company",
    "employer",
    "organization",
    "organisation",
    "firm",
    "startup",
    "vendor",
    "client",
    "engineer",
    "engineering",
    "architect",
    "developer",
    "manager",
    "lead",
    "director",
    "analyst",
    "scientist",
    "designer",
    "specialist",
    "consultant",
    "working",
    "work",
    "hired",
    "applying",
    "apply",
    "transitioning",
    "moving",
    "prepare",
    "preparing",
];

const NON_COMPANY_TITLE_WORDS: &[&str] = &[
    "engineer",
    "engineering",
    "architect",
    "developer",
    "manager",
    "lead",
    "director",
    "analyst",
    "scientist",
    "designer",
    "specialist",
    "role",
    "team",
    "backend",
    "frontend",
    "fullstack",
    "full",
    "stack",
    "senior",
    "junior",
    "staff",
    "principal",
    "integrations",
    "integration",
    "healthcare",
    "clinical",
    "software",
    "data",
    "systems",
    "system",
    "cloud",
    "security",
    "platform",
    "product",
    "project",
    "certification",
    "exam",
    "course",
    "module",
    "lesson",
    "services",
    "service",
    "app",
    "launch",
    "scale",
    "rest",
    "home",
    "work",
    "production",
    "staging",
    "local",
    "client",
    "patient",
    "user",
    "portal",
    "tenant",
    "token",
    "server",
    "gateway",
    "oauth",
    "fhir",
    "smart",
    "ehr",
    "connector",
    "api",
    "credentials",
    "credential",
    "webhook",
    "webhooks",
    "endpoint",
    "endpoints",
    "assertion",
    "assertions",
    "bundle",
    "route",
    "routes",
    "handler",
    "handlers",
    "worker",
    "workers",
    "queue",
    "store",
    "schema",
    "schemas",
    "docker",
    "kubernetes",
    "linux",
    "windows",
    "mac",
    "python",
    "rust",
    "javascript",
    "typescript",
    "java",
    "go",
    "react",
    "node",
    "postgres",
    "postgresql",
    "redis",
    "kafka",
    "the",
    "a",
    "an",
];

const CORPORATE_SUFFIXES: &[&str] = &[
    "inc",
    "inc.",
    "corp",
    "corp.",
    "corporation",
    "llc",
    "ltd",
    "ltd.",
    "gmbh",
    "co",
    "co.",
];

const FACTUAL_ADVERBS: &[&str] = &[
    "also",
    "currently",
    "internally",
    "typically",
    "often",
    "always",
];

const FACTUAL_VERBS: &[&str] = &[
    "is",
    "are",
    "has",
    "have",
    "runs",
    "run",
    "ran",
    "uses",
    "use",
    "used",
    "stores",
    "store",
    "stored",
    "assigns",
    "assign",
    "assigned",
    "sends",
    "send",
    "sent",
    "receives",
    "receive",
    "received",
    "keeps",
    "keep",
    "kept",
    "maintains",
    "maintain",
    "maintained",
    "exposes",
    "expose",
    "exposed",
    "connects",
    "connect",
    "connected",
    "needs",
    "need",
    "needed",
    "requires",
    "require",
    "required",
    "supports",
    "support",
    "supported",
    "permits",
    "permit",
    "permitted",
    "allows",
    "allow",
    "allowed",
    "authenticates",
    "authenticate",
    "authenticated",
    "registers",
    "register",
    "registered",
    "configures",
    "configure",
    "configured",
    "deploys",
    "deploy",
    "deployed",
    "builds",
    "build",
    "built",
    "delivers",
    "deliver",
    "delivered",
    "processes",
    "process",
    "processed",
    "manages",
    "manage",
    "managed",
    "handles",
    "handle",
    "handled",
    "holds",
    "hold",
    "logs",
    "log",
    "chooses",
    "choose",
    "decides",
    "decide",
];

const TRANSITIVE_VERBS: &[&str] = &[
    "permit", "permits", "allow", "allows", "require", "requires", "enable", "enables", "expect",
    "expects",
];

/// Returns real external company/employer names referenced in `contexts` (such
/// as `programGoal` or `priorKnowledge`) that are not documented by saved source
/// titles or URLs.
pub(crate) fn ungrounded_companies(
    contexts: &[&str],
    sources: &[LearningSourceDto],
) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for context in contexts {
        for candidate in extract_named_companies(context) {
            if is_company_grounded(&candidate, sources) {
                continue;
            }
            if !seen
                .iter()
                .any(|held| held.eq_ignore_ascii_case(&candidate))
            {
                seen.push(candidate);
            }
        }
    }
    seen
}

/// Checks `text` (prose outside code fences) for sentences attributing
/// invented proprietary facts, architecture, rules or actions to any
/// ungrounded company name in `companies`.
pub(crate) fn company_framing_faults(text: &str, companies: &[String]) -> Vec<String> {
    company_framing_fault_entries(text, companies)
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

pub(crate) fn company_framing_fault_entries(
    text: &str,
    companies: &[String],
) -> Vec<(String, String)> {
    if companies.is_empty() || text.trim().is_empty() {
        return Vec::new();
    }
    let prose = prose_without_fences(text);
    let mut faults: Vec<(String, String)> = Vec::new();
    for (start, end) in sentences(&prose) {
        let Some(sentence) = prose.get(start..end) else {
            continue;
        };
        let normalized = normalize_for_matching(sentence);
        for company in companies {
            let needle = company.to_ascii_lowercase();
            if !contains_whole_phrase(&normalized, &needle) {
                continue;
            }
            if sentence_attributes_company(&normalized, &needle) {
                let message = format!(
                    "States invented internals, rules, or actions as facts about external company {company} without verified company sources; frame as a proposed design or neutral scenario instead."
                );
                if !faults.iter().any(|(_, existing)| existing == &message) {
                    faults.push((sentence.trim().to_owned(), message));
                }
            }
        }
    }
    faults
}

/// True when `text` contains `phrase` at ASCII alphanumeric word boundaries.
pub(crate) fn contains_whole_phrase(haystack: &str, phrase: &str) -> bool {
    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_phrase = phrase.trim().to_ascii_lowercase();
    if lower_phrase.is_empty() {
        return false;
    }
    let bytes = lower_haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = lower_haystack[from..].find(&lower_phrase) {
        let start = from + rel;
        let end = start + lower_phrase.len();
        let left_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn normalize_for_matching(text: &str) -> String {
    text.replace(['\u{2019}', '\u{2018}'], "'")
        .replace(['*', '_', '`'], " ")
        .to_ascii_lowercase()
}

fn clean_token(raw: &str) -> &str {
    raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '.')
        .trim_end_matches('.')
}

fn has_clause_break(raw: &str) -> bool {
    raw.ends_with([',', '.', ';', ':', '?', '!'])
}

fn is_candidate_word(token: &str) -> bool {
    let clean = clean_token(token);
    let Some(first) = clean.chars().next() else {
        return false;
    };
    if !first.is_ascii_uppercase() || clean.len() < 2 {
        return false;
    }
    let lower = clean.to_ascii_lowercase();
    if NON_COMPANY_TITLE_WORDS.contains(&lower.as_str()) {
        return false;
    }
    // Filter out pure uppercase technical acronyms (`FHIR`, `SMART`, `OAuth` has lowercase).
    let has_lower = clean.chars().any(|ch| ch.is_ascii_lowercase());
    has_lower || CORPORATE_SUFFIXES.contains(&lower.as_str())
}

fn extract_named_companies(text: &str) -> Vec<String> {
    let raw_tokens: Vec<&str> = text.split_whitespace().collect();
    let mut found = Vec::new();
    for (idx, &raw) in raw_tokens.iter().enumerate() {
        let lower = clean_token(raw).to_ascii_lowercase();
        let direct_trigger = matches!(
            lower.as_str(),
            "joining" | "joined" | "company" | "employer"
        );
        let prep_trigger = match lower.as_str() {
            "at" | "for" | "to" => window_has_cue(&raw_tokens, idx),
            "with" => window_has_cue_matching(&raw_tokens, idx, |w| {
                matches!(
                    w,
                    "interview" | "interviewing" | "company" | "employer" | "firm" | "team"
                )
            }),
            _ => false,
        };
        if !direct_trigger && !prep_trigger {
            continue;
        }
        let mut parts: Vec<&str> = Vec::new();
        for &next_raw in raw_tokens.iter().skip(idx + 1).take(3) {
            let cleaned = clean_token(next_raw);
            if !is_candidate_word(next_raw) {
                break;
            }
            parts.push(cleaned);
            if has_clause_break(next_raw) {
                break;
            }
        }
        if parts.is_empty() {
            continue;
        }
        if parts.len() >= 2
            && parts.last().is_some_and(|last| {
                CORPORATE_SUFFIXES.contains(&last.to_ascii_lowercase().as_str())
            })
        {
            parts.pop();
        }
        let name = parts.join(" ");
        if !name.is_empty()
            && !found
                .iter()
                .any(|held: &String| held.eq_ignore_ascii_case(&name))
        {
            found.push(name);
        }
    }
    found
}

fn window_has_cue(tokens: &[&str], prep_idx: usize) -> bool {
    window_has_cue_matching(tokens, prep_idx, |word| EMPLOYER_CUES.contains(&word))
}

fn window_has_cue_matching(
    tokens: &[&str],
    prep_idx: usize,
    predicate: impl Fn(&str) -> bool,
) -> bool {
    let start = prep_idx.saturating_sub(5);
    for raw in tokens[start..prep_idx].iter().rev() {
        if has_clause_break(raw) {
            break;
        }
        let word = clean_token(raw).to_ascii_lowercase();
        if predicate(&word) {
            return true;
        }
    }
    false
}

fn is_company_grounded(company: &str, sources: &[LearningSourceDto]) -> bool {
    let compact = company.to_ascii_lowercase().replace([' ', '-'], "");
    let hyphenated = company.to_ascii_lowercase().replace(' ', "-");
    sources.iter().any(|source| {
        contains_whole_phrase(&source.title, company)
            || source.url.as_deref().is_some_and(|url| {
                let lower_url = url.to_ascii_lowercase();
                contains_whole_phrase(&lower_url, company)
                    || (!hyphenated.is_empty() && contains_whole_phrase(&lower_url, &hyphenated))
                    || (!compact.is_empty() && lower_url.contains(&format!("{compact}.")))
            })
    })
}

fn sentence_attributes_company(normalized: &str, company_lower: &str) -> bool {
    let possessive = format!("{company_lower}'s");
    if contains_whole_phrase(normalized, &possessive) {
        return true;
    }

    let words: Vec<&str> = normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '\'')
        .filter(|word| !word.is_empty())
        .collect();
    let company_words: Vec<&str> = company_lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .filter(|word| !word.is_empty())
        .collect();
    if company_words.is_empty() || words.len() < company_words.len() {
        return false;
    }

    for index in 0..=words.len() - company_words.len() {
        if words[index..index + company_words.len()] != company_words[..] {
            continue;
        }
        let after = index + company_words.len();
        // `<company> <adverb?> <factual verb>`
        let verb_index = if words
            .get(after)
            .is_some_and(|word| FACTUAL_ADVERBS.contains(word))
        {
            after + 1
        } else {
            after
        };
        if let Some(&verb) = words.get(verb_index) {
            if FACTUAL_VERBS.contains(&verb) {
                let allowed_identity = verb == "is"
                    && matches!(
                        words.get(verb_index + 1).copied(),
                        Some("a" | "an" | "the" | "not" | "mentioned" | "named")
                    )
                    && words[verb_index + 1..].iter().take(4).any(|next| {
                        matches!(
                            *next,
                            "company"
                                | "employer"
                                | "organization"
                                | "organisation"
                                | "firm"
                                | "goal"
                        )
                    });
                if !allowed_identity {
                    return true;
                }
            }
        }
        // `<transitive verb> ... <company> to ...` (for example `permit Documo to do`)
        if words.get(after).copied() == Some("to")
            && words[index.saturating_sub(3)..index]
                .iter()
                .any(|prev| TRANSITIVE_VERBS.contains(prev))
        {
            return true;
        }
    }
    false
}
