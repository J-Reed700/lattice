use super::{
    quote_in, unreadable, CheckReply, ClaimEvidence, ClaimVerdict, JudgeOutcome, Result,
    MAX_QUOTE_CHARS,
};

/// The labels of the fields a judge writes.
pub(super) const FIELD_LABELS: [&str; 8] = [
    "Label",
    "Reason",
    "Explanation",
    "Source passage",
    "Source quote",
    "Quote",
    "Evidence",
    "Verdict",
];

/// The reply with markdown emphasis taken off its field lines (`**Reason:** …`,
/// `**Verdict**: **unsupported**`), which some models add and the field
/// parsing does not expect. Any other line is left as written.
pub(super) fn without_field_emphasis(text: &str) -> String {
    text.lines()
        .map(|line| {
            let emphasis = |c: char| c == '*' || c == '_' || c.is_whitespace();
            let bare = line.trim_start_matches(emphasis);
            let Some((label, value)) = bare.split_once(':') else {
                return line.to_owned();
            };
            let label = label.trim_end_matches(emphasis);
            if !FIELD_LABELS
                .iter()
                .any(|field| label.eq_ignore_ascii_case(field))
            {
                return line.to_owned();
            }
            format!("{label}: {}", value.trim_matches(emphasis))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Providers sometimes join the three labeled fields onto one line. Recover
/// only their whitespace layout: each label must occur exactly once, in order,
/// with a single final verdict. Evidence locations are still checked below.
pub(super) fn normalize_reasoned_fields(text: &str) -> Option<String> {
    const REASON: &str = "reason:";
    const SOURCE: &str = "source passage:";
    const VERDICT: &str = "verdict:";
    let text = text.trim();
    // Explicit line boundaries take precedence over label-like prose inside
    // the explanation (for example, "maps to a source passage: ..."). Only
    // attempt the conservative inline recovery for a noncanonical layout.
    if final_verdict(text).is_some() {
        return Some(text.to_owned());
    }
    let lower = text.to_ascii_lowercase();
    if !lower.starts_with(REASON)
        || [REASON, SOURCE, VERDICT]
            .iter()
            .any(|label| lower.matches(label).count() != 1)
    {
        return None;
    }
    let source = lower.find(SOURCE)?;
    let verdict = lower.find(VERDICT)?;
    if source <= REASON.len() || verdict <= source + SOURCE.len() {
        return None;
    }
    if !text.get(..source)?.ends_with(char::is_whitespace)
        || !text.get(..verdict)?.ends_with(char::is_whitespace)
    {
        return None;
    }
    let reason = text
        .get(REASON.len()..source)?
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = format!(
        "Reason: {}\nSource passage: {}\nVerdict: {}",
        reason.trim(),
        text.get(source + SOURCE.len()..verdict)?.trim(),
        text.get(verdict + VERDICT.len()..)?.trim()
    );
    final_verdict(&normalized)?;
    Some(normalized)
}

pub(super) fn final_verdict(text: &str) -> Option<ClaimVerdict> {
    let lower = text.to_ascii_lowercase();
    if lower.matches("reason:").count() != 1 {
        return None;
    }
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let mut reason = None;
    for expected in ["Reason", "Source passage"] {
        let (label, value) = lines.next()?.split_once(':')?;
        if !label.trim().eq_ignore_ascii_case(expected) || value.trim().is_empty() {
            return None;
        }
        if expected == "Reason" {
            reason = Some(value.to_ascii_lowercase());
        }
    }
    let (label, value) = lines.next()?.split_once(':')?;
    if !label.trim().eq_ignore_ascii_case("Verdict") || lines.next().is_some() {
        return None;
    }
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    // A canonical three-line response may echo its final decision at the end
    // of the explanation. Accept only the identical value, never conflicting
    // decisions, extra result lines or ambiguous inline field layouts.
    if lower.matches("verdict:").count() != 1 {
        let reason = reason?;
        let (comparison, repeated) = reason.rsplit_once("verdict:")?;
        if lower.matches("verdict:").count() != 2
            || comparison.trim().is_empty()
            || repeated.trim() != value
        {
            return None;
        }
    }
    match value.as_str() {
        "supported" => Some(ClaimVerdict::Supported),
        "contradicted" => Some(ClaimVerdict::Contradicted),
        "unsupported" => Some(ClaimVerdict::Unsupported),
        _ => None,
    }
}

/// The outcome of a located, reasoned reply: its parse is [`CheckReply::read`],
/// the rest looks its source line up among the `passages` shown.
pub(super) fn read_reasoned(text: &str, passages: &[String]) -> Result<JudgeOutcome> {
    CheckReply::read(text)
        .and_then(|reply| judge_reply(reply, passages, |_| true))
        .map_err(unreadable)
}

/// The outcome a read reply makes: a supported or contradicted verdict needs
/// a passage among those `permitted`, an unsupported one carries no quote.
pub(super) fn judge_reply(
    reply: CheckReply,
    passages: &[String],
    permitted: impl Fn(&str) -> bool,
) -> std::result::Result<JudgeOutcome, &'static str> {
    let located = reply
        .source
        .as_deref()
        .and_then(|source| located_passage(source, passages));
    let quote = match reply.verdict {
        ClaimVerdict::Supported | ClaimVerdict::Contradicted => Some(
            located
                .filter(|quote| permitted(quote))
                .ok_or("the reply names no valid supporting evidence location")?,
        ),
        _ => None,
    };
    Ok(JudgeOutcome {
        verdict: reply.verdict,
        quote,
        reason: Some(reply.reason),
        confidence: None,
        uncertain: false,
    })
}

pub(super) fn located_quote(text: &str, passages: &[String]) -> Option<String> {
    located_passage(&labeled_value(text, &["Source passage"])?, passages)
}

/// The passage a `Source passage:` value names among those shown.
pub(super) fn located_passage(value: &str, passages: &[String]) -> Option<String> {
    let value = strip_wrapping_quotes(value);
    let index: usize = value
        .trim_matches(['[', ']'])
        .strip_prefix("passage-")?
        .parse()
        .ok()?;
    passages
        .get(index)
        .filter(|text| !text.trim().is_empty())
        .cloned()
}

#[derive(Debug, Default)]
pub struct JudgeExplanation {
    pub reason: Option<String>,
    pub quote: Option<String>,
}

/// Read a value from the small labeled section after the first verdict word.
/// The verdict itself is still parsed independently so first-token logprobs
/// remain meaningful and an explanation can never change the classification.
pub(super) fn labeled_value(text: &str, labels: &[&str]) -> Option<String> {
    text.lines().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        if !labels
            .iter()
            .any(|expected| label.trim().eq_ignore_ascii_case(expected))
        {
            return None;
        }
        let value = value.trim().trim_matches('`').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

pub(super) fn strip_wrapping_quotes(value: &str) -> String {
    let mut value = value.trim();
    if value.len() >= 2 {
        let first = value.chars().next();
        let last = value.chars().next_back();
        let starts = matches!(first, Some('"' | '“' | '`' | '\''));
        let ends = matches!(last, Some('"' | '”' | '`' | '\''));
        if starts && ends {
            let start = first.map_or(0, char::len_utf8);
            let end = last.map_or(0, char::len_utf8);
            value = &value[start..value.len() - end];
        }
    }
    value.trim().to_string()
}

pub(super) fn is_empty_quote(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "none" | "n/a" | "not available" | "no exact quote"
    )
}

pub fn parse_judge_explanation(
    text: &str,
    evidence: &ClaimEvidence,
    verdict: ClaimVerdict,
) -> JudgeExplanation {
    let reason = labeled_value(text, &["Reason", "Explanation"])
        .map(|reason| strip_wrapping_quotes(&reason))
        .filter(|reason| !reason.is_empty());
    let quote = labeled_value(text, &["Source quote", "Quote", "Evidence"])
        .map(|quote| strip_wrapping_quotes(&quote))
        .filter(|quote| !is_empty_quote(quote))
        .and_then(|quote| quote_in(&evidence.text, &quote).map(str::to_owned))
        .map(|quote| truncate_chars(&quote, MAX_QUOTE_CHARS));

    let reason = reason.or_else(|| match verdict {
        ClaimVerdict::Unsupported => {
            Some("The cited passage did not state or directly entail the claim.".to_string())
        }
        ClaimVerdict::Supported | ClaimVerdict::Contradicted | ClaimVerdict::Unverified => None,
    });

    JudgeExplanation { reason, quote }
}

/// The label a first token begins, if it begins one.
///
/// The three labels start with three different letters, so a tokenizer that
/// splits "unsupported" as "uns" + "upported" still names it in its first
/// piece. A token must be a prefix of its label: "so" begins no label.
pub(super) fn label_of_token(token: &str) -> Option<ClaimVerdict> {
    let word: String = token
        .chars()
        .skip_while(|c| !c.is_alphabetic())
        .take_while(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    if word.is_empty() {
        return None;
    }
    [
        ("supported", ClaimVerdict::Supported),
        ("contradicted", ClaimVerdict::Contradicted),
        ("unsupported", ClaimVerdict::Unsupported),
    ]
    .into_iter()
    .find(|(label, _)| label.starts_with(&word) || word.starts_with(label))
    .map(|(_, verdict)| verdict)
}

/// The verdict and its probability among the three labels.
///
/// `None` unless the most likely first token begins a label: when the model
/// was about to write something else ("The claim…", a markdown star), the
/// labels' share of the distribution says nothing and the text is read instead.
pub fn verdict_from_logprobs(alternatives: &[(String, f32)]) -> Option<(ClaimVerdict, f32)> {
    let top = alternatives.iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
    label_of_token(&top.0)?;

    let mut mass: Vec<(ClaimVerdict, f32)> = Vec::new();
    for (token, logprob) in alternatives {
        let Some(verdict) = label_of_token(token) else {
            continue;
        };
        let p = logprob.exp();
        match mass.iter_mut().find(|(known, _)| *known == verdict) {
            Some((_, total)) => *total += p,
            None => mass.push((verdict, p)),
        }
    }
    let total: f32 = mass.iter().map(|(_, p)| p).sum();
    if !(total.is_finite() && total > 0.0) {
        return None;
    }
    mass.into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(verdict, p)| (verdict, (p / total).clamp(0.0, 1.0)))
}

/// The verdict a reply's text opens with, for providers without logprobs.
pub fn parse_verdict_word(text: &str) -> Option<ClaimVerdict> {
    let lowered = text.trim().to_lowercase();
    let lowered = lowered.trim_start_matches(|c: char| !c.is_alphabetic());
    let lowered = lowered
        .strip_prefix("label:")
        .or_else(|| lowered.strip_prefix("verdict:"))
        .unwrap_or(lowered)
        .trim_start();
    if lowered.starts_with("unsupported")
        || lowered.starts_with("not supported")
        || lowered.starts_with("neutral")
    {
        Some(ClaimVerdict::Unsupported)
    } else if lowered.starts_with("contradict") {
        Some(ClaimVerdict::Contradicted)
    } else if lowered.starts_with("support") {
        Some(ClaimVerdict::Supported)
    } else {
        None
    }
}

pub(super) fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}
