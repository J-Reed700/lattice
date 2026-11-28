//! The order a quiz's bank is written in: a plan of steps, and a runner that hands the steps out one at a time.
use crate::assessment_engine::LearningBlueprintRequirement;
use crate::assessment_generation::{Authored, AuthoringNeed, BankPass};
use std::collections::VecDeque;

/// The steps that write a quiz's bank, in order: the first form's worth of every requirement, then the rest of
/// every requirement's bank target. `held[i]` is how many items the bank holds that `requirements[i]` can take.
/// Each step is (index into requirements, count, pass); no step has a count of zero.
pub(crate) fn bank_steps(
    requirements: &[LearningBlueprintRequirement],
    held: &[usize],
) -> Vec<(usize, usize, BankPass)> {
    let mut steps = pass_steps(requirements, held, BankPass::FirstForm);
    steps.extend(pass_steps(requirements, held, BankPass::Retakes));
    steps
}

fn pass_steps(
    requirements: &[LearningBlueprintRequirement],
    held: &[usize],
    pass: BankPass,
) -> Vec<(usize, usize, BankPass)> {
    requirements
        .iter()
        .zip(held)
        .enumerate()
        .filter_map(|(index, (requirement, held))| {
            let count = match pass {
                BankPass::FirstForm => requirement.count.saturating_sub(*held),
                BankPass::Retakes => requirement
                    .bank_target()
                    .saturating_sub((*held).max(requirement.count)),
            };
            (count > 0).then_some((index, count, pass))
        })
        .collect()
}

/// A bank written need by need, in order.
pub(crate) struct BankWriting {
    queue: VecDeque<AuthoringNeed>,
    /// Every question written so far, with the outcomes it counts for.
    written: Vec<(Vec<String>, String)>,
    /// Kept source quotes so subsequent steps rotate onto fresher blocks.
    written_quotes: Vec<(Vec<String>, String)>,
    /// Requirements whose first form came back short.
    short: Vec<LearningBlueprintRequirement>,
}

impl BankWriting {
    pub(crate) fn new(needs: Vec<AuthoringNeed>) -> Self {
        Self {
            queue: needs.into(),
            written: Vec::new(),
            written_quotes: Vec::new(),
            short: Vec::new(),
        }
    }

    /// The next need to write. Its existing prompts also hold every prompt written before it for its outcome
    /// (an item counts for each outcome in its candidate's outcome_ids). A `Retakes` need is skipped when the
    /// `FirstForm` need of the same requirement came back short.
    pub(crate) fn next(&mut self) -> Option<AuthoringNeed> {
        loop {
            let mut need = self.queue.pop_front()?;
            if need.pass == BankPass::Retakes && self.short.contains(&need.requirement) {
                continue;
            }
            for (outcomes, prompt) in &self.written {
                if outcomes.contains(&need.requirement.outcome_id) {
                    need.existing_prompts.push(prompt.clone());
                }
            }
            let outcome_quotes: Vec<&str> = self
                .written_quotes
                .iter()
                .filter(|(outcomes, _)| outcomes.contains(&need.requirement.outcome_id))
                .map(|(_, quote)| quote.as_str())
                .collect();
            if !outcome_quotes.is_empty() && need.material.len() >= 3 {
                let counts: Vec<usize> = need
                    .material
                    .iter()
                    .map(|passage| passage_quote_count(&passage.text, &outcome_quotes))
                    .collect();
                let unsaturated = counts.iter().filter(|&&c| c < 2).count();
                if unsaturated >= 2 {
                    let filtered = need
                        .material
                        .iter()
                        .zip(&counts)
                        .filter(|&(_, &c)| c < 2)
                        .map(|(passage, _)| passage.clone())
                        .collect();
                    need.material = filtered;
                }
            }
            return Some(need);
        }
    }

    /// Record what writing `need` produced. A `FirstForm` need comes back short when it kept fewer items than its count.
    pub(crate) fn wrote(&mut self, need: &AuthoringNeed, authored: &Authored) {
        if need.pass == BankPass::FirstForm && authored.items.len() < need.count {
            self.short.push(need.requirement.clone());
        }
        for item in &authored.items {
            let candidate =
                crate::assessment_engine::LearningAssessmentCandidate::from(item.candidate.clone());
            self.written.push((
                item.candidate.outcome_ids.clone(),
                crate::assessment::item_review::item_repeat_descriptor(&candidate),
            ));
            let trimmed = item.source_quote.trim();
            if trimmed.len() >= 30 {
                self.written_quotes
                    .push((item.candidate.outcome_ids.clone(), trimmed.to_owned()));
            }
        }
    }
}

