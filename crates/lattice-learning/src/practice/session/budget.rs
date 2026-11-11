//! The time a session gets, for every kind of work.

use lattice_core::domain::learning::practice::work::Setting;

#[cfg(test)]
mod tests;

/// The budget a new session starts with: Coached has none, and the other settings take the learner's choice, else the
/// kind's default. None is untimed.
pub fn at_start(setting: Setting, chosen: Option<u32>, default: Option<u32>) -> Option<u32> {
    match setting {
        Setting::Coached => None,
        Setting::Mock | Setting::Watched => chosen.or(default),
    }
}

/// The budget after a change of setting: Coached keeps what the session has, and another setting gives an untimed
/// session the kind's default.
pub fn after_change(setting: Setting, current: Option<u32>, default: Option<u32>) -> Option<u32> {
    match (setting, current) {
        (Setting::Coached, current) => current,
        (_, None) => default,
        (_, current) => current,
    }
}
