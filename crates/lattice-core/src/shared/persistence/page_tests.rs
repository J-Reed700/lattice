use super::Page;
use crate::shared::error::AppError;

#[test]
fn no_limit_is_every_row() {
    let page = Page::new(None, None).unwrap();
    assert_eq!(page.limit(), None);
    assert_eq!(page.sql_limit(), -1);
    assert_eq!(page.offset(), 0);
}

#[test]
fn a_limit_above_any_old_cap_is_kept() {
    let page = Page::new(Some(100_000), Some(5)).unwrap();
    assert_eq!(page.sql_limit(), 100_000);
    assert_eq!(page.offset(), 5);
}

#[test]
fn a_limit_below_one_or_a_negative_offset_is_refused() {
    assert!(matches!(
        Page::new(Some(0), None),
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        Page::new(None, Some(-1)),
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(Page::first(0), Err(AppError::InvalidInput(_))));
}

#[test]
fn a_count_too_large_for_sqlite_still_means_that_many() {
    assert_eq!(Page::first(usize::MAX).unwrap().sql_limit(), i64::MAX);
}
