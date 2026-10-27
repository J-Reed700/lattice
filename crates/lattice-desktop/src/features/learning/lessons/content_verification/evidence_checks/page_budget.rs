//! How much a deeper evidence page should hold (course evidence index §5.3).
//! Every page a claim needs costs the judge one more answer for it, so a page
//! of a few passages spends more on answers than on reading. The budget is
//! the reading that costs the time of one more answer, from the backend's
//! measured pace and this job's measured answers. Relevance still decides
//! the order and which passages qualify; the window still bounds a request.
//! What a page costs at the same pace is what pass B's break-even rule weighs
//! against researching the claim on the web (D5, `walk`).
use lattice_core::application::ports::llm_port::Pace;

/// The judge's answers this job: the output tokens it spent and the claims
/// they answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::features::learning) struct AnswerSize {
    tokens: u64,
    claims: u64,
}

impl AnswerSize {
    /// `tokens` output tokens answering `claims` claims. A reply the backend
    /// reported no usage for measures nothing.
    pub(in crate::features::learning) fn of(tokens: u64, claims: usize) -> Self {
        if tokens == 0 || claims == 0 {
            return Self::default();
        }
        Self {
            tokens,
            claims: u64::try_from(claims).unwrap_or(u64::MAX),
        }
    }

    pub(in crate::features::learning) fn add(&mut self, other: Self) {
        self.tokens = self.tokens.saturating_add(other.tokens);
        self.claims = self.claims.saturating_add(other.claims);
    }

    /// The mean output tokens per claim so far; `None` before any answer
    /// was measured.
    pub(in crate::features::learning) fn per_claim(self) -> Option<f64> {
        (self.claims > 0).then(|| self.tokens as f64 / self.claims as f64)
    }
}

/// Tokens a deeper claim page should hold: the reading that costs as long as
/// one more answer would, answer tokens per claim × (prompt tokens/s ÷ output
/// tokens/s), from the backend's measured pace and this job's mean answer.
/// `None` while either is unknown.
pub(in crate::features::learning) fn page_budget(
    pace: Option<Pace>,
    answer_tokens_per_claim: Option<f64>,
) -> Option<usize> {
    let pace = pace?;
    let tokens =
        answer_tokens_per_claim? * pace.prompt_tokens_per_second / pace.output_tokens_per_second;
    (tokens.is_finite() && tokens > 0.0).then(|| tokens.round() as usize)
}

/// Seconds one more pass B page of `page_tokens` costs a claim: page tokens ÷
/// prompt tokens/s + answer tokens per claim ÷ output tokens/s. `None` while
/// either is unknown.
pub(in crate::features::learning) fn page_seconds(
    pace: Option<Pace>,
    page_tokens: usize,
    answer_tokens_per_claim: Option<f64>,
) -> Option<f64> {
    let pace = pace?;
    let seconds = page_tokens as f64 / pace.prompt_tokens_per_second
        + answer_tokens_per_claim? / pace.output_tokens_per_second;
    seconds.is_finite().then_some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// About 850 prompt and 11.3 output tokens a second, and 130 answer tokens a
    /// claim.
    #[test]
    fn a_page_reads_in_the_time_one_more_answer_takes() {
        let pace = Pace {
            prompt_tokens_per_second: 850.0,
            output_tokens_per_second: 11.3,
        };
        let budget = page_budget(Some(pace), Some(130.0)).unwrap();
        // 130 × 850 / 11.3 = 9,778.8
        assert_eq!(budget, 9_779);
        let reading = budget as f64 / pace.prompt_tokens_per_second;
        let answering = 130.0 / pace.output_tokens_per_second;
        assert!((reading - answering).abs() < 1.0 / pace.prompt_tokens_per_second);
        assert_eq!(page_budget(None, Some(130.0)), None);
        assert_eq!(page_budget(Some(pace), None), None);
    }

    #[test]
    fn the_answer_size_is_the_mean_over_measured_answers() {
        let mut answers = AnswerSize::default();
        assert_eq!(answers.per_claim(), None);
        // A batch of four, one claim alone, and a backend that reported no usage.
        answers.add(AnswerSize::of(520, 4));
        answers.add(AnswerSize::of(150, 1));
        answers.add(AnswerSize::of(0, 3));
        assert_eq!(answers.per_claim(), Some(134.0));
    }
}
