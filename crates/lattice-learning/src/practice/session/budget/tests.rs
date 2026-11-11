use super::*;

#[test]
fn coached_has_no_budget_whatever_the_learner_chose() {
    assert_eq!(at_start(Setting::Coached, Some(900), Some(600)), None);
    assert_eq!(at_start(Setting::Coached, None, Some(600)), None);
}

#[test]
fn a_timed_setting_takes_the_learners_choice_before_the_kinds_default() {
    for setting in [Setting::Mock, Setting::Watched] {
        assert_eq!(
            at_start(setting, Some(900), Some(600)),
            Some(900),
            "{setting:?}"
        );
        assert_eq!(at_start(setting, None, Some(600)), Some(600), "{setting:?}");
    }
}

#[test]
fn a_timed_setting_with_no_choice_and_no_default_stays_untimed() {
    assert_eq!(at_start(Setting::Mock, None, None), None);
}

#[test]
fn leaving_coached_gives_an_untimed_session_the_default_and_keeps_a_time_it_has() {
    assert_eq!(after_change(Setting::Mock, None, Some(600)), Some(600));
    assert_eq!(
        after_change(Setting::Watched, Some(900), Some(600)),
        Some(900)
    );
    assert_eq!(after_change(Setting::Mock, None, None), None);
}

#[test]
fn moving_to_coached_keeps_what_the_session_has() {
    assert_eq!(
        after_change(Setting::Coached, Some(900), Some(600)),
        Some(900)
    );
    assert_eq!(after_change(Setting::Coached, None, Some(600)), None);
}
