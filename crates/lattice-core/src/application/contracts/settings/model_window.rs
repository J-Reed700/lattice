//! How much a connected model takes at once. Its window is what was chosen for
//! it, else the app's default, because about forty paths size themselves to
//! the window and most have only run at that default. Its output ceiling is
//! what was chosen, else what its provider reports: a ceiling costs nothing
//! until it is used. Neither ever runs past what the provider reports.

use serde::{Deserialize, Serialize};

/// A model's context window and output ceiling, in tokens. As a choice, an
/// empty field follows the provider; as a report, it is a limit the provider
/// does not state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelWindowDto {
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl ModelWindowDto {
    /// The context window this choice runs at.
    pub fn context_window_within(&self, reported: &Self, default: u32) -> u32 {
        within(
            self.context_window.unwrap_or(default),
            reported.context_window,
        )
    }

    /// The output ceiling this choice runs at.
    pub fn max_output_within(&self, reported: &Self, fallback: u32) -> u32 {
        within(
            self.max_output_tokens
                .or(reported.max_output_tokens)
                .unwrap_or(fallback),
            reported.max_output_tokens,
        )
    }

    /// What is wrong with this choice, as a sentence for the settings check.
    pub fn problem(&self) -> Option<&'static str> {
        (self.context_window == Some(0) || self.max_output_tokens == Some(0))
            .then_some("a window and an output ceiling must be greater than 0")
    }
}

fn within(wanted: u32, reported: Option<u32>) -> u32 {
    reported.map_or(wanted, |limit| wanted.min(limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORTED: ModelWindowDto = ModelWindowDto {
        context_window: Some(1_000_000),
        max_output_tokens: Some(128_000),
    };
    const NOT_REPORTED: ModelWindowDto = ModelWindowDto {
        context_window: None,
        max_output_tokens: None,
    };

    #[test]
    fn with_no_choice_a_window_stays_at_the_default_and_output_follows_the_provider() {
        let chosen = ModelWindowDto::default();
        assert_eq!(chosen.context_window_within(&REPORTED, 131_072), 131_072);
        assert_eq!(chosen.max_output_within(&REPORTED, 8_192), 128_000);
    }

    #[test]
    fn a_default_past_what_a_smaller_model_reports_is_held_to_it() {
        let small = ModelWindowDto {
            context_window: Some(64_000),
            max_output_tokens: Some(8_192),
        };
        let chosen = ModelWindowDto::default();
        assert_eq!(chosen.context_window_within(&small, 131_072), 64_000);
    }

    #[test]
    fn with_no_report_a_choice_stands_and_the_rest_falls_back() {
        let chosen = ModelWindowDto {
            context_window: Some(40_000),
            max_output_tokens: None,
        };
        assert_eq!(chosen.context_window_within(&NOT_REPORTED, 131_072), 40_000);
        assert_eq!(chosen.max_output_within(&NOT_REPORTED, 8_192), 8_192);
    }

    #[test]
    fn a_choice_runs_as_set_up_to_what_is_reported_and_no_further() {
        let chosen = ModelWindowDto {
            context_window: Some(200_000),
            max_output_tokens: Some(500_000),
        };
        assert_eq!(chosen.context_window_within(&REPORTED, 131_072), 200_000);
        assert_eq!(chosen.max_output_within(&REPORTED, 131_072), 128_000);
    }

    #[test]
    fn a_window_of_nothing_is_a_problem() {
        assert!(ModelWindowDto::default().problem().is_none());
        let empty = ModelWindowDto {
            context_window: Some(0),
            max_output_tokens: None,
        };
        assert!(empty.problem().is_some());
    }
}
