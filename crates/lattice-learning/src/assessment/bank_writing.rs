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
    /// Requirements whose first form came back short.
    short: Vec<LearningBlueprintRequirement>,
}

impl BankWriting {
    pub(crate) fn new(needs: Vec<AuthoringNeed>) -> Self {
        Self {
            queue: needs.into(),
            written: Vec::new(),
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
            return Some(need);
        }
    }

    /// Record what writing `need` produced. A `FirstForm` need comes back short when it kept fewer items than its count.
    pub(crate) fn wrote(&mut self, need: &AuthoringNeed, authored: &Authored) {
        if need.pass == BankPass::FirstForm && authored.items.len() < need.count {
            self.short.push(need.requirement.clone());
        }
        self.written.extend(authored.items.iter().map(|item| {
            (
                item.candidate.outcome_ids.clone(),
                item.candidate.prompt.clone(),
            )
        }));
    }
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
}
