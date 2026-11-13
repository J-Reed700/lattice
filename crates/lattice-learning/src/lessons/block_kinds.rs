//! What each kind of lesson section is held to, and the kinds' names.
use crate::dto::LearningBlockKind;

impl LearningBlockKind {
    /// Every kind, in the lesson schema's order.
    pub(crate) const ALL: [Self; 8] = [
        Self::Explanation,
        Self::WorkedExample,
        Self::Contrast,
        Self::Check,
        Self::GuidedPractice,
        Self::IndependentPractice,
        Self::Reflection,
        Self::Recap,
    ];

    /// The kind's serialized name.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Explanation => "explanation",
            Self::WorkedExample => "worked_example",
            Self::Contrast => "contrast",
            Self::Check => "check",
            Self::GuidedPractice => "guided_practice",
            Self::IndependentPractice => "independent_practice",
            Self::Reflection => "reflection",
            Self::Recap => "recap",
        }
    }

    /// The kind named `name`.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// Explanation, WorkedExample: sections that teaching added for an idea, a term, an action or a later use goes into; a contrast holds only the confusion it sets beside an idea.
    pub(crate) fn takes_teaching(&self) -> bool {
        matches!(self, Self::Explanation | Self::WorkedExample)
    }

    /// Explanation, Contrast, Recap: sections whose sentences cite sources.
    pub(crate) fn cites(&self) -> bool {
        matches!(self, Self::Explanation | Self::Contrast | Self::Recap)
    }

    /// WorkedExample, Contrast, Check: sections whose code the lesson runs.
    pub(crate) fn runs_examples(&self) -> bool {
        matches!(self, Self::WorkedExample | Self::Contrast | Self::Check)
    }

    /// GuidedPractice, IndependentPractice: sections graded by criteria.
    pub(crate) fn sets_practice(&self) -> bool {
        matches!(self, Self::GuidedPractice | Self::IndependentPractice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_the_serialized_names() {
        for kind in LearningBlockKind::ALL {
            assert_eq!(
                serde_json::to_string(&kind).ok(),
                Some(format!("\"{}\"", kind.as_str()))
            );
            assert_eq!(LearningBlockKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(LearningBlockKind::parse("summary"), None);
    }
}
