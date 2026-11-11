//! The time a session gets, for every kind of work.

use lattice_core::domain::learning::practice::work::Setting;

#[cfg(test)]
mod tests;

/// The budget a new session starts with: Coached has none, and the other settings take the learner's choice, else the
/// kind's default. None is untimed.
pub fn at_start(_setting: Setting, _chosen: Option<u32>, _default: Option<u32>) -> Option<u32> {
    None
}

/// The budget after a change of setting: Coached keeps what the session has, and another setting gives an untimed
/// session the kind's default.
pub fn after_change(
    _setting: Setting,
    _current: Option<u32>,
    _default: Option<u32>,
) -> Option<u32> {
    None
}
