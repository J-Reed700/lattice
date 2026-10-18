use super::*;

const DAY: i64 = 86_400;
const READ: i64 = 1_790_000_000;

fn dates(published: Option<i64>, modified: Option<i64>) -> SourceDates {
    SourceDates {
        published,
        modified,
        archived_at: None,
        http_last_modified: None,
        acquired_at: READ,
        basis: vec![],
    }
}

#[test]
fn the_effective_date_is_the_latest_stated_date_not_after_reading() {
    let effective = effective_date(&dates(Some(READ - 10 * DAY), Some(READ - DAY)));
    assert_eq!(effective.at, Some(READ - DAY));
    let future = effective_date(&dates(Some(READ - 10 * DAY), Some(READ + DAY)));
    assert_eq!(future.at, Some(READ - 10 * DAY));
    assert_eq!(future.reasons[0].code, ReasonCode::FutureDateIgnored);
}

#[test]
fn weak_dates_are_used_only_without_stated_ones_and_say_so() {
    let mut weak = dates(None, None);
    weak.http_last_modified = Some(READ - 5 * DAY);
    weak.archived_at = Some(READ - 9 * DAY);
    let effective = effective_date(&weak);
    assert_eq!(effective.at, Some(READ - 5 * DAY));
    assert_eq!(effective.reasons[0].code, ReasonCode::WeakDateBasis);
    weak.http_last_modified = None;
    assert_eq!(effective_date(&weak).at, Some(READ - 9 * DAY));
    assert_eq!(effective_date(&dates(None, None)).at, None);
}

#[test]
fn timeless_claims_never_need_a_date() {
    assert_eq!(
        validity(&TemporalScope::Timeless, &dates(None, None), false, None),
        TemporalValidity::Valid
    );
}

#[test]
fn as_of_claims_need_a_source_dated_on_or_after_the_date() {
    let scope = TemporalScope::AsOf {
        date: READ - 100 * DAY,
    };
    assert_eq!(
        validity(&scope, &dates(Some(READ - 50 * DAY), None), false, None),
        TemporalValidity::Valid
    );
    assert!(matches!(
        validity(&scope, &dates(Some(READ - 200 * DAY), None), false, None),
        TemporalValidity::Invalid { .. }
    ));
    assert!(matches!(
        validity(&scope, &dates(None, None), false, None),
        TemporalValidity::Undated { .. }
    ));
}

#[test]
fn since_claims_are_placed_by_the_version_or_by_a_date_after_the_newest_primary() {
    let scope = TemporalScope::Since {
        version: Some("7.2".into()),
        date: None,
    };
    assert_eq!(
        validity(&scope, &dates(None, None), true, None),
        TemporalValidity::Valid
    );
    assert!(matches!(
        validity(&scope, &dates(None, None), false, None),
        TemporalValidity::CannotPlace { .. }
    ));
    let primary = Some(("docs-v1", READ - 30 * DAY));
    assert_eq!(
        validity(&scope, &dates(Some(READ - 10 * DAY), None), false, primary),
        TemporalValidity::Valid
    );
    assert!(matches!(
        validity(&scope, &dates(Some(READ - 90 * DAY), None), false, primary),
        TemporalValidity::CannotPlace { .. }
    ));
    let dated = TemporalScope::Since {
        version: None,
        date: Some(READ - 30 * DAY),
    };
    assert!(matches!(
        validity(&dated, &dates(Some(READ - 90 * DAY), None), false, None),
        TemporalValidity::Invalid { .. }
    ));
}

#[test]
fn current_claims_are_stale_against_a_newer_primary_and_valid_without_one() {
    let primary = Some(("python-org-v2", READ - 10 * DAY));
    assert_eq!(
        validity(
            &TemporalScope::Current,
            &dates(Some(READ - 400 * DAY), None),
            false,
            primary
        ),
        TemporalValidity::Stale {
            newer_primary: "python-org-v2".into()
        }
    );
    assert_eq!(
        validity(
            &TemporalScope::Current,
            &dates(Some(READ - 10 * DAY), None),
            false,
            primary
        ),
        TemporalValidity::Valid
    );
    assert_eq!(
        validity(
            &TemporalScope::Current,
            &dates(Some(READ - 400 * DAY), None),
            false,
            None
        ),
        TemporalValidity::Valid
    );
    assert!(matches!(
        validity(&TemporalScope::Current, &dates(None, None), false, primary),
        TemporalValidity::Undated { .. }
    ));
}