fn normalize_quote_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn passage_quote_count(passage_text: &str, outcome_quotes: &[&str]) -> usize {
    let norm_passage = normalize_quote_ws(passage_text);
    outcome_quotes
        .iter()
        .filter(|quote| {
            let norm_quote = normalize_quote_ws(quote);
            norm_quote.len() >= 30 && norm_passage.contains(&norm_quote)
        })
        .count()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::assessment_engine::{LearningAssessmentKey, LearningItemFormat};
    use crate::assessment_generation::{AuthoredItem, StoredItem};

    fn requirement(
        outcome: &str,
        format: LearningItemFormat,
        count: usize,
    ) -> LearningBlueprintRequirement {
        LearningBlueprintRequirement {
            outcome_id: outcome.into(),
            format,
            count,
            difficulty_min: 1,
            difficulty_max: 3,
        }
    }

    fn need(
        outcome: &str,
        format: LearningItemFormat,
        count: usize,
        pass: BankPass,
    ) -> AuthoringNeed {
        AuthoringNeed {
            requirement: requirement(outcome, format, count),
            outcome_title: "Sorting parcels".into(),
            outcome_description: "Say which shelf a parcel goes to.".into(),
            lesson_id: None,
            count,
            pass,
            material: vec![],
            existing_prompts: vec!["Which shelf takes a flat parcel?".into()],
        }
    }

    fn authored(outcome: &str, prompt: &str, count: usize) -> Authored {
        Authored {
            items: (0..count)
                .map(|n| AuthoredItem {
                    candidate: StoredItem {
                        id: format!("{prompt}-{n}"),
                        outcome_ids: vec![outcome.into()],
                        format: LearningItemFormat::SingleChoice,
                        difficulty: 2,
                        prompt: prompt.into(),
                        options: vec![],
                        left: vec![],
                        source_version_ids: vec![],
                        rubric: vec![],
                        key: LearningAssessmentKey::Choice(0),
                        explanation: String::new(),
                        distractor_rationales: vec![],
                    },
                    source_quote: String::new(),
                    lesson_id: None,
                })
                .collect(),
            dropped: vec![],
        }
    }

    #[test]
    fn a_bank_is_written_a_first_form_of_every_requirement_before_any_retake_items() {
        use BankPass::{FirstForm, Retakes};
        let requirements = [
            requirement("a", LearningItemFormat::SingleChoice, 3),
            requirement("a", LearningItemFormat::MultiSelect, 1),
            requirement("a", LearningItemFormat::ShortAnswer, 1),
            requirement("a", LearningItemFormat::Cloze, 1),
        ];

        let steps = bank_steps(&requirements, &[1, 0, 0, 0]);

        assert_eq!(
            steps,
            [
                (0, 2, FirstForm),
                (1, 1, FirstForm),
                (2, 1, FirstForm),
                (3, 1, FirstForm),
                (0, 6, Retakes),
                (1, 2, Retakes),
                (2, 2, Retakes),
                (3, 2, Retakes),
            ]
        );
    }

    #[test]
    fn a_later_need_is_given_the_questions_written_before_it_for_its_outcome() {
        let mut writing = BankWriting::new(vec![
            need("a", LearningItemFormat::SingleChoice, 1, BankPass::Retakes),
            need("a", LearningItemFormat::Cloze, 1, BankPass::Retakes),
            need("b", LearningItemFormat::Cloze, 1, BankPass::Retakes),
        ]);

        let first = writing.next().unwrap();
        writing.wrote(&first, &authored("a", "Where does a round parcel go?", 1));
        let second = writing.next().unwrap();
        let third = writing.next().unwrap();

        assert_eq!(
            second.existing_prompts.last().map(String::as_str),
            Some("Where does a round parcel go?")
        );
        assert!(!third
            .existing_prompts
            .contains(&"Where does a round parcel go?".to_owned()));
    }

    #[test]
    fn a_requirement_whose_first_form_came_back_short_is_not_asked_again_for_retakes() {
        use BankPass::{FirstForm, Retakes};
        let single = LearningItemFormat::SingleChoice;
        let cloze = LearningItemFormat::Cloze;
        let mut writing = BankWriting::new(vec![
            need("a", single, 2, FirstForm),
            need("a", cloze, 1, FirstForm),
            need("a", single, 2, Retakes),
            need("a", cloze, 1, Retakes),
        ]);

        let first = writing.next().unwrap();
        writing.wrote(&first, &authored("a", "Where does a tall parcel go?", 2));
        let second = writing.next().unwrap();
        writing.wrote(&second, &authored("a", "unused", 0));

        let mut rest = Vec::new();
        while let Some(need) = writing.next() {
            rest.push((need.requirement.format, need.pass));
        }
        assert_eq!(rest, [(single, Retakes)]);
    }

    #[test]
    fn saturated_block_is_filtered_when_at_least_two_other_blocks_remain() {
        use crate::assessment_generation::MaterialPassage;
        let passage = |title: &str, text: &str| MaterialPassage {
            lesson_title: "Lesson".into(),
            title: title.into(),
            text: text.into(),
            source_version_ids: vec![],
        };
        let mut first_need = need(
            "a",
            LearningItemFormat::SingleChoice,
            2,
            BankPass::FirstForm,
        );
        first_need.material = vec![
            passage(
                "Block 0",
                "Alpha sentence explains capability descriptors in detail for tenants.",
            ),
            passage(
                "Block 1",
                "Beta sentence describes OAuth2 credential verification steps.",
            ),
            passage(
                "Block 2",
                "Gamma sentence covers Direct Secure Messaging trust certificates.",
            ),
        ];
        let second_need = AuthoringNeed {
            requirement: requirement("a", LearningItemFormat::Ordering, 1),
            ..first_need.clone()
        };
        let mut writing = BankWriting::new(vec![first_need, second_need]);
        let current = writing.next().unwrap();
        let make_item = |id: &str| AuthoredItem {
            candidate: StoredItem {
                id: id.into(),
                outcome_ids: vec!["a".into()],
                format: LearningItemFormat::SingleChoice,
                difficulty: 2,
                prompt: format!("Prompt {id}"),
                options: vec![],
                left: vec![],
                source_version_ids: vec![],
                rubric: vec![],
                key: LearningAssessmentKey::Choice(0),
                explanation: String::new(),
                distractor_rationales: vec![],
            },
            source_quote: "Alpha sentence explains capability descriptors in detail for tenants."
                .into(),
            lesson_id: None,
        };
        writing.wrote(
            &current,
            &Authored {
                items: vec![make_item("i1"), make_item("i2")],
                dropped: vec![],
            },
        );
        let next_need = writing.next().unwrap();
        let titles: Vec<&str> = next_need
            .material
            .iter()
            .map(|m| m.title.as_str())
            .collect();
        assert_eq!(titles, ["Block 1", "Block 2"]);
    }
}
